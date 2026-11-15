//! The interpreter's host: the Win32 side of the platform boundary a lifted program runs against.
//! It is the lift's `run` module under the name its users take, until the module itself moves here,
//! with the residual engine behind its [`run`] for the code the lift does not have, and the record
//! of a run's events at that boundary, its [`session`].

mod host;
pub mod session;

pub use host::{residual, run};
pub use ra_asm_lift::run::*;
