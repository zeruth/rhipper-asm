//! The Nintendo 64's cartridge ROM and the console's memory map. A ROM is a 64-byte header, the
//! boot code (IPL3) at `0x40` to `0x1000`, and the program. Its dumps come in three byte orders,
//! told apart by the first word: `.z64` is the cartridge's own big-endian order, `.v64` swaps the
//! bytes of every halfword, `.n64` reverses every word; [`to_z64`] folds the other two back.
//!
//! The boot code decides where the program lands. Nintendo's copies the 1 MiB after it to the
//! header's entry point and jumps there, and which of its variants a cartridge carries is the CIC
//! chip the cartridge pairs it with: the 6103 and the 6106 variants move the entry down by 1 MiB
//! and 2 MiB. libdragon's own boot code instead looks for an ELF file at a 256-byte boundary of
//! the ROM from `0x1000` on, loads its segments and enters it at the ELF's entry point.
//!
//! The memory map folds the VR4300's segments onto one address per place: RDRAM seen through
//! `kseg0` or `kseg1` is its `kseg0` address, the one programs link at; the RCP's registers and
//! the cartridge domain are their `kseg1` addresses, the uncached ones a program reaches them
//! through. `kuseg`, `ksseg` and `kseg3` go through the TLB, which the image does not know, so no
//! place answers them.

use anyhow::{Result, bail, ensure};

use crate::psx::Region;
use crate::{ByteOrder, Reader};

/// The first word of a `.z64` ROM: the PI's domain 1 timing and the clock rate the header asks for.
pub const MAGIC: [u8; 4] = [0x80, 0x37, 0x12, 0x40];
/// The header's size: the boot code starts here.
pub const HEADER_SIZE: usize = 0x40;
/// Where the program starts in the ROM: the end of the boot code.
pub const PROGRAM_OFFSET: usize = 0x1000;
/// How much of the program Nintendo's boot code copies to the entry point.
pub const PROGRAM_SIZE: usize = 0x10_0000;
/// The stride and the end of libdragon's search for its ELF file: the boot code reads the ROM
/// from [`PROGRAM_OFFSET`] up to 64 MiB, one candidate every 256 bytes.
pub const ELF_ALIGN: usize = 0x100;
pub const ELF_SEARCH_END: usize = 0x400_0000;

/// RDRAM, 8 MiB with the Expansion Pak, at its `kseg0` address.
pub const RDRAM: u64 = 0x8000_0000;
pub const RDRAM_SIZE: u64 = 0x80_0000;
/// The RCP: the RSP's memories and registers, the RDP's, and the MIPS interface, video, audio,
/// peripheral, RDRAM and serial interfaces, from SP at `0xa4000000` to SI's at `0xa4800000`.
pub const RCP: u64 = 0xa400_0000;
pub const RCP_SIZE: u64 = 0x90_0000;
/// The cartridge domain the PI maps the ROM in, and the development registers beside it (the
/// ISViewer buffer at `0xb3ff0020`).
pub const CARTRIDGE: u64 = 0xb000_0000;
pub const CARTRIDGE_SIZE: u64 = 0xfc0_0000;

/// The byte order a ROM dump is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// The cartridge's order, big-endian words.
    Z64,
    /// The bytes of every halfword swapped.
    V64,
    /// Every word reversed: little-endian words.
    N64,
}

impl Order {
    /// The order the first word of `bytes` says, or `None` for a file that is no ROM.
    pub fn of(bytes: &[u8]) -> Option<Self> {
        let word: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        let [a, b, c, d] = MAGIC;
        if word == MAGIC {
            Some(Self::Z64)
        } else if word == [b, a, d, c] {
            Some(Self::V64)
        } else if word == [d, c, b, a] {
            Some(Self::N64)
        } else {
            None
        }
    }

    /// The extension a dump in this order carries.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Z64 => "z64",
            Self::V64 => "v64",
            Self::N64 => "n64",
        }
    }
}

/// Whether `bytes` start a ROM in any of the three orders.
pub fn is_rom(bytes: &[u8]) -> bool {
    Order::of(bytes).is_some()
}

/// Puts `bytes`, a ROM in `order`, in the cartridge's order. A swapped dump is whole halfwords or
/// whole words; one that is not was cut short.
pub fn to_z64(bytes: &mut [u8], order: Order) -> Result<()> {
    let unit = match order {
        Order::Z64 => return Ok(()),
        Order::V64 => 2,
        Order::N64 => 4,
    };
    ensure!(bytes.len().is_multiple_of(unit), "the .{} ROM's {:#x} bytes are not whole {unit}-byte units", order.extension(), bytes.len());
    for chunk in bytes.chunks_exact_mut(unit) {
        chunk.reverse();
    }
    Ok(())
}

/// The CIC a cartridge pairs its boot code with, named by the NTSC chip; the PAL chip of the same
/// number but the 7 runs the same boot code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cic {
    Cic6101,
    Cic6102,
    Cic6103,
    Cic6105,
    Cic6106,
    /// The PAL 6101's boot code, which differs from the NTSC one.
    Cic7102,
    /// The 64DD's.
    Cic8303,
}

/// The CRC-32 of the boot code of every CIC, `0x40` to `0x1000`.
const BOOT_CODE_CRCS: [(u32, Cic); 7] = [
    (0x6170_a4a1, Cic::Cic6101),
    (0x90bb_6cb5, Cic::Cic6102),
    (0x0b05_0ee0, Cic::Cic6103),
    (0x98bc_2c86, Cic::Cic6105),
    (0xacc8_580a, Cic::Cic6106),
    (0x009e_9ea3, Cic::Cic7102),
    (0x0e01_8159, Cic::Cic8303),
];

impl Cic {
    /// The CIC whose boot code `boot_code` (the ROM's `0x40` to `0x1000`) is, or `None` for boot
    /// code of no Nintendo CIC, as libdragon's is.
    pub fn of(boot_code: &[u8]) -> Option<Self> {
        let crc = crc32(boot_code);
        BOOT_CODE_CRCS.iter().find(|(c, _)| *c == crc).map(|&(_, cic)| cic)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Cic6101 => "6101",
            Self::Cic6102 => "6102",
            Self::Cic6103 => "6103",
            Self::Cic6105 => "6105",
            Self::Cic6106 => "6106",
            Self::Cic7102 => "7102",
            Self::Cic8303 => "8303",
        }
    }

    /// The address this CIC's boot code enters for a header's `entry`: the 6103's and the 6106's
    /// boot code subtract 1 MiB and 2 MiB from it, the others take it as it is.
    pub fn entry(self, entry: u32) -> u32 {
        match self {
            Self::Cic6103 => entry.wrapping_sub(0x10_0000),
            Self::Cic6106 => entry.wrapping_sub(0x20_0000),
            _ => entry,
        }
    }
}

/// The CRC-32 of IEEE 802.3, the one the CIC table is computed with.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 { crc >> 1 } else { (crc >> 1) ^ 0xedb8_8320 };
        }
    }
    !crc
}

/// How the boot code brings the program in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boot {
    /// Nintendo's: the [`PROGRAM_SIZE`] bytes at [`PROGRAM_OFFSET`], or as many as the ROM has,
    /// copied to `address` and entered there.
    Flat { address: u32, size: u32 },
    /// libdragon's: the ELF file at `offset` of the ROM, loaded by its segments.
    Elf { offset: usize },
}

/// A ROM's header and how it boots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rom {
    /// The order the dump was in before [`to_z64`] folded it.
    pub order: Order,
    /// The header's entry point, before the CIC moves it.
    pub entry: u32,
    /// The two checksums of the program the boot code verifies.
    pub checksums: (u32, u32),
    /// The game's title, its padding cut.
    pub title: String,
    /// The category, the two-letter game id and the region, as the header's four bytes at `0x3b`.
    pub game_code: String,
    pub version: u8,
    /// The CIC the boot code hashes to; `None` for boot code of no Nintendo CIC.
    pub cic: Option<Cic>,
    pub boot: Boot,
}

impl Rom {
    /// Reads the header of `rom`, already in the `.z64` order that `order` was folded from. Boot
    /// code of a known CIC boots flat; other boot code boots the ELF file it finds, else boots
    /// flat at the header's entry as it is.
    pub fn parse(rom: &[u8], order: Order) -> Result<Self> {
        let r = Reader::new(rom).with_order(ByteOrder::Big);
        let magic = r.slice(0, 4, "the ROM's first word")?;
        ensure!(magic == MAGIC, "the first word at offset 0x0 is {magic:02x?}, not a .z64 ROM's 80 37 12 40");
        let entry = r.u32(0x08, "the entry point")?;
        let checksums = (r.u32(0x10, "the first checksum")?, r.u32(0x14, "the second checksum")?);
        let title = r.fixed_str(0x20, 0x14, "the title")?.trim_end().to_owned();
        let game_code =
            r.slice(0x3b, 4, "the game code")?.iter().map(|&b| if b.is_ascii_graphic() { char::from(b) } else { '.' }).collect();
        let version = r.u8(0x3f, "the version")?;
        let cic = Cic::of(r.slice(HEADER_SIZE, PROGRAM_OFFSET - HEADER_SIZE, "the boot code")?);
        let elf = if cic.is_none() { find_elf(rom) } else { None };
        let boot = if let Some(offset) = elf {
            Boot::Elf { offset }
        } else {
            ensure!(rom.len() > PROGRAM_OFFSET, "the ROM's {:#x} bytes end before the program at offset {PROGRAM_OFFSET:#x}", rom.len());
            let address = cic.map_or(entry, |cic| cic.entry(entry));
            let size = u32::try_from((rom.len() - PROGRAM_OFFSET).min(PROGRAM_SIZE)).unwrap_or(0);
            let Some((Region::Ram, _)) = fold(address.into()) else {
                bail!("the entry point {address:#010x} the header gives at offset 0x8 does not lie in RDRAM")
            };
            ensure!(
                fold(u64::from(address) + u64::from(size) - 1).is_some_and(|(region, _)| region == Region::Ram),
                "the {size:#x} bytes the boot code copies to {address:#010x} run past RDRAM"
            );
            Boot::Flat { address, size }
        };
        Ok(Self { order, entry, checksums, title, game_code, version, cic, boot })
    }
}

/// The offset of the first ELF file at a 256-byte boundary from [`PROGRAM_OFFSET`] on, where
/// libdragon's boot code looks for it.
pub fn find_elf(rom: &[u8]) -> Option<usize> {
    let end = rom.len().min(ELF_SEARCH_END);
    (PROGRAM_OFFSET..end).step_by(ELF_ALIGN).find(|&offset| rom[offset..].starts_with(b"\x7fELF"))
}

/// The place `va` reaches and its folded address, or `None` for an address no place answers: the
/// TLB's segments, and the parts of the physical space that are neither RDRAM, nor the RCP, nor
/// the cartridge domain. The RCP and the cartridge are devices, answered by the platform runtime.
pub fn fold(va: u64) -> Option<(Region, u64)> {
    // A 64-bit address of the 32-bit compatibility segments is the sign extension of a 32-bit one.
    let va = if va >> 32 == 0xffff_ffff { va & 0xffff_ffff } else { va };
    let va = u32::try_from(va).ok()?;
    if !matches!(va >> 29, 4 | 5) {
        return None;
    }
    let physical = u64::from(va & 0x1fff_ffff);
    let within = |base: u64, size: u64| {
        let base = base & 0x1fff_ffff;
        physical >= base && physical - base < size
    };
    if within(RDRAM, RDRAM_SIZE) {
        Some((Region::Ram, RDRAM | physical))
    } else if within(RCP, RCP_SIZE) || within(CARTRIDGE, CARTRIDGE_SIZE) {
        Some((Region::Hardware, 0xa000_0000 | physical))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_is_ieee_802_3() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn the_first_word_names_the_order() {
        assert_eq!(Order::of(&[0x80, 0x37, 0x12, 0x40, 0]), Some(Order::Z64));
        assert_eq!(Order::of(&[0x37, 0x80, 0x40, 0x12]), Some(Order::V64));
        assert_eq!(Order::of(&[0x40, 0x12, 0x37, 0x80]), Some(Order::N64));
        assert_eq!(Order::of(b"\x7fELF"), None);
        assert_eq!(Order::of(&[0x80, 0x37]), None);
    }

    #[test]
    fn the_swapped_orders_fold_back() {
        let z64 = [0x80, 0x37, 0x12, 0x40, 1, 2, 3, 4];
        let mut v64 = [0x37, 0x80, 0x40, 0x12, 2, 1, 4, 3];
        let mut n64 = [0x40, 0x12, 0x37, 0x80, 4, 3, 2, 1];
        to_z64(&mut v64, Order::V64).unwrap();
        to_z64(&mut n64, Order::N64).unwrap();
        assert_eq!((v64, n64), (z64, z64));
        let err = to_z64(&mut [0x40, 0x12, 0x37, 0x80, 1, 2], Order::N64).unwrap_err();
        assert!(err.to_string().contains(".n64"), "{err}");
    }

    #[test]
    fn the_6103_and_the_6106_move_the_entry() {
        assert_eq!(Cic::Cic6102.entry(0x8000_0400), 0x8000_0400);
        assert_eq!(Cic::Cic6103.entry(0x8010_0400), 0x8000_0400);
        assert_eq!(Cic::Cic6106.entry(0x8020_0400), 0x8000_0400);
    }

    #[test]
    fn the_memory_map_folds_the_mirrors_of_rdram_onto_kseg0() {
        for va in [0x8000_0400, 0xa000_0400, 0xffff_ffff_a000_0400] {
            assert_eq!(fold(va), Some((Region::Ram, 0x8000_0400)), "{va:#x}");
        }
        assert_eq!(fold(0x807f_fffc), Some((Region::Ram, 0x807f_fffc)));
        assert_eq!(fold(0x8080_0000), None, "RDRAM is 8 MiB");
        assert_eq!(fold(0x0000_0400), None, "kuseg is the TLB's");
        assert_eq!(fold(0xc000_0000), None, "ksseg is the TLB's");
        assert_eq!(fold(0x8440_0000), Some((Region::Hardware, 0xa440_0000)));
        assert_eq!(fold(0xa480_0018), Some((Region::Hardware, 0xa480_0018)));
        assert_eq!(fold(0xa490_0000), None, "SI's registers end the RCP");
        assert_eq!(fold(0xb3ff_0020), Some((Region::Hardware, 0xb3ff_0020)));
        assert_eq!(fold(0xbfc0_0000), None, "the PIF is not the cartridge's");
    }

    #[test]
    fn a_rom_cut_before_its_program_is_refused() {
        let mut rom = vec![0; PROGRAM_OFFSET];
        rom[..4].copy_from_slice(&MAGIC);
        rom[8..12].copy_from_slice(&0x8000_0400u32.to_be_bytes());
        let err = Rom::parse(&rom, Order::Z64).unwrap_err();
        assert!(err.to_string().contains("0x1000"), "{err}");
    }

    #[test]
    fn a_flat_rom_boots_at_the_header_entry() {
        let mut rom = vec![0; PROGRAM_OFFSET + 0x800];
        rom[..4].copy_from_slice(&MAGIC);
        rom[8..12].copy_from_slice(&0x8000_0400u32.to_be_bytes());
        rom[0x20..0x26].copy_from_slice(b"TITLE ");
        let parsed = Rom::parse(&rom, Order::V64).unwrap();
        assert_eq!(parsed.boot, Boot::Flat { address: 0x8000_0400, size: 0x800 });
        assert_eq!((parsed.title.as_str(), parsed.cic, parsed.order), ("TITLE", None, Order::V64));
    }
}
