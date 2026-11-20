//! The Dreamcast: the devices of Holly behind their registers (the system bus's interrupt
//! registers, the Maple bus with the controllers on it, the video output of the PowerVR) and
//! the SH-4's own (its timers, its serial port), over the host. The console's boot ROM leaves
//! a program no kernel but a few system calls for the disc, so nothing is called here by a name.
//!
//! The video output shows the frame buffer its registers name in the video memory, the Maple
//! bus answers a program's frames for the controller in each port, the timers count the
//! peripheral clock, and the serial port is the console's text.
//!
//! What is not here yet: the tile accelerator and the renderer (a list a program sends is not
//! drawn, and a render it starts ends at once), the AICA with its processor, whose registers
//! keep what is written, and the GD-ROM drive, which is empty. No carrier runs an SH-4 program
//! yet, so this console is tested at its registers.

use std::collections::BTreeMap;

use crate::clock::Scheduler;
use crate::host::{Frame, Host};
use crate::scanout::{Format, Scan};
use crate::{Answer, Block, Delivery, Extra, Map, Memory, MemoryExt, Order, Platform, Stop, pad};

/// Main RAM, 16 MiB, at its `P1` address, where a device's address of it lands.
pub const RAM: u64 = 0x8c00_0000;
/// The video memory, 8 MiB, at the `P2` address a program writes its frame buffer through.
pub const VRAM: u64 = 0xa500_0000;
pub const VRAM_SIZE: u64 = 0x80_0000;
/// The SH-4's cycles in a second, in a frame of the NTSC picture and in its blank; the
/// peripheral clock the timers count is a quarter of the processor's.
pub const CLOCK: u64 = 200_000_000;
const FRAME: u64 = CLOCK * 1001 / 60_000;
const BLANK: u64 = FRAME * 45 / 525;
const PERIPHERAL: u64 = 4;

/// The lines of the normal interrupts' status register.
pub mod intr {
    pub const RENDER_DONE: u32 = 0b111;
    pub const VBLANK_IN: u32 = 1 << 3;
    pub const VBLANK_OUT: u32 = 1 << 4;
    pub const MAPLE_DMA: u32 = 1 << 12;
}

/// Holly's part of the bus: the system bus's registers to the PowerVR's, and the AICA's with
/// the clock after them.
const HOLLY: u64 = 0x005f_6800;
const HOLLY_END: u64 = 0x005f_a000;
const AICA: u64 = 0x0070_0000;
const AICA_END: u64 = 0x0071_0010;
/// The SH-4's own: the interrupt controller, the timers, the serial port with its FIFO.
const INTC: u64 = 0xffd0_0000;
const TMU: u64 = 0xffd8_0000;
const SCIF: u64 = 0xffe8_0000;

const ISTNRM: u64 = 0x005f_6900;
const ISTEXT: u64 = 0x005f_6904;
const ISTERR: u64 = 0x005f_6908;
/// The masks of the three levels a line can interrupt at, each for the three status registers.
const MASKS: u64 = 0x005f_6910;
const MASKS_END: u64 = 0x005f_6940;
const MDSTAR: u64 = 0x005f_6c04;
const MDTSEL: u64 = 0x005f_6c10;
const MDEN: u64 = 0x005f_6c14;
const MDST: u64 = 0x005f_6c18;
const PVR_ID: u64 = 0x005f_8000;
const PVR_REVISION: u64 = 0x005f_8004;
const STARTRENDER: u64 = 0x005f_8014;
const FB_R_CTRL: u64 = 0x005f_8044;
const FB_R_SOF1: u64 = 0x005f_8050;
const FB_R_SOF2: u64 = 0x005f_8054;
const FB_R_SIZE: u64 = 0x005f_805c;
const SPG_VBLANK_INT: u64 = 0x005f_80cc;
const SPG_CONTROL: u64 = 0x005f_80d0;
const SPG_LOAD: u64 = 0x005f_80d8;
const SPG_STATUS: u64 = 0x005f_810c;
const RTC: u64 = 0x0071_0000;
const TSTR: u64 = 0xffd8_0004;
const TIMERS: u64 = 0xffd8_0008;
const TIMERS_END: u64 = 0xffd8_002c;
const SCFTDR2: u64 = 0xffe8_000c;
const SCFSR2: u64 = 0xffe8_0010;

/// The clock's seconds, counted from 1950, as the console starts: the ninth of September 1999.
const RTC_START: u32 = 0x5d75_9600;

/// The devices by their registers: Holly's and the AICA's at their physical addresses, the
/// SH-4's own where `P4` has them.
pub const BLOCKS: [Block; 11] = [
    Block::new("system-bus", HOLLY, 0x800),
    Block::new("gd-rom", 0x005f_7000, 0x100),
    Block::new("g1", 0x005f_7400, 0x100),
    Block::new("g2", 0x005f_7800, 0x100),
    Block::new("pvr-dma", 0x005f_7c00, 0x100),
    Block::new("pvr", 0x005f_8000, 0x2000),
    Block::new("aica", AICA, 0x8000),
    Block::new("rtc", RTC, 0x10),
    Block::new("intc", INTC, 0x10),
    Block::new("tmu", TMU, 0x30),
    Block::new("scif", SCIF, 0x28),
];

/// The controller's buttons, a clear bit for a pressed one in its condition.
pub mod button {
    pub const C: u16 = 1 << 0;
    pub const B: u16 = 1 << 1;
    pub const A: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const DOWN: u16 = 1 << 5;
    pub const LEFT: u16 = 1 << 6;
    pub const RIGHT: u16 = 1 << 7;
    pub const Z: u16 = 1 << 8;
    pub const Y: u16 = 1 << 9;
    pub const X: u16 = 1 << 10;
    pub const D: u16 = 1 << 11;
}

/// The Maple bus's commands and answers this console speaks.
mod maple {
    pub(super) const DEVICE_INFO: u32 = 1;
    pub(super) const GET_CONDITION: u32 = 9;
    pub(super) const INFO: u32 = 5;
    pub(super) const ACKNOWLEDGE: u32 = 7;
    pub(super) const DATA: u32 = 8;
    /// The function code of a controller, and what a standard one has: its buttons and axes.
    pub(super) const CONTROLLER: u32 = 0x0100_0000;
    pub(super) const STANDARD: u32 = 0xfe06_0f00;
    /// What the bus leaves where no device answered.
    pub(super) const NOTHING: u32 = 0xffff_ffff;
}

/// The two words of a controller's condition for what the host's pad holds: the buttons (the
/// cross is A, the circle B, the square X, the triangle Y), the two triggers from the second
/// shoulder buttons, and the sticks.
pub fn condition(pad: &pad::State) -> [u32; 2] {
    use button::{A, B, DOWN, LEFT, RIGHT, START, UP, X, Y};
    let pressed = [
        (pad::CROSS, A),
        (pad::CIRCLE, B),
        (pad::SQUARE, X),
        (pad::TRIANGLE, Y),
        (pad::START, START),
        (pad::UP, UP),
        (pad::DOWN, DOWN),
        (pad::LEFT, LEFT),
        (pad::RIGHT, RIGHT),
    ];
    let held = pressed.iter().filter(|(host, _)| pad.buttons & host != 0).fold(0u16, |all, (_, button)| all | button);
    let trigger = |button: u16| if pad.buttons & button != 0 { 0xff } else { 0u32 };
    let [right_x, right_y, left_x, left_y] = pad.sticks;
    [u32::from(!held) | (trigger(pad::R2) << 16) | (trigger(pad::L2) << 24), u32::from_le_bytes([left_x, left_y, right_x, right_y])]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    BlankIn,
    BlankOut,
}

/// A channel of the timer unit: it counts down from its constant at a division of the
/// peripheral clock.
#[derive(Debug, Clone, Copy, Default)]
struct Timer {
    constant: u32,
    count: u32,
    control: u32,
    fraction: u64,
}

pub struct Dreamcast {
    host: Host,
    clock: Scheduler<Event>,
    istnrm: u32,
    istext: u32,
    isterr: u32,
    masks: [u32; 12],
    timers: [Timer; 3],
    started: u32,
    /// The registers that only keep what is written.
    kept: BTreeMap<u64, u32>,
    frames: u64,
    frame_at: u64,
    closed: bool,
}

impl Dreamcast {
    pub fn new(host: Host) -> Self {
        let mut clock = Scheduler::default();
        clock.after(FRAME, Event::BlankIn);
        Self {
            host,
            clock,
            istnrm: 0,
            istext: 0,
            isterr: 0,
            masks: [0; 12],
            timers: [Timer::default(); 3],
            started: 0,
            kept: BTreeMap::new(),
            frames: 0,
            frame_at: 0,
            closed: false,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn raise(&mut self, lines: u32) {
        self.istnrm |= lines;
    }

    fn kept(&self, at: u64) -> u32 {
        self.kept.get(&at).copied().unwrap_or(0)
    }

    /// The address in RAM of an address a device was given, which is a physical one.
    fn ram(address: u32) -> u64 {
        RAM | u64::from(address & 0x00ff_ffff)
    }

    /// The beam's status: its line, counted on from the line the blank starts at, and whether
    /// the blank lasts.
    fn beam(&self) -> u32 {
        let lines = u64::from(match (self.kept(SPG_LOAD) >> 16) & 0x3ff {
            0 => 525,
            load => load + 1,
        });
        let since = self.clock.now().saturating_sub(self.frame_at).min(FRAME - 1);
        let line = (u64::from(self.kept(SPG_VBLANK_INT) & 0x3ff) + since * lines / FRAME) % lines;
        line as u32 | (u32::from(self.frames > 0 && since < BLANK) << 13)
    }

    fn register(&self, at: u64) -> u32 {
        match at {
            // The other two registers' lines show in the first as one bit each.
            ISTNRM => self.istnrm | (u32::from(self.istext != 0) << 30) | (u32::from(self.isterr != 0) << 31),
            ISTEXT => self.istext,
            ISTERR => self.isterr,
            MASKS..MASKS_END => self.masks[((at - MASKS) / 4) as usize],
            // A transfer on the Maple bus ends as it starts.
            MDST => 0,
            PVR_ID => 0x17fd_11db,
            PVR_REVISION => 0x11,
            SPG_STATUS => self.beam(),
            RTC => (RTC_START + (self.clock.now() / CLOCK) as u32) >> 16,
            0x0071_0004 => (RTC_START + (self.clock.now() / CLOCK) as u32) & 0xffff,
            TSTR => self.started,
            TIMERS..TIMERS_END => {
                let timer = &self.timers[((at - TIMERS) / 12) as usize];
                match (at - TIMERS) % 12 {
                    0 => timer.constant,
                    4 => timer.count,
                    _ => timer.control,
                }
            }
            // The transmit FIFO is empty and the line idle.
            SCFSR2 => 0x60,
            _ => self.kept(at),
        }
    }

    fn set_register(&mut self, memory: &mut dyn Memory, at: u64, value: u32) {
        match at {
            // A one acknowledges a line.
            ISTNRM => self.istnrm &= !value,
            ISTERR => self.isterr &= !value,
            ISTEXT => {}
            MASKS..MASKS_END => self.masks[((at - MASKS) / 4) as usize] = value,
            MDST => {
                if value & 1 != 0 && self.kept(MDEN) & 1 != 0 {
                    self.maple(memory);
                }
            }
            // No renderer: the render a program starts is over, with nothing drawn.
            STARTRENDER => self.raise(intr::RENDER_DONE),
            TSTR => self.started = value & 7,
            TIMERS..TIMERS_END => {
                let timer = &mut self.timers[((at - TIMERS) / 12) as usize];
                match (at - TIMERS) % 12 {
                    0 => timer.constant = value,
                    4 => timer.count = value,
                    // The underflow flag is cleared by a zero and never set by a write.
                    _ => timer.control = (value & 0x3f) | (timer.control & value & 0x100),
                }
            }
            SCFTDR2 => self.host.console.write(&[value as u8]),
            _ => {
                self.kept.insert(at, value);
            }
        }
    }

    /// Runs the frames at the Maple bus's command table to the last: each names a port, where
    /// the answer goes, and the frame to send, a word of the command, the two addresses and the
    /// count of the words that follow. The main unit of a port that holds a pad answers what it
    /// is and its condition; nothing else is on the bus.
    fn maple(&mut self, memory: &mut dyn Memory) {
        let mut at = Self::ram(self.kept(MDSTAR));
        // A table without its last frame would never end: none is longer than this.
        for _ in 0..256 {
            let (header, answer) = (memory.u32(at), Self::ram(memory.u32(at + 4)));
            let (last, port, words) = (header >> 31 != 0, ((header >> 16) & 3) as usize, u64::from(header & 0xff));
            // The patterns but the first send no frame: a reset, a light gun's trigger.
            if matches!((header >> 8) & 7, 0) {
                let frame = memory.u32(at + 8);
                let (command, to, from) = (frame & 0xff, (frame >> 8) & 0xff, (frame >> 16) & 0xff);
                let pad = if to & 0x3f == 0x20 { self.host.input.pad(port) } else { None };
                let data: Option<(u32, Vec<u32>)> = pad.map(|pad| match command {
                    maple::DEVICE_INFO => (maple::INFO, Self::device_info()),
                    maple::GET_CONDITION => {
                        let [buttons, sticks] = condition(&pad);
                        (maple::DATA, vec![maple::CONTROLLER, buttons, sticks])
                    }
                    _ => (maple::ACKNOWLEDGE, Vec::new()),
                });
                match data {
                    Some((response, data)) => {
                        // The answer comes from the unit asked, alone on its port, to the asker.
                        let sender = (to & 0xc0) | 0x20;
                        memory.set_u32(answer, response | (from << 8) | (sender << 16) | ((data.len() as u32) << 24));
                        for (i, word) in data.into_iter().enumerate() {
                            memory.set_u32(answer + 4 + 4 * i as u64, word);
                        }
                    }
                    None => memory.set_u32(answer, maple::NOTHING),
                }
                at += 12 + 4 * words;
            } else {
                at += 4;
            }
            if last {
                break;
            }
        }
        self.raise(intr::MAPLE_DMA);
    }

    /// What a controller says it is: its function and what of it it has, its region and
    /// connector, its name and its licence, each padded with spaces, and its power.
    fn device_info() -> Vec<u32> {
        let mut bytes = Vec::with_capacity(112);
        for word in [maple::CONTROLLER, maple::STANDARD, 0, 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&[0xff, 0]);
        bytes.extend(format!("{:<30}", "Dreamcast Controller").bytes());
        bytes.extend(format!("{:<60}", "Produced By or Under License From SEGA ENTERPRISES,LTD.").bytes());
        for power in [0x01aeu16, 0x01f4] {
            bytes.extend_from_slice(&power.to_le_bytes());
        }
        bytes.chunks_exact(4).map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]])).collect()
    }

    /// The timers that run count the cycles down; one that passes zero starts again from its
    /// constant with its underflow flag set.
    fn timers(&mut self, cycles: u64) {
        for (n, timer) in self.timers.iter_mut().enumerate() {
            if self.started & (1 << n) == 0 {
                continue;
            }
            let divisor = PERIPHERAL * [4, 16, 64, 256, 1024][(timer.control & 7).min(4) as usize];
            timer.fraction += cycles;
            let ticks = timer.fraction / divisor;
            timer.fraction %= divisor;
            if ticks <= u64::from(timer.count) {
                timer.count -= ticks as u32;
            } else {
                let period = u64::from(timer.constant) + 1;
                timer.count = timer.constant - ((ticks - u64::from(timer.count) - 1) % period) as u32;
                timer.control |= 0x100;
            }
        }
    }

    /// The picture the video output shows: the frame buffer the read registers name, the two
    /// fields' rows by turns when the output is interlaced; black while it shows none.
    fn picture(&self, memory: &mut dyn Memory) -> Frame {
        let (control, size) = (self.kept(FB_R_CTRL), self.kept(FB_R_SIZE));
        if control & 1 == 0 {
            return Frame::new(640, 480);
        }
        let format = [Format::RGB555, Format::RGB565, Format::RGB888, Format::XRGB8888][((control >> 2) & 3) as usize];
        let (row, lines, skip) = (((size & 0x3ff) + 1) * 4, ((size >> 10) & 0x3ff) + 1, ((size >> 20) & 0x3ff).saturating_sub(1) * 4);
        let field = |start: u64| Scan {
            address: VRAM | (u64::from(self.kept(start)) & (VRAM_SIZE - 1)),
            width: row / format.bytes,
            height: lines,
            stride: row + skip,
            format,
            order: Order::Little,
        };
        if self.kept(SPG_CONTROL) & 0x10 == 0 {
            return field(FB_R_SOF1).frame(memory);
        }
        let mut frame = Frame::new(row / format.bytes, 2 * lines);
        field(FB_R_SOF1).rows(memory, &mut frame, 0, 2);
        field(FB_R_SOF2).rows(memory, &mut frame, 1, 2);
        frame
    }
}

impl Platform for Dreamcast {
    fn name(&self) -> &'static str {
        "dreamcast"
    }

    fn blocks(&self) -> &'static [Block] {
        &BLOCKS
    }

    fn map(&self) -> Map {
        // Holly and the AICA through the uncached segment and at their own addresses, the
        // SH-4's own where they are; the RAM through the uncached segment and the user's.
        const PORTS: [Block; 7] = [
            Block::new("holly", 0xa000_0000 | HOLLY, HOLLY_END - HOLLY),
            Block::new("holly", HOLLY, HOLLY_END - HOLLY),
            Block::new("aica", 0xa000_0000 | AICA, AICA_END - AICA),
            Block::new("aica", AICA, AICA_END - AICA),
            Block::new("intc", INTC, 0x10),
            Block::new("tmu", TMU, 0x30),
            Block::new("scif", SCIF, 0x28),
        ];
        const VIDEO: [Extra; 1] = [Extra { name: "vram", start: VRAM, len: VRAM_SIZE, mirrors: &[0x0500_0000, 0x8500_0000] }];
        Map { order: Order::Little, ports: &PORTS, ram: &[0x0000_0000, 0xa000_0000], repeats: 1, extras: &VIDEO }
    }

    fn fold(&self, address: u64) -> Option<u64> {
        // A 32-bit address a 64-bit register holds sign-extended is the same address.
        let address = u64::from(address as u32);
        if address >= 0xff00_0000 {
            return [(INTC, 0x10), (TMU, 0x30), (SCIF, 0x28)]
                .iter()
                .any(|(start, len)| (*start..start + len).contains(&address))
                .then_some(address);
        }
        let physical = address & 0x1fff_ffff;
        ((HOLLY..HOLLY_END).contains(&physical) || (AICA..AICA_END).contains(&physical)).then_some(physical)
    }

    fn read(&mut self, _memory: &mut dyn Memory, register: u64, bits: u32) -> u64 {
        let value = u64::from(self.register(register & !3)) >> (8 * (register & 3));
        if bits >= 32 { value } else { value & ((1 << bits) - 1) }
    }

    fn write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u64) {
        let (at, shift) = (register & !3, 8 * (register & 3) as u32);
        let written = (value as u32) << shift;
        // A narrow write changes its bytes of a register that keeps a value; one that takes
        // commands or acknowledgements takes the bytes written alone.
        let word = if bits >= 32 || matches!(at, ISTNRM | ISTERR | MDST | STARTRENDER | SCFTDR2) {
            written
        } else {
            let mask = ((1u32 << bits) - 1) << shift;
            (self.register(at) & !mask) | (written & mask)
        };
        self.set_register(memory, at, word);
    }

    fn call(&mut self, _memory: &mut dyn Memory, name: &str, _args: &[u64]) -> Result<Answer, Stop> {
        Err(Stop::fault(format!("the Dreamcast has no kernel to answer {name}")))
    }

    fn resume(&mut self, _memory: &mut dyn Memory) -> Option<u64> {
        None
    }

    fn advance(&mut self, memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop> {
        if self.closed || !self.host.video.idle() {
            return Err(Stop::Exit(0));
        }
        self.timers(cycles);
        let mut left = cycles;
        while let Some(event) = self.clock.step(&mut left) {
            match event {
                Event::BlankIn => {
                    self.frames += 1;
                    self.frame_at = self.clock.now();
                    self.raise(intr::VBLANK_IN);
                    let picture = self.picture(memory);
                    if !self.host.video.present(&picture) {
                        self.closed = true;
                    }
                    // A transfer the program set to start at the blank.
                    if self.kept(MDTSEL) & 1 != 0 && self.kept(MDEN) & 1 != 0 {
                        self.maple(memory);
                    }
                    self.clock.after(BLANK, Event::BlankOut);
                    self.clock.after(FRAME, Event::BlankIn);
                }
                Event::BlankOut => self.raise(intr::VBLANK_OUT),
            }
        }
        if self.closed { Err(Stop::Exit(0)) } else { Ok(()) }
    }

    fn idle(&self) -> u64 {
        self.clock.until_next().unwrap_or(0)
    }

    fn interrupt(&self) -> bool {
        let status = [self.register(ISTNRM), self.istext, self.isterr];
        let holly = self.masks.chunks_exact(4).any(|level| status.iter().zip(level).any(|(lines, mask)| lines & mask != 0));
        // A timer interrupts at its underflow when the program asked it to.
        holly || self.timers.iter().any(|timer| timer.control & 0x120 == 0x120)
    }

    fn deliver(&mut self, _memory: &mut dyn Memory) -> Option<Delivery> {
        None
    }

    fn delivered(&mut self, _memory: &mut dyn Memory, _delivery: &Delivery, _result: u64) {}

    fn host(&mut self) -> &mut Host {
        &mut self.host
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;
    use crate::host::Capture;

    fn machine() -> (Dreamcast, Ram, Capture) {
        let capture = Capture::new();
        (Dreamcast::new(Host::headless(&capture)), Ram::new(RAM, 0x1_0000), capture)
    }

    #[test]
    fn the_registers_fold_from_their_segments_and_memory_does_not() {
        let (dreamcast, ..) = machine();
        assert_eq!(dreamcast.fold(0xa05f_8000), Some(PVR_ID));
        assert_eq!(dreamcast.fold(0x005f_6900), Some(ISTNRM));
        assert_eq!(dreamcast.fold(0xffff_ffff_ffe8_000c), Some(SCFTDR2), "a sign-extended address");
        assert_eq!(dreamcast.fold(0x8c01_0000), None);
        assert_eq!(dreamcast.fold(VRAM), None, "the video memory is memory");
        assert_eq!(dreamcast.fold(0xff00_001c), None, "the cache's control is no device");
    }

    #[test]
    fn the_serial_port_is_the_console() {
        let (mut dreamcast, mut ram, capture) = machine();
        for byte in b"hi\n" {
            assert_ne!(dreamcast.read(&mut ram, SCFSR2, 16) & 0x20, 0, "the transmit FIFO has room");
            dreamcast.write(&mut ram, SCFTDR2, 8, u64::from(*byte));
        }
        assert_eq!(capture.lock().console, b"hi\n");
    }

    #[test]
    fn a_frame_of_cycles_shows_the_frame_buffer_and_raises_the_blank() {
        let capture = Capture::new();
        let (mut dreamcast, mut vram) = (Dreamcast::new(Host::headless(&capture)), Ram::new(VRAM, 0x1000));
        // 640 by 480 at 565 from the video memory's start: the first pixel red.
        vram.set_u16(VRAM, 0xf800);
        dreamcast.write(&mut vram, FB_R_SIZE, 32, (1 << 20) + (479 << 10) + 319);
        dreamcast.write(&mut vram, FB_R_SOF1, 32, 0);
        dreamcast.write(&mut vram, FB_R_CTRL, 32, 0b0101);
        dreamcast.advance(&mut vram, FRAME).unwrap();
        let last = capture.lock().last.clone();
        assert_eq!((last.width, last.height, last.pixels[0], last.pixels[1]), (640, 480, 0x00ff_0000, 0));
        assert_ne!(dreamcast.read(&mut vram, ISTNRM, 32) & u64::from(intr::VBLANK_IN), 0);
        assert_ne!(dreamcast.read(&mut vram, SPG_STATUS, 32) & 0x2000, 0, "the beam is in the blank");
        assert!(!dreamcast.interrupt(), "no level's mask lets the line through");
        dreamcast.write(&mut vram, MASKS + 0x20, 32, u64::from(intr::VBLANK_IN));
        assert!(dreamcast.interrupt());
        dreamcast.write(&mut vram, ISTNRM, 32, u64::from(intr::VBLANK_IN));
        assert!(!dreamcast.interrupt(), "a one acknowledges it");
        dreamcast.advance(&mut vram, BLANK).unwrap();
        assert_ne!(dreamcast.read(&mut vram, ISTNRM, 32) & u64::from(intr::VBLANK_OUT), 0);
        assert_eq!(dreamcast.read(&mut vram, SPG_STATUS, 32) & 0x2000, 0);
    }

    #[test]
    fn the_maple_bus_answers_for_the_pad_in_a_port_and_for_no_other() {
        let pressed = pad::State { buttons: pad::CROSS | pad::START | pad::R2, ..pad::State::default() };
        let capture = Capture::new().press(0, 0, pressed);
        let (mut dreamcast, mut ram) = (Dreamcast::new(Host::headless(&capture)), Ram::new(RAM, 0x1_0000));
        // The first port's condition, then what the second port's main unit is, the last frame.
        let table =
            [1, 0x0c00_2000, 9 | (0x20 << 8) | (1 << 24), maple::CONTROLLER, 0x8001_0000, 0x0c00_3000, 1 | (0x60 << 8) | (0x40 << 16)];
        for (i, word) in table.into_iter().enumerate() {
            ram.set_u32(RAM + 0x1000 + 4 * i as u64, word);
        }
        dreamcast.write(&mut ram, MDSTAR, 32, 0x0c00_1000);
        dreamcast.write(&mut ram, MDEN, 32, 1);
        dreamcast.write(&mut ram, MDST, 32, 1);
        assert_eq!(ram.u32(RAM + 0x2000), 0x0320_0008, "data from the first port's main unit, three words");
        assert_eq!(ram.u32(RAM + 0x2004), maple::CONTROLLER);
        assert_eq!(ram.u32(RAM + 0x2008), 0x00ff_fff3, "A and start pressed, the right trigger down");
        assert_eq!(ram.u32(RAM + 0x200c), 0x8080_8080, "the sticks at rest");
        assert_eq!(ram.u32(RAM + 0x3000), maple::NOTHING, "nothing is in the second port");
        assert_eq!(dreamcast.read(&mut ram, MDST, 32), 0, "the transfer is over");
        assert_ne!(dreamcast.read(&mut ram, ISTNRM, 32) & u64::from(intr::MAPLE_DMA), 0);
    }

    #[test]
    fn a_controller_says_what_it_is() {
        let info = Dreamcast::device_info();
        assert_eq!((info.len(), info[0], info[1]), (28, maple::CONTROLLER, maple::STANDARD));
        let name: Vec<u8> = info[4..].iter().flat_map(|word| word.to_le_bytes()).skip(2).take(20).collect();
        assert_eq!(name, b"Dreamcast Controller");
    }

    #[test]
    fn a_timer_counts_the_peripheral_clock_down_and_starts_again_at_zero() {
        let (mut dreamcast, mut ram, _) = machine();
        dreamcast.write(&mut ram, TIMERS, 32, 100);
        dreamcast.write(&mut ram, TIMERS + 4, 32, 100);
        // An interrupt at the underflow, the clock by four.
        dreamcast.write(&mut ram, TIMERS + 8, 16, 0x20);
        dreamcast.advance(&mut ram, 16 * 50).unwrap();
        assert_eq!(dreamcast.read(&mut ram, TIMERS + 4, 32), 100, "a timer not started stands");
        dreamcast.write(&mut ram, TSTR, 8, 1);
        dreamcast.advance(&mut ram, 16 * 50).unwrap();
        assert_eq!(dreamcast.read(&mut ram, TIMERS + 4, 32), 50);
        assert!(!dreamcast.interrupt());
        dreamcast.advance(&mut ram, 16 * 51).unwrap();
        assert_eq!(dreamcast.read(&mut ram, TIMERS + 4, 32), 100, "from its constant again");
        assert_ne!(dreamcast.read(&mut ram, TIMERS + 8, 16) & 0x100, 0);
        assert!(dreamcast.interrupt());
        dreamcast.write(&mut ram, TIMERS + 8, 16, 0x20);
        assert!(!dreamcast.interrupt(), "a zero clears the underflow");
    }

    #[test]
    fn the_clock_reads_the_seconds_in_two_halves() {
        let (mut dreamcast, mut ram, _) = machine();
        dreamcast.advance(&mut ram, 2 * CLOCK).unwrap();
        let seconds = (dreamcast.read(&mut ram, RTC, 32) << 16) | dreamcast.read(&mut ram, RTC + 4, 32);
        assert_eq!(seconds, u64::from(RTC_START) + 2);
    }
}
