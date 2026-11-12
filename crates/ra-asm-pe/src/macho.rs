//! A 64-bit Mach-O executable or library as the same [`Image`] a PE makes. The segments that map
//! (all but `__PAGEZERO`, which only reserves the low 4 GiB) decide the base and the size; the
//! sections are the sections, each named `segment,section`; the pointers dyld binds to another
//! image's symbol are the imports, each pointer an IAT slot, from the library its two-level
//! ordinal names; the export trie's symbols are the exports; the rebases are the base relocations;
//! and the functions the symbol table defines are the symbols a PDB would name. A C name keeps no
//! leading underscore here, since the source called `_main` `main`. No PE directory is present.

use anyhow::{Result, bail, ensure};
use ra_asm_format::macho::{self as format, MachOFile};

use crate::exports::{Export, ExportTable};
use crate::headers::{CoffHeader, DataDirectory, Headers, Machine, OptionalHeader, OptionalMagic};
use crate::image::Image;
use crate::imports::Import;
use crate::pdb::Symbol;
use crate::relocs::{RelocKind, Relocation};
use crate::section::Section;

const PAGE: u64 = 0x1000;

/// The headers a Mach-O file stands for, its sections, and the file as read.
pub(crate) fn parse(bytes: &[u8]) -> Result<(Headers, Vec<Section>, MachOFile)> {
    let mut macho = MachOFile::parse(bytes)?;
    let mapped: Vec<format::Segment> = macho.segments.iter().filter(|s| s.vmsize != 0 && s.initprot != 0).cloned().collect();
    ensure!(!mapped.is_empty(), "the Mach-O file maps no segment");
    let base = mapped.iter().map(|s| s.vmaddr).min().unwrap_or(0) & !(PAGE - 1);
    let end = mapped.iter().map(|s| s.vmaddr.saturating_add(s.vmsize)).max().unwrap_or(0);
    let size = end.checked_add(PAGE - 1).map(|e| (e & !(PAGE - 1)) - base).and_then(|s| u32::try_from(s).ok());
    let Some(size_of_image) = size else { bail!("the segments span {base:#x}..{end:#x}, more than 4 GiB") };
    let rva_of = |va: u64| va.checked_sub(base).and_then(|r| u32::try_from(r).ok()).filter(|&r| r < size_of_image);
    let writable = |segment: &str| macho.segments.iter().any(|s| s.name == segment && s.initprot & format::VM_PROT_WRITE != 0);

    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for s in macho.sections.iter().filter(|s| s.size != 0) {
        let name = format!("{},{}", s.segment, s.name);
        let (Some(virtual_address), Ok(virtual_size)) = (rva_of(s.addr), u32::try_from(s.size)) else {
            warnings.push(format!("section {name} at {:#x} ({:#x} bytes) lies outside the segments", s.addr, s.size));
            continue;
        };
        let zeros = s.is_zerofill();
        sections.push(Section {
            name,
            virtual_size,
            virtual_address,
            raw_size: if zeros { 0 } else { virtual_size },
            raw_offset: if zeros { 0 } else { s.offset },
            characteristics: crate::elf::characteristics(s.has_instructions(), writable(&s.segment), zeros),
        });
    }
    // A file of segments without sections still maps: each segment is a section.
    if sections.is_empty() {
        for s in &mapped {
            let (Some(virtual_address), Ok(virtual_size), Ok(raw_size), Ok(raw_offset)) =
                (rva_of(s.vmaddr), u32::try_from(s.vmsize), u32::try_from(s.filesize), u32::try_from(s.fileoff))
            else {
                warnings.push(format!("segment {} at {:#x} is larger than the image", s.name, s.vmaddr));
                continue;
            };
            let execute = s.initprot & format::VM_PROT_EXECUTE != 0;
            let write = s.initprot & format::VM_PROT_WRITE != 0;
            sections.push(Section {
                name: s.name.clone(),
                virtual_size,
                virtual_address,
                raw_size,
                raw_offset,
                characteristics: crate::elf::characteristics(execute, write, s.filesize == 0),
            });
        }
    }

    let entry_point = match macho.entry.map(|va| (va, rva_of(va))) {
        None => 0,
        Some((_, Some(rva))) => rva,
        Some((va, None)) => {
            warnings.push(format!("the entry point {va:#x} lies outside the segments"));
            0
        }
    };
    macho.warnings.extend(warnings);
    // The headers map when the segment that holds the start of the file starts the image.
    let size_of_headers = mapped
        .iter()
        .find(|s| s.fileoff == 0 && s.filesize != 0 && s.vmaddr == base)
        .map_or(0, |s| u32::try_from(macho.headers_size.min(s.filesize)).unwrap_or(0));
    let code: Vec<&Section> = sections.iter().filter(|s| s.is_code()).collect();
    let sum = |keep: &dyn Fn(&Section) -> bool| sections.iter().filter(|s| keep(s)).map(|s| s.virtual_size).fold(0u32, u32::saturating_add);
    let library = matches!(macho.kind, format::MH_DYLIB | format::MH_BUNDLE);
    let relocatable = library || macho.flags & format::MH_PIE != 0;
    let machine = match macho.cpu_type {
        format::CPU_TYPE_X86_64 => Machine::Amd64,
        format::CPU_TYPE_ARM64 => Machine::Arm64,
        other => Machine::Other((other & 0xffff) as u16),
    };
    let headers = Headers {
        pe_offset: 0,
        coff: CoffHeader {
            machine,
            sections: u16::try_from(sections.len()).unwrap_or(u16::MAX),
            timestamp: 0,
            symbol_table: 0,
            symbols: 0,
            optional_header_size: 0,
            characteristics: CoffHeader::EXECUTABLE_IMAGE
                | CoffHeader::LARGE_ADDRESS_AWARE
                | if library { CoffHeader::DLL } else { 0 }
                | if relocatable { 0 } else { CoffHeader::RELOCS_STRIPPED },
        },
        optional: OptionalHeader {
            magic: OptionalMagic::Pe32Plus,
            linker_version: (0, 0),
            size_of_code: sum(&|s: &Section| s.is_code()),
            size_of_initialized_data: sum(&|s: &Section| !s.is_code() && !s.is_uninitialized()),
            size_of_uninitialized_data: sum(&|s: &Section| s.is_uninitialized()),
            entry_point,
            base_of_code: code.first().map_or(0, |s| s.virtual_address),
            image_base: base,
            section_alignment: u32::try_from(PAGE).unwrap_or(0),
            file_alignment: 1,
            os_version: (0, 0),
            image_version: (0, 0),
            subsystem_version: (0, 0),
            size_of_image,
            size_of_headers,
            checksum: 0,
            subsystem: 0,
            dll_characteristics: if relocatable { OptionalHeader::DYNAMIC_BASE } else { 0 },
            stack_reserve: 0,
            stack_commit: 0,
            heap_reserve: 0,
            heap_commit: 0,
            directories: vec![DataDirectory::default(); 16],
        },
        section_table_offset: 0,
        rich: None,
    };
    Ok((headers, sections, macho))
}

/// The name the source gave a symbol: a C name without the underscore the compiler prefixed.
fn source_name(name: &str) -> String {
    name.strip_prefix('_').unwrap_or(name).to_owned()
}

/// Fills the tables of a Mach-O image from the file it was read from: imports, exports,
/// relocations, symbols and the reader's warnings.
pub(crate) fn read_tables(image: &mut Image) {
    let Some(macho) = image.macho.as_ref() else { return };
    let (imports, exports, relocations, symbols, warnings) =
        (imports(macho), exports(macho), relocations(macho), symbols(macho), macho.warnings.clone());
    image.imports = imports;
    image.exports = exports;
    image.relocations = relocations;
    image.symbols = symbols;
    image.warnings.extend(warnings);
}

/// Every pointer bound to another image's symbol, in address order, from the library its ordinal
/// names; a flat or a weak lookup leaves the library unknown and the name empty.
fn imports(macho: &MachOFile) -> Vec<Import> {
    let mut imports: Vec<Import> = macho
        .bindings
        .iter()
        .filter(|b| !b.name.is_empty())
        .map(|b| Import {
            dll: b.library.clone().unwrap_or_default(),
            name: Some(source_name(&b.name)),
            hint: 0,
            ordinal: None,
            iat_va: b.address,
            delayed: false,
        })
        .collect();
    imports.sort_by_key(|i| i.iat_va);
    imports.dedup_by_key(|i| i.iat_va);
    imports
}

/// The symbols of the export trie, by their index in address order; `None` when there are none
/// and the file names no install name of its own.
fn exports(macho: &MachOFile) -> Option<ExportTable> {
    let exports: Vec<Export> = macho
        .exports
        .iter()
        .enumerate()
        .map(|(i, e)| Export {
            name: Some(source_name(&e.name)),
            ordinal: u32::try_from(i).unwrap_or(u32::MAX),
            va: Some(e.address),
            forwarder: None,
        })
        .collect();
    (!exports.is_empty() || macho.install_name.is_some()).then(|| ExportTable {
        dll_name: macho.install_name.clone().unwrap_or_default(),
        ordinal_base: 0,
        timestamp: 0,
        exports,
    })
}

/// The rebases, as the base relocations of a PE: each a pointer the slide of the image moves.
fn relocations(macho: &MachOFile) -> Vec<Relocation> {
    macho.rebases.iter().map(|&va| Relocation { va, kind: RelocKind::Dir64 }).collect()
}

/// The functions the symbol table defines in a section of instructions, in address order, one
/// name per address: the first the table lists.
fn symbols(macho: &MachOFile) -> Vec<Symbol> {
    let mut symbols: Vec<Symbol> = macho
        .symbols
        .iter()
        .filter(|s| s.is_defined() && !s.name.is_empty())
        .filter(|s| {
            let section = macho.sections.get(usize::from(s.section) - 1);
            section.is_some_and(|section| section.has_instructions() && section.contains(s.value))
        })
        .map(|s| Symbol { va: s.value, name: source_name(&s.name), library: None, object: None })
        .collect();
    symbols.sort_by_key(|s| s.va);
    symbols.dedup_by_key(|s| s.va);
    symbols
}
