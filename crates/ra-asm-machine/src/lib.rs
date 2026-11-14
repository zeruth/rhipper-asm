//! The machines described by rows: a decoder, a lift, an access summary, an assembler and the
//! cases of each instruction from one table per machine. Its tests hold the guest harness of
//! `testdata/vectors/harness`, which runs those cases on the machine itself under qemu-user.
//!
//! An instruction of a fixed-width machine is a row ([`Row`]): the bits its encoding fixes, the
//! fields it carries in the rest, and where control goes after it. A table of rows ([`Table`])
//! checks once that no word is claimed by two rows unless one is strictly more specific, builds a
//! mask-and-match tree, and decodes a word to the most specific row that accepts it and the values
//! of that row's fields.

pub mod table;

pub use table::{Decoded, Error, Field, Flow, Row, Table};
