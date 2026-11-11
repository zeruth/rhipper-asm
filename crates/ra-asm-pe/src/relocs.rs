//! The base relocation directory: every place the loader patches when the image does not load at
//! its preferred base. In 32-bit code these are the absolute addresses, so they say where the
//! pointers are.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// The kind of a base relocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocKind {
    /// A 32-bit absolute address.
    HighLow,
    /// A 64-bit absolute address.
    Dir64,
    /// A kind the pipeline does not patch. The raw type is kept.
    Other(u8),
}

/// One relocation: the address of a patched field.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Relocation {
    pub va: u64,
    pub kind: RelocKind,
}

pub(crate) fn parse(image: &Image) -> Result<Vec<Relocation>> {
    let Some((va, size)) = image.directory_va(Directory::BaseReloc) else { return Ok(Vec::new()) };
    let bytes = image.read(va, size as usize).context("the base relocation directory")?;
    let reader = ra_asm_format::Reader::new(bytes);
    let mut relocations = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= bytes.len() {
        let page_rva = reader.u32(offset, "relocation block page")?;
        let block_size = reader.u32(offset + 4, "relocation block size")? as usize;
        if block_size < 8 {
            anyhow::bail!("relocation block at offset {offset:#x} of the directory has size {block_size}, less than its header");
        }
        let entries = (block_size - 8) / 2;
        for i in 0..entries {
            let entry = reader.u16(offset + 8 + i * 2, "relocation entry")?;
            let kind = (entry >> 12) as u8;
            let target = image.va(page_rva + u32::from(entry & 0x0fff));
            let kind = match kind {
                0 => continue, // padding
                3 => RelocKind::HighLow,
                10 => RelocKind::Dir64,
                other => RelocKind::Other(other),
            };
            relocations.push(Relocation { va: target, kind });
        }
        offset += block_size;
    }
    relocations.sort_by_key(|r| r.va);
    Ok(relocations)
}
