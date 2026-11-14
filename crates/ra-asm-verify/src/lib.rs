//! The evidence that a port behaves as its program.
//!
//! The unit of evidence is the function vector ([`vector`]): what one activation of a function
//! was given, what it asked of the functions it called, and what it left behind. This crate
//! holds its shape and its file, the recorder ([`record`]) that takes the vectors of a program's
//! functions out of an interpreted run, and the replayer ([`replay`]) that runs one function
//! again from its vector, its callees answered from the record, and says what differs.

pub mod record;
pub mod replay;
pub mod vector;

pub use record::{Recorder, Recording, record};
pub use replay::{Difference, Failure, Replayer, replay};
pub use vector::{Call, Error, Left, Register, Run, Vector, read, write};
