//! The Nintendo 64: the devices of its RCP behind their registers (the MIPS interface's interrupt
//! controller, the video, audio, peripheral and serial interfaces, the RSP's memories) and the
//! PIF with the controllers behind it, over the host. The console has no kernel: a program
//! brings its own, so nothing is called here by a name.
//!
//! The video interface shows the frame buffer the program points it at in RDRAM, the audio
//! interface plays the buffers it is handed, the peripheral interface copies between the
//! cartridge and RDRAM, and the serial interface runs the PIF's commands, which is how a program
//! reads its controllers. The `ISViewer`'s buffer in the cartridge's domain is the console's text.
//!
//! What is not here yet is the other processor and the rasterizer: a task the program starts
//! on the RSP ends at once with nothing done, and the RDP's commands are read for the one that
//! asks for an interrupt and draw nothing. A program that draws into its frame buffer itself is
//! shown; one that draws through the RDP shows what it cleared the buffer to.

use std::collections::{BTreeMap, VecDeque};

use crate::clock::Scheduler;
use crate::host::{Frame, Host};
use crate::scanout::{Format, Scan};
use crate::{Answer, Block, Delivery, Map, Memory, MemoryExt, Order, Platform, Stop, pad};

/// RDRAM at its `kseg0` address, where a device's address of it lands.
pub const RDRAM: u64 = 0x8000_0000;
/// The VR4300's cycles in a second, and in a frame of the NTSC picture.
pub const CLOCK: u64 = 93_750_000;
const FRAME: u64 = CLOCK * 1001 / 60_000;
/// The clock the audio interface divides for its sample rate, an NTSC console's.
const AUDIO_CLOCK: u64 = 48_681_812;

/// The lines of the MIPS interface's interrupt register.
pub mod intr {
    pub const SP: u32 = 1 << 0;
    pub const SI: u32 = 1 << 1;
    pub const AI: u32 = 1 << 2;
    pub const VI: u32 = 1 << 3;
    pub const PI: u32 = 1 << 4;
    pub const DP: u32 = 1 << 5;
}

const SP_MEMORY: u64 = 0x0400_0000;
const SP_MEMORY_LEN: u64 = 0x2000;
const SP: u64 = 0x0404_0000;
const DP: u64 = 0x0410_0000;
const MI: u64 = 0x0430_0000;
const VI: u64 = 0x0440_0000;
const AI: u64 = 0x0450_0000;
const PI: u64 = 0x0460_0000;
const RI: u64 = 0x0470_0000;
const SI: u64 = 0x0480_0000;
/// The PI's domains: the disk drive's, the cartridge's save memory and its ROM.
const DOMAINS: u64 = 0x0500_0000;
const CARTRIDGE: u64 = 0x1000_0000;
/// The PIF: its ROM, and the 64 bytes of RAM a program speaks to it through.
const PIF: u64 = 0x1fc0_0000;
const PIF_RAM: u64 = 0x1fc0_07c0;
const PIF_END: u64 = 0x1fc0_0800;
/// The `ISViewer`'s length register, a write of which prints that many bytes of its buffer.
const ISVIEWER_LENGTH: u64 = 0x13ff_0014;
const ISVIEWER_BUFFER: u64 = 0x13ff_0020;

/// The devices by their registers, at their physical addresses.
pub const BLOCKS: [Block; 13] = [
    Block::new("sp-memory", SP_MEMORY, SP_MEMORY_LEN),
    Block::new("sp", SP, 0x20),
    Block::new("sp-pc", 0x0408_0000, 0x8),
    Block::new("dp", DP, 0x20),
    Block::new("dp-span", 0x0420_0000, 0x10),
    Block::new("mi", MI, 0x10),
    Block::new("vi", VI, 0x38),
    Block::new("ai", AI, 0x18),
    Block::new("pi", PI, 0x34),
    Block::new("ri", RI, 0x20),
    Block::new("si", SI, 0x1c),
    Block::new("domains", DOMAINS, PIF - DOMAINS),
    Block::new("pif", PIF, PIF_END - PIF),
];

/// The buttons of the controller's two bytes, the first byte's in the upper half.
pub mod button {
    pub const A: u16 = 0x8000;
    pub const B: u16 = 0x4000;
    pub const Z: u16 = 0x2000;
    pub const START: u16 = 0x1000;
    pub const UP: u16 = 0x0800;
    pub const DOWN: u16 = 0x0400;
    pub const LEFT: u16 = 0x0200;
    pub const RIGHT: u16 = 0x0100;
    pub const L: u16 = 0x0020;
    pub const R: u16 = 0x0010;
    pub const C_UP: u16 = 0x0008;
    pub const C_DOWN: u16 = 0x0004;
    pub const C_LEFT: u16 = 0x0002;
    pub const C_RIGHT: u16 = 0x0001;
}

/// The controller's four bytes for what the host's pad holds: the cross is A, the square B, the
/// second left shoulder the trigger under the stick, the right stick and the two buttons left
/// over the four C buttons, the left stick the stick, up positive.
pub fn controller(pad: &pad::State) -> [u8; 4] {
    use button::{A, B, C_DOWN, C_LEFT, C_RIGHT, C_UP, DOWN, L, LEFT, R, RIGHT, START, UP, Z};
    let [right_x, right_y, left_x, left_y] = pad.sticks;
    let pressed = [
        (pad::CROSS, A),
        (pad::SQUARE, B),
        (pad::L2, Z),
        (pad::START, START),
        (pad::UP, UP),
        (pad::DOWN, DOWN),
        (pad::LEFT, LEFT),
        (pad::RIGHT, RIGHT),
        (pad::L1, L),
        (pad::R1, R),
        (pad::TRIANGLE, C_UP),
        (pad::CIRCLE, C_RIGHT),
    ];
    let mut buttons = pressed.iter().filter(|(held, _)| pad.buttons & held != 0).fold(0, |all, (_, button)| all | button);
    for (far, button) in [(right_y < 0x40, C_UP), (right_y > 0xc0, C_DOWN), (right_x < 0x40, C_LEFT), (right_x > 0xc0, C_RIGHT)] {
        if far {
            buttons |= button;
        }
    }
    // The stick's range is about eighty either way.
    let axis = |value: u8| ((i32::from(value) - 0x80) * 80 / 0x80) as i8;
    let [high, low] = buttons.to_be_bytes();
    [high, low, axis(left_x).cast_unsigned(), axis(left_y).wrapping_neg().cast_unsigned()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    /// The beam reaches the line the video interface interrupts at: a frame is shown.
    Frame,
    /// The audio buffer that plays is played out.
    Sound,
}

pub struct N64 {
    host: Host,
    clock: Scheduler<Event>,
    mi_mode: u32,
    mi_intr: u32,
    mi_mask: u32,
    vi: [u32; 14],
    /// The cycle the last frame was shown at.
    frame_at: u64,
    ai_address: u32,
    ai_control: u32,
    ai_rate: u32,
    /// The cycles each buffer the audio interface holds takes to play: two at the most.
    ai_buffers: VecDeque<u64>,
    pi: [u32; 13],
    si_address: u32,
    sp_status: u32,
    sp: [u32; 4],
    dp: [u32; 4],
    /// The RSP's data and instruction memories.
    sp_memory: Vec<u8>,
    pif: [u8; 64],
    rom: Vec<u8>,
    /// The bytes written in the PI's domains, which keep them as a cartridge's memory does.
    written: BTreeMap<u64, u8>,
    /// The registers that only keep what is written.
    kept: BTreeMap<u64, u32>,
    frames: u64,
    closed: bool,
}

impl N64 {
    pub fn new(host: Host) -> Self {
        let mut clock = Scheduler::default();
        clock.after(FRAME, Event::Frame);
        Self {
            host,
            clock,
            mi_mode: 0,
            mi_intr: 0,
            mi_mask: 0,
            vi: [0; 14],
            frame_at: 0,
            ai_address: 0,
            ai_control: 0,
            ai_rate: 0,
            ai_buffers: VecDeque::new(),
            pi: [0; 13],
            si_address: 0,
            // The RSP is halted until a program starts it.
            sp_status: 1,
            sp: [0; 4],
            dp: [0; 4],
            sp_memory: vec![0; SP_MEMORY_LEN as usize],
            pif: [0; 64],
            rom: Vec::new(),
            written: BTreeMap::new(),
            kept: BTreeMap::new(),
            frames: 0,
            closed: false,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn raise(&mut self, lines: u32) {
        self.mi_intr |= lines;
    }

    /// The byte at `at` of the places that are memory to a program: the RSP's memories, the
    /// PI's domains (what was written there, over the cartridge's ROM) and the PIF's RAM.
    fn byte(&self, at: u64) -> Option<u8> {
        match at {
            SP_MEMORY..SP => Some(self.sp_memory[((at - SP_MEMORY) % SP_MEMORY_LEN) as usize]),
            DOMAINS..PIF => Some(self.written.get(&at).copied().unwrap_or_else(|| {
                at.checked_sub(CARTRIDGE).and_then(|offset| self.rom.get(usize::try_from(offset).ok()?)).copied().unwrap_or(0)
            })),
            PIF_RAM..PIF_END => Some(self.pif[(at - PIF_RAM) as usize]),
            PIF..PIF_RAM => Some(0),
            _ => None,
        }
    }

    fn set_byte(&mut self, at: u64, value: u8) {
        match at {
            SP_MEMORY..SP => self.sp_memory[((at - SP_MEMORY) % SP_MEMORY_LEN) as usize] = value,
            DOMAINS..PIF => {
                self.written.insert(at, value);
            }
            PIF_RAM..PIF_END => self.pif[(at - PIF_RAM) as usize] = value,
            _ => {}
        }
    }

    /// The half-line the beam is on: the line the interrupt comes at when a frame starts, and
    /// round the picture's lines from there.
    fn line(&self) -> u32 {
        let lines = u64::from(match self.vi[6] & 0x3ff {
            0 => 525,
            sync => sync + 1,
        });
        let since = self.clock.now().saturating_sub(self.frame_at).min(FRAME - 1);
        ((u64::from(self.vi[3] & 0x3ff) + since * lines / FRAME) % lines) as u32 & 0x3fe
    }

    /// The register at `at`, a word.
    fn register(&self, at: u64) -> u32 {
        let index = |base: u64| ((at - base) / 4) as usize;
        match at {
            MI => self.mi_mode,
            // The RCP's version.
            0x0430_0004 => 0x0202_0102,
            0x0430_0008 => self.mi_intr,
            0x0430_000c => self.mi_mask,
            0x0440_0010 => self.line(),
            VI..0x0440_0038 => self.vi[index(VI)],
            AI => self.ai_address,
            // What is left of the buffer that plays, which a program reads as whether one does.
            0x0450_0004 => u32::from(!self.ai_buffers.is_empty()) * 8,
            0x0450_000c => {
                let full = if self.ai_buffers.len() >= 2 { 0x8000_0001 } else { 0 };
                full | (u32::from(!self.ai_buffers.is_empty()) << 30) | (self.ai_control << 25)
            }
            // No transfer is in flight: one ends as it starts.
            0x0460_0010 => (self.mi_intr & intr::PI) >> 1,
            PI..0x0460_0034 => self.pi[index(PI)],
            SI => self.si_address,
            0x0480_0018 => (self.mi_intr & intr::SI) << 11,
            0x0404_0010 => self.sp_status,
            SP..0x0404_0010 => self.sp[index(SP)],
            // The command buffer is ready for more.
            0x0410_000c => 0x80 | (self.dp[3] & 1),
            DP..0x0410_000c => self.dp[index(DP)],
            // The RDRAM interface's select, which the boot code reads as set up.
            0x0470_000c => 0x14,
            _ => self.kept.get(&at).copied().unwrap_or(0),
        }
    }

    fn set_register(&mut self, memory: &mut dyn Memory, at: u64, value: u32) {
        let index = |base: u64| ((at - base) / 4) as usize;
        match at {
            MI => {
                if value & 0x800 != 0 {
                    self.mi_intr &= !intr::DP;
                }
                self.mi_mode = value & 0x7f;
            }
            // A line's mask is cleared by one bit and set by the next.
            0x0430_000c => {
                for line in 0..6 {
                    if value & (1 << (2 * line)) != 0 {
                        self.mi_mask &= !(1 << line);
                    }
                    if value & (2 << (2 * line)) != 0 {
                        self.mi_mask |= 1 << line;
                    }
                }
            }
            // A write of the current line acknowledges the interrupt.
            0x0440_0010 => self.mi_intr &= !intr::VI,
            VI..0x0440_0038 => self.vi[index(VI)] = value,
            AI => self.ai_address = value & 0x00ff_ffff,
            0x0450_0004 => self.sound(memory, value & 0x3_fff8),
            0x0450_0008 => self.ai_control = value & 1,
            0x0450_000c => self.mi_intr &= !intr::AI,
            0x0450_0010 => self.ai_rate = value & 0x3fff,
            0x0460_0008 => self.cartridge_dma(memory, value, false),
            0x0460_000c => self.cartridge_dma(memory, value, true),
            0x0460_0010 => {
                if value & 2 != 0 {
                    self.mi_intr &= !intr::PI;
                }
            }
            PI..0x0460_0034 => self.pi[index(PI)] = value,
            SI => self.si_address = value & 0x00ff_ffff,
            // The PIF's RAM into RDRAM, its commands run first; and RDRAM into the PIF's RAM.
            0x0480_0004 => {
                self.joybus();
                memory.write(RDRAM | u64::from(self.si_address), &self.pif);
                self.raise(intr::SI);
            }
            0x0480_0010 => {
                memory.read(RDRAM | u64::from(self.si_address), &mut self.pif);
                self.raise(intr::SI);
            }
            0x0480_0018 => self.mi_intr &= !intr::SI,
            0x0404_0010 => self.sp_control(value),
            0x0404_0008 => self.sp_dma(memory, value, true),
            0x0404_000c => self.sp_dma(memory, value, false),
            SP..0x0404_0008 => self.sp[index(SP)] = value,
            0x0410_0004 => {
                self.dp[1] = value & 0x00ff_fff8;
                self.commands(memory);
            }
            0x0410_000c => {
                // The commands come from RDRAM, or from the RSP's data memory.
                if value & 1 != 0 {
                    self.dp[3] &= !1;
                }
                if value & 2 != 0 {
                    self.dp[3] |= 1;
                }
            }
            // A new start is where the RDP goes on from; an end alone adds to what it has.
            DP => {
                self.dp[0] = value & 0x00ff_fff8;
                self.dp[2] = self.dp[0];
            }
            _ => {
                self.kept.insert(at, value);
            }
        }
    }

    /// The RSP's status register takes commands, a bit each: the halt, the break and the
    /// interrupt cleared or set, and the eight signals. A program that clears the halt starts a
    /// task; with no RSP here the task ends where it starts, broken off and halted, and raises
    /// its interrupt when the program asked for one at a break.
    fn sp_control(&mut self, value: u32) {
        let was_halted = self.sp_status & 1 != 0;
        // Each pair of bits clears and sets one of the status's: the halt, then from the single
        // step on.
        for (clear, bit) in [(0, 0), (5, 5), (7, 6), (9, 7), (11, 8), (13, 9), (15, 10), (17, 11), (19, 12), (21, 13), (23, 14)] {
            if value & (1 << clear) != 0 {
                self.sp_status &= !(1 << bit);
            }
            if value & (2 << clear) != 0 {
                self.sp_status |= 1 << bit;
            }
        }
        if value & 4 != 0 {
            self.sp_status &= !2;
        }
        if value & 8 != 0 {
            self.mi_intr &= !intr::SP;
        }
        if value & 0x10 != 0 {
            self.raise(intr::SP);
        }
        if was_halted && self.sp_status & 1 == 0 {
            self.sp_status |= 3;
            if self.sp_status & 0x40 != 0 {
                self.raise(intr::SP);
            }
        }
    }

    /// A transfer between RDRAM and the RSP's memories: rows of a length, a skip of RDRAM
    /// between two.
    fn sp_dma(&mut self, memory: &mut dyn Memory, value: u32, to_rsp: bool) {
        let (length, count, skip) = (u64::from(value & 0xfff) + 1, u64::from((value >> 12) & 0xff) + 1, u64::from(value >> 20));
        let length = length.next_multiple_of(8);
        let (mut within, mut ram) = (u64::from(self.sp[0] & 0x1fff), RDRAM | u64::from(self.sp[1] & 0x00ff_ffff));
        for _ in 0..count {
            for _ in 0..length {
                // A transfer stays in the memory it starts in, the data's or the instructions'.
                let at = SP_MEMORY + (within & 0x1000) + (within & 0xfff);
                if to_rsp {
                    self.set_byte(at, memory.u8(ram));
                } else {
                    memory.set_u8(ram, self.byte(at).unwrap_or(0));
                }
                within = (within & 0x1000) | ((within + 1) & 0xfff);
                ram += 1;
            }
            ram += skip;
        }
    }

    /// The RDP's commands from the start register to the end's. None draws here; the one that
    /// asks for an interrupt once everything before it is drawn gets it.
    fn commands(&mut self, memory: &mut dyn Memory) {
        let (start, end) = (u64::from(self.dp[2]), u64::from(self.dp[1]));
        let word = |n64: &Self, memory: &mut dyn Memory, at: u64| {
            if n64.dp[3] & 1 == 0 {
                memory.word(RDRAM | at, Order::Big)
            } else {
                (0..4).fold(0, |word, i| (word << 8) | u32::from(n64.byte(SP_MEMORY + ((at + i) & 0xfff)).unwrap_or(0)))
            }
        };
        let mut at = start;
        while at < end {
            let command = (word(self, memory, at) >> 24) & 0x3f;
            // A triangle is as long as what it carries: shading, a texture, depth.
            let words = match command {
                0x08..=0x0f => 4 + [0, 2, 8, 10, 8, 10, 16, 18][(command & 7) as usize],
                0x24 | 0x25 => 2,
                _ => 1,
            };
            if command == 0x29 {
                self.raise(intr::DP);
            }
            at += 8 * words;
        }
        self.dp[2] = self.dp[1];
    }

    /// A transfer of the peripheral interface, `value` its length less one: from the cartridge
    /// into RDRAM, or from RDRAM into the cartridge's domain, which keeps it.
    fn cartridge_dma(&mut self, memory: &mut dyn Memory, value: u32, to_ram: bool) {
        let len = u64::from(value & 0x00ff_ffff) + 1;
        let (ram, cartridge) = (RDRAM | u64::from(self.pi[0] & 0x00ff_fffe), u64::from(self.pi[1]) & 0x1fff_fffe);
        if to_ram {
            let bytes: Vec<u8> = (cartridge..cartridge + len).map(|at| self.byte(at).unwrap_or(0)).collect();
            memory.write(ram, &bytes);
        } else {
            for (at, byte) in (cartridge..).zip(memory.bytes(ram, len as usize)) {
                self.set_byte(at, byte);
            }
        }
        self.pi[0] = self.pi[0].wrapping_add(len as u32);
        self.pi[1] = self.pi[1].wrapping_add(len as u32);
        self.raise(intr::PI);
    }

    /// A buffer handed to the audio interface: sixteen bits a side, played at the rate the DAC's
    /// divider gives. The interface holds the buffer that plays and one more, and interrupts as
    /// each starts.
    fn sound(&mut self, memory: &mut dyn Memory, len: u32) {
        if len == 0 || self.ai_buffers.len() >= 2 {
            return;
        }
        let bytes = memory.bytes(RDRAM | u64::from(self.ai_address), len as usize);
        let samples: Vec<[i16; 2]> =
            bytes.chunks_exact(4).map(|s| [i16::from_be_bytes([s[0], s[1]]), i16::from_be_bytes([s[2], s[3]])]).collect();
        let rate = AUDIO_CLOCK / (u64::from(self.ai_rate) + 1);
        // A divider a program has not set yet is no rate to play at.
        let rate = if (4_000..=96_000).contains(&rate) { rate } else { 44_100 };
        self.host.audio.play(&samples, rate as u32);
        let lasts = samples.len() as u64 * CLOCK / rate;
        self.ai_buffers.push_back(lasts);
        if self.ai_buffers.len() == 1 {
            self.raise(intr::AI);
            self.clock.after(lasts, Event::Sound);
        }
    }

    /// Runs the commands in the PIF's RAM, when its last byte asks for it: a frame for each
    /// channel in turn, the bytes to send and to receive, then the bytes sent, then room for
    /// the answer. The first four channels are the controller ports; nothing is on the fifth,
    /// the cartridge's.
    fn joybus(&mut self) {
        if self.pif[63] & 1 == 0 {
            return;
        }
        let (mut at, mut channel) = (0, 0);
        while at < 63 {
            let send = self.pif[at];
            match send {
                0x00 => {
                    channel += 1;
                    at += 1;
                    continue;
                }
                0xfe => break,
                0xfd | 0xff => {
                    at += 1;
                    continue;
                }
                _ => {}
            }
            let (send, receive) = (usize::from(send & 0x3f), usize::from(self.pif[at + 1] & 0x3f));
            let (command, answer) = (at + 2, at + 2 + send);
            if answer + receive > 63 {
                break;
            }
            let pad = if channel < 4 { self.host.input.pad(channel) } else { None };
            let bytes: Option<Vec<u8>> = match (pad, self.pif[command]) {
                // What the device is: a controller, with nothing in its slot.
                (Some(_), 0x00 | 0xff) => Some(vec![0x05, 0x00, 0x02]),
                (Some(pad), 0x01) => Some(controller(&pad).to_vec()),
                _ => None,
            };
            match bytes {
                Some(bytes) => {
                    for (slot, byte) in self.pif[answer..answer + receive].iter_mut().zip(bytes) {
                        *slot = byte;
                    }
                }
                // Nothing answered on the channel.
                None => self.pif[at + 1] |= 0x80,
            }
            at = answer + receive;
            channel += 1;
        }
        self.pif[63] &= !1;
    }

    /// The picture the video interface shows: the frame buffer at its origin, as wide and as
    /// high as its scales make the lines it scans; black while it shows none.
    fn picture(&self, memory: &mut dyn Memory) -> Frame {
        let format = match self.vi[0] & 3 {
            2 => Format::RGBA5551,
            3 => Format::RGBA8888,
            _ => return Frame::new(320, 240),
        };
        let stride = self.vi[2] & 0xfff;
        let span = |register: u32| (register & 0x3ff).saturating_sub((register >> 16) & 0x3ff);
        let scaled = |lines: u32, scale: u32| lines * (scale & 0xfff) / 1024;
        let (width, height) = match (scaled(span(self.vi[9]), self.vi[12]), scaled(span(self.vi[10]) / 2, self.vi[13])) {
            (0, _) | (_, 0) => (stride, stride * 3 / 4),
            (width, height) => (width.min(stride), height),
        };
        if width == 0 {
            return Frame::new(320, 240);
        }
        let address = RDRAM | u64::from(self.vi[1] & 0x00ff_ffff);
        Scan { address, width, height, stride: stride * format.bytes, format, order: Order::Big }.frame(memory)
    }
}

impl Platform for N64 {
    fn name(&self) -> &'static str {
        "n64"
    }

    fn blocks(&self) -> &'static [Block] {
        &BLOCKS
    }

    fn map(&self) -> Map {
        // The RCP, the PI's domains and the PIF through `kseg1`, where a program reaches them;
        // RDRAM there too.
        const PORTS: [Block; 1] = [Block::new("rcp", 0xa000_0000 | SP_MEMORY, PIF_END - SP_MEMORY)];
        Map { order: Order::Big, ports: &PORTS, ram: &[0xa000_0000], repeats: 1, extras: &[] }
    }

    /// What the boot code leaves in the RSP's data memory: the size of RDRAM, then an NTSC set,
    /// a cold reset and a retail console.
    fn start(&mut self, _memory: &mut dyn Memory, ram: (u64, u64)) {
        let size = u32::try_from(ram.1).unwrap_or(u32::MAX);
        self.sp_memory[..4].copy_from_slice(&size.to_be_bytes());
        self.sp_memory[9] = 1;
    }

    fn cartridge(&mut self, rom: Vec<u8>) {
        self.rom = rom;
    }

    fn fold(&self, address: u64) -> Option<u64> {
        // A 32-bit address a 64-bit register holds sign-extended is the same address.
        let address = address as u32;
        let physical = u64::from(address & 0x1fff_ffff);
        ((0x8000_0000..0xc000_0000).contains(&address) && (SP_MEMORY..PIF_END).contains(&physical)).then_some(physical)
    }

    fn read(&mut self, _memory: &mut dyn Memory, register: u64, bits: u32) -> u64 {
        let bytes = u64::from(bits / 8).clamp(1, 8);
        if self.byte(register).is_some() {
            return (register..register + bytes).fold(0, |value, at| (value << 8) | u64::from(self.byte(at).unwrap_or(0)));
        }
        let word = |at: u64| u64::from(self.register(at & !3));
        if bytes == 8 {
            return (word(register) << 32) | word(register + 4);
        }
        // A part of a register is its bytes from the address on, the first the highest.
        let shift = 8 * 4u64.saturating_sub((register & 3) + bytes);
        (word(register) >> shift) & ((1 << (8 * bytes)) - 1)
    }

    fn write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u64) {
        let bytes = u64::from(bits / 8).clamp(1, 8);
        if self.byte(register).is_some() {
            if register == ISVIEWER_LENGTH && bytes == 4 {
                let text: Vec<u8> = (ISVIEWER_BUFFER..ISVIEWER_BUFFER + (value & 0xffff)).map(|at| self.byte(at).unwrap_or(0)).collect();
                self.host.console.write(&text);
                return;
            }
            for (i, at) in (register..register + bytes).enumerate() {
                self.set_byte(at, (value >> (8 * (bytes as usize - 1 - i))) as u8);
            }
            return;
        }
        match bytes {
            8 => {
                self.set_register(memory, register & !3, (value >> 32) as u32);
                self.set_register(memory, (register & !3) + 4, value as u32);
            }
            4 => self.set_register(memory, register & !3, value as u32),
            _ => {
                let shift = 8 * 4u64.saturating_sub((register & 3) + bytes);
                let mask = ((1u32 << (8 * bytes)) - 1) << shift;
                let merged = (self.register(register & !3) & !mask) | (((value as u32) << shift) & mask);
                self.set_register(memory, register & !3, merged);
            }
        }
    }

    fn call(&mut self, _memory: &mut dyn Memory, name: &str, _args: &[u64]) -> Result<Answer, Stop> {
        Err(Stop::fault(format!("the Nintendo 64 has no kernel to answer {name}")))
    }

    fn resume(&mut self, _memory: &mut dyn Memory) -> Option<u64> {
        None
    }

    fn advance(&mut self, memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop> {
        if self.closed || !self.host.video.idle() {
            return Err(Stop::Exit(0));
        }
        let mut left = cycles;
        while let Some(event) = self.clock.step(&mut left) {
            match event {
                Event::Frame => {
                    self.frames += 1;
                    self.frame_at = self.clock.now();
                    self.raise(intr::VI);
                    let picture = self.picture(memory);
                    if !self.host.video.present(&picture) {
                        self.closed = true;
                    }
                    self.clock.after(FRAME, Event::Frame);
                }
                Event::Sound => {
                    self.ai_buffers.pop_front();
                    if let Some(lasts) = self.ai_buffers.front().copied() {
                        self.raise(intr::AI);
                        self.clock.after(lasts, Event::Sound);
                    }
                }
            }
        }
        if self.closed { Err(Stop::Exit(0)) } else { Ok(()) }
    }

    fn idle(&self) -> u64 {
        self.clock.until_next().unwrap_or(0)
    }

    fn interrupt(&self) -> bool {
        self.mi_intr & self.mi_mask != 0
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

    fn machine() -> (N64, Ram, Capture) {
        let capture = Capture::new();
        (N64::new(Host::headless(&capture)), Ram::new(RDRAM, 0x10_0000), capture)
    }

    #[test]
    fn the_registers_fold_from_the_kernel_s_segments_and_rdram_does_not() {
        let (n64, ..) = machine();
        assert_eq!(n64.fold(0xa430_0008), Some(0x0430_0008));
        assert_eq!(n64.fold(0xffff_ffff_a440_0010), Some(0x0440_0010), "a sign-extended address");
        assert_eq!(n64.fold(0xb3ff_0014), Some(ISVIEWER_LENGTH));
        assert_eq!(n64.fold(0x8010_0000), None);
        assert_eq!(n64.fold(0xa010_0000), None, "the uncached mirror of RDRAM is memory");
        assert_eq!(n64.fold(0x0430_0008), None, "the user's segment goes through the TLB");
    }

    #[test]
    fn the_boot_code_s_report_is_in_the_rsp_s_data_memory() {
        let (mut n64, mut ram, _) = machine();
        n64.start(&mut ram, (RDRAM, 0x40_0000));
        assert_eq!(n64.read(&mut ram, SP_MEMORY, 32), 0x40_0000);
        assert_eq!(n64.read(&mut ram, SP_MEMORY + 9, 8), 1);
        assert_eq!(n64.read(&mut ram, 0x0404_0010, 32) & 1, 1, "the RSP is halted");
    }

    #[test]
    fn the_isviewer_prints_its_buffer() {
        let (mut n64, mut ram, capture) = machine();
        n64.write(&mut ram, ISVIEWER_BUFFER, 32, u64::from(u32::from_be_bytes(*b"hi\n\0")));
        n64.write(&mut ram, ISVIEWER_LENGTH, 32, 3);
        assert_eq!(capture.lock().console, b"hi\n");
        assert_eq!(n64.read(&mut ram, ISVIEWER_BUFFER + 1, 8), u64::from(b'i'), "a domain keeps what is written");
    }

    #[test]
    fn a_frame_of_cycles_shows_the_buffer_and_raises_the_line_the_mask_lets_through() {
        let (mut n64, mut ram, capture) = machine();
        // 320 by 240 at 16 bits from 0x1000: the first pixel red, the second line's first blue.
        ram.write(RDRAM + 0x1000, &0xf801u16.to_be_bytes());
        ram.write(RDRAM + 0x1000 + 640, &0x003fu16.to_be_bytes());
        for (register, value) in
            [(0x00, 0x3202), (0x04, 0x1000), (0x08, 320), (0x24, 0x006c_02ec), (0x28, 0x0025_01ff), (0x30, 0x200), (0x34, 0x400)]
        {
            n64.write(&mut ram, VI + register, 32, value);
        }
        n64.advance(&mut ram, FRAME - 1).unwrap();
        assert!(!n64.interrupt());
        n64.advance(&mut ram, 1).unwrap();
        let last = capture.lock().last.clone();
        assert_eq!((last.width, last.height), (320, 237));
        assert_eq!((last.pixels[0], last.pixels[1], last.pixels[320]), (0x00ff_0000, 0, 0x0000_00ff));
        assert_ne!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::VI), 0);
        assert!(!n64.interrupt(), "the mask holds it back");
        n64.write(&mut ram, 0x0430_000c, 32, 0x80);
        assert!(n64.interrupt());
        n64.write(&mut ram, VI + 0x10, 32, 0);
        assert!(!n64.interrupt(), "a write of the current line acknowledges it");
    }

    #[test]
    fn the_peripheral_interface_copies_the_cartridge_into_rdram() {
        let (mut n64, mut ram, _) = machine();
        n64.cartridge((0..=255).collect());
        n64.write(&mut ram, PI, 32, 0x2000);
        n64.write(&mut ram, PI + 4, 32, 0x1000_0010);
        n64.write(&mut ram, PI + 0xc, 32, 7);
        assert_eq!(ram.bytes(RDRAM + 0x2000, 8), (0x10..0x18).collect::<Vec<u8>>());
        assert_ne!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::PI), 0);
        n64.write(&mut ram, PI + 0x10, 32, 2);
        assert_eq!(n64.read(&mut ram, 0x0430_0008, 32), 0);
        assert_eq!(n64.read(&mut ram, 0x1000_0020, 32), 0x2021_2223, "the ROM reads through the domain");
    }

    #[test]
    fn the_pif_answers_a_controller_s_status_and_its_buttons() {
        let pressed = pad::State { buttons: pad::CROSS | pad::START | pad::UP, sticks: [0x80, 0x80, 0xff, 0x00] };
        let capture = Capture::new().press(0, 0, pressed);
        let (mut n64, mut ram) = (N64::new(Host::headless(&capture)), Ram::new(RDRAM, 0x1000));
        // A status request and a read on the first port, a read on the empty second, the end.
        let mut frame = [0u8; 64];
        frame[..16].copy_from_slice(&[0xff, 1, 3, 0x00, 0xff, 0xff, 0xff, 1, 4, 0x01, 0xff, 0xff, 0xff, 0xff, 0xfe, 0]);
        frame[63] = 1;
        ram.write(RDRAM + 0x100, &frame);
        n64.write(&mut ram, SI, 32, 0x100);
        n64.write(&mut ram, SI + 0x10, 32, 0);
        n64.write(&mut ram, SI + 4, 32, 0);
        let answer = ram.bytes(RDRAM + 0x100, 64);
        assert_eq!(answer[4..7], [0x05, 0x00, 0x02], "a controller with an empty slot");
        assert_eq!(answer[8] & 0x80, 0x80, "nothing is on the second port");
        assert_ne!(n64.read(&mut ram, SI + 0x18, 32) & 0x1000, 0, "the transfer's interrupt");
        // The same read on the first port.
        frame[..8].copy_from_slice(&[0xff, 1, 4, 0x01, 0xff, 0xff, 0xff, 0xff]);
        frame[8] = 0xfe;
        ram.write(RDRAM + 0x100, &frame);
        n64.write(&mut ram, SI + 0x10, 32, 0);
        n64.write(&mut ram, SI + 4, 32, 0);
        let answer = ram.bytes(RDRAM + 0x100, 64);
        assert_eq!(answer[4..8], [0x98, 0x00, 79, 80], "A, start and up; the stick right and up");
    }

    #[test]
    fn the_audio_interface_plays_a_buffer_and_holds_one_more() {
        let (mut n64, mut ram, capture) = machine();
        for (i, sample) in [100i16, -100, 200, -200].into_iter().enumerate() {
            ram.write(RDRAM + 0x400 + 2 * i as u64, &sample.to_be_bytes());
        }
        // 48681812 / 1104 is 44095 Hz.
        n64.write(&mut ram, AI + 0x10, 32, 1103);
        n64.write(&mut ram, AI + 8, 32, 1);
        n64.write(&mut ram, AI, 32, 0x400);
        n64.write(&mut ram, AI + 4, 32, 8);
        {
            let kept = capture.lock();
            assert_eq!((kept.samples.as_slice(), kept.rate), (&[[100, -100], [200, -200]][..], 44_095));
        }
        assert_ne!(n64.read(&mut ram, AI + 0xc, 32) & 0x4000_0000, 0, "a buffer plays");
        n64.write(&mut ram, AI + 4, 32, 8);
        assert_ne!(n64.read(&mut ram, AI + 0xc, 32) & 0x8000_0000, 0, "and one waits: the interface is full");
        n64.write(&mut ram, AI + 0xc, 32, 0);
        // Two samples last a few thousand cycles each time.
        n64.advance(&mut ram, 2 * CLOCK / 44_095 + 1).unwrap();
        assert_eq!(n64.read(&mut ram, AI + 0xc, 32) & 0x8000_0000, 0);
        assert_ne!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::AI), 0, "the second buffer starts");
        n64.advance(&mut ram, 2 * CLOCK / 44_095 + 1).unwrap();
        assert_eq!(n64.read(&mut ram, AI + 0xc, 32) & 0x4000_0000, 0, "both are played out");
    }

    #[test]
    fn a_task_started_on_the_rsp_ends_at_once_with_its_interrupt() {
        let (mut n64, mut ram, _) = machine();
        // Interrupt at a break, then clear the halt.
        n64.write(&mut ram, 0x0404_0010, 32, 0x100);
        n64.write(&mut ram, 0x0404_0010, 32, 0x1);
        assert_eq!(n64.read(&mut ram, 0x0404_0010, 32) & 3, 3, "halted and broken off");
        assert_ne!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::SP), 0);
        n64.write(&mut ram, 0x0404_0010, 32, 0x8);
        assert_eq!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::SP), 0);
    }

    #[test]
    fn the_rdp_s_full_sync_raises_its_interrupt() {
        let (mut n64, mut ram, _) = machine();
        // A fill rectangle, a textured rectangle of two words, the full sync.
        for (i, word) in [0x3600_0000u32, 0, 0x2400_0000, 0, 0, 0, 0x2900_0000, 0].into_iter().enumerate() {
            ram.write(RDRAM + 0x800 + 4 * i as u64, &word.to_be_bytes());
        }
        n64.write(&mut ram, DP, 32, 0x800);
        n64.write(&mut ram, DP + 4, 32, 0x820);
        assert_ne!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::DP), 0);
        n64.write(&mut ram, MI, 32, 0x800);
        assert_eq!(n64.read(&mut ram, 0x0430_0008, 32) & u64::from(intr::DP), 0);
    }
}
