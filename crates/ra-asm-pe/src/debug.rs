//! The debug directory. The `CodeView` record names the PDB of the build, which the pipeline
//! reports and a later step may read for names.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// The `CodeView` `RSDS` record: the PDB that matches the image.
#[derive(Debug, Clone, Serialize)]
pub struct CodeView {
    /// The GUID in its usual text form.
    pub guid: String,
    pub age: u32,
    pub pdb_path: String,
}

/// One entry of the debug directory.
#[derive(Debug, Clone, Serialize)]
pub struct DebugEntry {
    /// The `IMAGE_DEBUG_TYPE_*` value: 2 is `CodeView`.
    pub kind: u32,
    pub timestamp: u32,
    pub size: u32,
    /// The RVA of the data, or zero when the data is only in the file.
    pub rva: u32,
    pub file_offset: u32,
    pub codeview: Option<CodeView>,
}

impl DebugEntry {
    pub const CODEVIEW: u32 = 2;
    pub const REPRO: u32 = 16;

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            1 => "coff",
            2 => "codeview",
            3 => "fpo",
            4 => "misc",
            9 => "borland",
            12 => "vc_feature",
            13 => "pogo",
            14 => "iltcg",
            16 => "repro",
            20 => "ex_dllcharacteristics",
            _ => "other",
        }
    }
}

pub(crate) fn parse(image: &Image) -> Result<Vec<DebugEntry>> {
    let Some((va, size)) = image.directory_va(Directory::Debug) else { return Ok(Vec::new()) };
    let count = u64::from(size) / 28;
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count {
        let entry_va = va + i * 28;
        let kind = image.u32(entry_va + 12).with_context(|| format!("debug directory entry {i}"))?;
        let entry = DebugEntry {
            kind,
            timestamp: image.u32(entry_va + 4)?,
            size: image.u32(entry_va + 16)?,
            rva: image.u32(entry_va + 20)?,
            file_offset: image.u32(entry_va + 24)?,
            codeview: None,
        };
        let codeview = if kind == DebugEntry::CODEVIEW {
            read_codeview(image, &entry).with_context(|| format!("the CodeView record of debug entry {i}"))?
        } else {
            None
        };
        entries.push(DebugEntry { codeview, ..entry });
    }
    Ok(entries)
}

fn read_codeview(image: &Image, entry: &DebugEntry) -> Result<Option<CodeView>> {
    // The data is usually mapped; it is only in the file when the section was discarded.
    let bytes: &[u8] = if entry.rva != 0 && image.is_mapped(image.va(entry.rva)) {
        image.read(image.va(entry.rva), entry.size as usize)?
    } else {
        let start = entry.file_offset as usize;
        let end = start.checked_add(entry.size as usize).filter(|&end| end <= image.file().len());
        let Some(end) = end else {
            anyhow::bail!("the record at file offset {start:#x} ({:#x} bytes) is past the end of the file", entry.size)
        };
        &image.file()[start..end]
    };
    if bytes.len() < 24 || &bytes[..4] != b"RSDS" {
        return Ok(None); // NB10 and other forms are not PDB 7.0 records
    }
    let reader = ra_asm_format::Reader::new(bytes);
    let g = reader.slice(4, 16, "the GUID")?;
    let guid = format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
        u16::from_le_bytes([g[4], g[5]]),
        u16::from_le_bytes([g[6], g[7]]),
        g[8],
        g[9],
        g[10],
        g[11],
        g[12],
        g[13],
        g[14],
        g[15]
    );
    let age = reader.u32(20, "the age")?;
    let pdb_path = reader.cstr(24, bytes.len() - 24, "the PDB path")?.to_owned();
    Ok(Some(CodeView { guid, age, pdb_path }))
}
