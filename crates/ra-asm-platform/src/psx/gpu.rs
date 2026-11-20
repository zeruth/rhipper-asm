//! The PlayStation's GPU: a megabyte of video memory, 1024 by 512 pixels of sixteen bits, and two
//! ports. `GP0` takes the drawing commands (fills, polygons, lines, rectangles, copies to, from
//! and within the video memory, and the drawing environment), `GP1` the display's (which part of
//! the video memory the television shows, and how). The commands are rendered here, in software,
//! as they arrive.

use crate::host::Frame;
use crate::raster::{self, Rect};

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

/// The offsets the GPU adds to a colour's channels before it drops their low bits, by the
/// pixel's place in a four by four tile.
const DITHER: [[i32; 4]; 4] = [[-4, 0, -3, 1], [2, -2, 3, -1], [-3, 1, -4, 0], [3, -1, 2, -2]];

/// How many words each `GP0` command takes, its own word among them, by its first byte; the
/// copies and the polylines go on past theirs.
fn command_words(command: u8) -> usize {
    match command {
        0x02 => 3,
        0x20..=0x3f => {
            let vertices = if command & 0x08 != 0 { 4 } else { 3 };
            let per_vertex = 1 + usize::from(command & 0x04 != 0);
            let colours = if command & 0x10 != 0 { vertices - 1 } else { 0 };
            1 + vertices * per_vertex + colours
        }
        0x40..=0x5f => {
            if command & 0x10 != 0 {
                4
            } else {
                3
            }
        }
        0x60..=0x7f => 2 + usize::from(command & 0x04 != 0) + usize::from(command & 0x18 == 0),
        0x80..=0x9f => 4,
        0xa0..=0xdf => 3,
        _ => 1,
    }
}

/// A corner of a primitive: where, what colour, and which texel.
#[derive(Debug, Clone, Copy, Default)]
struct Vertex {
    x: i32,
    y: i32,
    rgb: [i32; 3],
    uv: [i32; 2],
}

/// How a primitive's pixels are made.
#[derive(Debug, Clone, Copy, Default)]
struct Shade {
    textured: bool,
    /// The texel's colour as it is, not multiplied by the primitive's.
    raw: bool,
    semi: bool,
    /// The colour varies over the primitive, which is when the GPU dithers.
    shaded: bool,
    clut: u16,
}

/// A copy between the processor and the video memory in flight: the rectangle and the pixel the
/// next word goes to or comes from.
#[derive(Debug, Clone, Copy)]
struct Transfer {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    at: u32,
}

impl Transfer {
    fn new(xy: u32, wh: u32) -> Self {
        let size = |v: u32, mask: u32| ((v.wrapping_sub(1)) & mask) + 1;
        Self { x: xy & 0x3ff, y: (xy >> 16) & 0x1ff, width: size(wh & 0xffff, 0x3ff), height: size(wh >> 16, 0x1ff), at: 0 }
    }

    fn pixels(&self) -> u32 {
        self.width * self.height
    }

    /// The video memory index of the transfer's next pixel, which it moves past.
    fn next(&mut self) -> usize {
        let (dx, dy) = (self.at % self.width, self.at / self.width);
        self.at += 1;
        (((self.y + dy) & 0x1ff) as usize) * VRAM_WIDTH + ((self.x + dx) & 0x3ff) as usize
    }
}

/// The commands of the frame being drawn are printed ([`Gpu::trace_frame`]).
static FRAME_TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Gpu {
    pub vram: Vec<u16>,
    /// The drawing mode of `GP0(E1)`, as the status register's low bits hold it: the texture
    /// page, the blending, the texture's depth, dithering, drawing on the displayed lines.
    mode: u32,
    /// The texture window: the mask and the offset of each axis, in units of eight texels.
    window: [u32; 4],
    area: Rect,
    offset: (i32, i32),
    /// Draw with the mask bit set, and leave the pixels that have it alone.
    set_mask: bool,
    check_mask: bool,
    /// The display's side: off, where in the video memory it starts, its ranges and its mode.
    display_off: bool,
    display_start: (u32, u32),
    horizontal: (u32, u32),
    vertical: (u32, u32),
    display_mode: u32,
    dma_direction: u32,
    interrupt: bool,
    /// The words of the command being received.
    fifo: Vec<u32>,
    load: Option<Transfer>,
    store: Option<Transfer>,
    /// A polyline in flight: its command, and the last vertex drawn to.
    polyline: Option<(u8, Vertex)>,
    read_latch: u32,
}

impl Default for Gpu {
    fn default() -> Self {
        Self::new()
    }
}

fn sext11(v: u32) -> i32 {
    ((v << 21) as i32) >> 21
}

fn rgb_of(word: u32) -> [i32; 3] {
    [(word & 0xff) as i32, ((word >> 8) & 0xff) as i32, ((word >> 16) & 0xff) as i32]
}

/// A 24-bit colour as the video memory's fifteen bits.
fn to15(rgb: [i32; 3]) -> u16 {
    ((rgb[0] >> 3) | ((rgb[1] >> 3) << 5) | ((rgb[2] >> 3) << 10)) as u16
}

impl Gpu {
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_WIDTH * VRAM_HEIGHT],
            mode: 0,
            window: [0; 4],
            area: Rect::new(0, 0, 0, 0),
            offset: (0, 0),
            set_mask: false,
            check_mask: false,
            display_off: true,
            display_start: (0, 0),
            horizontal: (0x200, 0xc00),
            vertical: (0x10, 0x100),
            display_mode: 0,
            dma_direction: 0,
            interrupt: false,
            fifo: Vec::new(),
            load: None,
            store: None,
            polyline: None,
            read_latch: 0,
        }
    }

    /// Whether the GPU asked for its interrupt (`GP0(1F)`) since the last acknowledge.
    pub fn take_interrupt(&mut self) -> bool {
        std::mem::take(&mut self.interrupt)
    }

    /// The status register. `line` is the scanline the beam is on and `frame` the count of
    /// frames, which give the bit that tells the lines or the fields apart.
    pub fn status(&self, line: u32, frame: u64) -> u32 {
        let interlaced = self.display_mode & 0x24 == 0x24;
        let odd = if interlaced { frame & 1 == 1 } else { line & 1 == 1 };
        let in_blank = line >= 240;
        let mut status = self.mode & 0x7ff;
        status |= u32::from(self.set_mask) << 11;
        status |= u32::from(self.check_mask) << 12;
        // The field, which reads as set outside interlace.
        status |= u32::from(!interlaced || frame & 1 == 1) << 13;
        status |= (self.mode & 0x800) << 4;
        status |= (self.display_mode & 0x40) << 10;
        status |= (self.display_mode & 0x3f) << 17;
        status |= u32::from(self.display_off) << 23;
        status |= u32::from(self.interrupt) << 24;
        // Ready for a command, for a read of the video memory, and for a block by DMA: the
        // commands run as they arrive, so the GPU is never busy.
        status |= 1 << 26;
        status |= u32::from(self.store.is_some()) << 27;
        status |= 1 << 28;
        status |= self.dma_direction << 29;
        let request = match self.dma_direction {
            1 => 1,
            2 => (status >> 28) & 1,
            3 => (status >> 27) & 1,
            _ => 0,
        };
        status |= request << 25;
        status |= u32::from(odd && !in_blank) << 31;
        status
    }

    /// A word written to `GP1`.
    pub fn gp1(&mut self, word: u32) {
        match (word >> 24) & 0x3f {
            0x00 => {
                let vram = std::mem::take(&mut self.vram);
                *self = Self { vram, ..Self::new() };
            }
            0x01 => {
                self.fifo.clear();
                self.load = None;
                self.store = None;
                self.polyline = None;
            }
            0x02 => self.interrupt = false,
            0x03 => self.display_off = word & 1 != 0,
            0x04 => self.dma_direction = word & 3,
            0x05 => self.display_start = (word & 0x3ff, (word >> 10) & 0x1ff),
            0x06 => self.horizontal = (word & 0xfff, (word >> 12) & 0xfff),
            0x07 => self.vertical = (word & 0x3ff, (word >> 10) & 0x3ff),
            0x08 => self.display_mode = word & 0xff,
            0x10..=0x1f => {
                self.read_latch = match word & 7 {
                    2 => self.window[0] | (self.window[1] << 5) | (self.window[2] << 10) | (self.window[3] << 15),
                    3 => (self.area.left as u32) | ((self.area.top as u32) << 10),
                    4 => ((self.area.right - 1) as u32 & 0x3ff) | (((self.area.bottom - 1) as u32 & 0x3ff) << 10),
                    5 => (self.offset.0 as u32 & 0x7ff) | ((self.offset.1 as u32 & 0x7ff) << 11),
                    // The GPU's version: the one of the later consoles.
                    7 => 2,
                    _ => self.read_latch,
                };
            }
            _ => {}
        }
    }

    /// A read of `GPUREAD`: the next word of a copy from the video memory, else the last answer.
    pub fn read(&mut self) -> u32 {
        let Some(mut transfer) = self.store else { return self.read_latch };
        let mut word = 0;
        for half in 0..2 {
            if transfer.at < transfer.pixels() {
                word |= u32::from(self.vram[transfer.next()]) << (16 * half);
            }
        }
        self.store = (transfer.at < transfer.pixels()).then_some(transfer);
        self.read_latch = word;
        word
    }

    /// A word written to `GP0`.
    pub fn gp0(&mut self, word: u32) {
        if let Some(mut transfer) = self.load {
            for half in 0..2 {
                if transfer.at < transfer.pixels() {
                    let at = transfer.next();
                    let pixel = (word >> (16 * half)) as u16;
                    if !(self.check_mask && self.vram[at] & 0x8000 != 0) {
                        self.vram[at] = pixel | (u16::from(self.set_mask) << 15);
                    }
                }
            }
            self.load = (transfer.at < transfer.pixels()).then_some(transfer);
            return;
        }
        if let Some((command, last)) = self.polyline {
            if self.fifo.is_empty() && word & 0xf000_f000 == 0x5000_5000 {
                self.polyline = None;
                return;
            }
            self.fifo.push(word);
            let gouraud = command & 0x10 != 0;
            if self.fifo.len() == 1 + usize::from(gouraud) {
                let rgb = if gouraud { rgb_of(self.fifo[0]) } else { last.rgb };
                let next = self.vertex(*self.fifo.last().unwrap_or(&0), rgb, 0);
                self.fifo.clear();
                self.draw_line(last, next, command & 0x02 != 0, gouraud);
                self.polyline = Some((command, next));
            }
            return;
        }
        self.fifo.push(word);
        let command = (self.fifo[0] >> 24) as u8;
        if self.fifo.len() == command_words(command) {
            let words = std::mem::take(&mut self.fifo);
            self.run(command, &words);
        }
    }

    fn vertex(&self, xy: u32, rgb: [i32; 3], uv: u32) -> Vertex {
        Vertex {
            x: sext11(xy) + self.offset.0,
            y: sext11(xy >> 16) + self.offset.1,
            rgb,
            uv: [(uv & 0xff) as i32, ((uv >> 8) & 0xff) as i32],
        }
    }

    /// Says whether the frames `RA_GPU_FRAME=<n>` names are being drawn: the commands of frame
    /// `n` and the one after it are printed, with no other tracing on.
    pub(super) fn trace_frame(frame: u64) {
        static FROM: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
        if let Some(from) = FROM.get_or_init(|| std::env::var("RA_GPU_FRAME").ok().and_then(|n| n.parse().ok())) {
            let on = frame >= *from && frame < from + 2;
            if on != FRAME_TRACE.swap(on, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("gp0: frame {frame}, trace {}", if on { "on" } else { "off" });
            }
        }
    }

    fn run(&mut self, command: u8, w: &[u32]) {
        if FRAME_TRACE.load(std::sync::atomic::Ordering::Relaxed) || (crate::tracing() && std::env::var_os("RA_GPU_TRACE").is_some()) {
            let words: Vec<String> = w.iter().map(|word| format!("{word:08x}")).collect();
            eprintln!("gp0: {}", words.join(" "));
        }
        let started = crate::profile::start();
        self.draw(command, w);
        crate::profile::end(crate::profile::Part::Draw, started);
    }

    fn draw(&mut self, command: u8, w: &[u32]) {
        match command {
            0x02 => self.fill(w[0], w[1], w[2]),
            0x1f => self.interrupt = true,
            0x20..=0x3f => self.polygon(command, w),
            0x40..=0x5f => {
                let gouraud = command & 0x10 != 0;
                let first = self.vertex(w[1], rgb_of(w[0]), 0);
                let second = if gouraud { self.vertex(w[3], rgb_of(w[2]), 0) } else { self.vertex(w[2], rgb_of(w[0]), 0) };
                self.draw_line(first, second, command & 0x02 != 0, gouraud);
                if command & 0x08 != 0 {
                    self.polyline = Some((command, second));
                }
            }
            0x60..=0x7f => self.rectangle(command, w),
            0x80..=0x9f => self.copy(w[1], w[2], w[3]),
            0xa0..=0xbf => self.load = Some(Transfer::new(w[1], w[2])),
            0xc0..=0xdf => self.store = Some(Transfer::new(w[1], w[2])),
            0xe1 => self.mode = (self.mode & !0xfff) | (w[0] & 0x7ff) | ((w[0] >> 11) & 1) << 11,
            0xe2 => self.window = [w[0] & 0x1f, (w[0] >> 5) & 0x1f, (w[0] >> 10) & 0x1f, (w[0] >> 15) & 0x1f],
            0xe3 => {
                self.area.left = (w[0] & 0x3ff) as i32;
                self.area.top = ((w[0] >> 10) & 0x1ff) as i32;
            }
            0xe4 => {
                self.area.right = (w[0] & 0x3ff) as i32 + 1;
                self.area.bottom = ((w[0] >> 10) & 0x1ff) as i32 + 1;
            }
            0xe5 => self.offset = (sext11(w[0]), sext11(w[0] >> 11)),
            0xe6 => {
                self.set_mask = w[0] & 1 != 0;
                self.check_mask = w[0] & 2 != 0;
            }
            _ => {}
        }
    }

    /// `GP0(02)`: a rectangle of one colour, whatever the drawing area and the mask say; its
    /// left edge and its width in steps of sixteen pixels.
    fn fill(&mut self, colour: u32, xy: u32, wh: u32) {
        let pixel = to15(rgb_of(colour));
        let (x, y) = ((xy & 0x3f0) as usize, ((xy >> 16) & 0x1ff) as usize);
        let (width, height) = ((((wh & 0x3ff) + 0xf) & !0xf) as usize, ((wh >> 16) & 0x1ff) as usize);
        for row in 0..height {
            for column in 0..width {
                self.vram[((y + row) & 0x1ff) * VRAM_WIDTH + ((x + column) & 0x3ff)] = pixel;
            }
        }
    }

    /// `GP0(80)`: a rectangle of the video memory copied to another place of it.
    fn copy(&mut self, from: u32, to: u32, wh: u32) {
        let (mut source, mut target) = (Transfer::new(from, wh), Transfer::new(to, wh));
        let pixels: Vec<u16> = (0..source.pixels()).map(|_| self.vram[source.next()]).collect();
        for pixel in pixels {
            let at = target.next();
            if !(self.check_mask && self.vram[at] & 0x8000 != 0) {
                self.vram[at] = pixel | (u16::from(self.set_mask) << 15);
            }
        }
    }

    fn polygon(&mut self, command: u8, w: &[u32]) {
        let (gouraud, quad, textured) = (command & 0x10 != 0, command & 0x08 != 0, command & 0x04 != 0);
        let count = if quad { 4 } else { 3 };
        let mut vertices = [Vertex::default(); 4];
        let mut at = 0;
        let mut shade = Shade { textured, raw: textured && command & 0x01 != 0, semi: command & 0x02 != 0, shaded: gouraud, clut: 0 };
        for (i, vertex) in vertices.iter_mut().enumerate().take(count) {
            let rgb = if i == 0 || gouraud {
                at += 1;
                rgb_of(w[at - 1])
            } else {
                rgb_of(w[0])
            };
            let xy = w[at];
            at += 1;
            let uv = if textured {
                at += 1;
                w[at - 1]
            } else {
                0
            };
            match i {
                0 => shade.clut = (uv >> 16) as u16,
                // The second vertex's word carries the texture page: the drawing mode takes it.
                1 if textured => self.mode = (self.mode & !0x9ff) | ((uv >> 16) & 0x9ff),
                _ => {}
            }
            *vertex = self.vertex(xy, rgb, uv);
        }
        self.draw_triangle([vertices[0], vertices[1], vertices[2]], shade);
        if quad {
            self.draw_triangle([vertices[1], vertices[2], vertices[3]], shade);
        }
    }

    fn rectangle(&mut self, command: u8, w: &[u32]) {
        let textured = command & 0x04 != 0;
        let mut at = 2;
        let uv = if textured {
            at += 1;
            w[2]
        } else {
            0
        };
        let (width, height) = match (command >> 3) & 3 {
            0 => ((w[at] & 0x3ff) as i32, ((w[at] >> 16) & 0x1ff) as i32),
            1 => (1, 1),
            2 => (8, 8),
            _ => (16, 16),
        };
        let corner = self.vertex(w[1], rgb_of(w[0]), uv);
        let shade =
            Shade { textured, raw: textured && command & 0x01 != 0, semi: command & 0x02 != 0, shaded: false, clut: (uv >> 16) as u16 };
        let mut pixels = Vec::new();
        raster::rect(Rect::new(corner.x, corner.y, corner.x + width, corner.y + height), self.area, |x, y, u, v| {
            pixels.push((x, y, [(corner.uv[0] + u) & 0xff, (corner.uv[1] + v) & 0xff]));
        });
        for (x, y, uv) in pixels {
            self.pixel(x, y, corner.rgb, uv, shade);
        }
    }

    fn draw_triangle(&mut self, v: [Vertex; 3], shade: Shade) {
        // The GPU drops a primitive wider than the video memory or taller than it.
        let span = |f: fn(&Vertex) -> i32| v.iter().map(f).max().unwrap_or(0) - v.iter().map(f).min().unwrap_or(0);
        if span(|v| v.x) > 1023 || span(|v| v.y) > 511 {
            return;
        }
        let mut pixels = Vec::new();
        raster::triangle([(v[0].x, v[0].y), (v[1].x, v[1].y), (v[2].x, v[2].y)], 0, self.area, |x, y, weights| {
            let channel = |c: usize| weights.int([v[0].rgb[c], v[1].rgb[c], v[2].rgb[c]]);
            let texel = |c: usize| weights.int([v[0].uv[c], v[1].uv[c], v[2].uv[c]]);
            let rgb = if shade.shaded { [channel(0), channel(1), channel(2)] } else { v[0].rgb };
            let uv = if shade.textured { [texel(0), texel(1)] } else { [0, 0] };
            pixels.push((x, y, rgb, uv));
        });
        for (x, y, rgb, uv) in pixels {
            self.pixel(x, y, rgb, uv, shade);
        }
    }

    fn draw_line(&mut self, a: Vertex, b: Vertex, semi: bool, gouraud: bool) {
        if (a.x - b.x).abs() > 1023 || (a.y - b.y).abs() > 511 {
            return;
        }
        let shade = Shade { semi, shaded: gouraud, ..Shade::default() };
        let mut pixels = Vec::new();
        raster::line((a.x, a.y), (b.x, b.y), self.area, |x, y, step, steps| {
            let channel = |c: usize| a.rgb[c] + (b.rgb[c] - a.rgb[c]) * step / steps;
            pixels.push((x, y, if gouraud { [channel(0), channel(1), channel(2)] } else { a.rgb }));
        });
        for (x, y, rgb) in pixels {
            self.pixel(x, y, rgb, [0, 0], shade);
        }
    }

    /// The texel at `uv` of the texture page the drawing mode names, through the texture window
    /// and, for the two paletted depths, the lookup table at `clut`.
    fn texel(&self, uv: [i32; 2], clut: u16) -> u16 {
        let through = |v: i32, mask: u32, offset: u32| ((v as u32) & !(mask * 8)) | ((offset & mask) * 8);
        let u = through(uv[0], self.window[0], self.window[2]) as usize;
        let v = through(uv[1], self.window[1], self.window[3]) as usize;
        let (page_x, page_y) = ((self.mode & 0xf) as usize * 64, ((self.mode >> 4) & 1) as usize * 256);
        let at = |x: usize, y: usize| self.vram[(y & 0x1ff) * VRAM_WIDTH + (x & 0x3ff)];
        let (clut_x, clut_y) = (usize::from(clut & 0x3f) * 16, usize::from((clut >> 6) & 0x1ff));
        match (self.mode >> 7) & 3 {
            0 => {
                let index = (at(page_x + u / 4, page_y + v) >> ((u & 3) * 4)) & 0xf;
                at(clut_x + usize::from(index), clut_y)
            }
            1 => {
                let index = (at(page_x + u / 2, page_y + v) >> ((u & 1) * 8)) & 0xff;
                at(clut_x + usize::from(index), clut_y)
            }
            _ => at(page_x + u, page_y + v),
        }
    }

    /// One pixel of a primitive: the texel or the colour, modulated, dithered, blended with what
    /// the video memory holds, and stored under the mask.
    fn pixel(&mut self, x: i32, y: i32, rgb: [i32; 3], uv: [i32; 2], shade: Shade) {
        let at = (y as usize & 0x1ff) * VRAM_WIDTH + (x as usize & 0x3ff);
        let behind = self.vram[at];
        if self.check_mask && behind & 0x8000 != 0 {
            return;
        }
        let dither = if self.mode & 0x200 != 0 && (shade.shaded || (shade.textured && !shade.raw)) {
            DITHER[(y & 3) as usize][(x & 3) as usize]
        } else {
            0
        };
        let (mut colour, mut blend, mut mask) = ([0i32; 3], shade.semi, false);
        if shade.textured {
            let texel = self.texel(uv, shade.clut);
            if texel == 0 {
                return;
            }
            mask = texel & 0x8000 != 0;
            blend = shade.semi && mask;
            for (c, out) in colour.iter_mut().enumerate() {
                let five = i32::from((texel >> (5 * c)) & 0x1f);
                // The primitive's colour scales the texel's, 0x80 leaving it as it is.
                let eight = if shade.raw { five << 3 } else { (five * rgb[c]) >> 4 };
                *out = (eight + dither).clamp(0, 255) >> 3;
            }
        } else {
            for (c, out) in colour.iter_mut().enumerate() {
                *out = (rgb[c] + dither).clamp(0, 255) >> 3;
            }
        }
        if blend {
            for (c, out) in colour.iter_mut().enumerate() {
                let back = i32::from((behind >> (5 * c)) & 0x1f);
                *out = match (self.mode >> 5) & 3 {
                    0 => (back + *out) / 2,
                    1 => back + *out,
                    2 => back - *out,
                    _ => back + *out / 4,
                }
                .clamp(0, 31);
            }
        }
        let pixel = (colour[0] | (colour[1] << 5) | (colour[2] << 10)) as u16;
        self.vram[at] = pixel | (u16::from(mask || self.set_mask) << 15);
    }

    /// The whole video memory as a picture, 1024 by 512: what a person looks at to see where a
    /// program keeps its textures and what it drew off the screen.
    pub fn video_memory(&self) -> Frame {
        let wide = |five: u16| u32::from((five << 3) | (five >> 2));
        let pixels = self.vram.iter().map(|p| wide(p & 0x1f) << 16 | wide((p >> 5) & 0x1f) << 8 | wide((p >> 10) & 0x1f)).collect();
        Frame { width: VRAM_WIDTH as u32, height: VRAM_HEIGHT as u32, pixels }
    }

    /// The size of the picture the display mode shows.
    pub fn resolution(&self) -> (u32, u32) {
        let width = if self.display_mode & 0x40 != 0 {
            368
        } else {
            match self.display_mode & 3 {
                0 => 256,
                1 => 320,
                2 => 512,
                _ => 640,
            }
        };
        let lines = self.vertical.1.saturating_sub(self.vertical.0);
        let lines = if lines == 0 || lines > 288 { 240 } else { lines };
        (width, if self.display_mode & 0x24 == 0x24 { lines * 2 } else { lines })
    }

    /// The picture the television shows: the part of the video memory the display starts at, in
    /// fifteen or twenty-four bits; black while the display is off.
    pub fn frame(&self) -> Frame {
        let (width, height) = self.resolution();
        let mut frame = Frame::new(width, height);
        if self.display_off {
            return frame;
        }
        let (x0, y0) = (self.display_start.0 as usize, self.display_start.1 as usize);
        let wide = |five: u16| u32::from((five << 3) | (five >> 2));
        for y in 0..height as usize {
            let row = ((y0 + y) & 0x1ff) * VRAM_WIDTH;
            for x in 0..width as usize {
                frame.pixels[y * width as usize + x] = if self.display_mode & 0x10 != 0 {
                    // Three bytes to a pixel, laid over the sixteen-bit words.
                    let byte = |n: usize| {
                        let word = self.vram[row + ((x0 + n / 2) & 0x3ff)];
                        u32::from(if n & 1 == 0 { word & 0xff } else { word >> 8 })
                    };
                    byte(x * 3) << 16 | byte(x * 3 + 1) << 8 | byte(x * 3 + 2)
                } else {
                    let pixel = self.vram[row + ((x0 + x) & 0x3ff)];
                    wide(pixel & 0x1f) << 16 | wide((pixel >> 5) & 0x1f) << 8 | wide((pixel >> 10) & 0x1f)
                };
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GPU with the display on at 320 by 240 and the whole of that as its drawing area.
    fn gpu() -> Gpu {
        let mut gpu = Gpu::new();
        for word in [0x0300_0000, 0x0800_0001, 0x0500_0000] {
            gpu.gp1(word);
        }
        gpu.gp0(0xe300_0000);
        gpu.gp0(0xe400_0000 | (239 << 10) | 319);
        gpu
    }

    fn at(gpu: &Gpu, x: usize, y: usize) -> u16 {
        gpu.vram[y * VRAM_WIDTH + x]
    }

    #[test]
    fn a_fill_ignores_the_drawing_area_and_rounds_to_sixteen() {
        let mut gpu = gpu();
        for word in [0x02ff_0000, 0x0004_0008, 0x0002_0011] {
            gpu.gp0(word);
        }
        // Blue, from x 0 (8 rounds down) for 32 pixels (17 rounds up), two rows from y 4.
        assert_eq!(at(&gpu, 0, 4), 0x7c00);
        assert_eq!(at(&gpu, 31, 5), 0x7c00);
        assert_eq!((at(&gpu, 32, 4), at(&gpu, 0, 6), at(&gpu, 0, 3)), (0, 0, 0));
    }

    #[test]
    fn a_flat_triangle_fills_inside_the_drawing_area_with_the_offset() {
        let mut gpu = gpu();
        gpu.gp0(0xe500_0000 | (10 << 11) | 20);
        for word in [0x2000_00ff, 0x0000_0000, 0x0000_0008, 0x0008_0000] {
            gpu.gp0(word);
        }
        assert_eq!(at(&gpu, 20, 10), 0x001f, "the first vertex, moved by the offset");
        assert_eq!(at(&gpu, 26, 10), 0x001f);
        assert_eq!(at(&gpu, 28, 10), 0, "the right end is outside");
        assert_eq!(at(&gpu, 20, 18), 0, "the bottom end is outside");
        assert_eq!(at(&gpu, 19, 10), 0);
    }

    #[test]
    fn a_shaded_triangle_varies_between_its_corners() {
        let mut gpu = gpu();
        for word in [0x3000_00ff, 0x0000_0000, 0x0000_ff00, 0x0000_0040, 0x00ff_0000, 0x0040_0000] {
            gpu.gp0(word);
        }
        assert_eq!(at(&gpu, 0, 0), 0x001f, "red at the first corner");
        let middle = at(&gpu, 32, 0);
        assert_eq!((middle & 0x1f, (middle >> 5) & 0x1f), (15, 15), "half red, half green along the top");
    }

    #[test]
    fn a_copy_to_the_video_memory_then_back_gives_the_words() {
        let mut gpu = gpu();
        for word in [0xa000_0000, 0x0010_0020, 0x0002_0002, 0x2222_1111, 0x4444_3333] {
            gpu.gp0(word);
        }
        assert_eq!((at(&gpu, 32, 16), at(&gpu, 33, 16), at(&gpu, 32, 17), at(&gpu, 33, 17)), (0x1111, 0x2222, 0x3333, 0x4444));
        assert_eq!(gpu.status(0, 0) & (1 << 27), 0);
        for word in [0xc000_0000, 0x0010_0020, 0x0002_0002] {
            gpu.gp0(word);
        }
        assert_ne!(gpu.status(0, 0) & (1 << 27), 0, "ready to send");
        assert_eq!((gpu.read(), gpu.read()), (0x2222_1111, 0x4444_3333));
        assert_eq!(gpu.status(0, 0) & (1 << 27), 0);
    }

    #[test]
    fn a_textured_rectangle_reads_its_palette_and_skips_the_clear_texels() {
        let mut gpu = gpu();
        // A 4-bit texture at page (64, 0): the texels 1, 0, 2, 1 in one word; its palette at
        // (0, 480): index 1 is red with the mask bit, index 2 green.
        for word in [0xa000_0000, 0x0000_0040, 0x0001_0001, 0x0000_1201] {
            gpu.gp0(word);
        }
        for word in [0xa000_0000, 0x01e0_0000, 0x0001_0003, 0x801f_0000, 0x0000_03e0] {
            gpu.gp0(word);
        }
        gpu.gp0(0xe100_0001);
        let clut = (480 << 6) as u32;
        for word in [0x6580_8080, 0x0005_0005, clut << 16, 0x0001_0004] {
            gpu.gp0(word);
        }
        assert_eq!(at(&gpu, 5, 5), 0x801f);
        assert_eq!(at(&gpu, 6, 5), 0, "texel 0 is clear");
        assert_eq!(at(&gpu, 7, 5), 0x03e0);
        assert_eq!(at(&gpu, 8, 5), 0x801f);
    }

    #[test]
    fn half_transparency_averages_with_what_is_behind() {
        let mut gpu = gpu();
        for word in [0x0200_00ff, 0x0000_0000, 0x0010_0010] {
            gpu.gp0(word);
        }
        for word in [0x62ff_0000, 0x0000_0000, 0x0001_0001] {
            gpu.gp0(word);
        }
        assert_eq!(at(&gpu, 0, 0), (15 << 10) | 15, "half blue over half red");
        assert_eq!(at(&gpu, 1, 0), 0x001f);
    }

    #[test]
    fn a_polyline_runs_to_its_terminator() {
        let mut gpu = gpu();
        for word in [0x4800_ff00, 0x0000_0000, 0x0000_0003, 0x0003_0003, 0x5555_5555] {
            gpu.gp0(word);
        }
        assert_eq!((at(&gpu, 0, 0), at(&gpu, 3, 0), at(&gpu, 3, 3)), (0x03e0, 0x03e0, 0x03e0));
        // The next word is a command again.
        gpu.gp0(0xe100_0000);
        assert!(gpu.fifo.is_empty());
    }

    #[test]
    fn the_frame_is_the_displayed_part_and_black_while_off() {
        let mut gpu = gpu();
        for word in [0x02ff_ffff, 0x0000_0000, 0x0001_0010] {
            gpu.gp0(word);
        }
        let frame = gpu.frame();
        assert_eq!((frame.width, frame.height), (320, 240));
        assert_eq!((frame.pixels[0], frame.pixels[16], frame.pixels[320]), (0x00ff_ffff, 0, 0));
        gpu.gp1(0x0300_0001);
        assert_eq!(gpu.frame().pixels[0], 0);
    }
}
