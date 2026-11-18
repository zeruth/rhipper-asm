//! The ADPCM of both PlayStations' sound processors: sound in blocks of sixteen bytes, a header
//! of the shift and the filter, a byte of loop flags, and twenty-eight samples of four bits each,
//! each a correction to what the filter predicts from the two samples before it.

/// The filters' weights for the last sample and the one before, in 64ths.
const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// The block is the last of its sound: the voice stops, or goes to the loop's start.
pub const LOOP_END: u8 = 1;
/// With [`LOOP_END`]: the voice goes on from the loop's start instead of stopping.
pub const LOOP_REPEAT: u8 = 2;
/// The block is where the loop starts.
pub const LOOP_START: u8 = 4;

/// The two samples a block's first prediction reads: a voice keeps them from block to block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct History(pub i32, pub i32);

/// The twenty-eight samples of `block`, and the block's loop flags.
pub fn decode(block: &[u8; 16], history: &mut History) -> ([i16; 28], u8) {
    // A shift past twelve leaves nothing of a nibble: the hardware plays it as nine.
    let shift = match block[0] & 0xf {
        s @ 0..=12 => u32::from(s),
        _ => 9,
    };
    let (k0, k1) = FILTERS[usize::from((block[0] >> 4) & 7).min(4)];
    let mut out = [0i16; 28];
    for (i, sample) in out.iter_mut().enumerate() {
        let nibble = (block[2 + i / 2] >> (4 * (i & 1))) & 0xf;
        let correction = (i32::from(nibble) << 28 >> 28 << 12) >> shift;
        let value = (correction + (history.0 * k0 + history.1 * k1 + 32) / 64).clamp(-0x8000, 0x7fff);
        *history = History(value, history.0);
        *sample = value as i16;
    }
    (out, block[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_without_a_filter_is_its_nibbles_shifted() {
        let mut block = [0u8; 16];
        block[0] = 0x08;
        block[1] = LOOP_END;
        block[2] = 0xf1;
        let mut history = History::default();
        let (samples, flags) = decode(&block, &mut history);
        assert_eq!((samples[0], samples[1], samples[2]), (16, -16, 0));
        assert_eq!(flags, LOOP_END);
        assert_eq!(history, History(0, 0));
    }

    #[test]
    fn the_first_filter_carries_the_last_sample_on() {
        let mut block = [0u8; 16];
        block[0] = 0x1c;
        block[2] = 0x01;
        let mut history = History(640, 0);
        let (samples, _) = decode(&block, &mut history);
        // 1 + 640 * 60 / 64, then that again by 60 / 64 with the rounding.
        assert_eq!((samples[0], samples[1]), (601, 563));
    }
}
