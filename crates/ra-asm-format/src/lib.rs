//! Shared binary-format helpers: little-endian reads over a byte slice, where every refusal names
//! the field and the offset it was read at. The PE loader and its XBE reader are the first users;
//! [`elf`] reads the ELF container the loader maps the same way, and [`macho`] the Mach-O one.

pub mod elf;
pub mod macho;
pub mod reader;

pub use reader::{Cursor, Reader};
