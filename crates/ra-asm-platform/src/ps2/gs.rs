//! The PlayStation 2's graphics synthesizer: four megabytes of local memory that hold the frame
//! buffers, the depth buffers and the textures, the general registers a program reaches through
//! the GIF's packets, and the privileged ones at its own addresses, which say what the television
//! shows. A vertex written to `XYZ2` kicks the drawing of the primitive `PRIM` names once enough
//! of them are there; each pixel goes through the scissor, the alpha test, the depth test and the
//! blend to the frame buffer. It is rendered here, in software, as the packets arrive.
//!
//! The local memory is laid out a row after a row in each buffer's own width and format. The
//! hardware interleaves it in pages, blocks and columns, which shows only where a program reads a
//! buffer in another format than it wrote it in; such a read gives other pixels here.

use crate::host::Frame;
use crate::raster::{self, Rect};

const MEMORY: usize = 4 << 20;

/// The pixel formats that are not 32 bits of colour, which is zero: 24 bits, 16 with one bit of
/// alpha, 8 and 4 of index. The depth formats are these sizes with a higher bit set.
const PSMCT24: u32 = 0x01;
const PSMCT16: u32 = 0x02;
const PSMCT16S: u32 = 0x0a;
const PSMT8: u32 = 0x13;
const PSMT4: u32 = 0x14;

/// The bits a pixel of `format` takes in the local memory.
fn bits_of(format: u32) -> usize {
    match format & 0x3f {
        PSMCT16 | PSMCT16S | 0x32 | 0x3a => 16,
        PSMT8 => 8,
        PSMT4 => 4,
        _ => 32,
    }
}

/// The general registers by their addresses in a GIF packet.
mod reg {
    pub(super) const PRIM: u8 = 0x00;
    pub(super) const RGBAQ: u8 = 0x01;
    pub(super) const ST: u8 = 0x02;
    pub(super) const UV: u8 = 0x03;
    pub(super) const XYZF2: u8 = 0x04;
    pub(super) const XYZ2: u8 = 0x05;
    pub(super) const TEX0: u8 = 0x06;
    pub(super) const FOG: u8 = 0x0a;
    pub(super) const XYZF3: u8 = 0x0c;
    pub(super) const XYZ3: u8 = 0x0d;
    pub(super) const A_D: u8 = 0x0e;
    pub(super) const XYOFFSET: u8 = 0x18;
    pub(super) const PRMODECONT: u8 = 0x1a;
    pub(super) const PRMODE: u8 = 0x1b;
    pub(super) const TEXA: u8 = 0x3b;
    pub(super) const SCISSOR: u8 = 0x40;
    pub(super) const ALPHA: u8 = 0x42;
    pub(super) const COLCLAMP: u8 = 0x46;
    pub(super) const TEST: u8 = 0x47;
    pub(super) const FBA: u8 = 0x4a;
    pub(super) const FRAME: u8 = 0x4c;
    pub(super) const ZBUF: u8 = 0x4e;
    pub(super) const BITBLTBUF: u8 = 0x50;
    pub(super) const TRXPOS: u8 = 0x51;
    pub(super) const TRXREG: u8 = 0x52;
    pub(super) const TRXDIR: u8 = 0x53;
    pub(super) const HWREG: u8 = 0x54;
    pub(super) const SIGNAL: u8 = 0x60;
    pub(super) const FINISH: u8 = 0x61;
}

/// The privileged registers by their offsets from `0x12000000`.
pub const PMODE: u64 = 0x00;
pub const SMODE2: u64 = 0x20;
pub const DISPFB1: u64 = 0x70;
pub const DISPLAY1: u64 = 0x80;
pub const DISPFB2: u64 = 0x90;
pub const DISPLAY2: u64 = 0xa0;
pub const BGCOLOR: u64 = 0xe0;
pub const CSR: u64 = 0x1000;
pub const IMR: u64 = 0x1010;

/// The status register's events: a `SIGNAL` and a `FINISH` written, the vertical blank.
pub const CSR_SIGNAL: u64 = 1 << 0;
pub const CSR_FINISH: u64 = 1 << 1;
pub const CSR_VSINT: u64 = 1 << 3;

/// A vertex as the kick took it: its place in sixteenths of a pixel, its depth, its colour and
/// its texture coordinates (in texels, or as `S`, `T` and `Q`).
#[derive(Debug, Clone, Copy, Default)]
struct Vertex {
    x: i32,
    y: i32,
    z: u32,
    rgba: [i32; 4],
    uv: [f32; 2],
    stq: [f32; 3],
}

/// The two drawing environments a primitive picks between.
#[derive(Debug, Clone, Copy, Default)]
struct Context {
    frame: u64,
    zbuf: u64,
    tex0: u64,
    offset: u64,
    scissor: u64,
    alpha: u64,
    test: u64,
    fba: u64,
}

/// A host-to-local transfer in flight: where its next pixel goes.
#[derive(Debug, Clone, Copy)]
struct Transfer {
    base: usize,
    width: usize,
    format: u32,
    x: usize,
    y: usize,
    columns: usize,
    rows: usize,
    at: usize,
}

pub struct Gs {
    memory: Vec<u8>,
    /// The privileged registers, by offset.
    privileged: std::collections::BTreeMap<u64, u64>,
    pub csr: u64,
    pub imr: u64,
    contexts: [Context; 2],
    prim: u64,
    prmode: u64,
    /// `PRIM`'s attributes hold (1) or `PRMODE`'s do (0).
    prmodecont: u64,
    rgbaq: u64,
    st: u64,
    uv: u64,
    texa: u64,
    colclamp: u64,
    bitbltbuf: u64,
    trxpos: u64,
    trxreg: u64,
    /// The vertices the primitive being assembled has so far, and how many a strip or a fan has
    /// seen in all.
    queue: Vec<Vertex>,
    seen: usize,
    transfer: Option<Transfer>,
    /// The GIF tag being worked through: its loops left, its registers, and where in them.
    tag: Option<Tag>,
}

#[derive(Debug, Clone, Copy)]
struct Tag {
    loops: u32,
    format: u32,
    registers: u64,
    count: u32,
    at: u32,
    end: bool,
}

impl Default for Gs {
    fn default() -> Self {
        Self::new()
    }
}

fn field(value: u64, low: u32, bits: u32) -> u64 {
    (value >> low) & ((1 << bits) - 1)
}

impl Gs {
    pub fn new() -> Self {
        Self {
            memory: vec![0; MEMORY],
            privileged: std::collections::BTreeMap::new(),
            // The FIFO empty, the revision and the identifier of the chip.
            csr: 0x551b_4000,
            imr: 0x7f00,
            contexts: [Context::default(); 2],
            prim: 0,
            prmode: 0,
            prmodecont: 1,
            rgbaq: 0,
            st: 0,
            uv: 0,
            texa: 0,
            colclamp: 1,
            bitbltbuf: 0,
            trxpos: 0,
            trxreg: 0,
            queue: Vec::new(),
            seen: 0,
            transfer: None,
            tag: None,
        }
    }

    // region The privileged registers

    pub fn read(&self, offset: u64) -> u64 {
        match offset & !0xf {
            CSR => self.csr,
            IMR => self.imr,
            other => self.privileged.get(&other).copied().unwrap_or(0),
        }
    }

    pub fn write(&mut self, offset: u64, value: u64) {
        match offset & !0xf {
            CSR => {
                // A one acknowledges an event; the reset bit clears the chip's state.
                self.csr &= !(value & 0x1f);
                if value & 0x200 != 0 {
                    let memory = std::mem::take(&mut self.memory);
                    *self = Self { memory, ..Self::new() };
                }
            }
            IMR => self.imr = value & 0x7f00,
            other => {
                self.privileged.insert(other, value);
            }
        }
    }

    /// The vertical blank: the status register's event, and the field that alternates.
    pub fn vblank(&mut self) {
        self.csr |= CSR_VSINT;
        self.csr ^= 1 << 13;
    }

    /// Whether an event the interrupt mask lets through is pending.
    pub fn interrupt(&self) -> bool {
        (self.csr & 0x1f) & !(self.imr >> 8) != 0
    }

    /// The picture the television shows: the read circuit `PMODE` turns on, from its frame
    /// buffer, at the size its display register gives; a black one before a program set it up.
    pub fn frame(&self) -> Frame {
        let pmode = self.read(PMODE);
        let (dispfb, display) = if pmode & 2 != 0 || pmode & 1 == 0 {
            (self.read(DISPFB2), self.read(DISPLAY2))
        } else {
            (self.read(DISPFB1), self.read(DISPLAY1))
        };
        let width = (field(display, 32, 12) + 1) / (field(display, 23, 4) + 1);
        let mut height = (field(display, 44, 11) + 1) / (field(display, 27, 2) + 1);
        // An interlaced display that shows a field at a time holds both fields' lines.
        let smode2 = self.read(SMODE2);
        if smode2 & 1 != 0 && smode2 & 2 == 0 && height > 320 {
            height /= 2;
        }
        if pmode & 3 == 0 || display == 0 || width > 1024 || height > 1024 {
            return Frame::new(640, 448);
        }
        let (base, stride, format) = (field(dispfb, 0, 9) as usize * 8192, field(dispfb, 9, 6) as usize * 64, field(dispfb, 15, 5) as u32);
        let (x0, y0) = (field(dispfb, 32, 11) as usize, field(dispfb, 43, 11) as usize);
        let mut frame = Frame::new(width as u32, height as u32);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let [r, g, b, _] = self.pixel(base, stride, format, x0 + x, y0 + y);
                frame.pixels[y * width as usize + x] = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            }
        }
        frame
    }

    // endregion

    // region The local memory

    /// The pixel at (`x`, `y`) of the buffer at the byte `base`, `stride` pixels wide, as red,
    /// green, blue and alpha.
    fn pixel(&self, base: usize, stride: usize, format: u32, x: usize, y: usize) -> [u8; 4] {
        let at = y * stride + x;
        let byte = |i: usize| self.memory[(base + i) & (MEMORY - 1)];
        match bits_of(format) {
            16 => {
                let word = u16::from_le_bytes([byte(at * 2), byte(at * 2 + 1)]);
                let wide = |five: u16| ((five << 3) | (five >> 2)) as u8;
                [wide(word & 0x1f), wide((word >> 5) & 0x1f), wide((word >> 10) & 0x1f), if word & 0x8000 != 0 { 0x80 } else { 0 }]
            }
            8 => [byte(at), 0, 0, 0],
            4 => [(byte(at / 2) >> (4 * (at & 1))) & 0xf, 0, 0, 0],
            _ => [byte(at * 4), byte(at * 4 + 1), byte(at * 4 + 2), if format & 0x3f == PSMCT24 { 0x80 } else { byte(at * 4 + 3) }],
        }
    }

    /// Stores `rgba` at (`x`, `y`) of a buffer, under `mask`, whose set bits keep what is there.
    fn set_pixel(&mut self, base: usize, stride: usize, format: u32, x: usize, y: usize, rgba: [u8; 4], mask: u32) {
        let at = y * stride + x;
        if bits_of(format) == 16 {
            let word =
                u16::from(rgba[0] >> 3) | u16::from(rgba[1] >> 3) << 5 | u16::from(rgba[2] >> 3) << 10 | u16::from(rgba[3] >> 7) << 15;
            for (i, byte) in word.to_le_bytes().into_iter().enumerate() {
                self.memory[(base + at * 2 + i) & (MEMORY - 1)] = byte;
            }
            return;
        }
        let keep = if format & 0x3f == PSMCT24 { mask | 0xff00_0000 } else { mask };
        for (i, byte) in rgba.into_iter().enumerate() {
            let held = &mut self.memory[(base + at * 4 + i) & (MEMORY - 1)];
            let kept = (keep >> (8 * i)) as u8;
            *held = (*held & kept) | (byte & !kept);
        }
    }

    /// A word of image data to the transfer in flight: as many pixels as the format puts in it.
    fn image(&mut self, bytes: &[u8]) {
        let Some(mut transfer) = self.transfer else { return };
        let bits = if transfer.format & 0x3f == PSMCT24 { 24 } else { bits_of(transfer.format) };
        let total = transfer.columns * transfer.rows;
        let mut bit = 0;
        while bit + bits <= bytes.len() * 8 && transfer.at < total {
            let (x, y) = (transfer.x + transfer.at % transfer.columns, transfer.y + transfer.at / transfer.columns);
            let at = y * transfer.width + x;
            match bits {
                4 => {
                    let nibble = (bytes[bit / 8] >> (bit % 8)) & 0xf;
                    let held = &mut self.memory[(transfer.base + at / 2) & (MEMORY - 1)];
                    *held = (*held & !(0xf << (4 * (at & 1)))) | nibble << (4 * (at & 1));
                }
                24 => {
                    for i in 0..3 {
                        self.memory[(transfer.base + at * 4 + i) & (MEMORY - 1)] = bytes[bit / 8 + i];
                    }
                }
                _ => {
                    for i in 0..bits / 8 {
                        self.memory[(transfer.base + at * (bits / 8) + i) & (MEMORY - 1)] = bytes[bit / 8 + i];
                    }
                }
            }
            bit += bits;
            transfer.at += 1;
        }
        self.transfer = (transfer.at < total).then_some(transfer);
    }

    // endregion

    // region The GIF

    /// A quadword of a GIF path: a tag, or the data the tag in flight says it is.
    pub fn gif(&mut self, low: u64, high: u64) {
        let Some(mut tag) = self.tag.filter(|t| t.loops > 0) else {
            let loops = field(low, 0, 15) as u32;
            let count = field(low, 60, 4) as u32;
            let tag = Tag {
                loops,
                format: field(low, 58, 2) as u32,
                registers: high,
                count: if count == 0 { 16 } else { count },
                at: 0,
                end: low & 0x8000 != 0,
            };
            // The tag's own PRIM, when it carries one and there is data for it.
            if low & (1 << 46) != 0 && tag.format == 0 && loops > 0 {
                self.register(reg::PRIM, field(low, 47, 11));
            }
            self.tag = (loops > 0).then_some(tag);
            return;
        };
        let register = |tag: &Tag| ((tag.registers >> (4 * tag.at)) & 0xf) as u8;
        let step = |tag: &mut Tag| {
            tag.at += 1;
            if tag.at == tag.count {
                tag.at = 0;
                tag.loops -= 1;
            }
        };
        match tag.format {
            0 => {
                self.packed(register(&tag), low, high);
                step(&mut tag);
            }
            1 => {
                for half in [low, high] {
                    if tag.loops > 0 {
                        self.register(register(&tag), half);
                        step(&mut tag);
                    }
                }
            }
            _ => {
                self.image(&[low.to_le_bytes(), high.to_le_bytes()].concat());
                tag.loops -= 1;
            }
        }
        self.tag = Some(tag);
    }

    /// Whether the path's packet ended: its last tag had the end bit and its data is through.
    pub fn idle(&self) -> bool {
        self.tag.is_none_or(|t| t.loops == 0 && t.end)
    }

    /// A register in the packed form of a GIF packet: each has its own layout in the quadword.
    fn packed(&mut self, register: u8, low: u64, high: u64) {
        match register {
            reg::RGBAQ => {
                let q = self.rgbaq & 0xffff_ffff_0000_0000;
                self.rgbaq = q | field(low, 0, 8) | field(low, 32, 8) << 8 | field(high, 0, 8) << 16 | field(high, 32, 8) << 24;
            }
            reg::ST => {
                self.st = low;
                self.rgbaq = (self.rgbaq & 0xffff_ffff) | (high & 0xffff_ffff) << 32;
            }
            reg::UV => self.uv = field(low, 0, 14) | field(low, 32, 14) << 16,
            reg::XYZF2 => {
                let value = field(low, 0, 16) | field(low, 32, 16) << 16 | field(high, 4, 24) << 32 | field(high, 36, 8) << 56;
                self.register(if high & (1 << 47) != 0 { reg::XYZF3 } else { reg::XYZF2 }, value);
            }
            reg::XYZ2 => {
                let value = field(low, 0, 16) | field(low, 32, 16) << 16 | field(high, 0, 32) << 32;
                self.register(if high & (1 << 47) != 0 { reg::XYZ3 } else { reg::XYZ2 }, value);
            }
            reg::A_D => self.register(high as u8, low),
            // The fog's coefficient, which this renderer does not use, and the no-operation.
            reg::FOG | 0x0f => {}
            other => self.register(other, low),
        }
    }

    /// A write of a general register.
    pub fn register(&mut self, register: u8, value: u64) {
        // The registers of the two contexts sit side by side.
        let context = |r: u8, first: u8| usize::from(r - first);
        match register {
            reg::PRIM => {
                self.prim = value;
                self.queue.clear();
                self.seen = 0;
            }
            reg::RGBAQ => self.rgbaq = value,
            reg::ST => self.st = value,
            reg::UV => self.uv = value,
            reg::XYZF2 | reg::XYZ2 | reg::XYZF3 | reg::XYZ3 => {
                let depth = if matches!(register, reg::XYZF2 | reg::XYZF3) { field(value, 32, 24) } else { field(value, 32, 32) };
                self.vertex(
                    field(value, 0, 16) as i32,
                    field(value, 16, 16) as i32,
                    depth as u32,
                    matches!(register, reg::XYZF2 | reg::XYZ2),
                );
            }
            reg::TEX0 | 0x07 => self.contexts[context(register, reg::TEX0)].tex0 = value,
            reg::XYOFFSET | 0x19 => self.contexts[context(register, reg::XYOFFSET)].offset = value,
            reg::PRMODECONT => self.prmodecont = value & 1,
            reg::PRMODE => self.prmode = value,
            reg::TEXA => self.texa = value,
            reg::SCISSOR | 0x41 => self.contexts[context(register, reg::SCISSOR)].scissor = value,
            reg::ALPHA | 0x43 => self.contexts[context(register, reg::ALPHA)].alpha = value,
            reg::COLCLAMP => self.colclamp = value & 1,
            reg::TEST | 0x48 => self.contexts[context(register, reg::TEST)].test = value,
            reg::FBA | 0x4b => self.contexts[context(register, reg::FBA)].fba = value,
            reg::FRAME | 0x4d => self.contexts[context(register, reg::FRAME)].frame = value,
            reg::ZBUF | 0x4f => self.contexts[context(register, reg::ZBUF)].zbuf = value,
            reg::BITBLTBUF => self.bitbltbuf = value,
            reg::TRXPOS => self.trxpos = value,
            reg::TRXREG => self.trxreg = value,
            reg::TRXDIR => self.start_transfer(value & 3),
            reg::HWREG => self.image(&value.to_le_bytes()),
            reg::SIGNAL => self.csr |= CSR_SIGNAL,
            reg::FINISH => self.csr |= CSR_FINISH,
            _ => {}
        }
    }

    fn start_transfer(&mut self, direction: u64) {
        let (columns, rows) = (field(self.trxreg, 0, 12) as usize, field(self.trxreg, 32, 12) as usize);
        match direction {
            // From the host: the image data that follows fills the rectangle.
            0 => {
                self.transfer = Some(Transfer {
                    base: field(self.bitbltbuf, 32, 14) as usize * 256,
                    width: (field(self.bitbltbuf, 48, 6) as usize * 64).max(1),
                    format: field(self.bitbltbuf, 56, 6) as u32,
                    x: field(self.trxpos, 32, 11) as usize,
                    y: field(self.trxpos, 48, 11) as usize,
                    columns,
                    rows,
                    at: 0,
                })
                .filter(|t| t.columns * t.rows > 0);
            }
            // Within the local memory: the rectangle copied at once, pixel for pixel.
            2 => {
                let (source, source_width, format) = (
                    field(self.bitbltbuf, 0, 14) as usize * 256,
                    field(self.bitbltbuf, 16, 6) as usize * 64,
                    field(self.bitbltbuf, 24, 6) as u32,
                );
                let (target, target_width) = (field(self.bitbltbuf, 32, 14) as usize * 256, field(self.bitbltbuf, 48, 6) as usize * 64);
                let (sx, sy) = (field(self.trxpos, 0, 11) as usize, field(self.trxpos, 16, 11) as usize);
                let (dx, dy) = (field(self.trxpos, 32, 11) as usize, field(self.trxpos, 48, 11) as usize);
                let pixels: Vec<[u8; 4]> = (0..rows)
                    .flat_map(|y| (0..columns).map(move |x| (x, y)))
                    .map(|(x, y)| self.pixel(source, source_width, format, sx + x, sy + y))
                    .collect();
                for (i, rgba) in pixels.into_iter().enumerate() {
                    self.set_pixel(target, target_width, format, dx + i % columns, dy + i / columns, rgba, 0);
                }
            }
            _ => self.transfer = None,
        }
    }

    // endregion

    // region Drawing

    /// The attributes of the primitive being drawn: `PRIM`'s, or `PRMODE`'s under `PRIM`'s type.
    fn attributes(&self) -> u64 {
        if self.prmodecont == 1 { self.prim } else { (self.prim & 7) | (self.prmode & !7) }
    }

    /// A vertex written: it joins the primitive, which is drawn once it is whole when the write
    /// is a drawing kick.
    fn vertex(&mut self, x: i32, y: i32, z: u32, kick: bool) {
        let attributes = self.attributes();
        let context = self.contexts[field(attributes, 9, 1) as usize];
        let rgba = [0, 8, 16, 24].map(|low| field(self.rgbaq, low, 8) as i32);
        let q = f32::from_bits((self.rgbaq >> 32) as u32);
        let vertex = Vertex {
            x: x - field(context.offset, 0, 16) as i32,
            y: y - field(context.offset, 32, 16) as i32,
            z,
            rgba,
            uv: [field(self.uv, 0, 14) as f32 / 16.0, field(self.uv, 16, 14) as f32 / 16.0],
            stq: [f32::from_bits(self.st as u32), f32::from_bits((self.st >> 32) as u32), if q == 0.0 { 1.0 } else { q }],
        };
        self.queue.push(vertex);
        self.seen += 1;
        let kind = attributes & 7;
        let needed = match kind {
            0 => 1,
            1 | 2 | 6 => 2,
            _ => 3,
        };
        if self.queue.len() < needed {
            return;
        }
        let vertices = self.queue.clone();
        // What the next primitive keeps of this one: nothing, the last vertex of a line strip,
        // the last two of a triangle strip, the first and the last of a fan.
        match kind {
            2 => self.queue = vec![vertices[1]],
            4 => self.queue = vec![vertices[1], vertices[2]],
            5 => self.queue = vec![vertices[0], vertices[2]],
            _ => self.queue.clear(),
        }
        if !kick {
            return;
        }
        let shade = Shade::new(attributes, &context, self.colclamp);
        match kind {
            0 => self.draw_point(&vertices[0], &shade),
            1 | 2 => self.draw_line(&vertices[0], &vertices[1], &shade),
            6 => self.draw_sprite(&vertices[0], &vertices[1], &shade),
            _ => self.draw_triangle([vertices[0], vertices[1], vertices[2]], &shade),
        }
    }

    fn draw_point(&mut self, v: &Vertex, shade: &Shade) {
        let (x, y) = (v.x >> 4, v.y >> 4);
        if shade.scissor.holds(x, y) {
            self.fragment(x, y, v.z, v.rgba, [0.0, 0.0], shade);
        }
    }

    fn draw_line(&mut self, a: &Vertex, b: &Vertex, shade: &Shade) {
        let mut pixels = Vec::new();
        raster::line((a.x >> 4, a.y >> 4), (b.x >> 4, b.y >> 4), shade.scissor, |x, y, step, steps| {
            let mix = |from: i32, to: i32| from + (to - from) * step / steps;
            let rgba = if shade.gouraud { [0, 1, 2, 3].map(|c| mix(a.rgba[c], b.rgba[c])) } else { b.rgba };
            pixels.push((x, y, rgba));
        });
        for (x, y, rgba) in pixels {
            self.fragment(x, y, b.z, rgba, [0.0, 0.0], shade);
        }
    }

    /// A sprite: the rectangle between two corners, flat in the second's colour and depth, its
    /// texture coordinates running evenly from the first corner's to the second's.
    fn draw_sprite(&mut self, a: &Vertex, b: &Vertex, shade: &Shade) {
        let texel = |v: &Vertex| {
            if shade.fixed_uv { v.uv } else { [v.stq[0] / v.stq[2] * shade.texture.0 as f32, v.stq[1] / v.stq[2] * shade.texture.1 as f32] }
        };
        let (ta, tb) = (texel(a), texel(b));
        let (left, top, right, bottom) = (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
        // A pixel is covered when its corner is inside, the right and bottom edges outside.
        let rect = Rect::new((left + 15) >> 4, (top + 15) >> 4, (right + 15) >> 4, (bottom + 15) >> 4);
        let (width, height) = ((b.x - a.x) as f32 / 16.0, (b.y - a.y) as f32 / 16.0);
        let mut pixels = Vec::new();
        raster::rect(rect, shade.scissor, |x, y, _, _| {
            let along = |pixel: i32, start: i32, span: f32| if span == 0.0 { 0.0 } else { (pixel as f32 - start as f32 / 16.0) / span };
            let (fx, fy) = (along(x, a.x, width), along(y, a.y, height));
            pixels.push((x, y, [ta[0] + (tb[0] - ta[0]) * fx, ta[1] + (tb[1] - ta[1]) * fy]));
        });
        for (x, y, uv) in pixels {
            self.fragment(x, y, b.z, b.rgba, uv, shade);
        }
    }

    fn draw_triangle(&mut self, v: [Vertex; 3], shade: &Shade) {
        let mut pixels = Vec::new();
        raster::triangle([(v[0].x, v[0].y), (v[1].x, v[1].y), (v[2].x, v[2].y)], 4, shade.scissor, |x, y, weights| {
            let rgba =
                if shade.gouraud { [0, 1, 2, 3].map(|c| weights.int([v[0].rgba[c], v[1].rgba[c], v[2].rgba[c]])) } else { v[2].rgba };
            let z = weights.float([v[0].z as f32, v[1].z as f32, v[2].z as f32]) as u32;
            let uv = if !shade.textured {
                [0.0, 0.0]
            } else if shade.fixed_uv {
                [0, 1].map(|c| weights.float([v[0].uv[c], v[1].uv[c], v[2].uv[c]]))
            } else {
                // S, T and Q run evenly over the screen; their quotients are the texel.
                let part = |c: usize| weights.float([v[0].stq[c], v[1].stq[c], v[2].stq[c]]);
                let q = part(2);
                [part(0) / q * shade.texture.0 as f32, part(1) / q * shade.texture.1 as f32]
            };
            pixels.push((x, y, z, rgba, uv));
        });
        for (x, y, z, rgba, uv) in pixels {
            self.fragment(x, y, z, rgba, uv, shade);
        }
    }

    /// The texel of the context's texture at `uv`, the nearest one, through the lookup table of
    /// an indexed format.
    fn texel(&self, shade: &Shade, uv: [f32; 2]) -> [u8; 4] {
        let tex0 = shade.tex0;
        let (base, stride, format) =
            (field(tex0, 0, 14) as usize * 256, (field(tex0, 14, 6) as usize * 64).max(1), field(tex0, 20, 6) as u32);
        let wrap = |v: f32, size: usize| (v.floor() as i64).rem_euclid(size.max(1) as i64) as usize;
        let (u, v) = (wrap(uv[0], shade.texture.0), wrap(uv[1], shade.texture.1));
        let texel = self.pixel(base, stride, format, u, v);
        let mut rgba = match format & 0x3f {
            PSMT8 | PSMT4 => {
                // The table as it was sent, in a buffer 64 pixels wide: sixteen entries to a row
                // for an eight-bit index, whose bits 3 and 4 are swapped in the stored order,
                // eight to a row for a four-bit one.
                let index = usize::from(texel[0]);
                let (table, table_format) = (field(tex0, 37, 14) as usize * 256, field(tex0, 51, 4) as u32);
                let (column, row) = if format & 0x3f == PSMT8 {
                    let index = (index & 0xe7) | (index & 0x08) << 1 | (index & 0x10) >> 1;
                    (index % 16, index / 16)
                } else {
                    let index = index + field(tex0, 56, 5) as usize * 16;
                    (index % 8, index / 8)
                };
                self.pixel(table, 64, table_format, column, row)
            }
            _ => texel,
        };
        // A format without alpha takes it from TEXA: one value, or two by the pixel's own bit.
        match format & 0x3f {
            PSMCT24 => rgba[3] = field(self.texa, 0, 8) as u8,
            PSMCT16 | PSMCT16S => rgba[3] = if rgba[3] != 0 { field(self.texa, 32, 8) as u8 } else { field(self.texa, 0, 8) as u8 },
            _ => {}
        }
        rgba
    }

    /// One pixel of a primitive through the tests, the texture, the blend and the masks.
    fn fragment(&mut self, x: i32, y: i32, z: u32, rgba: [i32; 4], uv: [f32; 2], shade: &Shade) {
        let (x, y) = (x as usize, y as usize);
        let mut colour = rgba;
        if shade.textured {
            let texel = self.texel(shade, uv).map(i32::from);
            let modulated = |c: usize| (texel[c] * rgba[c]) >> 7;
            colour = match shade.function {
                // Modulate, with the texture's alpha when the texture has one; decal.
                0 => [modulated(0), modulated(1), modulated(2), if shade.texture_alpha { modulated(3) } else { rgba[3] }],
                1 => [texel[0], texel[1], texel[2], if shade.texture_alpha { texel[3] } else { rgba[3] }],
                // The two highlights add the primitive's alpha to the modulated colour.
                _ => {
                    let lit = |c: usize| modulated(c) + rgba[3];
                    [
                        lit(0),
                        lit(1),
                        lit(2),
                        if shade.texture_alpha { texel[3] + if shade.function == 2 { rgba[3] } else { 0 } } else { rgba[3] },
                    ]
                }
            }
            .map(|c| c.clamp(0, 255));
        }
        let test = shade.test;
        // The alpha test: a failed one may still write the colour or the depth.
        let (mut write_colour, mut write_depth) = (true, true);
        if test & 1 != 0 {
            let (alpha, reference) = (colour[3], field(test, 4, 8) as i32);
            let pass = match field(test, 1, 3) {
                0 => false,
                1 => true,
                2 => alpha < reference,
                3 => alpha <= reference,
                4 => alpha == reference,
                5 => alpha >= reference,
                6 => alpha > reference,
                _ => alpha != reference,
            };
            if !pass {
                match field(test, 12, 2) {
                    0 => return,
                    1 => write_depth = false,
                    2 => write_colour = false,
                    _ => write_depth = false,
                }
            }
        }
        let behind = self.pixel(shade.frame.0, shade.frame.1, shade.frame.2, x, y);
        // The destination alpha test.
        if test & (1 << 14) != 0 && (behind[3] & 0x80 != 0) != (test & (1 << 15) != 0) {
            return;
        }
        // The depth test, against a buffer of the frame's width.
        let depth_at = shade.depth.0 + (y * shade.frame.1 + x) * 4;
        if test & (1 << 16) != 0 {
            let held = u32::from_le_bytes([0, 1, 2, 3].map(|i| self.memory[(depth_at + i) & (MEMORY - 1)]));
            let pass = match field(test, 17, 2) {
                0 => false,
                1 => true,
                2 => z >= held,
                _ => z > held,
            };
            if !pass {
                return;
            }
        }
        if write_depth && !shade.depth.1 {
            for (i, byte) in z.to_le_bytes().into_iter().enumerate() {
                self.memory[(depth_at + i) & (MEMORY - 1)] = byte;
            }
        }
        if !write_colour {
            return;
        }
        if shade.blend {
            // (A - B) * C / 128 + D, each of the source, the destination or nothing.
            let alpha = shade.alpha;
            let source = |pick: u64, c: usize| match pick {
                0 => colour[c],
                1 => i32::from(behind[c]),
                _ => 0,
            };
            let factor = match field(alpha, 4, 2) {
                0 => colour[3],
                1 => i32::from(behind[3]),
                _ => field(alpha, 32, 8) as i32,
            };
            let mixed = [0, 1, 2].map(|c| {
                let mixed =
                    (((source(field(alpha, 0, 2), c) - source(field(alpha, 2, 2), c)) * factor) >> 7) + source(field(alpha, 6, 2), c);
                if shade.clamp { mixed.clamp(0, 255) } else { mixed & 0xff }
            });
            colour = [mixed[0], mixed[1], mixed[2], colour[3]];
        }
        let mut out = colour.map(|c| c.clamp(0, 255) as u8);
        if shade.set_alpha {
            out[3] |= 0x80;
        }
        self.set_pixel(shade.frame.0, shade.frame.1, shade.frame.2, x, y, out, shade.mask);
    }

    // endregion
}

/// What every pixel of one primitive shares: its attributes and its context, read once.
struct Shade {
    gouraud: bool,
    textured: bool,
    blend: bool,
    fixed_uv: bool,
    scissor: Rect,
    /// The frame buffer's first byte, its width in pixels and its format; the write mask.
    frame: (usize, usize, u32),
    mask: u32,
    /// The depth buffer's first byte, and whether writes to it are masked.
    depth: (usize, bool),
    tex0: u64,
    /// The texture's size, its function, and whether its alpha counts.
    texture: (usize, usize),
    function: u64,
    texture_alpha: bool,
    test: u64,
    alpha: u64,
    clamp: bool,
    set_alpha: bool,
}

impl Shade {
    fn new(attributes: u64, context: &Context, colclamp: u64) -> Self {
        let scissor = context.scissor;
        Self {
            gouraud: attributes & (1 << 3) != 0,
            textured: attributes & (1 << 4) != 0,
            blend: attributes & (1 << 6) != 0,
            fixed_uv: attributes & (1 << 8) != 0,
            scissor: Rect::new(
                field(scissor, 0, 11) as i32,
                field(scissor, 32, 11) as i32,
                field(scissor, 16, 11) as i32 + 1,
                field(scissor, 48, 11) as i32 + 1,
            ),
            frame: (
                field(context.frame, 0, 9) as usize * 8192,
                (field(context.frame, 16, 6) as usize * 64).max(1),
                field(context.frame, 24, 6) as u32,
            ),
            mask: field(context.frame, 32, 32) as u32,
            depth: (field(context.zbuf, 0, 9) as usize * 8192, context.zbuf & (1 << 32) != 0),
            tex0: context.tex0,
            texture: (1 << field(context.tex0, 26, 4).min(10), 1 << field(context.tex0, 30, 4).min(10)),
            function: field(context.tex0, 35, 2),
            texture_alpha: context.tex0 & (1 << 34) != 0,
            test: context.test,
            alpha: context.alpha,
            clamp: colclamp != 0,
            set_alpha: context.fba & 1 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GS showing 640 by 448 from a 32-bit frame buffer at the start of its memory, with the
    /// scissor the whole of it and no offset.
    fn gs() -> Gs {
        let mut gs = Gs::new();
        gs.write(PMODE, 1);
        gs.write(DISPFB1, 10 << 9);
        gs.write(DISPLAY1, (639 << 32) | (447 << 44));
        gs.register(reg::FRAME, 10 << 16);
        gs.register(reg::SCISSOR, (639 << 16) | (447 << 48));
        gs.register(reg::TEST, 0);
        gs
    }

    fn xyz(x: u64, y: u64) -> u64 {
        (x << 4) | (y << 4) << 16
    }

    fn at(gs: &Gs, x: usize, y: usize) -> u32 {
        gs.frame().pixels[y * 640 + x]
    }

    #[test]
    fn a_sprite_fills_between_its_corners_in_the_last_colour() {
        let mut gs = gs();
        gs.register(reg::PRIM, 6);
        gs.register(reg::RGBAQ, 0x8000_00ff);
        gs.register(reg::XYZ2, xyz(10, 20));
        gs.register(reg::RGBAQ, 0x80ff_0000);
        gs.register(reg::XYZ2, xyz(14, 22));
        assert_eq!(at(&gs, 10, 20), 0x0000_00ff, "blue: the second corner's colour");
        assert_eq!(at(&gs, 13, 21), 0x0000_00ff);
        assert_eq!((at(&gs, 14, 20), at(&gs, 10, 22), at(&gs, 9, 20)), (0, 0, 0), "the right and bottom edges are outside");
    }

    #[test]
    fn a_shaded_triangle_through_a_packed_gif_packet() {
        let mut gs = gs();
        // One loop of PRIM, then three of RGBAQ and XYZ2: a tag each, as a library writes them.
        gs.gif(1 | 0x8000 | (1 << 60), u64::from(reg::A_D));
        gs.gif(3 | 8, u64::from(reg::PRIM));
        gs.gif(3 | 0x8000 | (2 << 60), u64::from(reg::RGBAQ) | u64::from(reg::XYZ2) << 4);
        for (colour, (x, y)) in [(0xffu64, (0u64, 0u64)), (0xff << 32, (64, 0)), (0, (0, 64))] {
            let (red, green) = (colour & 0xff, colour >> 32);
            gs.gif(red | green << 32, if colour == 0 { 0xff } else { 0 });
            gs.gif((x << 4) | (y << 4) << 32, 0);
        }
        assert!(gs.idle());
        assert_eq!(at(&gs, 0, 0), 0x00ff_0000, "red at the first corner");
        let middle = at(&gs, 32, 0);
        assert_eq!((middle >> 16, (middle >> 8) & 0xff), (127, 127), "half red, half green along the top");
        assert_eq!(at(&gs, 40, 40), 0, "outside the triangle");
    }

    #[test]
    fn a_strip_draws_a_triangle_a_vertex_after_the_first_two() {
        let mut gs = gs();
        gs.register(reg::PRIM, 4);
        gs.register(reg::RGBAQ, 0x8000_ff00);
        for (x, y) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
            gs.register(reg::XYZ2, xyz(x, y));
        }
        assert_eq!((at(&gs, 1, 1), at(&gs, 6, 6)), (0x0000_ff00, 0x0000_ff00), "both halves of the square");
        assert_eq!(at(&gs, 8, 8), 0);
    }

    #[test]
    fn an_image_transfer_fills_a_texture_a_sprite_then_samples() {
        let mut gs = gs();
        // A 2 by 2 texture of 32 bits at block 0x2000: red, green, blue, white.
        gs.register(reg::BITBLTBUF, 0x2000 << 32 | 1 << 48);
        gs.register(reg::TRXPOS, 0);
        gs.register(reg::TRXREG, 2 | 2 << 32);
        gs.register(reg::TRXDIR, 0);
        gs.gif(1 | 0x8000 | (2 << 58), 0);
        gs.gif(0x8000_ff00_8000_00ff, 0x80ff_ffff_80ff_0000);
        assert!(gs.transfer.is_none(), "the transfer took its four pixels");
        // Decal, with the texture's alpha, 2 by 2 texels, stretched over four pixels.
        gs.register(reg::TEX0, 0x2000 | 1 << 14 | 1 << 26 | 1 << 30 | 1 << 34 | 1 << 35);
        gs.register(reg::PRIM, 6 | 1 << 4 | 1 << 8);
        gs.register(reg::RGBAQ, 0x8080_8080);
        gs.register(reg::UV, 0);
        gs.register(reg::XYZ2, xyz(100, 100));
        gs.register(reg::UV, (2 << 4) | (2 << 4) << 16);
        gs.register(reg::XYZ2, xyz(104, 104));
        assert_eq!(
            [at(&gs, 100, 100), at(&gs, 103, 100), at(&gs, 100, 103), at(&gs, 103, 103)],
            [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x00ff_ffff]
        );
    }

    #[test]
    fn the_depth_test_keeps_the_nearer_pixel_and_blending_averages() {
        let mut gs = gs();
        gs.register(reg::ZBUF, 0x100);
        // The depth test on, greater or equal.
        gs.register(reg::TEST, 1 << 16 | 2 << 17);
        gs.register(reg::PRIM, 6);
        gs.register(reg::RGBAQ, 0x8000_00ff);
        gs.register(reg::XYZ2, xyz(0, 0) | 5 << 32);
        gs.register(reg::XYZ2, xyz(4, 4) | 5 << 32);
        gs.register(reg::RGBAQ, 0x8000_ff00);
        gs.register(reg::XYZ2, xyz(2, 0) | 3 << 32);
        gs.register(reg::XYZ2, xyz(6, 4) | 3 << 32);
        assert_eq!((at(&gs, 3, 0), at(&gs, 5, 0)), (0x00ff_0000, 0x0000_ff00), "the farther sprite shows only beside the nearer");
        // Half of the source over half of the destination: (Cs - Cd) * 0x40 / 0x80 + Cd.
        gs.register(reg::TEST, 0);
        gs.register(reg::ALPHA, 1 << 2 | 2 << 4 | 1 << 6 | 0x40 << 32);
        gs.register(reg::PRIM, 6 | 1 << 6);
        gs.register(reg::RGBAQ, 0x80ff_0000);
        gs.register(reg::XYZ2, xyz(0, 0));
        gs.register(reg::XYZ2, xyz(1, 1));
        assert_eq!(at(&gs, 0, 0), 0x007f_007f, "half red, half blue");
    }

    #[test]
    fn the_status_register_takes_events_and_gives_them_up_to_a_one() {
        let mut gs = gs();
        gs.register(reg::FINISH, 0);
        gs.vblank();
        assert_eq!(gs.read(CSR) & 0xf, CSR_FINISH | CSR_VSINT);
        assert!(!gs.interrupt(), "every event is masked at first");
        gs.write(IMR, 0x7700);
        assert!(gs.interrupt());
        gs.write(CSR, CSR_VSINT);
        assert_eq!(gs.read(CSR) & 0xf, CSR_FINISH);
    }
}
