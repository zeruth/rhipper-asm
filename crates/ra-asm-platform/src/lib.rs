//! The platform boundary of a console's program, and the runtimes behind it.
//!
//! A console program is half software and half machine. Everything it does to the machine crosses
//! one boundary of three events, the same for every console:
//!
//! - `call`: a function of the BIOS or the kernel, by its name, on its register arguments;
//! - `access`: a read or a write of a device register, at the address the memory map folds it to;
//! - `deliver`: what the machine does to the program (an interrupt handler, a callback) at a point
//!   of the machine's clock.
//!
//! A [`Platform`] answers the three for one console. It is no emulator: the program's own code runs
//! as the interpreter or the port runs it, and only the devices and the kernel are here. What a
//! runtime needs of the program is its memory (a DMA transfer, a string argument), which it reaches
//! through [`Memory`], and what it needs of this machine is a picture, sound, input and files,
//! which it reaches through [`Host`].
//!
//! The parts every console shares are modules of their own: the clock and its scheduled events
//! ([`clock`]), the software rasterizer ([`raster`]), the controllers ([`pad`]), the ADPCM of the
//! sound processors ([`adpcm`]), the disc ([`iso`]), the frame buffer a video output shows
//! ([`scanout`]) and the host with its headless form ([`host`]). A console is a module that lays
//! its devices over them and says how its memory lies ([`Map`]), which is all a carrier needs to
//! put the machine behind a program: [`psx`], [`ps2`], [`n64`], [`psp`], [`dreamcast`].
// A renderer and a mixer turn pixel counts and fixed-point coordinates into floats and back on
// every line; the values are screen coordinates and sample levels, far inside a float's range.
#![allow(clippy::cast_precision_loss)]
// The devices' arithmetic is written as their documents give it (a blend is `(B + F) / 2`), and a
// register decode keeps the arms of two registers apart though they do the same.
#![allow(clippy::manual_midpoint, clippy::match_same_arms)]
// The consoles' names are names, not code.
#![allow(clippy::doc_markdown)]

pub mod adpcm;
pub mod clock;
/// Where a run's time goes among a console's devices, when `RA_RUN_STATS` is set: the seconds
/// spent drawing, mixing sound and showing frames, which a run prints beside its own.
pub mod profile {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    /// What the time is counted under.
    #[derive(Clone, Copy)]
    pub enum Part {
        Draw,
        Sound,
        Show,
    }

    static NANOS: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("RA_RUN_STATS").is_some())
    }

    /// The clock's reading, when the times are kept.
    pub fn start() -> Option<Instant> {
        on().then(Instant::now)
    }

    /// Counts the time since `started` under `part`.
    pub fn end(part: Part, started: Option<Instant>) {
        if let Some(started) = started {
            NANOS[part as usize].fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// What the run's thread is doing, as the carrier and the devices say when they change
    /// it: a sampler reads it a thousand times a second ([`sample`]).
    static PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    static SAMPLES: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
    /// The phases by number: the program's compiled code, the calls back from it into the
    /// run, the processor for the code no function has, the look at the machine, drawing.
    pub const PHASES: [&str; 8] = [
        "compiled code",
        "block ticks",
        "the processor",
        "looks at the machine",
        "drawing",
        "loads through the run",
        "stores through the run",
        "the runtime's functions",
    ];

    /// What the phase is about (the address of a load, the number of a function), which the
    /// sampler counts with the phase.
    static DETAIL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static DETAILS: std::sync::Mutex<Option<std::collections::HashMap<(u8, u32), u64>>> = std::sync::Mutex::new(None);

    /// Says what the phase from here on is about.
    pub fn detail(of: u32) {
        DETAIL.store(of, Ordering::Relaxed);
    }

    /// The samples of the phases that say what they are about, the most first: each a phase's
    /// number, what it was about and the samples.
    pub fn details() -> Vec<(u8, u32, u64)> {
        let map = DETAILS.lock().ok().and_then(|d| d.clone()).unwrap_or_default();
        let mut all: Vec<(u8, u32, u64)> = map.into_iter().map(|((phase, of), n)| (phase, of, n)).collect();
        all.sort_unstable_by_key(|&(_, _, n)| std::cmp::Reverse(n));
        all
    }

    /// Says what the run's thread does from here on, and gives what it did before.
    pub fn phase(now: u8) -> u8 {
        PHASE.swap(now, Ordering::Relaxed)
    }

    /// Starts the sampler, when the times are kept: a thread that notes the phase every
    /// millisecond until the process ends.
    pub fn sample() {
        if on() {
            std::thread::spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    let phase = PHASE.load(Ordering::Relaxed).min(7);
                    SAMPLES[usize::from(phase)].fetch_add(1, Ordering::Relaxed);
                    if phase >= 5
                        && let Ok(mut details) = DETAILS.lock()
                    {
                        let of = DETAIL.load(Ordering::Relaxed);
                        *details.get_or_insert_with(Default::default).entry((phase, of)).or_default() += 1;
                    }
                }
            });
        }
    }

    /// The samples taken in each phase.
    pub fn samples() -> Vec<(&'static str, u64)> {
        PHASES.iter().enumerate().map(|(n, name)| (*name, SAMPLES[n].load(Ordering::Relaxed))).collect()
    }

    /// The seconds spent drawing, mixing sound and showing frames.
    pub fn seconds() -> [f64; 3] {
        [0, 1, 2].map(|part| NANOS[part].load(Ordering::Relaxed) as f64 / 1e9)
    }
}

pub mod disc;
pub mod dreamcast;
pub mod host;
pub mod iso;
pub mod n64;
pub mod pad;
pub mod png;
pub mod printf;
pub mod ps2;
pub mod psp;
pub mod psx;
pub mod raster;
pub mod scanout;
#[cfg(windows)]
pub mod window;

pub use host::Host;

/// Whether the runtimes say on stderr what the program asks of their devices and what they
/// answer: with `RA_PLATFORM_TRACE` set, for finding where a program and a device disagree.
pub fn tracing() -> bool {
    static TRACING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACING.get_or_init(|| std::env::var_os("RA_PLATFORM_TRACE").is_some())
}

/// Why a platform ends the program's run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The program ended itself, or the person closed its window.
    Exit(i32),
    /// The program asked the platform for a thing it does not do: the message names it.
    Fault(String),
}

impl Stop {
    pub fn fault(message: impl Into<String>) -> Self {
        Self::Fault(message.into())
    }
}

/// The program's memory, by the addresses the program uses. An access the memory does not hold
/// answers `false`, and a runtime treats it as the console's open bus.
pub trait Memory {
    fn read(&mut self, address: u64, buf: &mut [u8]) -> bool;
    fn write(&mut self, address: u64, bytes: &[u8]) -> bool;
}

/// The little-endian words and the strings of a [`Memory`]; an address it does not hold reads as
/// zero and drops a write.
pub trait MemoryExt: Memory {
    fn u8(&mut self, address: u64) -> u8 {
        let mut b = [0; 1];
        self.read(address, &mut b);
        b[0]
    }

    fn u16(&mut self, address: u64) -> u16 {
        let mut b = [0; 2];
        self.read(address, &mut b);
        u16::from_le_bytes(b)
    }

    fn u32(&mut self, address: u64) -> u32 {
        let mut b = [0; 4];
        self.read(address, &mut b);
        u32::from_le_bytes(b)
    }

    fn u64(&mut self, address: u64) -> u64 {
        let mut b = [0; 8];
        self.read(address, &mut b);
        u64::from_le_bytes(b)
    }

    fn set_u8(&mut self, address: u64, value: u8) {
        self.write(address, &[value]);
    }

    fn set_u16(&mut self, address: u64, value: u16) {
        self.write(address, &value.to_le_bytes());
    }

    fn set_u32(&mut self, address: u64, value: u32) {
        self.write(address, &value.to_le_bytes());
    }

    fn set_u64(&mut self, address: u64, value: u64) {
        self.write(address, &value.to_le_bytes());
    }

    /// The word at `address` of a console whose words are in `order`.
    fn word(&mut self, address: u64, order: Order) -> u32 {
        let mut b = [0; 4];
        self.read(address, &mut b);
        match order {
            Order::Little => u32::from_le_bytes(b),
            Order::Big => u32::from_be_bytes(b),
        }
    }

    fn set_word(&mut self, address: u64, value: u32, order: Order) {
        match order {
            Order::Little => self.write(address, &value.to_le_bytes()),
            Order::Big => self.write(address, &value.to_be_bytes()),
        };
    }

    fn bytes(&mut self, address: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        self.read(address, &mut out);
        out
    }

    /// The bytes of the NUL-terminated string at `address`, `most` at the longest.
    fn cstr(&mut self, address: u64, most: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for at in address..address + most as u64 {
            let mut b = [0; 1];
            if !self.read(at, &mut b) || b[0] == 0 {
                break;
            }
            out.push(b[0]);
        }
        out
    }
}

impl<M: Memory + ?Sized> MemoryExt for M {}

/// A plain slice as a [`Memory`] that starts at an address: a test's, or a port's RAM.
pub struct Ram {
    pub base: u64,
    pub bytes: Vec<u8>,
}

impl Ram {
    pub fn new(base: u64, len: usize) -> Self {
        Self { base, bytes: vec![0; len] }
    }

    fn range(&self, address: u64, len: usize) -> Option<std::ops::Range<usize>> {
        let at = usize::try_from(address.checked_sub(self.base)?).ok()?;
        (at.checked_add(len)? <= self.bytes.len()).then_some(at..at + len)
    }
}

impl Memory for Ram {
    fn read(&mut self, address: u64, buf: &mut [u8]) -> bool {
        match self.range(address, buf.len()) {
            Some(range) => {
                buf.copy_from_slice(&self.bytes[range]);
                true
            }
            None => false,
        }
    }

    fn write(&mut self, address: u64, bytes: &[u8]) -> bool {
        match self.range(address, bytes.len()) {
            Some(range) => {
                self.bytes[range].copy_from_slice(bytes);
                true
            }
            None => false,
        }
    }
}

/// A run of device registers in the console's memory map, at the addresses the map folds them to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub name: &'static str,
    pub start: u64,
    pub len: u64,
}

impl Block {
    pub const fn new(name: &'static str, start: u64, len: u64) -> Self {
        Self { name, start, len }
    }

    pub fn holds(&self, address: u64) -> bool {
        address >= self.start && address - self.start < self.len
    }
}

/// The order of the bytes of a word in a console's memory and on its bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    #[default]
    Little,
    Big,
}

impl Order {
    /// The low `buf.len()` bytes of `value` as the console's memory holds them.
    pub fn put(self, value: u64, buf: &mut [u8]) {
        let len = buf.len().min(8);
        match self {
            Self::Little => buf[..len].copy_from_slice(&value.to_le_bytes()[..len]),
            Self::Big => buf[..len].copy_from_slice(&value.to_be_bytes()[8 - len..]),
        }
    }

    /// The value the first eight bytes of `bytes` at the most are, as the console's memory holds
    /// one.
    pub fn get(self, bytes: &[u8]) -> u64 {
        let len = bytes.len().min(8);
        let mut word = [0u8; 8];
        match self {
            Self::Little => {
                word[..len].copy_from_slice(&bytes[..len]);
                u64::from_le_bytes(word)
            }
            Self::Big => {
                word[8 - len..].copy_from_slice(&bytes[..len]);
                u64::from_be_bytes(word)
            }
        }
    }
}

/// Memory a console has beside the RAM its program is in (a scratchpad, the video memory), at
/// its own address and at each address that mirrors it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extra {
    pub name: &'static str,
    pub start: u64,
    pub len: u64,
    pub mirrors: &'static [u64],
}

/// How a console's memory lies, which is all a carrier has to know of the console to put its
/// machine behind a program: where the device registers answer, where the RAM answers again,
/// and what memory there is beside the RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Map {
    pub order: Order,
    /// Every run of addresses the device registers answer at, each mirror a run of its own. An
    /// access in one goes to [`Platform::fold`] and then to [`Platform::read`] or
    /// [`Platform::write`].
    pub ports: &'static [Block],
    /// The first address of each segment the RAM answers through beside the one the program is
    /// linked in: the RAM is at the same place of each.
    pub ram: &'static [u64],
    /// How many times in a row the RAM answers in each segment: once, or more where the
    /// console's address decoder leaves the RAM's upper address lines out.
    pub repeats: u64,
    pub extras: &'static [Extra],
}

/// Code of the program the machine runs for it: an interrupt handler or a callback, entered as a
/// call with its arguments in the first argument registers, on a stack of its own when the
/// program gave it one. The run that carries the program makes the call and reports its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub entry: u64,
    pub args: [u64; 4],
    /// The stack pointer the code starts with; `None` for the stack of the code it interrupts.
    pub stack: Option<u64>,
    /// The global pointer the code starts with, when the program registered one beside it.
    pub gp: Option<u64>,
    /// Other registers the code starts with, by their numbers: those a `longjmp` restores.
    pub registers: Vec<(u32, u64)>,
    /// What the platform reads the end of the call by.
    pub token: u64,
}

/// What a call on the BIOS or the kernel comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The function's result, as its result register holds it.
    Value(u64),
    /// The function waits for the machine (an event, a semaphore, a vertical blank): the run lets
    /// the clock go on, makes the deliveries that come due, and asks [`Platform::resume`] until
    /// the wait is over.
    Wait,
    /// The function leaves the handler the run is delivering (`ReturnFromException`): the
    /// delivery ends here, whatever the handler's frames hold.
    Leave,
}

/// One console's side of the boundary.
/// The stacks a console keeps in its memory: see [`Platform::stacks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stacks {
    /// The program's stack pointer at its entry.
    pub program: u64,
    /// The stack pointer of the outermost call the machine makes into the program.
    pub handlers: u64,
    /// How far below the last a nested call's stack starts.
    pub nested: u64,
}

pub trait Platform {
    /// The console's name, as `Image::platform` gives it.
    fn name(&self) -> &'static str;

    /// The device registers the console answers, in address order.
    fn blocks(&self) -> &'static [Block];

    /// How the console's memory lies.
    fn map(&self) -> Map;

    /// What the console's own start leaves for the program before its first instruction (a boot
    /// program's report, a kernel's tables), with the RAM the carrier gave the program: its
    /// first address and its length.
    fn start(&mut self, _memory: &mut dyn Memory, _ram: (u64, u64)) {}

    /// The cartridge in the console's slot: the bytes of its ROM, in the cartridge's own order.
    fn cartridge(&mut self, _rom: Vec<u8>) {}

    /// The register an address of the program reaches through the console's mirrors; `None` for
    /// memory.
    fn fold(&self, address: u64) -> Option<u64>;

    /// The device register at `register`, read at `bits` wide.
    fn read(&mut self, memory: &mut dyn Memory, register: u64, bits: u32) -> u64;

    /// `value` written to the device register at `register`, `bits` wide.
    fn write(&mut self, memory: &mut dyn Memory, register: u64, bits: u32, value: u64);

    /// The BIOS or kernel function `name` on its register arguments.
    fn call(&mut self, memory: &mut dyn Memory, name: &str, args: &[u64]) -> Result<Answer, Stop>;

    /// The result of the function that answered [`Answer::Wait`], once what it waits for came.
    fn resume(&mut self, memory: &mut dyn Memory) -> Option<u64>;

    /// Lets the machine's clock run `cycles` on: the devices do what they do in that time, and what
    /// they raise is queued for [`Platform::deliver`].
    fn advance(&mut self, memory: &mut dyn Memory, cycles: u64) -> Result<(), Stop>;

    /// The cycles until the machine next does something of its own accord (a vertical blank, an
    /// answer of a device): what a program that only waits can be moved on by.
    fn idle(&self) -> u64;

    /// Where a program's stack starts when its image names none, and where the stack of the
    /// machine's calls into the program does, with the room each nested call has: both in the
    /// console's own memory, since a program hands addresses on its stack to the devices (a
    /// packet for the GPU built in a local). `None` leaves the stacks to the carrier.
    fn stacks(&self) -> Option<Stacks> {
        None
    }

    /// The level of the processor's interrupt line: whether a device asks for an interrupt the
    /// program's controller lets through. A run whose program is the console's own kernel takes
    /// the interrupt itself, as the processor does, and asks this in place of
    /// [`Platform::deliver`].
    fn interrupt(&self) -> bool {
        false
    }

    /// The next call the machine makes into the program, when one is due and the program takes it.
    fn deliver(&mut self, memory: &mut dyn Memory) -> Option<Delivery>;

    /// The end of a delivered call, with the result register's value.
    fn delivered(&mut self, memory: &mut dyn Memory, delivery: &Delivery, result: u64);

    fn host(&mut self) -> &mut Host;
}

/// The runtime of the console `name` over `host`; `None` for a console without one.
pub fn of(name: &str, host: Host) -> Option<Box<dyn Platform>> {
    match name {
        "psx" => Some(Box::new(psx::Psx::new(host))),
        "ps2" => Some(Box::new(ps2::Ps2::new(host))),
        "n64" => Some(Box::new(n64::N64::new(host))),
        "psp" => Some(Box::new(psp::Psp::new(host))),
        "dreamcast" => Some(Box::new(dreamcast::Dreamcast::new(host))),
        _ => None,
    }
}

/// The consoles [`of`] has a runtime for.
pub const NAMES: [&str; 5] = ["psx", "ps2", "n64", "psp", "dreamcast"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_goes_to_bytes_and_back_in_either_order() {
        let mut buf = [0u8; 2];
        Order::Little.put(0x1234_5678, &mut buf);
        assert_eq!(buf, [0x78, 0x56]);
        Order::Big.put(0x1234_5678, &mut buf);
        assert_eq!(buf, [0x56, 0x78]);
        assert_eq!(Order::Big.get(&[0x12, 0x34, 0x56, 0x78]), 0x1234_5678);
        assert_eq!(Order::Little.get(&[0x12, 0x34, 0x56, 0x78]), 0x7856_3412);
    }

    #[test]
    fn every_console_named_has_a_runtime_whose_ports_fold_to_registers() {
        for name in NAMES {
            let platform = of(name, Host::headless(&host::Capture::new())).unwrap_or_else(|| panic!("{name} has no runtime"));
            assert_eq!(platform.name(), name);
            let map = platform.map();
            assert!(map.repeats >= 1, "{name}");
            for port in map.ports {
                assert!(platform.fold(port.start).is_some(), "{name}: {:#x} folds to no register", port.start);
            }
            // Every block is reached through a run of ports.
            for block in platform.blocks() {
                let reached =
                    |port: &Block| platform.fold(port.start).is_some_and(|first| block.start >= first && block.start - first < port.len);
                assert!(map.ports.iter().any(reached), "{name}: no port reaches {}", block.name);
            }
        }
    }
}
