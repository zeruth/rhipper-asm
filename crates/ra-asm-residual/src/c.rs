//! The engine as a port links it: the static library of this crate, entered from the port's C
//! runtime when the port reaches code of the image that no lifted function has.
//!
//! ```c
//! int ra_residual_start(const char *image, const char *facts, void *at, uint64_t size);
//! int ra_residual_call(void *state, uint64_t address);
//! void ra_residual_finish(void);
//! ```
//!
//! `ra_residual_start` loads the reference binary at `image` and writes its mapped bytes into the
//! `size` bytes the driver mapped at `at`, the image's base: the port's memory then holds the
//! image at its addresses, and the engine reads the code there. `ra_residual_call` runs the
//! function at `address` over the `%State` at `state` and leaves the state as the function's
//! return does; the function's own writes go to the port's memory. `ra_residual_finish` prints
//! `residual: N entered` on stderr and writes the facts file at `facts`, whose `residual` lists
//! the entries the engine ran. A start or a call answers zero, or prints why it could not and
//! answers one.
//!
//! As in the interpreter's host, the engine runs a function that calls nothing: a call it makes
//! stops it, since the callee's code would need a way back into the port that the engine does
//! not have yet.

use std::collections::BTreeSet;
use std::ffi::{CStr, c_char, c_void};
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, ensure};
use ra_asm_analysis::Facts;
use ra_asm_analysis::facts::Input;

use crate::{Call, Calls, Engine, Machine, Reader, State, Stop};

/// The engine of the port, from its start to its finish.
struct Port {
    engine: Engine,
    facts: PathBuf,
    input: Input,
    /// The entries the engine ran.
    entered: BTreeSet<u64>,
}

static PORT: Mutex<Option<Port>> = Mutex::new(None);

/// Loads the image the port is of and maps its bytes at `at`.
///
/// # Safety
///
/// `image` and `facts` are NUL-terminated strings; `at` is `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ra_residual_start(image: *const c_char, facts: *const c_char, at: *mut c_void, size: u64) -> i32 {
    // SAFETY: the caller passes two C strings.
    let (image, facts) =
        unsafe { (CStr::from_ptr(image).to_string_lossy().into_owned(), CStr::from_ptr(facts).to_string_lossy().into_owned()) };
    let at = at as u64;
    let started = on_engine_stack(move || -> anyhow::Result<Port> {
        let path = PathBuf::from(image);
        let image = ra_asm_pe::load(&path).with_context(|| format!("cannot load {}", path.display()))?;
        let mapped = image.mapped();
        ensure!(at == image.image_base(), "the image is mapped at {at:#x}, not at its base {:#x}", image.image_base());
        ensure!(mapped.len() as u64 <= size, "the image takes {:#x} bytes, more than the {size:#x} mapped for it", mapped.len());
        // SAFETY: the caller mapped `size` writable bytes at `at`, and the image takes no more.
        unsafe { std::ptr::copy_nonoverlapping(mapped.as_ptr(), at as *mut u8, mapped.len()) };
        let args: Vec<String> = std::env::args().skip(1).collect();
        let input = Input::of(&image, &args);
        Ok(Port { engine: Engine::new(image), facts: PathBuf::from(facts), input, entered: BTreeSet::new() })
    });
    match started {
        Some(Ok(port)) => {
            *PORT.lock().unwrap_or_else(PoisonError::into_inner) = Some(port);
            0
        }
        Some(Err(e)) => {
            eprintln!("ra: the residual engine cannot start: {e:#}");
            1
        }
        None => {
            eprintln!("ra: the residual engine failed as it started");
            1
        }
    }
}

/// Runs the function at `address` over the `%State` at `state`.
///
/// # Safety
///
/// `state` is a `%State` of the image's machine, readable and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ra_residual_call(state: *mut c_void, address: u64) -> i32 {
    // The caller waits for the engine's thread, so its state stays where it is until the run ends.
    let state = state as usize;
    let ran = on_engine_stack(move || {
        let mut guard = PORT.lock().unwrap_or_else(PoisonError::into_inner);
        let port = guard.as_mut().ok_or_else(|| "the engine was not started".to_owned())?;
        // SAFETY: the caller passes a state of the image's machine.
        unsafe { run(port, state as *mut u8, address) }
    });
    match ran {
        Some(Ok(())) => 0,
        Some(Err(why)) => {
            eprintln!("ra: the residual engine at {address:#x}: {why}");
            1
        }
        None => {
            eprintln!("ra: the residual engine failed at {address:#x}");
            1
        }
    }
}

/// The stack the engine runs on: the lift and the interpreter of a debug build outgrow the
/// megabyte of the port's main thread, and a call from the driver's exception handler would stand
/// on what the faulting code left of it.
const STACK: usize = 256 << 20;

/// Runs `f` on a thread of [`STACK`] bytes while the caller waits: `None` when it panicked or no
/// thread could start.
fn on_engine_stack<T: Send>(f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|scope| {
        std::thread::Builder::new().name("ra-residual".into()).stack_size(STACK).spawn_scoped(scope, f).ok()?.join().ok()
    })
}

/// Prints what the engine entered and writes it to the facts file, once: the port's
/// `ExitProcess` and the process's detach both come here. It runs on the caller's thread, since
/// no thread starts while the process ends.
#[unsafe(no_mangle)]
pub extern "C" fn ra_residual_finish() {
    let _ = catch_unwind(|| {
        let Some(port) = PORT.lock().unwrap_or_else(PoisonError::into_inner).take() else { return };
        eprintln!("residual: {} entered", port.entered.len());
        let facts =
            Facts { input: port.input.clone(), calls: Vec::new(), entered: Vec::new(), residual: port.entered.iter().copied().collect() };
        let written = serde_json::to_string_pretty(&facts)
            .map_err(anyhow::Error::from)
            .and_then(|text| std::fs::write(&port.facts, text + "\n").map_err(anyhow::Error::from));
        if let Err(e) = written {
            eprintln!("ra: cannot write the residual facts to {}: {e:#}", port.facts.display());
        }
    });
}

/// The run of one function over the port's state, its effects put back into the port.
///
/// # Safety
///
/// `state` is a `%State` of the engine's machine.
unsafe fn run(port: &mut Port, state: *mut u8, address: u64) -> Result<(), String> {
    let mut entry = State::new(port.engine.isa());
    let len = entry.bytes().len();
    // SAFETY: the state is `len` bytes, as the machine's layout has it.
    unsafe { std::ptr::copy_nonoverlapping(state, entry.bytes_mut().as_mut_ptr(), len) };
    let outcome = port.engine.run(&Process, &entry, address, &mut Leaf).map_err(|e| format!("{e:#}"))?;
    port.entered.insert(address);
    // SAFETY: as above.
    unsafe { std::ptr::copy_nonoverlapping(outcome.state.bytes().as_ptr(), state, len) };
    // What lies below the stack pointer the function returned with is its dead frame, and, when
    // the driver enters the engine from an exception handler, the handler's live one: the stack
    // window under it is not written back.
    let sp = outcome.state.stack_pointer();
    let dead = (sp.saturating_sub(port.engine.stack_below), sp);
    for (start, bytes) in &outcome.written {
        for (at, byte) in (*start..).zip(bytes) {
            if (dead.0..dead.1).contains(&at) {
                continue;
            }
            if !accessible(at, 1, true) {
                return Err(format!("the function wrote {at:#x}, which the port cannot write"));
            }
            // SAFETY: the byte is committed and writable.
            unsafe { *(at as *mut u8) = *byte };
        }
    }
    match outcome.stop {
        None => Ok(()),
        Some(Stop::Fault(why)) => Err(why),
        Some(stop) => Err(format!("{stop:?}")),
    }
}

/// The port's own memory, read where the process has it committed and readable.
struct Process;

impl Reader for Process {
    fn read(&self, address: u64, buf: &mut [u8]) -> bool {
        if !accessible(address, buf.len(), false) {
            return false;
        }
        // SAFETY: every byte of the range is committed and readable.
        unsafe { std::ptr::copy_nonoverlapping(address as *const u8, buf.as_mut_ptr(), buf.len()) };
        true
    }
}

/// The calls of a function the engine runs, which the port does not answer yet.
struct Leaf;

impl Calls for Leaf {
    fn call(&mut self, call: Call<'_>, _: &mut Machine<'_, '_>) -> Result<u64, Stop> {
        let what = match call {
            Call::Address(address) => format!("a call of {address:#x}"),
            Call::Import { name, .. } => format!("a call of the import {name}"),
        };
        Err(Stop::fault(format!("{what} from code the residual engine runs, which the port does not answer")))
    }
}

/// `MEMORY_BASIC_INFORMATION`, as `VirtualQuery` fills it: the fields not read here keep the
/// layout.
#[repr(C)]
#[allow(dead_code)]
struct MemoryInfo {
    base: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    partition: u16,
    size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualQuery(address: *const c_void, info: *mut MemoryInfo, length: usize) -> usize;
}

const MEM_COMMIT: u32 = 0x1000;
const PAGE_GUARD: u32 = 0x100;
/// The protections that read: read-only, read-write, write-copy and their executable kinds.
const READABLE: u32 = 0x02 | 0x04 | 0x08 | 0x20 | 0x40 | 0x80;
const WRITABLE: u32 = 0x04 | 0x08 | 0x40 | 0x80;

/// Whether the `len` bytes at `address` are committed, and readable (or `write`, writable),
/// without a guard page among them.
fn accessible(address: u64, len: usize, write: bool) -> bool {
    let Some(end) = address.checked_add(len as u64) else { return false };
    let wanted = if write { WRITABLE } else { READABLE };
    let mut at = address;
    while at < end {
        let mut info = MemoryInfo {
            base: std::ptr::null_mut(),
            allocation_base: std::ptr::null_mut(),
            allocation_protect: 0,
            partition: 0,
            size: 0,
            state: 0,
            protect: 0,
            kind: 0,
        };
        // SAFETY: the query only describes the address; `info` is the structure it fills.
        if unsafe { VirtualQuery(at as *const c_void, &raw mut info, size_of::<MemoryInfo>()) } == 0 {
            return false;
        }
        if info.state != MEM_COMMIT || info.protect & PAGE_GUARD != 0 || info.protect & wanted == 0 {
            return false;
        }
        let next = (info.base as u64).saturating_add(info.size as u64);
        if next <= at {
            return false;
        }
        at = next;
    }
    true
}
