//! The recorder: the function vectors of one interpreted run at the machine level.
//!
//! The recorder is the run's [`Observer`]. At the entry of a function of the image it reads the
//! registers out of the `%State` the function was handed and arms the memory; until the exit it
//! logs what the activation read and wrote, and at the exit it reads the registers again. A call
//! the activation makes is not part of it: the recorder sets the callee's accesses apart and
//! keeps the call as an answered one, with the registers the callee changed and the bytes it
//! wrote, its own callees' included. The callee, when it is a function of the image, is an
//! activation in its own right, so a recursive function's inner activations are calls of the
//! outer one and candidates of their own.
//!
//! A run enters some functions a million times. A function keeps the vectors of its first
//! [`FIRST`] activations, which run its blocks over other values, and after them one whose
//! activation ran a block no kept vector of its function ran, [`KEPT`] at most. A file of the
//! vectors a run kept says so in a comment after its header ([`keeping`], [`file`]).
//!
//! What a vector holds, at this level:
//! - the registers are every register of the state, at the entry and at the exit; the bytes of
//!   the state are registers and never memory, and the interpreter's own frames (the `alloca`s
//!   of the IR) are neither;
//! - a call to a function over the state (a function of the image, an import's wrapper) has no
//!   arguments, since the callee reads them where its convention puts them; any other call has
//!   the values it was called with, each by its low 64 bits;
//! - a call's results are the registers of the caller's state it changed, the result registers
//!   of the convention whether it changed them or not (a level without a state has no old value
//!   to fall back on), and `return` for the value a callee outside the state's shape returned;
//! - a call's effects are the bytes written through the interpreter's memory. What a DLL of the
//!   host wrote on its own (a buffer `ReadFile` filled) is not seen.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use ra_asm_ir::interp::{Memory, Observer, Stop, Value};
use ra_asm_ir::{BlockId, FnType, Type};
use ra_asm_lift::run::{Options, Outcome, Watch};
use ra_asm_lift::{Isa, Lifted};
use ra_asm_pe::Image;

use crate::vector::{Call, Left, Register, Run, Vector};

/// The vectors a function keeps at most.
pub const KEPT: usize = 16;

/// The activations of a function whose vectors are kept whatever blocks they ran.
pub const FIRST: usize = 8;

/// The name of the result a callee outside the state's shape returned, among a call's results.
pub const RETURN: &str = "return";

/// The region of the interpreter's own frames, as a run names it.
const FRAMES: &str = "frames";

const X64: [&str; 16] = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
const X86: [&str; 8] = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
const X86_FLAGS: [&str; 7] = ["cf", "pf", "af", "zf", "sf", "df", "of"];
const ARM_FLAGS: [&str; 4] = ["n", "z", "c", "v"];

/// One register of a state: its name, and where its bytes lie.
#[derive(Debug, Clone)]
struct Slot {
    name: String,
    offset: usize,
    size: usize,
}

/// The registers of a `%State`, as the lift lays it out: the general registers, the program
/// counter, the flags as bytes and the vector registers.
#[derive(Debug, Clone)]
pub struct Shape {
    slots: Vec<Slot>,
    size: usize,
    /// How many of the first slots are the general registers, and where the vector registers
    /// start among the slots.
    general: usize,
    vectors: usize,
    /// The slots a callee answers in by its convention: a call's results hold them changed or
    /// not, so a level that has no state to keep the old value in can still be answered.
    answers: Vec<usize>,
}

impl Shape {
    /// The state of `isa` for an image of `bits` address bits. A 32-bit x86 program has the
    /// eight general registers of its machine; the state's other eight slots are not its.
    pub fn of(isa: Isa, bits: u32) -> Self {
        let owned = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        let numbered = |prefix: &str, count: u32| (0..count).map(|n| format!("{prefix}{n}")).collect::<Vec<_>>();
        let x86_vectors = || numbered("xmm", 16).into_iter().chain((0..16).map(|n| format!("ymm{n}h"))).collect::<Vec<_>>();
        // The general slots the state has, which is not always the registers the machine has.
        let (general_slots, general, counter, flags, vectors) = match isa {
            Isa::X86 if bits == 64 => (16, owned(&X64), "rip", owned(&X86_FLAGS), x86_vectors()),
            Isa::X86 => (16, owned(&X86), "eip", owned(&X86_FLAGS), x86_vectors()),
            Isa::Arm64 => {
                let mut general = numbered("x", 31);
                general.push("sp".to_owned());
                (32, general, "pc", owned(&ARM_FLAGS), numbered("v", 32))
            }
            Isa::Arm32 => (16, numbered("r", 16), "pc", owned(&ARM_FLAGS), numbered("q", 16)),
        };
        let counter_at = 8 * general_slots;
        let flags_at = counter_at + 8;
        let vectors_at = flags_at + 8;
        let size = vectors_at + 16 * vectors.len();
        let mut slots = Vec::new();
        let general_count = general.len();
        slots.extend(general.into_iter().enumerate().map(|(n, name)| Slot { name, offset: 8 * n, size: 8 }));
        slots.push(Slot { name: counter.to_owned(), offset: counter_at, size: 8 });
        slots.extend(flags.into_iter().enumerate().map(|(n, name)| Slot { name, offset: flags_at + n, size: 1 }));
        let vectors_from = slots.len();
        slots.extend(vectors.into_iter().enumerate().map(|(n, name)| Slot { name, offset: vectors_at + 16 * n, size: 16 }));
        // The result registers: `rax` and `rdx` (the high half of a wide x86 result) and `xmm0`;
        // the first two general registers and the first vector register on ARM.
        let second = if isa == Isa::X86 { 2 } else { 1 };
        Self { slots, size, general: general_count, vectors: vectors_from, answers: vec![0, second, vectors_from] }
    }

    /// The name of the register the raise knows as `slot`; none for a flag, or for a register
    /// the state of this machine does not have.
    pub(crate) fn name(&self, slot: ra_asm_lift::raise::liveness::Slot) -> Option<&str> {
        use ra_asm_lift::raise::liveness::Slot as Raised;
        let index = match slot {
            Raised::Gpr(n) if usize::from(n) < self.general => usize::from(n),
            Raised::Xmm(n) => self.vectors + usize::from(n),
            Raised::Gpr(_) | Raised::Flag(_) => return None,
        };
        self.slots.get(index).map(|slot| slot.name.as_str())
    }

    /// How many general registers the state has.
    pub(crate) fn general(&self) -> usize {
        self.general
    }

    /// The bytes a state takes.
    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// Where the register `name` lies in a state: its offset and its bytes.
    pub(crate) fn place(&self, name: &str) -> Option<(usize, usize)> {
        self.slots.iter().find(|slot| slot.name == name).map(|slot| (slot.offset, slot.size))
    }

    /// Whether `name` is a flag of the state, which is one byte of it.
    pub(crate) fn is_flag(&self, name: &str) -> bool {
        self.place(name).is_some_and(|(_, size)| size == 1)
    }

    /// The bytes of the state at `address`, or none when the memory does not hold one there.
    /// The memory must not be armed: the registers are no access of the program's.
    fn snapshot(&self, memory: &mut Memory, address: u64) -> Option<Vec<u8>> {
        memory.read_vec(address, self.size as u64).ok()
    }

    fn value(snapshot: &[u8], slot: &Slot) -> u128 {
        let mut bytes = [0u8; 16];
        bytes[..slot.size].copy_from_slice(&snapshot[slot.offset..slot.offset + slot.size]);
        u128::from_le_bytes(bytes)
    }

    /// Every register of a snapshot, in the state's order.
    fn registers(&self, snapshot: &[u8]) -> Vec<Register> {
        self.slots.iter().map(|slot| (slot.name.clone(), Self::value(snapshot, slot))).collect()
    }

    /// What a call answered: the registers that differ between two snapshots and the result
    /// registers of the convention, with their values in `after`.
    fn answered(&self, before: &[u8], after: &[u8]) -> Vec<Register> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(n, slot)| {
                let value = Self::value(after, slot);
                (self.answers.contains(&n) || value != Self::value(before, slot)).then(|| (slot.name.clone(), value))
            })
            .collect()
    }
}

/// What a run recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recording {
    /// The vectors kept, in the order their activations ended.
    pub vectors: Vec<Vector>,
    /// The activations of each function of the image the run entered, kept or not, by name.
    pub activations: BTreeMap<String, u64>,
}

/// What the recorder knows of one function: the blocks its kept vectors cover.
#[derive(Debug, Default)]
struct Coverage {
    covered: HashSet<BlockId>,
    kept: usize,
}

/// A call that is open: an activation of a function of the image, or any other call (a wrapper,
/// a helper, the host), whose accesses still have to be set apart from its caller's.
#[derive(Debug)]
struct Activation {
    name: String,
    /// The function's address in the image, when it is one of the image's over a state: the
    /// activations a vector is made of.
    address: Option<u64>,
    /// The state the call runs over: its own argument for a function of the image, its caller's
    /// for any other call.
    state: Option<u64>,
    /// The state as the call found it.
    entry: Option<Vec<u8>>,
    /// Whether the call returns a value, which is then one of its results.
    returns: bool,
    blocks: HashSet<BlockId>,
    /// The bytes the call read itself before anything inside it wrote them.
    reads: BTreeMap<u64, u8>,
    /// The addresses the call wrote itself.
    own: BTreeSet<u64>,
    /// Every byte written while the call was open, by itself or by its callees, as it stands.
    written: BTreeMap<u64, u8>,
    calls: Vec<Call>,
}

/// The observer that records the vectors of a run. It is told which functions are the image's;
/// every other call is only a callee.
#[derive(Debug)]
pub struct Recorder {
    shape: Shape,
    /// The functions a vector is recorded of, by name, with their addresses in the image.
    functions: HashMap<String, u64>,
    /// The name `enter` gave, until `begin` opens the call.
    entering: Option<String>,
    open: Vec<Activation>,
    coverage: HashMap<String, Coverage>,
    recording: Recording,
}

impl Recorder {
    /// A recorder of the functions `functions` names, each with its address, over states of
    /// `shape`. A function among them that does not take the state alone is not recorded.
    pub fn new(shape: Shape, functions: impl IntoIterator<Item = (String, u64)>) -> Self {
        Self {
            shape,
            functions: functions.into_iter().collect(),
            entering: None,
            open: Vec::new(),
            coverage: HashMap::new(),
            recording: Recording::default(),
        }
    }

    /// A recorder of the functions of `lifted`.
    pub fn of(lifted: &Lifted) -> Self {
        Self::new(Shape::of(lifted.isa, lifted.bits), lifted.functions.iter().map(|(address, name)| (name.clone(), *address)))
    }

    /// What was recorded so far; the recorder starts over.
    pub fn finish(&mut self) -> Recording {
        std::mem::take(&mut self.recording)
    }

    /// Ends the memory's log and gives its accesses to the call that was running. The bytes of
    /// the call's state and of the interpreter's frames are left out.
    fn settle(&mut self, memory: &mut Memory) {
        let access = memory.disarm();
        let Some(running) = self.open.last_mut() else { return };
        let size = self.shape.size as u64;
        let state = running.state.map(|start| (start, start.saturating_add(size)));
        let frames = memory.regions().iter().find(|region| region.name == FRAMES).map(|r| (r.start, r.start.saturating_add(r.len)));
        let program = |address: u64| ![state, frames].iter().flatten().any(|(start, end)| (*start..*end).contains(&address));
        // The log's reads are the ones before its own writes, so they go in first.
        for (start, bytes) in access.reads {
            for (address, byte) in (start..).zip(bytes) {
                if program(address) && !running.written.contains_key(&address) {
                    running.reads.entry(address).or_insert(byte);
                }
            }
        }
        for (start, bytes) in access.writes {
            for (address, byte) in (start..).zip(bytes) {
                if program(address) {
                    running.own.insert(address);
                    running.written.insert(address, byte);
                }
            }
        }
    }
}

impl Observer for Recorder {
    fn enter(&mut self, function: &str, _depth: usize) {
        self.entering = Some(function.to_owned());
    }

    fn leave(&mut self, _function: &str, _depth: usize) {}

    fn begin(&mut self, ty: &FnType, args: &[Value], memory: &mut Memory) {
        let Some(name) = self.entering.take() else { return };
        self.settle(memory);
        let over_state = ty.params == [Type::Ptr] && ty.ret == Type::Void;
        let address = if over_state { self.functions.get(&name).copied() } else { None };
        let state = match address {
            Some(_) => args.first().map(Value::as_u64),
            None => self.open.last().and_then(|caller| caller.state),
        };
        let entry = state.and_then(|state| self.shape.snapshot(memory, state));
        if let Some(caller) = self.open.last_mut()
            && caller.address.is_some()
        {
            let args = if over_state { Vec::new() } else { args.iter().map(Value::as_u64).collect() };
            caller.calls.push(Call { callee: name.clone(), args, results: Vec::new(), effects: Vec::new() });
        }
        self.open.push(Activation {
            name,
            address,
            state,
            entry,
            returns: ty.ret != Type::Void,
            blocks: HashSet::new(),
            reads: BTreeMap::new(),
            own: BTreeSet::new(),
            written: BTreeMap::new(),
            calls: Vec::new(),
        });
        memory.arm();
    }

    fn end(&mut self, result: &Result<Value, Stop>, memory: &mut Memory) {
        self.settle(memory);
        let Some(mut done) = self.open.pop() else { return };
        let exit = done.state.and_then(|state| self.shape.snapshot(memory, state));
        if let Some(address) = done.address {
            *self.recording.activations.entry(done.name.clone()).or_default() += 1;
            let coverage = self.coverage.entry(done.name.clone()).or_default();
            if coverage.kept < KEPT && (coverage.kept < FIRST || done.blocks.iter().any(|block| !coverage.covered.contains(block))) {
                coverage.kept += 1;
                coverage.covered.extend(done.blocks.iter().copied());
                // What the function wrote itself, as the activation left it: a callee may have
                // written the byte since.
                let out: BTreeMap<u64, u8> = done.own.iter().filter_map(|at| done.written.get(at).map(|byte| (*at, *byte))).collect();
                self.recording.vectors.push(Vector {
                    function: done.name.clone(),
                    address,
                    entry: done.entry.as_deref().map(|snapshot| self.shape.registers(snapshot)).unwrap_or_default(),
                    memory_in: runs(&done.reads),
                    calls: std::mem::take(&mut done.calls),
                    exit: exit.as_deref().map(|snapshot| self.shape.registers(snapshot)).unwrap_or_default(),
                    memory_out: runs(&out),
                    left: left(result),
                });
            }
        }
        if let Some(caller) = self.open.last_mut() {
            if caller.address.is_some()
                && let Some(call) = caller.calls.last_mut()
            {
                if let (Some(before), Some(after)) = (&done.entry, &exit) {
                    call.results = self.shape.answered(before, after);
                }
                if done.returns
                    && let Ok(value) = result
                {
                    call.results.push((RETURN.to_owned(), value.as_u128()));
                }
                call.effects = runs(&done.written);
            }
            // The callee's writes are the caller's to answer for to its own caller, and a byte
            // the callee wrote is no input of the caller's when it reads it later.
            caller.written.append(&mut done.written);
            memory.arm();
        }
    }

    fn block(&mut self, block: BlockId) {
        if let Some(running) = self.open.last_mut() {
            running.blocks.insert(block);
        }
    }
}

/// The bytes of a log joined into runs of adjacent addresses.
fn runs(bytes: &BTreeMap<u64, u8>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (&address, &byte) in bytes {
        match out.last_mut() {
            Some((start, run)) if start.checked_add(run.len() as u64) == Some(address) => run.push(byte),
            _ => out.push((address, vec![byte])),
        }
    }
    out
}

/// The way an activation ended, from what its call gave. The interpreter stops for a trap and
/// for an exception it could not hand the program alike, with a message: both are a trap here,
/// at the address the message names.
pub(crate) fn left(result: &Result<Value, Stop>) -> Left {
    match result {
        Ok(_) => Left::Return,
        Err(Stop::Exit(code)) => Left::Exit(*code as u32),
        Err(Stop::Limit(steps)) => Left::Trap { address: 0, reason: format!("the run limit of {steps} instructions was reached") },
        Err(Stop::Unpacked(address)) => Left::Trap { address: *address, reason: "control entered a page the run wrote".to_owned() },
        Err(Stop::Fault(message)) => {
            let reason = message.lines().next().unwrap_or_default().trim();
            let address = reason
                .rsplit_once(" at 0x")
                .and_then(|(_, rest)| u64::from_str_radix(rest.split(|c: char| !c.is_ascii_hexdigit()).next()?, 16).ok())
                .unwrap_or(0);
            Left::Trap { address, reason: reason.to_owned() }
        }
    }
}

/// The comment a file of the vectors a run kept holds after its header: which of them the
/// recorder keeps. A committed file without the comment of this recorder was kept otherwise.
pub fn keeping() -> String {
    format!("# kept: the first {FIRST} activations of each function, then each that ran a new block, {KEPT} at most")
}

/// The text of a vector file of the vectors a run kept, [`keeping`] after its header.
pub fn file(vectors: &[Vector]) -> Result<String, crate::vector::Error> {
    let text = crate::vector::write(vectors)?;
    let (header, rest) = text.split_once('\n').unwrap_or((&text, ""));
    Ok(format!("{header}\n{}\n{rest}", keeping()))
}

/// Runs `lifted`, raised to the machine level, through the interpreter as
/// [`ra_asm_lift::run::run`] does, and records the vectors of its functions on the way. A module
/// raised further has no function over a state, and records nothing.
pub fn record(image: &Image, lifted: &Lifted, options: &Options) -> Result<(Recording, Outcome)> {
    let recorder = Arc::new(Mutex::new(Recorder::of(lifted)));
    let options = Options { observer: Some(Watch(recorder.clone())), ..options.clone() };
    let outcome = ra_asm_lift::run::run(image, lifted, &HashMap::new(), &options)?;
    let recording = recorder.lock().unwrap_or_else(PoisonError::into_inner).finish();
    Ok((recording, outcome))
}
