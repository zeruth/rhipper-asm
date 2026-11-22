//! The 32-bit PowerPC of the consoles as a machine to lift: [`lift`] writes a program's code as
//! IR at the machine's own level, and [`run`] reads a program's ELF file, lifts it, and runs it
//! over the interpreter with the system calls a program without a C library makes.
//!
//! The processors meant are the big-endian 32-bit ones a GameCube and a Wii have, and the user
//! code of the larger ones after them: the integer machine is lifted, and a fixture built for it
//! is held to what it did under an emulator. [`disasm`] writes an instruction as the
//! disassembler does, which the fixture's listing holds it to.

pub mod disasm;
pub mod lift;
pub mod lift64;
pub mod run;
