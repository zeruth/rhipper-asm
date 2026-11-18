//! Frames as PNG files: what a headless run writes and a test reads back. The writer stores the
//! pixels without compressing them (a frame is small and a reference image is read by a person
//! once); the reader inflates any PNG of 8-bit RGB or RGBA without interlace, so a reference a
//! person saved again from an image editor still reads.

use crate::host::Frame;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in bytes.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// `frame` as a PNG of 8-bit RGB.
pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut raw = Vec::with_capacity((frame.width as usize * 3 + 1) * frame.height as usize);
    for row in frame.pixels.chunks(frame.width.max(1) as usize) {
        raw.push(0);
        for pixel in row {
            raw.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(0xffff).peekable();
    if blocks.peek().is_none() {
        zlib.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(block) = blocks.next() {
        let len = block.len() as u16;
        zlib.push(u8::from(blocks.peek().is_none()));
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = SIGNATURE.to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&frame.width.to_be_bytes());
    header.extend_from_slice(&frame.height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The bits of a deflate stream, least significant first.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
    held: u32,
    count: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Option<u32> {
        while self.count < n {
            self.held |= u32::from(*self.bytes.get(self.at)?) << self.count;
            self.at += 1;
            self.count += 8;
        }
        let value = self.held & ((1u32 << n) - 1);
        self.held >>= n;
        self.count -= n;
        Some(value)
    }
}

/// A canonical Huffman code: the count of codes of each length, and the symbols in code order.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &len in lengths {
            counts[len as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for len in 1..16 {
            offsets[len] = offsets[len - 1] + counts[len - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len != 0 {
                symbols[offsets[len as usize] as usize] = symbol as u16;
                offsets[len as usize] += 1;
            }
        }
        Self { counts, symbols }
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - count < first {
                return self.symbols.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LENGTH_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385,
    24577,
];
const DISTANCE_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order the code-length code's own lengths are written in.
const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The bytes a zlib stream holds.
fn inflate(zlib: &[u8]) -> Option<Vec<u8>> {
    let mut bits = Bits { bytes: zlib.get(2..)?, at: 0, held: 0, count: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.held = 0;
                bits.count = 0;
                let header = bits.bytes.get(bits.at..bits.at + 4)?;
                let len = usize::from(u16::from_le_bytes([header[0], header[1]]));
                out.extend_from_slice(bits.bytes.get(bits.at + 4..bits.at + 4 + len)?);
                bits.at += 4 + len;
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 1 {
                    let mut lengths = [8u8; 288];
                    lengths[144..256].fill(9);
                    lengths[256..280].fill(7);
                    (Huffman::new(&lengths), Huffman::new(&[5; 30]))
                } else {
                    let (hlit, hdist, hclen) = (bits.take(5)? as usize + 257, bits.take(5)? as usize + 1, bits.take(4)? as usize + 4);
                    let mut code_lengths = [0u8; 19];
                    for &slot in &ORDER[..hclen] {
                        code_lengths[slot] = bits.take(3)? as u8;
                    }
                    let code = Huffman::new(&code_lengths);
                    let mut lengths = Vec::with_capacity(hlit + hdist);
                    while lengths.len() < hlit + hdist {
                        let (value, repeat) = match code.decode(&mut bits)? {
                            symbol @ 0..=15 => (symbol as u8, 1),
                            16 => (*lengths.last()?, 3 + bits.take(2)?),
                            17 => (0, 3 + bits.take(3)?),
                            _ => (0, 11 + bits.take(7)?),
                        };
                        lengths.extend(std::iter::repeat_n(value, repeat as usize));
                    }
                    (Huffman::new(lengths.get(..hlit)?), Huffman::new(lengths.get(hlit..hlit + hdist)?))
                };
                loop {
                    match literals.decode(&mut bits)? {
                        symbol @ 0..=255 => out.push(symbol as u8),
                        256 => break,
                        symbol => {
                            let slot = usize::from(symbol - 257);
                            let len = usize::from(*LENGTH_BASE.get(slot)?) + bits.take(u32::from(LENGTH_EXTRA[slot]))? as usize;
                            let slot = usize::from(distances.decode(&mut bits)?);
                            let distance = usize::from(*DISTANCE_BASE.get(slot)?) + bits.take(u32::from(DISTANCE_EXTRA[slot]))? as usize;
                            let from = out.len().checked_sub(distance)?;
                            for i in 0..len {
                                out.push(out[from + i]);
                            }
                        }
                    }
                }
            }
            _ => return None,
        }
        if last {
            return Some(out);
        }
    }
}

/// The frame a PNG of 8-bit RGB or RGBA holds; `None` for another form or a damaged file.
pub fn decode(png: &[u8]) -> Option<Frame> {
    if png.get(..8)? != SIGNATURE {
        return None;
    }
    let (mut at, mut header, mut data) = (8, None, Vec::new());
    while let Some(head) = png.get(at..at + 8) {
        let len = u32::from_be_bytes(head[..4].try_into().ok()?) as usize;
        let body = png.get(at + 8..at + 8 + len)?;
        match &head[4..] {
            b"IHDR" => header = Some(body),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let header = header?;
    let width = u32::from_be_bytes(header.get(..4)?.try_into().ok()?);
    let height = u32::from_be_bytes(header.get(4..8)?.try_into().ok()?);
    let channels = match header.get(8..13)? {
        [8, 2, 0, 0, 0] => 3,
        [8, 6, 0, 0, 0] => 4,
        _ => return None,
    };
    let raw = inflate(&data)?;
    let stride = width as usize * channels;
    let mut frame = Frame::new(width, height);
    let mut previous = vec![0u8; stride];
    for (y, line) in raw.chunks(stride + 1).take(height as usize).enumerate() {
        let (filter, bytes) = line.split_first()?;
        let mut row = bytes.to_vec();
        row.resize(stride, 0);
        for i in 0..stride {
            let left = if i >= channels { i32::from(row[i - channels]) } else { 0 };
            let up = i32::from(previous[i]);
            let corner = if i >= channels { i32::from(previous[i - channels]) } else { 0 };
            let predicted = match filter {
                1 => left,
                2 => up,
                3 => (left + up) / 2,
                4 => {
                    let p = left + up - corner;
                    let (a, b, c) = ((p - left).abs(), (p - up).abs(), (p - corner).abs());
                    if a <= b && a <= c {
                        left
                    } else if b <= c {
                        up
                    } else {
                        corner
                    }
                }
                _ => 0,
            };
            row[i] = row[i].wrapping_add(predicted as u8);
        }
        for x in 0..width as usize {
            let p = &row[x * channels..];
            frame.pixels[y * width as usize + x] = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
        }
        previous = row;
    }
    Some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_written_reads_back() {
        let mut frame = Frame::new(5, 3);
        for (i, pixel) in frame.pixels.iter_mut().enumerate() {
            *pixel = (i as u32 * 0x0011_2233) & 0x00ff_ffff;
        }
        let png = encode(&frame);
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(decode(&png), Some(frame));
    }

    /// A 2x2 RGB image another encoder compressed with a fixed Huffman block and a `Sub` filter
    /// row: red, green on the first row, blue, white on the second.
    #[test]
    fn a_compressed_png_of_another_encoder_reads() {
        let raw = [1u8, 255, 0, 0, 1, 255, 0, 0, 0, 0, 255, 255, 255, 255];
        // Stored by hand as a fixed-Huffman stream of literals only.
        let mut bits: Vec<bool> = vec![true, true, false];
        let push = |bits: &mut Vec<bool>, code: u32, len: u32| {
            for i in (0..len).rev() {
                bits.push(code >> i & 1 == 1);
            }
        };
        for &byte in &raw {
            if byte < 144 {
                push(&mut bits, 0x30 + u32::from(byte), 8);
            } else {
                push(&mut bits, 0x190 + u32::from(byte) - 144, 9);
            }
        }
        push(&mut bits, 0, 7);
        let mut zlib = vec![0x78, 0x9c];
        for group in bits.chunks(8) {
            zlib.push(group.iter().enumerate().fold(0u8, |byte, (i, bit)| byte | u8::from(*bit) << i));
        }
        zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
        let mut png = SIGNATURE.to_vec();
        chunk(&mut png, b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]);
        chunk(&mut png, b"IDAT", &zlib);
        chunk(&mut png, b"IEND", &[]);
        let frame = decode(&png).expect("the image reads");
        assert_eq!(frame.pixels, [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x00ff_ffff]);
    }
}
