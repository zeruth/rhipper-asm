//! The SH-4 of the Dreamcast as the neutral record: the [`Sh4`] decoder of the 16-bit encoding, in
//! either byte order and either FPU mode of `fpscr`, and the [`registers`] it names.
//!
//! An SH transfer takes effect one instruction late, as a MIPS one does: the instruction after
//! `bra`, `bsr`, `braf`, `bsrf`, `jmp`, `jsr`, `rts`, `rte`, `bt/s` and `bf/s`, its delay slot, runs
//! before control reaches the target; `bt` and `bf` have none. The record keeps the transfer's own
//! flow; [`Sh4::delay`] says when that flow takes effect and [`Sh4::successors`] where control goes
//! once it has.

//!
//! [`lift`] writes a program's code as IR at the machine's own level, and [`run`] reads a
//! program's ELF file, lifts it, and runs it over the interpreter.

pub mod decode;
pub mod lift;
pub mod registers;
pub mod run;

pub use decode::Sh4;
pub use ra_asm_arch::Delay;
