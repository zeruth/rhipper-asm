//! The trace beside the port: the calls and returns of the program's functions with their values,
//! recorded by the port's own hooks (a port emitted with `run.trace`) and, for the reference, by
//! a debugger that breaks at the same functions; and the comparison of the two traces to the
//! first event where they part, which names the function that went wrong.
//!
//! A line of a trace is `> name(args)` at an entry and `< name = value` or `< name` at a return.
//! The port writes its arguments and its result with their kinds (`0x5:32` an integer of 32 bits,
//! `0x...:p` a pointer, `f:0x...:f64` the bits of a double); the reference's line carries the four
//! argument registers at an entry and `rax` with the low half of `xmm0` at a return, which the
//! comparison reads at the port's width and kind. The arguments are recorded for reading, not
//! compared: a pointer differs between the two processes. A pointer result is not compared either.

use std::fmt;
use std::fs::{self, File};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, Result};

use crate::pipeline::Outcome;

/// The functions the reference's trace breaks at: the functions of the port, by the address of
/// the function table (`functions.json`) and under its name.
pub(crate) fn plan(table: &Path) -> Result<Vec<(u64, String)>> {
    let text = fs::read_to_string(table).with_context(|| format!("cannot read {}: emit first", table.display()))?;
    let entries: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", table.display()))?;
    let mut functions = Vec::new();
    for entry in entries.as_array().into_iter().flatten() {
        let status = entry["status"].as_str().unwrap_or("");
        if status != "lifted" && status != "partial" {
            continue;
        }
        let (Some(addr), Some(name)) = (entry["addr"].as_str(), entry["name"].as_str()) else { continue };
        let Ok(address) = u64::from_str_radix(addr.trim_start_matches("0x"), 16) else { continue };
        functions.push((address, name.to_owned()));
    }
    Ok(functions)
}

/// How the port's trace compares with the reference's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Every event matched: the count.
    Same(usize),
    /// The traces part at `event` (from 1), in `function`: the innermost function of the port
    /// open at that point, or the function whose result differed.
    Parts { event: usize, function: String, detail: String },
    /// The traces could not be compared, with the reason.
    NotTraced(String),
}

impl Verdict {
    pub(crate) fn function(&self) -> Option<&str> {
        match self {
            Verdict::Parts { function, .. } => Some(function),
            Verdict::Same(_) | Verdict::NotTraced(_) => None,
        }
    }

    pub(crate) fn event(&self) -> Option<usize> {
        match self {
            Verdict::Parts { event, .. } => Some(*event),
            Verdict::Same(_) | Verdict::NotTraced(_) => None,
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Same(n) => write!(f, "same, {n} events"),
            Verdict::Parts { event, function, detail } => write!(f, "parts at event {event} in {function}: {detail}"),
            Verdict::NotTraced(reason) => write!(f, "not traced: {reason}"),
        }
    }
}

/// The kind a port's result was recorded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Int(u32),
    Ptr,
    F64,
    F32,
    /// The reference's raw registers.
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Value {
    bits: u64,
    kind: Kind,
    /// The reference's `xmm0`, read when the port's result is floating.
    float: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    enter: bool,
    name: String,
    value: Option<Value>,
    line: String,
}

fn parse(line: &str) -> Option<Event> {
    if let Some(rest) = line.strip_prefix("> ") {
        let name = rest.split('(').next()?.to_owned();
        return Some(Event { enter: true, name, value: None, line: line.to_owned() });
    }
    let rest = line.strip_prefix("< ")?;
    let (name, value) = match rest.split_once(" = ") {
        Some((name, value)) => (name, Some(value)),
        None => (rest, None),
    };
    let value = value.and_then(|text| {
        let mut tokens = text.split_whitespace();
        let first = tokens.next()?;
        let (number, kind) = match first.strip_prefix("f:") {
            Some(f) => match f.rsplit_once(':') {
                Some((n, "f32")) => (n, Kind::F32),
                Some((n, _)) => (n, Kind::F64),
                None => (f, Kind::F64),
            },
            None => match first.rsplit_once(':') {
                Some((n, "p")) => (n, Kind::Ptr),
                Some((n, bits)) => (n, Kind::Int(bits.parse().ok()?)),
                None => (first, Kind::Raw),
            },
        };
        let bits = u64::from_str_radix(number.trim_start_matches("0x"), 16).ok()?;
        let float = tokens.next().and_then(|t| u64::from_str_radix(t.trim_start_matches("f:0x"), 16).ok());
        Some(Value { bits, kind, float })
    });
    Some(Event { enter: false, name: name.to_owned(), value, line: line.to_owned() })
}

fn events(path: &Path) -> Vec<Event> {
    fs::read_to_string(path).map(|text| text.lines().filter_map(parse).collect()).unwrap_or_default()
}

/// Compares the port's trace with the reference's, event by event.
pub(crate) fn compare(port: &Path, reference: &Path) -> Verdict {
    let (port, reference) = (events(port), events(reference));
    if port.is_empty() {
        return Verdict::NotTraced("the port recorded no trace".into());
    }
    if reference.is_empty() {
        return Verdict::NotTraced("the reference recorded no trace".into());
    }
    // The functions of the port open at each point: a divergence inside one is its fault.
    let mut open: Vec<&str> = Vec::new();
    let mut index = 0;
    loop {
        let event = index + 1;
        let (p, r) = match (port.get(index), reference.get(index)) {
            (Some(p), Some(r)) => (p, r),
            (None, None) => return Verdict::Same(index),
            (Some(p), None) => {
                let function = open.last().map_or(p.name.as_str(), |f| f).to_owned();
                return Verdict::Parts { event, function, detail: format!("the reference's trace ends where the port {}", describe(p)) };
            }
            (None, Some(r)) => {
                let function = open.last().map_or(r.name.as_str(), |f| f).to_owned();
                return Verdict::Parts { event, function, detail: format!("the port's trace ends where the reference {}", describe(r)) };
            }
        };
        if p.enter != r.enter || p.name != r.name {
            let function = if p.enter { open.last().map_or(p.name.as_str(), |f| f) } else { p.name.as_str() }.to_owned();
            return Verdict::Parts { event, function, detail: format!("the port {}, the reference {}", describe(p), describe(r)) };
        }
        if p.enter {
            open.push(&p.name);
        } else {
            if let Some(at) = open.iter().rposition(|f| *f == p.name) {
                open.truncate(at);
            }
            if let (Some(pv), Some(rv)) = (&p.value, &r.value)
                && let Some(detail) = differ(pv, rv)
            {
                return Verdict::Parts { event, function: p.name.clone(), detail };
            }
        }
        index += 1;
    }
}

fn describe(e: &Event) -> String {
    if e.enter {
        format!("entered {}", e.name)
    } else {
        match &e.value {
            Some(v) => format!("returned {} from {}", v.text(), e.name),
            None => format!("returned from {}", e.name),
        }
    }
}

/// What differs between the port's result and the reference's, read at the port's kind.
fn differ(port: &Value, reference: &Value) -> Option<String> {
    let (p, r) = match port.kind {
        Kind::Int(bits) => {
            let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
            (port.bits & mask, reference.bits & mask)
        }
        Kind::F64 => (port.bits, reference.float?),
        Kind::F32 => (port.bits & 0xFFFF_FFFF, reference.float? & 0xFFFF_FFFF),
        Kind::Ptr | Kind::Raw => return None,
    };
    (p != r).then(|| format!("the port returned {}, the reference {:#x}", port.text(), r))
}

impl Value {
    fn text(&self) -> String {
        match self.kind {
            Kind::Int(bits) => format!("{:#x}:{bits}", self.bits),
            Kind::Ptr => format!("{:#x}:p", self.bits),
            Kind::F64 => format!("f:{:#x}:f64", self.bits),
            Kind::F32 => format!("f:{:#x}:f32", self.bits),
            Kind::Raw => format!("{:#x}", self.bits),
        }
    }
}

/// Runs the reference under a debugger that breaks at each function of `plan` (addresses of the
/// image at `image_base`) and at the return address of each call to one, writing the trace to
/// `trace`; the output goes to `log` as a plain run's does.
#[cfg(windows)]
pub(crate) fn run_reference(
    command: &mut Command,
    plan: &[(u64, String)],
    image_base: u64,
    trace: &Path,
    log: &mut File,
    timeout: Option<Duration>,
) -> Result<Outcome> {
    use std::os::windows::process::CommandExt;

    const DEBUG_ONLY_THIS_PROCESS: u32 = 2;
    let out = File::create(trace).with_context(|| format!("cannot write {}", trace.display()))?;
    command.creation_flags(DEBUG_ONLY_THIS_PROCESS);
    let started = std::time::Instant::now();
    let (mut child, readers) = crate::pipeline::spawn_logged(command, log, false)?;
    let mut debugger = debugger::Debugger::new(plan, image_base, std::io::BufWriter::new(out));
    let mut alive = false;
    let result = loop {
        match debugger.step() {
            Ok(debugger::Progress::Exited) => break Ok(()),
            Ok(debugger::Progress::Running) => {}
            Err(e) => break Err(e),
        }
        if !alive && timeout.is_some_and(|t| started.elapsed() >= t) {
            alive = true;
            let _ = child.kill();
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let outcome = crate::pipeline::collect(status, alive, started, readers);
    result?;
    Ok(outcome)
}

#[cfg(not(windows))]
pub(crate) fn run_reference(_: &mut Command, _: &[(u64, String)], _: u64, _: &Path, _: &mut File, _: Option<Duration>) -> Result<Outcome> {
    anyhow::bail!("the reference's trace needs the Windows debugger")
}

/// The debugger: the Windows debug events of one process, software breakpoints at the planned
/// entries and at the return addresses seen at them, and a frame stack per thread that pairs a
/// return with its entry by the stack pointer.
#[cfg(windows)]
mod debugger {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::{BufWriter, Write};

    use anyhow::{Result, bail};

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn WaitForDebugEvent(event: *mut DebugEvent, milliseconds: u32) -> i32;
        fn ContinueDebugEvent(pid: u32, tid: u32, status: u32) -> i32;
        fn ReadProcessMemory(process: Handle, address: *const c_void, buffer: *mut c_void, size: usize, read: *mut usize) -> i32;
        fn WriteProcessMemory(process: Handle, address: *mut c_void, buffer: *const c_void, size: usize, written: *mut usize) -> i32;
        fn FlushInstructionCache(process: Handle, address: *const c_void, size: usize) -> i32;
        fn GetThreadContext(thread: Handle, context: *mut Context) -> i32;
        fn SetThreadContext(thread: Handle, context: *const Context) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    const EXCEPTION_DEBUG_EVENT: u32 = 1;
    const CREATE_THREAD_DEBUG_EVENT: u32 = 2;
    const CREATE_PROCESS_DEBUG_EVENT: u32 = 3;
    const EXIT_THREAD_DEBUG_EVENT: u32 = 4;
    const EXIT_PROCESS_DEBUG_EVENT: u32 = 5;
    const LOAD_DLL_DEBUG_EVENT: u32 = 6;
    const EXCEPTION_BREAKPOINT: u32 = 0x8000_0003;
    const EXCEPTION_SINGLE_STEP: u32 = 0x8000_0004;
    const DBG_CONTINUE: u32 = 0x0001_0002;
    const DBG_EXCEPTION_NOT_HANDLED: u32 = 0x8001_0001;
    /// `CONTEXT_CONTROL | CONTEXT_INTEGER | CONTEXT_FLOATING_POINT` of AMD64.
    const CONTEXT_FULL: u32 = 0x0010_000B;
    const TRAP_FLAG: u32 = 0x100;
    const INT3: u8 = 0xCC;

    /// `DEBUG_EVENT`: the code and the ids, then the union, read as words.
    #[repr(C)]
    struct DebugEvent {
        code: u32,
        pid: u32,
        tid: u32,
        u: [u64; 20],
    }

    /// The AMD64 `CONTEXT`, with the registers the trace reads named.
    #[repr(C, align(16))]
    struct Context {
        p_home: [u64; 6],
        flags: u32,
        mxcsr: u32,
        segments: [u16; 6],
        eflags: u32,
        dr: [u64; 6],
        rax: u64,
        rcx: u64,
        rdx: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        rsi: u64,
        rdi: u64,
        r8: u64,
        r9: u64,
        r10: u64,
        r11: u64,
        r12: u64,
        r13: u64,
        r14: u64,
        r15: u64,
        rip: u64,
        /// The `XMM_SAVE_AREA32`: `xmm0` at 0xa0 in it.
        flt_save: [u8; 512],
        vector: [u8; 0x1d0],
    }

    const _: () = assert!(std::mem::size_of::<Context>() == 0x4d0);
    const _: () = assert!(std::mem::size_of::<DebugEvent>() == 176);

    struct Breakpoint {
        original: u8,
        /// The planned function entered here, by index into the names.
        entry: Option<usize>,
    }

    struct Frame {
        function: usize,
        /// The stack pointer after the function's `ret`, and the address it returns to.
        rsp_after: u64,
        ret: u64,
    }

    pub(super) enum Progress {
        Running,
        Exited,
    }

    pub(super) struct Debugger<'a> {
        plan: &'a [(u64, String)],
        image_base: u64,
        out: BufWriter<File>,
        process: Handle,
        threads: HashMap<u32, Handle>,
        breakpoints: HashMap<u64, Breakpoint>,
        frames: HashMap<u32, Vec<Frame>>,
        /// The breakpoint each thread is stepping over, lifted until the step lands.
        stepping: HashMap<u32, u64>,
    }

    impl<'a> Debugger<'a> {
        pub(super) fn new(plan: &'a [(u64, String)], image_base: u64, out: BufWriter<File>) -> Self {
            Debugger {
                plan,
                image_base,
                out,
                process: std::ptr::null_mut(),
                threads: HashMap::new(),
                breakpoints: HashMap::new(),
                frames: HashMap::new(),
                stepping: HashMap::new(),
            }
        }

        /// Waits for the next debug event and handles it; `Exited` once the process is gone.
        pub(super) fn step(&mut self) -> Result<Progress> {
            let mut event = DebugEvent { code: 0, pid: 0, tid: 0, u: [0; 20] };
            // SAFETY: `event` is a DEBUG_EVENT-sized, writable record for the call's duration.
            if unsafe { WaitForDebugEvent(&raw mut event, 50) } == 0 {
                return Ok(Progress::Running);
            }
            let mut status = DBG_CONTINUE;
            let mut exited = false;
            match event.code {
                CREATE_PROCESS_DEBUG_EVENT => {
                    self.process = event.u[1] as Handle;
                    self.threads.insert(event.tid, event.u[2] as Handle);
                    let delta = event.u[3].wrapping_sub(self.image_base);
                    for (index, (address, _)) in self.plan.iter().enumerate() {
                        self.plant(address.wrapping_add(delta), Some(index))?;
                    }
                    // SAFETY: the file handle of the event is the debugger's to close.
                    unsafe { CloseHandle(event.u[0] as Handle) };
                }
                CREATE_THREAD_DEBUG_EVENT => {
                    self.threads.insert(event.tid, event.u[0] as Handle);
                }
                EXIT_THREAD_DEBUG_EVENT => {
                    self.threads.remove(&event.tid);
                    self.frames.remove(&event.tid);
                }
                LOAD_DLL_DEBUG_EVENT => {
                    // SAFETY: as above, the file handle of the event.
                    unsafe { CloseHandle(event.u[0] as Handle) };
                }
                EXIT_PROCESS_DEBUG_EVENT => exited = true,
                EXCEPTION_DEBUG_EVENT => {
                    let code = event.u[0] as u32;
                    let address = event.u[2];
                    status = match code {
                        EXCEPTION_BREAKPOINT => {
                            if self.breakpoints.contains_key(&address) {
                                self.hit(event.tid, address)?;
                            }
                            // The loader's own breakpoint, before the entry point, is passed over.
                            DBG_CONTINUE
                        }
                        EXCEPTION_SINGLE_STEP => {
                            if let Some(address) = self.stepping.remove(&event.tid) {
                                self.write(address, INT3)?;
                            }
                            DBG_CONTINUE
                        }
                        _ => DBG_EXCEPTION_NOT_HANDLED,
                    };
                }
                _ => {}
            }
            // SAFETY: the ids come from the event just waited for.
            unsafe { ContinueDebugEvent(event.pid, event.tid, status) };
            if exited {
                self.out.flush()?;
                return Ok(Progress::Exited);
            }
            Ok(Progress::Running)
        }

        /// A breakpoint of the trace hit on `tid`: the entry or the return it records, then the
        /// step over the original instruction.
        fn hit(&mut self, tid: u32, address: u64) -> Result<()> {
            let Some(&thread) = self.threads.get(&tid) else { bail!("a breakpoint on an unknown thread {tid}") };
            let mut context = Context::zeroed();
            context.flags = CONTEXT_FULL;
            // SAFETY: the thread is stopped at the event and `context` is a full CONTEXT.
            if unsafe { GetThreadContext(thread, &raw mut context) } == 0 {
                bail!("GetThreadContext failed with {}", last_error());
            }
            // Returns first: a frame whose return lands here closes before an entry at the same
            // address opens.
            let frames = self.frames.entry(tid).or_default();
            while let Some(top) = frames.last() {
                if top.rsp_after == context.rsp && top.ret == address {
                    let name = &self.plan[top.function].1;
                    let xmm0 = u64::from_le_bytes(context.flt_save[0xa0..0xa8].try_into().expect("8 bytes"));
                    writeln!(self.out, "< {name} = {:#x} f:{xmm0:#x}", context.rax)?;
                    frames.pop();
                } else if top.rsp_after < context.rsp {
                    // A frame left without returning through its return address.
                    frames.pop();
                } else {
                    break;
                }
            }
            let entry = self.breakpoints[&address].entry;
            if let Some(function) = entry {
                let name = &self.plan[function].1;
                writeln!(self.out, "> {name}({:#x}, {:#x}, {:#x}, {:#x})", context.rcx, context.rdx, context.r8, context.r9)?;
                let ret = self.read_word(context.rsp)?;
                self.frames.entry(tid).or_default().push(Frame { function, rsp_after: context.rsp + 8, ret });
                if !self.breakpoints.contains_key(&ret) {
                    self.plant(ret, None)?;
                }
            }
            // Step over: the original byte back, the instruction pointer on it, one step, then
            // the breakpoint again.
            let original = self.breakpoints[&address].original;
            self.write(address, original)?;
            context.rip = address;
            context.eflags |= TRAP_FLAG;
            // SAFETY: the context was read from this stopped thread and keeps its flags word.
            if unsafe { SetThreadContext(thread, &raw const context) } == 0 {
                bail!("SetThreadContext failed with {}", last_error());
            }
            self.stepping.insert(tid, address);
            Ok(())
        }

        fn plant(&mut self, address: u64, entry: Option<usize>) -> Result<()> {
            let mut original = 0u8;
            // SAFETY: a one-byte read from the debuggee into a local.
            let ok =
                unsafe { ReadProcessMemory(self.process, address as *const c_void, (&raw mut original).cast(), 1, std::ptr::null_mut()) };
            if ok == 0 {
                bail!("cannot read the debuggee at {address:#x}: error {}", last_error());
            }
            self.write(address, INT3)?;
            self.breakpoints.insert(address, Breakpoint { original, entry });
            Ok(())
        }

        fn write(&mut self, address: u64, byte: u8) -> Result<()> {
            // SAFETY: a one-byte write into the debuggee from a local, then the cache flush the
            // documentation asks for after writing code.
            let ok = unsafe {
                let ok = WriteProcessMemory(self.process, address as *mut c_void, (&raw const byte).cast(), 1, std::ptr::null_mut());
                FlushInstructionCache(self.process, address as *const c_void, 1);
                ok
            };
            if ok == 0 {
                bail!("cannot write the debuggee at {address:#x}: error {}", last_error());
            }
            Ok(())
        }

        fn read_word(&self, address: u64) -> Result<u64> {
            let mut word = 0u64;
            // SAFETY: an eight-byte read from the debuggee into a local.
            let ok = unsafe { ReadProcessMemory(self.process, address as *const c_void, (&raw mut word).cast(), 8, std::ptr::null_mut()) };
            if ok == 0 {
                bail!("cannot read the debuggee's stack at {address:#x}: error {}", last_error());
            }
            Ok(word)
        }
    }

    impl Context {
        fn zeroed() -> Self {
            // SAFETY: CONTEXT is plain data; all zeros is a valid value.
            unsafe { std::mem::zeroed() }
        }
    }

    fn last_error() -> u32 {
        // SAFETY: reads the calling thread's last error.
        unsafe { GetLastError() }
    }
}
