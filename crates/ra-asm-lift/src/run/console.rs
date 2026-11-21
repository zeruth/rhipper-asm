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
    /// The program's functions as this machine's own code, when a port runs them.
    pub native: Option<Native>,
}

/// What a compiler made of a program's lift ([`crate::port`]): the run calls a function of it
/// where it would have interpreted the function's IR.
#[derive(Clone, Default)]
pub struct Native {
    /// The compiled functions, by the addresses the program has them at.
    pub functions: Vec<(u64, ra_asm_ir::interp::native::Code)>,
    /// The globals of the module the compiled code has copies of its own of, by name, with
    /// where each is: the run reads and writes them there.
    pub globals: Vec<(String, u64)>,
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
/// The polls in a row that find the registers as they were before the program counts as waiting.
const WAITING_POLLS: u32 = 8;
/// How far apart, in bytes of code, the polls of a waiting program find it.
const WAITING_SPAN: u64 = 128;
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
    /// The order of a register's bytes on the console's bus.
    order: ra_asm_platform::Order,
    /// What a headless run keeps, the console's text with it.
    pub(super) capture: Capture,
    /// The processor's instructions counted into the machine's clock so far.
    counted: u64,
    /// The states and the stack of the handlers, made on first use; the depth of the calls open.
    states: Vec<u64>,
    stack: u64,
    depth: usize,
    /// How many polls in a row found the program waiting; whether it called the machine since
    /// the last poll.
    seen_at: usize,
    /// The registers the last polls found, each set as one number.
    registers: std::collections::VecDeque<u64>,
    repeats: u32,
    busy: bool,
    /// Where the last polls found the program, and the memory's count of writes at the last.
    sites: [u64; 8],
    stores: u64,
    loads: u64,
    /// A PlayStation's processor, for the code of its program the lift has no function for.
    pub(super) r3000: Option<super::r3000::Processor>,
}

/// A region of a console's bus with nothing on it: a read finds the bus as nothing drives it.
struct Open;

impl ra_asm_ir::interp::memory::Device for Open {
    fn read(&mut self, _address: u64, buf: &mut [u8]) {
        buf.fill(0xff);
    }

    fn write(&mut self, _address: u64, _bytes: &[u8]) {}
}

/// The device registers of a console in the interpreter's memory, one mirror of them.
struct Ports {
    platform: Rc<RefCell<Box<dyn Platform>>>,
    writes: Writes,
    /// The order of a register's bytes on the console's bus.
    order: ra_asm_platform::Order,
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
        self.order.put(value, buf);
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

/// A command of the PlayStation's GTE: the sixty-four registers of the state's field at the
/// first argument, through the command the second is the word of, and back.
pub(super) fn gte(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, Stop> {
    let (at, command) = (args.first().map_or(0, Value::as_u64), args.get(1).map_or(0, Value::as_u64));
    let mut registers = [0i32; 64];
    for (n, register) in registers.iter_mut().enumerate() {
        *register = vm.memory.read_u32(at + 4 * n as u64)?.cast_signed();
    }
    ra_asm_platform::psx::gte::command(&mut registers, command as u32);
    for (n, register) in registers.into_iter().enumerate() {
        vm.memory.write_u32(at + 4 * n as u64, register.cast_unsigned())?;
    }
    Ok(Value::Int(0))
}

/// Where the code at the VR4300's general exception vector jumps: a program puts a `j` to its
/// handler there, or the handler's address built in a register and a `jr` through it. `None`
/// while the program has put no such code there.
fn vr4300_vector(vm: &mut Vm<'_>) -> Option<u64> {
    const VECTOR: u64 = 0x8000_0180;
    let mut words = [0u32; 3];
    for (n, word) in words.iter_mut().enumerate() {
        *word = u32::from_be_bytes(vm.memory.read_vec(VECTOR + 4 * n as u64, 4).ok()?.try_into().ok()?);
    }
    let [first, second, third] = words;
    if first >> 26 == 2 {
        return Some(u64::from(0x8000_0000 | ((first & 0x03ff_ffff) << 2)));
    }
    // lui r, high; addiu or ori r, r, low; jr r.
    let register = (first >> 16) & 31;
    let same = (second >> 21) & 31 == register && (second >> 16) & 31 == register;
    if first >> 26 != 0xf || !same || third != (register << 21) | 8 {
        return None;
    }
    let (high, low) = ((first & 0xffff) << 16, second & 0xffff);
    match second >> 26 {
        9 => Some(u64::from(high.wrapping_add(i32::from(low as u16 as i16).cast_unsigned()))),
        0xd => Some(u64::from(high | low)),
        _ => None,
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
        let mut platform = ra_asm_platform::of(name, host)?;
        // A cartridge's ROM is the image's file, in the cartridge's own order.
        if let (Some(_), Some(path)) = (&image.n64, &image.path)
            && let Ok(mut rom) = std::fs::read(path)
            && let Some(order) = ra_asm_format::n64::Order::of(&rom)
            && ra_asm_format::n64::to_z64(&mut rom, order).is_ok()
        {
            platform.cartridge(rom);
        }
        let map = platform.map();
        let platform = Rc::new(RefCell::new(platform));
        let writes: Writes = Rc::default();
        // Each run of the registers' addresses, a mirror of them a run of its own.
        for ports in map.ports {
            let device = Ports { platform: platform.clone(), writes: writes.clone(), order: map.order };
            vm.memory.add_device(ports.start, ports.len, Box::new(device));
        }
        // The RAM again in each segment that reaches it, from the first byte the run mapped;
        // and where the console's decoder leaves the RAM's upper address lines out (the
        // PlayStation's 2 MiB answer through the 8 MiB its memory control maps), after itself
        // in every segment, its own with them.
        let within = ram.0 & 0x1fff_ffff;
        let (span, wrap) = if map.repeats > 1 { (map.repeats * ram.1, ram.1 - 1) } else { (ram.1, u64::MAX) };
        for segment in map.ram {
            vm.memory.add_alias(segment + within, span, wrap, ram.0);
        }
        if map.repeats > 1 {
            vm.memory.add_alias(ram.0 + ram.1, span - ram.1, wrap, ram.0);
        }
        // The memory the console has beside the RAM, at its own address and at its mirrors.
        for extra in map.extras {
            vm.memory.add_region(ra_asm_ir::interp::Region::owned(extra.start, extra.len, extra.name));
            for mirror in extra.mirrors {
                vm.memory.add_alias(*mirror, extra.len, u64::MAX, extra.start);
            }
        }
        if name == "psx" {
            // The ROM, the image's own or the one beside it, through the cached segments.
            let rom = match (&image.psx_bios, &image.rom) {
                (Some(_), _) => Some((image.image_base(), u64::from(image.size_of_image()))),
                (None, Some((address, bytes))) => Some((*address, bytes.len() as u64)),
                (None, None) => None,
            };
            if let Some((at, len)) = rom {
                for start in [0x1fc0_0000, 0x9fc0_0000u64] {
                    vm.memory.add_alias(start, len, u64::MAX, at);
                }
            }
            // The first expansion region, where a kernel looks for a cartridge: none is in.
            for segment in [0x0000_0000, 0x8000_0000, 0xa000_0000u64] {
                vm.memory.add_device(segment + 0x1f00_0000, 0x80_0000, Box::new(Open));
            }
        }
        // What the console's own start leaves the program.
        platform.borrow_mut().start(&mut Guest { memory: &mut vm.memory, image }, ram);
        vm.poll = POLL;
        vm.memory.count_stores(ram.0, ram.1);
        let r3000 = if name == "psx" { super::r3000::Processor::new(vm, image.psx_kernel()) } else { None };
        Some(Self {
            r3000,
            platform,
            writes,
            order: map.order,
            capture,
            counted: 0,
            states: Vec::new(),
            stack: 0,
            depth: 0,
            seen_at: 0,
            registers: std::collections::VecDeque::new(),
            repeats: 0,
            busy: false,
            sites: [0; 8],
            stores: 0,
            loads: 0,
        })
    }
}

impl Machine {
    /// The level of the console's interrupt line.
    pub(super) fn interrupt(&self) -> bool {
        self.platform.borrow().interrupt()
    }

    /// Text of the console's TTY, written where the host shows the console's text.
    pub(super) fn tty(&self, text: &[u8]) {
        self.platform.borrow_mut().host().console.write(text);
    }
}

impl Runtime<'_> {
    /// The stacks the console keeps in its own memory, when the program has a console that does.
    pub(super) fn stacks(&self) -> Option<ra_asm_platform::Stacks> {
        self.machine.as_ref().and_then(|machine| machine.platform.borrow().stacks())
    }

    fn machine(&mut self) -> Result<&mut Machine, Stop> {
        self.machine.as_mut().ok_or_else(|| Stop::fault("the program's console has no platform runtime"))
    }

    /// Between two instructions: the register writes the devices queued go to the runtime, the
    /// clock moves on by the instructions run since, and what the machine has due is called.
    pub(super) fn console_poll(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let before = ra_asm_platform::profile::phase(3);
        let result = self.look(vm);
        ra_asm_platform::profile::phase(before);
        result
    }

    fn look(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let image = self.image;
        let idles = self.machine.is_some() && self.psx_idles(vm);
        let Some(machine) = &mut self.machine else { return Ok(()) };
        let (platform, order) = (machine.platform.clone(), machine.order);
        let writes: Vec<(u64, Vec<u8>)> = std::mem::take(&mut machine.writes.borrow_mut());
        let cycles = (vm.instructions - machine.counted) * CYCLES_PER_INSTRUCTION;
        machine.counted = vm.instructions;
        // A program that only waits goes round a few instructions: it reads what it waits for
        // from its memory or from a device's register, stores nothing, writes no register and
        // calls nothing. When poll after poll finds it so, the clock moves on toward what it
        // waits for. (A loop that only counts is a delay, which takes the time it takes.)
        let interrupted = vm.poll_argument.take();
        let kernel = machine.r3000.as_ref().is_some_and(|processor| processor.kernel);
        let polled = interrupted.is_some() && writes.is_empty() && !machine.busy;
        machine.busy = false;
        let stored = std::mem::replace(&mut machine.stores, vm.memory.stores()) != vm.memory.stores();
        let loaded = std::mem::replace(&mut machine.loads, vm.memory.loads()) != vm.memory.loads();
        let site = vm.site.unwrap_or(0);
        let near = machine.sites.iter().all(|s| s.abs_diff(site) <= WAITING_SPAN);
        machine.sites[machine.seen_at % machine.sites.len()] = site;
        machine.seen_at += 1;
        // A program that waits comes round to the registers it had: one that computes (a
        // checksum over its code reads memory and stores nothing, too) never has the same twice.
        let registers = match interrupted {
            Some(state) => vm.memory.read_vec(state, 32 * 8).map_or(0, |bytes| {
                bytes.chunks_exact(8).fold(site ^ 0xcbf2_9ce4_8422_2325, |hash, word| {
                    (hash ^ u64::from_le_bytes([word[0], word[1], word[2], word[3], word[4], word[5], word[6], word[7]]))
                        .wrapping_mul(0x0100_0000_01b3)
                })
            }),
            None => 0,
        };
        let familiar = machine.registers.contains(&registers);
        if machine.registers.len() >= 64 {
            machine.registers.pop_front();
        }
        machine.registers.push_back(registers);
        machine.repeats = if polled && near && loaded && !stored && familiar { machine.repeats + 1 } else { 0 };
        // A loop of nothing waits for an interrupt, whatever the polls found.
        let waiting = machine.repeats >= WAITING_POLLS || idles;
        {
            let mut platform = platform.borrow_mut();
            let mut guest = Guest { memory: &mut vm.memory, image };
            if waiting {
                // Not all the way at once: what the program waits for may be a register that
                // changes between two of the machine's events, as the beam's line does.
                let skipped = platform.idle().min(WAIT_QUANTUM * 4);
                platform.advance(&mut guest, skipped).map_err(stop)?;
            }
            for (address, bytes) in writes {
                let Some(register) = platform.fold(address) else { continue };
                let len = bytes.len().min(8);
                platform.write(&mut guest, register, len as u32 * 8, order.get(&bytes));
            }
            platform.advance(&mut guest, cycles).map_err(stop)?;
        }
        // A kernel takes its interrupts itself, as the processor raises them between two of the
        // program's instructions; an answered one has the machine call the program's handlers;
        // and a console without a kernel raises its processor's exception in the program.
        if kernel {
            self.psx_line(vm)
        } else if image.n64.is_some() {
            self.vr4300_line(vm, interrupted)
        } else {
            self.deliveries(vm)
        }
    }

    /// The Nintendo 64's interrupt line at the VR4300: the RCP's interrupt is the cause
    /// register's third pending bit, and with the status register's interrupts on, that line
    /// let through and no exception being handled, the processor takes the interrupt's
    /// exception. The program's handler at the general vector is a jump to a function of the
    /// program, which the run calls on the registers of the code it interrupts (`interrupted`,
    /// the state the poll came from), as the processor enters it: the handler saves and
    /// restores what it uses, and its `eret` is the function's return, which ends the
    /// exception.
    fn vr4300_line(&mut self, vm: &mut Vm<'_>, interrupted: Option<u64>) -> Result<(), Stop> {
        // The three registers' places in the coprocessor's global; the status register's
        // enable, its exception and error levels, and the line's bit in both registers.
        const STATUS: u64 = 12 * 8;
        const CAUSE: u64 = 13 * 8;
        const EPC: u64 = 14 * 8;
        const ENABLED: u64 = 1;
        const HANDLING: u64 = 6;
        const LINE: u64 = 0x400;
        let Some(cop0) = vm.symbol(crate::state::N64_COP0) else { return Ok(()) };
        let raised = self.machine()?.interrupt();
        let cause = (vm.memory.read_u64(cop0 + CAUSE)? & !(LINE | 0x7c)) | if raised { LINE } else { 0 };
        vm.memory.write_u64(cop0 + CAUSE, cause)?;
        let status = vm.memory.read_u64(cop0 + STATUS)?;
        if !raised || status & (ENABLED | HANDLING) != ENABLED || status & LINE == 0 {
            return Ok(());
        }
        let (Some(state), Some(entry)) = (interrupted, vr4300_vector(vm)) else { return Ok(()) };
        let Some(handler) = vm.name_at(entry).map(str::to_owned) else {
            return Err(Stop::fault(format!("the exception vector jumps to {entry:#x}, which is no lifted function")));
        };
        // The exception level is set and the place the program was at kept, as a word.
        vm.memory.write_u64(cop0 + STATUS, status | 2)?;
        vm.memory.write_u64(cop0 + EPC, i64::from(vm.site.unwrap_or(0) as u32 as i32) as u64)?;
        vm.call_named(self, &handler, vec![Value::Int(u128::from(state))])?;
        let status = vm.memory.read_u64(cop0 + STATUS)?;
        vm.memory.write_u64(cop0 + STATUS, status & !2)
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
        // Code the lift has no function for is the processor's, where the run has one.
        let name = vm.name_at(delivery.entry).map(str::to_owned);
        if name.is_none() && !self.r3000() {
            return Err(Stop::fault(format!("the machine calls {:#x}, which is no lifted function", delivery.entry)));
        }
        let machine_shape =
            name.as_ref().is_none_or(|name| vm.function(name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void));
        if !machine_shape {
            return self.call_back(vm, delivery.entry, [delivery.args[0], delivery.args[1], delivery.args[2]]);
        }
        let depth = self.machine()?.depth;
        if self.machine()?.states.len() <= depth {
            let state = self.new_state(vm)?;
            self.machine()?.states.push(state);
        }
        let stacks = self.stacks();
        if self.machine()?.stack == 0 {
            self.machine()?.stack = match stacks {
                Some(stacks) => stacks.handlers,
                // A state's worth of heap is a block like another: the stack is one of its own size.
                None => self.new_block(vm, HANDLER_STACK)? + HANDLER_STACK - 64,
            };
        }
        let global_pointer = self.image.global_pointer;
        let machine = self.machine()?;
        let state = machine.states[depth];
        // A nested handler runs below the one it interrupts.
        let nested = stacks.map_or(HANDLER_STACK / 8, |stacks| stacks.nested);
        let stack = delivery.stack.unwrap_or(machine.stack - depth as u64 * nested);
        machine.depth += 1;
        let mut registers =
            vec![(29, stack), (31, 0), (4, delivery.args[0]), (5, delivery.args[1]), (6, delivery.args[2]), (7, delivery.args[3])];
        registers.extend(delivery.registers.iter().map(|(register, value)| (u64::from(*register), *value)));
        let result = (|| {
            if let Some(gp) = delivery.gp.or(global_pointer) {
                vm.memory.write_u64(state + 28 * 8, gp)?;
            }
            for (register, value) in registers {
                // A word in a 64-bit register is held sign-extended.
                vm.memory.write_u64(state + register * 8, i64::from(value as u32 as i32) as u64)?;
            }
            match &name {
                Some(name) => {
                    vm.call_named(self, name, vec![Value::Int(u128::from(state))])?;
                }
                None => self.r3000_call(vm, state, delivery.entry)?,
            }
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
        self.machine()?.busy = true;
        let image = self.image;
        let platform = self.machine()?.platform.clone();
        let args: Vec<u64> = args.iter().map(Value::as_u64).collect();
        // The BIOS's `setjmp` saves the caller's registers, which the lift handed as the state
        // they are in: the return address, the stack and frame pointers, `s0` to `s7`, and the
        // global pointer, which a PlayStation program keeps in a global.
        if name == crate::raise::prototypes::PSX_SETJMP {
            let (buffer, state) = (args.first().copied().unwrap_or(0) as u32, args.get(1).copied().unwrap_or(0));
            let gp = match vm.symbol(crate::state::PSX_GP) {
                Some(at) => vm.memory.read_u64(at)?,
                None => 0,
            };
            for (slot, register) in [31u64, 29, 30, 16, 17, 18, 19, 20, 21, 22, 23].into_iter().enumerate() {
                let value = vm.memory.read_u64(state + register * 8)?;
                vm.memory.write_u32(u64::from(buffer) + 4 * slot as u64, value as u32)?;
            }
            vm.memory.write_u32(u64::from(buffer) + 44, gp as u32)?;
            return Ok(Value::Int(0));
        }
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
