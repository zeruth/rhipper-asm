//! The interpreter's host: the Win32 side of the platform boundary a lifted program runs against.
//! It is the lift's `run` module under the name its users take, until the module itself moves here,
//! with the residual engine behind its [`run`] for the code the lift does not have, the record
//! of a run's events at that boundary, its [`session`], a session played back in place of the host,
//! its [`replay`], and the conversion of the structures the program and the host hand each other
//! between their layouts, its [`adapt`]. The host's COM objects a 32-bit program calls through its
//! own vtables are guest objects of [`com`]. The program an unpacking stub wrote, the run stopped
//! where it enters it, is a PE file of its own by [`unpack`].

pub mod adapt;
pub mod com;
mod host;
pub mod replay;
pub mod session;
pub mod unpack;

pub use host::{residual, run};
pub use ra_asm_lift::run::*;
