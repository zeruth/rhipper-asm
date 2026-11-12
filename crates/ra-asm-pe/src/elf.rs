//! An ELF executable or shared object as the same [`Image`] a PE makes. The loaded segments decide
//! the base (the lowest one, down to its page) and the size; the allocated sections are the
//! sections, else the loaded segments stand for them; the slots the dynamic loader binds to an
//! undefined symbol (`GLOB_DAT`, `JUMP_SLOT`) are the imports, each slot an IAT slot, from the
//! library its version names; the visible symbols the dynamic table defines are the exports; the
//! relocations of an address are the base relocations; and the functions the symbol table names
//! are the symbols a PDB would name. No PE directory is present, so the PE's own tables stay empty.

use anyhow::{Result, bail, ensure};
use ra_asm_format::elf::{self as format, ElfFile, reloc};

use crate::exports::{Export, ExportTable};
use crate::headers::{CoffHeader, DataDirectory, Headers, Machine, OptionalHeader, OptionalMagic};
use crate::image::Image;
use crate::imports::Import;
use crate::pdb::Symbol;
use crate::relocs::{RelocKind, Relocation};
use crate::section::Section;

const PAGE: u64 = 0x1000;

/// The headers an ELF file stands for, its sections, and the file as read.
pub(crate) fn parse(bytes: &[u8]) -> Result<(Headers, Vec<Section>, ElfFile)> {
    let mut elf = ElfFile::parse(bytes)?;
    let loads: Vec<format::Segment> = elf.segments.iter().filter(|s| s.is_load() && s.memsz != 0).copied().collect();
    ensure!(!loads.is_empty(), "the ELF file has no loaded segment");
    let base = loads.iter().map(|s| s.vaddr).min().unwrap_or(0) & !(PAGE - 1);
    let end = loads.iter().map(|s| s.vaddr.saturating_add(s.memsz)).max().unwrap_or(0);
    let size = end.checked_add(PAGE - 1).map(|e| (e & !(PAGE - 1)) - base).and_then(|s| u32::try_from(s).ok());
    let Some(size_of_image) = size else { bail!("the loaded segments span {base:#x}..{end:#x}, more than 4 GiB") };
    let rva_of = |va: u64| va.checked_sub(base).and_then(|r| u32::try_from(r).ok()).filter(|&r| r < size_of_image);

    let mut sections = Vec::new();
    for s in elf.sections.iter().filter(|s| s.is_mapped()) {
        let (Some(virtual_address), Ok(virtual_size)) = (rva_of(s.addr), u32::try_from(s.size)) else {
            elf.warnings.push(format!("section {} at {:#x} ({:#x} bytes) lies outside the loaded segments", s.name, s.addr, s.size));
            continue;
        };
        let zeros = s.kind == format::SHT_NOBITS;
        sections.push(Section {
            name: s.name.clone(),
            virtual_size,
            virtual_address,
            raw_size: if zeros { 0 } else { virtual_size },
            raw_offset: if zeros { 0 } else { u32::try_from(s.offset).unwrap_or(u32::MAX) },
            characteristics: characteristics(s.flags & format::SHF_EXECINSTR != 0, s.flags & format::SHF_WRITE != 0, zeros),
        });
    }
    // A file stripped of its section headers still maps: each loaded segment is a section.
    if sections.is_empty() {
        for (i, s) in loads.iter().enumerate() {
            let (Some(virtual_address), Ok(virtual_size), Ok(raw_size)) =
                (rva_of(s.vaddr), u32::try_from(s.memsz), u32::try_from(s.filesz))
            else {
                elf.warnings.push(format!("loaded segment {i} at {:#x} is larger than the image", s.vaddr));
                continue;
            };
            sections.push(Section {
                name: format!("segment{i}"),
                virtual_size,
                virtual_address,
                raw_size,
                raw_offset: u32::try_from(s.offset).unwrap_or(u32::MAX),
                characteristics: characteristics(s.flags & format::PF_X != 0, s.flags & format::PF_W != 0, s.filesz == 0),
            });
        }
    }

    let entry_point = match (elf.entry, rva_of(elf.entry)) {
        (0, _) => 0,
        (_, Some(rva)) => rva,
        (va, None) => {
            elf.warnings.push(format!("the entry point {va:#x} lies outside the loaded segments"));
            0
        }
    };
    // The headers map when the first loaded segment starts the file at the base, as a linker lays it.
    let size_of_headers = loads
        .iter()
        .min_by_key(|s| s.vaddr)
        .filter(|s| s.offset == 0 && s.vaddr == base)
        .map_or(0, |s| u32::try_from(elf.headers_size.min(s.filesz)).unwrap_or(0));
    let code: Vec<&Section> = sections.iter().filter(|s| s.is_code()).collect();
    let sum = |keep: &dyn Fn(&Section) -> bool| sections.iter().filter(|s| keep(s)).map(|s| s.virtual_size).fold(0u32, u32::saturating_add);
    let shared = elf.kind == format::ET_DYN;
    let library = shared && elf.interpreter.is_none();
    let machine = match elf.machine {
        format::EM_386 => Machine::I386,
        format::EM_X86_64 => Machine::Amd64,
        format::EM_AARCH64 => Machine::Arm64,
        other => Machine::Other(other),
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
                | if elf.wide { CoffHeader::LARGE_ADDRESS_AWARE } else { 0 }
                | if library { CoffHeader::DLL } else { 0 }
                | if shared { 0 } else { CoffHeader::RELOCS_STRIPPED },
        },
        optional: OptionalHeader {
            magic: if elf.wide { OptionalMagic::Pe32Plus } else { OptionalMagic::Pe32 },
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
            dll_characteristics: if shared { OptionalHeader::DYNAMIC_BASE } else { 0 },
            stack_reserve: 0,
            stack_commit: 0,
            heap_reserve: 0,
            heap_commit: 0,
            directories: vec![DataDirectory::default(); 16],
        },
        section_table_offset: 0,
        rich: None,
    };
    Ok((headers, sections, elf))
}

/// The PE characteristics of a section or a segment of these permissions.
pub(crate) fn characteristics(execute: bool, write: bool, zeros: bool) -> u32 {
    let content = match (execute, zeros) {
        (true, _) => Section::CNT_CODE | Section::MEM_EXECUTE,
        (false, true) => Section::CNT_UNINITIALIZED_DATA,
        (false, false) => Section::CNT_INITIALIZED_DATA,
    };
    content | Section::MEM_READ | if write { Section::MEM_WRITE } else { 0 }
}

/// Fills the tables of an ELF image from the file it was read from: imports, exports, relocations,
/// symbols and the reader's warnings.
pub(crate) fn read_tables(image: &mut Image) {
    let Some(elf) = image.elf.as_ref() else { return };
    let (imports, exports, relocations, symbols, warnings) =
        (imports(elf), exports(elf), relocations(elf), symbols(elf), elf.warnings.clone());
    image.imports = imports;
    image.exports = exports;
    image.relocations = relocations;
    image.symbols = symbols;
    image.warnings.extend(warnings);
    apply_addends(image);
}

/// Fills the words the loader computes from a RELA record: a `RELATIVE` word gets its addend, an
/// absolute word the address of the symbol of this file it names plus the addend. A linker leaves
/// those words zero in a RELA file (the addend is in the record), so without this a pointer in the
/// data reads as null. The image is mapped at its linked addresses, where the load bias is zero.
/// A `RELATIVE` record of no addend (a packed `DT_RELR` one, whose addend is the word) keeps the word.
fn apply_addends(image: &mut Image) {
    let Some(elf) = image.elf.as_ref() else { return };
    let words: Vec<(u64, u64)> = elf
        .relocations
        .iter()
        .filter_map(|r| {
            let value = match (elf.machine, r.kind) {
                (format::EM_X86_64, reloc::X86_64_RELATIVE) | (format::EM_AARCH64, reloc::AARCH64_RELATIVE) if r.addend != 0 => {
                    r.addend as u64
                }
                (format::EM_X86_64, reloc::X86_64_64) | (format::EM_AARCH64, reloc::AARCH64_ABS64) => {
                    let symbol =
                        if r.symbol == 0 { 0 } else { elf.dynamic_symbols.get(r.symbol as usize).filter(|s| s.is_defined())?.value };
                    symbol.wrapping_add(r.addend as u64)
                }
                _ => return None,
            };
            Some((r.offset, value))
        })
        .collect();
    for (va, value) in words {
        if let Err(e) = image.write_pointer(va, value) {
            image.warnings.push(format!("the relocation at {va:#x}: {e:#}"));
        }
    }
}

/// Whether a relocation of `kind` binds a slot to a symbol of another module: the GOT slot the
/// code loads an address from, or the one a PLT stub jumps through.
fn binds_slot(machine: u16, kind: u32) -> bool {
    match machine {
        format::EM_X86_64 => matches!(kind, reloc::X86_64_GLOB_DAT | reloc::X86_64_JUMP_SLOT),
        format::EM_386 => matches!(kind, reloc::I386_GLOB_DAT | reloc::I386_JMP_SLOT),
        format::EM_AARCH64 => matches!(kind, reloc::AARCH64_GLOB_DAT | reloc::AARCH64_JUMP_SLOT),
        _ => false,
    }
}

/// Whether a relocation of `kind` copies a library's data into the image.
fn copies(machine: u16, kind: u32) -> bool {
    match machine {
        format::EM_X86_64 => kind == reloc::X86_64_COPY,
        format::EM_386 => kind == reloc::I386_COPY,
        format::EM_AARCH64 => kind == reloc::AARCH64_COPY,
        _ => false,
    }
}

/// Every slot bound to an undefined dynamic symbol, in address order. The library is the one the
/// symbol's version names, else the only library the file needs; with several and no version the
/// library is unknown and the name stays empty.
fn imports(elf: &ElfFile) -> Vec<Import> {
    let sole = if let [only] = elf.needed.as_slice() { Some(only.clone()) } else { None };
    let mut imports: Vec<Import> = elf
        .relocations
        .iter()
        .filter(|r| r.symbol != 0 && binds_slot(elf.machine, r.kind))
        .filter_map(|r| {
            let symbol = elf.dynamic_symbols.get(r.symbol as usize).filter(|s| !s.is_defined() && !s.name.is_empty())?;
            Some(Import {
                dll: symbol.library.clone().or_else(|| sole.clone()).unwrap_or_default(),
                name: Some(symbol.name.clone()),
                hint: 0,
                ordinal: None,
                iat_va: r.offset,
                delayed: false,
            })
        })
        .collect();
    imports.sort_by_key(|i| i.iat_va);
    imports.dedup_by_key(|i| i.iat_va);
    imports
}

/// The functions and data the dynamic table defines for another module to bind to, by their
/// index in the table; `None` when there are none and the file names no `DT_SONAME`.
fn exports(elf: &ElfFile) -> Option<ExportTable> {
    let exports: Vec<Export> = elf
        .dynamic_symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_defined() && s.is_visible() && !s.name.is_empty() && (s.is_function() || s.kind == format::STT_OBJECT))
        .map(|(i, s)| Export {
            name: Some(s.name.clone()),
            ordinal: u32::try_from(i).unwrap_or(u32::MAX),
            va: Some(s.value),
            forwarder: None,
        })
        .collect();
    (!exports.is_empty() || elf.soname.is_some()).then(|| ExportTable {
        dll_name: elf.soname.clone().unwrap_or_default(),
        ordinal_base: 0,
        timestamp: 0,
        exports,
    })
}

/// The dynamic relocations that write an address, as the base relocations of a PE: a word the
/// image's own base or a symbol's address fills. The bindings of the imports and the copies of
/// a library's data are not among them.
fn relocations(elf: &ElfFile) -> Vec<Relocation> {
    let mut relocations: Vec<Relocation> = elf
        .relocations
        .iter()
        .filter(|r| !binds_slot(elf.machine, r.kind) && !copies(elf.machine, r.kind))
        .map(|r| {
            let kind = match (elf.machine, r.kind) {
                (format::EM_X86_64, reloc::X86_64_64 | reloc::X86_64_RELATIVE)
                | (format::EM_AARCH64, reloc::AARCH64_ABS64 | reloc::AARCH64_RELATIVE) => RelocKind::Dir64,
                (format::EM_386, reloc::I386_32 | reloc::I386_RELATIVE) => RelocKind::HighLow,
                (_, kind) => RelocKind::Other(u8::try_from(kind).unwrap_or(u8::MAX)),
            };
            Relocation { va: r.offset, kind }
        })
        .collect();
    relocations.sort_by_key(|r| r.va);
    relocations
}

/// The functions the symbol table defines, else those the dynamic table does, in address order,
/// one name per address: the static table's first.
fn symbols(elf: &ElfFile) -> Vec<Symbol> {
    let mut symbols: Vec<Symbol> = elf
        .symbols
        .iter()
        .chain(&elf.dynamic_symbols)
        .filter(|s| s.is_defined() && s.is_function() && s.value != 0 && !s.name.is_empty())
        .map(|s| Symbol { va: s.value, name: s.name.clone(), library: None, object: None })
        .collect();
    symbols.sort_by_key(|s| s.va);
    symbols.dedup_by_key(|s| s.va);
    symbols
}
