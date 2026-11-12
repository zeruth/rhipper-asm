//! The decoder: one yaxpeax instruction becomes one neutral [`Instr`], with the operands, the flow
//! and every register and memory access. The text is yaxpeax's, in the architecture's own syntax
//! with the PC-relative targets resolved.

use ra_asm_arch::{Access, Arch, Flow, Instr, MemOperand, Operand, Prefixes, Register, Target};
use yaxpeax_arch::{Decoder, U8Reader};
use yaxpeax_arm::armv8::a64::{InstDecoder, Instruction, Opcode, Operand as Op, ShiftStyle, SizeCode};

use crate::registers;

/// The `AArch64` decoder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arm64;

/// A decoded instruction with yaxpeax's own record beside the neutral one.
#[derive(Debug, Clone)]
pub struct Decoded {
    pub instr: Instr,
    pub inst: Instruction,
}

impl Arm64 {
    /// Decodes one instruction and keeps yaxpeax's record beside the neutral one.
    pub fn decode_full(self, bytes: &[u8], address: u64) -> Option<Decoded> {
        if bytes.len() < 4 {
            return None;
        }
        let inst = InstDecoder::default().decode(&mut U8Reader::new(&bytes[..4])).ok()?;
        if inst.opcode == Opcode::Invalid {
            return None;
        }
        Some(Decoded { instr: record(&inst, address), inst })
    }
}

impl Arch for Arm64 {
    fn name(&self) -> &'static str {
        "arm64"
    }

    fn address_bits(&self) -> u16 {
        64
    }

    fn stack_pointer(&self) -> Register {
        registers::sp(64)
    }

    fn instruction_pointer(&self) -> Register {
        registers::pc()
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        self.decode_full(bytes, address).map(|d| d.instr)
    }

    fn link(&self, before: &[Instr], instr: &mut Instr) {
        // `adrp x8, page` then `ldr x8, [x8, #lo12]`, with nothing writing `x8` between: the load
        // names the whole address, as a RIP-relative operand does on x64.
        let Some(Operand::Mem(m)) = instr.operands.get_mut(1) else { return };
        let Some(base) = m.base else { return };
        if m.index.is_some() || m.absolute.is_some() {
            return;
        }
        let Some(previous) = before.iter().rev().find(|i| i.writes(base)) else { return };
        if previous.mnemonic != "adrp" {
            return;
        }
        if let (Some(Operand::Reg(page_reg)), Some(Operand::Addr(page))) = (previous.operands.first(), previous.operands.get(1))
            && page_reg.full == base.full
        {
            m.absolute = Some(page.wrapping_add(m.disp as u64));
        }
    }
}

/// The target of a PC-relative operand: `adrp` names a page relative to the instruction's own.
fn pc_target(opcode: Opcode, address: u64, offset: i64) -> u64 {
    if opcode == Opcode::ADRP { (address & !0xfff).wrapping_add(offset as u64) } else { address.wrapping_add(offset as u64) }
}

fn bits_of(size: SizeCode) -> u16 {
    match size {
        SizeCode::X => 64,
        SizeCode::W => 32,
    }
}

fn simd_bits(size: yaxpeax_arm::armv8::a64::SIMDSizeCode) -> u16 {
    use yaxpeax_arm::armv8::a64::SIMDSizeCode as C;
    match size {
        C::B => 8,
        C::H => 16,
        C::S => 32,
        C::D => 64,
        C::Q => 128,
    }
}

/// A general register operand: number 31 is the zero register unless the form takes the stack pointer.
fn general(n: u16, size: SizeCode, or_sp: bool) -> Register {
    let bits = bits_of(size);
    match (n, or_sp) {
        (31, true) => registers::sp(bits),
        (31, false) => registers::zero(bits),
        _ => registers::general(n, bits),
    }
}

/// The width of the memory access an instruction makes, in bits: from its mnemonic for the
/// sub-word forms, else from its first register operand.
fn access_bits(inst: &Instruction, mnemonic: &str) -> u16 {
    let first = match inst.operands[0] {
        Op::Register(size, _) | Op::RegisterOrSP(size, _) => bits_of(size),
        Op::RegisterPair(size, _) => bits_of(size) * 2,
        Op::SIMDRegister(size, _) => simd_bits(size),
        _ => 64,
    };
    match mnemonic {
        m if m.ends_with("sb") || m.ends_with("rb") || m == "strb" || m == "ldrb" || m == "sturb" || m == "ldurb" => 8,
        m if m.ends_with("sh") || m == "strh" || m == "ldrh" || m == "sturh" || m == "ldurh" => 16,
        "ldrsw" | "ldursw" => 32,
        "ldpsw" => 64,
        m if m.starts_with("ldp") || m.starts_with("stp") || m.starts_with("ldnp") || m.starts_with("stnp") || m.starts_with("ldxp") => {
            first * 2
        }
        _ => first,
    }
}

/// The mnemonic as yaxpeax prints the instruction: the alias a reader knows (`mov`, `cmp`, `lsr`,
/// `nop`) rather than the encoding's own name (`orr`, `subs`, `ubfm`, `hint`).
fn mnemonic_of(text: &str) -> String {
    text.split_whitespace().next().unwrap_or(text).to_owned()
}

/// The condition names, in encoding order.
pub fn condition_name(cond: u8) -> &'static str {
    ["eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "al", "nv"][(cond & 0xf) as usize]
}

/// Whether an opcode stores to memory.
fn stores(mnemonic: &str) -> bool {
    mnemonic.starts_with("st") && !mnemonic.starts_with("stlxr") || mnemonic.starts_with("cas") || mnemonic.starts_with("swp")
}

fn loads(mnemonic: &str) -> bool {
    mnemonic.starts_with("ld") || mnemonic.starts_with("cas") || mnemonic.starts_with("swp") || mnemonic == "prfm" || mnemonic == "prfum"
}

fn reads_flags(opcode: Opcode) -> bool {
    matches!(
        opcode,
        Opcode::Bcc(_)
            | Opcode::BCcc(_)
            | Opcode::CSEL
            | Opcode::CSINC
            | Opcode::CSINV
            | Opcode::CSNEG
            | Opcode::CCMP
            | Opcode::CCMN
            | Opcode::ADC
            | Opcode::ADCS
            | Opcode::SBC
            | Opcode::SBCS
            | Opcode::FCSEL
            | Opcode::FCCMP
            | Opcode::FCCMPE
            | Opcode::MRS
    )
}

fn writes_flags(opcode: Opcode) -> bool {
    matches!(
        opcode,
        Opcode::ADDS
            | Opcode::SUBS
            | Opcode::ANDS
            | Opcode::BICS
            | Opcode::ADCS
            | Opcode::SBCS
            | Opcode::CCMP
            | Opcode::CCMN
            | Opcode::FCMP
            | Opcode::FCMPE
            | Opcode::FCCMP
            | Opcode::FCCMPE
            | Opcode::MSR
            | Opcode::SETF8
            | Opcode::SETF16
            | Opcode::RMIF
            | Opcode::SUBPS
    )
}

fn record(inst: &Instruction, address: u64) -> Instr {
    let opcode = inst.opcode;
    let mut text = format!("{inst}");
    let mnemonic = mnemonic_of(&text);
    let store = stores(&mnemonic);
    let load = loads(&mnemonic);
    let memory = store || load;
    let width = if memory { access_bits(inst, &mnemonic) } else { 0 };
    let or_sp = matches!(
        opcode,
        Opcode::ADD | Opcode::SUB | Opcode::ADDS | Opcode::SUBS | Opcode::AND | Opcode::ORR | Opcode::EOR | Opcode::MOVZ | Opcode::MOVN
    );

    let mut operands = Vec::new();
    let mut registers: Vec<(Register, Access)> = Vec::new();
    let count = inst.operands.iter().take_while(|o| !matches!(o, Op::Nothing)).count();
    // The first operand is the destination of a plain instruction; a store, a compare-and-branch
    // and a test-and-branch read it; a load pair writes its first two.
    let first_is_written = !store
        && !matches!(
            opcode,
            Opcode::CBZ | Opcode::CBNZ | Opcode::TBZ | Opcode::TBNZ | Opcode::RET | Opcode::BR | Opcode::BLR | Opcode::CCMP | Opcode::CCMN
        )
        && count > 0
        && matches!(inst.operands[0], Op::Register(..) | Op::RegisterOrSP(..) | Op::SIMDRegister(..));
    let pair = matches!(opcode, Opcode::LDP | Opcode::LDPSW | Opcode::LDNP | Opcode::LDXP | Opcode::LDAXP);
    for (k, op) in inst.operands.iter().enumerate().take(count) {
        let access = if k == 0 && first_is_written || k == 1 && pair {
            // A compare-and-swap or a bitfield insert reads its destination too.
            if matches!(opcode, Opcode::BFM | Opcode::MOVK) || mnemonic.starts_with("cas") { Access::ReadWrite } else { Access::Write }
        } else {
            Access::Read
        };
        let neutral = match *op {
            Op::Nothing => continue,
            Op::Register(size, n) => {
                let r = general(n, size, false);
                if n != 31 {
                    registers.push((r, access));
                }
                Operand::Reg(r)
            }
            Op::RegisterOrSP(size, n) => {
                let r = general(n, size, or_sp || k > 0);
                registers.push((r, access));
                Operand::Reg(r)
            }
            Op::RegisterPair(size, n) => {
                let r = general(n, size, false);
                registers.push((r, access));
                registers.push((general((n + 1) & 31, size, false), access));
                Operand::Reg(r)
            }
            Op::SIMDRegister(size, n) => {
                let r = registers::vector(n, simd_bits(size));
                registers.push((r, access));
                Operand::Reg(r)
            }
            Op::SIMDRegisterElements(_, n, _)
            | Op::SIMDRegisterElementsLane(_, n, _, _)
            | Op::SIMDRegisterElementsMultipleLane(_, n, _, _, _) => {
                let r = registers::vector(n, 128);
                registers.push((r, access));
                Operand::Reg(r)
            }
            Op::SIMDRegisterGroup(_, n, _, group) | Op::SIMDRegisterGroupLane(n, _, group, _) => {
                for g in 0..group {
                    registers.push((registers::vector((n + u16::from(g)) & 31, 128), access));
                }
                Operand::Reg(registers::vector(n, 128))
            }
            Op::RegShift(style, amount, size, n) => {
                let r = general(n, size, false);
                if n != 31 {
                    registers.push((r, Access::Read));
                }
                let _ = (style, amount);
                Operand::Reg(r)
            }
            Op::ConditionCode(c) => Operand::Imm { value: i64::from(c), bits: 4 },
            Op::PCOffset(offset) => Operand::Addr(pc_target(opcode, address, offset)),
            Op::Immediate(v) => Operand::Imm { value: i64::from(v), bits: 32 },
            Op::Imm64(v) => Operand::Imm { value: v as i64, bits: 64 },
            Op::Imm16(v) | Op::PrefetchOp(v) | Op::SystemReg(v) | Op::ControlReg(v) => Operand::Imm { value: i64::from(v), bits: 16 },
            Op::ImmShift(v, shift) => Operand::Imm { value: i64::from(v) << shift, bits: 64 },
            Op::ImmShiftMSL(v, shift) => Operand::Imm { value: (i64::from(v) << shift) | ((1i64 << shift) - 1), bits: 64 },
            Op::ImmediateDouble(d) => Operand::Imm { value: d.to_bits() as i64, bits: 64 },
            Op::RegPreIndex(base, offset, writeback) => {
                let b = general(base, SizeCode::X, true);
                registers.push((b, if writeback { Access::ReadWrite } else { Access::Read }));
                Operand::Mem(MemOperand {
                    segment: None,
                    base: Some(b),
                    index: None,
                    scale: 1,
                    disp: i64::from(offset),
                    bits: width,
                    absolute: None,
                })
            }
            Op::RegPostIndex(base, _) | Op::RegPostIndexReg(base, _) => {
                let b = general(base, SizeCode::X, true);
                registers.push((b, Access::ReadWrite));
                if let Op::RegPostIndexReg(_, idx) = *op {
                    registers.push((general(idx, SizeCode::X, false), Access::Read));
                }
                Operand::Mem(MemOperand { segment: None, base: Some(b), index: None, scale: 1, disp: 0, bits: width, absolute: None })
            }
            Op::RegRegOffset(base, index, size, style, amount) => {
                let b = general(base, SizeCode::X, true);
                let i = general(index, size, false);
                registers.push((b, Access::Read));
                registers.push((i, Access::Read));
                let scale = if style == ShiftStyle::LSL || amount > 0 { 1u8 << amount.min(4) } else { 1 };
                Operand::Mem(MemOperand { segment: None, base: Some(b), index: Some(i), scale, disp: 0, bits: width, absolute: None })
            }
            Op::PstateField(v) => Operand::Imm { value: i64::from(v), bits: 8 },
        };
        operands.push(neutral);
    }
    // The implicit registers: the link register of a call, the return address of `ret`.
    match opcode {
        Opcode::BL | Opcode::BLR => registers.push((registers::general(30, 64), Access::Write)),
        Opcode::RET if count == 0 => registers.push((registers::general(30, 64), Access::Read)),
        _ => {}
    }
    if reads_flags(opcode) {
        registers.push((registers::nzcv(), Access::Read));
    }
    if writes_flags(opcode) {
        registers.push((registers::nzcv(), Access::Write));
    }

    let direct = |k: usize| match operands.get(k) {
        Some(Operand::Addr(t)) => Target::Direct(*t),
        _ => Target::Indirect,
    };
    let flow = match opcode {
        Opcode::B => Flow::Jump(direct(0)),
        Opcode::BR | Opcode::BRAA | Opcode::BRAAZ | Opcode::BRAB | Opcode::BRABZ => Flow::Jump(Target::Indirect),
        Opcode::BL => Flow::Call(direct(0)),
        Opcode::BLR | Opcode::BLRAA | Opcode::BLRAAZ | Opcode::BLRAB | Opcode::BLRABZ => Flow::Call(Target::Indirect),
        Opcode::Bcc(_) | Opcode::BCcc(_) => Flow::CondJump(direct(0)),
        Opcode::CBZ | Opcode::CBNZ => Flow::CondJump(direct(1)),
        Opcode::TBZ | Opcode::TBNZ => Flow::CondJump(direct(2)),
        Opcode::RET | Opcode::RETAA | Opcode::RETAB | Opcode::ERET | Opcode::ERETAA | Opcode::ERETAB => Flow::Return,
        Opcode::BRK | Opcode::UDF | Opcode::Invalid => Flow::Trap,
        Opcode::HLT => Flow::Halt,
        Opcode::SVC | Opcode::HVC | Opcode::SMC => Flow::Interrupt,
        _ => Flow::Next,
    };

    // yaxpeax writes a PC-relative target as `$+0x10`; the reader wants the address.
    if let Some(dollar) = text.find('$') {
        let target = operands.iter().find_map(|o| match o {
            Operand::Addr(t) => Some(*t),
            _ => None,
        });
        if let Some(target) = target {
            let end = text[dollar..].find([',', ' ']).map_or(text.len(), |e| dollar + e);
            text.replace_range(dollar..end, &format!("{target:#x}"));
        }
    }

    Instr {
        address,
        len: 4,
        mnemonic,
        text,
        operands,
        flow,
        prefixes: Prefixes::default(),
        registers,
        reads_memory: load,
        writes_memory: store,
        code: opcode_code(opcode),
    }
}

/// A stable number for the form of an instruction: the discriminant of yaxpeax's opcode.
fn opcode_code(opcode: Opcode) -> u32 {
    // The enum has no explicit discriminants; its layout starts with the tag.
    // SAFETY: not needed; the tag is read through a plain transmute-free hash of the Debug name.
    let name = format!("{opcode:?}");
    name.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
}

#[cfg(test)]
mod tests;
