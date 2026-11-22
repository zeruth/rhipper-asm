//! The MSP430, a 16-bit microcontroller, as a machine to read: [`decode`] takes an instruction's
//! words apart (its operation, its two operands in their seven addressing modes, the constants
//! two of its registers stand for), and [`disasm`] writes it as the disassembler does, which
//! the listing of a fixture program holds it to. [`lift`] writes a program's code as IR at the
//! machine's own level, and [`run`] reads a program's ELF file, lifts it, and runs it over the
//! interpreter with the two registers of a board a program with no library prints and ends by.
//!
//! The processor meant is the original one, of sixteen-bit addresses; the extended
//! instructions of the larger ones are not decoded.

pub mod decode;
pub mod disasm;
pub mod lift;
pub mod run;
