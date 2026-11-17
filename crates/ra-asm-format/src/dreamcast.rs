//! The Dreamcast's boot program, `1ST_READ.BIN`: a raw SH-4 image with no header, loaded and
//! entered at [`BASE`]. On a disc image of the CD-ROM kind (the MIL-CD form homebrew uses) the boot
//! ROM reads the file scrambled, its 32-byte slices permuted in chunks by a seeded generator, so a
//! file taken from such a disc is unscrambled before it is read; a file taken from a GD-ROM or a
//! development load is already plain. Which form a file is in, the file does not say; the caller
//! knows it from the disc the file came from.
//!
//! The memory map folds the SH-4's segments onto one address per place: main RAM seen through
//! `P1` or `P2` is its `P1` address, the one `KallistiOS` links at; the boot ROM and the Holly
//! registers are their `P2` addresses, the uncached ones the SDK writes; the store queues and the
//! on-chip control registers of `P4` are their own. `P0` and `P3` go through the MMU when it is on,
//! which the image does not know, so no place answers them.

use crate::psx::Region;

/// The name the boot ROM looks for on the disc.
pub const FILE_NAME: &str = "1ST_READ.BIN";
/// The address the boot ROM loads `1ST_READ.BIN` at and enters it.
pub const BASE: u64 = 0x8c01_0000;

/// Main RAM, 16 MiB, at its `P1` address.
pub const RAM: u64 = 0x8c00_0000;
pub const RAM_SIZE: u64 = 0x100_0000;
/// The boot ROM, 2 MiB, at its `P2` address.
pub const BOOT_ROM: u64 = 0xa000_0000;
pub const BOOT_ROM_SIZE: u64 = 0x20_0000;
/// The registers of Holly, the system ASIC: the `PowerVR`, the G1 and G2 buses, Maple and DMA.
pub const HOLLY: u64 = 0xa05f_0000;
pub const HOLLY_SIZE: u64 = 0x1_0000;
/// The store queues' area of `P4`.
pub const STORE_QUEUES: u64 = 0xe000_0000;
pub const STORE_QUEUES_SIZE: u64 = 0x400_0000;
/// The SH-4's on-chip control registers (the cache, the timers, the serial port, the DMA
/// controller), the last 16 MiB of `P4`.
pub const CONTROL_REGISTERS: u64 = 0xff00_0000;
pub const CONTROL_REGISTERS_SIZE: u64 = 0x100_0000;

/// The largest boot program: the RAM from [`BASE`] to its end.
pub const MAX_SIZE: u64 = RAM + RAM_SIZE - BASE;

/// A boot program the loader mapped: the form its file was in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boot {
    /// The file was the scrambled form of a CD-ROM disc, unscrambled before it was mapped.
    pub scrambled: bool,
}

/// The place `va` reaches and its folded address, or `None` for an address no place answers: `P0`
/// and `P3`, which the MMU may translate, the parts of `P4` but the store queues and the control
/// registers, and the unconnected parts of the physical space.
pub fn fold(va: u64) -> Option<(Region, u64)> {
    let va = u64::from(u32::try_from(va).ok()?);
    let within = |address: u64, base: u64, size: u64| address >= base && address - base < size;
    if within(va, STORE_QUEUES, STORE_QUEUES_SIZE) {
        return Some((Region::StoreQueue, va));
    }
    if within(va, CONTROL_REGISTERS, CONTROL_REGISTERS_SIZE) {
        return Some((Region::Hardware, va));
    }
    if !matches!(va >> 29, 4 | 5) {
        return None;
    }
    let physical = va & 0x1fff_ffff;
    // A place is folded into the segment its constant names: the segment bits of the constant
    // over the physical address.
    let segment = |address: u64| address & !0x1fff_ffff;
    let place = |base: u64, size: u64| within(physical, base - segment(base), size).then(|| segment(base) | physical);
    if let Some(folded) = place(RAM, RAM_SIZE) {
        Some((Region::Ram, folded))
    } else if let Some(folded) = place(HOLLY, HOLLY_SIZE) {
        Some((Region::Hardware, folded))
    } else {
        place(BOOT_ROM, BOOT_ROM_SIZE).map(|folded| (Region::Bios, folded))
    }
}

/// The largest chunk the permutation runs over; smaller chunks follow down to one slice.
const MAX_CHUNK: usize = 2048 * 1024;
/// The unit the permutation moves.
const SLICE: usize = 32;

/// The boot ROM's generator: its seed is the file's size, and it carries over from chunk to chunk.
struct Generator(u32);

impl Generator {
    fn next(&mut self) -> u32 {
        self.0 = (self.0.wrapping_mul(2109).wrapping_add(9273)) & 0x7fff;
        (self.0 + 0xc000) & 0xffff
    }
}

/// The order the slices of every chunk are read in: `(offset of the chunk, slice index)` pairs in
/// file order, and the length of the tail shorter than a slice that neither form moves.
fn slice_order(len: usize) -> (Vec<usize>, usize) {
    #[allow(clippy::cast_possible_truncation, reason = "the seed is the low 16 bits of the size")]
    let mut generator = Generator((len & 0xffff) as u32);
    let mut order = Vec::with_capacity(len / SLICE);
    let mut offset = 0;
    let mut left = len;
    let mut chunk = MAX_CHUNK;
    while chunk >= SLICE {
        while left >= chunk {
            let slices = chunk / SLICE;
            let mut index: Vec<usize> = (0..slices).collect();
            for i in (0..slices).rev() {
                // The product stays below 2^32: both factors are below 2^16.
                #[allow(clippy::cast_possible_truncation, reason = "a chunk holds at most 65536 slices")]
                let x = ((u64::from(generator.next()) * i as u64) >> 16) as usize;
                index.swap(i, x);
                order.push(offset + SLICE * index[i]);
            }
            left -= chunk;
            offset += chunk;
        }
        chunk >>= 1;
    }
    (order, left)
}

/// The plain image of a scrambled `1ST_READ.BIN`: the slices put back where the boot ROM puts them.
#[must_use]
pub fn unscramble(scrambled: &[u8]) -> Vec<u8> {
    let (order, tail) = slice_order(scrambled.len());
    let mut plain = vec![0; scrambled.len()];
    for (k, &to) in order.iter().enumerate() {
        plain[to..to + SLICE].copy_from_slice(&scrambled[k * SLICE..(k + 1) * SLICE]);
    }
    let body = scrambled.len() - tail;
    plain[body..].copy_from_slice(&scrambled[body..]);
    plain
}

/// The scrambled form of a plain image, the inverse of [`unscramble`]: the form a disc of the
/// CD-ROM kind carries.
#[must_use]
pub fn scramble(plain: &[u8]) -> Vec<u8> {
    let (order, tail) = slice_order(plain.len());
    let mut scrambled = Vec::with_capacity(plain.len());
    for &from in &order {
        scrambled.extend_from_slice(&plain[from..from + SLICE]);
    }
    scrambled.extend_from_slice(&plain[plain.len() - tail..]);
    scrambled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::cast_possible_truncation, reason = "a byte pattern")]
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    #[test]
    fn unscramble_inverts_scramble_at_every_chunk_shape() {
        // A size of whole chunks, one with every chunk size down to a slice and a tail, and one
        // past the largest chunk.
        for len in [0, 31, 32, 64, 4096, 4096 + 2048 + 32 + 5, MAX_CHUNK + 3 * 1024 + 17] {
            let plain = pattern(len);
            let scrambled = scramble(&plain);
            assert_eq!(scrambled.len(), len);
            assert_eq!(unscramble(&scrambled), plain, "size {len}");
        }
    }

    #[test]
    fn every_slice_moves_exactly_once() {
        let len = 4096 + 512 + 64 + 3;
        let (order, tail) = slice_order(len);
        assert_eq!(tail, 3);
        let mut seen = order.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), order.len());
        assert_eq!(order.len(), len / SLICE);
        assert!(order.iter().all(|&o| o % SLICE == 0 && o + SLICE <= len - tail));
    }

    #[test]
    fn the_scrambled_form_differs_from_the_plain_one() {
        let plain = pattern(8192);
        assert_ne!(scramble(&plain), plain);
    }

    #[test]
    fn the_memory_map_folds_the_mirrors_of_ram_onto_p1() {
        for va in [0x8c01_0000, 0xac01_0000] {
            assert_eq!(fold(va), Some((Region::Ram, BASE)), "{va:#x}");
        }
        assert_eq!(fold(0x8cff_fffc), Some((Region::Ram, 0x8cff_fffc)));
        assert_eq!(fold(0x8d00_0000), None, "RAM is 16 MiB");
        assert_eq!(fold(0x0c01_0000), None, "P0 is the MMU's");
        assert_eq!(fold(0xcc01_0000), None, "P3 is the MMU's");
        assert_eq!(fold(0x805f_8000), Some((Region::Hardware, 0xa05f_8000)));
        assert_eq!(fold(0xa05f_8000), Some((Region::Hardware, 0xa05f_8000)));
        assert_eq!(fold(0x8000_0100), Some((Region::Bios, 0xa000_0100)));
        assert_eq!(fold(0xe000_0020), Some((Region::StoreQueue, 0xe000_0020)));
        assert_eq!(fold(0xffe8_000c), Some((Region::Hardware, 0xffe8_000c)));
        assert_eq!(fold(0xf000_0000), None);
    }

    #[test]
    fn the_tail_shorter_than_a_slice_stays_in_place() {
        let plain = pattern(20);
        assert_eq!(scramble(&plain), plain);
        assert_eq!(unscramble(&plain), plain);
    }
}
