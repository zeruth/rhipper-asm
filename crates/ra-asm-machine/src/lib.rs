//! The machines described by rows: a decoder, a lift, an access summary, an assembler and the
//! cases of each instruction from one table per machine. Its tests hold the guest harness of
//! `testdata/vectors/harness`, which runs those cases on the machine itself under qemu-user.
//!
//! An instruction of a fixed-width machine is a row ([`Row`]): the bits its encoding fixes, the
//! fields it carries in the rest, and where control goes after it. A table of rows ([`Table`])
//! checks once that no word is claimed by two rows unless one is strictly more specific, builds a
//! mask-and-match tree, and decodes a word to the most specific row that accepts it and the values
//! of that row's fields.
//!
//! A [`Machine`] is a table with its register file and its syntax: it turns a decoded word into the
//! neutral [`ra_asm_arch::Instr`], with the text its objdump prints, from what the fields' kinds
//! say, so neither the analysis nor the disassembly knows anything of one machine.
//!
//! A row carries what its instruction does ([`Sem`]), a function over a [`Builder`]; the IR
//! builder runs it over the state a [`StateSpec`] lays out, and [`Machine::lift`] turns a word
//! into the function of that one instruction. The same function run against a recorder gives what
//! the instruction reads and writes ([`Machine::access`]), which fills the `Instr`'s registers and
//! memory flags.
//!
//! [`Machine::lift_function`] lifts a function's blocks from the same rows, a delayed row with its
//! delay slot; [`Machine::lift_function_outlined`] lifts an outlined row as a call to the one
//! function of it ([`Outlines`]).
//!
//! [`assemble`] turns a row and its field values back into the word, and [`cases()`] draws from a row
//! and a seed the instruction cases the reference vectors run: each field at its boundaries and at
//! random, over registers of boundary and random numbers.

pub mod access;
pub mod asm;
pub mod cases;
pub mod instr;
pub mod lift;
pub mod outline;
pub mod sem;
pub mod table;

pub use access::Summary;
pub use asm::{AsmError, assemble};
pub use cases::{Case, Plan, cases};
pub use instr::{Machine, Syntax};
pub use lift::Code;
pub use outline::Outlines;
pub use sem::{Builder, Cond, Fields, Lift, Op, STATE_TYPE, Sem, StateSpec, Val};
pub use table::{Decoded, Error, Field, Flow, Kind, Row, Table, Transfer};
