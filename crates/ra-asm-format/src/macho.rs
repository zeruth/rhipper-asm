//! The Mach-O container of macOS, 64-bit and little-endian, thin or fat: the load commands, the
//! segments that map the file by virtual address and their sections, the symbol table, the
//! libraries the file loads, the bindings of its imports (the opcodes of `LC_DYLD_INFO`, or the
//! chained fixups of today's linkers, `LC_DYLD_CHAINED_FIXUPS`), its rebases, its export trie,
//! and the function extents `__unwind_info` records, cut at the starts `LC_FUNCTION_STARTS` lists.
//! A fat file holds one such file per architecture; the reader takes one slice of it. What the
//! container means to the pipeline is the loader's to say; this reads what the file holds, and
//! every refusal names the field and its offset.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};

use crate::Reader;

pub const MH_MAGIC_64: u32 = 0xfeed_facf;
const MH_MAGIC: u32 = 0xfeed_face;
const MH_CIGAM_64: u32 = 0xcffa_edfe;
const MH_CIGAM: u32 = 0xcefa_edfe;
/// The magic of a fat file, whose header is big-endian: `0xcafebabe`, or `0xcafebabf` for the
/// layout with 64-bit offsets.
const FAT_MAGIC: [u8; 4] = [0xca, 0xfe, 0xba, 0xbe];
const FAT_MAGIC_64: [u8; 4] = [0xca, 0xfe, 0xba, 0xbf];
/// The most slices a fat file is taken to hold. A Java class file opens on the same magic, with
/// its version (45 and up) where the slice count would be.
const MAX_FAT_ARCHS: u32 = 20;

pub const CPU_TYPE_X86: u32 = 7;
pub const CPU_TYPE_ARM: u32 = 12;
const CPU_ARCH_ABI64: u32 = 0x0100_0000;
pub const CPU_TYPE_X86_64: u32 = CPU_TYPE_X86 | CPU_ARCH_ABI64;
pub const CPU_TYPE_ARM64: u32 = CPU_TYPE_ARM | CPU_ARCH_ABI64;

pub const MH_EXECUTE: u32 = 2;
pub const MH_DYLINKER: u32 = 7;
pub const MH_DYLIB: u32 = 6;
pub const MH_BUNDLE: u32 = 8;
/// `MH_PIE`: the executable loads at a random base.
pub const MH_PIE: u32 = 0x0020_0000;

pub const VM_PROT_READ: u32 = 1;
pub const VM_PROT_WRITE: u32 = 2;
pub const VM_PROT_EXECUTE: u32 = 4;

const SECTION_TYPE: u32 = 0xff;
const S_ZEROFILL: u32 = 1;
const S_GB_ZEROFILL: u32 = 0xc;
const S_THREAD_LOCAL_ZEROFILL: u32 = 0x12;
const S_ATTR_PURE_INSTRUCTIONS: u32 = 0x8000_0000;
const S_ATTR_SOME_INSTRUCTIONS: u32 = 0x400;

const N_STAB: u8 = 0xe0;
const N_TYPE: u8 = 0x0e;
const N_SECT: u8 = 0x0e;
const N_EXT: u8 = 0x01;

const LC_REQ_DYLD: u32 = 0x8000_0000;
const LC_SYMTAB: u32 = 0x2;
const LC_UNIXTHREAD: u32 = 0x5;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_ID_DYLIB: u32 = 0xd;
const LC_LOAD_DYLINKER: u32 = 0xe;
const LC_LOAD_WEAK_DYLIB: u32 = 0x18 | LC_REQ_DYLD;
const LC_SEGMENT_64: u32 = 0x19;
const LC_REEXPORT_DYLIB: u32 = 0x1f | LC_REQ_DYLD;
const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x22 | LC_REQ_DYLD;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x23 | LC_REQ_DYLD;
const LC_FUNCTION_STARTS: u32 = 0x26;
const LC_MAIN: u32 = 0x28 | LC_REQ_DYLD;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x33 | LC_REQ_DYLD;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x34 | LC_REQ_DYLD;

/// The thread states of `LC_UNIXTHREAD` the reader takes the entry from, with the offset of the
/// program counter in each.
const X86_THREAD_STATE64: u32 = 4;
const X86_64_RIP: usize = 16 * 8;
const ARM_THREAD_STATE64: u32 = 6;
const ARM64_PC: usize = 32 * 8;

/// The pointer formats of the chained fixups the reader walks.
const DYLD_CHAINED_PTR_ARM64E: u16 = 1;
const DYLD_CHAINED_PTR_64: u16 = 2;
const DYLD_CHAINED_PTR_64_OFFSET: u16 = 6;
const DYLD_CHAINED_PTR_ARM64E_USERLAND: u16 = 9;
const DYLD_CHAINED_PTR_ARM64E_USERLAND24: u16 = 12;
const DYLD_CHAINED_PTR_START_NONE: u16 = 0xffff;
const DYLD_CHAINED_PTR_START_MULTI: u16 = 0x8000;

const EXPORT_SYMBOL_FLAGS_KIND_MASK: u64 = 3;
const EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE: u64 = 2;
const EXPORT_SYMBOL_FLAGS_REEXPORT: u64 = 8;

/// The bound of the tables the reader walks: a real file is far below it, a corrupt count is not a
/// reason to loop for long.
const MAX_ENTRIES: usize = 1 << 20;

/// The width of a pointer the opcodes of `LC_DYLD_INFO` step by: every file here is 64-bit.
const POINTER: u64 = 8;

/// One slice of a fat file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatArch {
    pub cpu_type: u32,
    pub cpu_subtype: u32,
    pub offset: u64,
    pub size: u64,
}

/// One `LC_SEGMENT_64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub name: String,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
    pub maxprot: u32,
    pub initprot: u32,
    pub flags: u32,
}

/// One section of a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub segment: String,
    pub addr: u64,
    pub size: u64,
    pub offset: u32,
    pub flags: u32,
    pub reserved1: u32,
    pub reserved2: u32,
}

impl Section {
    /// `S_*`, the low byte of the flags.
    pub fn kind(&self) -> u32 {
        self.flags & SECTION_TYPE
    }

    /// Whether the section is zeros the file does not hold.
    pub fn is_zerofill(&self) -> bool {
        matches!(self.kind(), S_ZEROFILL | S_GB_ZEROFILL | S_THREAD_LOCAL_ZEROFILL)
    }

    /// Whether the section holds machine instructions.
    pub fn has_instructions(&self) -> bool {
        self.flags & (S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS) != 0
    }

    pub fn contains(&self, va: u64) -> bool {
        va >= self.addr && va - self.addr < self.size
    }
}

/// One `nlist_64` record of the symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The name as the file holds it: a C name with its leading underscore.
    pub name: String,
    pub value: u64,
    /// `n_type`.
    pub kind: u8,
    /// `n_sect`: the index of the section that defines the symbol, from 1 in the file's order.
    pub section: u8,
    pub desc: u16,
}

impl Symbol {
    /// Whether the file defines the symbol in one of its sections; a debugging record never.
    pub fn is_defined(&self) -> bool {
        self.kind & N_STAB == 0 && self.kind & N_TYPE == N_SECT && self.section != 0
    }

    pub fn is_external(&self) -> bool {
        self.kind & N_EXT != 0
    }
}

/// A pointer dyld binds to a symbol of another image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub address: u64,
    /// The symbol's name as the file holds it.
    pub name: String,
    /// The install name of the library the two-level namespace binds it from; `None` for a flat
    /// or a weak lookup, or the image itself.
    pub library: Option<String>,
    pub addend: i64,
    pub weak: bool,
    /// Bound at the first call through a stub rather than at load.
    pub lazy: bool,
}

/// A symbol the export trie offers another image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub address: u64,
}

/// What a 64-bit Mach-O file holds, as read.
#[derive(Debug, Clone)]
pub struct MachOFile {
    pub cpu_type: u32,
    pub cpu_subtype: u32,
    /// `filetype`: [`MH_EXECUTE`], [`MH_DYLIB`], ...
    pub kind: u32,
    pub flags: u32,
    /// The slices of the fat file the reader took this one from; empty for a thin file.
    pub fat: Vec<FatArch>,
    /// The offset and the size of this slice in the fat file; `None` for a thin file. Every file
    /// offset here is the slice's.
    pub slice: Option<(u64, u64)>,
    /// The bytes of the Mach header and the load commands.
    pub headers_size: u64,
    /// The segments, in the order of their load commands: the order the bind opcodes and the
    /// chained fixups index them by.
    pub segments: Vec<Segment>,
    /// Every section of every segment, in the file's order: the order `n_sect` counts from 1.
    pub sections: Vec<Section>,
    /// The virtual address of the entry: `LC_MAIN`'s, else the program counter of `LC_UNIXTHREAD`.
    pub entry: Option<u64>,
    /// The dynamic linker, `LC_LOAD_DYLINKER`.
    pub dylinker: Option<String>,
    /// The libraries the file loads, in the order the library ordinals count from 1.
    pub dylibs: Vec<String>,
    /// A library's own install name, `LC_ID_DYLIB`.
    pub install_name: Option<String>,
    /// The symbol table, in its order.
    pub symbols: Vec<Symbol>,
    /// The pointers dyld binds, in address order.
    pub bindings: Vec<Binding>,
    /// The pointers dyld slides with the image, in address order.
    pub rebases: Vec<u64>,
    /// The symbols of the export trie, in address order.
    pub exports: Vec<Export>,
    /// Whether the bindings and rebases came from `LC_DYLD_CHAINED_FIXUPS` rather than `LC_DYLD_INFO`.
    pub chained_fixups: bool,
    /// The starts of the functions `LC_FUNCTION_STARTS` lists, in address order.
    pub function_starts: Vec<u64>,
    /// The functions: the ranges of `__unwind_info` cut at the function starts, `(begin, end)` in
    /// address order.
    pub functions: Vec<(u64, u64)>,
    /// What the reader could not read beyond the headers: a table that fails lands here.
    pub warnings: Vec<String>,
}

/// Whether `bytes` open on a 64-bit Mach-O file, or on a fat file of them.
pub fn is_macho(bytes: &[u8]) -> bool {
    let r = Reader::new(bytes);
    match r.u32(0, "magic") {
        Ok(MH_MAGIC_64 | MH_MAGIC | MH_CIGAM_64 | MH_CIGAM) => true,
        _ => is_fat(bytes),
    }
}

fn is_fat(bytes: &[u8]) -> bool {
    (bytes.starts_with(&FAT_MAGIC) || bytes.starts_with(&FAT_MAGIC_64))
        && be32(Reader::new(bytes), 4, "nfat_arch").is_ok_and(|n| (1..=MAX_FAT_ARCHS).contains(&n))
}

/// The name of a CPU type, as `ras` names a machine.
pub fn cpu_name(cpu_type: u32) -> String {
    match cpu_type {
        CPU_TYPE_X86_64 => "x86_64".to_owned(),
        CPU_TYPE_ARM64 => "arm64".to_owned(),
        CPU_TYPE_X86 => "i386".to_owned(),
        CPU_TYPE_ARM => "arm".to_owned(),
        other => format!("cpu {other:#x}"),
    }
}

fn be32(r: Reader<'_>, offset: usize, what: &str) -> Result<u32> {
    Ok(r.u32(offset, what)?.swap_bytes())
}

fn be64(r: Reader<'_>, offset: usize, what: &str) -> Result<u64> {
    Ok(r.u64(offset, what)?.swap_bytes())
}

/// The slices of a fat file.
fn fat_archs(r: Reader<'_>) -> Result<Vec<FatArch>> {
    let wide = r.bytes().starts_with(&FAT_MAGIC_64);
    let count = be32(r, 4, "nfat_arch")? as usize;
    let mut archs = Vec::with_capacity(count);
    for i in 0..count {
        let what = |field: &str| format!("fat slice {i} {field}");
        archs.push(if wide {
            let at = 8 + 32 * i;
            FatArch {
                cpu_type: be32(r, at, &what("cputype"))?,
                cpu_subtype: be32(r, at + 4, &what("cpusubtype"))?,
                offset: be64(r, at + 8, &what("offset"))?,
                size: be64(r, at + 16, &what("size"))?,
            }
        } else {
            let at = 8 + 20 * i;
            FatArch {
                cpu_type: be32(r, at, &what("cputype"))?,
                cpu_subtype: be32(r, at + 4, &what("cpusubtype"))?,
                offset: u64::from(be32(r, at + 8, &what("offset"))?),
                size: u64::from(be32(r, at + 12, &what("size"))?),
            }
        });
    }
    Ok(archs)
}

/// The NUL-terminated string at `offset`, ending by `end`, and its length in bytes; a byte that
/// is not UTF-8 becomes U+FFFD, since a name is a label.
fn cstring(r: Reader<'_>, offset: usize, end: usize, what: &str) -> Result<(String, usize)> {
    let end = end.min(r.len());
    ensure!(offset < end, "{what} at offset {offset:#x} lies past the end of its table ({end:#x})");
    let window = &r.bytes()[offset..end];
    let Some(len) = window.iter().position(|&b| b == 0) else { bail!("{what} at offset {offset:#x} has no terminator before {end:#x}") };
    Ok((String::from_utf8_lossy(&window[..len]).into_owned(), len))
}

/// An unsigned LEB128 number at `cursor`, which advances past it.
fn uleb(r: Reader<'_>, cursor: &mut usize, what: &str) -> Result<u64> {
    let start = *cursor;
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = r.u8(*cursor, what)?;
        *cursor += 1;
        if shift < 64 {
            value |= u64::from(byte & 0x7f) << shift;
        }
        shift += 7;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        ensure!(shift < 70, "{what} at offset {start:#x} is longer than ten bytes");
    }
}

/// A signed LEB128 number at `cursor`, which advances past it.
fn sleb(r: Reader<'_>, cursor: &mut usize, what: &str) -> Result<i64> {
    let start = *cursor;
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = r.u8(*cursor, what)?;
        *cursor += 1;
        if shift < 64 {
            value |= u64::from(byte & 0x7f) << shift;
        }
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 64 && byte & 0x40 != 0 {
                value |= u64::MAX << shift;
            }
            return Ok(value as i64);
        }
        ensure!(shift < 70, "{what} at offset {start:#x} is longer than ten bytes");
    }
}

/// A `linkedit_data_command`'s table: its offset and size, checked against the file.
fn linkedit(r: Reader<'_>, at: usize, what: &str) -> Result<(usize, usize)> {
    let offset = r.u32(at + 8, &format!("{what} dataoff"))? as usize;
    let size = r.u32(at + 12, &format!("{what} datasize"))? as usize;
    ensure!(offset.checked_add(size).is_some_and(|end| end <= r.len()), "{what} at offset {offset:#x} ({size:#x} bytes) runs past the end of the file");
    Ok((offset, size))
}

/// The export trie `LC_DYLD_INFO` points at: its offset and size.
fn dyld_info_exports(r: Reader<'_>, at: usize) -> Result<(usize, usize)> {
    let offset = r.u32(at + 40, "the LC_DYLD_INFO export_off")? as usize;
    let size = r.u32(at + 44, "the LC_DYLD_INFO export_size")? as usize;
    Ok((offset, size))
}

/// The tables the load commands point at, read once the segments are known.
#[derive(Default)]
struct Tables {
    symtab: Option<usize>,
    dyld_info: Option<usize>,
    chained_fixups: Option<usize>,
    exports_trie: Option<usize>,
    function_starts: Option<usize>,
    main: Option<usize>,
    thread: Option<usize>,
}

/// One import of the chained fixups: its name, its library's ordinal, whether it is weak, and its addend.
struct ChainedImport {
    name: String,
    ordinal: i64,
    weak: bool,
    addend: i64,
}

impl MachOFile {
    /// Reads the headers and the tables of a Mach-O file, or of the slice of a fat file the
    /// pipeline reads best: x86-64, else arm64, else the first 64-bit one. The headers must read;
    /// a table that does not (the symbols, the bindings, the exports, the extents) leaves a warning
    /// and stays empty.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if !is_fat(bytes) {
            return Self::parse_thin(bytes);
        }
        let fat = fat_archs(Reader::new(bytes))?;
        let chosen = [CPU_TYPE_X86_64, CPU_TYPE_ARM64]
            .iter()
            .find_map(|&cpu| fat.iter().find(|a| a.cpu_type == cpu))
            .or_else(|| fat.iter().find(|a| a.cpu_type & CPU_ARCH_ABI64 != 0))
            .copied();
        let Some(arch) = chosen else { bail!("the fat file holds no 64-bit slice") };
        let name = cpu_name(arch.cpu_type);
        let range = usize::try_from(arch.offset)
            .ok()
            .zip(usize::try_from(arch.size).ok())
            .and_then(|(start, size)| Some(start..start.checked_add(size)?))
            .filter(|r| r.end <= bytes.len());
        let Some(range) = range else {
            bail!("the fat file's {name} slice at offset {:#x} ({:#x} bytes) runs past the end of the file", arch.offset, arch.size)
        };
        let mut file = Self::parse_thin(&bytes[range]).with_context(|| format!("the fat file's {name} slice at offset {:#x}", arch.offset))?;
        file.fat = fat;
        file.slice = Some((arch.offset, arch.size));
        Ok(file)
    }

    fn parse_thin(bytes: &[u8]) -> Result<Self> {
        let r = Reader::new(bytes);
        match r.u32(0, "the Mach-O magic")? {
            MH_MAGIC_64 => {}
            MH_MAGIC => bail!("the Mach-O file is 32-bit (magic {MH_MAGIC:#x} at offset 0x0); the loader reads 64-bit ones"),
            MH_CIGAM_64 | MH_CIGAM => bail!("the Mach-O file is big-endian (magic at offset 0x0); the loader reads little-endian ones"),
            other => bail!("the magic {other:#010x} at offset 0x0 is not a Mach-O one"),
        }
        let cpu_type = r.u32(4, "cputype")?;
        let cpu_subtype = r.u32(8, "cpusubtype")?;
        let kind = r.u32(12, "filetype")?;
        let ncmds = r.u32(16, "ncmds")? as usize;
        let sizeofcmds = r.u32(20, "sizeofcmds")? as usize;
        let flags = r.u32(24, "flags")?;
        ensure!(ncmds <= MAX_ENTRIES, "the Mach header at offset 0x0 names {ncmds} load commands");
        let commands_end = 32 + sizeofcmds;
        ensure!(commands_end <= r.len(), "the load commands ({sizeofcmds:#x} bytes from offset 0x20) run past the end of the file");

        let mut file = Self {
            cpu_type,
            cpu_subtype,
            kind,
            flags,
            fat: Vec::new(),
            slice: None,
            headers_size: commands_end as u64,
            segments: Vec::new(),
            sections: Vec::new(),
            entry: None,
            dylinker: None,
            dylibs: Vec::new(),
            install_name: None,
            symbols: Vec::new(),
            bindings: Vec::new(),
            rebases: Vec::new(),
            exports: Vec::new(),
            chained_fixups: false,
            function_starts: Vec::new(),
            functions: Vec::new(),
            warnings: Vec::new(),
        };
        let mut tables = Tables::default();
        let mut at = 32;
        for i in 0..ncmds {
            let cmd = r.u32(at, &format!("load command {i} cmd"))?;
            let size = r.u32(at + 4, &format!("load command {i} cmdsize"))? as usize;
            ensure!(
                size >= 8 && at + size <= commands_end,
                "load command {i} at offset {at:#x} has the size {size:#x}, which leaves the {sizeofcmds:#x} bytes of the load commands"
            );
            let string = |what: &str| -> Result<String> {
                let name = r.u32(at + 8, &format!("{what} name offset"))? as usize;
                Ok(cstring(r, at + name, at + size, what)?.0)
            };
            match cmd {
                LC_SEGMENT_64 => file.read_segment(r, at, size, i)?,
                LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB | LC_LAZY_LOAD_DYLIB | LC_LOAD_UPWARD_DYLIB => {
                    file.dylibs.push(string(&format!("load command {i} library"))?);
                }
                LC_ID_DYLIB => file.install_name = Some(string("the LC_ID_DYLIB")?),
                LC_LOAD_DYLINKER => file.dylinker = Some(string("the LC_LOAD_DYLINKER")?),
                LC_SYMTAB => tables.symtab = Some(at),
                LC_DYLD_INFO | LC_DYLD_INFO_ONLY => tables.dyld_info = Some(at),
                LC_DYLD_CHAINED_FIXUPS => tables.chained_fixups = Some(at),
                LC_DYLD_EXPORTS_TRIE => tables.exports_trie = Some(at),
                LC_FUNCTION_STARTS => tables.function_starts = Some(at),
                LC_MAIN => tables.main = Some(at),
                LC_UNIXTHREAD => tables.thread = Some(at),
                _ => {}
            }
            at += size;
        }
        file.read_entry(r, &tables)?;
        file.read_tables(r, &tables);
        Ok(file)
    }

    fn read_segment(&mut self, r: Reader<'_>, at: usize, size: usize, index: usize) -> Result<()> {
        let what = |field: &str| format!("load command {index} {field}");
        let nsects = r.u32(at + 64, &what("nsects"))? as usize;
        ensure!(72 + nsects * 80 <= size, "load command {index} at offset {at:#x} holds {nsects} sections in {size:#x} bytes");
        let segment = Segment {
            name: r.fixed_str(at + 8, 16, &what("segname"))?,
            vmaddr: r.u64(at + 24, &what("vmaddr"))?,
            vmsize: r.u64(at + 32, &what("vmsize"))?,
            fileoff: r.u64(at + 40, &what("fileoff"))?,
            filesize: r.u64(at + 48, &what("filesize"))?,
            maxprot: r.u32(at + 56, &what("maxprot"))?,
            initprot: r.u32(at + 60, &what("initprot"))?,
            flags: r.u32(at + 68, &what("flags"))?,
        };
        for s in 0..nsects {
            let at = at + 72 + s * 80;
            let what = |field: &str| format!("load command {index} section {s} {field}");
            self.sections.push(Section {
                name: r.fixed_str(at, 16, &what("sectname"))?,
                segment: r.fixed_str(at + 16, 16, &what("segname"))?,
                addr: r.u64(at + 32, &what("addr"))?,
                size: r.u64(at + 40, &what("size"))?,
                offset: r.u32(at + 48, &what("offset"))?,
                flags: r.u32(at + 64, &what("flags"))?,
                reserved1: r.u32(at + 68, &what("reserved1"))?,
                reserved2: r.u32(at + 72, &what("reserved2"))?,
            });
        }
        self.segments.push(segment);
        Ok(())
    }

    /// The entry: `LC_MAIN` names it by file offset, `LC_UNIXTHREAD` by the initial program counter.
    fn read_entry(&mut self, r: Reader<'_>, tables: &Tables) -> Result<()> {
        if let Some(at) = tables.main {
            let offset = r.u64(at + 8, "the LC_MAIN entryoff")?;
            match self.segments.iter().find(|s| offset >= s.fileoff && offset - s.fileoff < s.filesize) {
                Some(s) => self.entry = Some(s.vmaddr + (offset - s.fileoff)),
                None => self.warnings.push(format!("the LC_MAIN entry at file offset {offset:#x} lies in no segment")),
            }
        } else if let Some(at) = tables.thread {
            let flavor = r.u32(at + 8, "the LC_UNIXTHREAD flavor")?;
            let pc = match flavor {
                X86_THREAD_STATE64 => Some(at + 16 + X86_64_RIP),
                ARM_THREAD_STATE64 => Some(at + 16 + ARM64_PC),
                _ => None,
            };
            match pc {
                Some(pc) => self.entry = Some(r.u64(pc, "the LC_UNIXTHREAD program counter")?),
                None => self.warnings.push(format!("the LC_UNIXTHREAD at offset {at:#x} has the thread state flavor {flavor}, not read")),
            }
        }
        Ok(())
    }

    fn read_tables(&mut self, r: Reader<'_>, tables: &Tables) {
        if let Some(at) = tables.symtab {
            match self.read_symbols(r, at) {
                Ok(symbols) => self.symbols = symbols,
                Err(e) => self.warnings.push(format!("symbol table: {e:#}")),
            }
        }
        if let Some(at) = tables.chained_fixups {
            self.chained_fixups = true;
            if let Err(e) = self.read_chained_fixups(r, at) {
                self.warnings.push(format!("chained fixups: {e:#}"));
            }
        } else if let Some(at) = tables.dyld_info {
            if let Err(e) = self.read_dyld_info(r, at) {
                self.warnings.push(format!("dyld info: {e:#}"));
            }
        }
        let trie = match (tables.exports_trie, tables.dyld_info) {
            (Some(at), _) => linkedit(r, at, "the LC_DYLD_EXPORTS_TRIE").map(Some),
            (None, Some(at)) => dyld_info_exports(r, at).map(Some),
            (None, None) => Ok(None),
        };
        let exports = match trie {
            Ok(Some((offset, size))) if size != 0 => self.read_exports(r, offset, size),
            Ok(_) => Ok(Vec::new()),
            Err(e) => Err(e),
        };
        match exports {
            Ok(exports) => self.exports = exports,
            Err(e) => self.warnings.push(format!("export trie: {e:#}")),
        }
        if let Some(at) = tables.function_starts {
            match self.read_function_starts(r, at) {
                Ok(starts) => self.function_starts = starts,
                Err(e) => self.warnings.push(format!("function starts: {e:#}")),
            }
        }
        match self.read_unwind_info(r) {
            Ok(unwind) => self.functions = self.extents(&unwind),
            Err(e) => {
                self.warnings.push(format!("__unwind_info: {e:#}"));
                self.functions = self.extents(&[]);
            }
        }
        self.bindings.sort_by_key(|b| b.address);
        self.rebases.sort_unstable();
        self.rebases.dedup();
    }

    /// The address the Mach header loads at, from which the tables of `__LINKEDIT` count their
    /// offsets: the segment that maps the start of the file.
    pub fn header_address(&self) -> u64 {
        self.segments.iter().find(|s| s.fileoff == 0 && s.filesize != 0).map_or(0, |s| s.vmaddr)
    }

    /// The file offset of virtual address `va`, when a segment holds it in the file's bytes.
    pub fn offset_of(&self, va: u64) -> Option<u64> {
        self.segments.iter().find(|s| va >= s.vmaddr && va - s.vmaddr < s.filesize).map(|s| s.fileoff + (va - s.vmaddr))
    }

    /// The section called `name` in the segment called `segment`.
    pub fn section(&self, segment: &str, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.segment == segment && s.name == name)
    }

    /// The install name of the library of a two-level ordinal: from 1, in the order of the load
    /// commands; zero and below are the image itself and the flat and weak lookups.
    fn library(&self, ordinal: i64) -> Option<String> {
        usize::try_from(ordinal).ok().filter(|&o| o > 0).and_then(|o| self.dylibs.get(o - 1).cloned())
    }

    fn read_symbols(&self, r: Reader<'_>, at: usize) -> Result<Vec<Symbol>> {
        let symoff = r.u32(at + 8, "the LC_SYMTAB symoff")? as usize;
        let nsyms = r.u32(at + 12, "the LC_SYMTAB nsyms")? as usize;
        let stroff = r.u32(at + 16, "the LC_SYMTAB stroff")? as usize;
        let strsize = r.u32(at + 20, "the LC_SYMTAB strsize")? as usize;
        ensure!(nsyms <= MAX_ENTRIES, "the LC_SYMTAB names {nsyms} symbols");
        let mut symbols = Vec::with_capacity(nsyms);
        for i in 0..nsyms {
            let at = symoff + 16 * i;
            let what = |field: &str| format!("symbol {i} {field}");
            let strx = r.u32(at, &what("n_strx"))? as usize;
            let name = if strx == 0 { String::new() } else { cstring(r, stroff + strx, stroff + strsize, &what("name"))?.0 };
            symbols.push(Symbol {
                name,
                kind: r.u8(at + 4, &what("n_type"))?,
                section: r.u8(at + 5, &what("n_sect"))?,
                desc: r.u16(at + 6, &what("n_desc"))?,
                value: r.u64(at + 8, &what("n_value"))?,
            });
        }
        Ok(symbols)
    }

    /// The rebases and the bindings of the opcodes `LC_DYLD_INFO` points at: the binds at load, and
    /// the lazy ones the stubs bind at their first call. A weak bind coalesces a definition rather
    /// than import one, so it is not read.
    fn read_dyld_info(&mut self, r: Reader<'_>, at: usize) -> Result<()> {
        let table = |field: usize, what: &str| -> Result<(usize, usize)> {
            let offset = r.u32(at + field, &format!("the LC_DYLD_INFO {what}_off"))? as usize;
            let size = r.u32(at + field + 4, &format!("the LC_DYLD_INFO {what}_size"))? as usize;
            ensure!(
                offset.checked_add(size).is_some_and(|end| end <= r.len()),
                "the {what} opcodes at offset {offset:#x} ({size:#x} bytes) run past the end of the file"
            );
            Ok((offset, size))
        };
        let rebase = table(8, "rebase")?;
        let bind = table(16, "bind")?;
        let lazy = table(32, "lazy_bind")?;
        self.rebases = self.read_rebases(r, rebase.0, rebase.0 + rebase.1)?;
        let mut bindings = self.read_binds(r, bind.0, bind.0 + bind.1, false)?;
        bindings.extend(self.read_binds(r, lazy.0, lazy.0 + lazy.1, true)?);
        self.bindings = bindings;
        Ok(())
    }

    /// The address of `offset` in segment `index`, as an opcode at `at` names it.
    fn segment_address(&self, segment: Option<usize>, offset: u64, at: usize) -> Result<u64> {
        let Some(index) = segment else { bail!("the opcode at offset {at:#x} acts before any opcode names a segment") };
        let Some(s) = self.segments.get(index) else { bail!("the opcode at offset {at:#x} names segment {index}, of {}", self.segments.len()) };
        Ok(s.vmaddr.wrapping_add(offset))
    }

    fn read_rebases(&self, r: Reader<'_>, start: usize, end: usize) -> Result<Vec<u64>> {
        let what = "a rebase opcode";
        let mut rebases = Vec::new();
        let (mut segment, mut offset) = (None, 0u64);
        let mut cursor = start;
        while cursor < end {
            let at = cursor;
            let byte = r.u8(cursor, what)?;
            cursor += 1;
            let imm = u64::from(byte & 0x0f);
            let (times, skip) = match byte & 0xf0 {
                0x00 => break,
                0x10 => continue,
                0x20 => {
                    segment = Some(imm as usize);
                    offset = uleb(r, &mut cursor, what)?;
                    continue;
                }
                0x30 => {
                    offset = offset.wrapping_add(uleb(r, &mut cursor, what)?);
                    continue;
                }
                0x40 => {
                    offset = offset.wrapping_add(imm * POINTER);
                    continue;
                }
                0x50 => (imm, 0),
                0x60 => (uleb(r, &mut cursor, what)?, 0),
                0x70 => (1, uleb(r, &mut cursor, what)?),
                0x80 => {
                    let times = uleb(r, &mut cursor, what)?;
                    (times, uleb(r, &mut cursor, what)?)
                }
                _ => bail!("the rebase opcode {byte:#04x} at offset {at:#x} is unknown"),
            };
            ensure!(times <= (MAX_ENTRIES - rebases.len()) as u64, "the rebase opcode at offset {at:#x} repeats {times} times");
            for _ in 0..times {
                rebases.push(self.segment_address(segment, offset, at)?);
                offset = offset.wrapping_add(POINTER + skip);
            }
        }
        Ok(rebases)
    }

    /// The bindings of one bind opcode stream. A lazy stream holds one bind per stub, each ended by
    /// `BIND_OPCODE_DONE`, so only the end of the table ends it.
    fn read_binds(&self, r: Reader<'_>, start: usize, end: usize, lazy: bool) -> Result<Vec<Binding>> {
        let what = "a bind opcode";
        let mut bindings = Vec::new();
        let (mut segment, mut offset) = (None, 0u64);
        let (mut ordinal, mut name, mut addend, mut weak) = (0i64, String::new(), 0i64, false);
        let mut cursor = start;
        while cursor < end {
            let at = cursor;
            let byte = r.u8(cursor, what)?;
            cursor += 1;
            let imm = byte & 0x0f;
            let (times, skip) = match byte & 0xf0 {
                0x00 if lazy => continue,
                0x00 => break,
                0x10 => {
                    ordinal = i64::from(imm);
                    continue;
                }
                0x20 => {
                    ordinal = uleb(r, &mut cursor, what)? as i64;
                    continue;
                }
                0x30 => {
                    ordinal = if imm == 0 { 0 } else { i64::from((imm | 0xf0) as i8) };
                    continue;
                }
                0x40 => {
                    let (symbol, len) = cstring(r, cursor, end, "a bind's symbol name")?;
                    cursor += len + 1;
                    name = symbol;
                    // BIND_SYMBOL_FLAGS_WEAK_IMPORT
                    weak = imm & 1 != 0;
                    continue;
                }
                0x50 => continue,
                0x60 => {
                    addend = sleb(r, &mut cursor, what)?;
                    continue;
                }
                0x70 => {
                    segment = Some(usize::from(imm));
                    offset = uleb(r, &mut cursor, what)?;
                    continue;
                }
                0x80 => {
                    offset = offset.wrapping_add(uleb(r, &mut cursor, what)?);
                    continue;
                }
                0x90 => (1, 0),
                0xa0 => (1, uleb(r, &mut cursor, what)?),
                0xb0 => (1, u64::from(imm) * POINTER),
                0xc0 => {
                    let times = uleb(r, &mut cursor, what)?;
                    (times, uleb(r, &mut cursor, what)?)
                }
                0xd0 => bail!("the threaded binds at offset {at:#x} are the arm64e kernel's, not read"),
                _ => bail!("the bind opcode {byte:#04x} at offset {at:#x} is unknown"),
            };
            ensure!(times <= (MAX_ENTRIES - bindings.len()) as u64, "the bind opcode at offset {at:#x} repeats {times} times");
            for _ in 0..times {
                let address = self.segment_address(segment, offset, at)?;
                bindings.push(Binding { address, name: name.clone(), library: self.library(ordinal), addend, weak, lazy });
                offset = offset.wrapping_add(POINTER + skip);
            }
        }
        Ok(bindings)
    }

    /// The rebases and the bindings of the chained fixups: each page of a segment starts a chain
    /// of pointers, each of which says whether it binds an import or rebases a target, and how far
    /// the next one is.
    fn read_chained_fixups(&mut self, r: Reader<'_>, command: usize) -> Result<()> {
        let (h, size) = linkedit(r, command, "the LC_DYLD_CHAINED_FIXUPS")?;
        let end = h + size;
        let version = r.u32(h, "dyld_chained_fixups_header fixups_version")?;
        ensure!(version == 0, "the chained fixups header at offset {h:#x} has the version {version}, not 0");
        let starts = h + r.u32(h + 4, "dyld_chained_fixups_header starts_offset")? as usize;
        let imports_at = h + r.u32(h + 8, "dyld_chained_fixups_header imports_offset")? as usize;
        let names_at = h + r.u32(h + 12, "dyld_chained_fixups_header symbols_offset")? as usize;
        let count = r.u32(h + 16, "dyld_chained_fixups_header imports_count")? as usize;
        let format = r.u32(h + 20, "dyld_chained_fixups_header imports_format")?;
        let names_format = r.u32(h + 24, "dyld_chained_fixups_header symbols_format")?;
        ensure!(names_format == 0, "the chained fixups at offset {h:#x} compress their names (format {names_format}), not read");
        ensure!(count <= MAX_ENTRIES, "the chained fixups at offset {h:#x} name {count} imports");

        // An ordinal past the specials' threshold is one of them: the image, the main executable,
        // a flat or a weak lookup.
        let ordinal8 = |v: u64| if v > 0xf0 { i64::from(v as u8 as i8) } else { v as i64 };
        let ordinal16 = |v: u64| if v > 0xfff0 { i64::from(v as u16 as i16) } else { v as i64 };
        let mut imports = Vec::with_capacity(count);
        for i in 0..count {
            let what = |field: &str| format!("chained import {i} {field}");
            let (ordinal, weak, name_offset, addend) = match format {
                1 | 2 => {
                    let at = imports_at + if format == 1 { 4 * i } else { 8 * i };
                    let v = u64::from(r.u32(at, &what("record"))?);
                    let addend = if format == 2 { i64::from(r.i32(at + 4, &what("addend"))?) } else { 0 };
                    (ordinal8(v & 0xff), (v >> 8) & 1 != 0, (v >> 9) as usize, addend)
                }
                3 => {
                    let at = imports_at + 16 * i;
                    let v = r.u64(at, &what("record"))?;
                    (ordinal16(v & 0xffff), (v >> 16) & 1 != 0, (v >> 32) as usize, r.u64(at + 8, &what("addend"))? as i64)
                }
                other => bail!("the chained fixups at offset {h:#x} have the imports format {other}, none of 1, 2 or 3"),
            };
            let (name, _) = cstring(r, names_at + name_offset, end, &what("name"))?;
            imports.push(ChainedImport { name, ordinal, weak, addend });
        }

        let base = self.header_address();
        let segments = r.u32(starts, "dyld_chained_starts_in_image seg_count")? as usize;
        for s in 0..segments.min(self.segments.len()) {
            let info = r.u32(starts + 4 + 4 * s, &format!("the chained starts of segment {s}"))? as usize;
            if info == 0 {
                continue;
            }
            let at = starts + info;
            let what = |field: &str| format!("the chained starts of segment {s} {field}");
            let page_size = u64::from(r.u16(at + 4, &what("page_size"))?);
            let pointer_format = r.u16(at + 6, &what("pointer_format"))?;
            let segment_offset = r.u64(at + 8, &what("segment_offset"))?;
            let pages = usize::from(r.u16(at + 20, &what("page_count"))?);
            let stride = match pointer_format {
                DYLD_CHAINED_PTR_64 | DYLD_CHAINED_PTR_64_OFFSET => 4,
                DYLD_CHAINED_PTR_ARM64E | DYLD_CHAINED_PTR_ARM64E_USERLAND | DYLD_CHAINED_PTR_ARM64E_USERLAND24 => 8,
                other => bail!("segment {s}'s chained fixups at offset {at:#x} have the pointer format {other}, not read"),
            };
            for p in 0..pages {
                let start = r.u16(at + 22 + 2 * p, &what("page_start"))?;
                if start == DYLD_CHAINED_PTR_START_NONE {
                    continue;
                }
                ensure!(start & DYLD_CHAINED_PTR_START_MULTI == 0, "page {p} of segment {s} starts several chains, which only a 32-bit format does");
                let mut address = base + segment_offset + p as u64 * page_size + u64::from(start);
                for _ in 0..MAX_ENTRIES {
                    let Some(offset) = self.offset_of(address) else { bail!("the chained fixup at {address:#x} lies in no segment's file bytes") };
                    let v = r.u64(offset as usize, "a chained fixup")?;
                    let (bind, next) = if stride == 4 {
                        let bind = (v >> 63 == 1).then(|| (v & 0xff_ffff, (v >> 24) & 0xff));
                        (bind.map(|(ordinal, addend)| (ordinal as usize, addend as i64)), (v >> 51) & 0xfff)
                    } else {
                        let auth = v >> 63 == 1;
                        let ordinal = if pointer_format == DYLD_CHAINED_PTR_ARM64E_USERLAND24 { v & 0xff_ffff } else { v & 0xffff };
                        // A plain bind keeps a 19-bit signed addend; an authenticated one has none.
                        let addend = if auth { 0 } else { (((v >> 32) & 0x7_ffff) << 45) as i64 >> 45 };
                        (((v >> 62) & 1 == 1).then_some((ordinal as usize, addend)), (v >> 51) & 0x7ff)
                    };
                    match bind {
                        Some((ordinal, addend)) => {
                            let Some(import) = imports.get(ordinal) else {
                                bail!("the chained fixup at {address:#x} binds import {ordinal}, of {}", imports.len())
                            };
                            let library = self.library(import.ordinal);
                            self.bindings.push(Binding {
                                address,
                                name: import.name.clone(),
                                library,
                                addend: import.addend + addend,
                                weak: import.weak,
                                lazy: false,
                            });
                        }
                        None => self.rebases.push(address),
                    }
                    if next == 0 {
                        break;
                    }
                    address += next * stride;
                }
            }
        }
        Ok(())
    }

    /// The symbols of the export trie: each node may end a name, and its children extend it by
    /// the string on their edge. A re-export names another library's symbol, so it is not one here.
    fn read_exports(&self, r: Reader<'_>, start: usize, size: usize) -> Result<Vec<Export>> {
        let end = start + size;
        ensure!(end <= r.len(), "the export trie at offset {start:#x} ({size:#x} bytes) runs past the end of the file");
        let base = self.header_address();
        let what = "an export trie node";
        let mut exports = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![(start, String::new())];
        while let Some((node, prefix)) = stack.pop() {
            ensure!(node < end, "an export trie edge points at offset {node:#x}, past the trie's end {end:#x}");
            ensure!(seen.insert(node), "the export trie loops at offset {node:#x}");
            ensure!(seen.len() <= MAX_ENTRIES, "the export trie holds more than {MAX_ENTRIES} nodes");
            let mut cursor = node;
            let terminal = uleb(r, &mut cursor, what)? as usize;
            let children = cursor + terminal;
            if terminal != 0 {
                let flags = uleb(r, &mut cursor, what)?;
                if flags & EXPORT_SYMBOL_FLAGS_REEXPORT == 0 {
                    let value = uleb(r, &mut cursor, what)?;
                    let address = if flags & EXPORT_SYMBOL_FLAGS_KIND_MASK == EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE { value } else { base + value };
                    exports.push(Export { name: prefix.clone(), address });
                }
            }
            cursor = children;
            let count = r.u8(cursor, "an export trie node's child count")?;
            cursor += 1;
            for _ in 0..count {
                let (edge, len) = cstring(r, cursor, end, "an export trie edge")?;
                cursor += len + 1;
                let child = uleb(r, &mut cursor, what)? as usize;
                stack.push((start + child, format!("{prefix}{edge}")));
            }
        }
        exports.sort_by(|a, b| (a.address, &a.name).cmp(&(b.address, &b.name)));
        Ok(exports)
    }

    /// The starts of `LC_FUNCTION_STARTS`: deltas from the Mach header, the first from the header
    /// itself, until a zero.
    fn read_function_starts(&self, r: Reader<'_>, at: usize) -> Result<Vec<u64>> {
        let (offset, size) = linkedit(r, at, "the LC_FUNCTION_STARTS")?;
        let mut address = self.header_address();
        let mut starts = Vec::new();
        let mut cursor = offset;
        while cursor < offset + size && starts.len() < MAX_ENTRIES {
            let delta = uleb(r, &mut cursor, "a function start")?;
            if delta == 0 {
                break;
            }
            address = address.wrapping_add(delta);
            starts.push(address);
        }
        Ok(starts)
    }

    /// The ranges of `__unwind_info`: each first-level index entry covers the functions from its
    /// offset to the next entry's, the last entry a sentinel; its second-level page lists where
    /// each range of one unwind encoding starts. Neighbours that unwind alike share one range.
    fn read_unwind_info(&self, r: Reader<'_>) -> Result<Vec<(u64, u64)>> {
        let Some(s) = self.section("__TEXT", "__unwind_info").filter(|s| !s.is_zerofill() && s.size != 0) else { return Ok(Vec::new()) };
        let start = s.offset as usize;
        let section = usize::try_from(s.size).ok().and_then(|size| r.bytes().get(start..start.checked_add(size)?));
        let Some(section) = section else { bail!("the section at offset {start:#x} ({:#x} bytes) runs past the end of the file", s.size) };
        // The offsets inside are the section's; a refusal names them with the section's own.
        let u = Reader::new(section);
        let version = u.u32(0, "the __unwind_info version")?;
        ensure!(version == 1, "the __unwind_info at offset {start:#x} has the version {version}, not 1");
        let index = u.u32(20, "the __unwind_info index offset")? as usize;
        let entries = u.u32(24, "the __unwind_info index count")? as usize;
        ensure!(entries <= MAX_ENTRIES, "the __unwind_info at offset {start:#x} has {entries} index entries");
        let base = self.header_address();
        let mut ranges = Vec::new();
        for i in 0..entries.saturating_sub(1) {
            let at = index + 12 * i;
            let what = |field: &str| format!("__unwind_info index entry {i} {field}");
            let first = u.u32(at, &what("functionOffset"))?;
            let page = u.u32(at + 4, &what("secondLevelPagesSectionOffset"))? as usize;
            let bound = u.u32(at + 12, &what("the next entry's functionOffset"))?;
            if page == 0 {
                continue;
            }
            let what = |field: &str| format!("__unwind_info page {page:#x} {field}");
            let kind = u.u32(page, &what("kind"))?;
            let entries_at = page + usize::from(u.u16(page + 4, &what("entryPageOffset"))?);
            let count = usize::from(u.u16(page + 6, &what("entryCount"))?);
            let mut starts = Vec::with_capacity(count);
            for j in 0..count {
                starts.push(match kind {
                    2 => u.u32(entries_at + 8 * j, &what("functionOffset"))?,
                    3 => first.wrapping_add(u.u32(entries_at + 4 * j, &what("entry"))? & 0x00ff_ffff),
                    other => bail!("the __unwind_info page at {page:#x} has the kind {other}, neither regular (2) nor compressed (3)"),
                });
            }
            for (j, &begin) in starts.iter().enumerate() {
                let end = starts.get(j + 1).copied().unwrap_or(bound);
                if end > begin {
                    ranges.push((base + u64::from(begin), base + u64::from(end)));
                }
            }
        }
        ranges.sort_unstable();
        ranges.dedup();
        Ok(ranges)
    }

    /// The functions: the unwind ranges cut at every function start, since a linker merges the
    /// entries of neighbours that unwind alike; a start no range holds runs to the next start or to
    /// the end of its section.
    fn extents(&self, unwind: &[(u64, u64)]) -> Vec<(u64, u64)> {
        let mut starts: Vec<u64> = unwind.iter().map(|&(begin, _)| begin).chain(self.function_starts.iter().copied()).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut functions = Vec::with_capacity(starts.len());
        for (i, &start) in starts.iter().enumerate() {
            let k = unwind.partition_point(|&(begin, _)| begin <= start);
            let limit = k
                .checked_sub(1)
                .map(|k| unwind[k])
                .filter(|&(_, end)| start < end)
                .map(|(_, end)| end)
                .or_else(|| self.sections.iter().find(|s| s.contains(start)).map(|s| s.addr + s.size));
            let Some(limit) = limit else { continue };
            let end = starts.get(i + 1).map_or(limit, |&next| next.min(limit));
            if end > start {
                functions.push((start, end));
            }
        }
        functions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64-bit x86-64 executable: `__TEXT` with one `__text` section and the `LC_MAIN` entry.
    fn minimal() -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        let put32 = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let put64 = |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        put32(&mut b, 0, MH_MAGIC_64);
        put32(&mut b, 4, CPU_TYPE_X86_64);
        put32(&mut b, 12, MH_EXECUTE);
        put32(&mut b, 16, 2);
        put32(&mut b, 20, 152 + 24);
        // LC_SEGMENT_64 __TEXT with __text
        put32(&mut b, 32, LC_SEGMENT_64);
        put32(&mut b, 36, 152);
        b[40..46].copy_from_slice(b"__TEXT");
        put64(&mut b, 56, 0x1_0000_0000);
        put64(&mut b, 64, 0x1000);
        put64(&mut b, 80, 0x200);
        put32(&mut b, 88, VM_PROT_READ | VM_PROT_EXECUTE);
        put32(&mut b, 92, VM_PROT_READ | VM_PROT_EXECUTE);
        put32(&mut b, 96, 1);
        b[104..110].copy_from_slice(b"__text");
        b[120..126].copy_from_slice(b"__TEXT");
        put64(&mut b, 136, 0x1_0000_0100);
        put64(&mut b, 144, 0x10);
        put32(&mut b, 152, 0x100);
        put32(&mut b, 168, S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS);
        // LC_MAIN
        put32(&mut b, 184, LC_MAIN);
        put32(&mut b, 188, 24);
        put64(&mut b, 192, 0x100);
        b
    }

    #[test]
    fn a_minimal_executable_reads() {
        let m = MachOFile::parse(&minimal()).unwrap();
        assert_eq!((m.cpu_type, m.kind, m.entry), (CPU_TYPE_X86_64, MH_EXECUTE, Some(0x1_0000_0100)));
        assert_eq!(m.segments.len(), 1);
        assert_eq!((m.sections[0].name.as_str(), m.sections[0].segment.as_str()), ("__text", "__TEXT"));
        assert!(m.sections[0].has_instructions());
        assert_eq!(m.offset_of(0x1_0000_0010), Some(0x10));
        // No unwind info and no function starts: no extents.
        assert!(m.functions.is_empty() && m.warnings.is_empty(), "{:?}", m.warnings);
    }

    #[test]
    fn a_fat_file_maps_its_x86_64_slice() {
        let thin = minimal();
        let mut fat = vec![0u8; 0x1000];
        fat[..4].copy_from_slice(&FAT_MAGIC);
        fat[4..8].copy_from_slice(&2u32.to_be_bytes());
        // An arm64 slice that the reader passes over, then the x86-64 one.
        fat[8..12].copy_from_slice(&CPU_TYPE_ARM64.to_be_bytes());
        fat[16..20].copy_from_slice(&0x2000u32.to_be_bytes());
        fat[28..32].copy_from_slice(&CPU_TYPE_X86_64.to_be_bytes());
        fat[36..40].copy_from_slice(&0x1000u32.to_be_bytes());
        fat[40..44].copy_from_slice(&(thin.len() as u32).to_be_bytes());
        fat.extend_from_slice(&thin);
        assert!(is_macho(&fat));
        let m = MachOFile::parse(&fat).unwrap();
        assert_eq!(m.cpu_type, CPU_TYPE_X86_64);
        assert_eq!(m.slice, Some((0x1000, thin.len() as u64)));
        assert_eq!(m.fat.len(), 2);
    }

    #[test]
    fn load_commands_past_the_end_are_refused() {
        let mut bytes = minimal();
        bytes[20..24].copy_from_slice(&0x400u32.to_le_bytes());
        let e = MachOFile::parse(&bytes).unwrap_err();
        assert!(format!("{e:#}").contains("load commands"), "{e:#}");
    }

    #[test]
    fn a_java_class_is_not_a_fat_file() {
        assert!(!is_macho(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52]));
    }

    #[test]
    fn the_unwind_ranges_split_at_the_function_starts() {
        let mut m = MachOFile::parse(&minimal()).unwrap();
        m.function_starts = vec![0x1_0000_0100, 0x1_0000_0108];
        assert_eq!(m.extents(&[(0x1_0000_0100, 0x1_0000_0110)]), vec![(0x1_0000_0100, 0x1_0000_0108), (0x1_0000_0108, 0x1_0000_0110)]);
        // Without unwind info, a start runs to its section's end.
        assert_eq!(m.extents(&[]), vec![(0x1_0000_0100, 0x1_0000_0108), (0x1_0000_0108, 0x1_0000_0110)]);
    }
}
