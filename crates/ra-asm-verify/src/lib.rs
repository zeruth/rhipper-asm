//! The evidence that a port behaves as its program.
//!
//! The unit of evidence is the function vector ([`vector`]): what one activation of a function
//! was given, what it asked of the functions it called, and what it left behind. This crate
//! holds its shape and its file, the recorder ([`record`]) that takes the vectors of a program's
//! functions out of an interpreted run, and the replayer ([`replay`]) that runs one function
//! again from its vector, its callees answered from the record, and says what differs. Above
//! the levels that take a state a vector is read through the function's signature
//! ([`signature`]). A pure function no run entered gets vectors of generated inputs
//! ([`generate`]), the machine level their reference. What the replays have shown of each
//! function is kept across runs in the ledger ([`ledger`]).

pub mod generate;
pub mod ledger;
pub mod record;
pub mod replay;
pub mod signature;
pub mod vector;

pub use generate::{Summary, generate, mutate, summary};
pub use ledger::{Assumed, Counts, Evidence, Ledger, State, Tested, digest, read_ledger, write_ledger};
pub use record::{Recorder, Recording, keeping, record};
pub use replay::{Covered, Difference, Failure, Replayed, Replayer, Report, replay, replay_abi};
pub use signature::{Convention, Returned};
pub use vector::{Call, Error, Left, Register, Run, Vector, read, write};
