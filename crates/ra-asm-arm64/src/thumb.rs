//! The Thumb-2 decoder of 32-bit ARM: the 16-bit and 32-bit encodings become one [`Decoded`]
//! record the lift reads, and the neutral [`Instr`] the analysis reads.
//!
//! An instruction inside an IT block is predicated, and the 16-bit forms that set the flags outside
//! one do not inside it: [`Thumb::link`] finds the IT instruction before it in the same block and
//! decodes it again with its condition, from the raw encoding the record keeps as its code. A
//! 32-bit encoding's code is its first halfword above its second, a 16-bit one's the halfword
//! alone, so the two never meet. The PC reads as the instruction's address plus four, and a
//! literal load or an `adr` as that word-aligned; a branch target carries no Thumb bit, since
//! every function here is Thumb code. The ARM instruction set itself is not decoded: a `blx` to
//! ARM code decodes, and the lift stops at it. An encoding outside the subset decodes as
//! [`Op::Unsupported`] with its length, so the analysis goes on and the lift traps at it.

use ra_asm_arch::{Access, Arch, Flow, Instr, MemOperand, Operand, Prefixes, RegClass, Register, Target};

pub const SP: u8 = 13;
pub const LR: u8 = 14;
pub const PC: u8 = 15;
/// The id of the flags register, after `pc`.
pub const APSR: u16 = 16;

const NAMES: [&str; 16] = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc"];

/// The condition names, in encoding order.
const CONDITIONS: [&str; 16] = ["eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", "nv"];

/// Core register `n` (0..=15).
pub fn register(n: u8) -> Register {
    let class = match n {
        SP => RegClass::StackPointer,
        PC => RegClass::InstructionPointer,
        _ => RegClass::General,
    };
    Register { id: u16::from(n & 15), name: NAMES[usize::from(n & 15)], class, full: u16::from(n & 15), bits: 32, shift: 0 }
}

pub fn apsr() -> Register {
    Register { id: APSR, name: "apsr", class: RegClass::Flags, full: APSR, bits: 32, shift: 0 }
}

/// The Thumb-2 decoder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Thumb;

/// A shift of a register operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Lsl,
    Lsr,
    Asr,
    Ror,
    /// A rotate right by one through the carry.
    Rrx,
}

/// The second source of a data-processing instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A constant, with the carry its expansion gives when it rotates.
    Imm { value: u32, carry: Option<bool> },
    /// A register shifted by a constant: 1 to 32 for the right shifts, 0 to 31 for the others.
    Reg { rm: u8, shift: Shift, amount: u8 },
    /// A register shifted by the low byte of another.
    RegReg { rm: u8, shift: Shift, rs: u8 },
}

/// A data-processing operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alu {
    And,
    Eor,
    Sub,
    Rsb,
    Add,
    Adc,
    Sbc,
    Orr,
    Orn,
    Bic,
    Mov,
    Mvn,
    Tst,
    Teq,
    Cmp,
    Cmn,
}

impl Alu {
    pub fn name(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Eor => "eor",
            Self::Sub => "sub",
            Self::Rsb => "rsb",
            Self::Add => "add",
            Self::Adc => "adc",
            Self::Sbc => "sbc",
            Self::Orr => "orr",
            Self::Orn => "orn",
            Self::Bic => "bic",
            Self::Mov => "mov",
            Self::Mvn => "mvn",
            Self::Tst => "tst",
            Self::Teq => "teq",
            Self::Cmp => "cmp",
            Self::Cmn => "cmn",
        }
    }

    /// Whether the operation writes the flags alone.
    pub fn compares(self) -> bool {
        matches!(self, Self::Tst | Self::Teq | Self::Cmp | Self::Cmn)
    }

    /// Whether the flags come from the adder (C and V) rather than the shifter.
    pub fn arithmetic(self) -> bool {
        matches!(self, Self::Sub | Self::Rsb | Self::Add | Self::Adc | Self::Sbc | Self::Cmp | Self::Cmn)
    }

    /// Whether the operation reads no first register.
    pub fn unary(self) -> bool {
        matches!(self, Self::Mov | Self::Mvn)
    }
}

/// The offset of a single load or store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    /// A constant, negative when the encoding subtracts it.
    Imm(i32),
    /// A register shifted left.
    Reg { rm: u8, shift: u8 },
}

/// The operation of an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Alu {
        op: Alu,
        s: bool,
        rd: u8,
        rn: u8,
        src: Source,
    },
    /// `movw` (the low half, the high half cleared) or `movt` (the high half, the low half kept).
    MovWide {
        rd: u8,
        imm: u16,
        top: bool,
    },
    /// `mul`, `mla` (`acc`) and `mls` (`acc` and `subtract`).
    Mul {
        s: bool,
        rd: u8,
        rn: u8,
        rm: u8,
        acc: Option<u8>,
        subtract: bool,
    },
    /// `umull`, `smull`, `umlal` and `smlal`.
    LongMul {
        signed: bool,
        accumulate: bool,
        lo: u8,
        hi: u8,
        rn: u8,
        rm: u8,
    },
    Div {
        signed: bool,
        rd: u8,
        rn: u8,
        rm: u8,
    },
    /// `uxtb`, `sxtb`, `uxth` and `sxth`, the source rotated right first.
    Extend {
        signed: bool,
        bits: u8,
        rd: u8,
        rm: u8,
        rotate: u8,
    },
    /// `ubfx` and `sbfx`.
    Field {
        signed: bool,
        rd: u8,
        rn: u8,
        lsb: u8,
        width: u8,
    },
    /// `bfi`, or `bfc` with no source.
    Insert {
        rd: u8,
        rn: Option<u8>,
        lsb: u8,
        width: u8,
    },
    /// A single load or store; `index` adds the offset before the access, `wback` writes the
    /// offset address back to `rn`. A base of `pc` is the word-aligned PC.
    Mem {
        load: bool,
        bits: u8,
        signed: bool,
        rt: u8,
        rn: u8,
        offset: Offset,
        index: bool,
        wback: bool,
    },
    /// `ldrd` and `strd`.
    Dual {
        load: bool,
        rt: u8,
        rt2: u8,
        rn: u8,
        offset: i32,
        index: bool,
        wback: bool,
    },
    /// `ldm`, `stm`, `push` and `pop`: increment after, or decrement `before`.
    Multiple {
        load: bool,
        rn: u8,
        list: u16,
        wback: bool,
        before: bool,
    },
    /// `b`, conditional by its own encoding or by an IT block.
    Branch {
        cond: Option<u8>,
        target: u64,
    },
    /// `bl`, or `blx` to ARM code when `arm`.
    Call {
        target: u64,
        arm: bool,
    },
    /// `bx` and `blx` through a register.
    BranchReg {
        rm: u8,
        link: bool,
    },
    /// `cbz` and `cbnz`.
    CompareBranch {
        nonzero: bool,
        rn: u8,
        target: u64,
    },
    It {
        first: u8,
        mask: u8,
    },
    Adr {
        rd: u8,
        target: u64,
    },
    Svc(u8),
    /// `nop` and the other hints.
    Nop,
    /// `udf` and `bkpt`.
    Trap,
    Unsupported,
}

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub op: Op,
    pub len: u8,
    /// The encoding: a 32-bit one's first halfword above its second.
    pub raw: u32,
    /// The condition an IT block puts on the instruction.
    pub cond: Option<u8>,
}

fn field(v: u32, hi: u32, lo: u32) -> u32 {
    (v >> lo) & ((1 << (hi - lo + 1)) - 1)
}

fn bit(v: u32, n: u32) -> bool {
    v >> n & 1 == 1
}

fn reg(v: u32, lo: u32) -> u8 {
    field(v, lo + 3, lo) as u8
}

fn low(v: u32, lo: u32) -> u8 {
    field(v, lo + 2, lo) as u8
}

fn sign_extend(v: u32, width: u32) -> i64 {
    let shift = 64 - width;
    ((u64::from(v) << shift) as i64) >> shift
}

fn imm(value: u32) -> Source {
    Source::Imm { value, carry: None }
}

fn plain(rm: u8) -> Source {
    Source::Reg { rm, shift: Shift::Lsl, amount: 0 }
}

/// The value the PC reads as for an instruction at `address`, word-aligned for a literal.
pub fn aligned_pc(address: u64) -> u64 {
    (address + 4) & !3
}

/// The number of instructions an IT block of `mask` predicates.
pub fn it_len(mask: u8) -> usize {
    4 - (mask & 15).trailing_zeros().min(4) as usize
}

/// The condition of the instruction at `position` (from 0) of an IT block.
pub fn it_condition_at(first: u8, mask: u8, position: usize) -> u8 {
    if position == 0 { first } else { (first & 0xe) | ((mask >> (4 - position)) & 1) }
}

/// The condition the IT block among `before` (the instructions before this one in its block) puts
/// on the next instruction, if it covers it.
pub fn it_condition(before: &[Instr]) -> Option<u8> {
    for (position, i) in before.iter().rev().take(4).enumerate() {
        if i.mnemonic.starts_with("it") {
            let (Some(Operand::Imm { value: first, .. }), Some(Operand::Imm { value: mask, .. })) = (i.operands.first(), i.operands.get(1))
            else {
                return None;
            };
            let (first, mask) = (*first as u8, *mask as u8);
            return (position < it_len(mask)).then(|| it_condition_at(first, mask, position));
        }
    }
    None
}

/// Decodes one instruction at `address`; `it` is the condition an IT block puts on it.
pub fn decode(bytes: &[u8], address: u64, it: Option<u8>) -> Option<Decoded> {
    let h1 = u32::from(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]));
    if h1 >> 11 >= 0b11101 {
        let h2 = u32::from(u16::from_le_bytes([*bytes.get(2)?, *bytes.get(3)?]));
        Some(Decoded { op: decode32(h1, h2, address), len: 4, raw: h1 << 16 | h2, cond: it })
    } else {
        Some(Decoded { op: decode16(h1, address, it.is_some()), len: 2, raw: h1, cond: it })
    }
}

/// Decodes the instruction whose encoding a record keeps as its code.
pub fn decode_raw(raw: u32, address: u64, it: Option<u8>) -> Option<Decoded> {
    if raw > 0xffff {
        let [a, b, c, d] = ((raw >> 16) | (raw << 16)).to_le_bytes();
        decode(&[a, b, c, d], address, it)
    } else {
        decode(&(raw as u16).to_le_bytes(), address, it)
    }
}

fn decode16(h: u32, address: u64, in_it: bool) -> Op {
    let s = !in_it;
    let pc = address + 4;
    match h >> 11 {
        0b00000..=0b00010 => {
            let imm5 = field(h, 10, 6) as u8;
            let shift = [Shift::Lsl, Shift::Lsr, Shift::Asr][(h >> 11) as usize];
            let amount = if imm5 == 0 && shift != Shift::Lsl { 32 } else { imm5 };
            Op::Alu { op: Alu::Mov, s, rd: low(h, 0), rn: 0, src: Source::Reg { rm: low(h, 3), shift, amount } }
        }
        0b00011 => {
            let op = if bit(h, 9) { Alu::Sub } else { Alu::Add };
            let src = if bit(h, 10) { imm(field(h, 8, 6)) } else { plain(low(h, 6)) };
            Op::Alu { op, s, rd: low(h, 0), rn: low(h, 3), src }
        }
        0b00100 => Op::Alu { op: Alu::Mov, s, rd: low(h, 8), rn: 0, src: imm(h & 0xff) },
        0b00101 => Op::Alu { op: Alu::Cmp, s: true, rd: 0, rn: low(h, 8), src: imm(h & 0xff) },
        0b00110 => Op::Alu { op: Alu::Add, s, rd: low(h, 8), rn: low(h, 8), src: imm(h & 0xff) },
        0b00111 => Op::Alu { op: Alu::Sub, s, rd: low(h, 8), rn: low(h, 8), src: imm(h & 0xff) },
        0b01000 if !bit(h, 10) => data16(h, s),
        0b01000 => {
            let rdn = (field(h, 7, 7) << 3 | field(h, 2, 0)) as u8;
            let rm = reg(h, 3);
            match field(h, 9, 8) {
                0 if rdn != PC => Op::Alu { op: Alu::Add, s: false, rd: rdn, rn: rdn, src: plain(rm) },
                1 => Op::Alu { op: Alu::Cmp, s: true, rd: 0, rn: rdn, src: plain(rm) },
                2 if rdn != PC => Op::Alu { op: Alu::Mov, s: false, rd: rdn, rn: 0, src: plain(rm) },
                3 => Op::BranchReg { rm, link: bit(h, 7) },
                _ => Op::Unsupported,
            }
        }
        0b01001 => mem(true, 32, false, low(h, 8), PC, Offset::Imm((h & 0xff) as i32 * 4)),
        0b01010 | 0b01011 => {
            let (load, bits, signed) = [
                (false, 32, false),
                (false, 16, false),
                (false, 8, false),
                (true, 8, true),
                (true, 32, false),
                (true, 16, false),
                (true, 8, false),
                (true, 16, true),
            ][field(h, 11, 9) as usize];
            mem(load, bits, signed, low(h, 0), low(h, 3), Offset::Reg { rm: low(h, 6), shift: 0 })
        }
        0b01100..=0b01111 => {
            let byte = bit(h, 12);
            let scale = if byte { 1 } else { 4 };
            mem(bit(h, 11), if byte { 8 } else { 32 }, false, low(h, 0), low(h, 3), Offset::Imm(field(h, 10, 6) as i32 * scale))
        }
        0b10000 | 0b10001 => mem(bit(h, 11), 16, false, low(h, 0), low(h, 3), Offset::Imm(field(h, 10, 6) as i32 * 2)),
        0b10010 | 0b10011 => mem(bit(h, 11), 32, false, low(h, 8), SP, Offset::Imm((h & 0xff) as i32 * 4)),
        0b10100 => Op::Adr { rd: low(h, 8), target: aligned_pc(address) + u64::from((h & 0xff) * 4) },
        0b10101 => Op::Alu { op: Alu::Add, s: false, rd: low(h, 8), rn: SP, src: imm((h & 0xff) * 4) },
        0b10110 | 0b10111 => misc16(h, pc),
        0b11000 | 0b11001 => {
            let (load, rn, list) = (bit(h, 11), low(h, 8), (h & 0xff) as u16);
            Op::Multiple { load, rn, list, wback: !load || list & (1 << rn) == 0, before: false }
        }
        0b11010 | 0b11011 => match field(h, 11, 8) as u8 {
            0b1110 => Op::Trap,
            0b1111 => Op::Svc((h & 0xff) as u8),
            cond => Op::Branch { cond: Some(cond), target: pc.wrapping_add(sign_extend((h & 0xff) << 1, 9) as u64) },
        },
        0b11100 => Op::Branch { cond: None, target: pc.wrapping_add(sign_extend((h & 0x7ff) << 1, 12) as u64) },
        _ => Op::Unsupported,
    }
}

fn mem(load: bool, bits: u8, signed: bool, rt: u8, rn: u8, offset: Offset) -> Op {
    Op::Mem { load, bits, signed, rt, rn, offset, index: true, wback: false }
}

/// The 16-bit data-processing group: two low registers.
fn data16(h: u32, s: bool) -> Op {
    let (rm, rdn) = (low(h, 3), low(h, 0));
    let alu = |op: Alu| Op::Alu { op, s, rd: rdn, rn: rdn, src: plain(rm) };
    let compare = |op: Alu| Op::Alu { op, s: true, rd: 0, rn: rdn, src: plain(rm) };
    let shift = |shift: Shift| Op::Alu { op: Alu::Mov, s, rd: rdn, rn: 0, src: Source::RegReg { rm: rdn, shift, rs: rm } };
    match field(h, 9, 6) {
        0 => alu(Alu::And),
        1 => alu(Alu::Eor),
        2 => shift(Shift::Lsl),
        3 => shift(Shift::Lsr),
        4 => shift(Shift::Asr),
        5 => alu(Alu::Adc),
        6 => alu(Alu::Sbc),
        7 => shift(Shift::Ror),
        8 => compare(Alu::Tst),
        9 => Op::Alu { op: Alu::Rsb, s, rd: rdn, rn: rm, src: imm(0) },
        10 => compare(Alu::Cmp),
        11 => compare(Alu::Cmn),
        12 => alu(Alu::Orr),
        13 => Op::Mul { s, rd: rdn, rn: rm, rm: rdn, acc: None, subtract: false },
        14 => alu(Alu::Bic),
        _ => Op::Alu { op: Alu::Mvn, s, rd: rdn, rn: 0, src: plain(rm) },
    }
}

/// The 16-bit miscellaneous group: the stack, the compare-and-branch, the extends, push and pop,
/// the breakpoint, IT and the hints.
fn misc16(h: u32, pc: u64) -> Op {
    match field(h, 11, 8) {
        0b0000 => {
            let op = if bit(h, 7) { Alu::Sub } else { Alu::Add };
            Op::Alu { op, s: false, rd: SP, rn: SP, src: imm((h & 0x7f) * 4) }
        }
        0b0001 | 0b0011 | 0b1001 | 0b1011 => {
            let offset = field(h, 9, 9) << 6 | field(h, 7, 3) << 1;
            Op::CompareBranch { nonzero: bit(h, 11), rn: low(h, 0), target: pc + u64::from(offset) }
        }
        0b0010 => {
            let (signed, bits) = [(true, 16), (true, 8), (false, 16), (false, 8)][field(h, 7, 6) as usize];
            Op::Extend { signed, bits, rd: low(h, 0), rm: low(h, 3), rotate: 0 }
        }
        0b0100 | 0b0101 => {
            let list = (h & 0xff) as u16 | if bit(h, 8) { 1 << LR } else { 0 };
            Op::Multiple { load: false, rn: SP, list, wback: true, before: true }
        }
        0b1100 | 0b1101 => {
            let list = (h & 0xff) as u16 | if bit(h, 8) { 1 << PC } else { 0 };
            Op::Multiple { load: true, rn: SP, list, wback: true, before: false }
        }
        0b1110 => Op::Trap,
        0b1111 if h & 0xf == 0 => Op::Nop,
        0b1111 => Op::It { first: field(h, 7, 4) as u8, mask: (h & 0xf) as u8 },
        _ => Op::Unsupported,
    }
}

/// `ThumbExpandImm`: the constant of a modified immediate and the carry when it rotates.
fn expand_imm(imm12: u32) -> (u32, Option<bool>) {
    let imm8 = imm12 & 0xff;
    if imm12 >> 10 == 0 {
        let value = match (imm12 >> 8) & 3 {
            0 => imm8,
            1 => imm8 << 16 | imm8,
            2 => imm8 << 24 | imm8 << 8,
            _ => imm8 * 0x0101_0101,
        };
        (value, None)
    } else {
        let value = (0x80 | (imm12 & 0x7f)).rotate_right(imm12 >> 7);
        (value, Some(value >> 31 == 1))
    }
}

/// `DecodeImmShift`.
fn imm_shift(kind: u32, imm5: u32) -> (Shift, u8) {
    let imm5 = imm5 as u8;
    match kind {
        0 => (Shift::Lsl, imm5),
        1 => (Shift::Lsr, if imm5 == 0 { 32 } else { imm5 }),
        2 => (Shift::Asr, if imm5 == 0 { 32 } else { imm5 }),
        _ if imm5 == 0 => (Shift::Rrx, 1),
        _ => (Shift::Ror, imm5),
    }
}

/// The data-processing operations the modified-immediate and the shifted-register forms share.
fn alu32(op: u32, s: bool, rn: u8, rd: u8, src: Source) -> Op {
    let compare = rd == PC && s;
    let alu = match op {
        0b0000 if compare => Alu::Tst,
        0b0000 => Alu::And,
        0b0001 => Alu::Bic,
        0b0010 if rn == PC => Alu::Mov,
        0b0010 => Alu::Orr,
        0b0011 if rn == PC => Alu::Mvn,
        0b0011 => Alu::Orn,
        0b0100 if compare => Alu::Teq,
        0b0100 => Alu::Eor,
        0b1000 if compare => Alu::Cmn,
        0b1000 => Alu::Add,
        0b1010 => Alu::Adc,
        0b1011 => Alu::Sbc,
        0b1101 if compare => Alu::Cmp,
        0b1101 => Alu::Sub,
        0b1110 => Alu::Rsb,
        _ => return Op::Unsupported,
    };
    if rd == PC && !alu.compares() || rn == PC && !alu.unary() {
        return Op::Unsupported;
    }
    Op::Alu { op: alu, s, rd: if alu.compares() { 0 } else { rd }, rn: if alu.unary() { 0 } else { rn }, src }
}

fn decode32(h1: u32, h2: u32, address: u64) -> Op {
    let rn = reg(h1, 0);
    let rd = reg(h2, 8);
    let rm = reg(h2, 0);
    match field(h1, 12, 11) {
        0b01 => match field(h1, 10, 9) {
            0b00 if !bit(h1, 6) => {
                let (load, wback) = (bit(h1, 4), bit(h1, 5));
                let list = h2 as u16;
                match field(h1, 8, 7) {
                    0b01 | 0b10 if !(load && wback && list & (1 << rn) != 0) && (list & (1 << PC) == 0 || load && rn == SP) => {
                        Op::Multiple { load, rn, list, wback, before: field(h1, 8, 7) == 0b10 }
                    }
                    _ => Op::Unsupported,
                }
            }
            0b00 => {
                let (index, add, wback, load) = (bit(h1, 8), bit(h1, 7), bit(h1, 5), bit(h1, 4));
                if !index && !wback {
                    return Op::Unsupported;
                }
                let offset = (h2 & 0xff) as i32 * 4;
                Op::Dual { load, rt: reg(h2, 12), rt2: reg(h2, 8), rn, offset: if add { offset } else { -offset }, index, wback }
            }
            0b01 => {
                let (shift, amount) = imm_shift(field(h2, 5, 4), field(h2, 14, 12) << 2 | field(h2, 7, 6));
                alu32(field(h1, 8, 5), bit(h1, 4), rn, rd, Source::Reg { rm, shift, amount })
            }
            _ => Op::Unsupported,
        },
        0b10 if !bit(h2, 15) => {
            let imm12 = field(h1, 10, 10) << 11 | field(h2, 14, 12) << 8 | (h2 & 0xff);
            if !bit(h1, 9) {
                let (value, carry) = expand_imm(imm12);
                return alu32(field(h1, 8, 5), bit(h1, 4), rn, rd, Source::Imm { value, carry });
            }
            let lsb = (field(h2, 14, 12) << 2 | field(h2, 7, 6)) as u8;
            match field(h1, 8, 4) {
                0b00000 if rn == PC => Op::Adr { rd, target: aligned_pc(address) + u64::from(imm12) },
                0b00000 => Op::Alu { op: Alu::Add, s: false, rd, rn, src: imm(imm12) },
                0b01010 if rn == PC => Op::Adr { rd, target: aligned_pc(address) - u64::from(imm12) },
                0b01010 => Op::Alu { op: Alu::Sub, s: false, rd, rn, src: imm(imm12) },
                op @ (0b00100 | 0b01100) => Op::MovWide { rd, imm: (u32::from(rn) << 12 | imm12) as u16, top: op == 0b01100 },
                op @ (0b10100 | 0b11100) => {
                    let width = (h2 & 0x1f) as u8 + 1;
                    if lsb + width > 32 {
                        return Op::Unsupported;
                    }
                    Op::Field { signed: op == 0b10100, rd, rn, lsb, width }
                }
                0b10110 => {
                    let msb = (h2 & 0x1f) as u8;
                    if msb < lsb {
                        return Op::Unsupported;
                    }
                    Op::Insert { rd, rn: (rn != PC).then_some(rn), lsb, width: msb - lsb + 1 }
                }
                _ => Op::Unsupported,
            }
        }
        0b10 => branch32(h1, h2, address),
        0b11 => match field(h1, 10, 7) {
            0b0000..=0b0011 => mem32(h1, h2),
            0b0100 if field(h2, 15, 12) == 0xf => {
                let (op1, op2) = (field(h1, 7, 4), field(h2, 7, 4));
                if op1 & 0b1000 == 0 && op2 == 0 {
                    let shift = [Shift::Lsl, Shift::Lsr, Shift::Asr, Shift::Ror][field(op1, 2, 1) as usize];
                    return Op::Alu { op: Alu::Mov, s: bit(op1, 0), rd, rn: 0, src: Source::RegReg { rm: rn, shift, rs: rm } };
                }
                if op2 & 0b1000 != 0 && rn == PC {
                    let rotate = (field(h2, 5, 4) * 8) as u8;
                    let (signed, bits) = match op1 {
                        0b0000 => (true, 16),
                        0b0001 => (false, 16),
                        0b0100 => (true, 8),
                        0b0101 => (false, 8),
                        _ => return Op::Unsupported,
                    };
                    return Op::Extend { signed, bits, rd, rm, rotate };
                }
                Op::Unsupported
            }
            0b0110 => {
                let ra = reg(h2, 12);
                match (field(h1, 6, 4), field(h2, 5, 4)) {
                    (0, 0) => Op::Mul { s: false, rd, rn, rm, acc: (ra != PC).then_some(ra), subtract: false },
                    (0, 1) => Op::Mul { s: false, rd, rn, rm, acc: Some(ra), subtract: true },
                    _ => Op::Unsupported,
                }
            }
            0b0111 => {
                let (lo, hi) = (reg(h2, 12), rd);
                match (field(h1, 6, 4), field(h2, 7, 4)) {
                    (0b000, 0) => Op::LongMul { signed: true, accumulate: false, lo, hi, rn, rm },
                    (0b010, 0) => Op::LongMul { signed: false, accumulate: false, lo, hi, rn, rm },
                    (0b100, 0) => Op::LongMul { signed: true, accumulate: true, lo, hi, rn, rm },
                    (0b110, 0) => Op::LongMul { signed: false, accumulate: true, lo, hi, rn, rm },
                    (0b001, 0xf) => Op::Div { signed: true, rd, rn, rm },
                    (0b011, 0xf) => Op::Div { signed: false, rd, rn, rm },
                    _ => Op::Unsupported,
                }
            }
            _ => Op::Unsupported,
        },
        _ => Op::Unsupported,
    }
}

/// The 32-bit branches: `b` with and without a condition, `bl`, `blx` to ARM, and the hints and
/// `udf` that share the space.
fn branch32(h1: u32, h2: u32, address: u64) -> Op {
    let pc = address + 4;
    let s = field(h1, 10, 10);
    let (j1, j2) = (field(h2, 13, 13), field(h2, 11, 11));
    let long = || {
        let (i1, i2) = (!(j1 ^ s) & 1, !(j2 ^ s) & 1);
        sign_extend(s << 24 | i1 << 23 | i2 << 22 | field(h1, 9, 0) << 12 | field(h2, 10, 0) << 1, 25)
    };
    match (bit(h2, 14), bit(h2, 12)) {
        (false, false) => {
            let cond = field(h1, 9, 6) as u8;
            if cond < 0b1110 {
                let offset = sign_extend(s << 20 | j2 << 19 | j1 << 18 | field(h1, 5, 0) << 12 | field(h2, 10, 0) << 1, 21);
                return Op::Branch { cond: Some(cond), target: pc.wrapping_add(offset as u64) };
            }
            if h1 == 0xf3af && h2 & 0xff00 == 0x8000 {
                Op::Nop
            } else if h1 & 0xfff0 == 0xf7f0 && h2 & 0xf000 == 0xa000 {
                Op::Trap
            } else {
                Op::Unsupported
            }
        }
        (false, true) => Op::Branch { cond: None, target: pc.wrapping_add(long() as u64) },
        (true, true) => Op::Call { target: pc.wrapping_add(long() as u64), arm: false },
        (true, false) => Op::Call { target: (pc & !3).wrapping_add(long() as u64), arm: true },
    }
}

/// The 32-bit single loads and stores: a literal, a 12-bit offset, an 8-bit offset with its
/// index and write-back, or a register shifted left.
fn mem32(h1: u32, h2: u32) -> Op {
    let (load, signed, size) = (bit(h1, 4), bit(h1, 8), field(h1, 6, 5));
    let (rn, rt) = (reg(h1, 0), reg(h2, 12));
    if size == 3 || !load && signed || rt == PC {
        return Op::Unsupported;
    }
    let bits = 8 << size;
    let op = |offset: Offset, index: bool, wback: bool| Op::Mem { load, bits, signed, rt, rn, offset, index, wback };
    let imm12 = (h2 & 0xfff) as i32;
    if rn == PC {
        return if load { op(Offset::Imm(if bit(h1, 7) { imm12 } else { -imm12 }), true, false) } else { Op::Unsupported };
    }
    if bit(h1, 7) {
        return op(Offset::Imm(imm12), true, false);
    }
    if bit(h2, 11) {
        let (index, add, wback) = (bit(h2, 10), bit(h2, 9), bit(h2, 8));
        if !index && !wback || wback && rn == rt {
            return Op::Unsupported;
        }
        let imm8 = (h2 & 0xff) as i32;
        return op(Offset::Imm(if add { imm8 } else { -imm8 }), index, wback);
    }
    if field(h2, 10, 6) == 0 {
        return op(Offset::Reg { rm: reg(h2, 0), shift: field(h2, 5, 4) as u8 }, true, false);
    }
    Op::Unsupported
}

// region The neutral record

fn condition_suffix(cond: Option<u8>) -> &'static str {
    cond.map_or("", |c| CONDITIONS[usize::from(c & 15)])
}

fn mnemonic(op: &Op) -> String {
    match *op {
        Op::Alu { op, s, .. } => format!("{}{}", op.name(), if s && !op.compares() { "s" } else { "" }),
        Op::MovWide { top, .. } => (if top { "movt" } else { "movw" }).to_owned(),
        Op::Mul { s, acc, subtract, .. } => match (acc, subtract) {
            (None, _) => (if s { "muls" } else { "mul" }).to_owned(),
            (Some(_), false) => "mla".to_owned(),
            (Some(_), true) => "mls".to_owned(),
        },
        Op::LongMul { signed, accumulate, .. } => {
            format!("{}{}", if signed { "smul" } else { "umul" }, if accumulate { "al" } else { "l" }).replace("mulal", "mlal")
        }
        Op::Div { signed, .. } => (if signed { "sdiv" } else { "udiv" }).to_owned(),
        Op::Extend { signed, bits, .. } => format!("{}xt{}", if signed { "s" } else { "u" }, if bits == 8 { "b" } else { "h" }),
        Op::Field { signed, .. } => (if signed { "sbfx" } else { "ubfx" }).to_owned(),
        Op::Insert { rn, .. } => (if rn.is_some() { "bfi" } else { "bfc" }).to_owned(),
        Op::Mem { load, bits, signed, .. } => {
            format!(
                "{}{}{}",
                if load { "ldr" } else { "str" },
                if signed { "s" } else { "" },
                match bits {
                    8 => "b",
                    16 => "h",
                    _ => "",
                }
            )
        }
        Op::Dual { load, .. } => (if load { "ldrd" } else { "strd" }).to_owned(),
        Op::Multiple { load: true, rn: SP, wback: true, before: false, .. } => "pop".to_owned(),
        Op::Multiple { load: false, rn: SP, wback: true, before: true, .. } => "push".to_owned(),
        Op::Multiple { load, before, .. } => format!("{}{}", if load { "ldm" } else { "stm" }, if before { "db" } else { "ia" }),
        Op::Branch { cond, .. } => format!("b{}", condition_suffix(cond)),
        Op::Call { arm, .. } => (if arm { "blx" } else { "bl" }).to_owned(),
        Op::BranchReg { link, .. } => (if link { "blx" } else { "bx" }).to_owned(),
        Op::CompareBranch { nonzero, .. } => (if nonzero { "cbnz" } else { "cbz" }).to_owned(),
        Op::It { first, mask } => {
            let then = first & 1;
            let rest: String = (1..it_len(mask)).map(|p| if (mask >> (4 - p)) & 1 == then { 't' } else { 'e' }).collect();
            format!("it{rest}")
        }
        Op::Adr { .. } => "adr".to_owned(),
        Op::Svc(_) => "svc".to_owned(),
        Op::Nop => "nop".to_owned(),
        Op::Trap => "udf".to_owned(),
        Op::Unsupported => "unknown".to_owned(),
    }
}

fn shown(o: &Operand) -> String {
    match o {
        Operand::Reg(r) => r.name.to_owned(),
        Operand::Imm { value, .. } => format!("#{value:#x}"),
        Operand::Addr(a) => format!("{a:#x}"),
        Operand::Mem(m) => match (m.base, m.index, m.absolute) {
            (_, _, Some(a)) => format!("[{a:#x}]"),
            (Some(b), Some(i), _) => format!("[{}, {}]", b.name, i.name),
            (Some(b), None, _) => format!("[{}, #{}]", b.name, m.disp),
            _ => "[?]".to_owned(),
        },
    }
}

/// The neutral record of a decoded instruction.
pub fn record(d: &Decoded, address: u64) -> Instr {
    let mut operands = Vec::new();
    let mut registers = Vec::new();
    let r = |n: u8| Operand::Reg(register(n));
    let read = |registers: &mut Vec<(Register, Access)>, n: u8| registers.push((register(n), Access::Read));
    let write = |registers: &mut Vec<(Register, Access)>, n: u8| registers.push((register(n), Access::Write));
    let mut flow = Flow::Next;
    let (mut reads_memory, mut writes_memory) = (false, false);
    let mut flags = (false, false);
    match d.op {
        Op::Alu { op, s, rd, rn, src } => {
            if !op.compares() {
                operands.push(r(rd));
                write(&mut registers, rd);
            }
            if !op.unary() {
                operands.push(r(rn));
                read(&mut registers, rn);
            }
            match src {
                Source::Imm { value, .. } => operands.push(Operand::Imm { value: i64::from(value), bits: 32 }),
                Source::Reg { rm, .. } => {
                    operands.push(r(rm));
                    read(&mut registers, rm);
                }
                Source::RegReg { rm, rs, .. } => {
                    operands.extend([r(rm), r(rs)]);
                    read(&mut registers, rm);
                    read(&mut registers, rs);
                }
            }
            let carry_in = matches!(op, Alu::Adc | Alu::Sbc) || matches!(src, Source::Reg { shift: Shift::Rrx, .. });
            flags = (carry_in, s);
        }
        Op::MovWide { rd, imm, top } => {
            operands.extend([r(rd), Operand::Imm { value: i64::from(imm), bits: 16 }]);
            registers.push((register(rd), if top { Access::ReadWrite } else { Access::Write }));
        }
        Op::Mul { s, rd, rn, rm, acc, .. } => {
            operands.extend([r(rd), r(rn), r(rm)]);
            write(&mut registers, rd);
            for n in [Some(rn), Some(rm), acc].into_iter().flatten() {
                read(&mut registers, n);
            }
            flags = (false, s);
        }
        Op::LongMul { accumulate, lo, hi, rn, rm, .. } => {
            operands.extend([r(lo), r(hi), r(rn), r(rm)]);
            let access = if accumulate { Access::ReadWrite } else { Access::Write };
            registers.extend([(register(lo), access), (register(hi), access)]);
            read(&mut registers, rn);
            read(&mut registers, rm);
        }
        Op::Div { rd, rn, rm, .. } => {
            operands.extend([r(rd), r(rn), r(rm)]);
            write(&mut registers, rd);
            read(&mut registers, rn);
            read(&mut registers, rm);
        }
        Op::Extend { rd, rm, .. } => {
            operands.extend([r(rd), r(rm)]);
            write(&mut registers, rd);
            read(&mut registers, rm);
        }
        Op::Field { rd, rn, lsb, width, .. } => {
            operands.extend([
                r(rd),
                r(rn),
                Operand::Imm { value: i64::from(lsb), bits: 8 },
                Operand::Imm { value: i64::from(width), bits: 8 },
            ]);
            write(&mut registers, rd);
            read(&mut registers, rn);
        }
        Op::Insert { rd, rn, .. } => {
            operands.push(r(rd));
            registers.push((register(rd), Access::ReadWrite));
            if let Some(rn) = rn {
                operands.push(r(rn));
                read(&mut registers, rn);
            }
        }
        Op::Mem { load, bits, rt, rn, offset, index, wback, .. } => {
            operands.push(r(rt));
            let base = register(rn);
            let mut m =
                MemOperand { segment: None, base: Some(base), index: None, scale: 1, disp: 0, bits: u16::from(bits), absolute: None };
            match offset {
                Offset::Imm(v) if index => m.disp = i64::from(v),
                Offset::Imm(_) => {}
                Offset::Reg { rm, shift } => {
                    m.index = Some(register(rm));
                    m.scale = 1 << shift;
                    read(&mut registers, rm);
                }
            }
            if rn == PC {
                m.absolute = Some(aligned_pc(address).wrapping_add(m.disp as u64));
            } else {
                registers.push((base, if wback { Access::ReadWrite } else { Access::Read }));
            }
            operands.push(Operand::Mem(m));
            if load {
                write(&mut registers, rt);
                reads_memory = true;
            } else {
                read(&mut registers, rt);
                writes_memory = true;
            }
        }
        Op::Dual { load, rt, rt2, rn, offset, index, wback } => {
            let disp = if index { i64::from(offset) } else { 0 };
            let absolute = (rn == PC).then(|| aligned_pc(address).wrapping_add(disp as u64));
            operands.extend([
                r(rt),
                r(rt2),
                Operand::Mem(MemOperand { segment: None, base: Some(register(rn)), index: None, scale: 1, disp, bits: 64, absolute }),
            ]);
            registers.push((register(rn), if wback { Access::ReadWrite } else { Access::Read }));
            for n in [rt, rt2] {
                registers.push((register(n), if load { Access::Write } else { Access::Read }));
            }
            (reads_memory, writes_memory) = (load, !load);
        }
        Op::Multiple { load, rn, list, wback, .. } => {
            registers.push((register(rn), if wback { Access::ReadWrite } else { Access::Read }));
            operands.push(r(rn));
            for n in (0..16u8).filter(|n| list & (1 << n) != 0) {
                operands.push(r(n));
                if n != PC {
                    registers.push((register(n), if load { Access::Write } else { Access::Read }));
                }
            }
            (reads_memory, writes_memory) = (load, !load);
            if load && list & (1 << PC) != 0 {
                flow = Flow::Return;
            }
        }
        Op::Branch { cond, target } => {
            operands.push(Operand::Addr(target));
            flow = if cond.is_some() || d.cond.is_some() {
                Flow::CondJump(Target::Direct(target))
            } else {
                Flow::Jump(Target::Direct(target))
            };
            flags.0 = cond.is_some();
        }
        Op::Call { target, .. } => {
            operands.push(Operand::Addr(target));
            write(&mut registers, LR);
            flow = Flow::Call(Target::Direct(target));
        }
        Op::BranchReg { rm, link } => {
            operands.push(r(rm));
            read(&mut registers, rm);
            flow = match (link, rm) {
                (true, _) => {
                    write(&mut registers, LR);
                    Flow::Call(Target::Indirect)
                }
                (false, LR) => Flow::Return,
                (false, _) => Flow::Jump(Target::Indirect),
            };
        }
        Op::CompareBranch { rn, target, .. } => {
            operands.extend([r(rn), Operand::Addr(target)]);
            read(&mut registers, rn);
            flow = Flow::CondJump(Target::Direct(target));
        }
        Op::It { first, mask } => {
            operands.extend([Operand::Imm { value: i64::from(first), bits: 4 }, Operand::Imm { value: i64::from(mask), bits: 4 }]);
        }
        Op::Adr { rd, target } => {
            operands.extend([r(rd), Operand::Addr(target)]);
            write(&mut registers, rd);
        }
        Op::Svc(n) => {
            operands.push(Operand::Imm { value: i64::from(n), bits: 8 });
            for n in 0..8 {
                registers.push((register(n), Access::Read));
            }
            registers.push((register(0), Access::Write));
            flow = Flow::Interrupt;
        }
        Op::Trap => flow = Flow::Trap,
        Op::Nop | Op::Unsupported => {}
    }
    if flags.0 || d.cond.is_some() {
        registers.push((apsr(), Access::Read));
    }
    if flags.1 {
        registers.push((apsr(), Access::Write));
    }
    let mnemonic = format!("{}{}", mnemonic(&d.op), condition_suffix(d.cond));
    let text = if operands.is_empty() {
        mnemonic.clone()
    } else {
        format!("{mnemonic} {}", operands.iter().map(shown).collect::<Vec<_>>().join(", "))
    };
    Instr {
        address,
        len: d.len,
        mnemonic,
        text,
        operands,
        flow,
        prefixes: Prefixes::default(),
        registers,
        reads_memory,
        writes_memory,
        code: d.raw,
    }
}

// endregion

impl Arch for Thumb {
    fn name(&self) -> &'static str {
        "thumb"
    }

    fn address_bits(&self) -> u16 {
        32
    }

    fn stack_pointer(&self) -> Register {
        register(SP)
    }

    fn instruction_pointer(&self) -> Register {
        register(PC)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        decode(bytes, address, None).map(|d| record(&d, address))
    }

    fn link(&self, before: &[Instr], instr: &mut Instr) {
        let Some(cond) = it_condition(before) else { return };
        if let Some(d) = decode_raw(instr.code, instr.address, Some(cond)) {
            *instr = record(&d, instr.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(bytes: &[u8], address: u64) -> Op {
        decode(bytes, address, None).unwrap().op
    }

    #[test]
    fn the_fixture_forms_decode() {
        // muls r0, r0, r0
        assert_eq!(op(&[0x40, 0x43], 0), Op::Mul { s: true, rd: 0, rn: 0, rm: 0, acc: None, subtract: false });
        // bx lr
        assert_eq!(op(&[0x70, 0x47], 0), Op::BranchReg { rm: LR, link: false });
        // it le
        assert_eq!(op(&[0xd8, 0xbf], 0), Op::It { first: 13, mask: 8 });
        // ldr r3, [r2], #4
        assert_eq!(
            op(&[0x52, 0xf8, 0x04, 0x3b], 0),
            Op::Mem { load: true, bits: 32, signed: false, rt: 3, rn: 2, offset: Offset::Imm(4), index: false, wback: true }
        );
        // umull r1, r5, r1, r6
        assert_eq!(op(&[0xa1, 0xfb, 0x06, 0x15], 0), Op::LongMul { signed: false, accumulate: false, lo: 1, hi: 5, rn: 1, rm: 6 });
        // lsr.w r1, r5, #3
        assert_eq!(
            op(&[0x4f, 0xea, 0xd5, 0x01], 0),
            Op::Alu { op: Alu::Mov, s: false, rd: 1, rn: 0, src: Source::Reg { rm: 5, shift: Shift::Lsr, amount: 3 } }
        );
        // orr r5, r5, #0x30
        assert_eq!(op(&[0x45, 0xf0, 0x30, 0x05], 0), Op::Alu { op: Alu::Orr, s: false, rd: 5, rn: 5, src: imm(0x30) });
        // bl 0x20118 from 0x20188
        assert_eq!(op(&[0xff, 0xf7, 0xc6, 0xff], 0x20188), Op::Call { target: 0x20118, arm: false });
        // ldr r6, [pc, #0x3c] at 0x20140 reads 0x20180
        let d = decode(&[0x0f, 0x4e], 0x20140, None).unwrap();
        assert_eq!(record(&d, 0x20140).absolute_memory_operand(), Some(0x20180));
        // bhi 0x20148 from 0x20166
        assert_eq!(op(&[0xef, 0xd8], 0x20166), Op::Branch { cond: Some(8), target: 0x20148 });
        // pop {r4, r5, r6, pc}
        assert_eq!(op(&[0x70, 0xbd], 0), Op::Multiple { load: true, rn: SP, list: 0x8070, wback: true, before: false });
    }

    #[test]
    fn an_it_block_predicates_and_keeps_the_flags() {
        let it = record(&decode(&[0xd8, 0xbf], 0, None).unwrap(), 0);
        assert_eq!(it.mnemonic, "it");
        assert_eq!(it_condition(std::slice::from_ref(&it)), Some(13));
        // movs r0, #1 inside the block is `movle`, which leaves the flags alone.
        let d = decode(&[0x01, 0x20], 2, Some(13)).unwrap();
        assert_eq!(d.op, Op::Alu { op: Alu::Mov, s: false, rd: 0, rn: 0, src: imm(1) });
        assert_eq!(record(&d, 2).mnemonic, "movle");
        // ite eq: the second instruction is `ne`.
        assert_eq!(it_condition_at(0, 0b1100, 1), 1);
        assert_eq!(decode_raw(0xf852_3b04, 0, None).unwrap().len, 4);
    }
}
