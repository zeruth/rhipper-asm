//! The evidence that a port behaves as its program.
//!
//! The unit of evidence is the function vector ([`vector`]): what one activation of a function
//! was given, what it asked of the functions it called, and what it left behind. This crate
//! holds its shape and its file; recording a vector and replaying one are built on top of it.

pub mod vector;

pub use vector::{Call, Error, Left, Register, Run, Vector, read, write};
