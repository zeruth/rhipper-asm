//! The mapped image: the file laid out by RVA as the Windows loader lays it out, with reads by
//! virtual address that refuse an address no section holds. An XBE ([`crate::xbe`]) maps the same
//! way, its header standing where the PE headers would; an ELF file ([`crate::elf`]) maps by its
//! loaded segments, from the lowest one's page.

use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_format::Reader;

use crate::debug::{self, DebugEntry};
use crate::exception::{self, RuntimeFunction};
use crate::exports::{self, ExportTable};
use crate::headers::{Headers, Machine};
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
    /// What the XBE header holds beyond the PE's, when the image is one.
    pub xbe: Option<Xbe>,
    /// The ELF file the image was mapped from, when it is one: its interpreter, its libraries and
    /// the function extents of its `.eh_frame`, which no PE table holds.
    pub elf: Option<ra_asm_format::elf::ElfFile>,
    /// What the loader could not read. A directory that fails to parse lands here, not in an error.
    pub warnings: Vec<String>,
    file: Vec<u8>,
    mapped: Vec<u8>,
}

impl Image {
    /// Parses the bytes of a PE file (or an XBE) and maps them.
    pub fn parse(file: Vec<u8>) -> Result<Self> {
        let reader = Reader::new(&file);
        let (headers, sections, xbe, elf) = if xbe::is_xbe(reader) {
            let (headers, sections, xbe) = xbe::parse(reader)?;
            (headers, sections, Some(xbe), None)
        } else if ra_asm_format::elf::is_elf(&file) {
            let (headers, sections, elf) = crate::elf::parse(&file)?;
            (headers, sections, None, Some(elf))
        } else {
            let headers = Headers::parse(reader)?;
            let mut sections = Vec::with_capacity(headers.coff.sections as usize);
            let mut c = reader.at(headers.section_table_offset);
            for index in 0..headers.coff.sections as usize {
                sections.push(Section::parse(&mut c, index)?);
            }
            (headers, sections, None, None)
        };

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
            xbe,
            elf,
            warnings,
            file,
            mapped,
        };
        image.read_directories();
        Ok(image)
    }

    fn read_directories(&mut self) {
        // An ELF file has none of the PE's directories: its tables are its own.
        if self.elf.is_some() {
            crate::elf::read_tables(self);
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

    pub fn image_base(&self) -> u64 {
        self.headers.optional.image_base
    }

    pub fn size_of_image(&self) -> u32 {
        self.headers.optional.size_of_image
    }

    /// The virtual address of the entry point, or `None` when the image has none.
    pub fn entry_point(&self) -> Option<u64> {
        let rva = self.headers.optional.entry_point;
        (rva != 0).then(|| self.va(rva))
    }

    /// The virtual address of an RVA.
    pub fn va(&self, rva: u32) -> u64 {
        self.image_base() + u64::from(rva)
    }

    /// The RVA of a virtual address inside the image, or `None` when it lies outside.
    pub fn rva(&self, va: u64) -> Option<u32> {
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

    /// Whether `va` lies in a section that holds code or is executable.
    pub fn is_code(&self, va: u64) -> bool {
        self.section_of(va).is_some_and(|s| s.is_code() || s.is_executable())
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
        let Some(section) = self.section_of(va) else { bail!("no section holds address {va:#x}") };
        let end = self.va(section.end_rva()).min(self.va(self.size_of_image()));
        self.read(va, (end - va) as usize)
    }

    /// A [`Reader`] over the bytes at `va` to the end of the section.
    pub fn reader_at(&self, va: u64) -> Result<Reader<'_>> {
        Ok(Reader::new(self.read_to_section_end(va)?))
    }

    pub fn u8(&self, va: u64) -> Result<u8> {
        Ok(self.read(va, 1)?[0])
    }

    pub fn u16(&self, va: u64) -> Result<u16> {
        let b = self.read(va, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&self, va: u64) -> Result<u32> {
        let b = self.read(va, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&self, va: u64) -> Result<u64> {
        let b = self.read(va, 8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// A pointer-width value at `va`.
    pub fn pointer(&self, va: u64) -> Result<u64> {
        if self.is_64() { self.u64(va) } else { self.u32(va).map(u64::from) }
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

    /// The import whose IAT slot is at `va`.
    pub fn import_at(&self, va: u64) -> Option<&Import> {
        self.imports.iter().find(|i| i.iat_va == va)
    }
}
