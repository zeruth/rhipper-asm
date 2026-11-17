//! Shared binary-format helpers: reads over a byte slice in either byte order, little by default,
//! where every refusal names the field and the offset it was read at. The PE loader and its XBE
//! reader are the first users; [`elf`] reads the ELF container the loader maps the same way, and
//! [`macho`] the Mach-O one, and [`psx`] the PSX's `PS-X EXE` and its memory map; [`dreamcast`]
//! unscrambles the Dreamcast's boot program, a raw image with no container at all.

pub mod dreamcast;
pub mod elf;
pub mod macho;
pub mod psx;
pub mod reader;

pub use reader::{ByteOrder, Cursor, Reader};

#[cfg(test)]
mod tests;
