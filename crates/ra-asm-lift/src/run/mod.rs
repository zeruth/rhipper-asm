//! Running a lifted program through the IR interpreter: what the driver does for a port, done in
//! this process. The image is mapped at its base, the globals and the functions get addresses,
//! the import slots are bound, the main thread attaches, and the entry runs as the level shaped
//! it: over a `%State` at the machine and promoted levels, with its own arguments at the ABI
//! level. The program's output is captured (and written through for `ras run`), and the run ends
//! with the program's exit code or with the fault that stopped it, named with its address.
//!
//! The memory is the host's own, so one program runs at a time in a process: the image's base is
//! one address, and two runs would want it at once.

mod f80;
mod host;
pub(crate) mod os;
mod printf;

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result, anyhow};
use ra_asm_analysis::facts::{Facts, Input, SeenCall};
use ra_asm_ir::Type;
use ra_asm_ir::interp::{Memory, Region, Stop, Value, Vm};
use ra_asm_pe::Image;

use crate::Lifted;
use crate::raise::abi::{Kind, Signature};
use host::Runtime;

/// The exit code of a run a fault stopped, as the driver's `ra_trap` ends a port.
pub const FAULT_EXIT: i32 = 0xE000_0001u32 as i32;
/// The exit code of a run that reached its limit.
pub const LIMIT_EXIT: i32 = 0xE000_0007u32 as i32;

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
}

impl Default for Options {
    fn default() -> Self {
        Self { trace: false, limit: 2_000_000_000, args: Vec::new(), live: false, record: None, pause: None }
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
}

static LOCK: Mutex<()> = Mutex::new(());

/// Runs `lifted` (raised to any level, with the `signatures` of the ABI level when it is at that
/// level) through the interpreter. The run takes a thread of its own with a deep stack, since the
/// program's calls nest as the interpreter's.
pub fn run(image: &Image, lifted: &Lifted, signatures: &HashMap<String, Signature>, options: &Options) -> Result<Outcome> {
    let _one_at_a_time = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("ras-run".into())
            .stack_size(256 << 20)
            .spawn_scoped(scope, || run_here(image, lifted, signatures, options))
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

fn run_here(image: &Image, lifted: &Lifted, signatures: &HashMap<String, Signature>, options: &Options) -> Result<Outcome> {
    let base = image.image_base();
    let size = u64::from(image.size_of_image()).div_ceil(0x10000) * 0x10000;
    let image_region = patiently(|| os::allocate_at(base, size))
        .ok_or_else(|| anyhow!("cannot map the image at {base:#x}: the address is taken in this process"))?;
    let low = |len: u64, what: &str| patiently(|| os::allocate_low(len)).ok_or_else(|| anyhow!("no room below 4 GB for the {what}"));
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
    vm.set_static_region(statics.start, statics.len);
    vm.set_frame_region(frames.start, frames.len);
    if !lifted.standalone {
        vm.memory.write(base, image.mapped()).map_err(setup)?;
    }
    vm.place().map_err(setup)?;
    let path = image.path.as_ref().map_or_else(|| "program.exe".to_owned(), |p| p.display().to_string());
    let args = std::iter::once(path).chain(options.args.iter().cloned()).collect();
    let mut runtime = Runtime::new(image, lifted, &mut vm, args, options.live, stack, heap);
    runtime.bind(&mut vm).map_err(setup)?;
    let result = runtime.attach_process(&mut vm).and_then(|()| enter(&mut vm, &mut runtime, image, lifted, signatures));
    let (exit, stop) = match result.and_then(|code| runtime.check_uncaught().map(|()| code)) {
        Ok(code) | Err(Stop::Exit(code)) => (code, None),
        Err(Stop::Limit(n)) => (LIMIT_EXIT, Some(format!("the run limit of {n} instructions was reached"))),
        Err(Stop::Fault(message)) => (FAULT_EXIT, Some(message)),
    };
    if let Some(record) = &mut vm.record {
        record.flush().context("cannot write the record of the run")?;
    }
    let facts = facts_of(&vm, image, lifted, &options.args);
    Ok(Outcome { output: std::mem::take(&mut runtime.output), exit, stop, steps: vm.steps, facts })
}

/// What the run saw, in the image's addresses: a target is a lifted function's image address
/// (the machine levels hold it as such, the standalone level as the function's own address in
/// the interpreter, or its universal stub's on x86), and a target that is no lifted function (an
/// import, a runtime function) is left out, since the analysis has nothing to make of it.
fn facts_of(vm: &Vm<'_>, image: &Image, lifted: &Lifted, args: &[String]) -> Facts {
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
    Facts { input: Input::of(image, args), calls, entered }
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
    let args = kinds
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
    let result = vm.call_named(runtime, &name, args)?;
    Ok(if is_main && signature.is_some_and(|s| s.returns.is_some()) { result.as_u64() as i32 } else { 0 })
}
