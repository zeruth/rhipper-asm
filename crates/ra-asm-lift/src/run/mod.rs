//! Running a lifted program through the IR interpreter: what the driver does for a port, done in
//! this process. The image is mapped at its base, the globals and the functions get addresses,
//! the import slots are bound, the main thread attaches, and the entry runs as the level shaped
//! it: over a `%State` at the machine and promoted levels, with its own arguments at the ABI
//! level. The program's output is captured (and written through for `ras run`), and the run ends
//! with the program's exit code or with the fault that stopped it, named with its address.
//!
//! The memory is the host's own, so one program runs at a time in a process: the image's base is
//! one address, and two runs would want it at once.

mod console;
mod cortexm;
mod f80;
mod ftz;
mod host;
pub(crate) mod os;
mod printf;
mod r3000;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::{Context, Result, anyhow};
use ra_asm_analysis::facts::{Facts, Input, SeenCall};
use ra_asm_ir::interp::{Memory, Observer, Region, Stop, Value, Vm};
use ra_asm_ir::{BlockId, FnType, Type};
use ra_asm_pe::Image;

use crate::Lifted;
use crate::raise::abi::{Kind, Signature};
pub use console::{Console, Native};
use host::Runtime;
pub use r3000::stats as processor_stats;
pub use ra_asm_platform::host::Capture;
pub use ra_asm_platform::profile::{
    detail as phase_detail, details as phase_details, phase, sample as phase_sampler, samples as phase_samples, seconds as device_seconds,
};
pub use ra_asm_platform::psx::gte::command as gte_command;

/// Whether `image` is a program of a console whose machine a run carries: its devices, its BIOS
/// or kernel and its interrupts, with a picture, sound and input.
pub fn has_machine(image: &Image) -> bool {
    image.platform().is_some_and(|name| ra_asm_platform::NAMES.contains(&name))
}

/// The pad state a script's `up+cross` names; `None` for a name that is no button.
pub fn pad(buttons: &str) -> Option<ra_asm_platform::pad::State> {
    ra_asm_platform::pad::State::parse(buttons)
}

/// Whether the DLL `dll` of this host exports `name`: `None` where the host has no such DLL to
/// ask (another system, or a library that is not on its search path).
pub fn host_exports(dll: &str, name: &str) -> Option<bool> {
    os::library(dll).map(|module| os::procedure(module, name).is_some())
}

/// The R5900's float arithmetic `ra_f32_ftz_<name>` on the bits of its operands, as the
/// interpreter answers it, for a host of its own (a VU program's: its FMACs round as the FPU's
/// do); `None` for any other name. An operand past the function's count is not read.
pub fn ftz_call(name: &str, args: &[u32]) -> Option<u32> {
    let (a, b) = (args.first().copied().unwrap_or(0), args.get(1).copied().unwrap_or(0));
    Some(match name.strip_prefix("ra_f32_ftz_")? {
        "add" => ftz::add(a, b),
        "sub" => ftz::sub(a, b),
        "mul" => ftz::mul(a, b),
        "div" => ftz::div(a, b),
        "rsqrt" => ftz::rsqrt(a, b),
        "sqrt" => ftz::sqrt(a),
        "cvt_s_w" => ftz::cvt_s_w(a),
        "cvt_w_s" => ftz::cvt_w_s(a),
        _ => return None,
    })
}

/// The software `f80` of the exact x87 unit for a host of its own: answers a `ra_f80_*` call over
/// the interpreter's memory with its value and the status-word flags it raised, for the host to
/// merge into the program's status word; `None` for any other name. The instruction vectors
/// replay the x87 lift through this.
pub fn f80_call(vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Option<Result<(Value, u16), Stop>> {
    if !name.starts_with("ra_f80_") {
        return None;
    }
    let mut unit = f80::Unit::default();
    Some(host::f80_dispatch(&mut unit, vm, name, args).map(|value| (value, unit.flags)))
}

/// The exit code of a run a fault stopped, as the driver's `ra_trap` ends a port.
pub const FAULT_EXIT: i32 = 0xE000_0001u32 as i32;
/// The exit code of a run an access to unmapped memory stopped, no handler taking it: the
/// `STATUS_ACCESS_VIOLATION` Windows ends the process with.
pub const ACCESS_VIOLATION_EXIT: i32 = 0xC000_0005u32 as i32;
/// The exit code of a run that reached its limit.
pub const LIMIT_EXIT: i32 = 0xE000_0007u32 as i32;
/// The exit code of a run that stopped where control entered code it wrote.
pub const UNPACKED_EXIT: i32 = 0xE000_0008u32 as i32;

#[derive(Debug, Clone)]
pub struct Options {
    /// Name every function entered on stderr.
    pub trace: bool,
    /// Stop after this many IR instructions; zero for no limit.
    pub limit: u64,
    /// The program's arguments, after its path.
    pub args: Vec<String>,
    /// Write the program's output to stdout as it comes, besides capturing it.
    pub live: bool,
    /// Write the record of the run here: the calls and returns of the image's functions with
    /// their values, in the shape of a port's trace.
    pub record: Option<PathBuf>,
    /// Stop at each entry of the image's functions (those the record covers): the run stepped in
    /// lockstep with a port.
    pub pause: Option<Pause>,
    /// Tell this of every call the run makes, with its arguments, its blocks and its result: a
    /// recorder of the run.
    pub observer: Option<Watch>,
    /// Answers a call of the machine levels to code of the image that no lifted function has;
    /// without it such a call stops the run.
    pub residual: Option<Residual>,
    /// Keep every call across the platform boundary in the outcome's `boundary`. The calls take
    /// the memory's one log for what they write, so a run that keeps them has no observer that
    /// logs the memory itself.
    pub session: bool,
    /// Keep the pages the program writes from its start, and stop the run with the address where
    /// a call or a jump enters one of the image's: an unpacking stub's jump into what it unpacked,
    /// whose bytes the lift never read. The address is the outcome's `unpacked`.
    pub unpack: bool,
    /// Stands in for the host: hears of every call across the platform boundary before it runs
    /// and answers the part of each the host would (its DLLs', the processor's queries), so the
    /// run makes no call into the host. A session played back.
    pub answers: Option<Answers>,
    /// How a console program's machine meets this one; without it the run is headless and keeps
    /// only the console's text, in the outcome's output.
    pub console: Option<Console>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            trace: false,
            limit: 2_000_000_000,
            args: Vec::new(),
            live: false,
            record: None,
            pause: None,
            observer: None,
            residual: None,
            session: false,
            unpack: false,
            answers: None,
            console: None,
        }
    }
}

/// What stands in for the host in a run without one: it sees each call across the platform
/// boundary as the program makes it, and answers the part of the call the host would answer. The
/// runtime answers the rest itself (the heap, the output, the threads and the callbacks), so a
/// call the stand-in expects may run code of the program, whose calls it sees in turn.
pub trait Answerer {
    /// The call of `name` with `args` the program makes now, and the index the host's part of it
    /// is answered by; a stop, which ends the run, for a call the stand-in does not expect.
    fn expect(&mut self, name: &str, args: &[Value]) -> Result<usize, Stop>;

    /// The host's answer to the call `expect` gave `index`, with the bytes the host wrote laid
    /// into `memory`.
    fn answer(&mut self, memory: &mut Memory, index: usize) -> Result<Value, Stop>;
}

/// An [`Answerer`], as an option of a run: shared, since the run takes a thread of its own and the
/// caller reads what it saw once the run is over.
#[derive(Clone)]
pub struct Answers(pub Arc<Mutex<dyn Answerer + Send>>);

impl std::fmt::Debug for Answers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Answers")
    }
}

impl Answers {
    pub(crate) fn expect(&self, name: &str, args: &[Value]) -> Result<usize, Stop> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).expect(name, args)
    }

    pub(crate) fn answer(&self, memory: &mut Memory, index: usize) -> Result<Value, Stop> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).answer(memory, index)
    }
}

/// What runs the code of the image that the lift has no function for (code the analysis never
/// found, reached by an address the program computed), for a run at the machine levels.
pub trait Unlifted {
    /// Runs the code at `target` as the call of it in a lifted function would: over the `%State`
    /// at `state` in `memory`, leaving the state and the memory as the code leaves them (on x86 the
    /// return address the call pushed popped). A stop ends the program's run with it.
    fn call(&mut self, memory: &mut Memory, state: u64, target: u64) -> Result<(), Stop>;
}

/// An [`Unlifted`], as an option of a run: shared, since the run takes a thread of its own.
#[derive(Clone)]
pub struct Residual(pub Arc<Mutex<dyn Unlifted + Send>>);

impl std::fmt::Debug for Residual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Residual")
    }
}

impl Residual {
    pub(crate) fn call(&self, memory: &mut Memory, state: u64, target: u64) -> Result<(), Stop> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).call(memory, state, target)
    }
}

/// The interpreter's [`Observer`], as an option of a run: shared, since the run takes a thread of
/// its own and the caller reads what the observer gathered once the run is over.
#[derive(Clone)]
pub struct Watch(pub Arc<Mutex<dyn Observer + Send>>);

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Watch")
    }
}

impl Watch {
    fn observer(&self) -> MutexGuard<'_, dyn Observer + Send + 'static> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Observer for Watch {
    fn enter(&mut self, function: &str, depth: usize) {
        self.observer().enter(function, depth);
    }

    fn leave(&mut self, function: &str, depth: usize) {
        self.observer().leave(function, depth);
    }

    fn begin(&mut self, ty: &FnType, args: &[Value], memory: &mut Memory) {
        self.observer().begin(ty, args, memory);
    }

    fn end(&mut self, result: &Result<Value, Stop>, memory: &mut Memory) {
        self.observer().end(result, memory);
    }

    fn block(&mut self, block: BlockId) {
        self.observer().block(block);
    }
}

/// The interpreter's [`ra_asm_ir::interp::Pause`], as an option of a run.
#[derive(Clone)]
pub struct Pause(pub ra_asm_ir::interp::Pause);

impl std::fmt::Debug for Pause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pause")
    }
}

/// How a run ended.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// What the program wrote to its standard output.
    pub output: Vec<u8>,
    pub exit: i32,
    /// The fault or the limit that stopped the run, when one did.
    pub stop: Option<String>,
    /// The IR instructions executed.
    pub steps: u64,
    /// What the run saw, as the analysis of the next pass takes it.
    pub facts: Facts,
    /// The calls across the platform boundary in the order the program made them, when the
    /// options asked for them.
    pub boundary: Vec<HostCall>,
    /// Where control entered code the run wrote, when the options asked for the stop there and the
    /// run made it: the entry of the program an unpacker unpacked.
    pub unpacked: Option<u64>,
    /// The image's bytes from its base as the run left them, when it stopped at `unpacked`: the
    /// program the unpacker wrote, with the addresses `resolved` names in its import slots.
    pub image: Vec<u8>,
    /// Each function the program asked the host for by name (a `GetProcAddress`), in the order it
    /// asked.
    pub resolved: Vec<Resolved>,
    /// The calls the run made into the host's DLLs: none when the options' `answers` stood in for
    /// the host.
    pub hosted: u64,
}

/// A function of a DLL the host answered the program's ask for with an address of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The address the program got, which names the function in the run.
    pub address: u64,
    /// The DLL by the name the program loaded it by.
    pub dll: String,
    /// The function's name, or `#<ordinal>`.
    pub name: String,
}

/// One call of the program across the platform boundary: an import, or a `ra_*` function of the
/// runtime that answers for the system (the C runtime's stand-ins, the processor's queries), with
/// what it answered and the bytes it left in the program's memory. A call that calls back into the
/// program holds what the callback wrote too, and the calls the callback made follow it.
///
/// A write is seen when it goes through the interpreter's memory: a host DLL that writes through a
/// pointer it was handed writes past the log.
#[derive(Debug, Clone, PartialEq)]
pub struct HostCall {
    pub name: String,
    pub args: Vec<Value>,
    pub result: Result<Value, Stop>,
    /// The bytes written during the call, as runs of adjacent addresses in address order.
    pub writes: Vec<(u64, Vec<u8>)>,
}

/// A call into the image from outside it, as a host program makes one to an export of a library:
/// the function by its name in the lifted module, and its integer arguments.
#[derive(Debug, Clone)]
pub struct Call {
    pub function: String,
    pub args: Vec<u64>,
}

static LOCK: Mutex<()> = Mutex::new(());

/// The stack of the interpreter's thread, and the part of it the program's calls may take: the
/// rest is the host's, for the DLL calls and the run's own unwinding.
const THREAD_STACK: usize = 256 << 20;
const PROGRAM_STACK: usize = THREAD_STACK - (32 << 20);

/// The `PlayStation`'s 2 MiB of RAM at its `kseg0` address, where the image's text and data fold.
const PSX_RAM: u64 = 0x8000_0000;
const PSX_RAM_SIZE: u64 = 0x20_0000;
/// The `PlayStation 2`'s RAM a program reaches: from the last 512 KiB of the kernel's first
/// megabyte to the end of the 32 MiB.
const PS2_RAM: u64 = 0x0008_0000;
const PS2_RAM_END: u64 = 0x0200_0000;
/// The Nintendo 64's 4 MiB of RDRAM at its `kseg0` address.
const N64_RDRAM: u64 = 0x8000_0000;
const N64_RDRAM_SIZE: u64 = 0x40_0000;

/// Runs `lifted` (raised to any level, with the `signatures` of the ABI level when it is at that
/// level) through the interpreter. The run takes a thread of its own with a deep stack, since the
/// program's calls nest as the interpreter's.
pub fn run(image: &Image, lifted: &Lifted, signatures: &HashMap<String, Signature>, options: &Options) -> Result<Outcome> {
    on_own_thread(image, lifted, signatures, options, None).map(|(outcome, _)| outcome)
}

/// Calls one function of `lifted` through the interpreter, as a host program calls an export of
/// the library it loaded, and gives its result (the first result register at the machine levels,
/// the return value at the ABI level) with the outcome of the run. The image is set up as for
/// [`run`]; its entry does not run.
pub fn call(
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    call: &Call,
    options: &Options,
) -> Result<(u64, Outcome)> {
    on_own_thread(image, lifted, signatures, options, Some(call)).map(|(outcome, result)| (result, outcome))
}

fn on_own_thread(
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    options: &Options,
    call: Option<&Call>,
) -> Result<(Outcome, u64)> {
    let _one_at_a_time = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("ras-run".into())
            .stack_size(THREAD_STACK)
            .spawn_scoped(scope, || run_here(image, lifted, signatures, options, call))
            .context("cannot start the interpreter's thread")?;
        handle.join().map_err(|_| anyhow!("the interpreter panicked"))?
    })
}

/// An allocation at the addresses a run needs, asked for again for a few seconds while it fails:
/// the process's other threads (the tests beside this run, a host DLL's heap) hold an address or
/// the commit charge for a moment, and the image has no other base to go to.
fn patiently(mut allocate: impl FnMut() -> Option<os::Allocation>) -> Option<os::Allocation> {
    for _ in 0..100 {
        if let Some(allocation) = allocate() {
            return Some(allocation);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

fn setup(stop: Stop) -> anyhow::Error {
    match stop {
        Stop::Fault(m) => anyhow!("{m}"),
        other => anyhow!("{other:?}"),
    }
}

fn run_here(
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    options: &Options,
    call: Option<&Call>,
) -> Result<(Outcome, u64)> {
    let base = image.image_base();
    // The mapping starts at the 64 KB granule that holds the base: a PSP module loads at
    // `0x08804000`, past the granule's start.
    let granule = base & !0xffff;
    let size = (base - granule + u64::from(image.size_of_image())).div_ceil(0x10000) * 0x10000;
    // A PlayStation program has the console's RAM, the kernel's part below its text with it.
    // A Nintendo 64 program, its RDRAM, the heap libdragon takes past its data with it.
    // A PlayStation 2 program has the console's RAM from the kernel's last pages up, where its
    // start routine puts the stack and the heap.
    let (start, len) = match image.platform() {
        Some("psx") => (PSX_RAM, PSX_RAM_SIZE),
        Some("ps2") => (PS2_RAM, PS2_RAM_END - PS2_RAM),
        Some("n64") => (N64_RDRAM, N64_RDRAM_SIZE.max(size)),
        _ => (granule, size),
    };
    let image_region = patiently(|| os::allocate_at(start, len))
        .ok_or_else(|| anyhow!("cannot map the image at {base:#x}: the address is taken in this process"))?;
    // A PlayStation 2's hardware registers from 256 MB, which its device answers ahead of the
    // memory: the run's own regions lie above them.
    let floor = if image.platform() == Some("ps2") { host::PS2_HARDWARE_END } else { 0x1000_0000 };
    let low =
        |len: u64, what: &str| patiently(|| os::allocate_low_from(floor, len)).ok_or_else(|| anyhow!("no room below 4 GB for the {what}"));
    let statics = low(4 << 20, "globals")?;
    let frames = low(16 << 20, "frames")?;
    let stack = low(1 << 20, "stack")?;
    let heap = low(64 << 20, "heap")?;
    let mut memory = Memory::new();
    for (a, name) in [(&image_region, "image"), (&statics, "globals"), (&frames, "frames"), (&stack, "stack"), (&heap, "heap")] {
        memory.add_region(Region::new(a.start, a.len, name));
    }
    // A page the host's DLLs handed the program (a command line, an environment) is theirs.
    memory.set_probe(Box::new(|address| os::committed(address).map(|(start, len)| Region::new(start, len, "host"))));
    let mut vm = Vm::new(&lifted.module, memory);
    vm.trace = options.trace;
    vm.limit = options.limit;
    vm.stack = PROGRAM_STACK;
    if let Some(path) = &options.record {
        let file = std::fs::File::create(path).with_context(|| format!("cannot write the record at {}", path.display()))?;
        vm.record = Some(Box::new(std::io::BufWriter::new(file)));
    }
    if options.record.is_some() || options.pause.is_some() {
        // The image's own functions, as the port's hooks record them: not the stubs and helpers
        // the lift adds.
        let names: HashSet<&str> = lifted.functions.values().map(String::as_str).collect();
        vm.recorded = lifted.module.functions.iter().enumerate().filter(|(_, f)| names.contains(f.name.as_str())).map(|(i, _)| i).collect();
    }
    vm.pause = options.pause.as_ref().map(|p| p.0.clone());
    vm.observer = options.observer.clone().map(|watch| Box::new(watch) as Box<dyn Observer>);
    vm.set_static_region(statics.start, statics.len);
    vm.set_frame_region(frames.start, frames.len);
    // A PlayStation's ROM is the image itself, where the console has it, beside its RAM.
    if image.psx_bios.is_some() {
        vm.memory.add_region(Region::owned(base, u64::from(image.size_of_image()), "rom"));
    }
    if !lifted.standalone {
        vm.memory.write(base, image.mapped()).map_err(setup)?;
    }
    // The ROM a program reads beside itself is memory of its own, where the console has it.
    if let Some((address, bytes)) = &image.rom {
        vm.memory.add_region(Region::owned(*address, bytes.len() as u64, "rom"));
        vm.memory.write(*address, bytes).map_err(setup)?;
    }
    vm.place().map_err(setup)?;
    // A port's compiled functions run in place of their IR, on the globals the compiled code has.
    if let Some(native) = options.console.as_ref().and_then(|console| console.native.as_ref()) {
        for (address, code) in &native.functions {
            if let Some(name) = lifted.functions.get(address) {
                vm.set_native(name, *code);
            }
        }
        for (name, address) in &native.globals {
            vm.set_symbol(name, *address);
        }
    }
    // A run kept as a session, or played back from one, names the program by its file name
    // alone: the path is in the program's memory, so a checkout of a longer path moves every
    // allocation after it, and a session recorded in one tree would not replay in another.
    let portable = options.session || options.answers.is_some();
    let path = image.path.as_ref().map_or_else(
        || "program.exe".to_owned(),
        |p| match p.file_name() {
            Some(name) if portable => name.to_string_lossy().into_owned(),
            _ => p.display().to_string(),
        },
    );
    let args = std::iter::once(path).chain(options.args.iter().cloned()).collect();
    let mut runtime = Runtime::new(image, lifted, &mut vm, args, options.live, stack, heap);
    // A console's program runs against its console's machine: headless unless the options say how.
    let headless = Console { capture: Some(Capture::new()), ..Console::default() };
    runtime.machine = console::Machine::attach(image, &mut vm, options.console.as_ref().unwrap_or(&headless), options.live, (start, len));
    // A microcontroller's firmware runs against its core: the system timer and its exception.
    runtime.cortexm = cortexm::Core::attach(image, &mut vm);
    runtime.residual.clone_from(&options.residual);
    runtime.answers.clone_from(&options.answers);
    if options.answers.is_none() {
        runtime.attach_host();
    }
    if options.session {
        runtime.boundary = Some(host::Boundary::default());
    }
    runtime.bind(&mut vm).map_err(setup)?;
    if options.unpack {
        // What the loader wrote is the image as the lift read it; the program's writes start here.
        for (&va, name) in &lifted.functions {
            vm.set_origin(name, va);
        }
        vm.memory.track();
    }
    let mut returned = 0;
    let result = runtime.attach_process(&mut vm).and_then(|()| match call {
        None => enter(&mut vm, &mut runtime, image, lifted, signatures),
        Some(call) => invoke(&mut vm, &mut runtime, lifted, signatures, call).map(|value| {
            returned = value;
            0
        }),
    });
    let ended = result.and_then(|code| runtime.check_uncaught().map(|()| code));
    let unpacked = match &ended {
        Err(Stop::Unpacked(address)) => Some(*address),
        _ => None,
    };
    let (exit, stop) = match ended {
        Ok(code) | Err(Stop::Exit(code)) => (code, None),
        Err(Stop::Limit(n)) => (LIMIT_EXIT, Some(format!("the run limit of {n} instructions was reached"))),
        Err(Stop::Unpacked(address)) => (UNPACKED_EXIT, Some(format!("control entered {address:#x}, on a page the run wrote"))),
        Err(Stop::Fault(message)) if message.contains(ra_asm_ir::interp::memory::UNMAPPED) => (ACCESS_VIOLATION_EXIT, Some(message)),
        Err(Stop::Fault(message)) => (FAULT_EXIT, Some(message)),
    };
    if let Some(record) = &mut vm.record {
        record.flush().context("cannot write the record of the run")?;
    }
    let facts = facts_of(&vm, image, lifted, &options.args, &runtime.residual_ran);
    let boundary = runtime.boundary.take().map(|b| b.calls).unwrap_or_default();
    let image_bytes = match unpacked {
        Some(_) => vm.memory.read_vec(base, u64::from(image.size_of_image())).map_err(setup)?,
        None => Vec::new(),
    };
    let resolved = std::mem::take(&mut runtime.resolved);
    let mut output = std::mem::take(&mut runtime.output);
    if let Some(machine) = &runtime.machine {
        output.extend_from_slice(&machine.capture.lock().console);
    }
    let outcome =
        Outcome { output, exit, stop, steps: vm.steps, facts, boundary, unpacked, image: image_bytes, resolved, hosted: runtime.hosted };
    Ok((outcome, returned))
}

/// Calls one function by its integer arguments, as a host calls an export: a function of the ARM64
/// machine shape gets them in `x0` up, a stack and a link register of zero, and answers in `x0`; a
/// function of the ABI level gets them as its parameters and answers with its result. An x86
/// function of the machine shape, or a parameter that is no integer, stops the call: the arguments
/// would land by a guess.
fn invoke(
    vm: &mut Vm<'_>,
    runtime: &mut Runtime<'_>,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    call: &Call,
) -> Result<u64, Stop> {
    let name = call.function.as_str();
    let f = vm.function(name).ok_or_else(|| Stop::fault(format!("@{name} is not in the module")))?;
    let params = f.ty.params.clone();
    if params == [Type::Ptr] && f.ty.ret == Type::Void {
        if lifted.isa != crate::state::Isa::Arm64 || call.args.len() > 8 {
            return Err(Stop::fault(format!("@{name}: a call by {} arguments into the machine shape of this target", call.args.len())));
        }
        let state = runtime.new_state(vm)?;
        for (n, &arg) in call.args.iter().enumerate() {
            vm.memory.write_u64(state + 8 * n as u64, arg)?;
        }
        vm.memory.write_u64(state + 18 * 8, runtime.teb())?;
        vm.memory.write_u64(state + 31 * 8, runtime.stack_top())?;
        vm.call_named(runtime, name, vec![Value::Int(u128::from(state))])?;
        return vm.memory.read_u64(state);
    }
    // A function that takes its caller's argument block gets the arguments past its registers
    // as that block's words, at the top of the machine's stack, which the raised code leaves
    // alone, and the block's address last.
    let mut given = call.args.clone();
    if signatures.get(name).is_some_and(|s| s.variadic) && !params.is_empty() && given.len() + 1 >= params.len() {
        let words = given.split_off(params.len() - 1);
        if words.len() > 16 {
            return Err(Stop::fault(format!("@{name}: an argument block of {} words", words.len())));
        }
        let block = runtime.stack_top() - 128;
        for k in 0..16 {
            vm.memory.write_word(block + 4 * k as u64, 4, words.get(k).copied().unwrap_or(0))?;
        }
        given.push(block);
    }
    if params.len() != given.len() {
        return Err(Stop::fault(format!("@{name} takes {} arguments, the call gives {}", params.len(), call.args.len())));
    }
    let mut args = Vec::with_capacity(params.len());
    for (ty, &arg) in params.iter().zip(&given) {
        let value = match ty {
            Type::Int(bits) if *bits < 64 => arg & ((1u64 << bits) - 1),
            Type::Int(_) | Type::Ptr => arg,
            other => return Err(Stop::fault(format!("@{name}: a parameter of type {other:?} from an integer"))),
        };
        args.push(Value::Int(u128::from(value)));
    }
    Ok(vm.call_named(runtime, name, args)?.as_u64())
}

/// What the run saw, in the image's addresses: a target is a lifted function's image address
/// (the machine levels hold it as such, the standalone level as the function's own address in
/// the interpreter, or its universal stub's on x86), and a target that is no lifted function (an
/// import, a runtime function) is left out, since the analysis has nothing to make of it. The
/// code the [`Residual`] ran is `residual`.
fn facts_of(vm: &Vm<'_>, image: &Image, lifted: &Lifted, args: &[String], residual: &BTreeSet<u64>) -> Facts {
    let by_name: HashMap<&str, u64> = lifted.functions.iter().map(|(va, name)| (name.as_str(), *va)).collect();
    let image_address = |address: u64| -> Option<u64> {
        if lifted.functions.contains_key(&address) {
            return Some(address);
        }
        let name = vm.name_at(address)?;
        match name.strip_prefix("U_").and_then(|hex| u64::from_str_radix(hex, 16).ok()) {
            Some(va) if lifted.functions.contains_key(&va) => Some(va),
            _ => by_name.get(name).copied(),
        }
    };
    let calls = vm
        .seen
        .calls
        .iter()
        .filter_map(|(&site, targets)| {
            let targets: Vec<u64> = targets.iter().filter_map(|&t| image_address(t)).collect();
            (!targets.is_empty()).then_some(SeenCall { site, targets })
        })
        .collect();
    let entered = vm.seen.entered.iter().filter_map(|&i| by_name.get(lifted.module.functions[i].name.as_str()).copied()).collect();
    Facts { input: Input::of(image, args), calls, entered, residual: residual.iter().copied().collect() }
}

/// Calls the entry as the driver does: a function of the machine shape gets a state whose stack
/// pointer sits on a return address of zero (on ARM64 a link register of zero), and its `eax`
/// (`x0`) is the exit code; a function of the
/// ABI level gets its arguments by its signature, `main` the program's, and its result is the exit
/// code when it is `main`.
fn enter(
    vm: &mut Vm<'_>,
    runtime: &mut Runtime<'_>,
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
) -> Result<i32, Stop> {
    if image.rom_boots {
        // The console starts at its ROM's reset vector; the kernel there comes to the program.
        let state = runtime.new_state(vm)?;
        runtime.psx_boot(vm, state, None, ra_asm_format::psx::BIOS_RESET as u32)?;
        return Ok(vm.memory.read_u32(state + 2 * 8)? as i32);
    }
    let entry = lifted.entry.or_else(|| image.entry_point()).ok_or_else(|| Stop::fault("the image has no entry point"))?;
    let name = lifted.functions.get(&entry).cloned().unwrap_or_else(|| crate::function_name(entry));
    let f = vm.function(&name).ok_or_else(|| Stop::fault(format!("the entry @{name} is not in the module")))?;
    let is_main = lifted.entry.is_some() && lifted.entry != image.entry_point();
    if f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void && lifted.isa == crate::state::Isa::Arm64 {
        // The return goes through `x30`, so the stack holds nothing of the caller's; `x18` is
        // the thread's block, as Windows keeps it, and `sp` is the 32nd slot.
        let state = runtime.new_state(vm)?;
        vm.memory.write_u64(state + 18 * 8, runtime.teb())?;
        vm.memory.write_u64(state + 31 * 8, runtime.stack_top())?;
        vm.call_named(runtime, &name, vec![Value::Int(u128::from(state))])?;
        return Ok(vm.memory.read_u32(state)? as i32);
    }
    if f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void && lifted.isa == crate::state::Isa::Arm32 {
        // The return goes through `lr`, which holds zero; `sp` is `r13`.
        let state = runtime.new_state(vm)?;
        vm.memory.write_u64(state + 13 * 8, runtime.stack_top())?;
        vm.call_named(runtime, &name, vec![Value::Int(u128::from(state))])?;
        return Ok(vm.memory.read_u32(state)? as i32);
    }
    if f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void && lifted.isa.is_mips() {
        // The return goes through `ra`, which holds zero; `sp` is `$29`, `gp` `$28` as the start
        // routine of a C library would set it, and the result is `v0`.
        let state = runtime.new_state(vm)?;
        // A console's program starts on a stack in the console's memory.
        let stack = runtime.stacks().map_or(runtime.stack_top(), |stacks| stacks.program);
        vm.memory.write_u64(state + 29 * 8, stack)?;
        if let Some(gp) = image.global_pointer {
            vm.memory.write_u64(state + 28 * 8, gp)?;
        }
        if image.psx_bios.is_some() {
            runtime.psx_boot(vm, state, Some(&name), entry as u32)?;
        } else {
            vm.call_named(runtime, &name, vec![Value::Int(u128::from(state))])?;
        }
        return Ok(vm.memory.read_u32(state + 2 * 8)? as i32);
    }
    if f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void {
        let state = runtime.new_state(vm)?;
        let ptr = u64::from(lifted.bits / 8);
        let sp = runtime.stack_top() - ptr;
        vm.memory.write_word(sp, ptr, 0)?;
        vm.memory.write_u64(state + 32, sp)?;
        vm.call_named(runtime, &name, vec![Value::Int(u128::from(state))])?;
        return Ok(vm.memory.read_u32(state)? as i32);
    }
    let signature = signatures.get(&name);
    let kinds: Vec<Kind> = signature.map(|s| s.params.iter().chain(&s.stack_params).copied().collect()).unwrap_or_default();
    let (argc, argv) = if is_main { runtime.argv(vm)? } else { (0, 0) };
    let mut args: Vec<Value> = kinds
        .iter()
        .enumerate()
        .map(|(i, k)| match (k, i, is_main) {
            (Kind::Float, ..) => Value::F32(0.0),
            (Kind::Double, ..) => Value::F64(0.0),
            (_, 0, true) => Value::Int(u128::from(argc)),
            (_, 1, true) => Value::Int(u128::from(argv)),
            _ => Value::Int(0),
        })
        .collect();
    // A variadic entry takes the address of its argument block last: sixteen dwords at the top
    // of the machine's stack, which the raised code does not use, `main`'s two in the first.
    // Called without one, a start routine that copies its block read it at zero.
    if signature.is_some_and(|s| s.variadic) {
        let block = runtime.stack_top() - 128;
        for k in 0..16u64 {
            let word = match (k, is_main) {
                (0, true) => argc,
                (1, true) => argv,
                _ => 0,
            };
            vm.memory.write_word(block + 4 * k, 4, word)?;
        }
        args.push(Value::Int(u128::from(block)));
    }
    let result = vm.call_named(runtime, &name, args)?;
    Ok(if is_main && signature.is_some_and(|s| s.returns.is_some()) { result.as_u64() as i32 } else { 0 })
}
