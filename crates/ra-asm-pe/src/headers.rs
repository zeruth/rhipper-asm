//! The PE headers: the DOS header, the COFF file header and the optional header with its data
//! directories, as the loader reads them, and the rich header of Microsoft's linker before them.
//! Every read names the offset of a malformed field.

use anyhow::{Result, ensure};
use ra_asm_format::Reader;
use serde::Serialize;

pub const DOS_MAGIC: u16 = 0x5a4d;
pub const PE_SIGNATURE: u32 = 0x0000_4550;

/// The COFF machine types the loader knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Machine {
    I386,
    Amd64,
    Arm64,
    /// A machine the pipeline does not lift. The value is kept for the report.
    Other(u16),
}

impl Machine {
    /// The machine of a COFF `Machine` field value.
    pub fn of(value: u16) -> Self {
        match value {
            0x014c => Self::I386,
            0x8664 => Self::Amd64,
            0xaa64 => Self::Arm64,
            other => Self::Other(other),
        }
    }

    /// The width of a pointer, in bytes.
    pub fn pointer_size(self) -> u8 {
        match self {
            Self::I386 => 4,
            Self::Amd64 | Self::Arm64 | Self::Other(_) => 8,
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::I386 => "x86".to_owned(),
            Self::Amd64 => "x86_64".to_owned(),
            Self::Arm64 => "arm64".to_owned(),
            Self::Other(value) => format!("machine {value:#06x}"),
        }
    }
}

/// The COFF file header.
#[derive(Debug, Clone, Serialize)]
pub struct CoffHeader {
    pub machine: Machine,
    pub sections: u16,
    pub timestamp: u32,
    pub symbol_table: u32,
    pub symbols: u32,
    pub optional_header_size: u16,
    pub characteristics: u16,
}

impl CoffHeader {
    pub const RELOCS_STRIPPED: u16 = 0x0001;
    pub const EXECUTABLE_IMAGE: u16 = 0x0002;
    pub const LARGE_ADDRESS_AWARE: u16 = 0x0020;
    pub const DLL: u16 = 0x2000;

    pub fn is_dll(&self) -> bool {
        self.characteristics & Self::DLL != 0
    }
}

/// The magic of the optional header: PE32 or PE32+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionalMagic {
    Pe32,
    Pe32Plus,
}

/// One entry of the data directories: where a table is in the image and how long it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DataDirectory {
    pub rva: u32,
    pub size: u32,
}

impl DataDirectory {
    pub fn is_empty(self) -> bool {
        self.rva == 0 || self.size == 0
    }
}

/// The index of each data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Directory {
    Export = 0,
    Import = 1,
    Resource = 2,
    Exception = 3,
    Security = 4,
    BaseReloc = 5,
    Debug = 6,
    Architecture = 7,
    GlobalPtr = 8,
    Tls = 9,
    LoadConfig = 10,
    BoundImport = 11,
    Iat = 12,
    DelayImport = 13,
    ClrRuntime = 14,
}

/// The optional header. Every field is kept: the report prints them, and the loader needs most.
#[derive(Debug, Clone, Serialize)]
pub struct OptionalHeader {
    pub magic: OptionalMagic,
    pub linker_version: (u8, u8),
    pub size_of_code: u32,
    pub size_of_initialized_data: u32,
    pub size_of_uninitialized_data: u32,
    /// The address of the entry point, relative to the image base. Zero in a DLL with no entry.
    pub entry_point: u32,
    pub base_of_code: u32,
    /// The image base of the mapping.
    pub image_base: u64,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub os_version: (u16, u16),
    pub image_version: (u16, u16),
    pub subsystem_version: (u16, u16),
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub checksum: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub stack_reserve: u64,
    pub stack_commit: u64,
    pub heap_reserve: u64,
    pub heap_commit: u64,
    pub directories: Vec<DataDirectory>,
}

impl OptionalHeader {
    pub const DYNAMIC_BASE: u16 = 0x0040;
    pub const NX_COMPAT: u16 = 0x0100;
    pub const HIGH_ENTROPY_VA: u16 = 0x0020;

    /// The directory at `index`, or an empty one when the header has fewer directories.
    pub fn directory(&self, index: Directory) -> DataDirectory {
        self.directories.get(index as usize).copied().unwrap_or_default()
    }

    pub fn is_64(&self) -> bool {
        self.magic == OptionalMagic::Pe32Plus
    }

    pub fn subsystem_name(&self) -> &'static str {
        match self.subsystem {
            1 => "native",
            2 => "windows_gui",
            3 => "windows_cui",
            7 => "posix_cui",
            9 => "windows_ce_gui",
            10 => "efi_application",
            11 => "efi_boot_service_driver",
            12 => "efi_runtime_driver",
            13 => "efi_rom",
            14 => "xbox",
            16 => "windows_boot_application",
            _ => "unknown",
        }
    }
}

/// The whole header set of an image.
#[derive(Debug, Clone, Serialize)]
pub struct Headers {
    /// The file offset of the `PE\0\0` signature.
    pub pe_offset: u32,
    pub coff: CoffHeader,
    pub optional: OptionalHeader,
    /// The file offset of the first section header.
    pub section_table_offset: usize,
    /// The rich header Microsoft's linker writes between the DOS stub and the PE signature;
    /// `None` when another linker made the image or the header was stripped.
    pub rich: Option<RichHeader>,
}

/// One tool of the rich header: a product (the compiler, the linker, the assembler of one
/// toolset), the build of it, and how many objects it made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RichEntry {
    pub product: u16,
    pub build: u16,
    pub count: u32,
}

/// What a rich header's product is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RichTool {
    /// The imported functions of the image, counted.
    Imports,
    Compiler,
    Linker,
    Assembler,
    ImportLibrary,
    Export,
    Resource,
    Other,
}

impl RichEntry {
    /// The product's name in Microsoft's own table (`Utc1900_C`, `Linker1400`), for the products
    /// of the toolsets from Visual Studio 2008 on that the engine fingerprints; `None` for another.
    pub fn product_name(self) -> Option<&'static str> {
        Some(match self.product {
            0x0001 => "Import0",
            0x0083..=0x008f => [
                "Utc1500_C",
                "Utc1500_CPP",
                "Utc1500_C_Std",
                "Utc1500_CPP_Std",
                "Utc1500_CVTCIL_C",
                "Utc1500_CVTCIL_CPP",
                "Utc1500_LTCG_C",
                "Utc1500_LTCG_CPP",
                "Utc1500_LTCG_MSIL",
                "Utc1500_POGO_I_C",
                "Utc1500_POGO_I_CPP",
                "Utc1500_POGO_O_C",
                "Utc1500_POGO_O_CPP",
            ][usize::from(self.product - 0x0083)],
            0x0091 => "Linker900",
            0x0092 => "Export900",
            0x0093 => "Implib900",
            0x0094 => "Cvtres900",
            0x0095 => "Masm900",
            0x0096 => "AliasObj900",
            0x0097 => "Resource",
            0x0098 => "AliasObj1000",
            0x0099 => "Cvtpgd1600",
            0x009a => "Cvtres1000",
            0x009b => "Export1000",
            0x009c => "Implib1000",
            0x009d => "Linker1000",
            0x009e => "Masm1000",
            0x00aa..=0x00b4 => [
                "Utc1600_C",
                "Utc1600_CPP",
                "Utc1600_CVTCIL_C",
                "Utc1600_CVTCIL_CPP",
                "Utc1600_LTCG_C",
                "Utc1600_LTCG_CPP",
                "Utc1600_LTCG_MSIL",
                "Utc1600_POGO_I_C",
                "Utc1600_POGO_I_CPP",
                "Utc1600_POGO_O_C",
                "Utc1600_POGO_O_CPP",
            ][usize::from(self.product - 0x00aa)],
            0x00fd => "AliasObj1400",
            0x00fe => "Cvtpgd1900",
            0x00ff => "Cvtres1400",
            0x0100 => "Export1400",
            0x0101 => "Implib1400",
            0x0102 => "Linker1400",
            0x0103 => "Masm1400",
            0x0104..=0x010e => [
                "Utc1900_C",
                "Utc1900_CPP",
                "Utc1900_CVTCIL_C",
                "Utc1900_CVTCIL_CPP",
                "Utc1900_LTCG_C",
                "Utc1900_LTCG_CPP",
                "Utc1900_LTCG_MSIL",
                "Utc1900_POGO_I_C",
                "Utc1900_POGO_I_CPP",
                "Utc1900_POGO_O_C",
                "Utc1900_POGO_O_CPP",
            ][usize::from(self.product - 0x0104)],
            _ => return None,
        })
    }

    /// What the product is, by its name; [`RichTool::Other`] for a product the table lacks.
    pub fn tool(self) -> RichTool {
        let Some(name) = self.product_name() else { return RichTool::Other };
        if name == "Import0" {
            RichTool::Imports
        } else if name.starts_with("Utc") {
            RichTool::Compiler
        } else if name.starts_with("Linker") {
            RichTool::Linker
        } else if name.starts_with("Masm") {
            RichTool::Assembler
        } else if name.starts_with("Implib") {
            RichTool::ImportLibrary
        } else if name.starts_with("Export") {
            RichTool::Export
        } else if name.starts_with("Cvtres") || name == "Resource" {
            RichTool::Resource
        } else {
            RichTool::Other
        }
    }
}

/// The rich header: the tools that made the objects of the image, as the linker recorded them,
/// each entry masked by a key the linker derived from the DOS header and the entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RichHeader {
    /// The file offset of the masked `DanS` that opens the header.
    pub offset: usize,
    pub key: u32,
    /// The entries in the linker's order.
    pub entries: Vec<RichEntry>,
}

impl RichHeader {
    const RICH: u32 = 0x6863_6952;
    const DANS: u32 = 0x536e_6144;

    /// The rich header between the DOS header and the PE signature at `pe_offset`, or `None`
    /// when there is none or its `DanS` does not unmask.
    pub fn parse(reader: Reader<'_>, pe_offset: usize) -> Option<Self> {
        let end = pe_offset.min(reader.len());
        let rich = (0x40..end.saturating_sub(7)).step_by(4).find(|&o| reader.u32(o, "Rich").ok() == Some(Self::RICH))?;
        let key = reader.u32(rich + 4, "the rich key").ok()?;
        let offset = (0x40..rich).step_by(4).rev().find(|&o| reader.u32(o, "DanS").is_ok_and(|v| (v ^ key) == Self::DANS))?;
        // `DanS` is followed by three masked zeros, then the entries up to `Rich`.
        let mut entries = Vec::new();
        let mut at = offset + 16;
        while at + 8 <= rich {
            let id = reader.u32(at, "a rich entry").ok()? ^ key;
            let count = reader.u32(at + 4, "a rich entry").ok()? ^ key;
            entries.push(RichEntry { product: (id >> 16) as u16, build: id as u16, count });
            at += 8;
        }
        Some(Self { offset, key, entries })
    }

    /// The linker's entry: the one product of the header that links.
    pub fn linker(&self) -> Option<RichEntry> {
        self.entries.iter().copied().find(|e| e.tool() == RichTool::Linker)
    }
}

impl Headers {
    /// Reads the headers off the start of a file.
    pub fn parse(reader: Reader<'_>) -> Result<Self> {
        let magic = reader.u16(0, "the DOS magic")?;
        ensure!(magic == DOS_MAGIC, "the DOS magic at offset 0x0 is {magic:#06x}, not MZ");
        let pe_offset = reader.u32(0x3c, "e_lfanew")?;
        let signature = reader.u32(pe_offset as usize, "the PE signature")?;
        ensure!(signature == PE_SIGNATURE, "the PE signature at offset {pe_offset:#x} is {signature:#010x}, not PE\\0\\0");

        let mut c = reader.at(pe_offset as usize + 4);
        let coff = CoffHeader {
            machine: Machine::of(c.u16("Machine")?),
            sections: c.u16("NumberOfSections")?,
            timestamp: c.u32("TimeDateStamp")?,
            symbol_table: c.u32("PointerToSymbolTable")?,
            symbols: c.u32("NumberOfSymbols")?,
            optional_header_size: c.u16("SizeOfOptionalHeader")?,
            characteristics: c.u16("Characteristics")?,
        };
        let optional_offset = c.position();
        ensure!(coff.optional_header_size >= 2, "SizeOfOptionalHeader is {}: the image has no optional header", coff.optional_header_size);

        let magic = c.u16("the optional header magic")?;
        let magic = match magic {
            0x10b => OptionalMagic::Pe32,
            0x20b => OptionalMagic::Pe32Plus,
            other => anyhow::bail!("the optional header magic at offset {:#x} is {other:#06x}, not PE32 or PE32+", c.position() - 2),
        };
        let plus = magic == OptionalMagic::Pe32Plus;
        let linker_version = (c.u8("MajorLinkerVersion")?, c.u8("MinorLinkerVersion")?);
        let size_of_code = c.u32("SizeOfCode")?;
        let size_of_initialized_data = c.u32("SizeOfInitializedData")?;
        let size_of_uninitialized_data = c.u32("SizeOfUninitializedData")?;
        let entry_point = c.u32("AddressOfEntryPoint")?;
        let base_of_code = c.u32("BaseOfCode")?;
        let image_base = if plus {
            c.u64("ImageBase")?
        } else {
            c.skip(4); // BaseOfData
            u64::from(c.u32("ImageBase")?)
        };
        let section_alignment = c.u32("SectionAlignment")?;
        let file_alignment = c.u32("FileAlignment")?;
        let os_version = (c.u16("MajorOperatingSystemVersion")?, c.u16("MinorOperatingSystemVersion")?);
        let image_version = (c.u16("MajorImageVersion")?, c.u16("MinorImageVersion")?);
        let subsystem_version = (c.u16("MajorSubsystemVersion")?, c.u16("MinorSubsystemVersion")?);
        c.skip(4); // Win32VersionValue
        let size_of_image = c.u32("SizeOfImage")?;
        let size_of_headers = c.u32("SizeOfHeaders")?;
        let checksum = c.u32("CheckSum")?;
        let subsystem = c.u16("Subsystem")?;
        let dll_characteristics = c.u16("DllCharacteristics")?;
        let mut size_field = |what: &str| -> Result<u64> { if plus { c.u64(what) } else { c.u32(what).map(u64::from) } };
        let stack_reserve = size_field("SizeOfStackReserve")?;
        let stack_commit = size_field("SizeOfStackCommit")?;
        let heap_reserve = size_field("SizeOfHeapReserve")?;
        let heap_commit = size_field("SizeOfHeapCommit")?;
        c.skip(4); // LoaderFlags
        let count = c.u32("NumberOfRvaAndSizes")?;
        ensure!(count <= 16, "NumberOfRvaAndSizes is {count}, more than the 16 directories that exist");
        let mut directories = Vec::with_capacity(count as usize);
        for i in 0..count {
            let what = format!("data directory {i}");
            directories.push(DataDirectory { rva: c.u32(&what)?, size: c.u32(&what)? });
        }
        ensure!(
            section_alignment != 0 && file_alignment != 0,
            "SectionAlignment {section_alignment:#x} or FileAlignment {file_alignment:#x} is zero"
        );

        let optional = OptionalHeader {
            magic,
            linker_version,
            size_of_code,
            size_of_initialized_data,
            size_of_uninitialized_data,
            entry_point,
            base_of_code,
            image_base,
            section_alignment,
            file_alignment,
            os_version,
            image_version,
            subsystem_version,
            size_of_image,
            size_of_headers,
            checksum,
            subsystem,
            dll_characteristics,
            stack_reserve,
            stack_commit,
            heap_reserve,
            heap_commit,
            directories,
        };
        let section_table_offset = optional_offset + coff.optional_header_size as usize;
        let rich = RichHeader::parse(reader, pe_offset as usize);
        Ok(Self { pe_offset, coff, optional, section_table_offset, rich })
    }
}
