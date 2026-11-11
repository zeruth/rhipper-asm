//! The x86 and x86-64 decoder adapter. [`X86`] implements [`ra_asm_arch::Arch`] over iced-x86 and
//! fills the neutral [`ra_asm_arch::Instr`] record; [`X86::decode_full`] also hands back iced's own
//! instruction, which the lift reads for the semantics.

pub mod decode;
pub mod registers;

pub use decode::{Decoded, X86};
pub use iced_x86;
