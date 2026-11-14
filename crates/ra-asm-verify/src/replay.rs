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
//!
//! At the ABI level ([`Replayer::with_signatures`]) a function has no state: it takes arguments
//! and returns a value. The vector is read through the function's signature (see
//! [`crate::signature`]): the entry gives the arguments, and the comparison keeps to what the
//! signature says outlives the activation, which is the result at the width its type keeps, the
//! stack pointer the function leaves, the registers it keeps for its caller, and the memory
//! outside its frame. A call is answered by the callee's signature in the same way, its result
//! out of the registers the record holds. What this level does not decide beside the above:
//! - the record was made over a state, where a callee reads its arguments itself, so a call of a
//!   lifted function or an import is compared by its callee and its place in the order alone; a
//!   call through a pointer is compared by the address called;
//! - the frame is the replay's own, elsewhere than the recorded one, so what a callee wrote into
//!   the function's frame through a pointer it was handed is not seen by the function;
//! - what an x86 callee that is no lifted function popped, and the float an x86 import left on
//!   the x87 stack, are in no register of a vector: a function that needs one differs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use anyhow::{Result, anyhow, bail};
use ra_asm_ir::interp::memory::Access;
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm, mask};
use ra_asm_ir::{Const, Declaration, FnType, Module, Type};
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::{Lifted, RaiseLevel, import_of_wrapper};
use ra_asm_pe::Image;

use crate::record::{RETURN, Shape, left};
use crate::signature::Convention;
use crate::vector::{Call, Left, Register, Vector};

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

/// The runtime's call through a pointer at the levels that take a state, with the state and the
/// address called.
const RA_CALL: &str = "ra_call";

/// What the ABI level asks the runtime after an x86 call through a pointer: the bytes the callee
/// popped beyond its return address.
const POPS_OF: &str = "ra_pops_of";

/// What the name of an address starts with that the record called through and that is no lifted
/// function's: the replay names it, so a call of it comes to the host. No name of the module has
/// a space.
const NATIVE: &str = "ra_call at ";

/// Where an x86 import leaves a float, as a line names it.
const ST0: &str = "st0";

/// One way a replay differs from its vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// The module has no function of the vector's name in the shape of the replayed level: over
    /// a state, or with a signature.
    Function,
    /// The function takes another number of parameters than its signature gives arguments.
    Parameters { signature: usize, function: usize },
    /// The record does not hold `register`, which the call's answer lies in at the ABI level.
    Unanswered { index: usize, callee: String, register: String },
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
            Self::Function => f.write_str("the module has no function of this name in the shape of the replayed level"),
            Self::Parameters { signature, function } => {
                write!(f, "the function takes {function} parameters where its signature gives {signature} arguments")
            }
            Self::Unanswered { index, callee, register } => {
                write!(f, "call {index} of {callee}: the record does not hold {register}, which the answer lies in")
            }
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
    /// The address bits of the image: a 32-bit function keeps its addresses in a dword.
    bits: u32,
    /// What the ABI level reads a vector through; none at the levels that take a state.
    abi: Option<Abi>,
    /// The instructions a replay of one vector runs at most; zero for no limit.
    pub limit: u64,
}

/// What a replayer of the ABI level knows beside the module.
struct Abi {
    convention: Convention,
    signatures: HashMap<String, Signature>,
    /// The lifted functions by the names a call reaches them under, their own and the one of
    /// the universal entry an x86 pointer names, each with its address in the image and its own
    /// name.
    lifted: HashMap<String, (u64, String)>,
}

/// What a call of the ABI level reached.
enum Called<'a> {
    /// A lifted function, by its address in the image and its name.
    Lifted { address: u64, function: &'a str },
    /// An address the record called through that is no lifted function's.
    Native(u64),
    /// An import, a function of the runtime, or a function the raise added.
    Other,
}

impl Abi {
    fn called(&self, name: &str) -> Called<'_> {
        if let Some((address, function)) = self.lifted.get(name) {
            return Called::Lifted { address: *address, function: function.as_str() };
        }
        match name.strip_prefix(NATIVE).and_then(|hex| u64::from_str_radix(hex, 16).ok()) {
            Some(address) => Called::Native(address),
            None => Called::Other,
        }
    }
}

impl Called<'_> {
    /// Whether the record's `call` is this call, made as `name`. The record's level called an
    /// import through its wrapper and a pointer through the runtime.
    fn is(&self, call: &Call, name: &str) -> bool {
        let through = |address: u64| call.callee == RA_CALL && call.args.len() == 2 && call.args[1] == address;
        match self {
            Self::Lifted { address, function } => call.callee == *function || through(*address),
            Self::Native(address) => through(*address),
            Self::Other => call.callee == name || import_of_wrapper(&call.callee) == Some(name),
        }
    }
}

/// The addresses `vector` called through a pointer.
fn targets(vector: &Vector) -> impl Iterator<Item = u64> + '_ {
    vector.calls.iter().filter(|call| call.callee == RA_CALL && call.args.len() == 2).map(|call| call.args[1])
}

/// The ranges of a replay's machine that are not the program's.
struct Ranges {
    /// The state a function of the machine or the promoted level runs over.
    state: (u64, u64),
    /// The interpreter's own memory: the globals, the frames and the state.
    own: (u64, u64),
}

/// A stop while the machine of a vector is made, as an error.
fn setup(vector: &Vector, stop: Stop) -> anyhow::Error {
    match stop {
        Stop::Fault(message) => anyhow!("{}: {message}", vector.function),
        other => anyhow!("{}: {other:?}", vector.function),
    }
}

impl<'a> Replayer<'a> {
    /// A replayer of `lifted`, raised to `level`: the machine or the promoted level, whose
    /// functions take the state a vector is loaded into.
    pub fn new(image: &'a Image, lifted: &Lifted, level: RaiseLevel) -> Result<Self> {
        if level == RaiseLevel::Abi {
            bail!("a vector replays at the ABI level through the signatures of its functions: the level has no state to load it into");
        }
        Self::over(image, lifted, level == RaiseLevel::Machine)
    }

    /// A replayer of `lifted`, raised to the ABI level, whose functions take the arguments
    /// their `signatures` say: what the raise returned, under the names the functions have now.
    pub fn with_signatures(image: &'a Image, lifted: &Lifted, signatures: &HashMap<String, Signature>) -> Result<Self> {
        let mut replayer = Self::over(image, lifted, false)?;
        let mut names = HashMap::with_capacity(2 * lifted.functions.len());
        for (address, name) in &lifted.functions {
            names.insert(name.clone(), (*address, name.clone()));
            let entry = ra_asm_lift::raise::stubs::stub_name(&ra_asm_lift::function_name(*address));
            names.insert(entry, (*address, name.clone()));
        }
        replayer.abi = Some(Abi { convention: Convention::new(lifted.isa, lifted.bits), signatures: signatures.clone(), lifted: names });
        Ok(replayer)
    }

    /// The replayer of `lifted` at a level that keeps the flags across a call when `flags`.
    fn over(image: &'a Image, lifted: &Lifted, flags: bool) -> Result<Self> {
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
            flags,
            standalone: lifted.standalone,
            statics,
            bits: lifted.bits,
            abi: None,
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
        let Some(f) = self.module.function(&body) else { return Ok(vec![Difference::Function]) };
        let over_state = f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void;
        // The shape the level gives the function: a signature at the ABI level, the state below.
        let level = match &self.abi {
            None if over_state => None,
            None => return Ok(vec![Difference::Function]),
            Some(abi) => match abi.signatures.get(&vector.function) {
                Some(signature) => Some((abi, signature)),
                None => return Ok(vec![Difference::Function]),
            },
        };
        let fault = |stop: Stop| setup(vector, stop);
        let (mut vm, ranges) = self.machine(vector)?;
        let state = ranges.state.0;

        // The entry: the registers into the state, or the arguments the signature reads of them.
        let arguments = match level {
            None => {
                for (name, value) in &vector.entry {
                    let (offset, size) = self.place(vector, name)?;
                    vm.memory.write(state + offset as u64, &value.to_le_bytes()[..size]).map_err(fault)?;
                }
                vec![Value::Int(u128::from(state))]
            }
            Some((abi, signature)) => {
                let arguments = abi.convention.arguments(signature, vector)?;
                if arguments.len() != f.ty.params.len() {
                    return Ok(vec![Difference::Parameters { signature: arguments.len(), function: f.ty.params.len() }]);
                }
                arguments
            }
        };

        let mut host = Answers {
            vector,
            shape: &self.shape,
            bodies: &self.bodies,
            abi: self.abi.as_ref(),
            held: vector.calls.iter().map(|call| call.callee.as_str()).collect(),
            stops,
            state: ranges.state,
            own: ranges.own,
            next: 0,
            running: 0,
            stopped: false,
            popped: None,
            written: BTreeSet::new(),
            differences: Vec::new(),
        };
        vm.memory.arm();
        let result = vm.call_named(&mut host, &body, arguments);
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
        let mut dead = None;
        match level {
            // A promoted function that stopped by itself holds its registers in values still.
            None if self.flags || host.stopped || matches!(vector.left, Left::Return | Left::Exit(_)) => {
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
            None => {}
            Some((abi, signature)) => {
                if let (Left::Return, Ok(value)) = (&vector.left, &result) {
                    returned(&abi.convention, signature, &f.ty.ret, vector, value, &mut differences)?;
                }
                dead = Some(abi.convention.frame(signature, abi.convention.entry_stack_pointer(vector)?));
            }
        }
        // The frame died with the activation, and the ABI level keeps its own elsewhere.
        let outlives = |address: &u64| !dead.is_some_and(|(start, end)| (start..end).contains(address));
        let recorded: BTreeMap<u64, u8> = vector
            .memory_out
            .iter()
            .flat_map(|(start, bytes)| (*start..).zip(bytes.iter().copied()))
            .filter(|(address, _)| outlives(address))
            .collect();
        let mut written = BTreeMap::new();
        for &address in host.written.iter().filter(|address| outlives(address)) {
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

    /// The machine `vector` replays in, with the bytes of its `memory in` loaded: the image, the
    /// pages of the vector, and a range the vector does not touch for the interpreter.
    fn machine(&self, vector: &Vector) -> Result<(Vm<'_>, Ranges)> {
        let fault = |stop: Stop| setup(vector, stop);
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
        // An address the function called through is no page of the vector's, and must not come
        // to name something of the replay's own.
        taken.extend(targets(vector).map(|target| (target / PAGE * PAGE, (target / PAGE * PAGE).saturating_add(PAGE))));
        taken.push(image);
        taken.sort_unstable();
        let state_len = (self.shape.size() as u64).div_ceil(PAGE) * PAGE;
        let own_len = self.statics + FRAMES + state_len;
        let own = free_range(&taken, own_len).ok_or_else(|| anyhow!("{}: no room for the replay's own memory", vector.function))?;
        // The frame of a 32-bit function of the ABI level is addressed by a dword.
        if self.abi.is_some() && self.bits < 64 && (own + own_len) >> self.bits != 0 {
            bail!("{}: no room below {:#x} for the replay's own memory", vector.function, 1u64 << self.bits);
        }
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
        if self.abi.is_some() {
            // A pointer the program holds is an address of the image, in the bytes of the vector
            // if not in the data: it reaches the function of that address, which is the host's
            // to answer here, as the table of the runtime names it.
            if let Some(Const::Array(entries)) = self.module.global("ra_functions").and_then(|g| g.init.as_ref()) {
                for entry in entries {
                    if let Const::Struct(fields) = entry
                        && let (Some(Const::Int(address)), Some(Const::Global(name))) = (fields.first(), fields.get(1))
                    {
                        vm.alias(*address as u64, name);
                    }
                }
            }
            for target in targets(vector) {
                if vm.symbol_at(target).is_none() {
                    let name = format!("{NATIVE}{target:x}");
                    vm.bind_external(&name).map_err(fault)?;
                    vm.alias(target, &name);
                }
            }
        }
        for (address, bytes) in &vector.memory_in {
            vm.memory.write(*address, bytes).map_err(fault)?;
        }
        Ok((vm, Ranges { state: (state, state + state_len), own: (own, own + own_len) }))
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

/// The value a callee outside the state's shape returned, at the type the call takes it at.
fn value_of(bits: u128, ty: &Type) -> Value {
    match ty {
        Type::Float => Value::F32(f32::from_bits(bits as u32)),
        Type::Double => Value::F64(f64::from_bits(bits as u64)),
        _ => Value::Int(bits),
    }
}

/// What a function of `signature` that returned `value` at the type `ty` left otherwise than
/// `vector` holds, of what outlives a function without a state: the result, the stack pointer
/// by what the signature pops, and the registers the function keeps for its caller, which a
/// function that has none of them cannot have changed.
fn returned(
    convention: &Convention,
    signature: &Signature,
    ty: &Type,
    vector: &Vector,
    value: &Value,
    differences: &mut Vec<Difference>,
) -> Result<()> {
    match convention.result(signature.returns, ty, &vector.exit) {
        Ok(Some(result)) if !result.is(value) => differences.push(Difference::Register {
            recorded: result.value.as_u128(),
            replayed: value.as_u128() & mask(result.bits),
            name: result.register,
        }),
        Ok(_) => {}
        Err(register) => bail!("{}: the vector's exit does not hold {register}", vector.function),
    }
    let width = mask(convention.target().bits);
    if let Some(sp) = convention.stack_pointer()
        && let (Some(entry), Some(exit)) = (held(&vector.entry, sp), held(&vector.exit, sp))
    {
        let replayed = u128::from(convention.exit_stack_pointer(signature, entry as u64));
        if exit & width != replayed {
            differences.push(Difference::Register { name: sp.to_owned(), recorded: exit & width, replayed });
        }
    }
    for name in convention.kept(signature) {
        if let (Some(entry), Some(exit)) = (held(&vector.entry, name), held(&vector.exit, name))
            && entry != exit
        {
            differences.push(Difference::Register { name: name.to_owned(), recorded: exit, replayed: entry });
        }
    }
    Ok(())
}

/// The value of the register `name` among `registers`, when they hold it.
fn held(registers: &[Register], name: &str) -> Option<u128> {
    registers.iter().find(|(register, _)| register == name).map(|(_, value)| *value)
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
    /// What the ABI level answers a call through; none at the levels that take a state.
    abi: Option<&'a Abi>,
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
    /// What the lifted function last answered popped beyond its return address, for the x86
    /// function that called it through a pointer and asks the runtime; none when the callee
    /// was no lifted function, whose pops no vector holds.
    popped: Option<u64>,
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

    /// Counts a call as answered. The record's last call is the one the activation stopped in
    /// when the replay reads it so: then the replay stops there too, as the record did.
    fn answered(&mut self) -> Result<(), Stop> {
        self.next += 1;
        if !self.stops || self.next != self.vector.calls.len() {
            return Ok(());
        }
        self.stopped = true;
        Err(match &self.vector.left {
            Left::Exit(code) => Stop::Exit(*code as i32),
            Left::Trap { reason, .. } => Stop::fault(reason.clone()),
            other => Stop::fault(describe(other)),
        })
    }

    /// Answers a call of the ABI level, where the callee takes its own arguments and has no
    /// state: the record's next call must be of the same callee, as its level called it; what
    /// the callee wrote goes into the memory, and the result comes out of the registers the
    /// record holds, by the callee's signature.
    fn answer(&mut self, vm: &mut Vm<'_>, abi: &Abi, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let vector = self.vector;
        // The pops of a call through a pointer are the runtime's to say; the record's level
        // never asked for them.
        if name == POPS_OF {
            let Some(pops) = self.popped.take() else {
                let callee = vector.calls.get(self.next.wrapping_sub(1)).map_or_else(|| name.to_owned(), |call| call.callee.clone());
                let register = abi.convention.stack_pointer().unwrap_or_default().to_owned();
                return Err(self.differ(Difference::Unanswered { index: self.next, callee, register }));
            };
            return Ok(Value::Int(u128::from(pops)));
        }
        let called = abi.called(name);
        let recorded = vector.calls.get(self.next);
        let index = self.next + 1;
        let Some(call) = recorded.filter(|call| called.is(call, name)) else {
            // A function the raise added has no record: it runs, its own calls answered.
            if matches!(called, Called::Other) && self.bodies.contains(name) && self.running < RUNS {
                self.running += 1;
                let result = vm.call_named(self, &format!("{name}{BODY}"), args.to_vec());
                self.running -= 1;
                return result;
            }
            let recorded = recorded.map(|call| call.callee.clone());
            return Err(self.differ(Difference::Call { index, recorded, replayed: name.to_owned() }));
        };
        // A function of the runtime is called with the same arguments at every level.
        if call.callee == name && !call.args.is_empty() {
            self.check(index, call, ty, args)?;
        }
        self.wrote(vm.memory.disarm());
        for (address, bytes) in &call.effects {
            vm.memory.write(*address, bytes)?;
        }
        vm.memory.arm();
        self.answered()?;
        let returns = match called {
            Called::Lifted { function, .. } => {
                let signature = abi.signatures.get(function);
                self.popped = Some(signature.map_or(0, |signature| signature.pops));
                signature.and_then(|signature| signature.returns)
            }
            Called::Native(_) => {
                self.popped = None;
                Some(Kind::Int)
            }
            Called::Other => {
                if let Some((_, bits)) = call.results.iter().find(|(register, _)| register == RETURN) {
                    return Ok(value_of(*bits, &ty.ret));
                }
                Some(Kind::Int)
            }
        };
        // An x86 callee that is no lifted function leaves a float on the x87 stack.
        let float = matches!(ty.ret, Type::Float | Type::Double);
        if float && !abi.convention.target().is_64() && !matches!(called, Called::Lifted { .. }) {
            return Err(self.differ(Difference::Unanswered { index, callee: call.callee.clone(), register: ST0.to_owned() }));
        }
        match abi.convention.result(returns, &ty.ret, &call.results) {
            Ok(Some(result)) => Ok(result.value),
            Ok(None) => Ok(Value::Int(0)),
            Err(register) => Err(self.differ(Difference::Unanswered { index, callee: call.callee.clone(), register })),
        }
    }
}

impl Host for Answers<'_> {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        if let Some(abi) = self.abi {
            return self.answer(vm, abi, name, ty, args);
        }
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
        self.answered()?;
        Ok(returned.map_or(Value::Int(0), |bits| value_of(bits, &ty.ret)))
    }
}

/// Replays every vector against `lifted`, raised to `level`, and gives the ones that differ.
pub fn replay(image: &Image, lifted: &Lifted, level: RaiseLevel, vectors: &[Vector]) -> Result<Vec<Failure>> {
    failures(&Replayer::new(image, lifted, level)?, vectors)
}

/// Replays every vector against `lifted`, raised to the ABI level with `signatures`, and gives
/// the ones that differ.
pub fn replay_abi(image: &Image, lifted: &Lifted, signatures: &HashMap<String, Signature>, vectors: &[Vector]) -> Result<Vec<Failure>> {
    failures(&Replayer::with_signatures(image, lifted, signatures)?, vectors)
}

/// The vectors that differ under `replayer`, each numbered among the vectors of its function.
fn failures(replayer: &Replayer<'_>, vectors: &[Vector]) -> Result<Vec<Failure>> {
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
