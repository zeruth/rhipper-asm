//! The replayer: one recorded activation run again, alone, at a level that takes a state.
//!
//! A vector is loaded into a machine of its own: the registers of its entry into a fresh
//! `%State`, the bytes of its `memory in` at their addresses. The one function then runs in the
//! interpreter, and every call it makes comes to a host that answers from the record, in order:
//! the registers the callee changed go into the state, the bytes it wrote into the memory, and
//! the callee itself never runs. A call of another callee than the record's next, or with other
//! arguments, is a difference and ends the replay. A callee the record does not hold at all,
//! which a raise that outlines can introduce, is a function of the module and is run, its own
//! calls answered from the same record. At the end the way the function left, the registers and
//! the bytes it wrote are compared with the vector's.
//!
//! The memory is the replay's own ([`Region::owned`]): the image at its base, the pages the
//! vector names, and a range the vector does not touch for the state, the frames and the
//! globals of the interpreter. No address of the recorded run has to be free in this process.
//!
//! What a replay does not decide:
//! - a pointer argument into the state is the interpreter's and is not compared; one into the
//!   frames or the globals is a difference of its own, since a vector holds no answer written
//!   through it (`ra_cpuid`, the `ra_f80_*` of the exact x87 unit);
//! - a global the lift added without an address of the image (the x87 stack, `ra_mxcsr`) lies
//!   elsewhere than in the recorded run, so a function that touches one differs;
//! - a vector that left by a stop does not say whether its last call stopped or returned: an
//!   exit is always the call's, a trap is read both ways and replays when one way does;
//! - the promoted level passes no flag across a call or a return, so the flags of the exit are
//!   compared at the machine level only, and at the promoted level the registers of a function
//!   that stopped by itself are not compared: it holds them in values, not in the state.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use anyhow::{Result, anyhow, bail};
use ra_asm_ir::interp::memory::Access;
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{Declaration, FnType, Module, Type};
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;

use crate::record::{RETURN, Shape, left};
use crate::vector::{Call, Left, Vector};

/// The instructions a replay of one vector runs at most: a module that differs may never return.
pub const LIMIT: u64 = 100_000_000;

/// What the name of a function's body ends with in the replayed module. A name is one word, so
/// no function of the module has such a name already.
const BODY: &str = " body";

const PAGE: u64 = 0x1000;

/// The bytes of the frames a replay has: one function runs, and the callees it is not answered.
const FRAMES: u64 = 1 << 20;

/// The lowest address the replay's own range starts at, and the boundary it starts on.
const LOWEST: u64 = 0x0100_0000;
const GRAIN: u64 = 0x1_0000;

/// The callees the record does not hold that run one inside the other at most.
const RUNS: usize = 32;

/// The fault that ends a replay at a call that differs.
const DIFFERS: &str = "the replay differs from its record";

/// One way a replay differs from its vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// The module has no function over a state of the vector's name.
    Function,
    /// The function called `replayed` where the record's call `index` (from one) is of
    /// `recorded`, or where the record has no call left.
    Call { index: usize, recorded: Option<String>, replayed: String },
    /// The call was made with other arguments than the record's.
    Arguments { index: usize, callee: String, recorded: Vec<u64>, replayed: Vec<u64> },
    /// The call's argument `argument` (from one) points into the interpreter's frames or
    /// globals, where the vector holds no answer.
    Pointer { index: usize, callee: String, argument: usize },
    /// The record answers the call in a register the state does not have.
    Answer { index: usize, callee: String, register: String },
    /// The function ended without making the record's call `index`.
    Unasked { index: usize, callee: String },
    /// The function left another way.
    Left { recorded: Left, replayed: Left },
    /// A register of the exit holds another value.
    Register { name: String, recorded: u128, replayed: u128 },
    /// The function wrote other bytes: the first address that differs, with the byte of each
    /// side (none where that side wrote nothing), and how many addresses differ.
    Memory { address: u64, recorded: Option<u8>, replayed: Option<u8>, bytes: usize },
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let byte = |byte: Option<u8>| byte.map_or_else(|| "nothing".to_owned(), |byte| format!("{byte:02x}"));
        match self {
            Self::Function => f.write_str("the module has no function over a state of this name"),
            Self::Call { index, recorded: Some(recorded), replayed } => {
                write!(f, "call {index}: {replayed} is called where the record calls {recorded}")
            }
            Self::Call { index, recorded: None, replayed } => write!(f, "call {index}: {replayed} is called after the record's last call"),
            Self::Arguments { index, callee, recorded, replayed } => {
                write!(f, "call {index} of {callee}: recorded with ({}), replayed with ({})", values(recorded), values(replayed))
            }
            Self::Pointer { index, callee, argument } => {
                write!(
                    f,
                    "call {index} of {callee}: argument {argument} points into the interpreter's own memory, where a vector holds no answer"
                )
            }
            Self::Answer { index, callee, register } => {
                write!(f, "call {index} of {callee}: the record answers in {register}, which is no register of the state")
            }
            Self::Unasked { index, callee } => write!(f, "call {index} of {callee} is never made"),
            Self::Left { recorded, replayed } => write!(f, "left: recorded {}, replayed {}", describe(recorded), describe(replayed)),
            Self::Register { name, recorded, replayed } => write!(f, "register {name}: recorded {recorded:#x}, replayed {replayed:#x}"),
            Self::Memory { address, recorded, replayed, bytes } => {
                write!(f, "memory at {address:#x}: recorded {}, replayed {} (the first of {bytes} bytes)", byte(*recorded), byte(*replayed))
            }
        }
    }
}

/// The arguments of a call, as a line says them.
fn values(values: &[u64]) -> String {
    values.iter().map(|value| format!("{value:#x}")).collect::<Vec<_>>().join(", ")
}

/// The way an activation ended, as a line says it.
fn describe(left: &Left) -> String {
    match left {
        Left::Return => "a return".to_owned(),
        Left::Exit(code) => format!("an exit with {code:#x}"),
        Left::Trap { address, reason } if reason.is_empty() => format!("a trap at {address:#x}"),
        Left::Trap { address, reason } => format!("a trap at {address:#x} ({reason})"),
        Left::Exception { code, address } => format!("the exception {code:#x} at {address:#x}"),
    }
}

/// Whether two activations ended the same way. A trap is the address it stopped at: its reason
/// is the interpreter's wording.
fn same_left(recorded: &Left, replayed: &Left) -> bool {
    match (recorded, replayed) {
        (Left::Trap { address: a, .. }, Left::Trap { address: b, .. }) => a == b,
        _ => recorded == replayed,
    }
}

/// A vector that did not replay: its function, its number among the vectors of that function
/// (from one) and what differs. It prints one line a difference, each naming all three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub function: String,
    pub vector: usize,
    pub differences: Vec<Difference>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, difference) in self.differences.iter().enumerate() {
            if n > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{} vector {}: {difference}", self.function, self.vector)?;
        }
        Ok(())
    }
}

/// A module made ready to replay vectors: every function's body is set apart under a name of
/// its own, so a call of the function, the recursive one included, comes to the replay's host.
pub struct Replayer<'a> {
    image: &'a Image,
    module: Module,
    shape: Shape,
    /// The functions of the module, by the names their callers know.
    bodies: HashSet<String>,
    /// Whether the level keeps the flags in the state across a call and a return.
    flags: bool,
    standalone: bool,
    /// The bytes the functions' addresses and the globals without one take.
    statics: u64,
    /// The instructions a replay of one vector runs at most; zero for no limit.
    pub limit: u64,
}

impl<'a> Replayer<'a> {
    /// A replayer of `lifted`, raised to `level`: the machine or the promoted level, whose
    /// functions take the state a vector is loaded into.
    pub fn new(image: &'a Image, lifted: &Lifted, level: RaiseLevel) -> Result<Self> {
        if level == RaiseLevel::Abi {
            bail!("a vector replays at the machine and the promoted levels: the ABI level has no state to load it into");
        }
        if !lifted.module.frozen.is_empty() {
            bail!("the module holds frozen functions: it replays once the raise has thawed them");
        }
        let mut module = lifted.module.clone();
        let mut bodies = HashSet::with_capacity(module.functions.len());
        for f in &mut module.functions {
            // The name stays what a call, a table and a pointer mean; it is the host's now.
            module.declarations.push(Declaration { name: f.name.clone(), ty: f.ty.clone(), conv: f.conv, attrs: Vec::new() });
            bodies.insert(f.name.clone());
            f.name.push_str(BODY);
        }
        let statics = {
            let vm = Vm::new(&module, Memory::new());
            let layout = vm.layout();
            let globals: u64 = module
                .globals
                .iter()
                .filter(|g| g.address.is_none())
                .map(|g| layout.size(&g.ty) + u64::from(g.align).max(layout.align(&g.ty)).max(16))
                .sum();
            let symbols = 16 * (module.functions.len() + module.declarations.len()) as u64;
            (symbols + globals + PAGE).div_ceil(GRAIN) * GRAIN
        };
        Ok(Self {
            image,
            module,
            shape: Shape::of(lifted.isa, lifted.bits),
            bodies,
            flags: level == RaiseLevel::Machine,
            standalone: lifted.standalone,
            statics,
            limit: LIMIT,
        })
    }

    /// Replays `vector` and gives what differs; nothing when the function did what the vector
    /// holds. An error is a vector the machine could not be made for, not a difference.
    pub fn replay(&self, vector: &Vector) -> Result<Vec<Difference>> {
        let stopped = !matches!(vector.left, Left::Return) && !vector.calls.is_empty();
        let differences = self.run(vector, stopped)?;
        if differences.is_empty() || !stopped || matches!(vector.left, Left::Exit(_)) {
            return Ok(differences);
        }
        // A trap may be the function's own, after its last call returned.
        Ok(if self.run(vector, false)?.is_empty() { Vec::new() } else { differences })
    }

    /// One replay of `vector`, its last call read as the one that stopped when `stops`.
    fn run(&self, vector: &Vector, stops: bool) -> Result<Vec<Difference>> {
        let body = format!("{}{BODY}", vector.function);
        let over_state = self.module.function(&body).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void);
        if !over_state {
            return Ok(vec![Difference::Function]);
        }
        let fault = |stop: Stop| match stop {
            Stop::Fault(message) => anyhow!("{}: {message}", vector.function),
            other => anyhow!("{}: {other:?}", vector.function),
        };

        // The memory: the image, the pages of the vector, and a range for the interpreter.
        let base = self.image.image_base();
        let mapped = self.image.mapped();
        let image_len = u64::from(self.image.size_of_image()).max(mapped.len() as u64).div_ceil(PAGE) * PAGE;
        let image = (base, base.saturating_add(image_len));
        let mut taken = pages(vector, image);
        let mut memory = Memory::new();
        memory.add_region(Region::owned(base, image_len, "image"));
        for &(start, end) in &taken {
            memory.add_region(Region::owned(start, end - start, "program"));
        }
        taken.push(image);
        taken.sort_unstable();
        let state_len = (self.shape.size() as u64).div_ceil(PAGE) * PAGE;
        let own_len = self.statics + FRAMES + state_len;
        let own = free_range(&taken, own_len).ok_or_else(|| anyhow!("{}: no room for the replay's own memory", vector.function))?;
        let (frames, state) = (own + self.statics, own + self.statics + FRAMES);
        memory.add_region(Region::owned(own, self.statics, "globals"));
        memory.add_region(Region::owned(frames, FRAMES, "frames"));
        memory.add_region(Region::owned(state, state_len, "state"));

        let mut vm = Vm::new(&self.module, memory);
        vm.limit = self.limit;
        vm.set_static_region(own, self.statics);
        vm.set_frame_region(frames, FRAMES);
        if !self.standalone {
            vm.memory.write(base, mapped).map_err(fault)?;
        }
        vm.place().map_err(fault)?;

        // The entry.
        for (name, value) in &vector.entry {
            let (offset, size) = self.place(vector, name)?;
            vm.memory.write(state + offset as u64, &value.to_le_bytes()[..size]).map_err(fault)?;
        }
        for (address, bytes) in &vector.memory_in {
            vm.memory.write(*address, bytes).map_err(fault)?;
        }

        let mut host = Answers {
            vector,
            shape: &self.shape,
            bodies: &self.bodies,
            held: vector.calls.iter().map(|call| call.callee.as_str()).collect(),
            stops,
            state: (state, state + state_len),
            own: (own, own + own_len),
            next: 0,
            running: 0,
            stopped: false,
            written: BTreeSet::new(),
            differences: Vec::new(),
        };
        vm.memory.arm();
        let result = vm.call_named(&mut host, &body, vec![Value::Int(u128::from(state))]);
        host.wrote(vm.memory.disarm());
        if !host.differences.is_empty() {
            return Ok(host.differences);
        }

        // The exit.
        let mut differences = Vec::new();
        if let Some(call) = vector.calls.get(host.next) {
            differences.push(Difference::Unasked { index: host.next + 1, callee: call.callee.clone() });
        }
        let replayed = if host.stopped { vector.left.clone() } else { left(&result) };
        if !same_left(&vector.left, &replayed) {
            differences.push(Difference::Left { recorded: vector.left.clone(), replayed });
        }
        // A promoted function that stopped by itself holds its registers in values still.
        if self.flags || host.stopped || matches!(vector.left, Left::Return | Left::Exit(_)) {
            let snapshot = vm.memory.read_vec(state, self.shape.size() as u64).map_err(fault)?;
            for (name, recorded) in &vector.exit {
                if !self.flags && self.shape.is_flag(name) {
                    continue;
                }
                let (offset, size) = self.place(vector, name)?;
                let replayed = number(&snapshot[offset..offset + size]);
                if replayed != *recorded {
                    differences.push(Difference::Register { name: name.clone(), recorded: *recorded, replayed });
                }
            }
        }
        let recorded: BTreeMap<u64, u8> =
            vector.memory_out.iter().flat_map(|(start, bytes)| (*start..).zip(bytes.iter().copied())).collect();
        let mut written = BTreeMap::new();
        for &address in &host.written {
            written.insert(address, vm.memory.read_u8(address).map_err(fault)?);
        }
        let differing: BTreeSet<u64> =
            recorded.keys().chain(written.keys()).copied().filter(|address| recorded.get(address) != written.get(address)).collect();
        if let Some(&address) = differing.first() {
            differences.push(Difference::Memory {
                address,
                recorded: recorded.get(&address).copied(),
                replayed: written.get(&address).copied(),
                bytes: differing.len(),
            });
        }
        Ok(differences)
    }

    /// Where a register the vector names lies in the state.
    fn place(&self, vector: &Vector, name: &str) -> Result<(usize, usize)> {
        self.shape.place(name).ok_or_else(|| anyhow!("{}: the vector's register {name} is no register of the state", vector.function))
    }
}

/// The value of a register's bytes.
fn number(bytes: &[u8]) -> u128 {
    let mut wide = [0u8; 16];
    wide[..bytes.len()].copy_from_slice(bytes);
    u128::from_le_bytes(wide)
}

/// The pages the vector's bytes lie in, outside the image, as ranges in address order.
fn pages(vector: &Vector, image: (u64, u64)) -> Vec<(u64, u64)> {
    let runs = vector.memory_in.iter().chain(&vector.memory_out).chain(vector.calls.iter().flat_map(|call| &call.effects));
    let mut pages = BTreeSet::new();
    for (address, bytes) in runs {
        if bytes.is_empty() {
            continue;
        }
        let last = address.saturating_add(bytes.len() as u64 - 1);
        pages.extend((address / PAGE..=last / PAGE).filter(|page| !(image.0..image.1).contains(&(page * PAGE))));
    }
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for page in pages {
        let start = page * PAGE;
        match ranges.last_mut() {
            Some((_, end)) if *end == start => *end = start.saturating_add(PAGE),
            _ => ranges.push((start, start.saturating_add(PAGE))),
        }
    }
    ranges
}

/// The lowest range of `len` bytes that touches none of `taken`, which is in address order.
fn free_range(taken: &[(u64, u64)], len: u64) -> Option<u64> {
    let mut at = LOWEST;
    for &(start, end) in taken {
        if start > at && start - at >= len {
            break;
        }
        at = at.max(end.checked_next_multiple_of(GRAIN)?);
    }
    at.checked_add(len).map(|_| at)
}

/// The host of a replay: the record of one vector, asked call by call.
struct Answers<'a> {
    vector: &'a Vector,
    shape: &'a Shape,
    bodies: &'a HashSet<String>,
    /// The callees the record holds.
    held: HashSet<&'a str>,
    /// Whether the record's last call is the one the activation stopped in.
    stops: bool,
    /// The range of the state the function runs over.
    state: (u64, u64),
    /// The range of the interpreter's own memory: the globals, the frames and the state.
    own: (u64, u64),
    /// The calls answered so far.
    next: usize,
    /// The callees the record does not hold that are running, one inside the other.
    running: usize,
    /// Whether the replay ended in the record's last call, as the record did.
    stopped: bool,
    /// The addresses the function wrote itself, outside the interpreter's own memory.
    written: BTreeSet<u64>,
    differences: Vec<Difference>,
}

impl Answers<'_> {
    /// Takes the log of what the function did since the memory was armed.
    fn wrote(&mut self, access: Access) {
        let own = self.own;
        for (start, bytes) in access.writes {
            self.written.extend((start..).take(bytes.len()).filter(|address| !(own.0..own.1).contains(address)));
        }
    }

    /// Notes a difference and gives the fault that ends the replay.
    fn differ(&mut self, difference: Difference) -> Stop {
        self.differences.push(difference);
        Stop::fault(DIFFERS)
    }

    /// Compares the arguments of a call with the record's. A function over the state has none
    /// in a vector, and a pointer into the state is the interpreter's.
    fn check(&mut self, index: usize, call: &Call, ty: &FnType, args: &[Value]) -> Result<(), Stop> {
        let over_state = ty.params == [Type::Ptr] && ty.ret == Type::Void;
        let replayed: Vec<u64> = if over_state { Vec::new() } else { args.iter().map(Value::as_u64).collect() };
        let mut same = replayed.len() == call.args.len();
        for (n, (mine, theirs)) in replayed.iter().zip(&call.args).enumerate() {
            if ty.params.get(n) == Some(&Type::Ptr) {
                if (self.state.0..self.state.1).contains(mine) {
                    continue;
                }
                if (self.own.0..self.own.1).contains(mine) {
                    return Err(self.differ(Difference::Pointer { index, callee: call.callee.clone(), argument: n + 1 }));
                }
            }
            same &= mine == theirs;
        }
        if same {
            Ok(())
        } else {
            Err(self.differ(Difference::Arguments { index, callee: call.callee.clone(), recorded: call.args.clone(), replayed }))
        }
    }
}

impl Host for Answers<'_> {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let vector = self.vector;
        let recorded = vector.calls.get(self.next);
        let index = self.next + 1;
        let Some(call) = recorded.filter(|call| call.callee == name) else {
            // A callee that runs away in itself is cut short: the replay has the stack of its caller.
            if !self.held.contains(name) && self.bodies.contains(name) && self.running < RUNS {
                self.running += 1;
                let result = vm.call_named(self, &format!("{name}{BODY}"), args.to_vec());
                self.running -= 1;
                return result;
            }
            let recorded = recorded.map(|call| call.callee.clone());
            return Err(self.differ(Difference::Call { index, recorded, replayed: name.to_owned() }));
        };
        self.check(index, call, ty, args)?;
        // The answer is the callee's, not the function's: it stays out of the log.
        self.wrote(vm.memory.disarm());
        let mut returned = None;
        for (register, value) in &call.results {
            if register == RETURN {
                returned = Some(*value);
                continue;
            }
            let Some((offset, size)) = self.shape.place(register) else {
                return Err(self.differ(Difference::Answer { index, callee: call.callee.clone(), register: register.clone() }));
            };
            vm.memory.write(self.state.0 + offset as u64, &value.to_le_bytes()[..size])?;
        }
        for (address, bytes) in &call.effects {
            vm.memory.write(*address, bytes)?;
        }
        vm.memory.arm();
        self.next += 1;
        if self.stops && self.next == vector.calls.len() {
            self.stopped = true;
            return Err(match &vector.left {
                Left::Exit(code) => Stop::Exit(*code as i32),
                Left::Trap { reason, .. } => Stop::fault(reason.clone()),
                other => Stop::fault(describe(other)),
            });
        }
        Ok(match (returned, &ty.ret) {
            (Some(bits), Type::Float) => Value::F32(f32::from_bits(bits as u32)),
            (Some(bits), Type::Double) => Value::F64(f64::from_bits(bits as u64)),
            (Some(bits), _) => Value::Int(bits),
            (None, _) => Value::Int(0),
        })
    }
}

/// Replays every vector against `lifted`, raised to `level`, and gives the ones that differ.
pub fn replay(image: &Image, lifted: &Lifted, level: RaiseLevel, vectors: &[Vector]) -> Result<Vec<Failure>> {
    let replayer = Replayer::new(image, lifted, level)?;
    let mut numbers: HashMap<&str, usize> = HashMap::new();
    let mut failures = Vec::new();
    for vector in vectors {
        let count = numbers.entry(vector.function.as_str()).or_default();
        *count += 1;
        let differences = replayer.replay(vector)?;
        if !differences.is_empty() {
            failures.push(Failure { function: vector.function.clone(), vector: *count, differences });
        }
    }
    Ok(failures)
}
