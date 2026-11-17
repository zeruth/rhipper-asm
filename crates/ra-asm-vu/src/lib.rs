//! The micro mode of the Emotion Engine's vector units, VU0 and VU1. One 64-bit word pairs an
//! upper instruction, the four-lane float arithmetic over the 32 vector registers and the
//! accumulator (the broadcast field, the `dest` mask, the MAC, status and clip flags), with a lower
//! one: the 16 integer registers, the loads and stores, the branches, the divider, the elementary
//! function unit and `xgkick`. Both issue at once. The upper word also carries the `i`, `e`, `m`,
//! `d` and `t` bits; with `i` set the lower word is no instruction but a float the pair loads into
//! `I`, which the text spells `loi`.
//!
//! A branch takes effect one pair late, as a MIPS branch does: the pair after it, its delay slot,
//! runs first. The `e` bit ends the program the same way, after the next pair. The neutral record
//! keeps the transfer's own flow; [`Vu::delay`] says when it takes effect and [`Vu::successors`]
//! where control goes once it has.
//!
//! The text is the assembler's (`dvp-as`): the upper and the lower instruction separated by a
//! tab, a branch target as the address in the unit's micro memory, counted in bytes.

use ra_asm_arch::{Access, Arch, Flow, Instr, MemOperand, Operand, Prefixes, RegClass, Register, Target};

pub use ra_asm_arch::Delay;

/// The first vector register, `vf00`: `vf<n>` is `VF0 + n`. `vf00` always reads `(0, 0, 0, 1)`.
pub const VF0: u16 = 0;
/// The first integer register, `vi00`: `vi<n>` is `VI0 + n`. `vi00` always reads zero.
pub const VI0: u16 = 32;
/// The accumulator the `...a` forms of the upper instructions write.
pub const ACC: u16 = 48;
/// The divider's result.
pub const Q: u16 = 49;
/// The elementary function unit's result.
pub const P: u16 = 50;
/// The immediate register `loi` loads.
pub const I: u16 = 51;
/// The random number register.
pub const R: u16 = 52;
pub const STATUS: u16 = 53;
pub const MAC: u16 = 54;
pub const CLIP: u16 = 55;
pub const PC: u16 = 56;
pub const COUNT: usize = 57;

const NAMES: [&str; COUNT] = [
    "vf00", "vf01", "vf02", "vf03", "vf04", "vf05", "vf06", "vf07", "vf08", "vf09", "vf10", "vf11", "vf12", "vf13", "vf14", "vf15", "vf16",
    "vf17", "vf18", "vf19", "vf20", "vf21", "vf22", "vf23", "vf24", "vf25", "vf26", "vf27", "vf28", "vf29", "vf30", "vf31", "vi00", "vi01",
    "vi02", "vi03", "vi04", "vi05", "vi06", "vi07", "vi08", "vi09", "vi10", "vi11", "vi12", "vi13", "vi14", "vi15", "acc", "q", "p", "i",
    "r", "status", "mac", "clip", "pc",
];

const fn entry(id: u16) -> Register {
    let (class, bits) = if id < VI0 {
        (RegClass::Vector, 128)
    } else if id < ACC {
        (RegClass::General, 16)
    } else {
        match id {
            ACC => (RegClass::Vector, 128),
            STATUS => (RegClass::Flags, 12),
            MAC => (RegClass::Flags, 16),
            CLIP => (RegClass::Flags, 24),
            PC => (RegClass::InstructionPointer, 16),
            _ => (RegClass::Float, 32),
        }
    };
    Register { id, name: NAMES[id as usize], class, full: id, bits, shift: 0 }
}

const fn table() -> [Register; COUNT] {
    let mut out = [entry(0); COUNT];
    let mut id = 0;
    while id < COUNT {
        out[id] = entry(id as u16);
        id += 1;
    }
    out
}

static REGISTERS: [Register; COUNT] = table();

/// The register of an id below [`COUNT`].
pub fn register(id: u16) -> Register {
    REGISTERS[usize::from(id)]
}

/// The lanes in the order of the text, `x` first; the `dest` mask holds `x` in bit 3.
const FIELDS: [&str; 4] = ["x", "y", "z", "w"];

/// The `ft` and `it` field.
fn rt(word: u32) -> u32 {
    (word >> 16) & 0x1f
}

/// The `fs` and `is` field.
fn rs(word: u32) -> u32 {
    (word >> 11) & 0x1f
}

/// The `fd` and `id` field.
fn rd(word: u32) -> u32 {
    (word >> 6) & 0x1f
}

/// The lane a two-bit field at `shift` selects: `fsf` at 21, `ftf` at 23.
fn field(word: u32, shift: u32) -> &'static str {
    FIELDS[((word >> shift) & 3) as usize]
}

fn sign(value: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    i64::from(((value << shift) as i32) >> shift)
}

fn merge(registers: &mut Vec<(Register, Access)>, register: Register, access: Access) {
    match registers.iter_mut().find(|(r, _)| r.id == register.id) {
        Some(entry) if entry.1 != access => entry.1 = Access::ReadWrite,
        Some(_) => {}
        None => registers.push((register, access)),
    }
}

/// One half of a pair, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    /// The mnemonic with its broadcast field and without the `dest` mask: `maddw`, `lq`.
    pub mnemonic: String,
    /// The `dest` mask, `x` in bit 3 and `w` in bit 0, of an instruction that has one.
    pub dest: Option<u8>,
    pub text: String,
    pub operands: Vec<Operand>,
    pub registers: Vec<(Register, Access)>,
    pub reads_memory: bool,
    pub writes_memory: bool,
    pub flow: Flow,
    /// The opcode, unique within the half: the upper's in 12 bits, the lower's in 18.
    pub key: u32,
}

/// How a load or store forms its address from its integer register, in quadwords.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The register plus an offset.
    Offset,
    /// The register alone.
    Plain,
    /// The register, which then steps up by one.
    Increment,
    /// The register less one, which it keeps.
    Decrement,
}

struct Builder {
    op: Op,
    args: Vec<String>,
}

impl Builder {
    fn new(mnemonic: impl Into<String>, key: u32) -> Self {
        let op = Op {
            mnemonic: mnemonic.into(),
            dest: None,
            text: String::new(),
            operands: Vec::new(),
            registers: Vec::new(),
            reads_memory: false,
            writes_memory: false,
            flow: Flow::Next,
            key,
        };
        Self { op, args: Vec::new() }
    }

    fn dest(mut self, word: u32) -> Self {
        self.op.dest = Some(((word >> 21) & 0xf) as u8);
        self
    }

    /// A write through a `dest` mask that leaves a lane as it was reads the register too.
    fn write(&self, access: Access) -> Access {
        if access == Access::Write && self.op.dest.is_some_and(|d| d != 0xf) { Access::ReadWrite } else { access }
    }

    fn touch(&mut self, id: u16, access: Access) {
        merge(&mut self.op.registers, register(id), access);
    }

    fn named(&mut self, id: u16, access: Access, text: String) {
        self.touch(id, access);
        self.op.operands.push(Operand::Reg(register(id)));
        self.args.push(text);
    }

    /// A vector register, its text followed by `lanes`.
    fn vf(&mut self, n: u32, access: Access, lanes: &str) {
        let access = self.write(access);
        self.named(VF0 + n as u16, access, format!("vf{n:02}{lanes}"));
    }

    fn vi(&mut self, n: u32, access: Access) -> Option<()> {
        if n > 15 {
            return None;
        }
        self.named(VI0 + n as u16, access, format!("vi{n:02}"));
        Some(())
    }

    /// `acc`, `q`, `p`, `i` or `r`.
    fn special(&mut self, id: u16, access: Access) {
        let access = if id == ACC { self.write(access) } else { access };
        self.named(id, access, register(id).name.to_owned());
    }

    fn imm(&mut self, value: i64, bits: u16, hex: bool) {
        self.op.operands.push(Operand::Imm { value, bits });
        self.args.push(if hex { format!("{value:#x}") } else { value.to_string() });
    }

    fn target(&mut self, target: u64) {
        self.op.operands.push(Operand::Addr(target));
        self.args.push(format!("{target:#x}"));
    }

    /// A load or store through integer register `base`. An integer one (`integer` the width of the
    /// access) touches the one lane its `dest` mask selects; a vector one the whole quadword.
    fn mem(&mut self, base: u32, offset: i64, step: Step, integer: Option<u16>, write: bool) -> Option<()> {
        if base > 15 {
            return None;
        }
        let index = register(VI0 + base as u16);
        self.touch(index.id, if matches!(step, Step::Increment | Step::Decrement) { Access::ReadWrite } else { Access::Read });
        let lane = match (integer, self.op.dest) {
            (Some(bits), Some(dest @ (8 | 4 | 2 | 1))) => Some((i64::from(3 - dest.trailing_zeros()) * 4, bits)),
            _ => None,
        };
        let (lane, bits) = lane.unwrap_or((0, 128));
        let disp = match step {
            Step::Offset => offset * 16,
            Step::Decrement => -16,
            Step::Plain | Step::Increment => 0,
        } + lane;
        let operand = MemOperand { segment: None, base: None, index: Some(index), scale: 16, disp, bits, absolute: None };
        let name = index.name;
        self.args.push(match step {
            Step::Offset => format!("{offset}({name})"),
            Step::Plain => format!("({name})"),
            Step::Increment => format!("({name}++)"),
            Step::Decrement => format!("(--{name})"),
        });
        self.op.operands.push(Operand::Mem(operand));
        if write {
            self.op.writes_memory = true;
        } else {
            self.op.reads_memory = true;
        }
        Some(())
    }

    /// The op, its text the mnemonic, the `dest` mask, `flags` and the operands.
    fn finish(mut self, flags: &str) -> Op {
        let mut head = self.op.mnemonic.clone();
        if let Some(dest) = self.op.dest {
            head.push('.');
            for (bit, lane) in [8, 4, 2, 1].into_iter().zip(FIELDS) {
                if dest & bit != 0 {
                    head.push_str(lane);
                }
            }
        }
        head.push_str(flags);
        self.op.text = if self.args.is_empty() { head } else { format!("{head} {}", self.args.join(", ")) };
        self.op
    }
}

/// The third operand of an upper arithmetic instruction.
#[derive(Clone, Copy)]
enum Source {
    /// One lane of `ft`, broadcast to every lane.
    Broadcast,
    Q,
    I,
    /// `ft`, lane by lane.
    Vector,
}

/// `name` into `fd`, or into the accumulator, from `fs` and the source.
fn arithmetic(word: u32, name: &str, source: Source, into_acc: bool, key: u32) -> Builder {
    let lane = FIELDS[(word & 3) as usize];
    let suffix = match source {
        Source::Broadcast => lane,
        Source::Q => "q",
        Source::I => "i",
        Source::Vector => "",
    };
    let acc = if into_acc { "a" } else { "" };
    let mut b = Builder::new(format!("{name}{acc}{suffix}"), key).dest(word);
    if into_acc {
        b.special(ACC, Access::Write);
    } else {
        b.vf(rd(word), Access::Write, "");
    }
    b.vf(rs(word), Access::Read, "");
    match source {
        Source::Broadcast => b.vf(rt(word), Access::Read, lane),
        Source::Q => b.special(Q, Access::Read),
        Source::I => b.special(I, Access::Read),
        Source::Vector => b.vf(rt(word), Access::Read, ""),
    }
    if matches!(name, "madd" | "msub" | "opmsub") {
        b.touch(ACC, Access::Read);
    }
    if !matches!(name, "max" | "mini") {
        b.touch(MAC, Access::Write);
        b.touch(STATUS, Access::ReadWrite);
    }
    b
}

/// `abs` and the conversions: `ft` from `fs`.
fn convert(word: u32, name: &str, key: u32) -> Builder {
    let mut b = Builder::new(name, key).dest(word);
    b.vf(rt(word), Access::Write, "");
    b.vf(rs(word), Access::Read, "");
    b
}

/// The upper instruction of `word`, `flags` the text of its `e`, `m`, `d` and `t` bits.
fn upper(word: u32, flags: &str) -> Option<Op> {
    let op = word & 0x3f;
    let (name, source) = match op {
        0x00..=0x1b => (["add", "sub", "madd", "msub", "max", "mini", "mul"][(op >> 2) as usize], Source::Broadcast),
        0x1c => ("mul", Source::Q),
        0x1d => ("max", Source::I),
        0x1e => ("mul", Source::I),
        0x1f => ("mini", Source::I),
        0x20 => ("add", Source::Q),
        0x21 => ("madd", Source::Q),
        0x22 => ("add", Source::I),
        0x23 => ("madd", Source::I),
        0x24 => ("sub", Source::Q),
        0x25 => ("msub", Source::Q),
        0x26 => ("sub", Source::I),
        0x27 => ("msub", Source::I),
        0x28 => ("add", Source::Vector),
        0x29 => ("madd", Source::Vector),
        0x2a => ("mul", Source::Vector),
        0x2b => ("max", Source::Vector),
        0x2c => ("sub", Source::Vector),
        0x2d => ("msub", Source::Vector),
        0x2e => ("opmsub", Source::Vector),
        0x2f => ("mini", Source::Vector),
        0x3c..=0x3f => return special_upper(word, flags),
        _ => return None,
    };
    Some(arithmetic(word, name, source, false, op).finish(flags))
}

/// The upper instructions whose opcode goes on into the `fd` field: 11 bits.
fn special_upper(word: u32, flags: &str) -> Option<Op> {
    let op = word & 0x7ff;
    let key = 0x800 | op;
    let lane = (word & 3) as usize;
    let acc = |name: &str, source: Source| arithmetic(word, name, source, true, key);
    let b = match op {
        0x03c..=0x03f => acc("add", Source::Broadcast),
        0x07c..=0x07f => acc("sub", Source::Broadcast),
        0x0bc..=0x0bf => acc("madd", Source::Broadcast),
        0x0fc..=0x0ff => acc("msub", Source::Broadcast),
        0x13c..=0x13f => convert(word, ["itof0", "itof4", "itof12", "itof15"][lane], key),
        0x17c..=0x17f => convert(word, ["ftoi0", "ftoi4", "ftoi12", "ftoi15"][lane], key),
        0x1bc..=0x1bf => acc("mul", Source::Broadcast),
        0x1fc => acc("mul", Source::Q),
        0x1fd => convert(word, "abs", key),
        0x1fe => acc("mul", Source::I),
        0x1ff => {
            let mut b = Builder::new("clipw", key).dest(word);
            b.vf(rs(word), Access::Read, "xyz");
            b.vf(rt(word), Access::Read, "w");
            b.touch(CLIP, Access::ReadWrite);
            b
        }
        0x23c => acc("add", Source::Q),
        0x23d => acc("madd", Source::Q),
        0x23e => acc("add", Source::I),
        0x23f => acc("madd", Source::I),
        0x27c => acc("sub", Source::Q),
        0x27d => acc("msub", Source::Q),
        0x27e => acc("sub", Source::I),
        0x27f => acc("msub", Source::I),
        0x2bc => acc("add", Source::Vector),
        0x2bd => acc("madd", Source::Vector),
        0x2be => acc("mul", Source::Vector),
        0x2fc => acc("sub", Source::Vector),
        0x2fd => acc("msub", Source::Vector),
        0x2fe => acc("opmul", Source::Vector),
        0x2ff => Builder::new("nop", key),
        _ => return None,
    };
    Some(b.finish(flags))
}

/// The lower word of a pair whose `i` bit is set: a float for `I`.
fn loi(word: u32) -> Op {
    let value = f32::from_bits(word);
    let mut b = Builder::new("loi", 0x7f << 11);
    b.touch(I, Access::Write);
    b.op.operands.push(Operand::Imm { value: i64::from(word as i32), bits: 32 });
    b.args.push(if value.is_finite() { format!("{value:?}") } else { format!("{word:#x}") });
    b.finish("")
}

/// The lower instruction of `word` at `address`, a branch target wrapping within `mask`.
fn lower(word: u32, address: u64, mask: u64) -> Option<Op> {
    let op = word >> 25;
    let key = op << 11;
    let (it, is) = (rt(word), rs(word));
    let offset = sign(word & 0x7ff, 11);
    let target = address.wrapping_add(8).wrapping_add((offset * 8) as u64) & mask;
    let imm15 = i64::from(((word >> 10) & 0x7800) | (word & 0x7ff));
    let imm12 = i64::from(((word >> 10) & 0x800) | (word & 0x7ff));
    let imm24 = i64::from(word & 0xff_ffff);
    let new = |name: &str| Builder::new(name, key);
    let b = match op {
        0x00 => {
            let mut b = new("lq").dest(word);
            b.vf(it, Access::Write, "");
            b.mem(is, offset, Step::Offset, None, false)?;
            b
        }
        0x01 => {
            let mut b = new("sq").dest(word);
            b.vf(is, Access::Read, "");
            b.mem(it, offset, Step::Offset, None, true)?;
            b
        }
        0x04 => {
            let mut b = new("ilw").dest(word);
            b.vi(it, Access::Write)?;
            b.mem(is, offset, Step::Offset, Some(16), false)?;
            b
        }
        0x05 => {
            let mut b = new("isw").dest(word);
            b.vi(it, Access::Read)?;
            b.mem(is, offset, Step::Offset, Some(32), true)?;
            b
        }
        0x08 | 0x09 => {
            let mut b = new(if op == 0x08 { "iaddiu" } else { "isubiu" });
            b.vi(it, Access::Write)?;
            b.vi(is, Access::Read)?;
            b.imm(imm15, 15, false);
            b
        }
        0x10 | 0x12 | 0x13 => {
            let mut b = new(["fceq", "", "fcand", "fcor"][(op & 3) as usize]);
            b.vi(1, Access::Write)?;
            b.imm(imm24, 24, true);
            b.touch(CLIP, Access::Read);
            b
        }
        0x11 => {
            let mut b = new("fcset");
            b.imm(imm24, 24, true);
            b.touch(CLIP, Access::Write);
            b
        }
        0x14 | 0x16 | 0x17 => {
            let mut b = new(["fseq", "", "fsand", "fsor"][(op & 3) as usize]);
            b.vi(it, Access::Write)?;
            b.imm(imm12, 12, true);
            b.touch(STATUS, Access::Read);
            b
        }
        0x15 => {
            let mut b = new("fsset");
            b.imm(imm12, 12, true);
            b.touch(STATUS, Access::ReadWrite);
            b
        }
        0x18 | 0x1a | 0x1b => {
            let mut b = new(["fmeq", "", "fmand", "fmor"][(op & 3) as usize]);
            b.vi(it, Access::Write)?;
            b.vi(is, Access::Read)?;
            b.touch(MAC, Access::Read);
            b
        }
        0x1c => {
            let mut b = new("fcget");
            b.vi(it, Access::Write)?;
            b.touch(CLIP, Access::Read);
            b
        }
        0x20 => {
            let mut b = new("b");
            b.target(target);
            b.op.flow = Flow::Jump(Target::Direct(target));
            b
        }
        0x21 => {
            let mut b = new("bal");
            b.vi(it, Access::Write)?;
            b.target(target);
            b.op.flow = Flow::Call(Target::Direct(target));
            b
        }
        0x24 => {
            let mut b = new("jr");
            b.vi(is, Access::Read)?;
            // `bal` and `jalr` link through `vi15` by the assembler's convention.
            b.op.flow = if is == 15 { Flow::Return } else { Flow::Jump(Target::Indirect) };
            b
        }
        0x25 => {
            let mut b = new("jalr");
            b.vi(it, Access::Write)?;
            b.vi(is, Access::Read)?;
            // The register it calls through comes first, where `Instr::indirect_operand` reads it.
            b.op.operands.swap(0, 1);
            b.op.flow = Flow::Call(Target::Indirect);
            b
        }
        0x28 | 0x29 => {
            let mut b = new(if op == 0x28 { "ibeq" } else { "ibne" });
            b.vi(it, Access::Read)?;
            b.vi(is, Access::Read)?;
            b.target(target);
            b.op.flow = Flow::CondJump(Target::Direct(target));
            b
        }
        0x2c..=0x2f => {
            let mut b = new(["ibltz", "ibgtz", "iblez", "ibgez"][(op & 3) as usize]);
            b.vi(is, Access::Read)?;
            b.target(target);
            b.op.flow = Flow::CondJump(Target::Direct(target));
            b
        }
        0x40 => return special_lower(word),
        _ => return None,
    };
    Some(b.finish(""))
}

/// The lower instructions of opcode `0x40`, told apart by the low six bits.
fn special_lower(word: u32) -> Option<Op> {
    let op = word & 0x3f;
    let key = (0x40 << 11) | op;
    let b = match op {
        0x30 | 0x31 | 0x34 | 0x35 => {
            let name = match op {
                0x30 => "iadd",
                0x31 => "isub",
                0x34 => "iand",
                _ => "ior",
            };
            let mut b = Builder::new(name, key);
            b.vi(rd(word), Access::Write)?;
            b.vi(rs(word), Access::Read)?;
            b.vi(rt(word), Access::Read)?;
            b
        }
        0x32 => {
            let mut b = Builder::new("iaddi", key);
            b.vi(rt(word), Access::Write)?;
            b.vi(rs(word), Access::Read)?;
            b.imm(sign(rd(word), 5), 5, false);
            b
        }
        0x3c..=0x3f => return extended_lower(word),
        _ => return None,
    };
    Some(b.finish(""))
}

/// The elementary function unit's instruction of an extended opcode.
fn efu(op: u32) -> &'static str {
    match op {
        0x73c => "esadd",
        0x73d => "ersadd",
        0x73e => "eleng",
        0x73f => "erleng",
        0x77c => "eatanxy",
        0x77d => "eatanxz",
        0x77e => "esum",
        0x7bc => "esqrt",
        0x7bd => "ersqrt",
        0x7be => "ercpr",
        0x7fc => "esin",
        0x7fd => "eatan",
        _ => "eexp",
    }
}

/// The lower instructions whose opcode goes on into the `id` field: 11 bits.
fn extended_lower(word: u32) -> Option<Op> {
    let op = word & 0x7ff;
    let key = (0x40 << 11) | op;
    let (it, is) = (rt(word), rs(word));
    let (fsf, ftf) = (field(word, 21), field(word, 23));
    let new = |name: &str| Builder::new(name, key);
    let b = match op {
        // A move of no lane is the lower `nop`.
        0x33c if (word >> 21).trailing_zeros() >= 4 => new("nop"),
        0x33c | 0x33d => {
            let mut b = new(if op == 0x33c { "move" } else { "mr32" }).dest(word);
            b.vf(it, Access::Write, "");
            b.vf(is, Access::Read, "");
            b
        }
        0x37c | 0x37e => {
            let mut b = new(if op == 0x37c { "lqi" } else { "lqd" }).dest(word);
            b.vf(it, Access::Write, "");
            b.mem(is, 0, if op == 0x37c { Step::Increment } else { Step::Decrement }, None, false)?;
            b
        }
        0x37d | 0x37f => {
            let mut b = new(if op == 0x37d { "sqi" } else { "sqd" }).dest(word);
            b.vf(is, Access::Read, "");
            b.mem(it, 0, if op == 0x37d { Step::Increment } else { Step::Decrement }, None, true)?;
            b
        }
        0x3bc | 0x3be => {
            let mut b = new(if op == 0x3bc { "div" } else { "rsqrt" });
            b.special(Q, Access::Write);
            b.vf(is, Access::Read, fsf);
            b.vf(it, Access::Read, ftf);
            b.touch(STATUS, Access::ReadWrite);
            b
        }
        0x3bd => {
            let mut b = new("sqrt");
            b.special(Q, Access::Write);
            b.vf(it, Access::Read, ftf);
            b.touch(STATUS, Access::ReadWrite);
            b
        }
        0x3bf => new("waitq"),
        0x3fc => {
            let mut b = new("mtir");
            b.vi(it, Access::Write)?;
            b.vf(is, Access::Read, fsf);
            b
        }
        0x3fd => {
            let mut b = new("mfir").dest(word);
            b.vf(it, Access::Write, "");
            b.vi(is, Access::Read)?;
            b
        }
        0x3fe => {
            let mut b = new("ilwr").dest(word);
            b.vi(it, Access::Write)?;
            b.mem(is, 0, Step::Plain, Some(16), false)?;
            b
        }
        0x3ff => {
            let mut b = new("iswr").dest(word);
            b.vi(it, Access::Read)?;
            b.mem(is, 0, Step::Plain, Some(32), true)?;
            b
        }
        0x43c | 0x43d => {
            let mut b = new(if op == 0x43c { "rnext" } else { "rget" }).dest(word);
            b.vf(it, Access::Write, "");
            b.special(R, if op == 0x43c { Access::ReadWrite } else { Access::Read });
            b
        }
        0x43e | 0x43f => {
            let mut b = new(if op == 0x43e { "rinit" } else { "rxor" });
            b.special(R, if op == 0x43e { Access::Write } else { Access::ReadWrite });
            b.vf(is, Access::Read, fsf);
            b
        }
        0x67c => {
            let mut b = new("mfp").dest(word);
            b.vf(it, Access::Write, "");
            b.special(P, Access::Read);
            b
        }
        0x6bc | 0x6bd => {
            let mut b = new(if op == 0x6bc { "xtop" } else { "xitop" });
            b.vi(it, Access::Write)?;
            b
        }
        0x6fc => {
            // The GIF reads the packet the register points at.
            let mut b = new("xgkick");
            b.vi(is, Access::Read)?;
            b.op.reads_memory = true;
            b
        }
        // The whole-vector functions name no lane: they read the ones they need.
        0x73c..=0x73f | 0x77c..=0x77e => {
            let mut b = new(efu(op));
            b.special(P, Access::Write);
            b.vf(is, Access::Read, "");
            b
        }
        0x7bc..=0x7be | 0x7fc..=0x7fe => {
            let mut b = new(efu(op));
            b.special(P, Access::Write);
            b.vf(is, Access::Read, fsf);
            b
        }
        0x7bf => new("waitp"),
        _ => return None,
    };
    Some(b.finish(""))
}

/// The bits of the upper word beside its instruction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bits {
    /// The lower word is a float for `I`.
    pub i: bool,
    /// The program ends after the next pair.
    pub e: bool,
    /// VU0 only: the pair interlocks with the core's `qmtc2.i` and `cfc2.i`.
    pub m: bool,
    /// A debug break, when the core enabled them.
    pub d: bool,
    /// A halt that interrupts the core, when the core enabled them.
    pub t: bool,
}

/// A decoded pair: the two halves and the bits of the upper word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub upper: Op,
    /// The lower instruction, or `loi` when the `i` bit makes the lower word a float.
    pub lower: Op,
    pub bits: Bits,
}

impl Pair {
    /// Where control goes after the pair: the lower's transfer, the end of the program after an
    /// `e` bit, an interrupt of the core after a `d` or `t` bit.
    pub fn flow(&self) -> Flow {
        if self.bits.e {
            Flow::Halt
        } else if self.lower.flow != Flow::Next {
            self.lower.flow
        } else if self.bits.d || self.bits.t {
            Flow::Interrupt
        } else {
            Flow::Next
        }
    }

    /// The pair as one neutral record of 8 bytes: the lower's operands first, as the transfer's
    /// operand must be, and `code` the upper's key in the low 12 bits and the lower's above.
    pub fn instr(self, address: u64) -> Instr {
        let flow = self.flow();
        let mut registers = self.lower.registers;
        for (register, access) in self.upper.registers {
            merge(&mut registers, register, access);
        }
        let mut operands = self.lower.operands;
        operands.extend(self.upper.operands);
        Instr {
            address,
            len: 8,
            mnemonic: format!("{} {}", self.upper.mnemonic, self.lower.mnemonic),
            text: format!("{}\t{}", self.upper.text, self.lower.text),
            operands,
            flow,
            prefixes: Prefixes::default(),
            registers,
            reads_memory: self.lower.reads_memory,
            writes_memory: self.lower.writes_memory,
            code: self.upper.key | (self.lower.key << 12),
        }
    }
}

/// The micro mode decoder of VU1, or with `vu0` of VU0, whose micro memory is smaller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Vu {
    pub vu0: bool,
}

impl Vu {
    /// The size of the unit's micro memory in bytes, within which a branch target wraps.
    pub fn memory(self) -> u64 {
        if self.vu0 { 0x1000 } else { 0x4000 }
    }

    /// The pair of the 64-bit `word` at `address`; `None` when either half is no instruction, or
    /// when an `e` bit sits beside a branch, which the unit forbids.
    pub fn pair(self, word: u64, address: u64) -> Option<Pair> {
        let (low, high) = (word as u32, (word >> 32) as u32);
        let bit = |n: u32| high & (1 << n) != 0;
        let bits = Bits { i: bit(31), e: bit(30), m: bit(29), d: bit(28), t: bit(27) };
        let letters: String =
            [(bits.e, 'e'), (bits.m, 'm'), (bits.d, 'd'), (bits.t, 't')].iter().filter(|(on, _)| *on).map(|(_, c)| *c).collect();
        let flags = if letters.is_empty() { letters } else { format!("[{letters}]") };
        let upper = upper(high, &flags)?;
        let lower = if bits.i { loi(low) } else { lower(low, address, self.memory() - 1)? };
        if bits.e && lower.flow != Flow::Next {
            return None;
        }
        Some(Pair { upper, lower, bits })
    }

    /// When the flow of `instr`, decoded by this crate, takes effect: a branch and the end of the
    /// program after the next pair.
    pub fn delay(instr: &Instr) -> Delay {
        match instr.flow {
            Flow::Next | Flow::Interrupt | Flow::Trap => Delay::None,
            Flow::Jump(_) | Flow::CondJump(_) | Flow::Call(_) | Flow::Return | Flow::Halt => Delay::Slot,
        }
    }

    /// Where control goes once the flow of `instr` has taken effect, the taken target first: a
    /// branch falls through, or returns from its call, past its delay slot.
    pub fn successors(instr: &Instr) -> Vec<u64> {
        let after = match Self::delay(instr) {
            Delay::None => instr.end(),
            Delay::Slot | Delay::Likely => instr.end().wrapping_add(8),
        };
        let mut out = Vec::with_capacity(2);
        if let Flow::Jump(Target::Direct(target)) | Flow::CondJump(Target::Direct(target)) = instr.flow {
            out.push(target);
        }
        if instr.flow.falls_through() {
            out.push(after);
        }
        out
    }
}

impl Arch for Vu {
    fn name(&self) -> &'static str {
        if self.vu0 { "vu0" } else { "vu1" }
    }

    fn address_bits(&self) -> u16 {
        16
    }

    /// The micro mode has no stack: `vi00`, which always reads zero whatever writes it, stands in.
    fn stack_pointer(&self) -> Register {
        register(VI0)
    }

    fn instruction_pointer(&self) -> Register {
        register(PC)
    }

    fn delay(&self, instr: &Instr) -> Delay {
        Vu::delay(instr)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let word = u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
        Some(self.pair(word, address)?.instr(address))
    }
}
