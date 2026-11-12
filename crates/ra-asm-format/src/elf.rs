//! The ELF container of an executable or a shared object, 32 or 64 bits, either byte order: the
//! program headers that map the file by virtual address, the section headers, the symbol table
//! when the file keeps one, the dynamic section (the libraries it needs, its own name, the dynamic
//! symbols with the library each one's version names, the relocations the dynamic loader applies)
//! and the function extents `.eh_frame` records, one FDE per function. What a container of this
//! shape means to the pipeline (an import, an export, a section) is the loader's to say; this
//! reads what the file holds, and every refusal names the field and its offset.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};

use crate::Reader;

pub const MAGIC: &[u8; 4] = b"\x7fELF";

pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;

pub const EM_386: u16 = 3;
pub const EM_ARM: u16 = 40;
pub const EM_X86_64: u16 = 62;
pub const EM_AARCH64: u16 = 183;

pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_GNU_EH_FRAME: u32 = 0x6474_e550;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_DYNSYM: u32 = 11;
pub const SHT_NOBITS: u32 = 8;
pub const SHF_WRITE: u64 = 1;
pub const SHF_ALLOC: u64 = 2;
pub const SHF_EXECINSTR: u64 = 4;
pub const SHF_TLS: u64 = 0x400;

pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_GNU_IFUNC: u8 = 10;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;
/// The first reserved section index: a symbol at or past it is absolute or common, not in a section.
pub const SHN_LORESERVE: u16 = 0xff00;

const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_PLTRELSZ: u64 = 2;
const DT_HASH: u64 = 4;
const DT_STRTAB: u64 = 5;
const DT_SYMTAB: u64 = 6;
const DT_RELA: u64 = 7;
const DT_RELASZ: u64 = 8;
const DT_STRSZ: u64 = 10;
const DT_SYMENT: u64 = 11;
const DT_SONAME: u64 = 14;
const DT_REL: u64 = 17;
const DT_RELSZ: u64 = 18;
const DT_PLTREL: u64 = 20;
const DT_JMPREL: u64 = 23;
const DT_RELRSZ: u64 = 35;
const DT_RELR: u64 = 36;
const DT_VERSYM: u64 = 0x6fff_fff0;
const DT_VERNEED: u64 = 0x6fff_fffe;
const DT_VERNEEDNUM: u64 = 0x6fff_ffff;

/// The bound of the tables the reader walks: a real file is far below it, a corrupt count is not a
/// reason to loop for long.
const MAX_ENTRIES: usize = 1 << 20;

/// The kinds of relocation the loader gives a meaning to, by machine.
pub mod reloc {
    pub const X86_64_64: u32 = 1;
    pub const X86_64_COPY: u32 = 5;
    pub const X86_64_GLOB_DAT: u32 = 6;
    pub const X86_64_JUMP_SLOT: u32 = 7;
    pub const X86_64_RELATIVE: u32 = 8;
    pub const I386_32: u32 = 1;
    pub const I386_COPY: u32 = 5;
    pub const I386_GLOB_DAT: u32 = 6;
    pub const I386_JMP_SLOT: u32 = 7;
    pub const I386_RELATIVE: u32 = 8;
    pub const AARCH64_ABS64: u32 = 257;
    pub const AARCH64_COPY: u32 = 1024;
    pub const AARCH64_GLOB_DAT: u32 = 1025;
    pub const AARCH64_JUMP_SLOT: u32 = 1026;
    pub const AARCH64_RELATIVE: u32 = 1027;

    /// The kind that adds the load bias to the word in place, which a packed `DT_RELR` entry stands for.
    pub fn relative(machine: u16) -> Option<u32> {
        match machine {
            super::EM_X86_64 => Some(X86_64_RELATIVE),
            super::EM_386 => Some(I386_RELATIVE),
            super::EM_AARCH64 => Some(AARCH64_RELATIVE),
            _ => None,
        }
    }
}

/// One program header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

impl Segment {
    pub fn is_load(&self) -> bool {
        self.kind == PT_LOAD
    }
}

/// One section header, with its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHeader {
    pub name: String,
    pub kind: u32,
    pub flags: u64,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub entsize: u64,
}

impl SectionHeader {
    /// Whether the section takes room in the process's memory: allocated, and not the TLS template
    /// of zeros, which each thread gets a copy of and the image itself does not hold.
    pub fn is_mapped(&self) -> bool {
        self.flags & SHF_ALLOC != 0 && self.addr != 0 && self.size != 0 && !(self.kind == SHT_NOBITS && self.flags & SHF_TLS != 0)
    }
}

/// One record of a symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    /// `STT_*`, the low nibble of `st_info`.
    pub kind: u8,
    /// `STB_*`, the high nibble of `st_info`.
    pub binding: u8,
    /// `STV_*`, the low two bits of `st_other`.
    pub visibility: u8,
    /// The index of the section that defines the symbol; zero when it is undefined here.
    pub section: u16,
    /// The library the symbol's version requires it from, for a dynamic symbol the file imports
    /// under a version (`libc.so.6` for `puts@GLIBC_2.2.5`).
    pub library: Option<String>,
}

impl Symbol {
    /// Whether the file defines the symbol in one of its sections.
    pub fn is_defined(&self) -> bool {
        self.section != 0 && self.section < SHN_LORESERVE
    }

    pub fn is_function(&self) -> bool {
        matches!(self.kind, STT_FUNC | STT_GNU_IFUNC)
    }

    /// Whether another module may bind to it: global or weak, and of default or protected visibility.
    pub fn is_visible(&self) -> bool {
        matches!(self.binding, STB_GLOBAL | STB_WEAK) && matches!(self.visibility, 0 | 3)
    }
}

/// One dynamic relocation: the place it patches, its kind, and the dynamic symbol it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relocation {
    pub offset: u64,
    pub kind: u32,
    /// The index of the symbol in the dynamic symbol table; zero for none.
    pub symbol: u32,
    /// The explicit addend of a `RELA` record; zero for a `REL` one, whose addend is in place.
    pub addend: i64,
}

/// What an ELF file holds, as read.
#[derive(Debug, Clone)]
pub struct ElfFile {
    /// `ELFCLASS64`.
    pub wide: bool,
    /// `ELFDATA2MSB`.
    pub big_endian: bool,
    /// `e_type`: [`ET_EXEC`], [`ET_DYN`], ...
    pub kind: u16,
    /// `e_machine`: [`EM_X86_64`], [`EM_386`], ...
    pub machine: u16,
    pub entry: u64,
    pub segments: Vec<Segment>,
    /// The section headers, in the file's order; empty for a file stripped of them.
    pub sections: Vec<SectionHeader>,
    /// The bytes from the start of the file to the end of the program headers.
    pub headers_size: u64,
    /// The static symbol table, `.symtab`, in its order; empty when the file was stripped.
    pub symbols: Vec<Symbol>,
    /// The dynamic symbol table, by index.
    pub dynamic_symbols: Vec<Symbol>,
    /// The program interpreter, `PT_INTERP`: the dynamic loader of an executable.
    pub interpreter: Option<String>,
    /// The libraries the dynamic section needs, `DT_NEEDED`, in its order.
    pub needed: Vec<String>,
    /// The file's own name for the dynamic loader, `DT_SONAME`.
    pub soname: Option<String>,
    /// The relocations the dynamic loader applies: `DT_RELA`, `DT_REL`, `DT_JMPREL` and the
    /// packed `DT_RELR`.
    pub relocations: Vec<Relocation>,
    /// The functions `.eh_frame` describes, one FDE each: `(begin, end)` in address order.
    pub functions: Vec<(u64, u64)>,
    /// What the reader could not read beyond the headers: a table that fails lands here.
    pub warnings: Vec<String>,
}

/// Whether `bytes` open on an ELF file.
pub fn is_elf(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The fixed-width reads of the file in its own byte order and width.
#[derive(Debug, Clone, Copy)]
struct Data<'a> {
    reader: Reader<'a>,
    big: bool,
    wide: bool,
}

impl<'a> Data<'a> {
    fn bytes(&self) -> &'a [u8] {
        self.reader.bytes()
    }

    fn u8(&self, offset: usize, what: &str) -> Result<u8> {
        self.reader.u8(offset, what)
    }

    fn u16(&self, offset: usize, what: &str) -> Result<u16> {
        let v = self.reader.u16(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    fn u32(&self, offset: usize, what: &str) -> Result<u32> {
        let v = self.reader.u32(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    fn u64(&self, offset: usize, what: &str) -> Result<u64> {
        let v = self.reader.u64(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    /// A word of the file's class: eight bytes in a 64-bit file, four in a 32-bit one.
    fn word(&self, offset: usize, what: &str) -> Result<u64> {
        if self.wide { self.u64(offset, what) } else { self.u32(offset, what).map(u64::from) }
    }

    fn word_size(&self) -> usize {
        if self.wide { 8 } else { 4 }
    }

    /// The NUL-terminated string at `offset`, at most `max` bytes; a byte that is not UTF-8 becomes
    /// U+FFFD, since a name is a label.
    fn string(&self, offset: usize, max: usize, what: &str) -> Result<String> {
        let bytes = self.bytes();
        ensure!(offset <= bytes.len(), "{what} at offset {offset:#x} is past the end of the file ({:#x} bytes)", bytes.len());
        let window = &bytes[offset..bytes.len().min(offset.saturating_add(max))];
        let Some(end) = window.iter().position(|&b| b == 0) else {
            bail!("{what} at offset {offset:#x} has no terminator within {max} bytes")
        };
        Ok(String::from_utf8_lossy(&window[..end]).into_owned())
    }
}

impl ElfFile {
    /// Reads the headers and the tables of an ELF file. The headers must read; a table that does
    /// not (the symbols, the dynamic section, `.eh_frame`) leaves a warning and stays empty.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(is_elf(bytes), "the file does not start with the ELF magic");
        let reader = Reader::new(bytes);
        let class = reader.u8(4, "the ELF class (EI_CLASS)")?;
        let encoding = reader.u8(5, "the ELF data encoding (EI_DATA)")?;
        let wide = match class {
            1 => false,
            2 => true,
            other => bail!("the ELF class at offset 0x4 is {other}, neither 32 (1) nor 64 bits (2)"),
        };
        let big = match encoding {
            1 => false,
            2 => true,
            other => bail!("the ELF data encoding at offset 0x5 is {other}, neither little (1) nor big-endian (2)"),
        };
        let d = Data { reader, big, wide };
        let kind = d.u16(16, "e_type")?;
        let machine = d.u16(18, "e_machine")?;
        let (entry, phoff, shoff, rest) = if wide {
            (d.u64(24, "e_entry")?, d.u64(32, "e_phoff")?, d.u64(40, "e_shoff")?, 52)
        } else {
            (u64::from(d.u32(24, "e_entry")?), u64::from(d.u32(28, "e_phoff")?), u64::from(d.u32(32, "e_shoff")?), 40)
        };
        let phentsize = usize::from(d.u16(rest + 2, "e_phentsize")?);
        let phnum = usize::from(d.u16(rest + 4, "e_phnum")?);
        let shentsize = usize::from(d.u16(rest + 6, "e_shentsize")?);
        let shnum = usize::from(d.u16(rest + 8, "e_shnum")?);
        let shstrndx = usize::from(d.u16(rest + 10, "e_shstrndx")?);

        let mut segments = Vec::with_capacity(phnum);
        if phnum != 0 {
            ensure!(phentsize >= if wide { 56 } else { 32 }, "e_phentsize {phentsize} is smaller than a program header");
        }
        for i in 0..phnum {
            let at = usize::try_from(phoff).unwrap_or(usize::MAX).saturating_add(i * phentsize);
            let what = |field: &str| format!("program header {i} {field}");
            segments.push(if wide {
                Segment {
                    kind: d.u32(at, &what("p_type"))?,
                    flags: d.u32(at + 4, &what("p_flags"))?,
                    offset: d.u64(at + 8, &what("p_offset"))?,
                    vaddr: d.u64(at + 16, &what("p_vaddr"))?,
                    filesz: d.u64(at + 32, &what("p_filesz"))?,
                    memsz: d.u64(at + 40, &what("p_memsz"))?,
                    align: d.u64(at + 48, &what("p_align"))?,
                }
            } else {
                Segment {
                    kind: d.u32(at, &what("p_type"))?,
                    offset: u64::from(d.u32(at + 4, &what("p_offset"))?),
                    vaddr: u64::from(d.u32(at + 8, &what("p_vaddr"))?),
                    filesz: u64::from(d.u32(at + 16, &what("p_filesz"))?),
                    memsz: u64::from(d.u32(at + 20, &what("p_memsz"))?),
                    flags: d.u32(at + 24, &what("p_flags"))?,
                    align: u64::from(d.u32(at + 28, &what("p_align"))?),
                }
            });
        }
        let headers_size = phoff + (phnum * phentsize) as u64;

        let sections = if shoff == 0 { Vec::new() } else { read_sections(d, shoff, shentsize, shnum, shstrndx)? };
        let mut elf = Self {
            wide,
            big_endian: big,
            kind,
            machine,
            entry,
            segments,
            sections,
            headers_size,
            symbols: Vec::new(),
            dynamic_symbols: Vec::new(),
            interpreter: None,
            needed: Vec::new(),
            soname: None,
            relocations: Vec::new(),
            functions: Vec::new(),
            warnings: Vec::new(),
        };
        if let Some(interp) = elf.segments.iter().find(|s| s.kind == PT_INTERP) {
            match d.string(interp.offset as usize, interp.filesz as usize, "the program interpreter") {
                Ok(path) => elf.interpreter = Some(path),
                Err(e) => elf.warnings.push(format!("interpreter: {e:#}")),
            }
        }
        if let Some(table) = elf.sections.iter().find(|s| s.kind == SHT_SYMTAB) {
            match elf.read_symbols(d, table) {
                Ok(symbols) => elf.symbols = symbols,
                Err(e) => elf.warnings.push(format!("symbol table: {e:#}")),
            }
        }
        if let Err(e) = elf.read_dynamic(d) {
            elf.warnings.push(format!("dynamic section: {e:#}"));
        }
        match elf.read_eh_frame(d) {
            Ok(functions) => elf.functions = functions,
            Err(e) => elf.warnings.push(format!(".eh_frame: {e:#}")),
        }
        Ok(elf)
    }

    /// The file offset of virtual address `va`, when a loaded segment holds it in the file's bytes.
    pub fn offset_of(&self, va: u64) -> Option<u64> {
        self.segments.iter().find(|s| s.is_load() && va >= s.vaddr && va - s.vaddr < s.filesz).map(|s| s.offset + (va - s.vaddr))
    }

    /// The section called `name`.
    pub fn section(&self, name: &str) -> Option<&SectionHeader> {
        self.sections.iter().find(|s| s.name == name)
    }

    fn offset_or(&self, va: u64, what: &str) -> Result<usize> {
        match self.offset_of(va) {
            Some(offset) => Ok(offset as usize),
            None => bail!("{what} at {va:#x} lies in no loaded segment's file bytes"),
        }
    }

    fn read_symbols(&self, d: Data<'_>, table: &SectionHeader) -> Result<Vec<Symbol>> {
        let strings = self.sections.get(table.link as usize).context("the symbol table's string table index")?;
        let entsize = if table.entsize == 0 { if self.wide { 24 } else { 16 } } else { table.entsize as usize };
        let count = (table.size as usize / entsize).min(MAX_ENTRIES);
        let mut symbols = Vec::with_capacity(count);
        for i in 0..count {
            let at = table.offset as usize + i * entsize;
            symbols.push(read_symbol(d, at, |name| d.string(strings.offset as usize + name as usize, 4096, "a symbol name"))?);
        }
        Ok(symbols)
    }

    /// The dynamic section: its strings, the libraries, the dynamic symbols and their versions,
    /// the relocations.
    fn read_dynamic(&mut self, d: Data<'_>) -> Result<()> {
        let Some(dynamic) = self.segments.iter().find(|s| s.kind == PT_DYNAMIC).copied() else { return Ok(()) };
        let entry = 2 * d.word_size();
        let mut tags: Vec<(u64, u64)> = Vec::new();
        for i in 0..(dynamic.filesz as usize / entry).min(MAX_ENTRIES) {
            let at = dynamic.offset as usize + i * entry;
            let tag = d.word(at, &format!("dynamic entry {i} d_tag"))?;
            if tag == DT_NULL {
                break;
            }
            tags.push((tag, d.word(at + d.word_size(), &format!("dynamic entry {i} d_val"))?));
        }
        let value = |tag: u64| tags.iter().find(|&&(t, _)| t == tag).map(|&(_, v)| v);
        let Some(strtab) = value(DT_STRTAB) else { return Ok(()) };
        let strtab = self.offset_or(strtab, "DT_STRTAB")?;
        let strsz = value(DT_STRSZ).unwrap_or(0) as usize;
        let string = |offset: u64, what: &str| -> Result<String> {
            let max = strsz.saturating_sub(offset as usize).max(1).min(4096);
            d.string(strtab + offset as usize, max, what)
        };
        for &(tag, v) in &tags {
            match tag {
                DT_NEEDED => self.needed.push(string(v, "a DT_NEEDED name")?),
                DT_SONAME => self.soname = Some(string(v, "the DT_SONAME")?),
                _ => {}
            }
        }

        // The relocations first: when nothing else counts the dynamic symbols, the highest index
        // a relocation names bounds them.
        let rela_size = if self.wide { 24 } else { 12 };
        let rel_size = if self.wide { 16 } else { 8 };
        let mut tables = Vec::new();
        if let (Some(at), Some(size)) = (value(DT_RELA), value(DT_RELASZ)) {
            tables.push((at, size, true, "DT_RELA"));
        }
        if let (Some(at), Some(size)) = (value(DT_REL), value(DT_RELSZ)) {
            tables.push((at, size, false, "DT_REL"));
        }
        if let (Some(at), Some(size)) = (value(DT_JMPREL), value(DT_PLTRELSZ)) {
            tables.push((at, size, value(DT_PLTREL) == Some(DT_RELA), "DT_JMPREL"));
        }
        for (va, size, rela, what) in tables {
            let start = self.offset_or(va, what)?;
            let entsize = if rela { rela_size } else { rel_size };
            for i in 0..(size as usize / entsize).min(MAX_ENTRIES) {
                let at = start + i * entsize;
                let offset = d.word(at, &format!("{what} record {i} r_offset"))?;
                let info = d.word(at + d.word_size(), &format!("{what} record {i} r_info"))?;
                let (symbol, kind) =
                    if self.wide { ((info >> 32) as u32, (info & 0xffff_ffff) as u32) } else { ((info >> 8) as u32, (info & 0xff) as u32) };
                let addend = match (rela, self.wide) {
                    (false, _) => 0,
                    (true, true) => d.u64(at + 16, &format!("{what} record {i} r_addend"))? as i64,
                    (true, false) => i64::from(d.u32(at + 8, &format!("{what} record {i} r_addend"))? as i32),
                };
                self.relocations.push(Relocation { offset, kind, symbol, addend });
            }
        }
        // The packed relative relocations of `DT_RELR`, which Android's and today's linkers write:
        // each word they name is a `RELATIVE` record whose addend is in place.
        if let (Some(va), Some(size), Some(kind)) = (value(DT_RELR), value(DT_RELRSZ), reloc::relative(self.machine)) {
            let start = self.offset_or(va, "DT_RELR")?;
            let width = d.word_size();
            let mut entries = Vec::new();
            for i in 0..(size as usize / width).min(MAX_ENTRIES) {
                entries.push(d.word(start + i * width, &format!("DT_RELR entry {i}"))?);
            }
            for offset in unpack_relr(&entries, width as u64) {
                self.relocations.push(Relocation { offset, kind, symbol: 0, addend: 0 });
            }
        }

        // The dynamic symbols: as many as `.dynsym` holds, else as the hash table's chain counts,
        // else as the relocations name.
        let Some(symtab) = value(DT_SYMTAB) else { return Ok(()) };
        let symtab = self.offset_or(symtab, "DT_SYMTAB")?;
        let syment = value(DT_SYMENT).map_or(if self.wide { 24 } else { 16 }, |v| v as usize);
        ensure!(syment >= if self.wide { 24 } else { 16 }, "DT_SYMENT {syment} is smaller than a symbol");
        let count = if let Some(table) = self.sections.iter().find(|s| s.kind == SHT_DYNSYM) {
            table.size as usize / syment
        } else if let Some(hash) = value(DT_HASH) {
            d.u32(self.offset_or(hash, "DT_HASH")? + 4, "the DT_HASH chain count")? as usize
        } else {
            self.relocations.iter().map(|r| r.symbol as usize + 1).max().unwrap_or(0)
        };
        let mut symbols = Vec::with_capacity(count.min(MAX_ENTRIES));
        for i in 0..count.min(MAX_ENTRIES) {
            symbols.push(read_symbol(d, symtab + i * syment, |name| string(u64::from(name), "a dynamic symbol name"))?);
        }

        // The versions: each dynamic symbol's index into the versions the file needs, and each of
        // those the library that defines it.
        if let (Some(versym), Some(verneed), Some(count)) = (value(DT_VERSYM), value(DT_VERNEED), value(DT_VERNEEDNUM)) {
            let mut libraries: HashMap<u16, String> = HashMap::new();
            let mut at = self.offset_or(verneed, "DT_VERNEED")?;
            for i in 0..(count as usize).min(MAX_ENTRIES) {
                let what = |field: &str| format!("version need {i} {field}");
                let entries = d.u16(at + 2, &what("vn_cnt"))?;
                let file = string(u64::from(d.u32(at + 4, &what("vn_file"))?), &what("file name"))?;
                let mut aux = at + d.u32(at + 8, &what("vn_aux"))? as usize;
                for _ in 0..entries {
                    let index = d.u16(aux + 6, &what("vna_other"))?;
                    libraries.insert(index, file.clone());
                    let next = d.u32(aux + 12, &what("vna_next"))? as usize;
                    if next == 0 {
                        break;
                    }
                    aux += next;
                }
                let next = d.u32(at + 12, &what("vn_next"))? as usize;
                if next == 0 {
                    break;
                }
                at += next;
            }
            let versym = self.offset_or(versym, "DT_VERSYM")?;
            for (i, symbol) in symbols.iter_mut().enumerate() {
                let version = d.u16(versym + 2 * i, "a DT_VERSYM entry")? & 0x7fff;
                symbol.library = libraries.get(&version).cloned();
            }
        }
        self.dynamic_symbols = symbols;
        Ok(())
    }

    /// The function extents of `.eh_frame`: the section when the file names it, else the table
    /// `PT_GNU_EH_FRAME`'s header points at.
    fn read_eh_frame(&self, d: Data<'_>) -> Result<Vec<(u64, u64)>> {
        let (va, start, end) = if let Some(s) = self.section(".eh_frame").filter(|s| s.kind == SHT_PROGBITS && s.addr != 0) {
            (s.addr, s.offset as usize, (s.offset + s.size) as usize)
        } else if let Some(hdr) = self.segments.iter().find(|s| s.kind == PT_GNU_EH_FRAME) {
            let at = hdr.offset as usize;
            ensure!(d.u8(at, "the .eh_frame_hdr version")? == 1, "the .eh_frame_hdr at offset {at:#x} is not version 1");
            let encoding = d.u8(at + 1, "the .eh_frame_hdr eh_frame_ptr encoding")?;
            let mut cursor = at + 4;
            let Some(va) = read_encoded(d, &mut cursor, encoding, hdr.vaddr + 4, 0)? else { return Ok(Vec::new()) };
            let start = self.offset_or(va, ".eh_frame")?;
            let segment = self.segments.iter().find(|s| s.is_load() && va >= s.vaddr && va - s.vaddr < s.filesz).unwrap();
            (va, start, (segment.offset + segment.filesz) as usize)
        } else {
            return Ok(Vec::new());
        };
        ensure!(end <= d.bytes().len(), ".eh_frame at offset {start:#x} runs past the end of the file");
        let va_of = |offset: usize| va + (offset - start) as u64;

        // The FDE pointer encoding of each CIE, by the CIE's offset.
        let mut cies: HashMap<usize, u8> = HashMap::new();
        let mut functions = Vec::new();
        let mut at = start;
        while at + 4 <= end {
            let mut length = u64::from(d.u32(at, "an .eh_frame record length")?);
            let mut body = at + 4;
            if length == 0 {
                break;
            }
            if length == 0xffff_ffff {
                length = d.u64(body, "an .eh_frame record's 64-bit length")?;
                body += 8;
            }
            let next = body.checked_add(length as usize).filter(|&n| n <= end);
            let Some(next) = next else { bail!("the .eh_frame record at offset {at:#x} runs past the section's end") };
            let id = d.u32(body, "an .eh_frame record's CIE id")?;
            if id == 0 {
                cies.insert(at, cie_encoding(d, body + 4, next)?);
            } else {
                let cie = body.checked_sub(id as usize).with_context(|| format!("the FDE at offset {at:#x} points before the file"))?;
                let encoding = match cies.get(&cie) {
                    Some(&e) => e,
                    None => {
                        // A CIE after its FDE, or one the walk stepped over: its id is past its length.
                        let e = cie_encoding(d, cie + 8, end)?;
                        cies.insert(cie, e);
                        e
                    }
                };
                let mut cursor = body + 4;
                let pc = va_of(cursor);
                let begin = read_encoded(d, &mut cursor, encoding, pc, 0)?;
                // The range has the format of the encoding and none of its base.
                let range = read_encoded(d, &mut cursor, encoding & 0x0f, 0, 0)?;
                if let (Some(begin), Some(range)) = (begin, range)
                    && begin != 0
                    && range != 0
                {
                    functions.push((begin, begin + range));
                }
            }
            at = next;
        }
        functions.sort_unstable();
        functions.dedup();
        Ok(functions)
    }
}

/// The section headers, each with its name from the section name table.
fn read_sections(d: Data<'_>, shoff: u64, shentsize: usize, shnum: usize, shstrndx: usize) -> Result<Vec<SectionHeader>> {
    ensure!(shentsize >= if d.wide { 64 } else { 40 }, "e_shentsize {shentsize} is smaller than a section header");
    let read = |i: usize| -> Result<(u32, SectionHeader)> {
        let at = usize::try_from(shoff).unwrap_or(usize::MAX).saturating_add(i * shentsize);
        let what = |field: &str| format!("section header {i} {field}");
        let name = d.u32(at, &what("sh_name"))?;
        let header = if d.wide {
            SectionHeader {
                name: String::new(),
                kind: d.u32(at + 4, &what("sh_type"))?,
                flags: d.u64(at + 8, &what("sh_flags"))?,
                addr: d.u64(at + 16, &what("sh_addr"))?,
                offset: d.u64(at + 24, &what("sh_offset"))?,
                size: d.u64(at + 32, &what("sh_size"))?,
                link: d.u32(at + 40, &what("sh_link"))?,
                info: d.u32(at + 44, &what("sh_info"))?,
                entsize: d.u64(at + 56, &what("sh_entsize"))?,
            }
        } else {
            SectionHeader {
                name: String::new(),
                kind: d.u32(at + 4, &what("sh_type"))?,
                flags: u64::from(d.u32(at + 8, &what("sh_flags"))?),
                addr: u64::from(d.u32(at + 12, &what("sh_addr"))?),
                offset: u64::from(d.u32(at + 16, &what("sh_offset"))?),
                size: u64::from(d.u32(at + 20, &what("sh_size"))?),
                link: d.u32(at + 24, &what("sh_link"))?,
                info: d.u32(at + 28, &what("sh_info"))?,
                entsize: u64::from(d.u32(at + 36, &what("sh_entsize"))?),
            }
        };
        Ok((name, header))
    };
    // A file with more sections than `e_shnum` holds keeps the count in section 0's size, and the
    // name table's index in its link.
    let first = read(0)?;
    let count = if shnum == 0 { first.1.size as usize } else { shnum };
    let names = if shstrndx == 0xffff { first.1.link as usize } else { shstrndx };
    ensure!(count <= MAX_ENTRIES, "the ELF header names {count} sections");
    let mut headers = vec![first];
    for i in 1..count {
        headers.push(read(i)?);
    }
    let table = headers.get(names).map(|(_, s)| (s.offset as usize, s.size as usize));
    let mut sections = Vec::with_capacity(headers.len());
    for (index, mut s) in headers {
        let index = index as usize;
        if let Some((offset, size)) = table
            && index < size
        {
            s.name = d.string(offset + index, size - index, "a section name")?;
        }
        sections.push(s);
    }
    Ok(sections)
}

/// One symbol record at `at`, with its name from `name_of`.
fn read_symbol(d: Data<'_>, at: usize, name_of: impl Fn(u32) -> Result<String>) -> Result<Symbol> {
    let name = d.u32(at, "st_name")?;
    let (info, other, section, value, size) = if d.wide {
        (
            d.u8(at + 4, "st_info")?,
            d.u8(at + 5, "st_other")?,
            d.u16(at + 6, "st_shndx")?,
            d.u64(at + 8, "st_value")?,
            d.u64(at + 16, "st_size")?,
        )
    } else {
        (
            d.u8(at + 12, "st_info")?,
            d.u8(at + 13, "st_other")?,
            d.u16(at + 14, "st_shndx")?,
            u64::from(d.u32(at + 4, "st_value")?),
            u64::from(d.u32(at + 8, "st_size")?),
        )
    };
    let name = if name == 0 { String::new() } else { name_of(name)? };
    Ok(Symbol { name, value, size, kind: info & 0xf, binding: info >> 4, visibility: other & 3, section, library: None })
}

/// The pointer encoding of the FDEs of the CIE whose body (past its id) starts at `at`: the `R`
/// of its augmentation, or an absolute pointer without one.
fn cie_encoding(d: Data<'_>, at: usize, end: usize) -> Result<u8> {
    let version = d.u8(at, "a CIE version")?;
    let augmentation = d.string(at + 1, end.saturating_sub(at + 1).max(1), "a CIE augmentation")?;
    let mut cursor = at + 1 + augmentation.len() + 1;
    if augmentation.contains("eh") {
        cursor += d.word_size();
    }
    leb128(d, &mut cursor, false)?; // code alignment
    leb128(d, &mut cursor, true)?; // data alignment
    if version == 1 {
        cursor += 1;
    } else {
        leb128(d, &mut cursor, false)?; // return address register
    }
    if !augmentation.starts_with('z') {
        return Ok(0);
    }
    leb128(d, &mut cursor, false)?; // augmentation data length
    for c in augmentation.chars().skip(1) {
        match c {
            'R' => return d.u8(cursor, "a CIE's FDE pointer encoding"),
            'L' => cursor += 1,
            'P' => {
                let encoding = d.u8(cursor, "a CIE's personality encoding")?;
                cursor += 1;
                // Only its length matters here: the base is not needed to skip it.
                read_encoded(d, &mut cursor, encoding & 0x0f, 0, 0)?;
            }
            'S' | 'B' | 'G' => {}
            other => bail!("a CIE's augmentation {augmentation:?} has the unknown letter {other:?}"),
        }
    }
    Ok(0)
}

/// A pointer in the DWARF exception encoding `encoding` at `cursor`, which advances past it:
/// `pc` is the address of the field, the base of a `pcrel` one, and `data` the base of a `datarel`
/// one. `None` for the omitted encoding.
fn read_encoded(d: Data<'_>, cursor: &mut usize, encoding: u8, pc: u64, data: u64) -> Result<Option<u64>> {
    if encoding == 0xff {
        return Ok(None);
    }
    let at = *cursor;
    let what = "an .eh_frame pointer";
    let value = match encoding & 0x0f {
        0x00 => {
            *cursor += d.word_size();
            d.word(at, what)?
        }
        0x01 => leb128(d, cursor, false)?,
        0x09 => leb128(d, cursor, true)?,
        0x02 => {
            *cursor += 2;
            u64::from(d.u16(at, what)?)
        }
        0x0a => {
            *cursor += 2;
            d.u16(at, what)? as i16 as i64 as u64
        }
        0x03 => {
            *cursor += 4;
            u64::from(d.u32(at, what)?)
        }
        0x0b => {
            *cursor += 4;
            i64::from(d.u32(at, what)? as i32) as u64
        }
        0x04 | 0x0c => {
            *cursor += 8;
            d.u64(at, what)?
        }
        other => bail!("the pointer encoding {encoding:#x} at offset {at:#x} has the unknown format {other:#x}"),
    };
    let base = match encoding & 0x70 {
        0x00 => 0,
        0x10 => pc,
        0x30 => data,
        other => bail!("the pointer encoding {encoding:#x} at offset {at:#x} has the unsupported application {other:#x}"),
    };
    ensure!(encoding & 0x80 == 0, "the pointer at offset {at:#x} is indirect ({encoding:#x}), which an extent never is");
    let value = base.wrapping_add(value);
    Ok(Some(if d.wide { value } else { value & 0xffff_ffff }))
}

/// An unsigned or a signed LEB128 number at `cursor`, which advances past it.
fn leb128(d: Data<'_>, cursor: &mut usize, signed: bool) -> Result<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = d.u8(*cursor, "a LEB128 number")?;
        *cursor += 1;
        if shift < 64 {
            value |= u64::from(byte & 0x7f) << shift;
        }
        shift += 7;
        if byte & 0x80 == 0 {
            if signed && shift < 64 && byte & 0x40 != 0 {
                value |= u64::MAX << shift;
            }
            return Ok(value);
        }
        ensure!(shift < 70, "the LEB128 number at offset {:#x} is longer than ten bytes", *cursor);
    }
}

/// The addresses a `DT_RELR` table relocates, of words `width` bytes wide: an even entry is an
/// address, and an odd one a bitmap of the words after the last, bit 1 the first of them, each
/// bitmap covering one word fewer than its bits.
fn unpack_relr(entries: &[u64], width: u64) -> Vec<u64> {
    let bits = width * 8;
    let mut addresses = Vec::new();
    let mut next = 0u64;
    for &entry in entries {
        if entry & 1 == 0 {
            addresses.push(entry);
            next = entry.wrapping_add(width);
            continue;
        }
        for bit in 1..bits {
            if (entry >> bit) & 1 != 0 {
                addresses.push(next.wrapping_add((bit - 1) * width));
            }
        }
        next = next.wrapping_add((bits - 1) * width);
    }
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64-bit little-endian executable with one loaded segment and nothing else.
    fn minimal(big: bool) -> Vec<u8> {
        let mut b = vec![0u8; 0x78];
        b[..4].copy_from_slice(MAGIC);
        b[4] = 2;
        b[5] = if big { 2 } else { 1 };
        b[6] = 1;
        let put16 =
            |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let put32 =
            |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let put64 =
            |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        put16(&mut b, 16, ET_EXEC);
        put16(&mut b, 18, EM_X86_64);
        put64(&mut b, 24, 0x40_0078);
        put64(&mut b, 32, 0x40);
        put16(&mut b, 54, 56);
        put16(&mut b, 56, 1);
        put32(&mut b, 0x40, PT_LOAD);
        put32(&mut b, 0x44, PF_R | PF_X);
        put64(&mut b, 0x50, 0x40_0000);
        put64(&mut b, 0x60, 0x78);
        put64(&mut b, 0x68, 0x78);
        b
    }

    #[test]
    fn a_minimal_executable_reads_in_either_byte_order() {
        for big in [false, true] {
            let elf = ElfFile::parse(&minimal(big)).unwrap();
            assert_eq!((elf.wide, elf.big_endian, elf.kind, elf.machine, elf.entry), (true, big, ET_EXEC, EM_X86_64, 0x40_0078));
            assert_eq!(elf.segments.len(), 1);
            assert_eq!(elf.segments[0].vaddr, 0x40_0000);
            assert_eq!(elf.offset_of(0x40_0010), Some(0x10));
            assert!(elf.sections.is_empty() && elf.warnings.is_empty(), "{:?}", elf.warnings);
        }
    }

    #[test]
    fn a_truncated_program_header_names_its_field() {
        let mut bytes = minimal(false);
        bytes.truncate(0x50);
        let e = ElfFile::parse(&bytes).unwrap_err();
        assert!(format!("{e:#}").contains("program header 0"), "{e:#}");
    }

    #[test]
    fn the_leb128_numbers_read_as_dwarf_writes_them() {
        let d = Data { reader: Reader::new(&[0xe5, 0x8e, 0x26, 0x7f, 0x80, 0x7f]), big: false, wide: true };
        let mut at = 0;
        assert_eq!(leb128(d, &mut at, false).unwrap(), 624_485);
        assert_eq!(leb128(d, &mut at, true).unwrap() as i64, -1);
        assert_eq!(leb128(d, &mut at, true).unwrap() as i64, -128);
        assert_eq!(at, 6);
    }

    #[test]
    fn a_relr_table_unpacks_to_its_words() {
        // 0x1000, then the words at 0x1008 and 0x1018 (bits 1 and 3), then past the 63 words
        // the bitmap covers the one at 0x1008 + 63 * 8 (bit 1 of the next).
        let entries = [0x1000, 0b1011, 0b11];
        assert_eq!(unpack_relr(&entries, 8), vec![0x1000, 0x1008, 0x1018, 0x1008 + 63 * 8]);
        // A 32-bit table: 31 words a bitmap.
        assert_eq!(unpack_relr(&[0x2000, 0b101, 0b11], 4), vec![0x2000, 0x2008, 0x2004 + 31 * 4]);
    }
}
