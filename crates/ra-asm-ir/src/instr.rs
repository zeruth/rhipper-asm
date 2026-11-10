//! The instruction record and its operands. The IR stays architecture-neutral: an x86 register
//! is a named scalar location, and a memory operand names its base, index, scale and displacement
//! explicitly.

use serde::{Deserialize, Serialize};

/// The width of a value, in bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Width {
    Bits8,
    Bits16,
    Bits32,
    Bits64,
}

/// An operand of an instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operand {
    /// A named location, such as a register or a flag.
    Reg(String, Width),
    /// An immediate, sign-extended to its width.
    Imm(i64, Width),
    /// A memory location through base + index * scale + displacement.
    Mem {
        base: Option<String>,
        index: Option<String>,
        scale: u8,
        disp: i64,
        width: Width,
    },
}

/// One operation. `mnemonic` is architecture-neutral at first; the x86 decoder fills it with its
/// own name, and the translators match on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instr {
    /// The address of the instruction in the mapped image.
    pub address: u64,
    pub mnemonic: String,
    pub operands: Vec<Operand>,
}

impl Instr {
    /// A leaf instruction: an address, a name and no operands.
    pub fn new(address: u64, mnemonic: impl Into<String>) -> Self {
        Self { address, mnemonic: mnemonic.into(), operands: Vec::new() }
    }
}
