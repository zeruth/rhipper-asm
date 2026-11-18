//! A console program's machine in a run: the platform runtime of its console (`ra-asm-platform`)
//! behind the interpreter. The device registers are devices of the interpreter's memory that
//! read the runtime and queue their writes for it; the BIOS and kernel calls the lift named go to
//! it; and between the program's instructions the run moves the machine's clock on by the
//! instructions run and makes the calls into the program the machine has due (its interrupt
//! handlers), each to its end before the program goes on.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use ra_asm_ir::Type;
use ra_asm_ir::interp::{Stop, Value, Vm};
use ra_asm_pe::Image;
use ra_asm_platform::host::{Capture, Directory, Host};
use ra_asm_platform::{Answer, Delivery, Platform};

use super::host::Runtime;

/// How a console program's machine meets this one.
#[derive(Clone, Default)]
pub struct Console {
    /// Keep the picture, the sound and the console's text here and show nothing: a headless run,
    /// which ends at the frame the capture names. Without one the picture is a window where the
    /// host has one.
    pub capture: Option<Capture>,
    /// The disc in the drive: an ISO 9660 image, or the directory of its files.
    pub disc: Option<PathBuf>,
    /// Where the files of the console's other devices (the host's, the memory cards') are.
    pub files: Option<PathBuf>,
}

impl std::fmt::Debug for Console {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Console").field("headless", &self.capture.is_some()).field("disc", &self.disc).field("files", &self.files).finish()
    }
}

/// The cycles of the console's clock an instruction of its processor takes: two, the mean the
/// processors of these consoles come to with their load delays and cache misses.
const CYCLES_PER_INSTRUCTION: u64 = 2;
/// The instructions between two looks at the machine.
pub(super) const POLL: u64 = 256;
/// The cycles the clock moves at a time while a call waits for the machine.
const WAIT_QUANTUM: u64 = 4096;
/// The stack the machine's calls into the program run on when the program gave none.
const HANDLER_STACK: u64 = 64 << 10;
/// What ends a handler that leaves through the kernel, as a fault the delivery reads.
const LEFT: &str = "the handler left through the kernel";

/// A register write a device took, for the runtime to take after the instruction.
type Writes = Rc<RefCell<Vec<(u64, Vec<u8>)>>>;

pub(super) struct Machine {
    platform: Rc<RefCell<Box<dyn Platform>>>,
    writes: Writes,
    /// What a headless run keeps, the console's text with it.
    pub(super) capture: Capture,
    /// The processor's instructions counted into the machine's clock so far.
    counted: u64,
    /// The states and the stack of the handlers, made on first use; the depth of the calls open.
    states: Vec<u64>,
    stack: u64,
    depth: usize,
}

/// The device registers of a console in the interpreter's memory, one mirror of them.
struct Ports {
    platform: Rc<RefCell<Box<dyn Platform>>>,
    writes: Writes,
}

/// The program's RAM seen through another segment of its console's address space: the bytes at
/// `start` are those at `target` of the RAM the run mapped, which is the host's own memory at
/// the program's addresses.
struct Mirror {
    start: u64,
    target: u64,
    /// The mapped RAM's first address and its length.
    ram: (u64, u64),
}

impl Mirror {
    /// Where an access of `len` bytes at `address` lands in the RAM, when it lies whole in it.
    fn place(&self, address: u64, len: usize) -> Option<u64> {
        let at = self.target + (address - self.start);
        (at >= self.ram.0 && at + len as u64 <= self.ram.0 + self.ram.1).then_some(at)
    }
}

impl ra_asm_ir::interp::memory::Device for Mirror {
    fn read(&mut self, address: u64, buf: &mut [u8]) {
        match self.place(address, buf.len()) {
            // SAFETY: the range lies in the RAM the run mapped and committed at these addresses.
            Some(at) => unsafe { std::ptr::copy_nonoverlapping(at as usize as *const u8, buf.as_mut_ptr(), buf.len()) },
            None => buf.fill(0),
        }
    }

    fn write(&mut self, address: u64, bytes: &[u8]) {
        if let Some(at) = self.place(address, bytes.len()) {
            // SAFETY: as for a read.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), at as usize as *mut u8, bytes.len()) }
        }
    }
}

/// No memory: what a register read gets, which reads none.
struct Nothing;

impl ra_asm_platform::Memory for Nothing {
    fn read(&mut self, _address: u64, _buf: &mut [u8]) -> bool {
        false
    }

    fn write(&mut self, _address: u64, _bytes: &[u8]) -> bool {
        false
    }
}

impl ra_asm_ir::interp::memory::Device for Ports {
    fn read(&mut self, address: u64, buf: &mut [u8]) {
        let mut platform = self.platform.borrow_mut();
        let value = match platform.fold(address) {
            Some(register) => platform.read(&mut Nothing, register, buf.len().min(8) as u32 * 8),
            None => 0,
        };
        for (byte, source) in buf.iter_mut().zip(value.to_le_bytes()) {
            *byte = source;
        }
    }

    fn write(&mut self, address: u64, bytes: &[u8]) {
        self.writes.borrow_mut().push((address, bytes.to_vec()));
    }
}

/// The interpreter's memory as the runtime reads the program's: an address the console mirrors
/// folds to the place the run mapped, and a device register is no memory.
struct Guest<'a> {
    memory: &'a mut ra_asm_ir::interp::Memory,
    image: &'a Image,
}

impl Guest<'_> {
    fn place(&self, address: u64) -> Option<u64> {
        use ra_asm_format::psx::Region;
        match self.image.memory_region(address) {
            Some((Region::Hardware | Region::StoreQueue, _)) => None,
            Some((_, folded)) => Some(folded),
            None => Some(address),
        }
    }
}

impl ra_asm_platform::Memory for Guest<'_> {
    fn read(&mut self, address: u64, buf: &mut [u8]) -> bool {
        self.place(address).is_some_and(|at| self.memory.read(at, buf).is_ok())
    }

    fn write(&mut self, address: u64, bytes: &[u8]) -> bool {
        self.place(address).is_some_and(|at| self.memory.write(at, bytes).is_ok())
    }
}

fn stop(stop: ra_asm_platform::Stop) -> Stop {
    match stop {
        ra_asm_platform::Stop::Exit(code) => Stop::Exit(code),
        ra_asm_platform::Stop::Fault(message) => Stop::Fault(message),
    }
}

impl Machine {
    /// Gives `host` a window for its picture, its input and its sound, where the system has one;
    /// elsewhere the run stays headless, its console's text written through.
    fn show(host: &mut Host, image: &Image) {
        #[cfg(windows)]
        {
            use ra_asm_platform::window::{Shared, Window};
            if let Some(window) = Window::open(&format!("{} - Rhipper", crate::image_name(image))) {
                let window = Rc::new(RefCell::new(window));
                host.video = Box::new(Shared(window.clone()));
                host.input = Box::new(Shared(window.clone()));
                host.audio = Box::new(Shared(window));
            }
        }
        #[cfg(not(windows))]
        let _ = (host, image);
    }

    /// The machine of `image`'s console, its registers mapped into `vm` at every mirror the
    /// console reaches them through, and the RAM the run mapped at `ram` (its first address and
    /// its length) at the RAM's own mirrors; `None` for a console without a runtime.
    pub(super) fn attach(image: &Image, vm: &mut Vm<'_>, console: &Console, live: bool, ram: (u64, u64)) -> Option<Self> {
        let name = image.platform()?;
        if !ra_asm_platform::NAMES.contains(&name) {
            return None;
        }
        let capture = console.capture.clone().unwrap_or_default();
        // A window's run has nowhere else to show the console's text.
        capture.lock().live |= live || console.capture.is_none();
        let mut host = Host::headless(&capture);
        host.storage = Box::new(Directory::new(console.disc.clone(), console.files.clone()));
        if console.capture.is_none() {
            Self::show(&mut host, image);
        }
        let platform = Rc::new(RefCell::new(ra_asm_platform::of(name, host)?));
        let writes: Writes = Rc::default();
        // Each run of registers at its own address and at its mirrors in the other segments.
        let mirrors: &[u64] = match name {
            "psx" => &[0, 0x8000_0000, 0xa000_0000],
            _ => &[0, 0xa000_0000],
        };
        let blocks = platform.borrow().blocks();
        let (start, end) = (blocks.first()?.start, blocks.last().map(|b| b.start + b.len)?);
        for mirror in mirrors {
            vm.memory.add_device(start | mirror, end - start, Box::new(Ports { platform: platform.clone(), writes: writes.clone() }));
        }
        // The RAM's mirrors, each from the first byte the run mapped, and the scratchpad: the
        // PlayStation's RAM is mapped at `kseg0`, the PlayStation 2's at its own addresses.
        let (segments, scratchpad): (&[u64], (u64, u64)) = match name {
            "psx" => (&[0x0000_0000, 0xa000_0000], (0x1f80_0000, 0x400)),
            _ => (&[0x2000_0000, 0x3000_0000, 0x8000_0000, 0xa000_0000], (ra_asm_platform::ps2::SCRATCHPAD, 0x4000)),
        };
        let within = ram.0 & 0x1fff_ffff;
        for segment in segments {
            vm.memory.add_device(segment + within, ram.1, Box::new(Mirror { start: segment + within, target: ram.0, ram }));
        }
        vm.memory.add_region(ra_asm_ir::interp::Region::owned(scratchpad.0, scratchpad.1, "scratchpad"));
        if name == "psx" {
            let cache = ra_asm_platform::psx::CACHE_CONTROL;
            vm.memory.add_device(cache, 4, Box::new(Ports { platform: platform.clone(), writes: writes.clone() }));
        }
        vm.poll = POLL;
        Some(Self { platform, writes, capture, counted: 0, states: Vec::new(), stack: 0, depth: 0 })
    }
}

impl Runtime<'_> {
    fn machine(&mut self) -> Result<&mut Machine, Stop> {
        self.machine.as_mut().ok_or_else(|| Stop::fault("the program's console has no platform runtime"))
    }

    /// Between two instructions: the register writes the devices queued go to the runtime, the
    /// clock moves on by the instructions run since, and what the machine has due is called.
    pub(super) fn console_poll(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let image = self.image;
        let Some(machine) = &mut self.machine else { return Ok(()) };
        let platform = machine.platform.clone();
        let writes: Vec<(u64, Vec<u8>)> = std::mem::take(&mut machine.writes.borrow_mut());
        let cycles = (vm.instructions - machine.counted) * CYCLES_PER_INSTRUCTION;
        machine.counted = vm.instructions;
        {
            let mut platform = platform.borrow_mut();
            let mut guest = Guest { memory: &mut vm.memory, image };
            for (address, bytes) in writes {
                let Some(register) = platform.fold(address) else { continue };
                let mut word = [0u8; 8];
                let len = bytes.len().min(8);
                word[..len].copy_from_slice(&bytes[..len]);
                platform.write(&mut guest, register, len as u32 * 8, u64::from_le_bytes(word));
            }
            platform.advance(&mut guest, cycles).map_err(stop)?;
        }
        self.deliveries(vm)
    }

    /// Makes every call into the program the machine has due, each to its end.
    fn deliveries(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let image = self.image;
        let platform = self.machine()?.platform.clone();
        loop {
            let due = platform.borrow_mut().deliver(&mut Guest { memory: &mut vm.memory, image });
            let Some(delivery) = due else { return Ok(()) };
            let result = match self.deliver(vm, &delivery) {
                Ok(value) => value,
                Err(Stop::Fault(message)) if message.starts_with(LEFT) => 0,
                Err(other) => return Err(other),
            };
            platform.borrow_mut().delivered(&mut Guest { memory: &mut vm.memory, image }, &delivery, result);
        }
    }

    /// One call of the machine into the program: the function at the delivery's entry with its
    /// arguments, on the stack the delivery names or the handlers' own. A function of the machine
    /// shape gets a state of its own, so the registers of the code it interrupts stay as they are.
    fn deliver(&mut self, vm: &mut Vm<'_>, delivery: &Delivery) -> Result<u64, Stop> {
        let Some(name) = vm.name_at(delivery.entry).map(str::to_owned) else {
            return Err(Stop::fault(format!("the machine calls {:#x}, which is no lifted function", delivery.entry)));
        };
        let machine_shape = vm.function(&name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void);
        if !machine_shape {
            return self.call_back(vm, delivery.entry, [delivery.args[0], delivery.args[1], delivery.args[2]]);
        }
        let depth = self.machine()?.depth;
        if self.machine()?.states.len() <= depth {
            let state = self.new_state(vm)?;
            self.machine()?.states.push(state);
        }
        if self.machine()?.stack == 0 {
            // A state's worth of heap is a block like another: the stack is one of its own size.
            let block = self.new_block(vm, HANDLER_STACK)?;
            self.machine()?.stack = block + HANDLER_STACK - 64;
        }
        let global_pointer = self.image.global_pointer;
        let machine = self.machine()?;
        let state = machine.states[depth];
        // A nested handler runs below the one it interrupts.
        let stack = delivery.stack.unwrap_or(machine.stack - depth as u64 * (HANDLER_STACK / 8));
        machine.depth += 1;
        let registers = [(29, stack), (31, 0), (4, delivery.args[0]), (5, delivery.args[1]), (6, delivery.args[2]), (7, delivery.args[3])];
        let result = (|| {
            if let Some(gp) = delivery.gp.or(global_pointer) {
                vm.memory.write_u64(state + 28 * 8, gp)?;
            }
            for (register, value) in registers {
                // A word in a 64-bit register is held sign-extended.
                vm.memory.write_u64(state + register * 8, i64::from(value as u32 as i32) as u64)?;
            }
            vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
            vm.memory.read_u64(state + 2 * 8)
        })();
        if let Some(machine) = &mut self.machine {
            machine.depth -= 1;
        }
        result
    }

    /// A BIOS or kernel function of the console by its name: answered by the runtime, with the
    /// clock run on and the machine's calls made while the function waits for the machine.
    pub(super) fn console_call(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        // The machine is as far as the program is before the function sees it.
        self.console_poll(vm)?;
        let image = self.image;
        let platform = self.machine()?.platform.clone();
        let args: Vec<u64> = args.iter().map(Value::as_u64).collect();
        let answer = platform.borrow_mut().call(&mut Guest { memory: &mut vm.memory, image }, name, &args).map_err(stop)?;
        let value = match answer {
            Answer::Value(value) => value,
            Answer::Leave => return Err(Stop::fault(LEFT)),
            Answer::Wait => loop {
                let came = {
                    let mut platform = platform.borrow_mut();
                    let mut guest = Guest { memory: &mut vm.memory, image };
                    platform.advance(&mut guest, WAIT_QUANTUM).map_err(stop)?;
                    platform.resume(&mut guest)
                };
                if let Some(value) = came {
                    break value;
                }
                self.deliveries(vm)?;
            },
        };
        // What the function raised is due before the program goes on.
        self.deliveries(vm)?;
        Ok(Value::Int(u128::from(value)))
    }

    /// An access of a device register the raise made a call: the runtime's, at once.
    pub(super) fn console_access(&mut self, vm: &mut Vm<'_>, write: bool, bits: u16, args: &[Value]) -> Result<Value, Stop> {
        self.console_poll(vm)?;
        let image = self.image;
        let platform = self.machine()?.platform.clone();
        let register = args.first().map_or(0, Value::as_u64);
        let value = {
            let mut platform = platform.borrow_mut();
            let mut guest = Guest { memory: &mut vm.memory, image };
            let register = platform.fold(register).unwrap_or(register);
            if write {
                platform.write(&mut guest, register, u32::from(bits), args.get(1).map_or(0, Value::as_u64));
                0
            } else {
                platform.read(&mut guest, register, u32::from(bits))
            }
        };
        if write {
            self.deliveries(vm)?;
        }
        Ok(Value::Int(u128::from(value)))
    }
}
