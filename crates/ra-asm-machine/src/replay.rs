//! The reference vectors of a described machine replayed on any host: every case of
//! `<directory>/*.vec` decoded by the machine's table, lifted from its words as a function at the
//! case's pc, run by the interpreter over the state and the memory cells the case names, and
//! compared with what the machine left: where control went, every register but the ones the case
//! masks as undefined, and every byte of the cells.
//!
//! A form is a row of a group, `<group> <row>` (the file's stem and the row's name). A form whose
//! vectors differ is a finding, not a failure, once `<directory>/PENDING.txt` lists it with its
//! first difference; the replay fails on an unlisted form that differs, on a listed one that
//! passes and on a listed one the files do not hold, so the pending list is exact.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};
use ra_asm_testing::vectors::{self, Case, Outcome, State};

use crate::generate::{PC, UNDEFINED};
use crate::instr::Machine;
use crate::lift::Code;
use crate::sem::{StateSpec, bytes};

/// The file of a machine's vector directory that lists the forms differing today.
pub const PENDING: &str = "PENDING.txt";

/// How many IR instructions a case may run: a branch back into its own code loops.
const LIMIT: u64 = 100_000;
/// Where the replay looks first for room for the state, past the cells of a case.
const STATE_FROM: u64 = 0x10_0000;
const PAGE: u64 = 0x1000;

/// What a replay saw, and what in it is not as the pending list says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replayed {
    pub machine: &'static str,
    pub cases: usize,
    pub forms: usize,
    /// The forms with a case that differs.
    pub differing: usize,
    /// The forms the pending list names.
    pub pending: usize,
    /// An unlisted form that differs, with its first difference as a line to paste into the
    /// pending list; a listed form that passes; a listed form without vectors.
    pub failures: Vec<String>,
}

impl Replayed {
    /// Whether exactly the forms of the pending list differ.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

impl fmt::Display for Replayed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} vectors: {} cases replayed over {} forms; {} forms differ, {} pending",
            self.machine, self.cases, self.forms, self.differing, self.pending
        )?;
        if !self.failures.is_empty() {
            write!(f, "\n{} forms are not as {PENDING} says:\n{}", self.failures.len(), self.failures.join("\n"))?;
        }
        Ok(())
    }
}

/// Why the vectors were not replayed.
#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    /// A vector file that is not in the format of [`ra_asm_testing::vectors`].
    Parse {
        path: PathBuf,
        error: vectors::Error,
    },
    /// The directory holds no `.vec` file.
    Empty(PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Error::Parse { path, error } => write!(f, "{}: {error}", path.display()),
            Error::Empty(path) => write!(f, "{} holds no .vec file", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// Replays every case of `<directory>/*.vec` through `machine`'s decoder, its lift over the state
/// `spec` lays out and the interpreter, and checks the forms that differ against
/// `<directory>/PENDING.txt`.
pub fn replay(machine: &Machine, spec: &StateSpec, directory: &Path) -> Result<Replayed, Error> {
    let files = vector_files(directory)?;
    if files.is_empty() {
        return Err(Error::Empty(directory.to_path_buf()));
    }
    let pending = read_pending(&directory.join(PENDING))?;
    let mut forms: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut cases = 0;
    for path in &files {
        let group = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let text = std::fs::read_to_string(path).map_err(|error| Error::Io { path: path.clone(), error })?;
        let parsed = vectors::parse(&text).map_err(|error| Error::Parse { path: path.clone(), error })?;
        for (index, case) in parsed.iter().enumerate() {
            cases += 1;
            let (form, outcome) = replay_case(machine, spec, case);
            let key = format!("{group} {form}");
            let found = forms.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                Vec::new()
            });
            if let Err(first) = outcome {
                found.push(format!("case {} ({}): {first}", index + 1, vectors::hex(&case.bytes)));
            }
        }
    }
    let failures = pending_failures(&order, &forms, &pending);
    let differing = forms.values().filter(|found| !found.is_empty()).count();
    Ok(Replayed { machine: machine.name, cases, forms: forms.len(), differing, pending: pending.len(), failures })
}

/// The `.vec` files of `directory`, sorted.
fn vector_files(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let items = std::fs::read_dir(directory).map_err(|error| Error::Io { path: directory.to_path_buf(), error })?;
    let mut files: Vec<PathBuf> =
        items.flatten().map(|item| item.path()).filter(|path| path.extension().is_some_and(|ext| ext == "vec")).collect();
    files.sort();
    Ok(files)
}

/// The forms the pending list names, with their notes: `<group> <row>`, then after `: ` the first
/// difference the replay saw; `#` starts a comment line.
fn read_pending(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    let text = std::fs::read_to_string(path).map_err(|error| Error::Io { path: path.to_path_buf(), error })?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once(": ").map_or_else(|| (line.to_owned(), String::new()), |(k, v)| (k.to_owned(), v.to_owned())))
        .collect())
}

/// What is not as the pending list says, in `order` (the forms as first seen), then the listed
/// forms the files do not hold.
fn pending_failures(order: &[String], forms: &BTreeMap<String, Vec<String>>, pending: &BTreeMap<String, String>) -> Vec<String> {
    let mut failures = Vec::new();
    for key in order {
        match (forms[key].first(), pending.contains_key(key)) {
            (Some(first), false) => failures.push(format!("{key}: {first}")),
            (None, true) => failures.push(format!("remove from {PENDING}: {key}")),
            _ => {}
        }
    }
    for key in pending.keys() {
        if !forms.contains_key(key) {
            failures.push(format!("{PENDING} names a form without vectors: {key}"));
        }
    }
    failures
}

/// The form of `case`, its row's name or its first word when no row accepts it, and its first
/// difference from what the machine left.
fn replay_case(machine: &Machine, spec: &StateSpec, case: &Case) -> (String, Result<(), String>) {
    let words: Option<Vec<u32>> = case.bytes.chunks(4).map(|word| <[u8; 4]>::try_from(word).ok().map(u32::from_be_bytes)).collect();
    let Some(words) = words.filter(|words| !words.is_empty()) else {
        return ("?".to_owned(), Err(format!("{} bytes are not whole words", case.bytes.len())));
    };
    let Some(decoded) = machine.table.decode(words[0]) else {
        return (format!("{:08x}", words[0]), Err("the word decodes to no row of the machine".to_owned()));
    };
    (decoded.row.name.to_owned(), run(machine, spec, case, &words))
}

/// Where a lifted run ended.
enum Ending {
    /// Control went to an address through the runtime: past the case's code, or a call.
    Pc(u64),
    /// The function returned, with the program counter in the state.
    Returned,
    /// The run faulted, with the kind the vector files name and what the interpreter said.
    Fault(&'static str, String),
    Other(String),
}

/// The runtime of a case's function: a trap faults, a call ends the case at its target, and the
/// lift's stop at the address past the code ends it there; nothing else may be called.
struct Ends {
    /// The address past the case's code.
    past: u64,
    pc_mask: u64,
    ending: Option<Ending>,
}

impl Host for Ends {
    fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let arg = |i: usize| args.get(i).map_or(0, Value::as_u64);
        self.ending = Some(match name {
            "ra_trap" => Ending::Fault("trap", format!("the trap of code {:#x} at {:#x}", arg(1), arg(0))),
            "ra_unsupported" if arg(0) == self.past && arg(1) == 0 => Ending::Pc(self.past),
            "ra_unsupported" => Ending::Other(format!("the lift does not cover the word {:#010x} at {:#x}", arg(1), arg(0))),
            "ra_call" => Ending::Pc(arg(1) & self.pc_mask),
            other => Ending::Other(format!("a call to {other}")),
        });
        Err(Stop::fault(name))
    }
}

/// Runs the lift of `words` over the input of `case`; the first way it differs from the outcome.
fn run(machine: &Machine, spec: &StateSpec, case: &Case, words: &[u32]) -> Result<(), String> {
    let number = |name: &str| (0..spec.registers).find(|&n| machine.registers.get(n as usize).is_some_and(|r| r.name == name));
    let output = match &case.outcome {
        Outcome::State(state) => Some(state),
        Outcome::Fault(_) => None,
    };
    for name in case.input.registers.keys().chain(output.into_iter().flat_map(|out| out.registers.keys())) {
        if name != PC && name != UNDEFINED && number(name).is_none() {
            return Err(format!("`{name}` is no register of the machine"));
        }
    }
    let at = register(&case.input, PC)?.unwrap_or(0) as u64;
    let spans = spans(&case.input, output)?;
    let size = spec.size();
    let state = room(&spans, size).ok_or("no room for the state beside the cells")?;

    let code = Code { base: at, words };
    let function = machine.lift_function(spec, &code, at, &[]);
    let name = function.name.clone();
    let mut module = Module::x86_64_windows(machine.name);
    spec.declare(&mut module);
    module.functions.push(function);
    let mut memory = Memory::new();
    memory.add_region(Region::owned(state, size, "state"));
    for &(start, end) in &spans {
        memory.add_region(Region::owned(start, end - start, "cells"));
    }
    let mut vm = Vm::new(&module, memory);
    vm.limit = LIMIT;
    let unmapped = |stop: Stop| format!("the input does not map: {stop:?}");
    let (width, pc_width) = (bytes(spec.bits) as usize, bytes(spec.pc_bits) as usize);
    for n in 0..spec.registers {
        let name = machine.registers.get(n as usize).map_or("", |r| r.name);
        let value = register(&case.input, name)?.unwrap_or(0) & mask(spec.bits);
        vm.memory.write(state + spec.register_offset(n), &value.to_le_bytes()[..width.min(16)]).map_err(unmapped)?;
    }
    vm.memory.write(state + spec.pc_offset(), &u128::from(at).to_le_bytes()[..pc_width.min(16)]).map_err(unmapped)?;
    for (&address, cell) in &case.input.memory {
        vm.memory.write(address, cell).map_err(unmapped)?;
    }

    let pc_mask = mask(spec.pc_bits) as u64;
    let past = machine.wrap(at.wrapping_add(4 * words.len() as u64));
    let mut ends = Ends { past, pc_mask, ending: None };
    let result = vm.call_named(&mut ends, &name, vec![Value::Int(u128::from(state))]);
    let ending = match (ends.ending, result) {
        (Some(ending), _) => ending,
        (None, Ok(_)) => Ending::Returned,
        (None, Err(Stop::Fault(message))) => Ending::Fault("segv", message),
        (None, Err(other)) => Ending::Other(format!("{other:?}")),
    };
    let stopped = match (&case.outcome, ending) {
        (Outcome::Fault(kind), Ending::Fault(got, _)) if kind == got => return Ok(()),
        (Outcome::Fault(kind), Ending::Fault(got, message)) => {
            return Err(format!("faulted with {got} ({message}), the machine with {kind}"));
        }
        (Outcome::Fault(kind), Ending::Pc(pc)) => return Err(format!("stopped at {pc:#x}, the machine faulted with {kind}")),
        (Outcome::Fault(kind), Ending::Returned) => return Err(format!("returned, the machine faulted with {kind}")),
        (Outcome::Fault(kind), Ending::Other(message)) => return Err(format!("{message}; the machine faulted with {kind}")),
        (Outcome::State(_), Ending::Fault(got, message)) => return Err(format!("faulted with {got} ({message}); the machine did not")),
        (Outcome::State(_), Ending::Other(message)) => return Err(format!("{message}; the machine did not stop there")),
        (Outcome::State(_), Ending::Pc(pc)) => Some(pc),
        (Outcome::State(_), Ending::Returned) => None,
    };
    let Some(out) = output else { return Ok(()) };

    let unread = |stop: Stop| format!("the state does not read back: {stop:?}");
    let pc = match stopped {
        Some(pc) => pc,
        None => le(&vm.memory.read_vec(state + spec.pc_offset(), pc_width as u64).map_err(unread)?) as u64 & pc_mask,
    };
    let expected_pc = after(&case.input, out, PC)? as u64 & pc_mask;
    if pc != expected_pc {
        return Err(format!("stopped at {pc:#x}, the machine at {expected_pc:#x}"));
    }
    let undefined = out.registers.get(UNDEFINED);
    for n in 0..spec.registers {
        let Some(register) = machine.registers.get(n as usize) else { continue };
        if undefined.is_some_and(|numbers| masked(numbers, n)) {
            continue;
        }
        let lifted = le(&vm.memory.read_vec(state + spec.register_offset(n), width as u64).map_err(unread)?) & mask(spec.bits);
        let native = after(&case.input, out, register.name)? & mask(spec.bits);
        if lifted != native {
            return Err(format!("{} is {lifted:#x}, the machine gives {native:#x}", register.name));
        }
    }
    for &(start, end) in &spans {
        let lifted = vm.memory.read_vec(start, end - start).map_err(unread)?;
        let mut native = vec![0u8; lifted.len()];
        for (&address, cell) in case.input.memory.iter().chain(&out.memory) {
            if (start..end).contains(&address) {
                let from = (address - start) as usize;
                native[from..from + cell.len()].copy_from_slice(cell);
            }
        }
        if let Some(i) = lifted.iter().zip(&native).position(|(a, b)| a != b) {
            return Err(format!("the byte at {:#x} is {:#04x}, the machine gives {:#04x}", start + i as u64, lifted[i], native[i]));
        }
    }
    Ok(())
}

/// The value of register `name` on a side of a case; `None` when the side does not name it.
fn register(state: &State, name: &str) -> Result<Option<u128>, String> {
    let Some(value) = state.registers.get(name) else { return Ok(None) };
    if value.len() > 16 {
        return Err(format!("`{name}` has {} bytes, wider than the replay's 128 bits", value.len()));
    }
    Ok(Some(value.iter().fold(0, |n, &b| n << 8 | u128::from(b))))
}

/// The value register `name` has after the case: the output's, or the input's when the output
/// leaves it unchanged.
fn after(input: &State, output: &State, name: &str) -> Result<u128, String> {
    Ok(register(output, name)?.or(register(input, name)?).unwrap_or(0))
}

/// Whether the undefined mask `mask`, the most significant byte first, holds register `n`.
fn masked(mask: &[u8], n: u32) -> bool {
    let byte = (n / 8) as usize;
    byte < mask.len() && mask[mask.len() - 1 - byte] & (1 << (n % 8)) != 0
}

/// The address ranges the cells of both sides of a case cover, merged where they touch.
fn spans(input: &State, output: Option<&State>) -> Result<Vec<(u64, u64)>, String> {
    let mut cells: Vec<(u64, u64)> = Vec::new();
    for (&address, cell) in input.memory.iter().chain(output.into_iter().flat_map(|out| &out.memory)) {
        let end = address.checked_add(cell.len() as u64).ok_or_else(|| format!("the cell at {address:#x} runs past the address space"))?;
        cells.push((address, end));
    }
    cells.sort_unstable();
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for (start, end) in cells {
        match spans.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => spans.push((start, end)),
        }
    }
    Ok(spans)
}

/// The first page from [`STATE_FROM`] on where `size` bytes of state miss every span.
fn room(spans: &[(u64, u64)], size: u64) -> Option<u64> {
    let mut at = STATE_FROM;
    while let Some(&(_, end)) = spans.iter().find(|&&(start, end)| at < end && start < at.saturating_add(size)) {
        at = end.checked_next_multiple_of(PAGE)?;
    }
    at.checked_add(size).map(|_| at)
}

/// A little-endian value of up to 16 bytes.
fn le(bytes: &[u8]) -> u128 {
    bytes.iter().rev().fold(0, |n, &b| n << 8 | u128::from(b))
}

/// The low `bits` bits set.
fn mask(bits: u16) -> u128 {
    if bits >= 128 { u128::MAX } else { (1 << bits) - 1 }
}
