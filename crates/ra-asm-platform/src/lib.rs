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
//! sound processors ([`adpcm`]), the disc ([`iso`]) and the host with its headless form
//! ([`host`]). A console is a module that lays its devices over them: [`psx`], [`ps2`].
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
pub mod host;
pub mod disc;
pub mod iso;
pub mod pad;
pub mod png;
pub mod printf;
pub mod ps2;
pub mod psx;
pub mod raster;
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
        _ => None,
    }
}

/// The consoles [`of`] has a runtime for.
pub const NAMES: [&str; 2] = ["psx", "ps2"];
