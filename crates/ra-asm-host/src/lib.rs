//! The interpreter's host: the Win32 side of the platform boundary a lifted program runs against.
//! It is the lift's `run` module under the name its users take, until the module itself moves here,
//! and the record of a run's events at that boundary, its [`session`].

pub mod session;

pub use ra_asm_lift::run::*;
