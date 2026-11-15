//! The MIPS machine, described by rows of `ra-asm-machine`: the integer instructions of MIPS I and
//! II with the MIPS32 conditional moves and `mul`, in either byte order, and the [`Mips`] decoder
//! the analysis walks.
//!
//! A MIPS transfer takes effect one instruction late: the instruction after a branch, its delay
//! slot, runs before control reaches the target, and after a likely branch it runs only when the
//! branch is taken. The neutral record keeps the branch's own flow; [`Mips::delay`] says when that
//! flow takes effect and [`Mips::successors`] where control goes once it has.

pub mod arch;

pub use arch::{Delay, Mips};
