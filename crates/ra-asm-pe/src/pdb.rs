//! The PDB of a build: the names of its functions, and the library each came from.
//!
//! A PDB is an MSF container: fixed-size blocks, a stream directory that lists each stream's
//! blocks, and streams with fixed numbers. Stream 1 is the PDB information (the GUID and age the
//! image's `CodeView` record must match); stream 3 is the DBI, whose header names the symbol record
//! stream and whose substreams list the modules (one per object file, with the library it came
//! from) and the section contributions (which module owns each range of each section). The symbol
//! record stream holds the public symbols (`S_PUB32`: a decorated linkage name at a
//! segment:offset); each module's own stream holds its procedures (`S_GPROC32`, `S_LPROC32`: the
//! display name, the static functions included). Both name an address as `segment:offset`, where
//! the segment is a one-based index into the image's section table.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_format::Reader;
use serde::Serialize;

use crate::image::Image;

/// A function the PDB names.
#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub va: u64,
    /// The name as the PDB has it: a procedure's display name (`main`, `Rect::area`), or the
    /// decorated linkage name of a public symbol (`_main`, `?area@Rect@@UEBANXZ`) when the
    /// procedure record is not there.
    pub name: String,
    /// Whether `name` is a procedure's display name, the source's own (`__security_check_cookie`,
    /// `_alldiv`), rather than a linkage name a 32-bit build decorates (`___security_check_cookie`,
    /// `__alldiv`): nothing comes off it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub display: bool,
    /// The static library whose object holds the function (`libucrt.lib`), by its file name in
    /// lower case; `None` for an object of the program itself.
    pub library: Option<String>,
    /// The object file that holds the function (`core.obj`), by its file name as the build has
    /// it; `None` when the symbols do not say. The modules of a port follow it.
    pub object: Option<String>,
}

/// A range of the image that an object of a static library fills: a section contribution of a
/// module that came from a library.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryRange {
    pub va: u64,
    pub size: u32,
    /// The library's file name in lower case (`libucrt.lib`).
    pub library: String,
}

/// A variable the PDB names: a global (`S_GDATA32`) or a file's or a function's static
/// (`S_LDATA32`).
#[derive(Debug, Clone, Serialize)]
pub struct DataSymbol {
    pub va: u64,
    /// The name as the source has it (`ticdup`, `mainzone`).
    pub name: String,
    /// Its size in bytes, from its type; `None` for a type the reading does not size.
    pub size: Option<u32>,
    /// Whether its type is a pointer or an integer of 4 bytes or less, read as one word.
    pub scalar: bool,
}

/// What the PDB tells about the image: the names of its functions and its variables, and the ranges
/// its libraries fill.
#[derive(Debug, Clone, Default)]
pub struct Pdb {
    pub symbols: Vec<Symbol>,
    pub libraries: Vec<LibraryRange>,
    pub data: Vec<DataSymbol>,
}

const MSF_MAGIC: &[u8; 32] = b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\0\0\0";
const S_PUB32: u16 = 0x110e;
const S_LPROC32: u16 = 0x110f;
const S_GPROC32: u16 = 0x1110;
const S_LPROC32_ID: u16 = 0x1146;
const S_GPROC32_ID: u16 = 0x1147;
const S_LDATA32: u16 = 0x110c;
const S_GDATA32: u16 = 0x110d;

/// The functions of `image` that the PDB at `path` names, in address order, one per address, and
/// the ranges of the image its static libraries fill, in address order. The PDB must carry `guid`
/// and `age`, the image's `CodeView` record.
pub fn read(image: &Image, path: &Path, guid: &str, age: u32) -> Result<Pdb> {
    let file = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let msf = Msf::parse(&file)?;
    let info = msf.stream(1).context("the PDB information stream")?;
    let reader = Reader::new(&info);
    let pdb_age = reader.u32(8, "the PDB age")?;
    let g = reader.slice(12, 16, "the PDB GUID")?;
    let pdb_guid = format_guid(g);
    ensure!(
        pdb_guid.eq_ignore_ascii_case(guid),
        "{} is the PDB of another build: its GUID is {pdb_guid}, the image's {guid}",
        path.display()
    );
    ensure!(pdb_age == age, "{} is the PDB of another build: its age is {pdb_age}, the image's {age}", path.display());

    let dbi = msf.stream(3).context("the DBI stream")?;
    let dbi = Dbi::parse(&dbi)?;
    let va_of = |segment: u16, offset: u32| -> Option<u64> {
        let section = image.sections.get(usize::from(segment).checked_sub(1)?)?;
        let rva = section.virtual_address.checked_add(offset)?;
        section.contains_rva(rva).then(|| image.va(rva))
    };
    let module_of = |segment: u16, offset: u32| -> Option<&Module> {
        let module = dbi.contributions.iter().find(|c| c.segment == segment && offset >= c.offset && offset - c.offset < c.size)?.module;
        dbi.modules.get(usize::from(module))
    };
    let symbol = |va: u64, name: &str, display: bool, segment: u16, offset: u32| {
        let module = module_of(segment, offset);
        Symbol {
            va,
            name: name.to_owned(),
            display,
            library: module.and_then(|m| m.library.clone()),
            object: module.and_then(|m| m.object.clone()),
        }
    };

    // The types, for the sizes of the variables; a PDB without a readable type stream names its
    // variables all the same.
    let types = msf.stream(2).ok().and_then(|bytes| Types::parse(&bytes).ok()).unwrap_or_default();
    let mut data: Vec<DataSymbol> = Vec::new();
    let mut variable = |kind: u16, record: &[u8]| {
        if !matches!(kind, S_LDATA32 | S_GDATA32) {
            return;
        }
        let r = Reader::new(record);
        let (Ok(ty), Ok(offset), Ok(segment), Ok(name)) =
            (r.u32(0, "type"), r.u32(4, "offset"), r.u16(8, "segment"), r.cstr(10, 4096, "name"))
        else {
            return;
        };
        // The compiler's own data has no name.
        if name.is_empty() {
            return;
        }
        let Some(va) = va_of(segment, offset) else { return };
        data.push(DataSymbol { va, name: name.to_owned(), size: types.size(ty), scalar: types.scalar(ty) });
    };

    // The procedures first: a module's own record names a function as the source did.
    let mut out: Vec<Symbol> = Vec::new();
    for module in &dbi.modules {
        let Some(stream) = module.symbol_stream else { continue };
        let Ok(bytes) = msf.stream(stream) else { continue };
        let end = bytes.len().min(module.symbol_bytes as usize);
        if end < 4 {
            continue;
        }
        for (kind, data) in records(&bytes[4..end]) {
            variable(kind, data);
            if !matches!(kind, S_LPROC32 | S_GPROC32 | S_LPROC32_ID | S_GPROC32_ID) {
                continue;
            }
            let r = Reader::new(data);
            let (Ok(offset), Ok(segment), Ok(name)) = (r.u32(28, "offset"), r.u16(32, "segment"), r.cstr(35, 4096, "name")) else {
                continue;
            };
            let Some(va) = va_of(segment, offset) else { continue };
            out.push(symbol(va, name, true, segment, offset));
        }
    }
    // Then the public symbols, for the functions no module stream describes: the functions of a
    // library built without debug information.
    if let Some(stream) = dbi.symbol_record_stream
        && let Ok(bytes) = msf.stream(stream)
    {
        for (kind, data) in records(&bytes) {
            variable(kind, data);
            if kind != S_PUB32 {
                continue;
            }
            let r = Reader::new(data);
            let (Ok(flags), Ok(offset), Ok(segment), Ok(name)) =
                (r.u32(0, "flags"), r.u32(4, "offset"), r.u16(8, "segment"), r.cstr(10, 4096, "name"))
            else {
                continue;
            };
            // Bit 1 of the flags marks a function; the rest are data.
            if flags & 2 == 0 {
                continue;
            }
            let Some(va) = va_of(segment, offset) else { continue };
            out.push(symbol(va, name, false, segment, offset));
        }
    }
    // One name per address: the procedure record wins, being first.
    out.sort_by_key(|s| s.va);
    out.dedup_by_key(|s| s.va);
    let mut libraries: Vec<LibraryRange> = dbi
        .contributions
        .iter()
        .filter_map(|c| {
            let library = dbi.modules.get(usize::from(c.module))?.library.clone()?;
            Some(LibraryRange { va: va_of(c.segment, c.offset)?, size: c.size, library })
        })
        .collect();
    libraries.sort_by_key(|r| r.va);
    // One variable per address: a global's record wins over a static's of the same place.
    data.sort_by_key(|d| d.va);
    data.dedup_by_key(|d| d.va);
    Ok(Pdb { symbols: out, libraries, data })
}

/// The type stream (TPI), as far as the sizes of the variables need it: each record by its index.
#[derive(Default)]
struct Types {
    first: u32,
    records: Vec<(u16, Vec<u8>)>,
    /// The size of each structure, union and class by name, from its definition: a variable's
    /// type is often the forward reference.
    defined: std::collections::HashMap<String, u64>,
}

const LF_MODIFIER: u16 = 0x1001;
const LF_POINTER: u16 = 0x1002;
const LF_ARRAY: u16 = 0x1503;
const LF_CLASS: u16 = 0x1504;
const LF_STRUCTURE: u16 = 0x1505;
const LF_UNION: u16 = 0x1506;
const LF_ENUM: u16 = 0x1507;

impl Types {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let r = Reader::new(bytes);
        let header = r.u32(4, "the TPI header size")? as usize;
        let first = r.u32(8, "the first type index")?;
        let length = r.u32(16, "the type record bytes")? as usize;
        let body = r.slice(header, length.min(bytes.len().saturating_sub(header)), "the type records")?;
        let mut records = Vec::new();
        let mut pos = 0;
        while pos + 4 <= body.len() {
            let len = usize::from(u16::from_le_bytes([body[pos], body[pos + 1]]));
            let end = pos + 2 + len;
            if len < 2 || end > body.len() {
                break;
            }
            records.push((u16::from_le_bytes([body[pos + 2], body[pos + 3]]), body[pos + 4..end].to_vec()));
            pos = end;
        }
        let mut types = Self { first, records, defined: std::collections::HashMap::new() };
        for i in 0..types.records.len() {
            let (kind, data) = &types.records[i];
            if let Some((size, name, forward)) = Self::aggregate(*kind, data)
                && !forward
            {
                types.defined.entry(name).or_insert(size);
            }
        }
        Ok(types)
    }

    /// A structure's, a class's or a union's size, name, and whether the record is only a forward
    /// reference.
    fn aggregate(kind: u16, data: &[u8]) -> Option<(u64, String, bool)> {
        let r = Reader::new(data);
        let (props, at) = match kind {
            LF_STRUCTURE | LF_CLASS => (r.u16(2, "properties").ok()?, 16),
            LF_UNION => (r.u16(2, "properties").ok()?, 8),
            _ => return None,
        };
        let (size, used) = numeric(data.get(at..)?)?;
        let name = Reader::new(data.get(at + used..)?).cstr(0, 4096, "name").ok()?.to_owned();
        Some((size, name, props & 0x80 != 0))
    }

    fn record(&self, index: u32) -> Option<&(u16, Vec<u8>)> {
        self.records.get(index.checked_sub(self.first)? as usize)
    }

    /// The size of the type `index` in bytes.
    fn size(&self, index: u32) -> Option<u32> {
        self.size_at(index, 0).and_then(|s| u32::try_from(s).ok())
    }

    fn size_at(&self, index: u32, depth: u8) -> Option<u64> {
        if depth > 16 {
            return None;
        }
        if index < 0x1000 {
            return simple_size(index);
        }
        let (kind, data) = self.record(index)?;
        let r = Reader::new(data);
        match *kind {
            LF_MODIFIER => self.size_at(r.u32(0, "modified").ok()?, depth + 1),
            LF_POINTER => Some(u64::from((r.u32(4, "attributes").ok()? >> 13) & 0x3f)).filter(|&s| s > 0),
            LF_ARRAY => numeric(data.get(8..)?).map(|(size, _)| size),
            LF_ENUM => self.size_at(r.u32(4, "underlying").ok()?, depth + 1),
            LF_STRUCTURE | LF_CLASS | LF_UNION => {
                let (size, name, forward) = Self::aggregate(*kind, data)?;
                if forward { self.defined.get(&name).copied() } else { Some(size) }
            }
            _ => None,
        }
    }

    /// Whether the type `index` is a pointer or an integer of up to 4 bytes.
    fn scalar(&self, index: u32) -> bool {
        let mut index = index;
        for _ in 0..16 {
            if index < 0x1000 {
                return simple_size(index).is_some_and(|s| s <= 4) && (index & 0xff) != 0x40 && (index & 0xff) != 0x41;
            }
            let Some((kind, data)) = self.record(index) else { return false };
            match *kind {
                LF_MODIFIER | LF_ENUM => {
                    let at = if *kind == LF_MODIFIER { 0 } else { 4 };
                    let Ok(next) = Reader::new(data).u32(at, "type") else { return false };
                    index = next;
                }
                LF_POINTER => return self.size_at(index, 0).is_some_and(|s| s <= 4),
                _ => return false,
            }
        }
        false
    }
}

/// The size of a built-in type index: its mode (bits 8-11) makes it a pointer of 4 or 8 bytes,
/// else its kind (the low byte) says.
fn simple_size(index: u32) -> Option<u64> {
    match (index >> 8) & 0xf {
        0 => {}
        4 => return Some(4),
        6 => return Some(8),
        _ => return None,
    }
    Some(match index & 0xff {
        0x10 | 0x20 | 0x30 | 0x68 | 0x69 | 0x70 => 1,
        0x11 | 0x21 | 0x31 | 0x71 | 0x72 | 0x73 | 0x7a => 2,
        0x12 | 0x22 | 0x32 | 0x74 | 0x75 | 0x40 | 0x7b => 4,
        0x13 | 0x23 | 0x33 | 0x76 | 0x77 | 0x41 => 8,
        0x42 => 10,
        _ => return None,
    })
}

/// A numeric leaf: its value and the bytes it takes.
fn numeric(data: &[u8]) -> Option<(u64, usize)> {
    let r = Reader::new(data);
    let leaf = r.u16(0, "numeric leaf").ok()?;
    if leaf < 0x8000 {
        return Some((u64::from(leaf), 2));
    }
    Some(match leaf {
        0x8000 => (u64::from(r.u8(2, "char").ok()?), 3),
        0x8001 | 0x8002 => (u64::from(r.u16(2, "short").ok()?), 4),
        0x8003 | 0x8004 => (u64::from(r.u32(2, "long").ok()?), 6),
        0x8009 | 0x800a => (r.u64(2, "quad").ok()?, 10),
        _ => return None,
    })
}

/// A GUID as the `CodeView` record formats it.
pub(crate) fn format_guid(g: &[u8]) -> String {
    format!(
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
    )
}

/// The symbol records of a stream: each is a 16-bit length (of what follows it), a 16-bit kind
/// and the data. A length that runs past the end ends the walk.
fn records(bytes: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 4 > bytes.len() {
            return None;
        }
        let len = usize::from(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]));
        let kind = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        let end = pos + 2 + len;
        if len < 2 || end > bytes.len() {
            return None;
        }
        let data = &bytes[pos + 4..end];
        pos = end;
        Some((kind, data))
    })
}

/// The MSF container: the block size and the blocks of every stream.
struct Msf<'a> {
    file: &'a [u8],
    block_size: usize,
    streams: Vec<(usize, Vec<u32>)>,
}

impl<'a> Msf<'a> {
    fn parse(file: &'a [u8]) -> Result<Self> {
        let r = Reader::new(file);
        ensure!(r.slice(0, 32, "the MSF magic")? == MSF_MAGIC, "not a PDB 7.0 file: the MSF magic is missing");
        let block_size = r.u32(32, "the MSF block size")? as usize;
        ensure!(matches!(block_size, 512 | 1024 | 2048 | 4096 | 8192 | 16384 | 32768), "an MSF block size of {block_size:#x}");
        let directory_bytes = r.u32(44, "the directory size")? as usize;
        let block_map = r.u32(52, "the block map address")? as usize;
        let map_count = directory_bytes.div_ceil(block_size);
        let map = r.slice(block_map * block_size, map_count * 4, "the directory block map")?;
        let map_blocks: Vec<u32> = map.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let msf = Self { file, block_size, streams: Vec::new() };
        let directory = msf.read_blocks(&map_blocks, directory_bytes).context("the stream directory")?;
        let d = Reader::new(&directory);
        let count = d.u32(0, "the stream count")? as usize;
        ensure!(count < 1 << 20, "{count} streams");
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            let size = d.u32(4 + i * 4, "a stream size")?;
            sizes.push(if size == u32::MAX { 0 } else { size as usize });
        }
        let mut pos = 4 + count * 4;
        let mut streams = Vec::with_capacity(count);
        for size in sizes {
            let blocks = size.div_ceil(block_size);
            let list = d.slice(pos, blocks * 4, "a stream's block list")?;
            pos += blocks * 4;
            streams.push((size, list.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()));
        }
        Ok(Self { streams, ..msf })
    }

    fn read_blocks(&self, blocks: &[u32], size: usize) -> Result<Vec<u8>> {
        // The size is the file's word: the blocks that are there bound the allocation, and a block
        // listed past the size is not read.
        let mut out = Vec::with_capacity(size.min(blocks.len() * self.block_size));
        for &block in blocks.iter().take(size.div_ceil(self.block_size)) {
            let start = block as usize * self.block_size;
            let Some(bytes) = self.file.get(start..start + self.block_size) else {
                bail!("block {block} is past the end of the file ({:#x} bytes)", self.file.len())
            };
            out.extend_from_slice(bytes);
        }
        out.truncate(size);
        Ok(out)
    }

    fn stream(&self, index: u16) -> Result<Vec<u8>> {
        let Some((size, blocks)) = self.streams.get(usize::from(index)) else { bail!("no stream {index}") };
        self.read_blocks(blocks, *size)
    }
}

struct Module {
    symbol_stream: Option<u16>,
    symbol_bytes: u32,
    /// The library the object came from, when it came from one.
    library: Option<String>,
    /// The object file's name, for a module that is one.
    object: Option<String>,
}

struct Contribution {
    segment: u16,
    offset: u32,
    size: u32,
    module: u16,
}

/// What the DBI stream says about the modules and their code.
struct Dbi {
    symbol_record_stream: Option<u16>,
    modules: Vec<Module>,
    contributions: Vec<Contribution>,
}

impl Dbi {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let r = Reader::new(bytes);
        let signature = r.i32(0, "the DBI signature")?;
        ensure!(signature == -1, "a DBI stream with signature {signature}, not the current one");
        let stream = |v: u16| (v != u16::MAX).then_some(v);
        let symbol_record_stream = stream(r.u16(20, "the symbol record stream")?);
        let module_info_size = r.i32(24, "the module info size")?.max(0) as usize;
        let contribution_size = r.i32(28, "the section contribution size")?.max(0) as usize;

        let mut modules = Vec::new();
        let module_info = r.slice(64, module_info_size, "the module info substream")?;
        let m = Reader::new(module_info);
        let mut pos = 0;
        while pos + 64 <= module_info.len() {
            let symbol_stream = stream(m.u16(pos + 34, "a module's symbol stream")?);
            let symbol_bytes = m.u32(pos + 36, "a module's symbol size")?;
            let name = m.cstr(pos + 64, 4096, "a module name")?;
            let object = m.cstr(pos + 64 + name.len() + 1, 4096, "an object file name")?;
            pos = (pos + 64 + name.len() + 1 + object.len() + 1).next_multiple_of(4);
            // A library member names its library as the object file; a plain object names itself.
            let library =
                (object != name && !object.is_empty()).then(|| object.rsplit(['\\', '/']).next().unwrap_or(object).to_ascii_lowercase());
            // The linker's own modules (`* Linker *`, an import's `Import:KERNEL32.dll`) are no
            // object files.
            let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
            let object = Path::new(file).extension().is_some_and(|e| e.eq_ignore_ascii_case("obj") || e.eq_ignore_ascii_case("o"));
            let object = object.then(|| file.to_owned());
            modules.push(Module { symbol_stream, symbol_bytes, library, object });
        }

        let mut contributions = Vec::new();
        let table = r.slice(64 + module_info_size, contribution_size, "the section contribution substream")?;
        if table.len() >= 4 {
            let version = u32::from_le_bytes([table[0], table[1], table[2], table[3]]);
            let entry = match version {
                0xf12e_ba2d => 28,
                0xf131_51e5 => 32,
                other => bail!("a section contribution table of version {other:#x}"),
            };
            let t = Reader::new(table);
            let mut pos = 4;
            while pos + entry <= table.len() {
                contributions.push(Contribution {
                    segment: t.u16(pos, "a contribution's section")?,
                    offset: t.u32(pos + 4, "a contribution's offset")?,
                    size: t.u32(pos + 8, "a contribution's size")?,
                    module: t.u16(pos + 16, "a contribution's module")?,
                });
                pos += entry;
            }
        }
        Ok(Self { symbol_record_stream, modules, contributions })
    }
}
