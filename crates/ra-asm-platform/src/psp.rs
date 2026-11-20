//! The PSP, a user module's side: the functions of the kernel's libraries that reach the
//! machine (the display, the controller, the sound channels, the clock), over the host. A user
//! module sees no device register: everything it does to the machine is a call, so this console
//! is its calls alone, and the video memory the display shows.
//!
//! The display shows the frame buffer the program names with `sceDisplaySetFrameBuf`, in the
//! video memory or wherever the program keeps it, at each vertical blank; the controller is the
//! host's pad; a sound channel plays the buffers it is handed.
//!
//! What is not here yet is the graphics engine: a display list is not run, so a program that
//! draws through `sceGu` stops at its first list. The threads, the semaphores and the memory
//! blocks are the carrier's, which runs the program's threads.

use crate::clock::Scheduler;
use crate::host::{Frame, Host};
use crate::scanout::{Format, Scan};
use crate::{Answer, Block, Delivery, Extra, Map, Memory, MemoryExt, Order, Platform, Stop, pad};

/// The video memory, 2 MiB, which a program reaches through the uncached mirror too.
pub const VRAM: u64 = 0x0400_0000;
pub const VRAM_SIZE: u64 = 0x20_0000;
/// The picture's size.
pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 272;
/// The processor's cycles in a second at the clock a game starts with, and in a frame; the
/// blank is the fourteen lines of the 286 the picture does not use.
pub const CLOCK: u64 = 222_000_000;
const FRAME: u64 = CLOCK * 1001 / 60_000;
const BLANK: u64 = FRAME * 14 / 286;
/// The rate of a sound channel.
const RATE: u32 = 44_100;
/// The sound channels a program reserves, and the one of the output at a rate of its own.
const CHANNELS: usize = 8;
const SRC: usize = CHANNELS;
/// The full volume of a channel's side.
const VOLUME: i32 = 0x8000;
/// `SCE_ERROR_AUDIO_CHANNEL_BUSY` and the two a wrong channel gets.
const CHANNEL_BUSY: u64 = 0x8026_0002;
const INVALID_CHANNEL: u64 = 0x8026_0003;
const NO_CHANNELS: u64 = 0x8026_0005;
/// The ids of the callbacks a program creates.
const CALLBACKS: u64 = 0x0450_0000;

/// The controller's buttons as `SceCtrlData` holds them for what the host's pad holds: the
/// same bits, but the first shoulder buttons, which are the PSP's two triggers.
pub fn buttons(pad: &pad::State) -> u32 {
    u32::from(pad.buttons & 0xf0f9) | (u32::from(pad.buttons & (pad::L1 | pad::R1)) >> 2)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    Blank,
}

/// What a call that waits waits for.
#[derive(Debug, Clone, Copy)]
enum Wait {
    /// The next vertical blank after this many frames.
    Blank(u64),
    /// The clock at this cycle, and the call's result then.
    Until(u64, u64),
}

/// A sound channel a program reserved.
#[derive(Debug, Clone, Copy, Default)]
struct Channel {
    reserved: bool,
    samples: u32,
    stereo: bool,
    rate: u32,
    /// The cycle the channel has played what it was handed.
    free_at: u64,
}

/// The frame buffer the display shows.
#[derive(Debug, Clone, Copy, Default)]
struct Buffer {
    top: u32,
    width: u32,
    format: u32,
}

pub struct Psp {
    host: Host,
    clock: Scheduler<Event>,
    buffer: Buffer,
    mode: u32,
    frames: u64,
    blank_at: u64,
    wait: Option<Wait>,
    channels: [Channel; CHANNELS + 1],
    sampling: [u32; 2],
    callbacks: u64,
    closed: bool,
}

impl Psp {
    pub fn new(host: Host) -> Self {
        let mut clock = Scheduler::default();
        clock.after(FRAME, Event::Blank);
        Self {
            host,
            clock,
            buffer: Buffer::default(),
            mode: 0,
            frames: 0,
            blank_at: 0,
            wait: None,
            channels: [Channel::default(); CHANNELS + 1],
            sampling: [0; 2],
            callbacks: 0,
            closed: false,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The microseconds since the console started.
    fn micros(&self) -> u64 {
        (u128::from(self.clock.now()) * 1_000_000 / u128::from(CLOCK)) as u64
    }

    /// The picture: the frame buffer in its format, or black while the program named none.
    fn picture(&self, memory: &mut dyn Memory) -> Frame {
        let format = match self.buffer.format {
            0 => Format::BGR565,
            1 => Format::ABGR1555,
            2 => Format::ABGR4444,
            _ => Format::ABGR8888,
        };
        if self.buffer.top == 0 || self.buffer.width == 0 {
            return Frame::new(WIDTH, HEIGHT);
        }
        let address = u64::from(self.buffer.top);
        Scan { address, width: WIDTH, height: HEIGHT, stride: self.buffer.width * format.bytes, format, order: Order::Little }.frame(memory)
    }

    /// `SceCtrlData` at `address`: the time, the buttons (a clear bit for a pressed one in the
    /// negative form) and the stick.
    fn controller(&mut self, memory: &mut dyn Memory, address: u64, negative: bool) {
        let pad = self.host.input.pad(0).unwrap_or_default();
        let held = buttons(&pad);
        memory.set_u32(address, self.micros() as u32);
        memory.set_u32(address + 4, if negative { !held } else { held });
        memory.write(address + 8, &[pad.sticks[2], pad.sticks[3], 0, 0, 0, 0, 0, 0]);
    }

    /// A buffer handed to a sound channel, at a volume a side: played at once to the host, and
    /// the channel is busy for as long as it lasts. A call that blocks waits for the channel to
    /// have played what it had; one that does not is refused while it has.
    fn output(&mut self, memory: &mut dyn Memory, channel: usize, volume: [u64; 2], buffer: u64, blocking: bool) -> Answer {
        let Some(state) = self.channels.get(channel).copied().filter(|c| c.reserved) else { return Answer::Value(INVALID_CHANNEL) };
        let now = self.clock.now();
        if !blocking && state.free_at > now {
            return Answer::Value(CHANNEL_BUSY);
        }
        let width = if state.stereo { 4 } else { 2 };
        let bytes = memory.bytes(buffer, state.samples as usize * width);
        let scale = |sample: i16, volume: u64| (i32::from(sample) * (volume as i32).clamp(0, VOLUME) / VOLUME) as i16;
        let samples: Vec<[i16; 2]> = bytes
            .chunks_exact(width)
            .map(|s| {
                let left = i16::from_le_bytes([s[0], s[1]]);
                let right = if state.stereo { i16::from_le_bytes([s[2], s[3]]) } else { left };
                [scale(left, volume[0]), scale(right, volume[1])]
            })
            .collect();
        self.host.audio.play(&samples, state.rate);
        let starts = state.free_at.max(now);
        self.channels[channel].free_at = starts + u64::from(state.samples) * CLOCK / u64::from(state.rate.max(1));
        if starts > now {
            self.wait = Some(Wait::Until(starts, u64::from(state.samples)));
            Answer::Wait
        } else {
            Answer::Value(u64::from(state.samples))
        }
    }
}

impl Platform for Psp {
    fn name(&self) -> &'static str {
        "psp"
    }

    fn blocks(&self) -> &'static [Block] {
        &[]
    }

    fn map(&self) -> Map {
        const VIDEO: [Extra; 1] = [Extra { name: "vram", start: VRAM, len: VRAM_SIZE, mirrors: &[0x4000_0000 | VRAM] }];
        Map { order: Order::Little, ports: &[], ram: &[], repeats: 1, extras: &VIDEO }
    }

    fn fold(&self, _address: u64) -> Option<u64> {
        None
    }

    fn read(&mut self, _memory: &mut dyn Memory, _register: u64, _bits: u32) -> u64 {
        0
    }

    fn write(&mut self, _memory: &mut dyn Memory, _register: u64, _bits: u32, _value: u64) {}

    #[allow(clippy::too_many_lines)]
    fn call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop> {
        let arg = |n: usize| args.get(n).copied().unwrap_or(0) & 0xffff_ffff;
        let value = match name {
            "sceDisplaySetMode" => {
                self.mode = arg(0) as u32;
                0
            }
            "sceDisplayGetMode" => {
                for (n, value) in [self.mode, WIDTH, HEIGHT].into_iter().enumerate() {
                    memory.set_u32(arg(n), value);
                }
                0
            }
            "sceDisplaySetFrameBuf" => {
                self.buffer = Buffer { top: arg(0) as u32, width: arg(1) as u32, format: arg(2) as u32 };
                0
            }
            "sceDisplayGetFrameBuf" => {
                for (n, value) in [self.buffer.top, self.buffer.width, self.buffer.format].into_iter().enumerate() {
                    memory.set_u32(arg(n), value);
                }
                0
            }
            "sceDisplayWaitVblankStart" | "sceDisplayWaitVblankStartCB" | "sceDisplayWaitVblank" | "sceDisplayWaitVblankCB" => {
                self.wait = Some(Wait::Blank(self.frames));
                return Ok(Answer::Wait);
            }
            "sceDisplayGetVcount" => self.frames & 0xffff_ffff,
            "sceDisplayIsVblank" => u64::from(self.frames > 0 && self.clock.now() - self.blank_at < BLANK),
            "sceGeEdramGetAddr" => VRAM,
            "sceGeEdramGetSize" => VRAM_SIZE,
            "sceCtrlSetSamplingCycle" => u64::from(std::mem::replace(&mut self.sampling[0], arg(0) as u32)),
            "sceCtrlSetSamplingMode" => u64::from(std::mem::replace(&mut self.sampling[1], arg(0) as u32)),
            "sceCtrlGetSamplingCycle" | "sceCtrlGetSamplingMode" => {
                memory.set_u32(arg(0), self.sampling[usize::from(name.ends_with("Mode"))]);
                0
            }
            "sceCtrlReadBufferPositive" | "sceCtrlPeekBufferPositive" | "sceCtrlReadBufferNegative" | "sceCtrlPeekBufferNegative" => {
                if arg(1) == 0 {
                    0
                } else {
                    self.controller(memory, arg(0), name.ends_with("Negative"));
                    1
                }
            }
            "sceAudioChReserve" => {
                // A negative channel asks for any free one.
                let channel = match arg(0) {
                    n if n < CHANNELS as u64 => Some(n as usize).filter(|n| !self.channels[*n].reserved),
                    n if n >= 0x8000_0000 => (0..CHANNELS).rev().find(|n| !self.channels[*n].reserved),
                    _ => return Ok(Answer::Value(INVALID_CHANNEL)),
                };
                match channel {
                    Some(n) => {
                        self.channels[n] = Channel { reserved: true, samples: arg(1) as u32, stereo: arg(2) == 0, rate: RATE, free_at: 0 };
                        n as u64
                    }
                    None => NO_CHANNELS,
                }
            }
            "sceAudioChRelease" => match self.channels.get_mut(arg(0) as usize).filter(|c| c.reserved) {
                Some(channel) => {
                    channel.reserved = false;
                    0
                }
                None => INVALID_CHANNEL,
            },
            "sceAudioSetChannelDataLen" => match self.channels.get_mut(arg(0) as usize).filter(|c| c.reserved) {
                Some(channel) => {
                    channel.samples = arg(1) as u32;
                    0
                }
                None => INVALID_CHANNEL,
            },
            "sceAudioOutput" | "sceAudioOutputBlocking" => {
                return Ok(self.output(memory, arg(0) as usize, [arg(1); 2], arg(2), name.ends_with("Blocking")));
            }
            "sceAudioOutputPanned" | "sceAudioOutputPannedBlocking" => {
                return Ok(self.output(memory, arg(0) as usize, [arg(1), arg(2)], arg(3), name.ends_with("Blocking")));
            }
            // The output at a rate of the program's own: one channel, always of two sides.
            "sceAudioSRCChReserve" | "sceAudioOutput2Reserve" => {
                let rate = if name == "sceAudioSRCChReserve" { arg(1) as u32 } else { RATE };
                self.channels[SRC] = Channel { reserved: true, samples: arg(0) as u32, stereo: true, rate: rate.max(1), free_at: 0 };
                0
            }
            "sceAudioSRCChRelease" | "sceAudioOutput2Release" => {
                self.channels[SRC].reserved = false;
                0
            }
            "sceAudioSRCOutputBlocking" | "sceAudioOutput2OutputBlocking" => {
                return Ok(self.output(memory, SRC, [arg(0); 2], arg(1), true));
            }
            "sceKernelGetSystemTimeLow" | "sceKernelLibcClock" => self.micros() & 0xffff_ffff,
            // A callback is made and never called: nothing here asks a program to exit but the
            // end of its run. The thread that sleeps for its callbacks is done.
            "sceKernelCreateCallback" => {
                self.callbacks += 1;
                CALLBACKS + self.callbacks
            }
            "sceKernelRegisterExitCallback" | "sceKernelSleepThreadCB" | "sceKernelSleepThread" => 0,
            // The caches hold nothing the memory does not.
            "sceKernelDcacheWritebackAll"
            | "sceKernelDcacheWritebackInvalidateAll"
            | "sceKernelDcacheWritebackRange"
            | "sceKernelDcacheWritebackInvalidateRange"
            | "sceKernelDcacheInvalidateRange"
            | "sceKernelIcacheInvalidateAll"
            | "sceKernelIcacheInvalidateRange" => 0,
            _ => return Err(Stop::fault(format!("the PSP kernel function {name} is not interpreted"))),
        };
        Ok(Answer::Value(value))
    }

    fn resume(&mut self, _memory: &mut dyn Memory) -> Option<u64> {
        let value = match self.wait? {
            Wait::Blank(frames) => (self.frames > frames).then_some(0),
            Wait::Until(cycle, value) => (self.clock.now() >= cycle).then_some(value),
        }?;
        self.wait = None;
        Some(value)
    }

    fn advance(&mut self, memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop> {
        if self.closed || !self.host.video.idle() {
            return Err(Stop::Exit(0));
        }
        let mut left = cycles;
        while let Some(Event::Blank) = self.clock.step(&mut left) {
            self.frames += 1;
            self.blank_at = self.clock.now();
            let picture = self.picture(memory);
            if !self.host.video.present(&picture) {
                self.closed = true;
            }
            self.clock.after(FRAME, Event::Blank);
        }
        if self.closed { Err(Stop::Exit(0)) } else { Ok(()) }
    }

    fn idle(&self) -> u64 {
        self.clock.until_next().unwrap_or(0)
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

    fn machine() -> (Psp, Ram, Capture) {
        let capture = Capture::new();
        (Psp::new(Host::headless(&capture)), Ram::new(VRAM, VRAM_SIZE as usize), capture)
    }

    fn value(psp: &mut Psp, ram: &mut Ram, name: &str, args: &[u64]) -> u64 {
        match psp.call(ram, name, args) {
            Ok(Answer::Value(value)) => value,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn the_display_shows_the_frame_buffer_at_the_blank() {
        let (mut psp, mut ram, capture) = machine();
        assert_eq!(value(&mut psp, &mut ram, "sceGeEdramGetAddr", &[]), VRAM);
        // 512 pixels a row of 32 bits: the first pixel red, the second row's first blue.
        ram.set_u32(VRAM, 0xff00_00ff);
        ram.set_u32(VRAM + 512 * 4, 0xffff_0000);
        assert_eq!(value(&mut psp, &mut ram, "sceDisplaySetMode", &[0, 480, 272]), 0);
        assert_eq!(value(&mut psp, &mut ram, "sceDisplaySetFrameBuf", &[VRAM, 512, 3, 1]), 0);
        psp.advance(&mut ram, FRAME).unwrap();
        let last = capture.lock().last.clone();
        assert_eq!((last.width, last.height), (WIDTH, HEIGHT));
        assert_eq!((last.pixels[0], last.pixels[1], last.pixels[480]), (0x00ff_0000, 0, 0x0000_00ff));
        assert_eq!(value(&mut psp, &mut ram, "sceDisplayGetVcount", &[]), 1);
        assert_eq!(value(&mut psp, &mut ram, "sceDisplayIsVblank", &[]), 1);
    }

    #[test]
    fn a_wait_for_the_blank_ends_at_the_next_one() {
        let (mut psp, mut ram, _) = machine();
        assert_eq!(psp.call(&mut ram, "sceDisplayWaitVblankStart", &[]), Ok(Answer::Wait));
        psp.advance(&mut ram, FRAME - 1).unwrap();
        assert_eq!(psp.resume(&mut ram), None);
        psp.advance(&mut ram, 1).unwrap();
        assert_eq!(psp.resume(&mut ram), Some(0));
        assert_eq!(psp.resume(&mut ram), None, "the wait is over");
    }

    #[test]
    fn the_controller_reads_the_host_s_pad() {
        let pressed = pad::State { buttons: pad::CROSS | pad::L1 | pad::R1 | pad::L2, sticks: [0x80, 0x80, 0x10, 0xf0] };
        let capture = Capture::new().press(0, 0, pressed);
        let (mut psp, mut ram) = (Psp::new(Host::headless(&capture)), Ram::new(0x1000, 0x100));
        assert_eq!(value(&mut psp, &mut ram, "sceCtrlSetSamplingMode", &[1]), 0);
        assert_eq!(value(&mut psp, &mut ram, "sceCtrlReadBufferPositive", &[0x1000, 1]), 1);
        assert_eq!(ram.u32(0x1004), 0x4300, "the cross and the two triggers");
        assert_eq!(ram.bytes(0x1008, 2), [0x10, 0xf0]);
        assert_eq!(value(&mut psp, &mut ram, "sceCtrlPeekBufferNegative", &[0x1000, 1]), 1);
        assert_eq!(ram.u32(0x1004), !0x4300);
    }

    #[test]
    fn a_sound_channel_plays_its_buffer_and_a_blocking_call_waits_for_the_last() {
        let (mut psp, mut ram, capture) = machine();
        let channel = value(&mut psp, &mut ram, "sceAudioChReserve", &[0xffff_ffff, 64, 0]);
        assert_eq!(channel, 7, "any channel is the last free");
        for (i, sample) in [1000i16, -1000].into_iter().enumerate() {
            ram.write(VRAM + 2 * i as u64, &sample.to_le_bytes());
        }
        assert_eq!(value(&mut psp, &mut ram, "sceAudioOutputBlocking", &[channel, 0x4000, VRAM]), 64);
        {
            let kept = capture.lock();
            assert_eq!((kept.samples.len(), kept.samples[0], kept.rate), (64, [500, -500], 44_100));
        }
        assert_eq!(value(&mut psp, &mut ram, "sceAudioOutput", &[channel, 0x8000, VRAM]), CHANNEL_BUSY);
        assert_eq!(psp.call(&mut ram, "sceAudioOutputBlocking", &[channel, 0x8000, VRAM]), Ok(Answer::Wait));
        let lasts = 64 * CLOCK / 44_100;
        psp.advance(&mut ram, lasts - 1).unwrap();
        assert_eq!(psp.resume(&mut ram), None);
        psp.advance(&mut ram, 1).unwrap();
        assert_eq!(psp.resume(&mut ram), Some(64));
        assert_eq!(value(&mut psp, &mut ram, "sceAudioChRelease", &[channel]), 0);
        assert_eq!(value(&mut psp, &mut ram, "sceAudioOutput", &[channel, 0x8000, VRAM]), INVALID_CHANNEL);
    }

    #[test]
    fn a_function_the_console_has_not_stops_the_run_with_its_name() {
        let (mut psp, mut ram, _) = machine();
        assert_eq!(
            psp.call(&mut ram, "sceGeListEnQueue", &[]),
            Err(Stop::fault("the PSP kernel function sceGeListEnQueue is not interpreted"))
        );
    }
}
