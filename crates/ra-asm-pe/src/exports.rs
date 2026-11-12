//! The export directory: the functions and data the image offers by name or ordinal, and the
//! forwarders that name a function of another module.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// One export.
#[derive(Debug, Clone, Serialize)]
pub struct Export {
    /// The name, or `None` for an export by ordinal only.
    pub name: Option<String>,
    pub ordinal: u32,
    /// The virtual address of the export, or `None` for a forwarder.
    pub va: Option<u64>,
    /// `OTHERDLL.func` or `OTHERDLL.#12` for a forwarder.
    pub forwarder: Option<String>,
}

/// The export directory of an image.
#[derive(Debug, Clone, Serialize)]
pub struct ExportTable {
    /// The name of the module as the linker wrote it.
    pub dll_name: String,
    pub ordinal_base: u32,
    pub timestamp: u32,
    pub exports: Vec<Export>,
}

impl ExportTable {
    pub fn by_name(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|e| e.name.as_deref() == Some(name))
    }
}

pub(crate) fn parse(image: &Image) -> Result<Option<ExportTable>> {
    let Some((va, size)) = image.directory_va(Directory::Export) else { return Ok(None) };
    let (dir_start, dir_end) = (va, va + u64::from(size));
    let timestamp = image.u32(va + 4).context("export directory TimeDateStamp")?;
    let name_rva = image.u32(va + 12)?;
    let ordinal_base = image.u32(va + 16)?;
    let function_count = image.u32(va + 20)?;
    let name_count = image.u32(va + 24)?;
    let functions_va = image.va(image.u32(va + 28)?);
    let names_va = image.va(image.u32(va + 32)?);
    let ordinals_va = image.va(image.u32(va + 36)?);
    let dll_name = if name_rva == 0 {
        String::new()
    } else {
        image.cstr(image.va(name_rva), 256).context("the export directory's DLL name")?.to_owned()
    };

    // The address table by index; the name table says which indexes have names. A count the
    // image cannot hold is a refusal, not an allocation.
    ensure!(
        u64::from(function_count) * 4 <= u64::from(image.size_of_image()),
        "the export directory at {va:#x} counts {function_count} functions, more than the image holds"
    );
    let mut names: Vec<Option<String>> = vec![None; function_count as usize];
    for i in 0..name_count {
        let name_ptr_va = names_va + u64::from(i) * 4;
        let name = image.cstr(image.va(image.u32(name_ptr_va)?), 4096).with_context(|| format!("export name {i}"))?;
        let index = image.u16(ordinals_va + u64::from(i) * 2).with_context(|| format!("export ordinal {i}"))?;
        if let Some(slot) = names.get_mut(index as usize) {
            // The first name an index gets is the one the listing shows; a second name is an alias.
            if slot.is_none() {
                *slot = Some(name.to_owned());
            }
        }
    }

    let mut exports = Vec::with_capacity(function_count as usize);
    for (index, name) in names.into_iter().enumerate() {
        let rva = image.u32(functions_va + index as u64 * 4).with_context(|| format!("export address {index}"))?;
        if rva == 0 && name.is_none() {
            continue; // an unused ordinal
        }
        let target = image.va(rva);
        let forwarder = (target >= dir_start && target < dir_end).then(|| image.cstr(target, 4096).map(str::to_owned)).transpose()?;
        exports.push(Export {
            name,
            ordinal: ordinal_base.wrapping_add(index as u32),
            va: if forwarder.is_some() { None } else { Some(target) },
            forwarder,
        });
    }
    Ok(Some(ExportTable { dll_name, ordinal_base, timestamp, exports }))
}
