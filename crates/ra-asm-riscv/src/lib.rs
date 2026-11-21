//! The RISC-V decoder: a 32-bit instruction word of RV32 or RV64 as its fields.
//!
//! What is decoded is the base integer set (`I`), on both widths, and the multiply and divide
//! extension (`M`): the instructions a compiler writes for integer code when it is told to leave
//! the compressed ones out. An instruction is its mnemonic, its registers, its immediate and its
//! shape ([`Form`]), which says how the three read; a branch's and a jump's immediate is the
//! address it goes to. [`Instruction`]'s text is the disassembler's own, with no alias taken
//! (`addi a0, zero, 0x1`, never `li`): the fixture's listing is the oracle the tests hold it to.
//!
//! The compressed instructions, the floating-point ones, the atomics and the control registers
//! are not here yet: a word of those decodes to `None`.
//!
//! [`lift`] writes a program's code as IR at the machine's own level, and [`run`] reads a
//! program's ELF file, lifts it, and runs it over the interpreter with the system calls a program
//! without a C library makes.

pub mod lift;
pub mod run;

use std::fmt;

/// What is wrong with a program, or how a run of one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The names of the integer registers, by number, as the calling convention has them.
pub const REGISTERS: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "s2", "s3", "s4", "s5",
    "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

/// The width of the machine's registers: what `XLEN` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Rv32,
    Rv64,
}

/// How an instruction's fields read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `rd` and the upper twenty bits: `lui`, `auipc`.
    Upper,
    /// `rd` and where it goes: `jal`.
    Jump,
    /// `rd`, an offset and the register it is from: `jalr`.
    JumpRegister,
    /// Two registers compared, and where it goes: the branches.
    Branch,
    /// `rd`, and an offset from a register: the loads.
    Load,
    /// The register stored (`rs2`), and an offset from a register: the stores.
    Store,
    /// `rd`, a register and an immediate: `addi` and its kin, the shifts by a constant.
    Immediate,
    /// `rd` and two registers.
    Register,
    /// No operand: `ecall`, `ebreak`.
    System,
}

/// One instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub mnemonic: &'static str,
    pub form: Form,
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
    /// The immediate, its sign extended; the address a branch or a jump goes to.
    pub imm: i64,
}

impl Instruction {
    /// Where control may go beside the next instruction: a branch's or a `jal`'s target.
    #[must_use]
    pub fn target(&self) -> Option<u64> {
        matches!(self.form, Form::Jump | Form::Branch).then_some(self.imm.cast_unsigned())
    }

    /// Whether control never goes on to the next instruction: a jump that links nothing.
    #[must_use]
    pub fn ends_block(&self) -> bool {
        matches!(self.form, Form::Jump | Form::JumpRegister) && self.rd == 0
    }
}

/// A number as the disassembler writes one: hexadecimal, its sign before it.
struct Hex(i64);

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 0 { write!(f, "-0x{:x}", self.0.unsigned_abs()) } else { write!(f, "0x{:x}", self.0) }
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = |n: u8| REGISTERS[usize::from(n & 31)];
        let (m, imm) = (self.mnemonic, Hex(self.imm));
        match self.form {
            Form::Upper | Form::Jump => write!(f, "{m} {}, {imm}", r(self.rd)),
            Form::JumpRegister | Form::Load => write!(f, "{m} {}, {imm}({})", r(self.rd), r(self.rs1)),
            Form::Branch => write!(f, "{m} {}, {}, {imm}", r(self.rs1), r(self.rs2)),
            Form::Store => write!(f, "{m} {}, {imm}({})", r(self.rs2), r(self.rs1)),
            Form::Immediate => write!(f, "{m} {}, {}, {imm}", r(self.rd), r(self.rs1)),
            Form::Register => write!(f, "{m} {}, {}, {}", r(self.rd), r(self.rs1), r(self.rs2)),
            Form::System => f.write_str(m),
        }
    }
}

/// The bits `high..=low` of a word.
fn bits(word: u32, high: u32, low: u32) -> u32 {
    (word >> low) & ((1u32 << (high - low + 1)) - 1)
}

/// The low `width` bits of `value` as a signed number.
fn signed(value: u32, width: u32) -> i64 {
    let shift = 64 - width;
    (i64::from(value) << shift) >> shift
}

/// Decodes the instruction `word` at `address` for a machine of `width`: `None` for a word that
/// is no instruction of the sets the decoder has, or one the width has not (`addiw` on RV32, a
/// shift by more than the register holds).
#[must_use]
pub fn decode(word: u32, address: u64, width: Width) -> Option<Instruction> {
    let (opcode, rd, rs1, rs2) = (bits(word, 6, 0), bits(word, 11, 7) as u8, bits(word, 19, 15) as u8, bits(word, 24, 20) as u8);
    let (funct3, funct7) = (bits(word, 14, 12), bits(word, 31, 25));
    let wide = width == Width::Rv64;
    let make = |mnemonic, form, imm| Some(Instruction { mnemonic, form, rd, rs1, rs2, imm });
    let i_imm = signed(bits(word, 31, 20), 12);
    match opcode {
        0x37 => make("lui", Form::Upper, i64::from(bits(word, 31, 12))),
        0x17 => make("auipc", Form::Upper, i64::from(bits(word, 31, 12))),
        0x6f => {
            let offset =
                signed(bits(word, 31, 31) << 20 | bits(word, 19, 12) << 12 | bits(word, 20, 20) << 11 | bits(word, 30, 21) << 1, 21);
            make("jal", Form::Jump, address.wrapping_add_signed(offset).cast_signed())
        }
        0x67 if funct3 == 0 => make("jalr", Form::JumpRegister, i_imm),
        0x63 => {
            let mnemonic = ["beq", "bne", "", "", "blt", "bge", "bltu", "bgeu"][funct3 as usize];
            let offset = signed(bits(word, 31, 31) << 12 | bits(word, 7, 7) << 11 | bits(word, 30, 25) << 5 | bits(word, 11, 8) << 1, 13);
            (!mnemonic.is_empty()).then_some(())?;
            make(mnemonic, Form::Branch, address.wrapping_add_signed(offset).cast_signed())
        }
        0x03 => {
            let mnemonic = ["lb", "lh", "lw", "ld", "lbu", "lhu", "lwu", ""][funct3 as usize];
            // The loads of a doubleword and of an unsigned word are the wide machine's.
            (!mnemonic.is_empty() && (wide || !matches!(funct3, 3 | 6))).then_some(())?;
            make(mnemonic, Form::Load, i_imm)
        }
        0x23 => {
            let mnemonic = ["sb", "sh", "sw", "sd"].get(funct3 as usize).copied()?;
            (wide || funct3 != 3).then_some(())?;
            make(mnemonic, Form::Store, signed(bits(word, 31, 25) << 5 | bits(word, 11, 7), 12))
        }
        0x13 => match funct3 {
            // A shift's count is six bits on the wide machine and five on the other; the bits
            // above it say which shift.
            1 | 5 => {
                let (count, above) = if wide { (bits(word, 25, 20), bits(word, 31, 26) << 1) } else { (bits(word, 24, 20), funct7) };
                let mnemonic = match (funct3, above) {
                    (1, 0) => "slli",
                    (5, 0) => "srli",
                    (5, 0x20) => "srai",
                    _ => return None,
                };
                make(mnemonic, Form::Immediate, i64::from(count))
            }
            _ => make(["addi", "", "slti", "sltiu", "xori", "", "ori", "andi"][funct3 as usize], Form::Immediate, i_imm),
        },
        0x1b if wide => {
            let mnemonic = match (funct3, funct7) {
                (0, _) => return make("addiw", Form::Immediate, i_imm),
                (1, 0) => "slliw",
                (5, 0) => "srliw",
                (5, 0x20) => "sraiw",
                _ => return None,
            };
            make(mnemonic, Form::Immediate, i64::from(bits(word, 24, 20)))
        }
        0x33 | 0x3b => {
            let word_form = opcode == 0x3b;
            (wide || !word_form).then_some(())?;
            let mnemonic = match (word_form, funct7, funct3) {
                (false, 0, n) => ["add", "sll", "slt", "sltu", "xor", "srl", "or", "and"][n as usize],
                (false, 0x20, 0) => "sub",
                (false, 0x20, 5) => "sra",
                (false, 1, n) => ["mul", "mulh", "mulhsu", "mulhu", "div", "divu", "rem", "remu"][n as usize],
                (true, 0, 0) => "addw",
                (true, 0, 1) => "sllw",
                (true, 0, 5) => "srlw",
                (true, 0x20, 0) => "subw",
                (true, 0x20, 5) => "sraw",
                (true, 1, 0) => "mulw",
                (true, 1, 4) => "divw",
                (true, 1, 5) => "divuw",
                (true, 1, 6) => "remw",
                (true, 1, 7) => "remuw",
                _ => return None,
            };
            make(mnemonic, Form::Register, 0)
        }
        0x73 if word == 0x0000_0073 => make("ecall", Form::System, 0),
        0x73 if word == 0x0010_0073 => make("ebreak", Form::System, 0),
        _ => None,
    }
}

/// The instruction at the start of `bytes`, which lies at `address`, and its length: two bytes
/// for a compressed instruction (its low two bits are not both set), four for the rest. What
/// decodes to nothing still has its length.
#[must_use]
pub fn decode_at(bytes: &[u8], address: u64, width: Width) -> (Option<Instruction>, u8) {
    let half = u16::from_le_bytes([bytes.first().copied().unwrap_or(0), bytes.get(1).copied().unwrap_or(0)]);
    if half & 3 != 3 {
        return (compressed(half, address, width), 2);
    }
    match bytes.get(..4) {
        Some(word) => (decode(u32::from_le_bytes([word[0], word[1], word[2], word[3]]), address, width), 4),
        None => (None, 2),
    }
}

/// Decodes a compressed instruction (the `C` extension) as the instruction of the base set it
/// stands for: `c.addi sp, -16` is `addi sp, sp, -16`, `c.jr ra` is `jalr zero, 0(ra)`. The
/// floating-point loads and stores among them are not decoded.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn compressed(half: u16, address: u64, width: Width) -> Option<Instruction> {
    let h = u32::from(half);
    let wide = width == Width::Rv64;
    // The fields: a full register, the two short registers (x8 to x15), and the pieces the
    // immediates are put together from.
    let (full, low) = (bits(h, 11, 7) as u8, bits(h, 6, 2) as u8);
    let (short1, short2) = (8 + bits(h, 9, 7) as u8, 8 + bits(h, 4, 2) as u8);
    let imm6 = signed(bits(h, 12, 12) << 5 | bits(h, 6, 2), 6);
    let shift = i64::from(bits(h, 12, 12) << 5 | bits(h, 6, 2));
    let make = |mnemonic, form, rd, rs1, rs2, imm| Some(Instruction { mnemonic, form, rd, rs1, rs2, imm });
    match (h & 3, bits(h, 15, 13)) {
        (0, 0) => {
            let imm = bits(h, 10, 7) << 6 | bits(h, 12, 11) << 4 | bits(h, 5, 5) << 3 | bits(h, 6, 6) << 2;
            (imm != 0).then_some(())?;
            make("addi", Form::Immediate, short2, 2, 0, i64::from(imm))
        }
        (0, 2 | 6) => {
            let offset = i64::from(bits(h, 5, 5) << 6 | bits(h, 12, 10) << 3 | bits(h, 6, 6) << 2);
            if bits(h, 15, 13) == 2 {
                make("lw", Form::Load, short2, short1, 0, offset)
            } else {
                make("sw", Form::Store, 0, short1, short2, offset)
            }
        }
        (0, 3 | 7) if wide => {
            let offset = i64::from(bits(h, 6, 5) << 6 | bits(h, 12, 10) << 3);
            if bits(h, 15, 13) == 3 {
                make("ld", Form::Load, short2, short1, 0, offset)
            } else {
                make("sd", Form::Store, 0, short1, short2, offset)
            }
        }
        (1, 0) => make("addi", Form::Immediate, full, full, 0, imm6),
        (1, 1) if wide => (full != 0).then_some(()).and_then(|()| make("addiw", Form::Immediate, full, full, 0, imm6)),
        (1, 2) => make("addi", Form::Immediate, full, 0, 0, imm6),
        (1, 3) if full == 2 => {
            let imm = signed(bits(h, 12, 12) << 9 | bits(h, 4, 3) << 7 | bits(h, 5, 5) << 6 | bits(h, 2, 2) << 5 | bits(h, 6, 6) << 4, 10);
            (imm != 0).then_some(())?;
            make("addi", Form::Immediate, 2, 2, 0, imm)
        }
        (1, 3) => {
            (imm6 != 0).then_some(())?;
            make("lui", Form::Upper, full, 0, 0, imm6 & 0xf_ffff)
        }
        (1, 4) => match bits(h, 11, 10) {
            0 | 1 => {
                (wide || shift < 32).then_some(())?;
                make(if bits(h, 11, 10) == 0 { "srli" } else { "srai" }, Form::Immediate, short1, short1, 0, shift)
            }
            2 => make("andi", Form::Immediate, short1, short1, 0, imm6),
            _ => {
                let mnemonic = match (bits(h, 12, 12), bits(h, 6, 5)) {
                    (0, 0) => "sub",
                    (0, 1) => "xor",
                    (0, 2) => "or",
                    (0, 3) => "and",
                    (1, 0) if wide => "subw",
                    (1, 1) if wide => "addw",
                    _ => return None,
                };
                make(mnemonic, Form::Register, short1, short1, short2, 0)
            }
        },
        (1, 5) => {
            let offset = signed(
                bits(h, 12, 12) << 11
                    | bits(h, 8, 8) << 10
                    | bits(h, 10, 9) << 8
                    | bits(h, 6, 6) << 7
                    | bits(h, 7, 7) << 6
                    | bits(h, 2, 2) << 5
                    | bits(h, 11, 11) << 4
                    | bits(h, 5, 3) << 1,
                12,
            );
            make("jal", Form::Jump, 0, 0, 0, address.wrapping_add_signed(offset).cast_signed())
        }
        (1, 6 | 7) => {
            let offset =
                signed(bits(h, 12, 12) << 8 | bits(h, 6, 5) << 6 | bits(h, 2, 2) << 5 | bits(h, 11, 10) << 3 | bits(h, 4, 3) << 1, 9);
            let mnemonic = if bits(h, 15, 13) == 6 { "beq" } else { "bne" };
            make(mnemonic, Form::Branch, 0, short1, 0, address.wrapping_add_signed(offset).cast_signed())
        }
        (2, 0) => {
            (wide || shift < 32).then_some(())?;
            make("slli", Form::Immediate, full, full, 0, shift)
        }
        (2, 2) => {
            (full != 0).then_some(())?;
            make("lw", Form::Load, full, 2, 0, i64::from(bits(h, 3, 2) << 6 | bits(h, 12, 12) << 5 | bits(h, 6, 4) << 2))
        }
        (2, 3) if wide => {
            (full != 0).then_some(())?;
            make("ld", Form::Load, full, 2, 0, i64::from(bits(h, 4, 2) << 6 | bits(h, 12, 12) << 5 | bits(h, 6, 5) << 3))
        }
        (2, 4) => match (bits(h, 12, 12), full, low) {
            (0, 0, 0) => None,
            (0, rs1, 0) => make("jalr", Form::JumpRegister, 0, rs1, 0, 0),
            (0, rd, rs2) => make("add", Form::Register, rd, 0, rs2, 0),
            (_, 0, 0) => make("ebreak", Form::System, 0, 0, 0, 0),
            (_, rs1, 0) => make("jalr", Form::JumpRegister, 1, rs1, 0, 0),
            (_, rd, rs2) => make("add", Form::Register, rd, rd, rs2, 0),
        },
        (2, 6) => make("sw", Form::Store, 0, 2, low, i64::from(bits(h, 8, 7) << 6 | bits(h, 12, 9) << 2)),
        (2, 7) if wide => make("sd", Form::Store, 0, 2, low, i64::from(bits(h, 9, 7) << 6 | bits(h, 12, 10) << 3)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(word: u32, address: u64, width: Width) -> Option<String> {
        decode(word, address, width).map(|i| i.to_string())
    }

    #[test]
    fn an_instruction_reads_as_the_disassembler_writes_it() {
        for (word, address, expected) in [
            (0x0001_0637u32, 0x111b8u64, "lui a2, 0x10"),
            (0x01f5_5693, 0x111bc, "srli a3, a0, 0x1f"),
            (0x1906_3603, 0x111c4, "ld a2, 0x190(a2)"),
            (0xf007_0713, 0x111cc, "addi a4, a4, -0x100"),
            (0x02c5_0633, 0x111d4, "mul a2, a0, a2"),
            (0x0000_8067, 0x111fc, "jalr zero, 0x0(ra)"),
            (0x04e0_5e63, 0x112c8, "bge zero, a4, 0x11324"),
            (0x0211_3c23, 0x11334, "sd ra, 0x38(sp)"),
            (0xe49f_f0ef, 0x11370, "jal ra, 0x111b8"),
            (0x4383_d813, 0x11300, "srai a6, t2, 0x38"),
            (0x0000_0073, 0, "ecall"),
        ] {
            assert_eq!(text(word, address, Width::Rv64).as_deref(), Some(expected), "{word:08x}");
        }
    }

    #[test]
    fn each_width_has_its_own_instructions() {
        // ld, addiw and a shift by 56 are the wide machine's alone.
        for word in [0x1906_3603u32, 0x0015_051b, 0x4383_d813] {
            assert!(decode(word, 0, Width::Rv64).is_some(), "{word:08x}");
            assert_eq!(decode(word, 0, Width::Rv32), None, "{word:08x}");
        }
        // A compressed instruction, a floating-point one and a word of zeros are none yet.
        for word in [0x0000_4501u32, 0x0020_81d3, 0] {
            assert_eq!(decode(word, 0, Width::Rv64), None, "{word:08x}");
        }
    }

    #[test]
    fn a_jump_says_where_it_goes() {
        let call = decode(0xe49f_f0ef, 0x11370, Width::Rv64).unwrap();
        assert_eq!((call.target(), call.ends_block()), (Some(0x111b8), false));
        let back = decode(0x0000_8067, 0x111fc, Width::Rv64).unwrap();
        assert_eq!((back.target(), back.ends_block()), (None, true));
    }
}
