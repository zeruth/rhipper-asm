//! The program an unpacking stub wrote, as a PE file of its own. The run stopped where control
//! entered what it wrote ([`Outcome::unpacked`]); the file is the packed one's headers over the
//! run's memory there: each section holds what the run left in it, and the entry is where the run
//! stopped.
//!
//! The imports are those the stub fixed itself: an import slot of the program is a word of its
//! memory that holds an address the host answered a `GetProcAddress` with, and each run of
//! adjacent slots of one DLL is an import address table. The descriptors and the names of the
//! rebuilt import directory go in a section of their own after the image's, and each slot holds
//! its name's entry again, as the loader wants to find it.
//!
//! The file keeps the packed image's base and has no relocations: the stub applied the program's
//! own (at no delta, the run mapping the image at its base) and left no table of them.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_lift::run::{Outcome, Resolved};
use ra_asm_pe::headers::OptionalHeader;
use ra_asm_pe::{Directory, Image, Section};

/// `IMAGE_FILE_RELOCS_STRIPPED`: the image loads at its base or not at all.
const RELOCS_STRIPPED: u16 = 0x0001;
/// The size of an import descriptor.
const DESCRIPTOR: usize = 20;
/// The name of the section the rebuilt import directory takes.
const IMPORTS: &str = ".idata";

/// The rebuilt import directory: the raw data and the slot-to-lookup-entry pairs.
type ImportDirectory = (Vec<u8>, Vec<(u32, u64)>);

/// The PE file of the program the run of `packed` unpacked, from the `outcome` of its run with
/// [`Options::unpack`](ra_asm_lift::run::Options::unpack).
pub fn image(packed: &Image, outcome: &Outcome) -> Result<Vec<u8>> {
    ensure!(packed.xbe.is_none() && packed.elf.is_none() && packed.macho.is_none(), "the packed image is no PE");
    let Some(entry) = outcome.unpacked else {
        bail!("the run did not stop where control entered what it wrote (stop: {})", outcome.stop.as_deref().unwrap_or("none"));
    };
    let base = packed.image_base();
    let entry_rva = entry.checked_sub(base).and_then(|rva| u32::try_from(rva).ok()).context("the entry lies outside the image")?;
    ensure!(
        outcome.image.len() == packed.size_of_image() as usize,
        "the run kept {:#x} bytes of an image of {:#x}",
        outcome.image.len(),
        packed.size_of_image()
    );
    let optional = &packed.headers.optional;
    let (file_align, section_align) = (optional.file_alignment as usize, optional.section_alignment);
    ensure!(file_align.is_power_of_two() && section_align.is_power_of_two(), "the packed image's alignments are no powers of two");
    let width = usize::from(packed.pointer_size());

    let slots = slots(packed, &outcome.image, &outcome.resolved);
    let named: Vec<(u32, &str)> = slots.iter().map(|&(rva, r)| (rva, r.dll.as_str())).collect();
    let tables = runs(&named, width as u32);
    let end = packed.sections.iter().map(Section::end_rva).max().context("the packed image has no section")?;
    let imports_rva = end.next_multiple_of(section_align);
    let (imports, filled) = import_directory(&slots, &tables, imports_rva, width)?;
    let mut memory = outcome.image.clone();
    for (slot, value) in filled {
        put(&mut memory, slot as usize, width, value);
    }

    let table = packed.headers.section_table_offset;
    let count = packed.sections.len() + usize::from(!tables.is_empty());
    let size_of_headers = (table + count * Section::SIZE).next_multiple_of(file_align);
    let first = packed.sections.iter().map(|s| s.virtual_address).min().unwrap_or(section_align);
    ensure!(size_of_headers <= first as usize, "the headers with {count} sections run into the first section at {first:#x}");
    ensure!(packed.file().len() >= table, "the packed file is shorter than its headers");
    let mut file = vec![0u8; size_of_headers];
    file[..table].copy_from_slice(&packed.file()[..table]);

    let mut headers = Vec::with_capacity(count);
    for section in &packed.sections {
        let start = section.virtual_address as usize;
        let bytes = memory
            .get(start..start + section.mapped_size() as usize)
            .with_context(|| format!("section {} lies past the image", section.name))?;
        let mut characteristics = section.characteristics & !Section::CNT_UNINITIALIZED_DATA;
        characteristics |= if section.contains_rva(entry_rva) { Section::CNT_CODE } else { Section::CNT_INITIALIZED_DATA };
        headers.push(append(&mut file, file_align, &section.name, section.virtual_address, bytes, characteristics));
    }
    let size_of_image = if tables.is_empty() {
        end.next_multiple_of(section_align)
    } else {
        let characteristics = Section::CNT_INITIALIZED_DATA | Section::MEM_READ;
        headers.push(append(&mut file, file_align, IMPORTS, imports_rva, &imports, characteristics));
        (imports_rva + u32::try_from(imports.len())?).next_multiple_of(section_align)
    };
    for (i, header) in headers.iter().enumerate() {
        file[table + i * Section::SIZE..][..Section::SIZE].copy_from_slice(header);
    }

    let pe = packed.headers.pe_offset as usize;
    put(&mut file, pe + 6, 2, count as u64);
    let characteristics = packed.headers.coff.characteristics | RELOCS_STRIPPED;
    put(&mut file, pe + 22, 2, u64::from(characteristics));
    let opt = pe + 24;
    put(&mut file, opt + 16, 4, u64::from(entry_rva));
    put(&mut file, opt + 56, 4, u64::from(size_of_image));
    put(&mut file, opt + 60, 4, size_of_headers as u64);
    put(&mut file, opt + 64, 4, 0);
    let dll = optional.dll_characteristics & !(OptionalHeader::DYNAMIC_BASE | OptionalHeader::HIGH_ENTROPY_VA);
    put(&mut file, opt + 70, 2, u64::from(dll));
    let directories = opt + if optional.is_64() { 112 } else { 96 };
    let import = if tables.is_empty() { (0, 0) } else { (imports_rva, (tables.len() + 1) * DESCRIPTOR) };
    for (index, (rva, size)) in
        [(Directory::Import, import), (Directory::BaseReloc, (0, 0)), (Directory::BoundImport, (0, 0)), (Directory::Iat, (0, 0))]
    {
        let index = index as usize;
        if index < optional.directories.len() {
            put(&mut file, directories + 8 * index, 4, u64::from(rva));
            put(&mut file, directories + 8 * index + 4, 4, size as u64);
        }
    }
    Ok(file)
}

/// The import slots of the program in `memory`, by RVA in order: the words of its sections that
/// hold an address in `resolved`, but the packed image's own import slots, which the loader
/// filled for the stub.
fn slots<'a>(packed: &Image, memory: &[u8], resolved: &'a [Resolved]) -> Vec<(u32, &'a Resolved)> {
    let by_address: HashMap<u64, &Resolved> = resolved.iter().map(|r| (r.address, r)).collect();
    let own: HashSet<u64> = packed.imports.iter().map(|i| i.iat_va).collect();
    let width = usize::from(packed.pointer_size());
    let mut found = Vec::new();
    for section in &packed.sections {
        let end = (section.end_rva() as usize).min(memory.len());
        let mut rva = (section.virtual_address as usize).next_multiple_of(width);
        while rva + width <= end {
            let value = memory[rva..rva + width].iter().rev().fold(0u64, |v, &b| (v << 8) | u64::from(b));
            if let Some(&r) = by_address.get(&value)
                && !own.contains(&(packed.image_base() + rva as u64))
            {
                found.push((rva as u32, r));
            }
            rva += width;
        }
    }
    found.sort_by_key(|&(rva, _)| rva);
    found.dedup_by_key(|&mut (rva, _)| rva);
    found
}

/// The import address tables among `slots` (by RVA in order, each with its DLL), as ranges of
/// them: each a run of adjacent slots of one DLL.
fn runs(slots: &[(u32, &str)], width: u32) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (i, &(rva, dll)) in slots.iter().enumerate() {
        if let Some(last) = runs.last_mut()
            && let Some(&(before, before_dll)) = i.checked_sub(1).map(|j| &slots[j])
            && before + width == rva
            && before_dll.eq_ignore_ascii_case(dll)
        {
            last.end = i + 1;
            continue;
        }
        runs.push(i..i + 1);
    }
    runs
}

/// The import directory of the import address tables `tables` of `slots`, laid out for its
/// section at `rva`: the descriptors, each table's lookup table, the names of the functions and of
/// the DLLs. With it, the value each slot holds in the file: its lookup entry.
fn import_directory(slots: &[(u32, &Resolved)], tables: &[Range<usize>], rva: u32, width: usize) -> Result<ImportDirectory> {
    let ordinal_flag = 1u64 << (8 * width - 1);
    let lookups: usize = tables.iter().map(|t| (t.len() + 1) * width).sum();
    let mut data = vec![0u8; (tables.len() + 1) * DESCRIPTOR + lookups];
    let mut filled = Vec::with_capacity(slots.len());
    let mut lookup = (tables.len() + 1) * DESCRIPTOR;
    let at = |offset: usize| -> Result<u64> { Ok(u64::from(rva + u32::try_from(offset)?)) };
    for (i, table) in tables.iter().enumerate() {
        let first = lookup;
        for &(slot, resolved) in &slots[table.clone()] {
            let entry = if let Some(ordinal) = resolved.name.strip_prefix('#') {
                let ordinal: u16 = ordinal.parse().with_context(|| format!("{} of {} is no ordinal", resolved.name, resolved.dll))?;
                ordinal_flag | u64::from(ordinal)
            } else {
                let name = at(data.len())?;
                data.extend_from_slice(&[0, 0]);
                terminated(&mut data, &resolved.name);
                name
            };
            put(&mut data, lookup, width, entry);
            filled.push((slot, entry));
            lookup += width;
        }
        lookup += width;
        let dll = at(data.len())?;
        terminated(&mut data, &slots[table.start].1.dll);
        let descriptor = i * DESCRIPTOR;
        put(&mut data, descriptor, 4, at(first)?);
        put(&mut data, descriptor + 12, 4, dll);
        put(&mut data, descriptor + 16, 4, u64::from(slots[table.start].0));
    }
    Ok((data, filled))
}

/// Appends `name` with its terminator, padded to an even length: a hint/name entry starts on one.
fn terminated(data: &mut Vec<u8>, name: &str) {
    data.extend_from_slice(name.as_bytes());
    data.push(0);
    if data.len() % 2 == 1 {
        data.push(0);
    }
}

/// Appends a section's `bytes` to `file` at the next file-aligned offset, and gives its header.
fn append(file: &mut Vec<u8>, align: usize, name: &str, rva: u32, bytes: &[u8], characteristics: u32) -> [u8; Section::SIZE] {
    let raw_size = bytes.len().next_multiple_of(align);
    let raw_offset = if raw_size == 0 { 0 } else { file.len() };
    file.extend_from_slice(bytes);
    file.resize(file.len().next_multiple_of(align), 0);
    let mut header = [0u8; Section::SIZE];
    let name = name.as_bytes();
    header[..name.len().min(8)].copy_from_slice(&name[..name.len().min(8)]);
    put(&mut header, 8, 4, bytes.len() as u64);
    put(&mut header, 12, 4, u64::from(rva));
    put(&mut header, 16, 4, raw_size as u64);
    put(&mut header, 20, 4, raw_offset as u64);
    put(&mut header, 36, 4, u64::from(characteristics));
    header
}

/// Writes the low `width` bytes of `value` at `offset`, little-endian.
fn put(bytes: &mut [u8], offset: usize, width: usize, value: u64) {
    bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
}

#[cfg(test)]
mod tests;
