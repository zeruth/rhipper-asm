//! Live debugging in lockstep: the port and the reference run side by side, each stopped at every
//! entry of the program's functions and let go one entry at a time, so the workbench steps both by
//! function, shows the arguments and the memory of each at the same place, and stops where they
//! part.
//!
//! The port is a build with the trace hooks (`run.trace`), started with `RHIPPER_STEP` naming a
//! pipe that its driver serves: it stops at each traced entry, says where, and waits for a
//! command (see `runtime/driver.c`). The reference is its own IR through the interpreter, on a
//! thread of this process, paused at the same entries by the interpreter's
//! [`ra_asm_ir::interp::Pause`]. The state of a side at a stop is the function and its arguments
//! with their kinds; the two states are equal when the functions are the same and every argument
//! that is not a pointer has the same value (an integer at the width of the reference's
//! parameter): a pointer differs between the two processes, as in the trace.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use ra_asm_ir::Type;
use ra_asm_ir::interp::{Memory, Stop, Value};
use serde_json::json;

use crate::project::Project;

/// How long a side may take to reach its next entry, or the port to open its pipe or to end.
const WAIT: Duration = Duration::from_secs(120);

/// The most bytes a `read` returns, as the driver's buffer holds.
pub(crate) const MAX_READ: u64 = 4096;

/// One argument at a stop: its kind as the trace hooks name it (`u` an integer, `p` a pointer,
/// `f` and `g` the bits of a double and a float, `?` another type) and its bits; the width of an
/// integer when the side knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Arg {
    kind: char,
    bits: u64,
    width: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    function: String,
    args: Vec<Arg>,
}

/// Where a side stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Stopped(Entry),
    /// The side ran to its end: the exit code, and what stopped it when it did not end itself.
    Ended { exit: Option<i32>, detail: Option<String> },
}

/// The port, stopped at an entry or ended.
struct Port {
    child: Child,
    /// The pipe, to read the stops from and to write the commands to; gone once the port ended.
    pipe: Option<(BufReader<File>, File)>,
    place: Place,
}

/// What the lockstep asks of the interpreter's thread at a stop.
enum Command_ {
    Next,
    Read { address: u64, size: u64, reply: Sender<Result<Vec<u8>, String>> },
    /// Run on without stopping.
    Free,
}

/// The reference through the interpreter, on its own thread.
struct Reference {
    commands: Sender<Command_>,
    events: Receiver<Place>,
    place: Place,
}

/// A session of the two sides.
pub(crate) struct Lockstep {
    port: Port,
    reference: Reference,
    /// The entries both sides passed, the one they stand at included.
    entry: usize,
}

static SESSIONS: AtomicUsize = AtomicUsize::new(0);

impl Lockstep {
    /// Starts the port of `project` (a build with the trace hooks) and the reference's IR, each
    /// stopped at its first entry.
    pub(crate) fn start(project: &Project, dll: bool) -> Result<Lockstep> {
        if dll {
            bail!("the port is a DLL: a DLL steps in the program that loads it, which the lockstep does not run");
        }
        let binary = project.port_binary(false);
        if !binary.is_file() {
            bail!("no port at {}: build it first", binary.display());
        }
        let log_dir = project.log_dir();
        std::fs::create_dir_all(&log_dir)?;
        let reference = Reference::start(project)?;
        let port = Port::start(project, &binary, &log_dir)?;
        Ok(Lockstep { port, reference, entry: 1 })
    }

    /// Lets both sides go on, entry by entry: `count` entries (of `function` alone, when given,
    /// by the port's side), or to the first entry where the two states differ when `divergence`;
    /// in either case no further than the first difference when `divergence`, and no further than
    /// the end of either side. Answers with the state.
    pub(crate) fn step(&mut self, function: Option<&str>, count: usize, divergence: bool) -> Result<serde_json::Value> {
        let mut left = count;
        loop {
            if matches!(self.port.place, Place::Ended { .. }) || matches!(self.reference.place, Place::Ended { .. }) {
                break;
            }
            self.port.next()?;
            self.reference.next()?;
            self.entry += 1;
            if divergence && differs(&self.port.place, &self.reference.place).is_some() {
                break;
            }
            let counted = match (&self.port.place, function) {
                (Place::Stopped(entry), Some(f)) => entry.function == f,
                (_, Some(_)) => false,
                (_, None) => true,
            };
            if counted && !divergence {
                left = left.saturating_sub(1);
                if left == 0 {
                    break;
                }
            }
        }
        Ok(self.state())
    }

    /// Both sides where they stand: the entry, each side's place, and whether the two are equal,
    /// with the difference when they are not.
    pub(crate) fn state(&self) -> serde_json::Value {
        let difference = differs(&self.port.place, &self.reference.place);
        json!({
            "entry": self.entry,
            "port": place_json(&self.port.place),
            "reference": place_json(&self.reference.place),
            "equal": difference.is_none(),
            "differs": difference,
        })
    }

    /// The `size` bytes at `address` on each side, as hexadecimal, or the reason they cannot be
    /// read: the port's own memory and the interpreter's.
    pub(crate) fn read(&mut self, address: u64, size: u64) -> serde_json::Value {
        let side = |result: Result<Vec<u8>, String>| match result {
            Ok(bytes) => json!({"bytes": hex_bytes(&bytes)}),
            Err(error) => json!({"error": error}),
        };
        json!({
            "address": format!("{address:#x}"), "size": size,
            "port": side(self.port.read(address, size)),
            "reference": side(self.reference.read(address, size)),
        })
    }
}

impl Drop for Lockstep {
    fn drop(&mut self) {
        // The reference runs on to its end on its thread; the port is stopped.
        let _ = self.reference.commands.send(Command_::Free);
        if let Some((_, pipe)) = &mut self.port.pipe {
            let _ = pipe.write_all(b"q\n");
        }
        self.port.pipe = None;
        let _ = self.port.child.kill();
        let _ = self.port.child.wait();
    }
}

impl Port {
    fn start(project: &Project, binary: &std::path::Path, log_dir: &std::path::Path) -> Result<Port> {
        let name = format!(r"\\.\pipe\rhipper-step-{}-{}", std::process::id(), SESSIONS.fetch_add(1, Ordering::Relaxed));
        let log = log_dir.join("lockstep-port.log");
        let out = File::create(&log).with_context(|| format!("cannot write {}", log.display()))?;
        let err = out.try_clone()?;
        let mut child = Command::new(binary)
            .args(&project.run.args)
            .current_dir(&project.run.cwd)
            .env("RHIPPER_STEP", &name)
            .env("RHIPPER_TRACE", log_dir.join("lockstep-port.trace"))
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .with_context(|| format!("cannot start {}", binary.display()))?;
        // The driver makes the pipe at the port's first traced entry.
        let started = Instant::now();
        let pipe = loop {
            match OpenOptions::new().read(true).write(true).open(&name) {
                Ok(pipe) => break pipe,
                Err(_) if started.elapsed() < WAIT => {
                    if let Some(status) = child.try_wait()? {
                        bail!(
                            "the port ended with {status} before its first traced entry: emit it with `run.trace` and build it (its output in {})",
                            log.display()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("the port opened no pipe at {name} in {} s: {e}", WAIT.as_secs());
                }
            }
        };
        let reader = BufReader::new(pipe.try_clone()?);
        let mut port = Port { child, pipe: Some((reader, pipe)), place: Place::Ended { exit: None, detail: None } };
        port.place = port.wait_stop()?;
        Ok(port)
    }

    /// Lets the port go on to its next entry.
    fn next(&mut self) -> Result<()> {
        if let Some((_, pipe)) = &mut self.pipe
            && pipe.write_all(b"n\n").is_ok()
        {
            self.place = self.wait_stop()?;
        } else {
            self.place = self.ended()?;
        }
        Ok(())
    }

    /// The stop the port writes next, or its end when the pipe closes.
    fn wait_stop(&mut self) -> Result<Place> {
        let mut line = String::new();
        let read = match &mut self.pipe {
            Some((reader, _)) => reader.read_line(&mut line).unwrap_or(0),
            None => 0,
        };
        if read == 0 {
            return self.ended();
        }
        parse_stop(line.trim_end()).map(Place::Stopped).with_context(|| format!("the port said `{}` at a stop", line.trim_end()))
    }

    fn ended(&mut self) -> Result<Place> {
        self.pipe = None;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if started.elapsed() >= WAIT {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return Ok(Place::Ended { exit: None, detail: Some(format!("the port did not end in {} s and was stopped", WAIT.as_secs())) });
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Ok(Place::Ended { exit: status.code(), detail: None })
    }

    fn read(&mut self, address: u64, size: u64) -> Result<Vec<u8>, String> {
        let Some((reader, pipe)) = &mut self.pipe else { return Err("the port ended".into()) };
        writeln!(pipe, "r {address:x} {size:x}").map_err(|e| format!("the port's pipe: {e}"))?;
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| format!("the port's pipe: {e}"))?;
        let line = line.trim_end();
        if let Some(error) = line.strip_prefix('!') {
            return Err(error.to_owned());
        }
        parse_hex(line).ok_or_else(|| format!("the port answered `{line}` to a read"))
    }
}

impl Reference {
    fn start(project: &Project) -> Result<Reference> {
        let (events, stops) = mpsc::channel::<Place>();
        let (commands, inbox) = mpsc::channel::<Command_>();
        let inbox = Mutex::new(inbox);
        let free = AtomicBool::new(false);
        let at_stop = events.clone();
        let pause: ra_asm_ir::interp::Pause = Arc::new(move |f: &ra_asm_ir::Function, args: &[Value], memory: &mut Memory| {
            if free.load(Ordering::Relaxed) {
                return;
            }
            let args = f.ty.params.iter().zip(args).map(|(ty, a)| arg_of(ty, a)).collect();
            if at_stop.send(Place::Stopped(Entry { function: f.name.clone(), args })).is_err() {
                free.store(true, Ordering::Relaxed);
                return;
            }
            let inbox = inbox.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                match inbox.recv() {
                    Ok(Command_::Next) => return,
                    Ok(Command_::Read { address, size, reply }) => {
                        let _ = reply.send(memory.read_vec(address, size).map_err(|stop| stop_text(&stop)));
                    }
                    Ok(Command_::Free) | Err(_) => {
                        free.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
        });
        let root = project.root.clone();
        std::thread::Builder::new()
            .name("ras-lockstep".into())
            .spawn(move || {
                let place = match interpret(&root, pause) {
                    Ok(outcome) => Place::Ended { exit: Some(outcome.exit), detail: outcome.stop },
                    Err(e) => Place::Ended { exit: None, detail: Some(format!("{e:#}")) },
                };
                let _ = events.send(place);
            })
            .context("cannot start the reference's thread")?;
        let mut reference = Reference { commands, events: stops, place: Place::Ended { exit: None, detail: None } };
        reference.place = reference.wait_stop();
        Ok(reference)
    }

    fn next(&mut self) -> Result<()> {
        if matches!(self.place, Place::Stopped(_)) {
            self.commands.send(Command_::Next).ok();
            self.place = self.wait_stop();
        }
        Ok(())
    }

    fn wait_stop(&self) -> Place {
        match self.events.recv_timeout(WAIT) {
            Ok(place) => place,
            Err(RecvTimeoutError::Timeout) => {
                Place::Ended { exit: None, detail: Some(format!("the reference reached no entry in {} s", WAIT.as_secs())) }
            }
            Err(RecvTimeoutError::Disconnected) => Place::Ended { exit: None, detail: Some("the interpreter's thread ended".into()) },
        }
    }

    fn read(&self, address: u64, size: u64) -> Result<Vec<u8>, String> {
        if !matches!(self.place, Place::Stopped(_)) {
            return Err("the reference ended".into());
        }
        let (reply, answer) = mpsc::channel();
        self.commands.send(Command_::Read { address, size, reply }).map_err(|_| "the interpreter's thread ended".to_owned())?;
        answer.recv_timeout(WAIT).map_err(|_| "the interpreter did not answer".to_owned())?
    }
}

/// The reference of the project at `root` through the interpreter, paused at each entry.
fn interpret(root: &std::path::Path, pause: ra_asm_ir::interp::Pause) -> Result<ra_asm_lift::run::Outcome> {
    let project = Project::load(root)?;
    let image = project.load_image()?;
    let file = project.facts_file();
    let facts = if file.is_file() { crate::run::read_facts(&file, &image).unwrap_or_default() } else { ra_asm_analysis::Facts::default() };
    let options = ra_asm_lift::run::Options {
        args: project.run.args.clone(),
        pause: Some(ra_asm_lift::run::Pause(pause)),
        ..Default::default()
    };
    crate::pipeline::interpret(&image, &facts, &project, &options)
}

/// An argument of the interpreter at its parameter's type, in the kinds the port's hooks write.
fn arg_of(ty: &Type, value: &Value) -> Arg {
    match ty {
        Type::Int(bits) => Arg { kind: 'u', bits: value.as_u64(), width: Some(u32::from(*bits)) },
        Type::Ptr | Type::Func(_) => Arg { kind: 'p', bits: value.as_u64(), width: None },
        Type::Double => Arg { kind: 'f', bits: value.as_f64().to_bits(), width: None },
        Type::Float => Arg { kind: 'g', bits: u64::from(value.as_f32().to_bits()), width: None },
        _ => Arg { kind: '?', bits: 0, width: None },
    }
}

fn stop_text(stop: &Stop) -> String {
    match stop {
        Stop::Fault(message) => message.clone(),
        other => format!("{other:?}"),
    }
}

/// A stop of the port's pipe: `> name kinds value...`, `-` for no arguments.
fn parse_stop(line: &str) -> Option<Entry> {
    let mut tokens = line.strip_prefix("> ")?.split_whitespace();
    let function = tokens.next()?.to_owned();
    let kinds = tokens.next()?;
    let kinds = if kinds == "-" { "" } else { kinds };
    let values: Vec<u64> = tokens.map(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16).ok()).collect::<Option<_>>()?;
    Some(Entry { function, args: kinds.chars().zip(values).map(|(kind, bits)| Arg { kind, bits, width: None }).collect() })
}

fn parse_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(text, "{b:02x}");
    }
    text
}

/// What differs between the port's place and the reference's, when anything does.
fn differs(port: &Place, reference: &Place) -> Option<String> {
    match (port, reference) {
        (Place::Stopped(p), Place::Stopped(r)) => {
            if p.function != r.function {
                return Some(format!("the port entered {}, the reference {}", p.function, r.function));
            }
            if p.args.len() != r.args.len() {
                return Some(format!("{} has {} arguments in the port, {} in the reference", p.function, p.args.len(), r.args.len()));
            }
            for (i, (pa, ra)) in p.args.iter().zip(&r.args).enumerate() {
                let n = i + 1;
                if pa.kind != ra.kind {
                    return Some(format!("argument {n} of {} is `{}` in the port, `{}` in the reference", p.function, pa.kind, ra.kind));
                }
                let mask = match (pa.kind, ra.width.or(pa.width)) {
                    ('u', Some(bits)) if bits < 64 => (1u64 << bits) - 1,
                    ('g', _) => 0xFFFF_FFFF,
                    ('p' | '?', _) => 0,
                    _ => u64::MAX,
                };
                if pa.bits & mask != ra.bits & mask {
                    return Some(format!(
                        "argument {n} of {} is {:#x} in the port, {:#x} in the reference",
                        p.function,
                        pa.bits & mask,
                        ra.bits & mask
                    ));
                }
            }
            None
        }
        (Place::Ended { exit: p, .. }, Place::Ended { exit: r, .. }) => {
            (p != r).then(|| format!("the port ended with {}, the reference with {}", exit_text(*p), exit_text(*r)))
        }
        (Place::Ended { exit, .. }, Place::Stopped(r)) => {
            Some(format!("the port ended with {} where the reference entered {}", exit_text(*exit), r.function))
        }
        (Place::Stopped(p), Place::Ended { exit, .. }) => {
            Some(format!("the reference ended with {} where the port entered {}", exit_text(*exit), p.function))
        }
    }
}

fn exit_text(exit: Option<i32>) -> String {
    exit.map_or_else(|| "no exit code".to_owned(), |code| format!("{:#x}", code.cast_unsigned()))
}

fn place_json(place: &Place) -> serde_json::Value {
    match place {
        Place::Stopped(entry) => json!({
            "function": entry.function,
            "args": entry.args.iter().map(|a| json!({"kind": a.kind.to_string(), "value": format!("{:#x}", a.bits)})).collect::<Vec<_>>(),
        }),
        Place::Ended { exit, detail } => json!({"ended": true, "exit": exit.map(exit_text_of), "detail": detail}),
    }
}

fn exit_text_of(code: i32) -> String {
    exit_text(Some(code))
}
