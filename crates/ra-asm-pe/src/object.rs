//! Relocatable objects and the archives that hold them: a COFF `.obj` or an ELF `.o`, alone or
//! as a member of a `.lib` or `.a`. What the signatures need of one: each code section's bytes,
//! the relocations that patch them (their place, their width and the symbol they name) and the
//! function symbols that start inside. A member that is no object this reader knows (an import
//! stub of a `.lib`, a `/bigobj` member, a big-endian ELF) is skipped and counted, not refused:
//! a library holds what its linker wanted, and the functions are what matter.

use anyhow::{Result, bail, ensure};
use ra_asm_format::Reader;

/// One relocation of a code section.
#[derive(Debug, Clone)]
pub struct ObjectReloc {
    /// The offset of the patched bytes in the section.
    pub offset: u32,
    /// How many bytes the linker overwrites.
    pub size: u8,
    /// The symbol the relocation names, for a named symbol of the object or of another; `None`
    /// for a section symbol (a jump table, a string of the same object).
    pub symbol: Option<String>,
}

/// A section of an object that holds code.
#[derive(Debug, Clone)]
pub struct ObjectSection {
    pub name: String,
    pub bytes: Vec<u8>,
    /// In offset order.
    pub relocations: Vec<ObjectReloc>,
}

/// A function the object defines: a symbol that starts inside a code section.
#[derive(Debug, Clone)]
pub struct ObjectFunction {
    /// The linkage name as the object has it (`_printf` on x86, `printf` on x64).
    pub name: String,
    /// The index into [`Object::sections`].
    pub section: usize,
    pub offset: u32,
    /// The bytes from the function's start to the next symbol of its section, or the section's end.
    pub size: u32,
    pub external: bool,
}

/// What one object holds that a signature can be made of.
#[derive(Debug, Clone)]
pub struct Object {
    /// The file or the archive member's name (`printf.obj`).
    pub name: String,
    /// The machine, in the PE's numbering for a COFF object and the ELF `e_machine` for an ELF one,
    /// with [`Object::machine_name`] telling them apart.
    pub machine: u16,
    pub elf: bool,
    pub sections: Vec<ObjectSection>,
    pub functions: Vec<ObjectFunction>,
}

impl Object {
    /// `x86`, `x64`, `arm64`, `mips`… or the number for a machine without a name here.
    pub fn machine_name(&self) -> String {
        let name = match (self.elf, self.machine) {
            (false, 0x14c) | (true, 3) => "x86",
            (false, 0x8664) | (true, 62) => "x64",
            (false, 0xaa64) | (true, 183) => "arm64",
            (false, 0x1c0 | 0x1c4) | (true, 40) => "arm",
            (false, 0x166 | 0x266 | 0x366 | 0x466) | (true, 8) => "mips",
            (true, 20) => "powerpc",
            _ => return format!("{:#x}", self.machine),
        };
        name.to_owned()
    }
}

/// A library or an object file read whole: the objects it holds, and the members it skipped.
#[derive(Debug, Default)]
pub struct Library {
    pub objects: Vec<Object>,
    pub skipped: Vec<String>,
}

/// Reads the file at `path`: an archive (`.lib`, `.a`) member by member, else one object.
pub fn read(path: &std::path::Path) -> Result<Library> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    parse(&name, &bytes)
}

/// [`read`] over bytes already in memory, named `name`.
pub fn parse(name: &str, bytes: &[u8]) -> Result<Library> {
    let mut library = Library::default();
    if bytes.starts_with(ARCHIVE_MAGIC) {
        for (member, data) in archive_members(bytes)? {
            match parse_object(&member, data) {
                Ok(Some(object)) => library.objects.push(object),
                Ok(None) => library.skipped.push(member),
                Err(e) => bail!("{name}: member {member}: {e:#}"),
            }
        }
    } else {
        match parse_object(name, bytes)? {
            Some(object) => library.objects.push(object),
            None => bail!("{name} is neither a COFF nor an ELF object nor an archive"),
        }
    }
    Ok(library)
}

/// One object from its bytes: `None` when the bytes are no object this reader knows. `name` is
/// kept as its file name: a library built elsewhere names its members by their full build path.
pub fn parse_object(name: &str, bytes: &[u8]) -> Result<Option<Object>> {
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    if bytes.starts_with(b"\x7fELF") {
        return elf::parse(name, bytes);
    }
    coff::parse(name, bytes)
}

const ARCHIVE_MAGIC: &[u8] = b"!<arch>\n";

/// The members of an archive, with their names resolved: the linker's own members (`/`, `//`,
/// `__.SYMDEF`) are left out.
fn archive_members(bytes: &[u8]) -> Result<Vec<(String, &[u8])>> {
    let reader = Reader::new(bytes);
    let mut members = Vec::new();
    let mut long_names: &[u8] = &[];
    let mut offset = ARCHIVE_MAGIC.len();
    while offset + 60 <= bytes.len() {
        let header = reader.slice(offset, 60, "archive member header")?;
        ensure!(&header[58..60] == b"`\n", "archive member header at offset {offset:#x} has no terminator");
        let name_field = String::from_utf8_lossy(&header[..16]).trim_end().to_owned();
        let size_text = String::from_utf8_lossy(&header[48..58]).trim().to_owned();
        let size: usize = size_text.parse().map_err(|_| anyhow::anyhow!("archive member at offset {offset:#x} has size {size_text:?}"))?;
        let data = reader.slice(offset + 60, size, "archive member data")?;
        offset += 60 + size + (size & 1);
        if name_field == "//" {
            long_names = data;
            continue;
        }
        if name_field == "/" || name_field.starts_with("__.SYMDEF") {
            continue;
        }
        let (name, data) = if let Some(rest) = name_field.strip_prefix("#1/") {
            // BSD: the name is the first bytes of the data.
            let len: usize = rest.parse().map_err(|_| anyhow::anyhow!("archive member at offset {offset:#x} has name {name_field:?}"))?;
            let len = len.min(data.len());
            (String::from_utf8_lossy(&data[..len]).trim_end_matches('\0').to_owned(), &data[len..])
        } else if let Some(rest) = name_field.strip_prefix('/') {
            let start: usize = rest.parse().map_err(|_| anyhow::anyhow!("archive member at offset {offset:#x} has name {name_field:?}"))?;
            ensure!(start < long_names.len(), "archive member at offset {offset:#x} names long name {start} past the table");
            let end = long_names[start..].iter().position(|&b| b == b'\n' || b == 0).map_or(long_names.len(), |p| start + p);
            (String::from_utf8_lossy(&long_names[start..end]).trim_end_matches('/').to_owned(), data)
        } else {
            (name_field.trim_end_matches('/').to_owned(), data)
        };
        members.push((name, data));
    }
    Ok(members)
}

mod coff {
    use anyhow::{Result, ensure};
    use ra_asm_format::Reader;

    use super::{Object, ObjectFunction, ObjectReloc, ObjectSection};

    const SCN_CNT_CODE: u32 = 0x20;
    const SCN_MEM_EXECUTE: u32 = 0x2000_0000;
    const SCN_LNK_NRELOC_OVFL: u32 = 0x0100_0000;
    const SYM_CLASS_EXTERNAL: u8 = 2;
    const SYM_CLASS_STATIC: u8 = 3;
    const SYM_CLASS_LABEL: u8 = 6;
    const SYM_DTYPE_FUNCTION: u16 = 0x20;

    /// One record of the symbol table.
    struct Symbol {
        name: String,
        value: u32,
        section: i16,
        class: u8,
        function: bool,
        is_section: bool,
    }

    /// The width of the bytes a relocation patches, by machine and type; `None` for a type
    /// that patches nothing (`ABSOLUTE`).
    fn reloc_size(machine: u16, kind: u16) -> Option<u8> {
        match (machine, kind) {
            (_, 0) => None,
            (0x14c, 13) => Some(1),
            (0x14c, 1..=5 | 8..=10) | (0x8664, 0xA | 0xC) => Some(2),
            (0x8664, 1) | (0xaa64, 0xE | 0xF) => Some(8),
            _ => Some(4),
        }
    }

    pub(super) fn parse(name: &str, bytes: &[u8]) -> Result<Option<Object>> {
        let reader = Reader::new(bytes);
        if bytes.len() < 20 {
            return Ok(None);
        }
        let machine = reader.u16(0, "machine")?;
        let section_count = reader.u16(2, "section count")? as usize;
        // An import stub (`IMAGE_FILE_MACHINE_UNKNOWN` with 0xFFFF sections) and a `/bigobj`
        // member (the same signature with its own header) are no objects of this reader.
        if machine == 0 || section_count == 0xFFFF {
            return Ok(None);
        }
        let symbol_table = reader.u32(8, "symbol table offset")? as usize;
        let symbol_count = reader.u32(12, "symbol count")? as usize;
        let optional = reader.u16(16, "optional header size")? as usize;
        // A machine this reader does not know is no object of it (a text file opens with any two
        // bytes); a known one whose symbol table runs past the file is a refusal, not an allocation.
        if !matches!(machine, 0x14c | 0x8664 | 0xaa64 | 0x1c0 | 0x1c4 | 0x5032 | 0x5064) {
            return Ok(None);
        }
        ensure!(
            symbol_count == 0 || symbol_table.checked_add(symbol_count.saturating_mul(18)).is_some_and(|end| end <= bytes.len()),
            "{name}: the symbol table ({symbol_count} symbols at {symbol_table:#x}) runs past the end of the file ({} bytes)",
            bytes.len()
        );
        let strings_offset = symbol_table + symbol_count * 18;
        let strings = if symbol_count > 0 && strings_offset + 4 <= bytes.len() {
            let len = (reader.u32(strings_offset, "string table size")? as usize).min(bytes.len() - strings_offset);
            &bytes[strings_offset..strings_offset + len]
        } else {
            &[][..]
        };
        let symbol_name = |index: usize| -> Result<String> {
            let at = symbol_table + index * 18;
            let raw = reader.slice(at, 8, "symbol name")?;
            if raw[..4] == [0, 0, 0, 0] {
                let offset = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
                ensure!(offset < strings.len(), "symbol {index} names string table offset {offset:#x} past its end");
                let end = strings[offset..].iter().position(|&b| b == 0).map_or(strings.len(), |p| offset + p);
                Ok(String::from_utf8_lossy(&strings[offset..end]).into_owned())
            } else {
                let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
                Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
            }
        };

        // The symbols first: the relocations name them and the functions are among them.
        let mut symbols: Vec<Option<Symbol>> = Vec::with_capacity(symbol_count);
        let mut index = 0;
        while index < symbol_count {
            let at = symbol_table + index * 18;
            let value = reader.u32(at + 8, "symbol value")?;
            let section = reader.u16(at + 12, "symbol section")? as i16;
            let kind = reader.u16(at + 14, "symbol type")?;
            let class = reader.u8(at + 16, "symbol class")?;
            let aux = reader.u8(at + 17, "symbol aux count")? as usize;
            let name = symbol_name(index)?;
            // A section's own symbol has its name, value zero and a definition aux record.
            let is_section = class == SYM_CLASS_STATIC && value == 0 && aux == 1 && name.starts_with('.');
            symbols.push(Some(Symbol { name, value, section, class, function: kind & 0xF0 == SYM_DTYPE_FUNCTION, is_section }));
            for _ in 0..aux {
                symbols.push(None);
            }
            index += 1 + aux;
        }

        let mut sections = Vec::new();
        // The section number of each code section, to the index in `sections`.
        let mut code_index: Vec<Option<usize>> = vec![None; section_count + 1];
        let table = 20 + optional;
        for (number, index_of) in code_index.iter_mut().enumerate().skip(1) {
            let at = table + (number - 1) * 40;
            let section_name = reader.fixed_str(at, 8, "section name")?;
            let raw_size = reader.u32(at + 16, "section raw size")? as usize;
            let raw_offset = reader.u32(at + 20, "section raw offset")? as usize;
            let reloc_offset = reader.u32(at + 24, "section relocation offset")? as usize;
            let mut reloc_count = reader.u16(at + 32, "section relocation count")? as usize;
            let characteristics = reader.u32(at + 36, "section characteristics")?;
            if characteristics & (SCN_CNT_CODE | SCN_MEM_EXECUTE) == 0 || raw_offset == 0 || raw_size == 0 {
                continue;
            }
            let data = reader.slice(raw_offset, raw_size, "section data")?.to_vec();
            let mut relocations = Vec::new();
            let mut first = 0;
            if characteristics & SCN_LNK_NRELOC_OVFL != 0 && reloc_count == 0xFFFF {
                reloc_count = reader.u32(reloc_offset, "relocation overflow count")? as usize;
                first = 1;
            }
            for i in first..reloc_count {
                let r = reloc_offset + i * 10;
                let offset = reader.u32(r, "relocation address")?;
                let symbol = reader.u32(r + 4, "relocation symbol")? as usize;
                let kind = reader.u16(r + 8, "relocation type")?;
                let Some(size) = reloc_size(machine, kind) else { continue };
                let target = symbols.get(symbol).and_then(Option::as_ref);
                let symbol = target.filter(|s| !s.is_section && !s.name.is_empty()).map(|s| s.name.clone());
                relocations.push(ObjectReloc { offset, size, symbol });
            }
            relocations.sort_by_key(|r| r.offset);
            *index_of = Some(sections.len());
            sections.push(ObjectSection { name: section_name, bytes: data, relocations });
        }

        // The functions: every external symbol in a code section, and every static one typed as a
        // function; a label is a place inside one. Each runs to the next function of its section:
        // a label or a static datum between them is part of it.
        let mut starts: Vec<(usize, u32, String, bool, bool)> = symbols
            .iter()
            .flatten()
            .filter(|s| s.section > 0 && !s.is_section && matches!(s.class, SYM_CLASS_EXTERNAL | SYM_CLASS_STATIC | SYM_CLASS_LABEL))
            .filter_map(|s| {
                let section = code_index.get(s.section as usize).copied().flatten()?;
                let function = s.class == SYM_CLASS_EXTERNAL || (s.class == SYM_CLASS_STATIC && s.function);
                Some((section, s.value, s.name.clone(), function, s.class == SYM_CLASS_EXTERNAL))
            })
            .collect();
        starts.sort_by_key(|s| (s.0, s.1));
        let mut functions = Vec::new();
        for (i, (section, offset, name, function, external)) in starts.iter().enumerate() {
            if !function {
                continue;
            }
            let end = starts[i + 1..]
                .iter()
                .find(|s| s.0 == *section && s.3 && s.1 > *offset)
                .map_or(sections[*section].bytes.len() as u32, |s| s.1);
            if end <= *offset {
                continue;
            }
            functions.push(ObjectFunction {
                name: name.clone(),
                section: *section,
                offset: *offset,
                size: end - offset,
                external: *external,
            });
        }
        Ok(Some(Object { name: name.to_owned(), machine, elf: false, sections, functions }))
    }
}

mod elf {
    use anyhow::{Result, bail, ensure};
    use ra_asm_format::Reader;

    use super::{Object, ObjectFunction, ObjectReloc, ObjectSection};

    const ET_REL: u16 = 1;
    const SHT_SYMTAB: u32 = 2;
    const SHT_RELA: u32 = 4;
    const SHT_REL: u32 = 9;
    const SHF_EXECINSTR: u64 = 4;
    const STT_FUNC: u8 = 2;
    const STT_SECTION: u8 = 3;

    /// One section header, in the file's width.
    struct Header {
        name: u32,
        kind: u32,
        flags: u64,
        offset: usize,
        size: usize,
        link: u32,
        info: u32,
        entsize: usize,
    }

    /// One record of the symbol table.
    struct Symbol {
        name: String,
        value: u64,
        size: u64,
        kind: u8,
        global: bool,
        section: u16,
    }

    /// The width of the bytes a relocation patches: a whole instruction word unless the type is
    /// a known data width of `x86-64` or `AArch64`.
    fn reloc_size(machine: u16, kind: u32) -> u8 {
        match (machine, kind) {
            (62, 1 | 24..=27) | (183, 257 | 260) => 8,
            (62, 12 | 13) | (183, 259) => 2,
            (62, 14 | 15) => 1,
            _ => 4,
        }
    }

    /// The file offset of entry `i` of a table section, refused past the end of the file: the
    /// offset and the entry size are 64-bit words of the file, so their sum is checked.
    fn entry_at(h: &Header, i: usize, entsize: usize, len: usize) -> Result<usize> {
        match i.checked_mul(entsize).and_then(|o| h.offset.checked_add(o)) {
            Some(at) if at <= len => Ok(at),
            _ => bail!(
                "entry {i} of the section at offset {:#x} ({entsize:#x} bytes each) lies past the end of the file ({len:#x} bytes)",
                h.offset
            ),
        }
    }

    pub(super) fn parse(name: &str, bytes: &[u8]) -> Result<Option<Object>> {
        let reader = Reader::new(bytes);
        let class = reader.u8(4, "ELF class")?;
        let data = reader.u8(5, "ELF data encoding")?;
        if data != 1 || !matches!(class, 1 | 2) {
            // Big-endian or an unknown class: not read here.
            return Ok(None);
        }
        let wide = class == 2;
        let kind = reader.u16(16, "ELF type")?;
        if kind != ET_REL {
            return Ok(None);
        }
        let machine = reader.u16(18, "ELF machine")?;
        let (shoff, shentsize_at, shnum_at, shstrndx_at) = if wide {
            (reader.u64(40, "section header offset")? as usize, 58, 60, 62)
        } else {
            (reader.u32(32, "section header offset")? as usize, 46, 48, 50)
        };
        let shentsize = reader.u16(shentsize_at, "section header entry size")? as usize;
        let shnum = reader.u16(shnum_at, "section header count")? as usize;
        let shstrndx = reader.u16(shstrndx_at, "section name table index")? as usize;
        ensure!(shentsize >= if wide { 64 } else { 40 }, "section header entry size {shentsize} is too small");

        let mut headers = Vec::with_capacity(shnum);
        for i in 0..shnum {
            let at = shoff + i * shentsize;
            let header = if wide {
                Header {
                    name: reader.u32(at, "section name")?,
                    kind: reader.u32(at + 4, "section type")?,
                    flags: reader.u64(at + 8, "section flags")?,
                    offset: reader.u64(at + 24, "section offset")? as usize,
                    size: reader.u64(at + 32, "section size")? as usize,
                    link: reader.u32(at + 40, "section link")?,
                    info: reader.u32(at + 44, "section info")?,
                    entsize: reader.u64(at + 56, "section entry size")? as usize,
                }
            } else {
                Header {
                    name: reader.u32(at, "section name")?,
                    kind: reader.u32(at + 4, "section type")?,
                    flags: u64::from(reader.u32(at + 8, "section flags")?),
                    offset: reader.u32(at + 16, "section offset")? as usize,
                    size: reader.u32(at + 20, "section size")? as usize,
                    link: reader.u32(at + 24, "section link")?,
                    info: reader.u32(at + 28, "section info")?,
                    entsize: reader.u32(at + 36, "section entry size")? as usize,
                }
            };
            headers.push(header);
        }
        let string_at = |table: usize, offset: u32| -> Result<String> {
            let Some(h) = headers.get(table) else { return Ok(String::new()) };
            let (start, end) = (h.offset.saturating_add(offset as usize), h.offset.saturating_add(h.size));
            ensure!(start <= end && end <= bytes.len(), "string {offset:#x} of section {table} is past its end");
            let s = &bytes[start..end];
            let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
            Ok(String::from_utf8_lossy(&s[..len]).into_owned())
        };

        // The symbol table, with its strings.
        let mut symbols: Vec<Symbol> = Vec::new();
        if let Some(h) = headers.iter().find(|h| h.kind == SHT_SYMTAB) {
            let entsize = if h.entsize == 0 { if wide { 24 } else { 16 } } else { h.entsize };
            for i in 0..h.size / entsize {
                let at = entry_at(h, i, entsize, bytes.len())?;
                let name = reader.u32(at, "symbol name")?;
                let (info, value, size, section) = if wide {
                    (
                        reader.u8(at + 4, "symbol info")?,
                        reader.u64(at + 8, "symbol value")?,
                        reader.u64(at + 16, "symbol size")?,
                        reader.u16(at + 6, "symbol section")?,
                    )
                } else {
                    (
                        reader.u8(at + 12, "symbol info")?,
                        u64::from(reader.u32(at + 4, "symbol value")?),
                        u64::from(reader.u32(at + 8, "symbol size")?),
                        reader.u16(at + 14, "symbol section")?,
                    )
                };
                let name = string_at(h.link as usize, name)?;
                symbols.push(Symbol { name, value, size, kind: info & 0xF, global: info >> 4 != 0, section });
            }
        }

        let mut sections = Vec::new();
        let mut code_index: Vec<Option<usize>> = vec![None; shnum];
        for (number, h) in headers.iter().enumerate() {
            if h.flags & SHF_EXECINSTR == 0 || h.size == 0 || h.kind != 1 {
                continue;
            }
            let data = reader.slice(h.offset, h.size, "section data")?.to_vec();
            code_index[number] = Some(sections.len());
            sections.push(ObjectSection { name: string_at(shstrndx, h.name)?, bytes: data, relocations: Vec::new() });
        }
        for h in headers.iter().filter(|h| matches!(h.kind, SHT_REL | SHT_RELA)) {
            let Some(section) = code_index.get(h.info as usize).copied().flatten() else { continue };
            let rela = h.kind == SHT_RELA;
            let entsize =
                if h.entsize == 0 { (if wide { 16 } else { 8 }) + if rela { if wide { 8 } else { 4 } } else { 0 } } else { h.entsize };
            for i in 0..h.size / entsize {
                let at = entry_at(h, i, entsize, bytes.len())?;
                let (offset, sym, kind) = if wide {
                    let info = reader.u64(at + 8, "relocation info")?;
                    (reader.u64(at, "relocation offset")?, (info >> 32) as usize, (info & 0xFFFF_FFFF) as u32)
                } else {
                    let info = reader.u32(at + 4, "relocation info")?;
                    (u64::from(reader.u32(at, "relocation offset")?), (info >> 8) as usize, info & 0xFF)
                };
                if kind == 0 {
                    continue;
                }
                let symbol = symbols
                    .get(sym)
                    .filter(|s| s.kind != STT_SECTION && !s.name.is_empty() && !s.name.starts_with(".L"))
                    .map(|s| s.name.clone());
                sections[section].relocations.push(ObjectReloc { offset: offset as u32, size: reloc_size(machine, kind), symbol });
            }
        }
        for s in &mut sections {
            s.relocations.sort_by_key(|r| r.offset);
        }

        // The functions: the `FUNC` symbols of the code sections, each as long as its size says,
        // or to the next symbol when the size is zero.
        let mut starts: Vec<(usize, u64)> = symbols
            .iter()
            .filter(|s| !s.name.is_empty() && s.kind != STT_SECTION)
            .filter_map(|s| Some((code_index.get(s.section as usize).copied().flatten()?, s.value)))
            .collect();
        starts.sort_unstable();
        let mut functions = Vec::new();
        for s in symbols.iter().filter(|s| s.kind == STT_FUNC && !s.name.is_empty()) {
            let Some(section) = code_index.get(s.section as usize).copied().flatten() else { continue };
            let section_len = sections[section].bytes.len() as u64;
            let end = if s.size > 0 {
                s.value.saturating_add(s.size).min(section_len)
            } else {
                starts.iter().find(|&&(sec, v)| sec == section && v > s.value).map_or(section_len, |&(_, v)| v)
            };
            if end <= s.value {
                continue;
            }
            functions.push(ObjectFunction {
                name: s.name.clone(),
                section,
                offset: s.value as u32,
                size: (end - s.value) as u32,
                external: s.global,
            });
        }
        Ok(Some(Object { name: name.to_owned(), machine, elf: true, sections, functions }))
    }
}
