//! The section table: where each section is in the file and in the mapped image, and what it holds.

use anyhow::Result;
use ra_asm_format::Cursor;
use serde::Serialize;

/// One section header.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub name: String,
    /// The size of the section in the mapped image.
    pub virtual_size: u32,
    /// The RVA of the section in the mapped image.
    pub virtual_address: u32,
    /// The size of the section's data in the file.
    pub raw_size: u32,
    /// The file offset of the section's data.
    pub raw_offset: u32,
    pub characteristics: u32,
}

impl Section {
    pub const SIZE: usize = 40;
    pub const CNT_CODE: u32 = 0x0000_0020;
    pub const CNT_INITIALIZED_DATA: u32 = 0x0000_0040;
    pub const CNT_UNINITIALIZED_DATA: u32 = 0x0000_0080;
    pub const MEM_DISCARDABLE: u32 = 0x0200_0000;
    pub const MEM_EXECUTE: u32 = 0x2000_0000;
    pub const MEM_READ: u32 = 0x4000_0000;
    pub const MEM_WRITE: u32 = 0x8000_0000;

    pub(crate) fn parse(c: &mut Cursor<'_>, index: usize) -> Result<Self> {
        let what = |field: &str| format!("section {index} {field}");
        let name = c.fixed_str(8, &what("Name"))?;
        let virtual_size = c.u32(&what("VirtualSize"))?;
        let virtual_address = c.u32(&what("VirtualAddress"))?;
        let raw_size = c.u32(&what("SizeOfRawData"))?;
        let raw_offset = c.u32(&what("PointerToRawData"))?;
        c.skip(12); // relocations and line numbers: none in an image
        let characteristics = c.u32(&what("Characteristics"))?;
        Ok(Self { name, virtual_size, virtual_address, raw_size, raw_offset, characteristics })
    }

    /// The size of the section in memory: its virtual size, or its raw size when the header leaves
    /// the virtual size zero, as some linkers do for an object-like section.
    pub fn mapped_size(&self) -> u32 {
        if self.virtual_size == 0 { self.raw_size } else { self.virtual_size }
    }

    /// The first RVA after the section.
    pub fn end_rva(&self) -> u32 {
        self.virtual_address.saturating_add(self.mapped_size())
    }

    pub fn contains_rva(&self, rva: u32) -> bool {
        rva >= self.virtual_address && rva < self.end_rva()
    }

    pub fn is_code(&self) -> bool {
        self.characteristics & Self::CNT_CODE != 0
    }

    pub fn is_executable(&self) -> bool {
        self.characteristics & Self::MEM_EXECUTE != 0
    }

    pub fn is_writable(&self) -> bool {
        self.characteristics & Self::MEM_WRITE != 0
    }

    pub fn is_readable(&self) -> bool {
        self.characteristics & Self::MEM_READ != 0
    }

    pub fn is_uninitialized(&self) -> bool {
        self.characteristics & Self::CNT_UNINITIALIZED_DATA != 0
    }

    /// The section's flags as `r`, `w`, `x` and `c` (code) letters, for a listing.
    pub fn flags(&self) -> String {
        let mut s = String::with_capacity(4);
        s.push(if self.is_readable() { 'r' } else { '-' });
        s.push(if self.is_writable() { 'w' } else { '-' });
        s.push(if self.is_executable() { 'x' } else { '-' });
        s.push(if self.is_code() { 'c' } else { '-' });
        s
    }
}
