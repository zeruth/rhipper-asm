//! Shared binary-format helpers: little-endian reads over a byte slice, where every refusal names
//! the field and the offset it was read at. The PE loader is the first user; ELF and Mach-O follow.

pub mod reader;

pub use reader::{Cursor, Reader};
