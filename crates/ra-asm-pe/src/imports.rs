//! The import directory and the delay-load import directory: every function the image takes from
//! another module, with the IAT slot the code calls through.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// One imported function.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub dll: String,
    /// The name, or `None` for an import by ordinal.
    pub name: Option<String>,
    pub hint: u16,
    /// The ordinal of an import by ordinal.
    pub ordinal: Option<u16>,
    /// The virtual address of the IAT slot: `call [slot]` calls the import.
    pub iat_va: u64,
    /// Whether the import comes from the delay-load directory.
    pub delayed: bool,
}

impl Import {
    /// The name as the code sees it: the name, or `dll!#ordinal`.
    pub fn display_name(&self) -> String {
        match (&self.name, self.ordinal) {
            (Some(name), _) => name.clone(),
            (None, Some(ordinal)) => format!("{}!#{ordinal}", self.dll),
            (None, None) => format!("{}!?", self.dll),
        }
    }
}

pub(crate) fn parse(image: &Image) -> Result<Vec<Import>> {
    let mut imports = Vec::new();
    if let Some((va, _)) = image.directory_va(Directory::Import) {
        parse_import_directory(image, va, &mut imports)?;
    }
    if let Some((va, _)) = image.directory_va(Directory::DelayImport) {
        parse_delay_directory(image, va, &mut imports)?;
    }
    Ok(imports)
}

/// The ordinal flag of a thunk: the top bit of its width.
fn ordinal_flag(image: &Image) -> u64 {
    if image.is_64() { 1 << 63 } else { 1 << 31 }
}

fn parse_import_directory(image: &Image, directory_va: u64, out: &mut Vec<Import>) -> Result<()> {
    const DESCRIPTOR: u64 = 20;
    let mut va = directory_va;
    loop {
        let original_first_thunk = image.u32(va).with_context(|| format!("import descriptor at {va:#x}"))?;
        let name_rva = image.u32(va + 12)?;
        let first_thunk = image.u32(va + 16)?;
        if original_first_thunk == 0 && name_rva == 0 && first_thunk == 0 {
            return Ok(());
        }
        let dll = image.cstr(image.va(name_rva), 256).with_context(|| format!("import descriptor at {va:#x}: the DLL name"))?.to_owned();
        // The lookup table has the names; a bound image may have overwritten the IAT with addresses.
        let lookup = if original_first_thunk != 0 { original_first_thunk } else { first_thunk };
        read_thunks(image, &dll, image.va(lookup), image.va(first_thunk), false, out)?;
        va += DESCRIPTOR;
    }
}

fn parse_delay_directory(image: &Image, directory_va: u64, out: &mut Vec<Import>) -> Result<()> {
    const DESCRIPTOR: u64 = 32;
    let mut va = directory_va;
    loop {
        let attributes = image.u32(va).with_context(|| format!("delay import descriptor at {va:#x}"))?;
        let name = image.u32(va + 4)?;
        let iat = image.u32(va + 12)?;
        let names = image.u32(va + 16)?;
        if name == 0 && iat == 0 && names == 0 {
            return Ok(());
        }
        // Attribute bit 0 says the fields are RVAs; an older descriptor holds virtual addresses.
        let rva_based = attributes & 1 != 0;
        let to_va = |field: u32| -> u64 { if rva_based { image.va(field) } else { u64::from(field) } };
        let dll = image.cstr(to_va(name), 256).with_context(|| format!("delay import descriptor at {va:#x}: the DLL name"))?.to_owned();
        read_thunks(image, &dll, to_va(names), to_va(iat), true, out)?;
        va += DESCRIPTOR;
    }
}

fn read_thunks(image: &Image, dll: &str, lookup_va: u64, iat_va: u64, delayed: bool, out: &mut Vec<Import>) -> Result<()> {
    let width = u64::from(image.pointer_size());
    let flag = ordinal_flag(image);
    let mut index = 0u64;
    loop {
        let entry_va = lookup_va + index * width;
        let entry = image.pointer(entry_va).with_context(|| format!("import lookup entry of {dll} at {entry_va:#x}"))?;
        if entry == 0 {
            return Ok(());
        }
        let slot = iat_va + index * width;
        if entry & flag != 0 {
            out.push(Import { dll: dll.to_owned(), name: None, hint: 0, ordinal: Some((entry & 0xffff) as u16), iat_va: slot, delayed });
        } else {
            let hint_name_va = image.va((entry & 0x7fff_ffff) as u32);
            let hint = image.u16(hint_name_va).with_context(|| format!("hint of an import of {dll} at {hint_name_va:#x}"))?;
            let name = image.cstr(hint_name_va + 2, 4096).with_context(|| format!("name of an import of {dll} at {hint_name_va:#x}"))?;
            out.push(Import { dll: dll.to_owned(), name: Some(name.to_owned()), hint, ordinal: None, iat_va: slot, delayed });
        }
        index += 1;
    }
}
