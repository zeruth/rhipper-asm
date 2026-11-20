//! The PlayStation's macroblock decoder: the pictures of a video come to it as run-length coded
//! coefficients of 8 by 8 blocks, and leave it as pixels. A command word says what follows (the
//! macroblocks to decode, the two quantization tables, the table of the inverse transform), the
//! words after it are its data, and what it decodes is read back, a word at a time or by DMA.
//!
//! A colour macroblock is six blocks, the two halved colour differences and four of brightness,
//! and decodes to 16 by 16 pixels of 15 or 24 bits; a monochrome one is one block of 8 by 8
//! pixels of 4 or 8 bits. The decoder here takes a command's data whole before it decodes it.

use std::collections::VecDeque;

/// The word that ends a block, and pads the data after the last one.
const END: u16 = 0xfe00;

/// Where the coefficients of a block lie in the order they come in.
const ZIGZAG: [u8; 64] = [
    0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11, 18, 24, 31, 40, 44, 53, 10, 19, 23, 32,
    39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34, 37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
];

/// The table of the inverse transform every program sends: the cosines, in 1.15 fixed point.
const SCALE: [u16; 64] = [
    0x5a82, 0x5a82, 0x5a82, 0x5a82, 0x5a82, 0x5a82, 0x5a82, 0x5a82, 0x7d8a, 0x6a6d, 0x471c, 0x18f8, 0xe707, 0xb8e3, 0x9592, 0x8275, 0x7641,
    0x30fb, 0xcf04, 0x89be, 0x89be, 0xcf04, 0x30fb, 0x7641, 0x6a6d, 0xe707, 0x8275, 0xb8e3, 0x471c, 0x7d8a, 0x18f8, 0x9592, 0x5a82, 0xa57d,
    0xa57d, 0x5a82, 0x5a82, 0xa57d, 0xa57d, 0x5a82, 0x471c, 0x8275, 0x18f8, 0x6a6d, 0x9592, 0xe707, 0x7d8a, 0xb8e3, 0x30fb, 0x89be, 0x7641,
    0xcf04, 0xcf04, 0x7641, 0x89be, 0x30fb, 0x18f8, 0xb8e3, 0x6a6d, 0x8275, 0x7d8a, 0x9592, 0x471c, 0xe707,
];

/// What the data after a command word is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    None,
    Decode,
    /// The brightness's quantization table, and the colour's after it when set.
    Quantization(bool),
    Scale,
}

pub struct Mdec {
    command: Command,
    /// The command's bits the status register shows: the depth, the sign, the fifteenth bit.
    format: u32,
    /// The words of the command's data still to come, and those that came.
    remaining: u32,
    data: Vec<u8>,
    /// The quantization tables of the brightness and of the colour, and the transform's.
    luma: [u8; 64],
    chroma: [u8; 64],
    scale: [i16; 64],
    /// What was decoded and not yet read.
    out: VecDeque<u8>,
    /// The control register's two request enables.
    control: u32,
    /// Where a coefficient of a block goes, by the order it comes in.
    places: [u8; 64],
}

impl Default for Mdec {
    fn default() -> Self {
        let mut places = [0; 64];
        for (place, order) in ZIGZAG.iter().enumerate() {
            places[usize::from(*order)] = place as u8;
        }
        Self {
            command: Command::None,
            format: 0,
            remaining: 0,
            data: Vec::new(),
            luma: [0; 64],
            chroma: [0; 64],
            scale: SCALE.map(u16::cast_signed),
            out: VecDeque::new(),
            control: 0,
            places,
        }
    }
}

fn signed10(word: u16) -> i32 {
    (i32::from(word) << 22) >> 22
}

impl Mdec {
    /// The status register: nothing to read, a command at work, the requests the control
    /// register enabled, the format, and the count of the data words still to come, less one.
    pub fn status(&self) -> u32 {
        let busy = self.command != Command::None;
        let mut status = u32::from(self.out.is_empty()) << 31 | u32::from(busy) << 29 | self.format << 23;
        status |= u32::from(self.control & 2 != 0 && self.remaining > 0) << 28;
        status |= u32::from(self.control & 1 != 0 && !self.out.is_empty()) << 27;
        // The block the decoder is at: the first of brightness between two macroblocks.
        status |= 4 << 16;
        status | (self.remaining.wrapping_sub(1) & 0xffff)
    }

    /// A write of the control register: a reset, or the two request enables.
    pub fn control(&mut self, value: u32) {
        if value & (1 << 31) != 0 {
            (self.command, self.format, self.remaining) = (Command::None, 0, 0);
            self.data.clear();
            self.out.clear();
        }
        self.control = (value >> 29) & 3;
    }

    /// A word to the decoder: a command, or the next word of the data of the one at work.
    pub fn write(&mut self, word: u32) {
        if self.command == Command::None {
            self.format = (word >> 25) & 0xf;
            (self.command, self.remaining) = match word >> 29 {
                1 => (Command::Decode, word & 0xffff),
                2 => (Command::Quantization(word & 1 != 0), if word & 1 != 0 { 32 } else { 16 }),
                3 => (Command::Scale, 32),
                _ => (Command::None, 0),
            };
            self.data.clear();
            if self.remaining == 0 {
                self.command = Command::None;
            }
            return;
        }
        self.data.extend_from_slice(&word.to_le_bytes());
        self.remaining -= 1;
        if self.remaining > 0 {
            return;
        }
        let data = std::mem::take(&mut self.data);
        match std::mem::replace(&mut self.command, Command::None) {
            Command::Decode => self.decode(&data),
            Command::Quantization(colour) => {
                self.luma.copy_from_slice(&data[..64]);
                if colour {
                    self.chroma.copy_from_slice(&data[64..128]);
                }
            }
            Command::Scale => {
                for (entry, bytes) in self.scale.iter_mut().zip(data.chunks_exact(2)) {
                    *entry = i16::from_le_bytes([bytes[0], bytes[1]]);
                }
            }
            Command::None => {}
        }
    }

    /// A word of what was decoded, zeros once it is all read.
    pub fn read(&mut self) -> u32 {
        let mut bytes = [0; 4];
        for byte in &mut bytes {
            *byte = self.out.pop_front().unwrap_or(0);
        }
        u32::from_le_bytes(bytes)
    }

    /// One block off `words`: its coefficients by the quantization `table` and the block's own
    /// scale, in their places, through the inverse transform. `None` at the end of the data.
    fn block(&self, words: &mut impl Iterator<Item = u16>, table: &[u8; 64]) -> Option<[i16; 64]> {
        let mut first = words.next()?;
        while first == END {
            first = words.next()?;
        }
        let scale = i32::from(first >> 10);
        let mut coefficients = [0i32; 64];
        let mut k = 0;
        let mut value = signed10(first) * i32::from(table[0]);
        let mut word = first;
        loop {
            // A scale of zero is the block as it is: no table, no reordering.
            if scale == 0 {
                value = signed10(word) * 2;
            }
            let place = if scale == 0 { k } else { usize::from(self.places[k]) };
            coefficients[place] = value.clamp(-0x400, 0x3ff);
            let Some(next) = words.next() else { break };
            word = next;
            k += usize::from(word >> 10) + 1;
            if k >= 64 {
                break;
            }
            value = (signed10(word) * i32::from(table[k]) * scale + 4) / 8;
        }
        Some(self.transform(&coefficients))
    }

    /// The inverse transform: the table over the columns, then over the rows, and the sum
    /// rounded to the nine bits a sample has, held to a signed byte.
    fn transform(&self, block: &[i32; 64]) -> [i16; 64] {
        let mut columns = [0i64; 64];
        for x in 0..8 {
            for y in 0..8 {
                columns[x + y * 8] = (0..8).map(|u| i64::from(block[u * 8 + x]) * i64::from(self.scale[u * 8 + y])).sum();
            }
        }
        let mut out = [0i16; 64];
        for x in 0..8 {
            for y in 0..8 {
                let sum: i64 = (0..8).map(|u| columns[u + y * 8] * i64::from(self.scale[u * 8 + x])).sum();
                let rounded = ((sum >> 32) + ((sum >> 31) & 1)) as i32;
                out[x + y * 8] = ((rounded << 23) >> 23).clamp(-128, 127) as i16;
            }
        }
        out
    }

    /// Decodes the macroblocks of a command's data into what is read back.
    fn decode(&mut self, data: &[u8]) {
        let mut words = data.chunks_exact(2).map(|w| u16::from_le_bytes([w[0], w[1]]));
        let (depth, signed, mask) = ((self.format >> 2) & 3, self.format & 2 != 0, self.format & 1 != 0);
        let flip = if signed { 0 } else { 0x80 };
        if depth < 2 {
            // Monochrome: a block of brightness, a pixel a nibble or a byte.
            while let Some(block) = self.block(&mut words, &self.luma) {
                let pixels = block.map(|sample| (sample as u8) ^ flip);
                if depth == 1 {
                    self.out.extend(pixels);
                } else {
                    self.out.extend(pixels.chunks_exact(2).map(|pair| (pair[0] >> 4) | (pair[1] & 0xf0)));
                }
            }
            return;
        }
        loop {
            let Some(cr) = self.block(&mut words, &self.chroma) else { break };
            let Some(cb) = self.block(&mut words, &self.chroma) else { break };
            let mut pixels = [[0u8; 3]; 256];
            let mut whole = true;
            for quarter in 0..4 {
                let Some(y) = self.block(&mut words, &self.luma) else {
                    whole = false;
                    break;
                };
                let (left, top) = ((quarter & 1) * 8, (quarter >> 1) * 8);
                for row in 0..8 {
                    for column in 0..8 {
                        let (x, line) = (left + column, top + row);
                        let (r, b) = (i32::from(cr[x / 2 + line / 2 * 8]), i32::from(cb[x / 2 + line / 2 * 8]));
                        let brightness = i32::from(y[column + row * 8]);
                        let channel = |difference: i32| ((brightness + difference).clamp(-128, 127) as u8) ^ flip;
                        pixels[x + line * 16] = [channel((359 * r) >> 8), channel((-88 * b - 183 * r) >> 8), channel((454 * b) >> 8)];
                    }
                }
            }
            if !whole {
                break;
            }
            for [r, g, b] in pixels {
                if depth == 2 {
                    self.out.extend([r, g, b]);
                } else {
                    let pixel = u16::from(r >> 3) | u16::from(g >> 3) << 5 | u16::from(b >> 3) << 10 | u16::from(mask) << 15;
                    self.out.extend(pixel.to_le_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The status register as a reset leaves it: nothing to give, no command.
    const IDLE: u32 = 0x8004_0000;

    /// A decoder with flat quantization tables and the transform's own table sent, as a
    /// program sets one up.
    fn decoder() -> Mdec {
        let mut mdec = Mdec::default();
        mdec.control(1 << 31);
        mdec.write(0x4000_0001);
        for _ in 0..32 {
            mdec.write(0x0101_0101);
        }
        mdec.write(0x6000_0000);
        for pair in SCALE.chunks_exact(2) {
            mdec.write(u32::from(pair[0]) | u32::from(pair[1]) << 16);
        }
        mdec
    }

    #[test]
    fn the_status_counts_the_data_words_down_and_says_what_is_to_read() {
        let mut mdec = Mdec::default();
        mdec.control(1 << 31);
        assert_eq!(mdec.status(), IDLE | 0xffff);
        mdec.write(0x4000_0001);
        assert_eq!(mdec.status() & 0x2000_ffff, 0x2000_001f, "busy, thirty-two words to come");
        for _ in 0..32 {
            mdec.write(0x0202_0202);
        }
        assert_eq!(mdec.status(), IDLE | 0xffff);
        assert_eq!((mdec.luma[63], mdec.chroma[0]), (2, 2));
    }

    #[test]
    fn a_block_of_its_first_coefficient_alone_is_flat() {
        let mut mdec = decoder();
        // Monochrome at eight bits, signed: a scale of one, a first coefficient of 64, the end.
        mdec.write(0x2c00_0001);
        mdec.write(u32::from(1u16 << 10 | 64) | u32::from(END) << 16);
        assert_eq!(mdec.status() >> 31, 0, "something to read");
        let pixels: Vec<u8> = (0..16).flat_map(|_| mdec.read().to_le_bytes()).collect();
        // The transform takes an eighth of the first coefficient to every sample.
        assert_eq!(pixels, [8; 64]);
        assert_eq!(mdec.status() >> 31, 1);
    }

    #[test]
    fn a_colour_macroblock_is_sixteen_by_sixteen_pixels_of_its_depth() {
        let mut mdec = decoder();
        // Fifteen bits with the mask bit, unsigned: six blocks, each its first coefficient
        // alone; no colour difference, a brightness of 64 over the middle.
        let blocks: Vec<u16> = [0, 0, 512, 512, 512, 512].iter().flat_map(|dc| [1 << 10 | (*dc & 0x3ff), END]).collect();
        mdec.write(0x3a00_0000 | blocks.len() as u32 / 2);
        for pair in blocks.chunks_exact(2) {
            mdec.write(u32::from(pair[0]) | u32::from(pair[1]) << 16);
        }
        let pixels: Vec<u16> = (0..128).flat_map(|_| mdec.read().to_le_bytes()).collect::<Vec<u8>>().chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
        // 512 as ten signed bits is -512: an eighth of it, -64, over the middle of 128.
        let grey = (128u16 - 64) >> 3;
        assert_eq!(pixels, [grey | grey << 5 | grey << 10 | 0x8000; 256]);
        // The same at twenty-four bits is three bytes a pixel.
        mdec.write(0x3000_0000 | blocks.len() as u32 / 2);
        for pair in blocks.chunks_exact(2) {
            mdec.write(u32::from(pair[0]) | u32::from(pair[1]) << 16);
        }
        assert_eq!(mdec.out.len(), 768);
        assert_eq!(mdec.read(), 0x4040_4040);
    }
}
