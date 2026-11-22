//! The mapped image: the file laid out by RVA as the Windows loader lays it out, with reads by
//! virtual address that refuse an address no section holds. An XBE ([`crate::xbe`]) maps the same
//! way, its header standing where the PE headers would; an ELF file ([`crate::elf`]) maps by its
//! loaded segments, from the lowest one's page, and a Mach-O file ([`crate::macho`]) by its
//! segments the same way. A PSX `PS-X EXE` maps its text at the header's address, and its
//! addresses go through the console's memory map ([`ra_asm_format::psx`]): a RAM address of any
//! segment reads the one byte it names. A Dreamcast boot program ([`Image::dreamcast`]) has no
//! container: it maps whole at [`dreamcast::BASE`], through the console's map the same way. A
//! Nintendo 64 ROM ([`Image::n64`]) maps the program its boot code loads, big-endian, through the
//! console's map: the 1 MiB Nintendo's boot code copies to the entry point, or the ELF file
//! libdragon's loads by its segments.

use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_format::dreamcast::{self, Boot};
use ra_asm_format::n64::{self, Rom};
use ra_asm_format::psx::{self, PsxExe};
use ra_asm_format::{ByteOrder, Reader};

use crate::debug::{self, DebugEntry};
use crate::exception::{self, RuntimeFunction};
use crate::exports::{self, ExportTable};
use crate::headers::{CoffHeader, DataDirectory, Directory, Headers, Machine, OptionalHeader, OptionalMagic};
use crate::imports::{self, Import};
use crate::relocs::{self, Relocation};
use crate::resources::{self, Resource};
use crate::section::Section;
use crate::tls::{self, Tls};
use crate::xbe::{self, Xbe};

/// The largest image the loader maps. A real image is far smaller; a corrupt `SizeOfImage` is not
/// a reason to allocate the machine's memory.
const MAX_IMAGE_SIZE: u32 = 1 << 30;

/// The highest image base the loader takes: 2^40 below the top, above any kernel image's base.
const MAX_IMAGE_BASE: u64 = u64::MAX - (1 << 40);

pub use ra_asm_arch::ArmMap;

/// A parsed, mapped PE image.
#[derive(Debug)]
pub struct Image {
    pub path: Option<PathBuf>,
    pub headers: Headers,
    pub sections: Vec<Section>,
    pub imports: Vec<Import>,
    pub exports: Option<ExportTable>,
    pub relocations: Vec<Relocation>,
    /// The x64 exception table, in address order. Empty on x86.
    pub runtime_functions: Vec<RuntimeFunction>,
    pub tls: Option<Tls>,
    pub debug: Vec<DebugEntry>,
    /// The leaves of the resource directory, in the tree's order.
    pub resources: Vec<Resource>,
    /// The functions the PDB beside the image names, else those its link map names, in address
    /// order; empty without either. Filled by [`crate::load`], which knows the path.
    pub symbols: Vec<crate::pdb::Symbol>,
    /// The ranges of the image that the objects of static libraries fill, from the same PDB, in
    /// address order.
    pub libraries: Vec<crate::pdb::LibraryRange>,
    /// The variables the PDB beside the image names, in address order, one per address; empty
    /// without one.
    pub data_symbols: Vec<crate::pdb::DataSymbol>,
    /// What the XBE header holds beyond the PE's, when the image is one.
    pub xbe: Option<Xbe>,
    /// The ELF file the image was mapped from, when it is one: its interpreter, its libraries and
    /// the function extents of its `.eh_frame`, which no PE table holds.
    pub elf: Option<ra_asm_format::elf::ElfFile>,
    /// The Mach-O file the image was mapped from, when it is one: its dynamic linker, its
    /// libraries, its bindings and the function extents of its `__unwind_info`.
    pub macho: Option<ra_asm_format::macho::MachOFile>,
    /// The `PlayStation` executable's header, when the image is one: its addresses fold through the
    /// console's memory map.
    pub psx: Option<PsxExe>,
    /// What a `PlayStation` BIOS ROM says of itself, on an image that is one: the ROM maps whole
    /// at [`psx::BIOS_RESET`], where the processor starts, and is entered at its first word.
    pub psx_bios: Option<psx::Bios>,
    /// The image is a flat Cortex-M firmware: no container, its vector table at its start, flash
    /// placed by the reset handler's address and RAM from the family's base to the stack's top.
    pub firmware: bool,
    /// The image is a Game Boy Advance cartridge's ROM: the whole file at [`GBA_ROM`], the
    /// console's RAM and registers before it, entered at its first word in ARM code.
    pub gba: bool,
    /// Which instruction set a 32-bit ARM image's code is in, where a reader knows it and the
    /// file does not say: `true` for ARM code, `false` for Thumb ([`Image::arm_code`]).
    pub arm_state: Option<bool>,
    /// Where the code of a 32-bit ARM image changes instruction set: an ELF file's mapping
    /// symbols, and what the analysis learns as it follows the code.
    pub arm_map: std::sync::Arc<ArmMap>,
    /// The Dreamcast boot program the image is, when it is one: its addresses fold through the
    /// console's memory map.
    pub dreamcast: Option<Boot>,
    /// The Nintendo 64 ROM's header, when the image is one: its addresses fold through the
    /// console's memory map. A ROM libdragon's boot code loads keeps its ELF file in [`Image::elf`]
    /// too, whose offsets are the ELF file's own; the sections' are the ROM's.
    pub n64: Option<Rom>,
    /// The value the start routine gives the global pointer, on a machine whose code addresses its
    /// data through one: a 32-bit MIPS ELF's `$gp`, from its `.reginfo` or its `_gp` symbol.
    pub global_pointer: Option<u64>,
    /// A ROM beside the program, with the address it is at: the BIOS ROM of a `PlayStation`
    /// program's console.
    pub rom: Option<(u64, Vec<u8>)>,
    /// The ROM beside a `PlayStation` program is the console's kernel: the run starts at the
    /// ROM's reset vector, and the program is entered where the kernel would enter its shell.
    pub rom_boots: bool,
    /// The image is a program a `PlayStation` BIOS ROM keeps for RAM ([`Image::psx_bios_parts`]):
    /// the offset of its bytes in the ROM.
    pub rom_part: Option<usize>,
    /// The image is a program a console loads into RAM and runs over its ROM's kernel, lifted
    /// beside the ROM.
    pub loaded: bool,
    /// The segment of the console's address space the image's code names its own places
    /// through, when it is not the one the image is mapped at: a kernel linked for `kuseg`.
    pub link_segment: Option<u64>,
    /// What the loader could not read. A directory that fails to parse lands here, not in an error.
    pub warnings: Vec<String>,
    file: Vec<u8>,
    mapped: Vec<u8>,
}

impl Image {
    /// Parses the bytes of a PE file (or an XBE, an ELF, a Mach-O file or a PS-X EXE) and maps them.
    pub fn parse(file: Vec<u8>) -> Result<Self> {
        Self::parse_as(file, None)
    }

    /// Maps the bytes of a Dreamcast boot program, `1ST_READ.BIN`, at [`dreamcast::BASE`]: the file
    /// is the image, with no header to say so, and `scrambled` says whether it is the form a disc
    /// of the CD-ROM kind carries, which is unscrambled first.
    pub fn dreamcast(file: Vec<u8>, scrambled: bool) -> Result<Self> {
        let plain = if scrambled { dreamcast::unscramble(&file) } else { file };
        Self::parse_as(plain, Some(Boot { scrambled }))
    }

    /// Whether the code of a `PlayStation` image runs over a kernel that is code of the console
    /// too (the image is the ROM or a part of it, or a program the ROM beside it boots), and not
    /// one the run answers for.
    pub fn psx_kernel(&self) -> bool {
        self.psx_bios.is_some() || self.rom_boots || self.is_resident()
    }

    /// Whether the image is code a console has in RAM beside its ROM: a program the ROM keeps
    /// for RAM, or one the console loads there.
    pub fn is_resident(&self) -> bool {
        self.rom_part.is_some() || self.loaded
    }

    /// The programs a `PlayStation` BIOS ROM keeps for RAM ([`psx::Bios::copies`]: its kernel,
    /// its shell), each as the image it is once the ROM's start code has copied it: the bytes
    /// the copy takes at the address it puts them, entered at their first word. They are second
    /// images beside the ROM's, as a DLL is beside a program's, and code of the console's kernel
    /// as the ROM's is. Empty for an image that is no ROM, or a ROM that copies none.
    pub fn psx_bios_parts(&self) -> Result<Vec<Self>> {
        if self.psx_bios.is_none() {
            return Ok(Vec::new());
        }
        let mut parts = Vec::new();
        for (offset, address, len) in psx::Bios::copies(&self.file) {
            // The header a PS-X EXE of those bytes would have: its text's address and size, its
            // entry, and no stack of its own.
            let mut exe = vec![0u8; psx::HEADER_SIZE];
            exe[..8].copy_from_slice(psx::MAGIC);
            exe[0x10..0x14].copy_from_slice(&address.to_le_bytes());
            exe[0x18..0x1c].copy_from_slice(&address.to_le_bytes());
            exe[0x1c..0x20].copy_from_slice(&len.to_le_bytes());
            exe.extend_from_slice(&self.file[offset..offset + len as usize]);
            let mut part = Self::parse(exe).with_context(|| format!("the ROM's program for {address:#x}"))?;
            part.rom_part = Some(offset);
            // The segment the program was linked for: the one most of the addresses its code
            // builds in two halves, that lie in it, name it through.
            let physical = u64::from(address & 0x1fff_ffff);
            let mut votes = [0usize; 8];
            let words: Vec<u32> = part.mapped().chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
            for pair in words.windows(2) {
                let (high, low) = (pair[0], pair[1]);
                if high >> 26 != 0x0f || !matches!(low >> 26, 0x09 | 0x0d) || (low >> 21) & 31 != (high >> 16) & 31 {
                    continue;
                }
                let half = if low >> 26 == 0x09 { i32::from(low as u16 as i16).cast_unsigned() } else { low & 0xffff };
                let built = ((high & 0xffff) << 16).wrapping_add(half);
                if u64::from(built & 0x1fff_ffff).wrapping_sub(physical) < u64::from(len) {
                    votes[(built >> 29) as usize] += 1;
                }
            }
            let segment = [4usize, 0, 5].into_iter().max_by_key(|&s| votes[s]).unwrap_or(4);
            part.link_segment = (segment != 4).then_some((segment as u64) << 29);
            // Named apart from the ROM and from the other parts, by where it runs.
            part.path = self.path.as_ref().map(|path| {
                let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                path.with_file_name(format!("{stem}_{address:08x}.bin"))
            });
            parts.push(part);
        }
        Ok(parts)
    }

    fn parse_as(mut file: Vec<u8>, boot: Option<Boot>) -> Result<Self> {
        // A ROM dumped in a swapped order is read in the cartridge's, which its sections map.
        let rom_order = n64::Order::of(&file).filter(|_| boot.is_none());
        if let Some(order) = rom_order {
            n64::to_z64(&mut file, order)?;
        }
        let reader = Reader::new(&file);
        let mut psx = None;
        let mut psx_bios = None;
        let mut flat_firmware = false;
        let mut gba = false;
        let mut rom = None;
        let (headers, sections, xbe, elf, macho) = if boot.is_some() {
            let (headers, sections) = dreamcast_layout(file.len())?;
            (headers, sections, None, None, None)
        } else if let Some(order) = rom_order {
            let header = Rom::parse(&file, order)?;
            let (headers, sections, elf) = n64_layout(&file, &header)?;
            rom = Some(header);
            (headers, sections, None, elf, None)
        } else if psx::is_psx(&file) {
            let exe = PsxExe::parse(&file)?;
            let (headers, sections) = psx_layout(&exe)?;
            psx = Some(exe);
            (headers, sections, None, None, None)
        } else if let Some(bios) = psx::Bios::parse(&file) {
            let (headers, sections) = psx_bios_layout(file.len())?;
            psx_bios = Some(bios);
            (headers, sections, None, None, None)
        } else if xbe::is_xbe(reader) {
            let (headers, sections, xbe) = xbe::parse(reader)?;
            (headers, sections, Some(xbe), None, None)
        } else if ra_asm_format::elf::is_elf(&file) {
            let (headers, sections, elf) = crate::elf::parse(&file)?;
            (headers, sections, None, Some(elf), None)
        } else if ra_asm_format::macho::is_macho(&file) {
            let (headers, sections, macho) = crate::macho::parse(&file)?;
            (headers, sections, None, None, Some(macho))
        } else if is_gba_rom(&file) {
            let (headers, sections) = gba_layout(file.len())?;
            gba = true;
            (headers, sections, None, None, None)
        } else if let Some(firmware) = CortexM::parse(&file) {
            // A flat firmware image: no container, the vector table at its start.
            let (headers, sections) = cortex_m_layout(file.len(), firmware)?;
            flat_firmware = true;
            (headers, sections, None, None, None)
        } else {
            let headers = Headers::parse(reader)?;
            let mut sections = Vec::with_capacity(headers.coff.sections as usize);
            let mut c = reader.at(headers.section_table_offset);
            for index in 0..headers.coff.sections as usize {
                sections.push(Section::parse(&mut c, index)?);
            }
            (headers, sections, None, None, None)
        };
        // A fat Mach-O file maps the slice the reader took, whose file offsets are the slice's own;
        // the reader checked that the slice lies in the file.
        if let Some((offset, size)) = macho.as_ref().and_then(|m| m.slice) {
            file = file[offset as usize..(offset + size) as usize].to_vec();
        }

        let size_of_image = headers.optional.size_of_image;
        ensure!(size_of_image <= MAX_IMAGE_SIZE, "SizeOfImage {size_of_image:#x} is larger than the {MAX_IMAGE_SIZE:#x} the loader maps");
        // Every address the directories compute is the base plus a 32-bit RVA plus a scaled 32-bit
        // index, so the base keeps that much room below the top of the address space.
        let image_base = headers.optional.image_base;
        ensure!(image_base <= MAX_IMAGE_BASE, "ImageBase {image_base:#x} leaves no room for the image below the top of the address space");
        let mut warnings = Vec::new();
        let mut mapped = vec![0u8; size_of_image as usize];
        let header_bytes = (headers.optional.size_of_headers as usize).min(file.len()).min(mapped.len());
        mapped[..header_bytes].copy_from_slice(&file[..header_bytes]);
        for section in &sections {
            let rva = section.virtual_address as usize;
            if rva >= mapped.len() {
                warnings.push(format!("section {} at RVA {rva:#x} lies past SizeOfImage {size_of_image:#x}", section.name));
                continue;
            }
            let wanted = (section.raw_size.min(section.mapped_size()) as usize).min(mapped.len() - rva);
            let start = (section.raw_offset as usize).min(file.len());
            let available = wanted.min(file.len() - start);
            if available < wanted {
                warnings.push(format!(
                    "section {} wants {wanted:#x} bytes at file offset {start:#x}, the file has {available:#x}",
                    section.name
                ));
            }
            mapped[rva..rva + available].copy_from_slice(&file[start..start + available]);
        }

        let mut image = Self {
            path: None,
            headers,
            sections,
            imports: Vec::new(),
            exports: None,
            relocations: Vec::new(),
            runtime_functions: Vec::new(),
            tls: None,
            debug: Vec::new(),
            resources: Vec::new(),
            symbols: Vec::new(),
            libraries: Vec::new(),
            data_symbols: Vec::new(),
            xbe,
            elf,
            macho,
            psx,
            psx_bios,
            firmware: flat_firmware,
            gba,
            dreamcast: boot,
            arm_state: None,
            arm_map: std::sync::Arc::default(),
            n64: rom,
            global_pointer: None,
            rom: None,
            rom_boots: false,
            rom_part: None,
            loaded: false,
            link_segment: None,
            warnings,
            file,
            mapped,
        };
        if let Some(elf) = image.elf.as_ref().filter(|elf| elf.machine == ra_asm_format::elf::EM_ARM) {
            // `$a` and `$t`, or either with a suffix after a dot; `$d` is data and changes nothing.
            let set = |name: &str| match name.split('.').next() {
                Some("$a") => Some(true),
                Some("$t") => Some(false),
                _ => None,
            };
            for (va, arm) in elf.symbols.iter().filter_map(|s| set(&s.name).map(|arm| (s.value, arm))) {
                image.arm_map.mark(va, arm);
            }
            // The entry's low bit says the set the program starts in.
            image.arm_map.mark(elf.entry & !1, elf.entry & 1 == 0);
        }
        if image.gba {
            image.arm_map.mark(GBA_ROM, true);
        }
        image.read_directories();
        Ok(image)
    }

    fn read_directories(&mut self) {
        // A PS-X EXE has no tables: its header is all it records, and a `gp` of zero is the start
        // routine's to set.
        if let Some(exe) = &self.psx {
            self.global_pointer = (exe.gp != 0).then_some(u64::from(exe.gp));
            return;
        }
        // Nor has a BIOS ROM.
        if self.psx_bios.is_some() {
            return;
        }
        // Nor has a Dreamcast boot program, which is its bytes and nothing else.
        if self.dreamcast.is_some() {
            return;
        }
        // Nor has a ROM whose program Nintendo's boot code copies flat; one libdragon's loads is an
        // ELF file, whose tables follow.
        if self.n64.is_some() && self.elf.is_none() {
            return;
        }
        // An ELF file has none of the PE's directories: its tables are its own.
        if self.elf.is_some() {
            crate::elf::read_tables(self);
            return;
        }
        if self.macho.is_some() {
            crate::macho::read_tables(self);
            return;
        }
        // An XBE's imports are its kernel thunk table; the directories below are empty in one
        // but the TLS, which the header points at in the PE's layout.
        let imports = match &self.xbe {
            Some(xbe) => xbe::kernel_imports(self, xbe),
            None => imports::parse(self),
        };
        match imports {
            Ok(imports) => self.imports = imports,
            Err(e) => self.warnings.push(format!("imports: {e:#}")),
        }
        match exports::parse(self) {
            Ok(exports) => self.exports = exports,
            Err(e) => self.warnings.push(format!("exports: {e:#}")),
        }
        match relocs::parse(self) {
            Ok(relocations) => self.relocations = relocations,
            Err(e) => self.warnings.push(format!("relocations: {e:#}")),
        }
        match exception::parse(self) {
            Ok(functions) => self.runtime_functions = functions,
            Err(e) => self.warnings.push(format!("exception table: {e:#}")),
        }
        match tls::parse(self) {
            Ok(tls) => self.tls = tls,
            Err(e) => self.warnings.push(format!("TLS: {e:#}")),
        }
        match debug::parse(self) {
            Ok(debug) => self.debug = debug,
            Err(e) => self.warnings.push(format!("debug: {e:#}")),
        }
        match resources::parse(self) {
            Ok(resources) => self.resources = resources,
            Err(e) => self.warnings.push(format!("resources: {e:#}")),
        }
    }

    // region Geometry

    pub fn machine(&self) -> Machine {
        self.headers.coff.machine
    }

    pub fn is_64(&self) -> bool {
        self.headers.optional.is_64()
    }

    /// The width of a pointer in the image, in bytes.
    pub fn pointer_size(&self) -> u8 {
        if self.is_64() { 8 } else { 4 }
    }

    /// Whether the image's words are big-endian: an ELF file's `ELFDATA2MSB`, a Nintendo 64 ROM's,
    /// whatever order its dump was in. A PE, an XBE and a Mach-O file of the machines the loader
    /// maps are little-endian.
    pub fn big_endian(&self) -> bool {
        self.n64.is_some() || self.elf.as_ref().is_some_and(|elf| elf.big_endian)
    }

    /// Whether a 32-bit ARM program's code is ARM code and not Thumb, as far as its file says: an
    /// ELF file's entry with its low bit clear. A program of both is told by where it starts.
    pub fn arm_code(&self) -> bool {
        self.machine() == Machine::Arm
            && self.arm_state.unwrap_or_else(|| self.gba || self.elf.as_ref().is_some_and(|elf| elf.entry & 1 == 0))
    }

    /// Whether the code at `va` of a 32-bit ARM image is ARM code and not Thumb. A linker marks
    /// where each set starts with a mapping symbol (`$a`, `$t`), and a program of both is told
    /// apart by them; without them the whole image is what [`Image::arm_code`] says.
    pub fn arm_at(&self, va: u64) -> bool {
        if self.arm_state.is_some() || self.machine() != Machine::Arm {
            return self.arm_code();
        }
        self.arm_map.at(va).unwrap_or_else(|| self.arm_code())
    }

    /// Whether the image's code is of both instruction sets, as far as its map says.
    pub fn arm_and_thumb(&self) -> bool {
        self.arm_state.is_none() && self.arm_map.both()
    }

    /// The byte order of the program's words, which every read of a value of the image takes: the
    /// pointers, the tables and the instruction words of a big-endian program read big-endian.
    pub fn order(&self) -> ByteOrder {
        if self.big_endian() { ByteOrder::Big } else { ByteOrder::Little }
    }

    /// The pointer-width word at the start of `bytes`, in the image's order: a scan over a
    /// section's bytes reads its words this way. Bytes shorter than a pointer read as zero.
    pub fn pointer_in(&self, bytes: &[u8]) -> u64 {
        let reader = Reader::new(bytes).with_order(self.order());
        let word = if self.is_64() { reader.u64(0, "pointer") } else { reader.u32(0, "pointer").map(u64::from) };
        word.unwrap_or(0)
    }

    pub fn image_base(&self) -> u64 {
        self.headers.optional.image_base
    }

    pub fn size_of_image(&self) -> u32 {
        self.headers.optional.size_of_image
    }

    /// The virtual address of the entry point, or `None` when the image has none. A PS-X EXE always
    /// has one, which may be the first byte of its text; a Dreamcast boot program's is its first byte,
    /// and so is the program's of a ROM that Nintendo's boot code copies flat.
    pub fn entry_point(&self) -> Option<u64> {
        let rva = self.headers.optional.entry_point;
        let flat = self.psx.is_some() || self.psx_bios.is_some() || self.dreamcast.is_some() || self.n64.is_some() || self.firmware;
        (rva != 0 || flat).then(|| self.va(rva))
    }

    /// The virtual address of an RVA.
    pub fn va(&self, rva: u32) -> u64 {
        self.image_base() + u64::from(rva)
    }

    /// The RVA of a virtual address inside the image, or `None` when it lies outside. A console's
    /// address is folded first: a PS-X EXE's `kuseg` and `kseg1` mirrors of RAM are the `kseg0`
    /// address, a `PlayStation 2` program's `kseg` and uncached mirrors the `kuseg` one, a Dreamcast
    /// program's `P2` mirror the `P1` one, a Nintendo 64 program's `kseg1` mirror the `kseg0` one.
    pub fn rva(&self, va: u64) -> Option<u32> {
        let mapped = self.psx.is_some() || self.is_ps2() || self.dreamcast.is_some() || self.n64.is_some();
        let va = if mapped { self.memory_region(va)?.1 } else { va };
        let rva = va.checked_sub(self.image_base())?;
        (rva < u64::from(self.size_of_image())).then_some(rva as u32)
    }

    /// The section that holds `va`.
    pub fn section_of(&self, va: u64) -> Option<&Section> {
        let rva = self.rva(va)?;
        self.sections.iter().find(|s| s.contains_rva(rva))
    }

    /// The section that holds `rva`.
    pub fn section_of_rva(&self, rva: u32) -> Option<&Section> {
        self.sections.iter().find(|s| s.contains_rva(rva))
    }

    /// Whether `va` lies in the headers or in a section.
    pub fn is_mapped(&self, va: u64) -> bool {
        match self.rva(va) {
            Some(rva) => rva < self.headers.optional.size_of_headers || self.section_of_rva(rva).is_some(),
            None => false,
        }
    }

    /// Whether `va` lies in a section that holds code or is executable. Such a section holds the
    /// read-only data too of an image whose [`Image::data_in_code`] is true.
    pub fn is_code(&self, va: u64) -> bool {
        self.section_of(va).is_some_and(|s| s.is_code() || s.is_executable())
    }

    /// Whether the image keeps its read-only data in a code section: its export directory or its
    /// load configuration, which the linker writes into `.rdata`, lies in one, as in a DLL linked
    /// with `/MERGE:.rdata=.text` (the Windows system DLLs). The constants, the vtables and the
    /// tables of such an image are read out of its code section. A writable code section holds
    /// data too: no compiler writes its code, but an unpacking stub writes the program's code and
    /// data into one section, which the unpacked image keeps.
    pub fn data_in_code(&self) -> bool {
        let merged = [Directory::Export, Directory::LoadConfig].into_iter().any(|d| {
            let directory = self.headers.optional.directory(d);
            !directory.is_empty() && self.section_of_rva(directory.rva).is_some_and(|s| s.is_code() || s.is_executable())
        });
        merged || self.sections.iter().any(|s| (s.is_code() || s.is_executable()) && s.is_writable())
    }

    /// The console the image runs on, when its container or its processor names one: `psx` for a
    /// PS-X EXE, `psp` for a PRX module, `iop` for a `PlayStation 2` IRX module, `ps2` for an ELF
    /// of the R5900, `xbox` for an XBE, `dreamcast` for a boot program, `n64` for a cartridge ROM;
    /// `None` for a program of an operating system.
    pub fn platform(&self) -> Option<&'static str> {
        if self.psx.is_some() || self.psx_bios.is_some() {
            Some("psx")
        } else if self.n64.is_some() {
            Some("n64")
        } else if self.dreamcast.is_some() {
            Some("dreamcast")
        } else if self.gba {
            Some("gba")
        } else if self.elf.as_ref().is_some_and(|e| e.prx.is_some()) {
            Some("psp")
        } else if self.elf.as_ref().is_some_and(|e| e.irx.is_some()) {
            Some("iop")
        } else if self.is_ps2() {
            Some("ps2")
        } else if self.xbe.is_some() {
            Some("xbox")
        } else {
            None
        }
    }

    /// The handlers of a Cortex-M firmware's vector table, with the address of each one's entry
    /// in the table: the image is 32-bit ARM, its first word is a stack pointer in the RAM of a
    /// microcontroller (`0x2000_0000` up), and the words after it are handlers, each the address
    /// of code with its low bit set (the processor has only Thumb) or zero for an exception the
    /// firmware leaves alone. The processor enters them: nothing of the program calls one. The
    /// table ends at the first word that is neither, and `None` is an image with no such table.
    pub fn cortex_m_vectors(&self) -> Option<Vec<(u64, u64)>> {
        if self.machine() != Machine::Arm {
            return None;
        }
        let base = self.image_base();
        let stack = self.u32(base).ok()?;
        let handler = |word: u32| word & 1 == 1 && self.is_code(u64::from(word & !1));
        if !(0x2000_0000..0x4000_0000).contains(&stack) || stack % 4 != 0 || !handler(self.u32(base + 4).ok()?) {
            return None;
        }
        let mut handlers = Vec::new();
        // The system exceptions and as many interrupts as the largest of the family has.
        for n in 1..16 + 240 {
            let at = base + 4 * n;
            match self.u32(at) {
                Ok(0) => {}
                Ok(word) if handler(word) => handlers.push((at, u64::from(word & !1))),
                _ => break,
            }
        }
        Some(handlers)
    }

    /// Whether the image is a `PlayStation 2` program: an ELF whose code is the Emotion Engine's.
    fn is_ps2(&self) -> bool {
        self.elf.as_ref().and_then(|e| e.mips.as_ref()).is_some_and(ra_asm_format::elf::Mips::is_r5900)
    }

    /// The place of the console's memory map that `va` reaches, with its folded address, on an
    /// image of a console that has one (the `PlayStation's`, the `PlayStation 2's`, the
    /// Dreamcast's, the Nintendo 64's); `None` elsewhere, or for an address no place answers. An
    /// access to [`psx::Region::Hardware`] or [`psx::Region::StoreQueue`] is a device's, which the
    /// platform runtime answers and the image does not hold.
    pub fn memory_region(&self, va: u64) -> Option<(psx::Region, u64)> {
        if self.psx.is_some() {
            psx::fold(va)
        } else if self.n64.is_some() {
            n64::fold(va)
        } else if self.dreamcast.is_some() {
            dreamcast::fold(va)
        } else if self.is_ps2() {
            ra_asm_format::elf::ps2::fold(va)
        } else {
            None
        }
    }

    /// Whether `va` is an address as the image's code names its own places: on a console's
    /// image, the one the memory map folds it to, or the same place through the segment the
    /// image was linked for ([`Image::link_segment`]). The same place through another segment
    /// is a number that happens to read as an address.
    pub fn names_own(&self, va: u64) -> bool {
        match self.memory_region(va) {
            None => true,
            Some((_, folded)) => folded == va || self.link_segment.is_some_and(|segment| va & 0xe000_0000 == segment),
        }
    }

    /// The places of the console's memory map with their names and mirrors, on a PS-X EXE; empty
    /// elsewhere. [`Image::memory_region`] folds one address through the same map.
    pub fn memory_map(&self) -> &'static [crate::psx::Place] {
        if self.psx.is_some() { &crate::psx::MAP } else { &[] }
    }

    /// The whole mapped image, indexed by RVA.
    pub fn mapped(&self) -> &[u8] {
        &self.mapped
    }

    /// The bytes of the file as read.
    pub fn file(&self) -> &[u8] {
        &self.file
    }

    // endregion

    // region Reads by virtual address

    /// `len` bytes at `va`. The range must lie in the headers or in one section.
    pub fn read(&self, va: u64, len: usize) -> Result<&[u8]> {
        let Some(rva) = self.rva(va) else { bail!("address {va:#x} lies outside the image") };
        let start = rva as usize;
        let end = start.checked_add(len).filter(|&end| end <= self.mapped.len());
        let Some(end) = end else { bail!("the {len:#x} bytes at {va:#x} run past the end of the image") };
        let in_headers = (end as u32) <= self.headers.optional.size_of_headers;
        if !in_headers {
            let Some(section) = self.section_of_rva(rva) else { bail!("no section holds address {va:#x}") };
            ensure!(section.contains_rva((end - 1) as u32), "the {len:#x} bytes at {va:#x} run past the end of section {}", section.name);
        }
        Ok(&self.mapped[start..end])
    }

    /// The bytes from `va` to the end of its section.
    pub fn read_to_section_end(&self, va: u64) -> Result<&[u8]> {
        let Some((rva, section)) = self.rva(va).and_then(|rva| Some((rva, self.section_of_rva(rva)?))) else {
            bail!("no section holds address {va:#x}")
        };
        let end = section.end_rva().min(self.size_of_image());
        self.read(va, (end - rva) as usize)
    }

    /// Puts `bytes` at `va` in the mapped image, as a running program leaves its own image: code
    /// it unpacked or patched, data it wrote. The range must lie in the headers or in one section.
    pub fn write(&mut self, va: u64, bytes: &[u8]) -> Result<()> {
        self.read(va, bytes.len())?;
        let Some(rva) = self.rva(va) else { bail!("address {va:#x} lies outside the image") };
        let start = rva as usize;
        self.mapped[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// A [`Reader`] over the bytes at `va` to the end of the section, in the image's order.
    pub fn reader_at(&self, va: u64) -> Result<Reader<'_>> {
        Ok(Reader::new(self.read_to_section_end(va)?).with_order(self.order()))
    }

    pub fn u8(&self, va: u64) -> Result<u8> {
        Ok(self.read(va, 1)?[0])
    }

    /// The 16-bit value at `va`, in the image's order, as are [`Image::u32`] and [`Image::u64`].
    pub fn u16(&self, va: u64) -> Result<u16> {
        self.value_reader(va, 2)?.u16(0, "value")
    }

    pub fn u32(&self, va: u64) -> Result<u32> {
        self.value_reader(va, 4)?.u32(0, "value")
    }

    pub fn u64(&self, va: u64) -> Result<u64> {
        self.value_reader(va, 8)?.u64(0, "value")
    }

    fn value_reader(&self, va: u64, len: usize) -> Result<Reader<'_>> {
        Ok(Reader::new(self.read(va, len)?).with_order(self.order()))
    }

    /// The bytes `value` is at `width` bytes in the image's order: what a write of a word puts.
    fn bytes_of(&self, value: u64, width: usize) -> Vec<u8> {
        match self.order() {
            ByteOrder::Little => value.to_le_bytes()[..width].to_vec(),
            ByteOrder::Big => value.to_be_bytes()[8 - width..].to_vec(),
        }
    }

    /// Writes the 32-bit `value` at `va` in the image's order, as a loader fills a field it
    /// relocates.
    pub(crate) fn write_u32(&mut self, va: u64, value: u32) -> Result<()> {
        let bytes = self.bytes_of(u64::from(value), 4);
        self.write(va, &bytes)
    }

    /// A pointer-width value at `va`.
    pub fn pointer(&self, va: u64) -> Result<u64> {
        if self.is_64() { self.u64(va) } else { self.u32(va).map(u64::from) }
    }

    /// Writes a pointer-width `value` at `va` in the mapped image, as a loader fills a word it
    /// relocates. The word must lie in one section, as [`Image::read`] has it.
    pub(crate) fn write_pointer(&mut self, va: u64, value: u64) -> Result<()> {
        let bytes = self.bytes_of(value, usize::from(self.pointer_size()));
        self.write(va, &bytes)
    }

    /// The NUL-terminated string at `va`, at most `max` bytes long.
    pub fn cstr(&self, va: u64, max: usize) -> Result<&str> {
        let bytes = self.read_to_section_end(va).with_context(|| format!("string at {va:#x}"))?;
        Reader::new(bytes).cstr(0, max, "string").with_context(|| format!("string at {va:#x}"))
    }

    /// The string at `va` when the bytes there read as printable text of at least `min` characters.
    pub fn printable_cstr(&self, va: u64, min: usize, max: usize) -> Option<&str> {
        let s = self.cstr(va, max).ok()?;
        (s.len() >= min && s.chars().all(|c| c == '\t' || c == '\n' || c == '\r' || (' '..='~').contains(&c))).then_some(s)
    }

    // endregion

    /// The RVA of a data directory as a virtual address, when the directory is present.
    pub fn directory_va(&self, index: crate::headers::Directory) -> Option<(u64, u32)> {
        let dir = self.headers.optional.directory(index);
        (!dir.is_empty()).then(|| (self.va(dir.rva), dir.size))
    }

    /// The function extents a container records beside a PE's exception table, `(begin, end)` in
    /// address order: an ELF file's `.eh_frame`, a Mach-O file's `__unwind_info` cut at its function
    /// starts; empty for a PE.
    pub fn unwind_extents(&self) -> &[(u64, u64)] {
        match (&self.elf, &self.macho) {
            (Some(elf), _) => &elf.functions,
            (None, Some(macho)) => &macho.functions,
            (None, None) => &[],
        }
    }

    /// The import whose IAT slot is at `va`.
    pub fn import_at(&self, va: u64) -> Option<&Import> {
        self.imports.iter().find(|i| i.iat_va == va)
    }
}

/// The headers and the sections a PS-X EXE stands for, laid out as the BIOS loads it: `.text`, the
/// file's bytes after the header at the header's address, code and data both and writable, and
/// `.bss`, the range the BIOS clears, when the header names one. The base is the lowest of the two
/// folded addresses; the 2 KiB header is not mapped, since the BIOS copies only the text.
fn psx_layout(exe: &PsxExe) -> Result<(Headers, Vec<Section>)> {
    let text = exe.text_address();
    let bss = exe.bss_range();
    let base = bss.map_or(text, |(start, _)| start.min(text));
    let end = bss.map_or(0, |(start, size)| start + u64::from(size)).max(text + u64::from(exe.text.1));
    let rva = |va: u64| u32::try_from(va - base).context("the PS-X EXE spans more than 4 GiB");
    let mut sections = vec![Section {
        name: ".text".to_owned(),
        virtual_size: exe.text.1,
        virtual_address: rva(text)?,
        raw_size: exe.text.1,
        raw_offset: u32::try_from(psx::HEADER_SIZE).unwrap_or(0),
        characteristics: crate::elf::characteristics(true, true, false),
    }];
    if let Some((start, size)) = bss {
        sections.push(Section {
            name: ".bss".to_owned(),
            virtual_size: size,
            virtual_address: rva(start)?,
            raw_size: 0,
            raw_offset: 0,
            characteristics: crate::elf::characteristics(false, true, true),
        });
    }
    let size_of_image = rva(end)?;
    let headers = Headers {
        pe_offset: 0,
        coff: CoffHeader {
            machine: Machine::Mips,
            sections: u16::try_from(sections.len()).unwrap_or(u16::MAX),
            timestamp: 0,
            symbol_table: 0,
            symbols: 0,
            optional_header_size: 0,
            characteristics: CoffHeader::EXECUTABLE_IMAGE | CoffHeader::RELOCS_STRIPPED,
        },
        optional: OptionalHeader {
            magic: OptionalMagic::Pe32,
            linker_version: (0, 0),
            size_of_code: exe.text.1,
            size_of_initialized_data: 0,
            size_of_uninitialized_data: bss.map_or(0, |(_, size)| size),
            entry_point: rva(exe.entry())?,
            base_of_code: rva(text)?,
            image_base: base,
            section_alignment: u32::try_from(psx::HEADER_SIZE).unwrap_or(0),
            file_alignment: u32::try_from(psx::HEADER_SIZE).unwrap_or(0),
            os_version: (0, 0),
            image_version: (0, 0),
            subsystem_version: (0, 0),
            size_of_image,
            size_of_headers: 0,
            checksum: 0,
            subsystem: 0,
            dll_characteristics: 0,
            stack_reserve: 0,
            stack_commit: 0,
            heap_reserve: 0,
            heap_commit: 0,
            directories: vec![DataDirectory::default(); 16],
        },
        section_table_offset: 0,
        rich: None,
    };
    Ok((headers, sections))
}

/// The headers and the one section a `PlayStation` BIOS ROM of `len` bytes stands for: `.rom`, the
/// whole file at [`psx::BIOS_RESET`], code and data both and read-only, entered at its first word.
fn psx_bios_layout(len: usize) -> Result<(Headers, Vec<Section>)> {
    let size = u32::try_from(len).context("the ROM is larger than 4 GiB")?;
    let (mut headers, mut sections) = dreamcast_layout(len.min(0x8000))?;
    let rom = sections.first_mut().context("no section")?;
    ".rom".clone_into(&mut rom.name);
    (rom.virtual_size, rom.raw_size) = (size, size);
    rom.characteristics = crate::elf::characteristics(true, false, false);
    headers.coff.machine = Machine::Mips;
    headers.optional.image_base = psx::BIOS_RESET;
    headers.optional.size_of_code = size;
    headers.optional.size_of_image = size;
    Ok((headers, sections))
}

/// What the first words of a flat Cortex-M firmware image say: where flash is, where the reset
/// handler is, and where the stack starts, which is the end of the RAM the firmware uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CortexM {
    base: u64,
    reset: u64,
    stack: u64,
}

impl CortexM {
    /// Reads a file as the image a microcontroller is flashed with: no header, the vector table
    /// first. Its first word is the stack pointer, in the RAM of the family (`0x2000_0000` up);
    /// its second the reset handler, an address in the image with its low bit set; and most of
    /// the exception vectors after them are such addresses too, or zero. The image holds no
    /// address of its own, so flash is taken to start at the boundary below the reset handler
    /// that the image's size, as a power of two, lies on.
    fn parse(file: &[u8]) -> Option<Self> {
        let word = |n: usize| file.get(4 * n..4 * n + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let (stack, reset) = (word(0)?, word(1)?);
        if file.len() < 64 || !(0x2000_0000..0x4000_0000).contains(&stack) || stack % 4 != 0 || reset & 1 == 0 || reset >= 0x2000_0000 {
            return None;
        }
        let span = (file.len() as u64).next_power_of_two();
        let base = u64::from(reset & !1) & !(span - 1);
        let inside = |word: u32| word & 1 == 1 && (base..base + file.len() as u64).contains(&u64::from(word & !1));
        let vectors: Vec<u32> = (2..16).filter_map(word).collect();
        let handlers = vectors.iter().filter(|&&v| inside(v)).count();
        let plausible = inside(reset) && handlers >= 3 && vectors.iter().all(|&v| v == 0 || inside(v));
        plausible.then_some(Self { base, reset: u64::from(reset & !1), stack: u64::from(stack) })
    }
}

/// Where a Game Boy Advance cartridge's ROM is in the console's memory, and the memory before it.
pub const GBA_ROM: u64 = 0x0800_0000;
const GBA_EWRAM: (u64, u32) = (0x0200_0000, 0x4_0000);
const GBA_IWRAM: (u64, u32) = (0x0300_0000, 0x8000);
const GBA_IO: (u64, u32) = (0x0400_0000, 0x400);

/// Whether a file is a Game Boy Advance cartridge's ROM, by its header: an ARM branch over the
/// header for its first word, the fixed byte `0x96` at `0xb2`, and at `0xbd` the complement of
/// the sum of the bytes from `0xa0` that the console's boot code checks.
fn is_gba_rom(file: &[u8]) -> bool {
    file.len() >= 0xc0
        && file[3] == 0xea
        && file[0xb2] == 0x96
        && file[0xa0..0xbd].iter().fold(0u8, |sum, &byte| sum.wrapping_sub(byte)).wrapping_sub(0x19) == file[0xbd]
}

/// The headers and the sections a Game Boy Advance ROM of `len` bytes stands for: the console's
/// two blocks of RAM and its registers, which the file holds no byte of, and `.rom`, the whole
/// file at [`GBA_ROM`], entered at its first word in ARM code.
fn gba_layout(len: usize) -> Result<(Headers, Vec<Section>)> {
    let size = u32::try_from(len).ok().filter(|&size| size <= 0x200_0000).context("the ROM is larger than the 32 MiB a cartridge has")?;
    let base = GBA_EWRAM.0;
    let (mut headers, _) = dreamcast_layout(len.min(0x8000))?;
    let memory = |name: &str, (address, size): (u64, u32)| Section {
        name: name.to_owned(),
        virtual_size: size,
        virtual_address: (address - base) as u32,
        raw_size: 0,
        raw_offset: 0,
        characteristics: crate::elf::characteristics(false, true, true),
    };
    let rom = Section {
        name: ".rom".to_owned(),
        virtual_size: size,
        virtual_address: (GBA_ROM - base) as u32,
        raw_size: size,
        raw_offset: 0,
        characteristics: crate::elf::characteristics(true, false, false),
    };
    let sections = vec![memory(".ewram", GBA_EWRAM), memory(".iwram", GBA_IWRAM), memory(".io", GBA_IO), rom];
    headers.coff.machine = Machine::Arm;
    headers.coff.sections = 4;
    headers.optional.image_base = base;
    headers.optional.entry_point = (GBA_ROM - base) as u32;
    headers.optional.size_of_code = size;
    headers.optional.size_of_image = (GBA_ROM - base) as u32 + size.next_multiple_of(0x1000);
    Ok((headers, sections))
}

/// The headers and the sections a flat Cortex-M firmware image of `len` bytes stands for:
/// `.flash`, the whole file where the vector table puts it, code and read-only data both, and
/// `.ram`, from the family's RAM base to the stack's top, which the image holds no byte of.
fn cortex_m_layout(len: usize, firmware: CortexM) -> Result<(Headers, Vec<Section>)> {
    const RAM: u64 = 0x2000_0000;
    let size = u32::try_from(len).context("the firmware image is larger than 4 GiB")?;
    let (mut headers, mut sections) = dreamcast_layout(len.min(0x8000))?;
    let flash = sections.first_mut().context("no section")?;
    ".flash".clone_into(&mut flash.name);
    (flash.virtual_size, flash.raw_size) = (size, size);
    flash.characteristics = crate::elf::characteristics(true, false, false);
    let ram_end = firmware.stack.next_multiple_of(0x1000);
    sections.push(Section {
        name: ".ram".to_owned(),
        virtual_size: u32::try_from(ram_end - RAM).context("the firmware's RAM is larger than 4 GiB")?,
        virtual_address: u32::try_from(RAM - firmware.base).context("flash lies above the RAM")?,
        raw_size: 0,
        raw_offset: 0,
        characteristics: crate::elf::characteristics(false, true, true),
    });
    headers.coff.machine = Machine::Arm;
    headers.coff.sections = 2;
    headers.optional.image_base = firmware.base;
    headers.optional.entry_point = u32::try_from(firmware.reset - firmware.base).context("the reset handler lies outside the image")?;
    headers.optional.size_of_code = size;
    headers.optional.size_of_image = u32::try_from(ram_end - firmware.base).context("the firmware's memory is larger than 4 GiB")?;
    Ok((headers, sections))
}

/// The headers and the one section a Dreamcast boot program of `len` bytes stands for, laid out as
/// the boot ROM loads it: `.text`, the whole file at [`dreamcast::BASE`], code and data both and
/// writable, entered at its first byte. The file must fit in the RAM above the base.
fn dreamcast_layout(len: usize) -> Result<(Headers, Vec<Section>)> {
    ensure!(len != 0, "the boot program is empty");
    let size = u32::try_from(len).ok().filter(|&size| u64::from(size) <= dreamcast::MAX_SIZE);
    let Some(size) = size else {
        bail!("the boot program's {len:#x} bytes do not fit in the {:#x} bytes of RAM above {:#x}", dreamcast::MAX_SIZE, dreamcast::BASE)
    };
    let sections = vec![Section {
        name: ".text".to_owned(),
        virtual_size: size,
        virtual_address: 0,
        raw_size: size,
        raw_offset: 0,
        characteristics: crate::elf::characteristics(true, true, false),
    }];
    let headers = Headers {
        pe_offset: 0,
        coff: CoffHeader {
            machine: Machine::Sh4,
            sections: 1,
            timestamp: 0,
            symbol_table: 0,
            symbols: 0,
            optional_header_size: 0,
            characteristics: CoffHeader::EXECUTABLE_IMAGE | CoffHeader::RELOCS_STRIPPED,
        },
        optional: OptionalHeader {
            magic: OptionalMagic::Pe32,
            linker_version: (0, 0),
            size_of_code: size,
            size_of_initialized_data: 0,
            size_of_uninitialized_data: 0,
            entry_point: 0,
            base_of_code: 0,
            image_base: dreamcast::BASE,
            section_alignment: 1,
            file_alignment: 1,
            os_version: (0, 0),
            image_version: (0, 0),
            subsystem_version: (0, 0),
            size_of_image: size,
            size_of_headers: 0,
            checksum: 0,
            subsystem: 0,
            dll_characteristics: 0,
            stack_reserve: 0,
            stack_commit: 0,
            heap_reserve: 0,
            heap_commit: 0,
            directories: vec![DataDirectory::default(); 16],
        },
        section_table_offset: 0,
        rich: None,
    };
    Ok((headers, sections))
}

/// The headers and the sections of a Nintendo 64 ROM `rom`, already in the `.z64` order, as its
/// boot code loads the program: libdragon's ELF file by its segments, the sections' file offsets
/// moved to the ROM's; else `.text`, the bytes Nintendo's boot code copies to the entry point,
/// code and data both and writable, entered at its first byte.
fn n64_layout(rom: &[u8], header: &Rom) -> Result<(Headers, Vec<Section>, Option<ra_asm_format::elf::ElfFile>)> {
    let (address, size) = match header.boot {
        n64::Boot::Elf { offset } => {
            let (mut headers, mut sections, elf) =
                crate::elf::parse(&rom[offset..]).with_context(|| format!("the ELF file at ROM offset {offset:#x}"))?;
            let shift = u32::try_from(offset).context("the ELF file lies past 4 GiB of the ROM")?;
            for section in sections.iter_mut().filter(|s| s.raw_size != 0) {
                section.raw_offset = section.raw_offset.saturating_add(shift);
            }
            // The ELF header is no part of what the boot code loads, and the ROM's own header stands
            // at the file's start.
            headers.optional.size_of_headers = 0;
            return Ok((headers, sections, Some(elf)));
        }
        n64::Boot::Flat { address, size } => (address, size),
    };
    let base = n64::fold(address.into()).map_or(u64::from(address), |(_, va)| va);
    let sections = vec![Section {
        name: ".text".to_owned(),
        virtual_size: size,
        virtual_address: 0,
        raw_size: size,
        raw_offset: u32::try_from(n64::PROGRAM_OFFSET).unwrap_or(0),
        characteristics: crate::elf::characteristics(true, true, false),
    }];
    let headers = Headers {
        pe_offset: 0,
        coff: CoffHeader {
            machine: Machine::Mips,
            sections: 1,
            timestamp: 0,
            symbol_table: 0,
            symbols: 0,
            optional_header_size: 0,
            characteristics: CoffHeader::EXECUTABLE_IMAGE | CoffHeader::RELOCS_STRIPPED,
        },
        optional: OptionalHeader {
            magic: OptionalMagic::Pe32,
            linker_version: (0, 0),
            size_of_code: size,
            size_of_initialized_data: 0,
            size_of_uninitialized_data: 0,
            entry_point: 0,
            base_of_code: 0,
            image_base: base,
            section_alignment: 1,
            file_alignment: 1,
            os_version: (0, 0),
            image_version: (0, 0),
            subsystem_version: (0, 0),
            size_of_image: size,
            size_of_headers: 0,
            checksum: 0,
            subsystem: 0,
            dll_characteristics: 0,
            stack_reserve: 0,
            stack_commit: 0,
            heap_reserve: 0,
            heap_commit: 0,
            directories: vec![DataDirectory::default(); 16],
        },
        section_table_offset: 0,
        rich: None,
    };
    Ok((headers, sections, None))
}

#[cfg(test)]
mod tests;
