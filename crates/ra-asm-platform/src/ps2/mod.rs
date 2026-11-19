//! The PlayStation 2, the Emotion Engine's side: the devices behind its registers (the timers,
//! the GIF and the DMA channel that feeds it, the interrupt controller, the serial port, and the
//! graphics synthesizer with its privileged registers) and the EE kernel's functions, over the
//! host.
//!
//! What is not here yet is the other processor: the IOP and its modules, which the pads, the
//! sound and the files of a program go through, by the SIF. The SIF's kernel calls answer that
//! nothing is in flight and its registers keep what is written, so a program that waits for the
//! IOP waits for ever; the vector units' memories and the VIFs keep what is written too.

pub mod gs;
mod kernel;

use std::collections::BTreeMap;

use crate::clock::Scheduler;
use crate::host::Host;
use crate::{Answer, Block, Delivery, Memory, MemoryExt, Platform, Stop};

/// Main RAM: 32 MiB from address zero, mirrored uncached at `0x20000000` and `0x30000000`.
pub const RAM_SIZE: u64 = 32 << 20;
/// The scratchpad, 16 KiB.
pub const SCRATCHPAD: u64 = 0x7000_0000;
/// The EE's registers, and the graphics synthesizer's privileged ones.
pub const HARDWARE: u64 = 0x1000_0000;
pub const GS: u64 = 0x1200_0000;

/// The processor's cycles in a second, and in a frame of each television system.
pub const CLOCK: u64 = 294_912_000;
const NTSC: u64 = CLOCK * 1001 / 60_000;
const PAL: u64 = CLOCK / 50;

/// The interrupt controller's lines.
pub mod intc {
    pub const GS: u32 = 1 << 0;
    pub const VBLANK_START: u32 = 1 << 2;
    pub const VBLANK_END: u32 = 1 << 3;
    pub const TIMER0: u32 = 1 << 9;
}

/// The devices by their registers.
pub const BLOCKS: [Block; 14] = [
    Block::new("timer", 0x1000_0000, 0x2000),
    Block::new("ipu", 0x1000_2000, 0x1000),
    Block::new("gif", 0x1000_3000, 0x800),
    Block::new("vif0", 0x1000_3800, 0x400),
    Block::new("vif1", 0x1000_3c00, 0x400),
    Block::new("fifo", 0x1000_4000, 0x4000),
    Block::new("dma", 0x1000_8000, 0x7000),
    Block::new("intc", 0x1000_f000, 0x100),
    Block::new("sio", 0x1000_f100, 0x100),
    Block::new("sif", 0x1000_f200, 0x200),
    Block::new("hw", 0x1000_f400, 0x100),
    Block::new("dma", 0x1000_f500, 0xb00),
    Block::new("vu", 0x1100_0000, 0x1_0000),
    Block::new("gs", GS, 0x2000),
];

/// The serial port's transmit FIFO and its status, whose transmit side reads as empty.
const SIO_TXFIFO: u64 = 0x1000_f180;
/// The interrupt controller's status and mask.
const I_STAT: u64 = 0x1000_f000;
const I_MASK: u64 = 0x1000_f010;
/// The DMA controller's status (a channel's interrupt, and its mask in the upper half) and the
/// GIF's channel, the third.
const D_STAT: u64 = 0x1000_e010;
const D_ENABLER: u64 = 0x1000_f520;
const D_ENABLEW: u64 = 0x1000_f590;
const GIF_CHANNEL: u64 = 0x1000_a000;
const GIF_STAT: u64 = 0x1000_3020;
const GIF_FIFO: u64 = 0x1000_6000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    VBlankStart,
    VBlankEnd,
    Alarm(u32),
}

#[derive(Debug, Clone, Copy, Default)]
struct Timer {
    count: u32,
    mode: u32,
    compare: u32,
    hold: u32,
    fraction: u64,
}

pub struct Ps2 {
    host: Host,
    clock: Scheduler<Event>,
    pub gs: gs::Gs,
    i_stat: u32,
    i_mask: u32,
    d_stat: u32,
    timers: [Timer; 4],
    /// The registers that only keep what is written, by address.
    kept: BTreeMap<u64, u64>,
    /// The half of a 128-bit write to the GIF's FIFO that came first.
    fifo_low: Option<u64>,
    frames: u64,
    kernel: kernel::Kernel,
    closed: bool,
}

impl Ps2 {
    pub fn new(host: Host) -> Self {
        let mut clock = Scheduler::default();
        clock.after(NTSC, Event::VBlankStart);
        Self {
            host,
            clock,
            gs: gs::Gs::new(),
            i_stat: 0,
            i_mask: 0,
            d_stat: 0,
            timers: [Timer::default(); 4],
            kept: BTreeMap::new(),
            fifo_low: None,
            frames: 0,
            kernel: kernel::Kernel::default(),
            closed: false,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn frame_cycles(&self) -> u64 {
        // The video mode `SetGsCrt` was given: 3 is PAL.
        if self.kernel.video_mode == 3 { PAL } else { NTSC }
    }

    fn raise(&mut self, lines: u32) {
        self.i_stat |= lines;
    }

    /// The program's address of a DMA address: RAM, or the scratchpad when the top bit is set.
    fn dma_address(address: u32) -> u64 {
        if address & 0x8000_0000 != 0 { SCRATCHPAD | u64::from(address & 0x3ff0) } else { u64::from(address & 0x01ff_fff0) }
    }

    /// Runs the GIF's channel to its end: `QWC` quadwords from `MADR`, or the chain of tags from
    /// `TADR`, each tag saying how many quadwords and where the next tag is.
    fn gif_dma(&mut self, memory: &mut dyn Memory) {
        let register = |ps2: &Self, offset: u64| ps2.kept.get(&(GIF_CHANNEL + offset)).copied().unwrap_or(0) as u32;
        let control = register(self, 0);
        let send = |ps2: &mut Self, memory: &mut dyn Memory, address: u32, quadwords: u32| {
            for i in 0..u64::from(quadwords) {
                let at = Self::dma_address(address) + 16 * i;
                let (low, high) = (memory.u64(at), memory.u64(at + 8));
                ps2.gs.gif(low, high);
            }
        };
        if (control >> 2) & 3 == 0 {
            let (address, quadwords) = (register(self, 0x10), register(self, 0x20) & 0xffff);
            send(self, memory, address, quadwords);
        } else {
            let mut tag_at = register(self, 0x30);
            let mut stack = Vec::new();
            // A chain that loops would never end: none is longer than the memory.
            for _ in 0..0x10_0000 {
                let tag = memory.u64(Self::dma_address(tag_at));
                let (quadwords, id, address) = ((tag & 0xffff) as u32, (tag >> 28) & 7, (tag >> 32) as u32);
                let after_tag = tag_at.wrapping_add(16);
                let after_data = after_tag.wrapping_add(16 * quadwords);
                let (data, next) = match id {
                    // refe, ref, refs: the data is elsewhere; the next tag follows this one.
                    0 | 3 | 4 => (address, Some(after_tag).filter(|_| id != 0)),
                    // cnt: the data follows, and the next tag follows the data.
                    1 => (after_tag, Some(after_data)),
                    // next: the data follows, the next tag is where the tag says.
                    2 => (after_tag, Some(address)),
                    // call and ret: a tag elsewhere, and back to after the call's data.
                    5 => {
                        stack.push(after_data);
                        (after_tag, Some(address))
                    }
                    6 => (after_tag, stack.pop()),
                    _ => (after_tag, None),
                };
                send(self, memory, data, quadwords);
                match next {
                    // A tag with its interrupt bit ends the chain when the channel takes it.
                    Some(next) if !(tag & (1 << 31) != 0 && control & (1 << 7) != 0) => tag_at = next,
                    _ => break,
                }
            }
        }
        // The transfer is over: the channel stops, its count is zero, its interrupt is raised.
        self.kept.insert(GIF_CHANNEL, u64::from(control & !0x100));
        self.kept.insert(GIF_CHANNEL + 0x20, 0);
        self.d_stat |= 1 << 2;
        if self.gs.interrupt() {
            self.raise(intc::GS);
        }
    }

    fn timers(&mut self, cycles: u64) {
        for n in 0..4 {
            let timer = &mut self.timers[n];
            if timer.mode & 0x80 == 0 {
                continue;
            }
            // The bus clock is half the processor's; then as it is, a 16th, a 256th or a scanline.
            let divisor = 2 * match timer.mode & 3 {
                0 => 1,
                1 => 16,
                2 => 256,
                _ => 9_370,
            };
            timer.fraction += cycles;
            let mut ticks = timer.fraction / divisor;
            timer.fraction %= divisor;
            let mut interrupt = false;
            while ticks > 0 {
                let to_compare = if timer.count < timer.compare { u64::from(timer.compare - timer.count) } else { u64::MAX };
                let step = ticks.min(to_compare).min(u64::from(0x1_0000 - timer.count));
                timer.count += step as u32;
                ticks -= step;
                if timer.count == timer.compare {
                    if timer.mode & 0x100 != 0 {
                        timer.mode |= 0x400;
                        interrupt = true;
                    }
                    if timer.mode & 0x40 != 0 {
                        timer.count = 0;
                    }
                }
                if timer.count == 0x1_0000 {
                    timer.count = 0;
                    if timer.mode & 0x200 != 0 {
                        timer.mode |= 0x800;
                        interrupt = true;
                    }
                }
            }
            if interrupt {
                self.raise(intc::TIMER0 << n);
            }
        }
    }
}

impl Platform for Ps2 {
    fn name(&self) -> &'static str {
        "ps2"
    }

    fn blocks(&self) -> &'static [Block] {
        &BLOCKS
    }

    fn fold(&self, address: u64) -> Option<u64> {
        // A 32-bit address a 64-bit register holds sign-extended is the same address.
        let address = address as u32;
        let physical = u64::from(if address >= 0x8000_0000 { address & 0x1fff_ffff } else { address });
        BLOCKS.iter().any(|b| b.holds(physical)).then_some(physical)
    }

    fn read(&mut self, _memory: &mut dyn Memory, register: u64, bits: u32) -> u64 {
        let aligned = register & !0xf;
        let value = match aligned {
            GS..=0x1200_1fff => self.gs.read(aligned - GS),
            I_STAT => u64::from(self.i_stat),
            I_MASK => u64::from(self.i_mask),
            D_STAT => u64::from(self.d_stat),
            // The GIF: its FIFO empty, no path active.
            GIF_STAT => 0,
            // The DMA enable register reads through its write side.
            D_ENABLER => self.kept.get(&D_ENABLEW).copied().unwrap_or(0x1201),
            0x1000_0000..=0x1000_1fff => {
                let timer = &self.timers[((aligned >> 11) & 3) as usize];
                u64::from(match aligned & 0x70 {
                    0x00 => timer.count,
                    0x10 => timer.mode,
                    0x20 => timer.compare,
                    _ => timer.hold,
                })
            }
            _ => self.kept.get(&aligned).copied().unwrap_or(0),
        };
        let value = value >> (8 * (register & 0xf).min(7));
        if bits >= 64 { value } else { value & ((1 << bits) - 1) }
    }

    fn write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u64) {
        let aligned = register & !0xf;
        match aligned {
            GS..=0x1200_1fff => {
                // A register is 64 bits; a narrower write to its upper half is that half.
                let shift = 8 * (register & 0xf).min(7);
                let merged =
                    if bits >= 64 { value } else { (self.gs.read(aligned - GS) & !(((1u64 << bits) - 1) << shift)) | value << shift };
                // The status register takes what is written, not what it holds with it.
                self.gs.write(aligned - GS, if aligned - GS == gs::CSR { value << shift } else { merged });
            }
            SIO_TXFIFO => self.host.console.write(&[value as u8]),
            // A one acknowledges an interrupt; a one flips a line's mask.
            I_STAT => self.i_stat &= !(value as u32),
            I_MASK => self.i_mask ^= value as u32 & 0x7fff,
            D_STAT => {
                let value = value as u32;
                self.d_stat = (self.d_stat & !(value & 0xffff)) ^ (value & 0x03ff_0000);
            }
            GIF_FIFO => {
                // The FIFO takes a quadword, which a program writes as two halves.
                match (register & 8 != 0, self.fifo_low.take()) {
                    (true, Some(low)) => self.gs.gif(low, value),
                    (true, None) => self.gs.gif(0, value),
                    (false, _) => self.fifo_low = Some(value),
                }
            }
            0x1000_0000..=0x1000_1fff => {
                let timer = &mut self.timers[((aligned >> 11) & 3) as usize];
                let value = value as u32;
                match aligned & 0x70 {
                    0x00 => timer.count = value & 0xffff,
                    // The two flags clear where a one is written.
                    0x10 => timer.mode = (value & 0x3ff) | (timer.mode & 0xc00 & !value),
                    0x20 => timer.compare = value & 0xffff,
                    _ => timer.hold = value & 0xffff,
                }
            }
            _ => {
                self.kept.insert(aligned, value);
                // The GIF's channel starts when its control register is written with the start
                // bit, the controller enabled.
                if aligned == GIF_CHANNEL && value & 0x100 != 0 {
                    self.gif_dma(memory);
                }
            }
        }
    }

    fn call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop> {
        self.kernel_call(memory, name, args)
    }

    fn resume(&mut self, _memory: &mut dyn Memory) -> Option<u64> {
        self.kernel_resume()
    }

    fn advance(&mut self, memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop> {
        if self.closed || !self.host.video.idle() {
            return Err(Stop::Exit(0));
        }
        self.timers(cycles);
        let mut left = cycles;
        while let Some(event) = self.clock.step(&mut left) {
            match event {
                Event::VBlankStart => {
                    self.frames += 1;
                    self.gs.vblank();
                    self.raise(intc::VBLANK_START);
                    if self.gs.interrupt() {
                        self.raise(intc::GS);
                    }
                    self.kernel_vblank(memory);
                    if !self.host.video.present(&self.gs.frame()) {
                        self.closed = true;
                    }
                    // The blank lasts the lines the picture does not use, a twelfth of the frame.
                    self.clock.after(self.frame_cycles() / 12, Event::VBlankEnd);
                    self.clock.after(self.frame_cycles(), Event::VBlankStart);
                }
                Event::VBlankEnd => self.raise(intc::VBLANK_END),
                Event::Alarm(id) => self.kernel_alarm(id),
            }
        }
        if self.closed { Err(Stop::Exit(0)) } else { Ok(()) }
    }

    fn idle(&self) -> u64 {
        self.clock.until_next().unwrap_or(0)
    }

    fn deliver(&mut self, _memory: &mut dyn Memory) -> Option<Delivery> {
        self.kernel_deliver()
    }

    fn delivered(&mut self, _memory: &mut dyn Memory, delivery: &Delivery, result: u64) {
        self.kernel_delivered(delivery, result);
    }

    fn host(&mut self) -> &mut Host {
        &mut self.host
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;
    use crate::host::Capture;

    fn machine() -> (Ps2, Ram, Capture) {
        let capture = Capture::new();
        (Ps2::new(Host::headless(&capture)), Ram::new(0, 0x10_0000), capture)
    }

    #[test]
    fn the_registers_fold_from_their_mirrors_and_memory_does_not() {
        let (ps2, ..) = machine();
        assert_eq!(ps2.fold(0x1000_f180), Some(0x1000_f180));
        assert_eq!(ps2.fold(0xb000_f000), Some(0x1000_f000));
        assert_eq!(ps2.fold(0xffff_ffff_b200_1000), Some(0x1200_1000), "a sign-extended address");
        assert_eq!(ps2.fold(0x0010_0000), None);
        assert_eq!(ps2.fold(0x2010_0000), None, "the uncached mirror of RAM is memory");
        assert_eq!(ps2.fold(SCRATCHPAD), None);
    }

    #[test]
    fn the_serial_port_is_the_console() {
        let (mut ps2, mut ram, capture) = machine();
        for byte in b"hi\n" {
            assert_eq!(ps2.read(&mut ram, 0x1000_f130, 32) & 0x8000, 0, "the transmit FIFO has room");
            ps2.write(&mut ram, SIO_TXFIFO, 8, u64::from(*byte));
        }
        assert_eq!(capture.lock().console, b"hi\n");
    }

    #[test]
    fn the_gif_channel_sends_a_packet_from_memory_to_the_synthesizer() {
        let (mut ps2, mut ram, _) = machine();
        // The display, then a packet of A+D registers: the frame, the scissor, a red sprite.
        ps2.write(&mut ram, GS + gs::PMODE, 64, 1);
        ps2.write(&mut ram, GS + gs::DISPFB1, 64, 10 << 9);
        ps2.write(&mut ram, GS + gs::DISPLAY1, 64, (639 << 32) | (447 << 44));
        let registers: [(u64, u64); 6] = [
            (0x4c, 10 << 16),
            (0x40, (639 << 16) | (447 << 48)),
            (0x00, 6),
            (0x01, 0x8000_00ff),
            (0x05, 0),
            (0x05, (8 << 4) | (8 << 4) << 16),
        ];
        ram.set_u64(0x1000, registers.len() as u64 | 0x8000 | (1 << 60));
        ram.set_u64(0x1008, 0xe);
        for (i, (register, value)) in registers.into_iter().enumerate() {
            ram.set_u64(0x1010 + 16 * i as u64, value);
            ram.set_u64(0x1018 + 16 * i as u64, register);
        }
        ps2.write(&mut ram, GIF_CHANNEL + 0x10, 32, 0x1000);
        ps2.write(&mut ram, GIF_CHANNEL + 0x20, 32, 7);
        ps2.write(&mut ram, GIF_CHANNEL, 32, 0x101);
        assert_eq!(ps2.read(&mut ram, GIF_CHANNEL, 32) & 0x100, 0, "the channel stopped");
        assert_ne!(ps2.read(&mut ram, D_STAT, 32) & 4, 0, "its interrupt is pending");
        let frame = ps2.gs.frame();
        assert_eq!((frame.pixels[0], frame.pixels[7 * 640 + 7], frame.pixels[8]), (0x00ff_0000, 0x00ff_0000, 0));
    }

    #[test]
    fn a_chain_follows_its_tags_to_the_end() {
        let (mut ps2, mut ram, _) = machine();
        // cnt with one quadword (a tag of no loops), then end with none.
        ram.set_u64(0x2000, 1 | 1 << 28);
        ram.set_u64(0x2010, 0x8000);
        ram.set_u64(0x2020, 7 << 28);
        ps2.write(&mut ram, GIF_CHANNEL + 0x30, 32, 0x2000);
        ps2.write(&mut ram, GIF_CHANNEL, 32, 0x105);
        assert_eq!(ps2.read(&mut ram, GIF_CHANNEL, 32) & 0x100, 0);
        assert!(ps2.gs.idle());
    }

    #[test]
    fn a_frame_of_cycles_presents_the_picture_and_sets_the_blank() {
        let (mut ps2, mut ram, capture) = machine();
        ps2.advance(&mut ram, NTSC - 1).unwrap();
        assert_eq!(capture.lock().frames, 0);
        ps2.advance(&mut ram, 1).unwrap();
        assert_eq!(capture.lock().frames, 1);
        assert_ne!(ps2.read(&mut ram, GS + gs::CSR, 64) & gs::CSR_VSINT, 0);
        assert_ne!(ps2.read(&mut ram, I_STAT, 32) & u64::from(intc::VBLANK_START), 0);
        // The program acknowledges both by writing a one.
        ps2.write(&mut ram, GS + gs::CSR, 64, gs::CSR_VSINT);
        ps2.write(&mut ram, I_STAT, 32, u64::from(intc::VBLANK_START));
        assert_eq!(ps2.read(&mut ram, GS + gs::CSR, 64) & gs::CSR_VSINT, 0);
        assert_eq!(ps2.read(&mut ram, I_STAT, 32) & u64::from(intc::VBLANK_START), 0);
    }

    #[test]
    fn a_timer_counts_the_bus_clock_and_raises_at_its_compare() {
        let (mut ps2, mut ram, _) = machine();
        ps2.write(&mut ram, 0x1000_0820, 32, 50);
        ps2.write(&mut ram, 0x1000_0810, 32, 0x1c0);
        ps2.advance(&mut ram, 2 * 49).unwrap();
        assert_eq!(ps2.read(&mut ram, 0x1000_0800, 32), 49);
        ps2.advance(&mut ram, 2 * 2).unwrap();
        assert_eq!(ps2.read(&mut ram, 0x1000_0800, 32), 1, "the compare resets it");
        assert_ne!(ps2.read(&mut ram, I_STAT, 32) & u64::from(intc::TIMER0 << 1), 0);
    }
}
