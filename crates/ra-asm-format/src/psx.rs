//! The PSX's executable, the `PS-X EXE` the BIOS loads, and the console's memory map. The file
//! is a 2 KiB header and the text: the header gives the initial `pc` and `gp`, the address the text
//! is copied to and its size, an area the BIOS clears (the BSS), the base and offset of the initial
//! `sp` and `fp`, and the region marker the BIOS checks. The memory map folds the CPU's segments
//! onto one address per place: an address of `kuseg`, `kseg0` or `kseg1` that reaches main RAM is
//! the `kseg0` address, the one the SDK links at; the scratchpad, the hardware registers and the
//! BIOS are their physical addresses.

use anyhow::{Result, bail, ensure};

use crate::Reader;

/// The identifier at the start of the file.
pub const MAGIC: &[u8; 8] = b"PS-X EXE";
/// The size of the header: the text starts at this file offset.
pub const HEADER_SIZE: usize = 0x800;

/// Main RAM, 2 MiB, at its `kseg0` address.
pub const RAM: u64 = 0x8000_0000;
pub const RAM_SIZE: u64 = 0x20_0000;
/// The data cache the R3000 keeps as 1 KiB of fast RAM.
pub const SCRATCHPAD: u64 = 0x1f80_0000;
pub const SCRATCHPAD_SIZE: u64 = 0x400;
/// The I/O ports: the interrupt controller, DMA, the timers, the CD-ROM, the GPU, the MDEC, the SPU
/// and the expansion region 2 that ends them.
pub const HARDWARE: u64 = 0x1f80_1000;
pub const HARDWARE_SIZE: u64 = 0x2000;
/// The cache control register of `kseg2`, which is no mirror of the others.
pub const CACHE_CONTROL: u64 = 0xfffe_0130;
/// The BIOS ROM, 512 KiB.
pub const BIOS: u64 = 0x1fc0_0000;
pub const BIOS_SIZE: u64 = 0x8_0000;

/// A place of the memory map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Ram,
    Scratchpad,
    /// The device registers: an access is the platform runtime's to answer, never the image's.
    Hardware,
    Bios,
}

impl Region {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ram => "ram",
            Self::Scratchpad => "scratchpad",
            Self::Hardware => "hardware",
            Self::Bios => "bios",
        }
    }
}

/// The place `va` reaches and its folded address, or `None` for an address no place answers: the
/// part of `kuseg` above the first 512 MiB, the scratchpad seen through `kseg1` (it is not on the
/// bus), and `kseg2` but its cache control register. The 2 MiB of RAM are not folded within the
/// 8 MiB window the memory controller can mirror them over: that depends on its configuration.
pub fn fold(va: u64) -> Option<(Region, u64)> {
    let va = u32::try_from(va).ok()?;
    if u64::from(va) == CACHE_CONTROL {
        return Some((Region::Hardware, CACHE_CONTROL));
    }
    let segment = va >> 29;
    let physical = u64::from(match segment {
        0 | 4 | 5 => va & 0x1fff_ffff,
        _ => return None,
    });
    let within = |base: u64, size: u64| physical >= base && physical - base < size;
    if within(0, RAM_SIZE) {
        Some((Region::Ram, RAM | physical))
    } else if within(SCRATCHPAD, SCRATCHPAD_SIZE) && segment != 5 {
        Some((Region::Scratchpad, physical))
    } else if within(HARDWARE, HARDWARE_SIZE) {
        Some((Region::Hardware, physical))
    } else if within(BIOS, BIOS_SIZE) {
        Some((Region::Bios, physical))
    } else {
        None
    }
}

/// Whether `bytes` start a PS-X EXE.
pub fn is_psx(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// A PS-X EXE's header and where its text lies in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsxExe {
    /// The initial `pc`, the entry point.
    pub pc: u32,
    /// The initial `gp`; zero when the start routine sets it, as the SDK's does.
    pub gp: u32,
    /// The address the BIOS copies the text to, and its size, a multiple of 2 KiB.
    pub text: (u32, u32),
    /// The data range the header records beside the text; the BIOS loads nothing there.
    pub data: (u32, u32),
    /// The range the BIOS clears before the start, `(0, 0)` for none.
    pub bss: (u32, u32),
    /// The base of the initial `sp` and `fp` and the offset added to it; a base of zero leaves the
    /// BIOS's own stack.
    pub stack: (u32, u32),
    /// The region marker, `Sony Computer Entertainment Inc. for North America area` and its kin;
    /// empty in a homebrew build.
    pub marker: String,
}

impl PsxExe {
    /// Reads the header of `bytes`; the text must lie whole in the file and in main RAM, the entry
    /// point in the text, and the BSS in main RAM apart from the text.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let r = Reader::new(bytes);
        let magic = r.slice(0, MAGIC.len(), "the PS-X EXE identifier")?;
        ensure!(magic == MAGIC, "the identifier at offset 0x0 is {:?}, not \"PS-X EXE\"", String::from_utf8_lossy(magic));
        let pair =
            |offset: usize, first: &str, second: &str| -> Result<(u32, u32)> { Ok((r.u32(offset, first)?, r.u32(offset + 4, second)?)) };
        let pc = r.u32(0x10, "the initial pc")?;
        let gp = r.u32(0x14, "the initial gp")?;
        let text = pair(0x18, "the text address", "the text size")?;
        let data = pair(0x20, "the data address", "the data size")?;
        let bss = pair(0x28, "the BSS address", "the BSS size")?;
        let stack = pair(0x30, "the stack base", "the stack offset")?;
        let marker = r.fixed_str(0x4c, HEADER_SIZE - 0x4c, "the region marker")?;
        r.slice(HEADER_SIZE, text.1 as usize, "the text")?;

        let in_ram = |what: &str, (address, size): (u32, u32)| -> Result<u64> {
            let start = fold(address.into()).filter(|(region, _)| *region == Region::Ram).map(|(_, va)| va);
            let start = start.filter(|start| start + u64::from(size) <= RAM + RAM_SIZE);
            let Some(start) = start else { bail!("{what} at {address:#010x} ({size:#x} bytes) does not lie in main RAM") };
            Ok(start)
        };
        let text_start = in_ram("the text", text)?;
        let text_end = text_start + u64::from(text.1);
        let entry = in_ram("the entry point", (pc, 4))?;
        ensure!(entry >= text_start && entry + 4 <= text_end, "the initial pc {pc:#010x} at offset 0x10 does not lie in the text");
        if bss.1 != 0 {
            let bss_start = in_ram("the BSS", bss)?;
            let apart = bss_start >= text_end || bss_start + u64::from(bss.1) <= text_start;
            ensure!(apart, "the BSS at {:#010x} ({:#x} bytes) overlaps the text", bss.0, bss.1);
        }
        Ok(Self { pc, gp, text, data, bss, stack, marker })
    }

    /// The text's address in the memory map, folded.
    pub fn text_address(&self) -> u64 {
        fold(self.text.0.into()).map_or(u64::from(self.text.0), |(_, va)| va)
    }

    /// The entry point's address in the memory map, folded.
    pub fn entry(&self) -> u64 {
        fold(self.pc.into()).map_or(u64::from(self.pc), |(_, va)| va)
    }

    /// The BSS's address in the memory map, folded, and its size; `None` without one.
    pub fn bss_range(&self) -> Option<(u64, u32)> {
        (self.bss.1 != 0).then(|| (fold(self.bss.0.into()).map_or(u64::from(self.bss.0), |(_, va)| va), self.bss.1))
    }

    /// The initial `sp` and `fp`, when the header sets them.
    pub fn initial_sp(&self) -> Option<u32> {
        (self.stack.0 != 0).then(|| self.stack.0.wrapping_add(self.stack.1))
    }
}
