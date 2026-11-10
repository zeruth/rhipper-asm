//! The x86-64 decoder: it turns the bytes of a section into [`ra_asm_ir::instr::Instr`] records.
//! The first milestone covers the integer subset: moves, arithmetic, control flow and the stack.

pub mod decode;
