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
    /// The static library whose object holds the function (`libucrt.lib`), by its file name in
    /// lower case; `None` for an object of the program itself.
    pub library: Option<String>,
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

/// What the PDB tells about the image: the names of its functions and the ranges its libraries fill.
#[derive(Debug, Clone, Default)]
pub struct Pdb {
    pub symbols: Vec<Symbol>,
    pub libraries: Vec<LibraryRange>,
}

const MSF_MAGIC: &[u8; 32] = b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\0\0\0";
const S_PUB32: u16 = 0x110e;
const S_LPROC32: u16 = 0x110f;
const S_GPROC32: u16 = 0x1110;
const S_LPROC32_ID: u16 = 0x1146;
const S_GPROC32_ID: u16 = 0x1147;

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
    let library_of = |segment: u16, offset: u32| -> Option<String> {
        let module = dbi.contributions.iter().find(|c| c.segment == segment && offset >= c.offset && offset - c.offset < c.size)?.module;
        dbi.modules.get(usize::from(module))?.library.clone()
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
            if !matches!(kind, S_LPROC32 | S_GPROC32 | S_LPROC32_ID | S_GPROC32_ID) {
                continue;
            }
            let r = Reader::new(data);
            let (Ok(offset), Ok(segment), Ok(name)) = (r.u32(28, "offset"), r.u16(32, "segment"), r.cstr(35, 4096, "name")) else {
                continue;
            };
            let Some(va) = va_of(segment, offset) else { continue };
            out.push(Symbol { va, name: name.to_owned(), library: library_of(segment, offset) });
        }
    }
    // Then the public symbols, for the functions no module stream describes: the functions of a
    // library built without debug information.
    if let Some(stream) = dbi.symbol_record_stream
        && let Ok(bytes) = msf.stream(stream)
    {
        for (kind, data) in records(&bytes) {
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
            out.push(Symbol { va, name: name.to_owned(), library: library_of(segment, offset) });
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
    Ok(Pdb { symbols: out, libraries })
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
            modules.push(Module { symbol_stream, symbol_bytes, library });
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
