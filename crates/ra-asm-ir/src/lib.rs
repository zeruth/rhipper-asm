//! The intermediate representation. One `Instr` is one operation with explicit operands; the
//! x86 decoder emits them, and the translators of the pipeline consume them.

pub mod instr;
