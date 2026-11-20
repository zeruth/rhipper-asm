//! The PlayStation: the devices behind its I/O ports (the interrupt controller, the DMA
//! channels, the three timers, the controller port, the CD-ROM controller, the GPU and the SPU)
//! and its BIOS, over the host.
//!
//! The machine's time is the cycles of its 33.8688 MHz processor. A vertical blank comes once a
//! frame's worth of them: the picture is presented, the sound mixed since the last one is played,
//! and the interrupt is raised.

mod bios;
pub mod card;
mod cdrom;
pub mod gpu;
pub mod gte;
pub mod mdec;
pub mod spu;

use crate::clock::Scheduler;
use crate::host::Host;
use crate::{Answer, Block, Delivery, Memory, MemoryExt, Platform, Stop, pad};

/// Main RAM at its `kseg0` address, where a DMA address lands.
pub const RAM: u64 = 0x8000_0000;
/// The I/O ports, at their physical address.
pub const PORTS: u64 = 0x1f80_1000;
pub const PORTS_LEN: u64 = 0x2000;
/// The cache control register, which is in no mirror.
pub const CACHE_CONTROL: u64 = 0xfffe_0130;

/// The processor's cycles in a second, and in a frame and a scanline of each television system.
pub const CLOCK: u64 = 33_868_800;
const NTSC: (u64, u64) = (CLOCK / 60, 263);
const PAL: (u64, u64) = (CLOCK / 50, 314);
/// The cycles from the last bit of a byte sent to a controller to the controller's acknowledge.
const JOY_ACK: u64 = 170;

/// The interrupt controller's lines.
pub mod irq {
    pub const VBLANK: u32 = 1 << 0;
    pub const GPU: u32 = 1 << 1;
    pub const CDROM: u32 = 1 << 2;
    pub const DMA: u32 = 1 << 3;
    pub const TIMER0: u32 = 1 << 4;
    pub const CONTROLLER: u32 = 1 << 7;
    pub const SPU: u32 = 1 << 9;
}

/// The devices by their ports.
pub const BLOCKS: [Block; 12] = [
    Block::new("memctl", 0x1f80_1000, 0x40),
    Block::new("joy", 0x1f80_1040, 0x10),
    Block::new("sio", 0x1f80_1050, 0x10),
    Block::new("memctl", 0x1f80_1060, 0x10),
    Block::new("irq", 0x1f80_1070, 0x10),
    Block::new("dma", 0x1f80_1080, 0x80),
    Block::new("timer", 0x1f80_1100, 0x30),
    Block::new("cdrom", 0x1f80_1800, 0x10),
    Block::new("gpu", 0x1f80_1810, 0x10),
    Block::new("mdec", 0x1f80_1820, 0x10),
    Block::new("spu", 0x1f80_1c00, 0x400),
    Block::new("exp2", 0x1f80_2000, 0x1000),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    VBlank,
    /// The byte sent to the controller port is out, and the device's answer is in.
    JoyByte,
    JoyAck,
}

#[derive(Debug, Clone, Copy, Default)]
struct Timer {
    value: u32,
    mode: u32,
    target: u32,
    /// The cycles counted toward the next tick of a divided clock.
    fraction: u64,
    /// A one-shot timer raised its interrupt.
    fired: bool,
}

/// What a transfer of the controller port speaks to: the first byte is its address.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Addressed {
    Pad,
    Card,
}

#[derive(Default)]
struct Joy {
    ports: [pad::Port; 2],
    slots: [card::Slot; 2],
    /// The device each port's transfer in flight speaks to.
    addressed: [Option<Addressed>; 2],
    /// The answer to the byte on the wire, and whether the device acknowledges it.
    incoming: Option<(u8, bool)>,
    received: Option<u8>,
    control: u16,
    mode: u16,
    baud: u16,
    interrupt: bool,
}

pub struct Psx {
    host: Host,
    clock: Scheduler<Event>,
    i_stat: u32,
    i_mask: u32,
    /// The interrupts raised and not yet offered to the program.
    fresh: u32,
    pub gpu: gpu::Gpu,
    pub spu: spu::Spu,
    pub mdec: mdec::Mdec,
    cdrom: cdrom::Cdrom,
    /// The DMA channels' three registers each, then the control and the interrupt registers.
    dma: [[u32; 3]; 7],
    dpcr: u32,
    dicr: u32,
    timers: [Timer; 3],
    joy: Joy,
    /// The image of the card in each slot, once the slot was spoken to; `None` in an empty one.
    cards: [Option<Option<Box<dyn crate::host::File>>>; 2],
    /// The memory control registers and the others that only keep what is written.
    kept: std::collections::BTreeMap<u64, u32>,
    frames: u64,
    /// The cycle of the last vertical blank.
    blank_at: u64,
    bios: bios::Bios,
    /// The person closed the picture: the run ends at its next step.
    closed: bool,
    /// The program ended itself through the debug port, with this code.
    ended: Option<i32>,
    /// The register reads a trace has said already, each with its value.
    traced: std::collections::HashSet<(u64, u32)>,
}

impl Psx {
    pub fn new(host: Host) -> Self {
        let mut clock = Scheduler::default();
        clock.after(NTSC.0, Event::VBlank);
        Self {
            host,
            clock,
            i_stat: 0,
            i_mask: 0,
            fresh: 0,
            gpu: gpu::Gpu::new(),
            spu: spu::Spu::new(),
            mdec: mdec::Mdec::default(),
            cdrom: cdrom::Cdrom::default(),
            dma: [[0; 3]; 7],
            dpcr: 0x0765_4321,
            dicr: 0,
            timers: [Timer::default(); 3],
            joy: Joy::default(),
            cards: [None, None],
            kept: std::collections::BTreeMap::new(),
            frames: 0,
            blank_at: 0,
            bios: bios::Bios::default(),
            closed: false,
            ended: None,
            traced: std::collections::HashSet::new(),
        }
    }

    /// The frames presented so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn raise(&mut self, lines: u32) {
        self.i_stat |= lines;
        self.fresh |= lines;
    }

    fn system(&self) -> (u64, u64) {
        if self.gpu.status(0, 0) & (1 << 20) != 0 { PAL } else { NTSC }
    }

    /// The scanline the beam is on: the vertical blank starts line 240.
    fn line(&self) -> u32 {
        let (frame, lines) = self.system();
        (((self.clock.now() - self.blank_at) * lines / frame + 240) % lines) as u32
    }

    // region The timers

    fn timers(&mut self, cycles: u64) {
        let line = self.system().0 / self.system().1;
        for n in 0..3 {
            let timer = &mut self.timers[n];
            let source = (timer.mode >> 8) & 3;
            // The dot clock at 320 pixels, a scanline, an eighth of the system clock.
            let divisor = match (n, source) {
                (0, 1 | 3) => 5,
                (1, 1 | 3) => line,
                (2, 2 | 3) => 8,
                _ => 1,
            };
            // The third timer's two stopped synchronization modes.
            if n == 2 && timer.mode & 1 != 0 && matches!((timer.mode >> 1) & 3, 0 | 3) {
                continue;
            }
            timer.fraction += cycles;
            let mut ticks = timer.fraction / divisor;
            timer.fraction %= divisor;
            let mut interrupt = false;
            while ticks > 0 {
                let to_target = if timer.value < timer.target { u64::from(timer.target - timer.value) } else { u64::MAX };
                let step = ticks.min(to_target).min(u64::from(0x1_0000 - timer.value));
                timer.value += step as u32;
                ticks -= step;
                if timer.value == timer.target {
                    timer.mode |= 1 << 11;
                    interrupt |= timer.mode & 0x10 != 0;
                    if timer.mode & 0x08 != 0 {
                        timer.value = 0;
                    }
                }
                if timer.value == 0x1_0000 {
                    timer.mode |= 1 << 12;
                    interrupt |= timer.mode & 0x20 != 0;
                    timer.value = 0;
                }
            }
            if interrupt && (timer.mode & 0x40 != 0 || !timer.fired) {
                timer.fired = true;
                self.raise(irq::TIMER0 << n);
            }
        }
    }

    fn timer_read(&mut self, offset: u64) -> u32 {
        let timer = &mut self.timers[(offset >> 4) as usize % 3];
        match offset & 0xc {
            0 => timer.value,
            4 => {
                // The two reached flags clear when read.
                let mode = timer.mode;
                timer.mode &= !(3 << 11);
                mode
            }
            _ => timer.target,
        }
    }

    fn timer_write(&mut self, offset: u64, value: u32) {
        let timer = &mut self.timers[(offset >> 4) as usize % 3];
        match offset & 0xc {
            0 => timer.value = value & 0xffff,
            4 => {
                timer.mode = (value & 0x3ff) | (1 << 10);
                timer.value = 0;
                timer.fired = false;
            }
            _ => timer.target = value & 0xffff,
        }
    }

    // endregion

    // region The controller port

    fn joy_send(&mut self, byte: u8) {
        let port = usize::from(self.joy.control >> 13 & 1);
        // A byte takes eight bits of the baud rate's timer to go out and its answer to come in.
        let factor = [1, 1, 16, 64][usize::from(self.joy.mode & 3)];
        let reload = if self.joy.baud == 0 { 0x88 } else { u64::from(self.joy.baud) };
        let transfer = ((reload * factor) & !1).max(2) * 8;
        self.clock.cancel(|event| *event == Event::JoyByte);
        self.clock.after(transfer, Event::JoyByte);
        if self.joy.control & 0x0002 == 0 {
            self.joy.incoming = Some((0xff, false));
            return;
        }
        let addressed = *self.joy.addressed[port].get_or_insert(if byte == 0x81 { Addressed::Card } else { Addressed::Pad });
        let (answer, acknowledged) = match addressed {
            Addressed::Pad => {
                let state = self.host.input.pad(port);
                self.joy.ports[port].exchange(state, byte)
            }
            Addressed::Card => {
                // The card in the slot is a file of the host's, looked for when first spoken to.
                let storage = &mut self.host.storage;
                let image = self.cards[port].get_or_insert_with(|| storage.open(&format!("mc{port}:card{port}.mcd"), true).map(card::open));
                match image {
                    Some(file) => self.joy.slots[port].exchange(Some(file.as_mut()), byte),
                    None => self.joy.slots[port].exchange(None, byte),
                }
            }
        };
        if !acknowledged {
            self.joy.addressed[port] = None;
        }
        self.joy.incoming = Some((answer, acknowledged));
    }

    fn joy_control(&mut self, value: u16) {
        if value & 0x40 != 0 {
            // The port's registers reset; what a card remembers is the card's.
            let mut slots = std::mem::take(&mut self.joy.slots);
            slots.iter_mut().for_each(card::Slot::deselect);
            self.joy = Joy { slots, ..Joy::default() };
            self.clock.cancel(|event| matches!(event, Event::JoyByte | Event::JoyAck));
            return;
        }
        if value & 0x10 != 0 {
            self.joy.interrupt = false;
        }
        self.joy.control = value & !0x50;
        if value & 0x0002 == 0 {
            self.joy.ports.iter_mut().for_each(pad::Port::deselect);
            self.joy.slots.iter_mut().for_each(card::Slot::deselect);
            self.joy.addressed = [None; 2];
        }
    }

    // endregion

    // region DMA

    /// The interrupt register with its master flag, which follows the channels' flags.
    fn dicr(&self) -> u32 {
        let flags = (self.dicr >> 24) & 0x7f;
        let enabled = (self.dicr >> 16) & 0x7f;
        let master = self.dicr & (1 << 15) != 0 || (self.dicr & (1 << 23) != 0 && flags & enabled != 0);
        (self.dicr & 0x7fff_ffff) | u32::from(master) << 31
    }

    /// Runs the transfer of `channel` to its end, as the program started it.
    fn dma_run(&mut self, memory: &mut dyn Memory, channel: usize) {
        let [base, block, control] = self.dma[channel];
        let from_ram = control & 1 != 0;
        let step = if control & 2 != 0 { 4u32.wrapping_neg() } else { 4 };
        let words = match (control >> 9) & 3 {
            0 => {
                let n = block & 0xffff;
                if n == 0 { 0x1_0000 } else { n }
            }
            _ => (block & 0xffff) * (block >> 16).max(1),
        };
        let mut address = base;
        match channel {
            // The macroblock decoder's two: its commands and their data in, its pixels out.
            0 | 1 => {
                for _ in 0..words {
                    if channel == 0 {
                        self.mdec.write(memory.u32(Self::ram(address)));
                    } else {
                        memory.set_u32(Self::ram(address), self.mdec.read());
                    }
                    address = address.wrapping_add(step);
                }
            }
            // The GPU: a list of packets, each a header of its length and the next one's address,
            // or a block of words to or from it.
            2 if (control >> 9) & 3 == 2 => {
                // A list that loops would never end: none is longer than the memory.
                for _ in 0..0x8_0000 {
                    let header = memory.u32(Self::ram(address));
                    if crate::tracing() && std::env::var_os("RA_GPU_TRACE").is_some() {
                        eprintln!("dma: list at {address:08x}, header {header:08x}");
                    }
                    for i in 1..=(header >> 24) {
                        let word = memory.u32(Self::ram(address.wrapping_add(4 * i)));
                        self.gpu.gp0(word);
                    }
                    if header & 0x80_0000 != 0 {
                        break;
                    }
                    address = header & 0xff_ffff;
                }
            }
            2 => {
                for _ in 0..words {
                    if from_ram {
                        let word = memory.u32(Self::ram(address));
                        self.gpu.gp0(word);
                    } else {
                        let word = self.gpu.read();
                        memory.set_u32(Self::ram(address), word);
                    }
                    address = address.wrapping_add(step);
                }
            }
            3 => {
                let bytes = self.cdrom.pull(words as usize * 4);
                for (i, chunk) in bytes.chunks(4).enumerate() {
                    memory.write(Self::ram(address.wrapping_add(4 * i as u32)), chunk);
                }
            }
            4 => {
                for _ in 0..words {
                    if from_ram {
                        self.spu.push(&memory.u32(Self::ram(address)).to_le_bytes());
                    } else {
                        memory.write(Self::ram(address), &self.spu.pull(4));
                    }
                    address = address.wrapping_add(step);
                }
            }
            // The ordering table's clear: each entry points at the one below, the last ends it.
            6 => {
                for i in 0..words {
                    let entry = if i + 1 == words { 0x00ff_ffff } else { address.wrapping_sub(4) & 0x001f_ffff };
                    memory.set_u32(Self::ram(address), entry);
                    address = address.wrapping_sub(4);
                }
            }
            _ => {}
        }
        self.dma[channel][2] &= !((1 << 24) | (1 << 28));
        if self.dicr & (1 << (16 + channel)) != 0 {
            let before = self.dicr() >> 31;
            self.dicr |= 1 << (24 + channel);
            if before == 0 && self.dicr() >> 31 != 0 {
                self.raise(irq::DMA);
            }
        }
        if channel == 2 && self.gpu.take_interrupt() {
            self.raise(irq::GPU);
        }
    }

    fn dma_write(&mut self, memory: &mut dyn Memory, offset: u64, value: u32) {
        match offset {
            0x70 => self.dpcr = value,
            0x74 => {
                // The flags clear where a one is written; the rest is kept.
                let cleared = self.dicr & 0x7f00_0000 & !value;
                self.dicr = (value & 0x00ff_803f) | cleared;
            }
            _ => {
                let (channel, register) = ((offset >> 4) as usize, ((offset >> 2) & 3) as usize);
                if channel >= 7 || register >= 3 {
                    return;
                }
                self.dma[channel][register] = value;
                // A channel runs when it is started and enabled; the manual mode waits for its
                // trigger too.
                let control = self.dma[channel][2];
                let enabled = self.dpcr >> (channel * 4 + 3) & 1 != 0;
                let triggered = (control >> 9) & 3 != 0 || control & (1 << 28) != 0;
                if register == 2 && enabled && control & (1 << 24) != 0 && triggered {
                    self.dma_run(memory, channel);
                }
            }
        }
    }

    // endregion

    /// The port at `register` as a word, whatever part of it the access reads.
    fn port_read(&mut self, register: u64) -> u32 {
        let offset = register - PORTS;
        match offset & !3 {
            0x040 => u32::from(self.joy.received.take().unwrap_or(0xff)),
            0x044 => {
                // Ready to send; a byte received; nothing on the wire; the acknowledge's
                // interrupt.
                let idle = self.joy.incoming.is_none();
                0x01 | u32::from(self.joy.received.is_some()) << 1 | u32::from(idle) << 2 | u32::from(self.joy.interrupt) << 9
            }
            0x048 => u32::from(self.joy.mode) | u32::from(self.joy.control) << 16,
            0x04c => u32::from(self.joy.baud) << 16,
            // The serial port: ready to send, nothing received.
            0x054 => 0x05,
            0x070 => self.i_stat,
            0x074 => self.i_mask,
            0x0f0 => self.dpcr,
            0x0f4 => self.dicr(),
            o @ 0x080..=0x0ec => self.dma[(o - 0x80) as usize >> 4].get(((o >> 2) & 3) as usize).copied().unwrap_or(0),
            o @ 0x100..=0x128 => self.timer_read(o - 0x100),
            0x810 => self.gpu.read(),
            0x814 => self.gpu.status(self.line(), self.frames),
            0x820 => self.mdec.read(),
            0x824 => self.mdec.status(),
            _ => self.kept.get(&(register & !3)).copied().unwrap_or(0),
        }
    }

    fn port_write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u32) {
        let offset = register - PORTS;
        match offset {
            0x040 => self.joy_send(value as u8),
            0x048 => {
                self.joy.mode = value as u16;
                if bits == 32 {
                    self.joy_control((value >> 16) as u16);
                }
            }
            0x04a => self.joy_control(value as u16),
            0x04e => self.joy.baud = value as u16,
            0x070 => self.i_stat &= value,
            0x074 => self.i_mask = value & 0x7ff,
            0x080..=0x0f7 => self.dma_write(memory, (offset - 0x80) & !3, value),
            0x100..=0x12b => self.timer_write(offset - 0x100, value),
            0x810 => {
                self.gpu.gp0(value);
                if self.gpu.take_interrupt() {
                    self.raise(irq::GPU);
                }
            }
            0x814 => self.gpu.gp1(value),
            0x820 => self.mdec.write(value),
            0x824 => self.mdec.control(value),
            _ => {
                self.kept.insert(register & !3, value);
            }
        }
    }
}

impl Platform for Psx {
    fn name(&self) -> &'static str {
        "psx"
    }

    fn blocks(&self) -> &'static [Block] {
        &BLOCKS
    }

    fn fold(&self, address: u64) -> Option<u64> {
        let address = u32::try_from(address).ok()?;
        if u64::from(address) == CACHE_CONTROL {
            return Some(CACHE_CONTROL);
        }
        let physical = u64::from(address & 0x1fff_ffff);
        (matches!(address >> 29, 0 | 4 | 5) && (PORTS..PORTS + PORTS_LEN + 0x1000).contains(&physical)).then_some(physical)
    }

    fn read(&mut self, _memory: &mut dyn Memory, register: u64, bits: u32) -> u64 {
        if register == CACHE_CONTROL {
            return u64::from(self.kept.get(&register).copied().unwrap_or(0));
        }
        let offset = register - PORTS;
        let value = match offset {
            // The CD-ROM controller's registers are a byte each.
            0x800..=0x803 => (0..u64::from(bits / 8)).fold(0, |word, i| word | u32::from(self.cdrom.read(offset + i)) << (8 * i)),
            // The SPU's are sixteen bits each.
            0xc00..=0xfff => {
                let low = u32::from(self.spu.read((offset - 0xc00) as usize));
                if bits == 32 { low | u32::from(self.spu.read((offset - 0xc00) as usize + 2)) << 16 } else { low }
            }
            // The debug ports' identifier, which a test program reads before it uses them.
            0x1080..=0x1083 => 0x5853_4350 >> (8 * (offset & 3)),
            _ => self.port_read(register) >> (8 * (register & 3)),
        };
        let value = if bits >= 32 { value } else { value & ((1 << bits) - 1) };
        // Each value of a register once: a program that polls reads the same one for long.
        if crate::tracing() && self.traced.insert((register, value)) {
            eprintln!("psx: read{bits} {register:#x} = {value:#x} at cycle {}", self.clock.now());
        }
        u64::from(value)
    }

    fn write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u64) {
        let value = value as u32;
        if register == CACHE_CONTROL {
            self.kept.insert(register, value);
            return;
        }
        let offset = register - PORTS;
        // The GPU's and the DMA's words are too many to say; the rest are the program's setup.
        if crate::tracing() && !matches!(offset, 0x80..=0xff | 0x810..=0x817 | 0x70..=0x77) {
            eprintln!("psx: write{bits} {register:#x} = {value:#x} at cycle {}", self.clock.now());
        }
        match offset {
            0x800..=0x803 => {
                if offset == 0x801 || offset == 0x800 {
                    self.cdrom.disc = self.host.storage.sector(16).is_some();
                    if self.cdrom.disc && self.cdrom.tracks.is_empty() {
                        (self.cdrom.tracks, self.cdrom.sectors) = self.host.storage.tracks();
                        // A disc that is a folder of its files has one track, of any length.
                        if self.cdrom.tracks.is_empty() {
                            self.cdrom.tracks.push(crate::disc::Track { number: 1, audio: false, start: 0 });
                            self.cdrom.sectors = 333_000;
                        }
                    }
                }
                self.cdrom.write(offset, value as u8);
            }
            0xc00..=0xfff => {
                self.spu.write((offset - 0xc00) as usize, value as u16);
                if bits == 32 {
                    self.spu.write((offset - 0xc00) as usize + 2, (value >> 16) as u16);
                }
            }
            // The debug ports a development board has in the second expansion region, which a
            // test program ends itself and prints through: a character, and the exit code.
            0x1080 if bits == 8 => self.host.console.write(&[value as u8]),
            0x1082 if bits == 16 => self.ended = Some(i32::from(value as u16 as i16)),
            _ => self.port_write(memory, register, bits, value),
        }
    }

    fn call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop> {
        self.bios_call(memory, name, args)
    }

    fn resume(&mut self, _memory: &mut dyn Memory) -> Option<u64> {
        self.event_came()
    }

    fn advance(&mut self, _memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop> {
        if let Some(code) = self.ended {
            return Err(Stop::Exit(code));
        }
        if self.closed || !self.host.video.idle() {
            return Err(Stop::Exit(0));
        }
        self.timers(cycles);
        self.spu.advance(cycles);
        let storage = &mut self.host.storage;
        // A sector as the drive reads it; of a disc the host has only the user data of, the
        // sector around the data.
        let whole = |lba: u32| {
            storage.raw_sector(lba).or_else(|| {
                let data = storage.sector(lba)?;
                let mut raw = vec![0u8; crate::disc::RAW];
                raw[12..15].copy_from_slice(&crate::disc::address(lba));
                raw[15] = 2;
                raw[16..24].copy_from_slice(&[0, 0, 8, 0, 0, 0, 8, 0]);
                raw[24..24 + data.len().min(crate::disc::DATA)].copy_from_slice(&data[..data.len().min(crate::disc::DATA)]);
                Some(raw)
            })
        };
        if self.cdrom.advance(cycles, whole) {
            self.raise(irq::CDROM);
        }
        // The drive's sound goes to the sound unit's mix; what the unit is a second behind on
        // is dropped, so the sound stays with the picture.
        self.spu.cd.extend(self.cdrom.audio.drain(..));
        if self.spu.cd.len() > spu::RATE as usize {
            let late = self.spu.cd.len() - spu::RATE as usize / 4;
            self.spu.cd.drain(..late);
        }
        let mut left = cycles;
        while let Some(event) = self.clock.step(&mut left) {
            match event {
                Event::VBlank => {
                    self.frames += 1;
                    gpu::Gpu::trace_frame(self.frames);
                    self.blank_at = self.clock.now();
                    self.raise(irq::VBLANK);
                    let samples = std::mem::take(&mut self.spu.out);
                    self.host.audio.play(&samples, spu::RATE);
                    if !self.host.video.present(&self.gpu.frame()) {
                        self.closed = true;
                    }
                    // With `RA_VRAM_DUMP` naming a directory, the video memory every hundredth frame.
                    if self.frames % 100 == 0
                        && let Some(dir) = std::env::var_os("RA_VRAM_DUMP")
                    {
                        let path = std::path::Path::new(&dir).join(format!("vram-{:05}.png", self.frames));
                        let _ = std::fs::write(path, crate::png::encode(&self.gpu.video_memory()));
                    }
                    self.clock.after(self.system().0, Event::VBlank);
                }
                Event::JoyByte => {
                    if let Some((answer, acknowledged)) = self.joy.incoming.take() {
                        self.joy.received = Some(answer);
                        if acknowledged {
                            self.clock.after(JOY_ACK, Event::JoyAck);
                        }
                    }
                }
                Event::JoyAck => {
                    self.joy.interrupt = true;
                    if self.joy.control & 0x1000 != 0 {
                        self.raise(irq::CONTROLLER);
                    }
                }
            }
        }
        if self.closed { Err(Stop::Exit(0)) } else { Ok(()) }
    }

    fn idle(&self) -> u64 {
        self.clock.until_next().unwrap_or(0)
    }

    fn stacks(&self) -> Option<crate::Stacks> {
        // The kernel starts a program at the top of the memory, and runs its handlers on a stack
        // of its own in its part below the program.
        Some(crate::Stacks { program: 0x801f_fff0, handlers: 0x8000_dff0, nested: 0x1000 })
    }

    fn interrupt(&self) -> bool {
        self.i_stat & self.i_mask != 0
    }

    fn deliver(&mut self, memory: &mut dyn Memory) -> Option<Delivery> {
        if self.bios.current.is_some() {
            return None;
        }
        if self.bios.steps.is_empty() {
            let pending = self.fresh & self.i_mask;
            if self.bios.handling || self.bios.critical || pending == 0 {
                return None;
            }
            self.fresh &= !pending;
            if !self.begin_exception(memory, pending) {
                return None;
            }
        }
        self.next_step(memory)
    }

    fn delivered(&mut self, _memory: &mut dyn Memory, _delivery: &Delivery, result: u64) {
        self.end_step(result);
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

    fn machine() -> (Psx, Ram, Capture) {
        let capture = Capture::new();
        (Psx::new(Host::headless(&capture)), Ram::new(RAM, 0x20_0000), capture)
    }

    fn call(psx: &mut Psx, ram: &mut Ram, name: &str, args: &[u64]) -> u64 {
        match psx.call(ram, name, args) {
            Ok(Answer::Value(v)) => v,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn the_ports_fold_from_every_segment_and_memory_does_not() {
        let (psx, ..) = machine();
        assert_eq!(psx.fold(0x1f80_1810), Some(0x1f80_1810));
        assert_eq!(psx.fold(0xbf80_1070), Some(0x1f80_1070));
        assert_eq!(psx.fold(0x9f80_1c00), Some(0x1f80_1c00));
        assert_eq!(psx.fold(0xfffe_0130), Some(CACHE_CONTROL));
        assert_eq!(psx.fold(0x8001_0000), None);
        assert_eq!(psx.fold(0x1f80_0000), None, "the scratchpad is memory");
    }

    #[test]
    fn the_tty_prints_and_the_string_functions_work_on_the_programs_memory() {
        let (mut psx, mut ram, capture) = machine();
        ram.write(RAM + 0x100, b"n=%d %s\n\0");
        ram.write(RAM + 0x120, b"pad\0");
        let printed = call(&mut psx, &mut ram, "printf", &[RAM + 0x100, 7, RAM + 0x120]);
        assert_eq!(capture.lock().console, b"n=7 pad\n");
        assert_eq!(printed, 8);
        assert_eq!(call(&mut psx, &mut ram, "strlen", &[RAM + 0x120]), 3);
        call(&mut psx, &mut ram, "strcpy", &[RAM + 0x200, RAM + 0x120]);
        call(&mut psx, &mut ram, "strcat", &[RAM + 0x200, RAM + 0x120]);
        assert_eq!(ram.cstr(RAM + 0x200, 16), b"padpad");
        assert_eq!(call(&mut psx, &mut ram, "strcmp", &[RAM + 0x200, RAM + 0x120]) as u32, 1);
        assert_eq!(call(&mut psx, &mut ram, "strchr", &[RAM + 0x200, u64::from(b'd')]), RAM + 0x202);
        ram.write(RAM + 0x300, b" -0x1f!\0");
        assert_eq!(call(&mut psx, &mut ram, "strtol", &[RAM + 0x300, RAM + 0x310, 0]) as u32, (-31i32).cast_unsigned());
        assert_eq!(u64::from(ram.u32(RAM + 0x310)), RAM + 0x306);
    }

    #[test]
    fn the_heap_hands_out_blocks_apart_and_takes_them_back() {
        let (mut psx, mut ram, _) = machine();
        call(&mut psx, &mut ram, "InitHeap", &[RAM + 0x1000, 0x100]);
        let first = call(&mut psx, &mut ram, "malloc", &[0x20]);
        let second = call(&mut psx, &mut ram, "malloc", &[0x20]);
        assert_eq!((first, second), (RAM + 0x1000, RAM + 0x1020));
        call(&mut psx, &mut ram, "free", &[first]);
        assert_eq!(call(&mut psx, &mut ram, "malloc", &[0x10]), first, "the freed block is taken again");
        assert_eq!(call(&mut psx, &mut ram, "malloc", &[0x1000]), 0, "more than the heap holds");
    }

    #[test]
    fn a_frame_of_cycles_presents_the_picture_and_raises_the_vertical_blank() {
        let (mut psx, mut ram, capture) = machine();
        psx.write(&mut ram, PORTS + 0x074, 32, 1);
        psx.advance(&mut ram, NTSC.0 - 1).unwrap();
        assert_eq!((capture.lock().frames, psx.read(&mut ram, PORTS + 0x070, 32)), (0, 0));
        psx.advance(&mut ram, 1).unwrap();
        assert_eq!((capture.lock().frames, psx.read(&mut ram, PORTS + 0x070, 32)), (1, 1));
        assert_eq!(capture.lock().samples.len() as u64, NTSC.0 / (CLOCK / u64::from(spu::RATE)));
        // The program acknowledges by writing the other bits.
        psx.write(&mut ram, PORTS + 0x070, 32, !1);
        assert_eq!(psx.read(&mut ram, PORTS + 0x070, 16), 0);
    }

    #[test]
    fn the_run_ends_when_the_picture_is_closed() {
        let capture = Capture::new().limit(2);
        let mut psx = Psx::new(Host::headless(&capture));
        let mut ram = Ram::new(RAM, 0x1000);
        assert_eq!(psx.advance(&mut ram, NTSC.0), Ok(()));
        assert_eq!(psx.advance(&mut ram, NTSC.0), Err(Stop::Exit(0)));
    }

    #[test]
    fn the_exit_hook_is_called_for_an_interrupt_outside_a_critical_section() {
        let (mut psx, mut ram, _) = machine();
        // A jmp_buf: the handler, its stack, and its gp in the twelfth word.
        ram.set_u32(RAM + 0x400, 0x8001_2340);
        ram.set_u32(RAM + 0x404, 0x801f_ff00);
        ram.set_u32(RAM + 0x400 + 44, 0x8002_0000);
        call(&mut psx, &mut ram, "SetCustomExitFromException", &[RAM + 0x400]);
        psx.write(&mut ram, PORTS + 0x074, 32, 1);
        assert_eq!(call(&mut psx, &mut ram, "syscall", &[1]), 1, "interrupts were on");
        psx.advance(&mut ram, NTSC.0).unwrap();
        assert_eq!(psx.deliver(&mut ram), None, "the critical section holds the interrupt");
        call(&mut psx, &mut ram, "syscall", &[2]);
        let delivery = psx.deliver(&mut ram).expect("the hook");
        assert_eq!((delivery.entry, delivery.stack, delivery.gp), (0x8001_2340, Some(0x801f_ff00), Some(0x8002_0000)));
        assert_eq!(psx.deliver(&mut ram), None, "one handler at a time");
        assert_eq!(psx.call(&mut ram, "ReturnFromException", &[]), Ok(Answer::Leave));
        psx.delivered(&mut ram, &delivery, 0);
        assert_eq!(psx.deliver(&mut ram), None, "the same interrupt is offered once");
    }

    #[test]
    fn the_pad_driver_fills_the_buffers_and_an_event_waits_for_the_blank() {
        let pressed = pad::State { buttons: pad::CROSS, ..pad::State::default() };
        let capture = Capture::new().press(0, 0, pressed);
        let mut psx = Psx::new(Host::headless(&capture));
        let mut ram = Ram::new(RAM, 0x1000);
        call(&mut psx, &mut ram, "InitPad", &[RAM + 0x100, 34, RAM + 0x130, 34]);
        call(&mut psx, &mut ram, "StartPad", &[]);
        let event = call(&mut psx, &mut ram, "OpenEvent", &[0xf200_0003, 2, 0x2000, 0]);
        call(&mut psx, &mut ram, "EnableEvent", &[event]);
        assert_eq!(psx.call(&mut ram, "WaitEvent", &[event]), Ok(Answer::Wait));
        assert_eq!(psx.resume(&mut ram), None);
        psx.advance(&mut ram, NTSC.0).unwrap();
        assert_eq!(psx.deliver(&mut ram), None, "nothing of the program is called");
        assert_eq!(psx.resume(&mut ram), Some(1));
        assert_eq!(ram.bytes(RAM + 0x100, 4), [0x00, 0x41, 0xff, 0xbf]);
        assert_eq!(ram.bytes(RAM + 0x130, 2), [0xff, 0x00], "the second port is empty");
    }

    #[test]
    fn the_controller_port_answers_a_poll_byte_by_byte() {
        let (mut psx, mut ram, _) = machine();
        psx.write(&mut ram, PORTS + 0x04a, 16, 0x1003);
        let mut answers = Vec::new();
        for byte in [0x01u64, 0x42, 0x00, 0x00, 0x00] {
            psx.write(&mut ram, PORTS + 0x040, 8, byte);
            assert_eq!(psx.read(&mut ram, PORTS + 0x044, 16) & 6, 0, "the byte is on the wire");
            psx.advance(&mut ram, 8 * 0x88).unwrap();
            assert_eq!(psx.read(&mut ram, PORTS + 0x044, 16) & 6, 6, "a byte came back");
            answers.push(psx.read(&mut ram, PORTS + 0x040, 8));
        }
        assert_eq!(answers, [0xff, 0x41, 0x5a, 0xff, 0xff]);
        psx.advance(&mut ram, JOY_ACK).unwrap();
        assert_ne!(psx.read(&mut ram, PORTS + 0x070, 32) & u64::from(irq::CONTROLLER), 0);
        assert_ne!(psx.read(&mut ram, PORTS + 0x044, 16) & 0x200, 0);
        psx.write(&mut ram, PORTS + 0x04a, 16, 0x1013);
        assert_eq!(psx.read(&mut ram, PORTS + 0x044, 16) & 0x200, 0, "acknowledged");
    }

    #[test]
    fn the_acknowledge_comes_after_the_byte_as_a_kernel_waits_for_it() {
        let (mut psx, mut ram, _) = machine();
        // A kernel's pad driver: select, send, acknowledge what came before while the byte is
        // on the wire, wait for the answer, then find the acknowledge's interrupt.
        psx.write(&mut ram, PORTS + 0x04a, 16, 0x1003);
        psx.write(&mut ram, PORTS + 0x040, 8, 0x01);
        psx.advance(&mut ram, 240).unwrap();
        psx.write(&mut ram, PORTS + 0x04a, 16, 0x1013);
        psx.write(&mut ram, PORTS + 0x070, 32, u64::from(!irq::CONTROLLER));
        assert_eq!(psx.read(&mut ram, PORTS + 0x044, 16) & 2, 0, "the answer is not in yet");
        psx.advance(&mut ram, 8 * 0x88 - 240).unwrap();
        assert_ne!(psx.read(&mut ram, PORTS + 0x044, 16) & 2, 0);
        assert_eq!(psx.read(&mut ram, PORTS + 0x040, 8), 0xff);
        assert_eq!(psx.read(&mut ram, PORTS + 0x070, 32) & u64::from(irq::CONTROLLER), 0, "nor the acknowledge");
        psx.advance(&mut ram, JOY_ACK).unwrap();
        assert_ne!(psx.read(&mut ram, PORTS + 0x070, 32) & u64::from(irq::CONTROLLER), 0);
    }

    #[test]
    fn dma_clears_an_ordering_table_and_sends_its_list_to_the_gpu() {
        let (mut psx, mut ram, _) = machine();
        // Four entries, cleared from the last down.
        psx.write(&mut ram, PORTS + 0x0e0, 32, 0x0000_100c);
        psx.write(&mut ram, PORTS + 0x0e4, 32, 4);
        psx.write(&mut ram, PORTS + 0x0f0, 32, 0x0800_0800);
        psx.write(&mut ram, PORTS + 0x0e8, 32, 0x1100_0002);
        assert_eq!([0x1000u64, 0x1004, 0x1008, 0x100c].map(|at| ram.u32(RAM + at)), [0x00ff_ffff, 0x0000_1000, 0x0000_1004, 0x0000_1008]);
        assert_eq!(psx.read(&mut ram, PORTS + 0x0e8, 32) & (1 << 24), 0, "the channel is done");
        // A fill hung on the last entry: three words after its header.
        ram.set_u32(RAM + 0x2000, 0x0300_1008);
        for (i, word) in [0x02ff_ffffu32, 0, 0x0001_0010].into_iter().enumerate() {
            ram.set_u32(RAM + 0x2004 + 4 * i as u64, word);
        }
        ram.set_u32(RAM + 0x100c, 0x0000_2000);
        psx.write(&mut ram, PORTS + 0x0a0, 32, 0x0000_100c);
        psx.write(&mut ram, PORTS + 0x0a8, 32, 0x0100_0401);
        assert_eq!(psx.gpu.vram[0], 0x7fff);
    }

    #[test]
    fn a_timer_counts_to_its_target_and_raises_its_interrupt() {
        let (mut psx, mut ram, _) = machine();
        psx.write(&mut ram, PORTS + 0x128, 32, 100);
        psx.write(&mut ram, PORTS + 0x124, 32, 0x0258);
        psx.advance(&mut ram, 8 * 99).unwrap();
        assert_eq!(psx.read(&mut ram, PORTS + 0x120, 32), 99);
        assert_eq!(psx.read(&mut ram, PORTS + 0x070, 32) & u64::from(irq::TIMER0 << 2), 0);
        psx.advance(&mut ram, 8 * 2).unwrap();
        assert_eq!(psx.read(&mut ram, PORTS + 0x120, 32), 1, "the target resets it");
        assert_ne!(psx.read(&mut ram, PORTS + 0x070, 32) & u64::from(irq::TIMER0 << 2), 0);
        assert_ne!(psx.read(&mut ram, PORTS + 0x124, 32) & (1 << 11), 0);
        assert_eq!(psx.read(&mut ram, PORTS + 0x124, 32) & (1 << 11), 0, "the flag clears when read");
    }
}
