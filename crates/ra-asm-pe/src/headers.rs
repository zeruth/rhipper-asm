//! The PE headers: the DOS header, the COFF header and the optional header, as the loader reads
//! them. Every read names the offset of a malformed field.

/// The COFF machine types the loader knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Machine {
    I386,
    Amd64,
    Arm64,
}

impl Machine {
    /// The machine of a COFF `Machine` field value.
    pub fn of(value: u16) -> Option<Self> {
        match value {
            0x014c => Some(Self::I386),
            0x8664 => Some(Self::Amd64),
            0xaa64 => Some(Self::Arm64),
            _ => None,
        }
    }
}

/// The COFF file header.
#[derive(Debug, Clone)]
pub struct CoffHeader {
    pub machine: Machine,
    pub sections: u16,
    pub timestamp: u32,
    pub optional_header_size: u16,
    pub characteristics: u16,
}

/// The magic of the optional header: PE32 or PE32+.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalMagic {
    Pe32,
    Pe32Plus,
}

/// The optional header, reduced to the fields the pipeline needs.
#[derive(Debug, Clone)]
pub struct OptionalHeader {
    pub magic: OptionalMagic,
    /// The image base of the mapping.
    pub image_base: u64,
    /// The address of the entry point, relative to the image base.
    pub entry_point: u32,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub size_of_image: u32,
}

/// The whole header set of an image.
#[derive(Debug, Clone)]
pub struct Headers {
    pub coff: CoffHeader,
    pub optional: OptionalHeader,
}
