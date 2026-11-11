//! The decoder: one iced instruction becomes one neutral [`Instr`], with the operands, the flow,
//! the prefixes and every register and memory access. The text is iced's Intel syntax.

use iced_x86::{
    Decoder, DecoderOptions, FlowControl, Formatter, Instruction, InstructionInfoFactory, IntelFormatter, Mnemonic, OpAccess, OpKind,
    Register as R,
};
use ra_asm_arch::{Access, Arch, Flow, Instr, MemOperand, Operand, Prefixes, Register, Target};

use crate::registers;

/// The x86 decoder in one mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86 {
    /// 32 or 64.
    pub bits: u32,
}

impl X86 {
    pub const X64: Self = Self { bits: 64 };
    pub const X32: Self = Self { bits: 32 };

    /// The decoder for a pointer width in bytes.
    pub fn for_pointer_size(bytes: u8) -> Self {
        if bytes == 8 { Self::X64 } else { Self::X32 }
    }

    /// Decodes one instruction and keeps iced's record beside the neutral one.
    pub fn decode_full(&self, bytes: &[u8], address: u64) -> Option<Decoded> {
        let mut decoder = Decoder::with_ip(self.bits, bytes, address, DecoderOptions::NONE);
        let iced = decoder.decode();
        if iced.is_invalid() {
            return None;
        }
        Some(Decoded { instr: self.record(&iced), iced })
    }

    /// Decodes a run of instructions with iced's records.
    pub fn decode_all_full(&self, bytes: &[u8], address: u64) -> Vec<Decoded> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let Some(d) = self.decode_full(&bytes[offset..], address + offset as u64) else { break };
            offset += d.instr.len as usize;
            out.push(d);
        }
        out
    }

    fn record(self, i: &Instruction) -> Instr {
        let mut text = String::new();
        let mut formatter = IntelFormatter::new();
        let options = formatter.options_mut();
        options.set_hex_prefix("0x");
        options.set_hex_suffix("");
        options.set_uppercase_hex(false);
        options.set_space_after_operand_separator(true);
        options.set_rip_relative_addresses(false);
        options.set_branch_leading_zeros(false);
        options.set_signed_immediate_operands(false);
        options.set_small_hex_numbers_in_decimal(false);
        formatter.format(i, &mut text);

        let mut operands = Vec::with_capacity(i.op_count() as usize);
        for n in 0..i.op_count() {
            operands.push(self.operand(i, n));
        }

        let flow = match i.flow_control() {
            // iced files hlt under Next; the lift and the analysis want to know execution stops.
            FlowControl::Next if i.mnemonic() == Mnemonic::Hlt => Flow::Halt,
            FlowControl::Next | FlowControl::XbeginXabortXend => Flow::Next,
            FlowControl::UnconditionalBranch => Flow::Jump(Target::Direct(i.near_branch_target())),
            FlowControl::IndirectBranch => Flow::Jump(Target::Indirect),
            FlowControl::ConditionalBranch => Flow::CondJump(Target::Direct(i.near_branch_target())),
            FlowControl::Return => Flow::Return,
            FlowControl::Call => Flow::Call(Target::Direct(i.near_branch_target())),
            FlowControl::IndirectCall => Flow::Call(Target::Indirect),
            FlowControl::Interrupt => {
                if matches!(i.mnemonic(), Mnemonic::Int3 | Mnemonic::Int1 | Mnemonic::Ud0 | Mnemonic::Ud1 | Mnemonic::Ud2) {
                    Flow::Trap
                } else {
                    Flow::Interrupt
                }
            }
            FlowControl::Exception => {
                if i.mnemonic() == Mnemonic::Hlt {
                    Flow::Halt
                } else {
                    Flow::Trap
                }
            }
        };

        let mut factory = InstructionInfoFactory::new();
        let info = factory.info(i);
        let mut registers: Vec<(Register, Access)> = Vec::new();
        for used in info.used_registers() {
            if used.register() == R::None {
                continue;
            }
            let access = match used.access() {
                OpAccess::Read => Access::Read,
                OpAccess::Write => Access::Write,
                OpAccess::ReadWrite => Access::ReadWrite,
                OpAccess::CondRead => Access::CondRead,
                OpAccess::CondWrite | OpAccess::ReadCondWrite => Access::CondWrite,
                OpAccess::None | OpAccess::NoMemAccess => continue,
            };
            registers.push((registers::register(used.register()), access));
        }
        let flags_bits = if self.bits == 64 { 64 } else { 32 };
        if i.rflags_read() != 0 {
            registers.push((registers::flags(flags_bits), Access::Read));
        }
        if i.rflags_modified() != 0 {
            registers.push((registers::flags(flags_bits), Access::Write));
        }
        let mut reads_memory = false;
        let mut writes_memory = false;
        for m in info.used_memory() {
            match m.access() {
                OpAccess::Read | OpAccess::CondRead => reads_memory = true,
                OpAccess::Write | OpAccess::CondWrite => writes_memory = true,
                OpAccess::ReadWrite | OpAccess::ReadCondWrite => {
                    reads_memory = true;
                    writes_memory = true;
                }
                OpAccess::None | OpAccess::NoMemAccess => {}
            }
        }

        Instr {
            address: i.ip(),
            len: i.len() as u8,
            mnemonic: format!("{:?}", i.mnemonic()).to_lowercase(),
            text,
            operands,
            flow,
            prefixes: Prefixes { rep: i.has_rep_prefix(), repne: i.has_repne_prefix(), lock: i.has_lock_prefix() },
            registers,
            reads_memory,
            writes_memory,
            code: i.code() as u32,
        }
    }

    fn operand(self, i: &Instruction, n: u32) -> Operand {
        match i.op_kind(n) {
            OpKind::Register => Operand::Reg(registers::register(i.op_register(n))),
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => Operand::Addr(i.near_branch_target()),
            OpKind::FarBranch16 | OpKind::FarBranch32 => Operand::Imm { value: i64::from(i.far_branch32()), bits: 32 },
            OpKind::Immediate8 => Operand::Imm { value: i64::from(i.immediate8()), bits: 8 },
            OpKind::Immediate8_2nd => Operand::Imm { value: i64::from(i.immediate8_2nd()), bits: 8 },
            OpKind::Immediate16 => Operand::Imm { value: i64::from(i.immediate16()), bits: 16 },
            OpKind::Immediate32 => Operand::Imm { value: i64::from(i.immediate32()), bits: 32 },
            OpKind::Immediate64 => Operand::Imm { value: i.immediate64() as i64, bits: 64 },
            OpKind::Immediate8to16 => Operand::Imm { value: i64::from(i.immediate8to16()), bits: 16 },
            OpKind::Immediate8to32 => Operand::Imm { value: i64::from(i.immediate8to32()), bits: 32 },
            OpKind::Immediate8to64 => Operand::Imm { value: i.immediate8to64(), bits: 64 },
            OpKind::Immediate32to64 => Operand::Imm { value: i.immediate32to64(), bits: 64 },
            OpKind::MemorySegSI | OpKind::MemorySegESI | OpKind::MemorySegRSI => {
                let base = match i.op_kind(n) {
                    OpKind::MemorySegSI => R::SI,
                    OpKind::MemorySegESI => R::ESI,
                    _ => R::RSI,
                };
                string_operand(i, i.memory_segment(), base)
            }
            OpKind::MemorySegDI | OpKind::MemorySegEDI | OpKind::MemorySegRDI => {
                let base = match i.op_kind(n) {
                    OpKind::MemorySegDI => R::DI,
                    OpKind::MemorySegEDI => R::EDI,
                    _ => R::RDI,
                };
                string_operand(i, i.memory_segment(), base)
            }
            OpKind::MemoryESDI | OpKind::MemoryESEDI | OpKind::MemoryESRDI => {
                let base = match i.op_kind(n) {
                    OpKind::MemoryESDI => R::DI,
                    OpKind::MemoryESEDI => R::EDI,
                    _ => R::RDI,
                };
                string_operand(i, R::ES, base)
            }
            OpKind::Memory => {
                let base = i.memory_base();
                let index = i.memory_index();
                let disp = i.memory_displacement64() as i64;
                let bits = (i.memory_size().size() * 8) as u16;
                let absolute = if i.is_ip_rel_memory_operand() {
                    Some(i.ip_rel_memory_address())
                } else if base == R::None && index == R::None {
                    Some(if self.bits == 64 { disp as u64 } else { disp as u64 & 0xffff_ffff })
                } else {
                    None
                };
                Operand::Mem(MemOperand {
                    segment: segment_of(i),
                    base: (base != R::None).then(|| registers::register(base)),
                    index: (index != R::None).then(|| registers::register(index)),
                    scale: i.memory_index_scale() as u8,
                    disp,
                    bits,
                    absolute,
                })
            }
        }
    }
}

/// The implicit memory operand of a string instruction: `segment:[base]`.
fn string_operand(i: &Instruction, segment: R, base: R) -> Operand {
    Operand::Mem(MemOperand {
        segment: (segment != R::None).then(|| registers::register(segment)),
        base: Some(registers::register(base)),
        index: None,
        scale: 1,
        disp: 0,
        bits: (i.memory_size().size() * 8) as u16,
        absolute: None,
    })
}

/// The segment of a memory operand when a prefix or the instruction names one other than the default.
fn segment_of(i: &Instruction) -> Option<Register> {
    let prefix = i.segment_prefix();
    if prefix != R::None {
        return Some(registers::register(prefix));
    }
    let segment = i.memory_segment();
    (segment == R::FS || segment == R::GS).then(|| registers::register(segment))
}

/// A decoded instruction with iced's own record beside the neutral one.
#[derive(Debug, Clone)]
pub struct Decoded {
    pub instr: Instr,
    pub iced: Instruction,
}

impl Arch for X86 {
    fn name(&self) -> &'static str {
        if self.bits == 64 { "x86_64" } else { "x86" }
    }

    fn address_bits(&self) -> u16 {
        self.bits as u16
    }

    fn stack_pointer(&self) -> Register {
        registers::register(if self.bits == 64 { R::RSP } else { R::ESP })
    }

    fn instruction_pointer(&self) -> Register {
        registers::register(if self.bits == 64 { R::RIP } else { R::EIP })
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        self.decode_full(bytes, address).map(|d| d.instr)
    }
}

#[cfg(test)]
mod tests;
