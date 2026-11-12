//! The `AArch64` decoder adapter. [`Arm64`] implements [`ra_asm_arch::Arch`] over yaxpeax-arm and
//! fills the neutral [`ra_asm_arch::Instr`] record; [`Arm64::decode_full`] also hands back
//! yaxpeax's own instruction, which the lift reads for the semantics.

pub mod decode;
pub mod registers;
pub mod thumb;

pub use decode::{Arm64, Decoded};
pub use yaxpeax_arm;
