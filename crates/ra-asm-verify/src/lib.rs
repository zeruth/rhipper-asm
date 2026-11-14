//! The evidence that a port behaves as its program.
//!
//! The unit of evidence is the function vector ([`vector`]): what one activation of a function
//! was given, what it asked of the functions it called, and what it left behind. This crate
//! holds its shape and its file, and the recorder ([`record`]) that takes the vectors of a
//! program's functions out of an interpreted run; replaying one is built on top of them.

pub mod record;
pub mod vector;

pub use record::{Recorder, Recording, record};
pub use vector::{Call, Error, Left, Register, Run, Vector, read, write};
