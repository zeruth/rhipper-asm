//! The intermediate representation: a subset of LLVM IR, with its own printer. The lift produces
//! it, the raising passes rewrite it, and the backends consume it. Every module the printer
//! writes must pass `clang -c -x ir`; that is the gate the tests hold it to.
//!
//! The subset: integer, float, pointer, vector, array and named struct types; `alloca`, `load`,
//! `store`, the binary operations, `icmp`, `fcmp`, `select`, the casts, `getelementptr`, `call`,
//! `phi`; `ret`, `br`, `switch` and `unreachable`. Values are named, never numbered, so a pass can
//! insert and remove them freely.

pub mod builder;
pub mod dom;
pub mod inst;
pub mod module;
pub mod printer;
pub mod types;

pub use builder::FunctionBuilder;
pub use inst::{BinOp, Callee, Cast, FCond, ICond, Inst, Instruction, Operand, Terminator};
pub use module::{Block, BlockId, CallConv, Const, Declaration, Function, Global, Linkage, Module, ValueId};
pub use types::{FnType, Type};
