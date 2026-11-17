//! The PSX's memory map as places with names: main RAM, which the text of a PS-X EXE lands in and
//! which `kuseg`, `kseg0` and `kseg1` mirror, and the BIOS, the scratchpad and the hardware
//! registers, which the map names but the image holds no byte of. [`ra_asm_format::psx::fold`]
//! answers for one address; this is the whole map, for a reader that lists it.

use ra_asm_format::psx::{self, Region};

/// The base of `kuseg`, the user segment, mapped (on the PSX) straight onto the physical bus.
pub const KUSEG: u64 = 0x0000_0000;
/// The base of `kseg0`, the cached kernel segment: the one the SDK links at.
pub const KSEG0: u64 = 0x8000_0000;
/// The base of `kseg1`, the uncached kernel segment.
pub const KSEG1: u64 = 0xa000_0000;

/// A place of the memory map: its region, its physical range, and the segments that reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub region: Region,
    /// The physical address of its first byte.
    pub start: u64,
    pub size: u64,
    /// The bases of the segments it is seen through, each a mirror of the same bytes.
    pub segments: &'static [u64],
}

impl Place {
    /// The name of the place, as [`Region::name`] gives it.
    pub fn name(&self) -> &'static str {
        self.region.name()
    }

    /// Whether the image holds bytes there: only RAM, where the BIOS copies the text. The others
    /// are the console's, which the platform runtime answers.
    pub fn holds_bytes(&self) -> bool {
        self.region == Region::Ram
    }

    /// The address of its first byte in each segment that reaches it, in the segments' order.
    pub fn mirrors(&self) -> impl Iterator<Item = u64> {
        self.segments.iter().map(move |segment| segment | self.start)
    }
}

/// The places of the PSX's memory map, in physical address order. The scratchpad is not on
/// `kseg1`'s bus, and `kseg2` holds only the cache control register, which no segment mirrors.
pub const MAP: [Place; 4] = [
    Place { region: Region::Ram, start: 0, size: psx::RAM_SIZE, segments: &[KUSEG, KSEG0, KSEG1] },
    Place { region: Region::Scratchpad, start: psx::SCRATCHPAD, size: psx::SCRATCHPAD_SIZE, segments: &[KUSEG, KSEG0] },
    Place { region: Region::Hardware, start: psx::HARDWARE, size: psx::HARDWARE_SIZE, segments: &[KUSEG, KSEG0, KSEG1] },
    Place { region: Region::Bios, start: psx::BIOS, size: psx::BIOS_SIZE, segments: &[KUSEG, KSEG0, KSEG1] },
];
