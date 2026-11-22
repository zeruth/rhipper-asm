//! The MSP430, a 16-bit microcontroller, as a machine to read: [`decode`] takes an instruction's
//! words apart (its operation, its two operands in their seven addressing modes, the constants
//! two of its registers stand for), and [`disasm`] writes it as the disassembler does, which
//! the listing of a fixture program holds it to.
//!
//! The processor meant is the original one, of sixteen-bit addresses; the extended
//! instructions of the larger ones are not decoded.

pub mod decode;
pub mod disasm;
