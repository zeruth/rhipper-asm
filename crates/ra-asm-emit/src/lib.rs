//! The backends: source out of the raised IR. The C backend is first; C++ and Rust share its
//! structuring.
//!
//! The IR arrives at the ABI level: functions with parameters and results, locals in allocas, data
//! in globals, conditions as comparisons. The structuring pass turns each function's graph into
//! statements (`if`, `else`, loops, `switch`, with `goto` only where the graph is not reducible),
//! and the printer writes them in the language's syntax, inlining a value into its one use where
//! that keeps the meaning.

pub mod c;
pub mod structure;

pub use c::{Options as COptions, emit_c};
