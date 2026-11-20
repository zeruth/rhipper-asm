//! A frame buffer in the program's memory as the picture the console's video output makes of it.
//! The consoles whose programs draw into memory themselves, or whose graphics processor leaves
//! its picture there (the Nintendo 64's video interface, the PSP's display, the Dreamcast's
//! frame buffer), show a run of pixels of one format from an address, a row after a row; what
//! differs is the format, the bytes between two rows and the order of a pixel's bytes.

use crate::host::Frame;
use crate::{Memory, MemoryExt, Order};

/// A channel of a pixel: the bit its field starts at, counted from the pixel's lowest, and the
/// field's width.
pub type Field = (u32, u32);

/// How a pixel holds its colour: its size in bytes and where each channel lies in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub bytes: u32,
    pub red: Field,
    pub green: Field,
    pub blue: Field,
}

impl Format {
    /// Five bits a channel with red highest, the lowest bit apart: the Nintendo 64's 16 bits.
    pub const RGBA5551: Self = Self { bytes: 2, red: (11, 5), green: (6, 5), blue: (1, 5) };
    /// Eight bits a channel with red highest: the Nintendo 64's 32 bits.
    pub const RGBA8888: Self = Self { bytes: 4, red: (24, 8), green: (16, 8), blue: (8, 8) };
    /// Red lowest: the PSP's four formats.
    pub const BGR565: Self = Self { bytes: 2, red: (0, 5), green: (5, 6), blue: (11, 5) };
    pub const ABGR1555: Self = Self { bytes: 2, red: (0, 5), green: (5, 5), blue: (10, 5) };
    pub const ABGR4444: Self = Self { bytes: 2, red: (0, 4), green: (4, 4), blue: (8, 4) };
    pub const ABGR8888: Self = Self { bytes: 4, red: (0, 8), green: (8, 8), blue: (16, 8) };
    /// Blue lowest: the Dreamcast's four formats.
    pub const RGB555: Self = Self { bytes: 2, red: (10, 5), green: (5, 5), blue: (0, 5) };
    pub const RGB565: Self = Self { bytes: 2, red: (11, 5), green: (5, 6), blue: (0, 5) };
    pub const RGB888: Self = Self { bytes: 3, red: (16, 8), green: (8, 8), blue: (0, 8) };
    pub const XRGB8888: Self = Self { bytes: 4, red: (16, 8), green: (8, 8), blue: (0, 8) };

    /// The pixel `word` as `0x00RRGGBB`: each channel's bits repeated down to fill its eight.
    pub fn colour(&self, word: u32) -> u32 {
        let channel = |(shift, bits): Field| {
            let value = (word >> shift) & ((1 << bits) - 1);
            let wide = value << (8 - bits);
            wide | (wide >> bits)
        };
        (channel(self.red) << 16) | (channel(self.green) << 8) | channel(self.blue)
    }
}

/// The frame buffer the video output shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scan {
    /// The address of the first pixel, as the program's memory has it.
    pub address: u64,
    pub width: u32,
    pub height: u32,
    /// The bytes from the start of one row to the start of the next.
    pub stride: u32,
    pub format: Format,
    pub order: Order,
}

impl Scan {
    /// The picture: a pixel the memory does not hold is black.
    pub fn frame(&self, memory: &mut dyn Memory) -> Frame {
        let mut frame = Frame::new(self.width, self.height);
        self.rows(memory, &mut frame, 0, 1);
        frame
    }

    /// The buffer's rows into `frame` from its row `first` on, every `step`th: a field of an
    /// interlaced picture is every second row.
    pub fn rows(&self, memory: &mut dyn Memory, frame: &mut Frame, first: u32, step: u32) {
        let size = self.format.bytes as usize;
        let width = self.width.min(frame.width) as usize;
        for (row, y) in (first..frame.height).step_by(step.max(1) as usize).enumerate().take(self.height as usize) {
            let line = memory.bytes(self.address + row as u64 * u64::from(self.stride), width * size);
            let out = &mut frame.pixels[y as usize * frame.width as usize..][..width];
            for (pixel, bytes) in out.iter_mut().zip(line.chunks_exact(size)) {
                *pixel = self.format.colour(self.order.get(bytes) as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;

    #[test]
    fn a_channel_fills_its_eight_bits() {
        assert_eq!(Format::RGB565.colour(0xffff), 0x00ff_ffff);
        assert_eq!(Format::RGB565.colour(0xf800), 0x00ff_0000);
        assert_eq!(Format::BGR565.colour(0xf800), 0x0000_00ff);
        assert_eq!(Format::RGBA5551.colour(0x8421), 0x0084_8484, "a bit of each channel, and the alpha's");
        assert_eq!(Format::ABGR4444.colour(0x0a05), 0x0055_00aa);
        assert_eq!(Format::RGBA8888.colour(0x1122_33ff), 0x0011_2233);
        assert_eq!(Format::ABGR8888.colour(0xff33_2211), 0x0011_2233);
    }

    #[test]
    fn a_buffer_is_read_a_row_after_a_row_in_the_console_s_order() {
        let mut ram = Ram::new(0x1000, 64);
        // Two rows of two pixels, a pixel of padding after each row.
        for (at, word) in [(0u64, 0xf800u16), (2, 0x07e0), (6, 0x001f), (8, 0xffff)] {
            ram.write(0x1000 + at, &word.to_be_bytes());
        }
        let scan = Scan { address: 0x1000, width: 2, height: 2, stride: 6, format: Format::RGB565, order: Order::Big };
        assert_eq!(scan.frame(&mut ram).pixels, [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x00ff_ffff]);
        let little = Scan { order: Order::Little, ..scan };
        assert_eq!(little.frame(&mut ram).pixels[0], Format::RGB565.colour(0x00f8));
    }

    #[test]
    fn a_field_fills_every_second_row() {
        let mut ram = Ram::new(0, 16);
        ram.write(0, &[0xff, 0xff, 0x1f, 0x00]);
        let scan = Scan { address: 0, width: 1, height: 2, stride: 2, format: Format::RGB565, order: Order::Little };
        let mut frame = Frame::new(1, 4);
        scan.rows(&mut ram, &mut frame, 1, 2);
        assert_eq!(frame.pixels, [0, 0x00ff_ffff, 0, 0x0000_00ff]);
    }
}
