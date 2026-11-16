//! The residual engine: a function the static analysis never found, lifted and run from the
//! memory of the program that reached it.
//!
//! A port meets such a function as a call to an address no lifted function has (a target the
//! program computed at run time, code it unpacked). The engine takes a reader of the program's
//! memory, a machine state and the address: it decodes the function there out of the bytes the
//! memory holds now, lifts it at the machine level, and interprets it over the state. Every call
//! the function makes to an address it knows (a direct call, a computed one) and every import it
//! calls is handed back to the caller, who runs the callee in the program's own way; the engine
//! runs one function and no more.
//!
//! A lift is kept by the function's address and the digest of its bytes: the same bytes run the
//! same lift again, changed bytes (code the program rewrote) are decoded and lifted anew, and
//! the earlier lift stays for the bytes it was made of.
//!
//! The image the engine is made with gives what the bytes alone do not: its sections, its
//! imports and its machine. Its sections are read again from the memory at each lift, so a
//! jump table or an import slot is read as the program holds it.
//!
//! What a run does not do: the memory is read, never written; the bytes the function wrote come
//! back with the outcome. Only the image and a window of the stack around the stack pointer are
//! mapped, so an access elsewhere (the heap, another module) stops the run as a fault that names
//! the address.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Result, anyhow, ensure};
use ra_asm_analysis::{Analysis, Facts, builder};
use ra_asm_ir::interp::memory::Access;
use ra_asm_ir::interp::{Host, Memory, Region, Value, Vm};
use ra_asm_ir::{Declaration, FnType, Module, Type};
use ra_asm_lift::{Isa, LiftOptions, RaiseLevel, X87Unit};
use ra_asm_pe::{Image, Toolset};

pub use ra_asm_ir::interp::Stop;

/// The C entry a Windows port links, over the port's own memory.
#[cfg(windows)]
pub mod c;

/// The grain of what the engine maps: the program's pages and its own.
const PAGE: u64 = 0x1000;

/// The alignment of the engine's own memory.
const GRAIN: u64 = 0x100_0000;

/// The frames of the interpreter's `alloca`s.
const FRAMES: u64 = 1 << 20;

/// The memory of the program, as the engine reads it.
pub trait Reader {
    /// Fills `buf` with the bytes at `address`; false when any of them is not readable.
    fn read(&self, address: u64, buf: &mut [u8]) -> bool;
}

/// The registers of a machine, laid out as the lift's `%State` is (see [`ra_asm_lift::state`]):
/// the general registers, the program counter, the flags as bytes and the vector registers. On
/// x86 the general registers are in the encoding's order (`rax`, `rcx`, `rdx`, `rbx`, `rsp`, …),
/// the flags are `cf`, `pf`, `af`, `zf`, `sf`, `df`, `of`, and the vector slots hold `xmm0..15`
/// and above them the upper halves of `ymm0..15`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    isa: Isa,
    bytes: Vec<u8>,
}

/// Where the fields of a `%State` lie: its general slots, and the offset of each field.
struct Layout {
    general: usize,
    counter: usize,
    flags: usize,
    vectors: usize,
    size: usize,
}

impl Layout {
    fn of(isa: Isa) -> Self {
        let fields = ra_asm_lift::state::state_fields_for(isa);
        let [Type::Array(general, _), _, _, Type::Array(_, _), ..] = fields.as_slice() else {
            unreachable!("a %State is its general registers, its counter, its flags and its vector registers")
        };
        let general = *general as usize;
        let counter = 8 * general;
        let size = usize::try_from(isa.spec().size()).expect("a state's size");
        Self { general, counter, flags: counter + 8, vectors: counter + 16, size }
    }
}

impl State {
    /// The state of `isa` with every register zero.
    pub fn new(isa: Isa) -> Self {
        Self { isa, bytes: vec![0; Layout::of(isa).size] }
    }

    pub fn isa(&self) -> Isa {
        self.isa
    }

    /// The bytes of the state, as the lifted code reads them.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    fn word(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.bytes[at..at + 8].try_into().expect("a word of the state"))
    }

    fn set_word(&mut self, at: usize, value: u64) {
        self.bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// The general register `n`.
    pub fn general(&self, n: usize) -> u64 {
        assert!(n < Layout::of(self.isa).general, "no general register {n} in the state");
        self.word(8 * n)
    }

    pub fn set_general(&mut self, n: usize, value: u64) {
        assert!(n < Layout::of(self.isa).general, "no general register {n} in the state");
        self.set_word(8 * n, value);
    }

    /// The program counter's slot.
    pub fn counter(&self) -> u64 {
        self.word(Layout::of(self.isa).counter)
    }

    pub fn set_counter(&mut self, value: u64) {
        self.set_word(Layout::of(self.isa).counter, value);
    }

    /// The flag byte `n`.
    pub fn flag(&self, n: usize) -> u8 {
        assert!(n < 8, "no flag {n} in the state");
        self.bytes[Layout::of(self.isa).flags + n]
    }

    pub fn set_flag(&mut self, n: usize, value: u8) {
        assert!(n < 8, "no flag {n} in the state");
        self.bytes[Layout::of(self.isa).flags + n] = value;
    }

    /// The vector slot `n`.
    pub fn vector(&self, n: usize) -> u128 {
        let at = Layout::of(self.isa).vectors + 16 * n;
        u128::from_le_bytes(self.bytes[at..at + 16].try_into().expect("a vector slot of the state"))
    }

    pub fn set_vector(&mut self, n: usize, value: u128) {
        let at = Layout::of(self.isa).vectors + 16 * n;
        self.bytes[at..at + 16].copy_from_slice(&value.to_le_bytes());
    }

    /// The stack pointer: `rsp` (`esp` in its low half), ARM64's `sp`, Thumb's `r13`.
    pub fn stack_pointer(&self) -> u64 {
        self.general(match self.isa {
            Isa::X86 => 4,
            Isa::Arm64 => 31,
            Isa::Arm32 => 13,
            Isa::Mips | Isa::MipsN32 => 29,
        })
    }
}

/// A call the function made, which the engine hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call<'a> {
    /// The code at the address, called directly, jumped to, or reached through a computed
    /// target. The callee takes the state and answers in it, as the code there would: on x86 it
    /// pops the return address the call pushed.
    Address(u64),
    /// An import, by its symbol, with its arguments by the convention of its prototype. The
    /// callee answers with its result, the import wrapper of the lift puts it in the state.
    Import { name: &'a str, args: &'a [u64] },
}

/// What the caller of [`Engine::run`] answers the function's calls with.
pub trait Calls {
    /// Answers `call` over the function's machine: the result of an import, ignored for a call
    /// of an address, whose callee answers in the state. A stop ends the function's run with it.
    fn call(&mut self, call: Call<'_>, machine: &mut Machine<'_, '_>) -> Result<u64, Stop>;
}

/// The function's machine as a callee sees it: the state the function runs over and the memory
/// the engine mapped.
pub struct Machine<'a, 'm> {
    vm: &'a mut Vm<'m>,
    isa: Isa,
    state: u64,
    len: usize,
}

impl Machine<'_, '_> {
    /// The state as the function left it at the call.
    pub fn state(&mut self) -> Result<State, Stop> {
        Ok(State { isa: self.isa, bytes: self.vm.memory.read_vec(self.state, self.len as u64)? })
    }

    /// Puts `state` back for the function to go on with.
    pub fn set_state(&mut self, state: &State) -> Result<(), Stop> {
        if state.isa != self.isa {
            return Err(Stop::fault("a state of another machine"));
        }
        self.vm.memory.write(self.state, &state.bytes)
    }

    /// `len` bytes of the mapped memory at `address`.
    pub fn read(&mut self, address: u64, len: usize) -> Result<Vec<u8>, Stop> {
        self.vm.memory.read_vec(address, len as u64)
    }

    /// Writes the mapped memory at `address`: a callee's effect, which the outcome's writes of the
    /// function do not hold.
    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<(), Stop> {
        self.vm.memory.write(address, bytes)
    }
}

/// How a run of a function ended.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The state the function left.
    pub state: State,
    /// What stopped the function before it returned: a trap, a fault, a limit, an exit a callee
    /// answered with; none when it returned.
    pub stop: Option<Stop>,
    /// The bytes the function wrote itself, as it left them, as runs of adjacent addresses.
    pub written: Vec<(u64, Vec<u8>)>,
    /// Whether the lift came from the cache.
    pub cached: bool,
    /// The IR instructions executed.
    pub steps: u64,
}

/// One lift of a function, for the bytes it was made of.
struct Lift {
    /// The ranges of the function's blocks, and the digest of their bytes.
    extents: Vec<(u64, u64)>,
    digest: u64,
    module: Module,
    /// The function's name in the module.
    name: String,
    /// The functions it calls, declared in the module for the caller to answer, by address.
    callees: HashMap<String, u64>,
    imports: BTreeSet<String>,
    /// The bytes the module's globals and symbols take in the interpreter.
    statics: u64,
    warnings: Vec<String>,
}

/// The engine of one image: its lifts, by address.
pub struct Engine {
    image: Image,
    isa: Isa,
    lifts: HashMap<u64, Vec<Lift>>,
    hits: u64,
    misses: u64,
    /// The IR instructions a run executes at most; zero for no limit.
    pub limit: u64,
    /// The bytes of the stack mapped below the stack pointer and above it.
    pub stack_below: u64,
    pub stack_above: u64,
}

impl Engine {
    /// The engine of the program `image` is the image of.
    pub fn new(image: Image) -> Self {
        let isa = match image.machine() {
            ra_asm_pe::Machine::Arm64 => Isa::Arm64,
            ra_asm_pe::Machine::Arm => Isa::Arm32,
            _ => Isa::X86,
        };
        Self { image, isa, lifts: HashMap::new(), hits: 0, misses: 0, limit: 100_000_000, stack_below: 0x4_0000, stack_above: 0x1_0000 }
    }

    /// The machine of the image's states.
    pub fn isa(&self) -> Isa {
        self.isa
    }

    /// The runs that found their lift in the cache.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// The runs that lifted their function.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// What the lift of the function at `address` warned of, for the bytes `memory` holds there
    /// now; none when the engine has not lifted them.
    pub fn warnings(&self, memory: &dyn Reader, address: u64) -> Option<&[String]> {
        let lifts = self.lifts.get(&address)?;
        lifts.iter().find(|lift| Self::holds(memory, lift)).map(|lift| lift.warnings.as_slice())
    }

    /// Runs the function at `address` over `state`, its code and its data read from `memory`, its
    /// calls answered by `calls`. An error is a function the engine could not lift or a machine it
    /// could not set up; a stop of the function itself is the outcome's.
    pub fn run(&mut self, memory: &dyn Reader, state: &State, address: u64, calls: &mut dyn Calls) -> Result<Outcome> {
        ensure!(state.isa == self.isa, "a state of {:?} for a function of {:?}", state.isa, self.isa);
        let (index, cached) = self.lift_for(memory, address)?;
        let lift = &self.lifts[&address][index];

        let base = self.image.image_base();
        let image = (base, base.saturating_add(u64::from(self.image.size_of_image())));
        let sp = if self.isa == Isa::X86 && self.image.pointer_size() == 4 {
            state.stack_pointer() & 0xffff_ffff
        } else {
            state.stack_pointer()
        };
        let stack = (sp.saturating_sub(self.stack_below), sp.saturating_add(self.stack_above));
        let mut machine = Memory::new();
        let mut taken = Vec::new();
        for (start, bytes) in readable(memory, &[image, stack]) {
            let len = bytes.len() as u64;
            machine.add_region(Region::owned(start, len, "program"));
            machine.write(start, &bytes).map_err(setup)?;
            taken.push((start, start + len));
        }
        let state_len = (state.bytes.len() as u64).div_ceil(PAGE) * PAGE;
        let own_len = lift.statics + FRAMES + state_len;
        let own = free_range(&taken, own_len).ok_or_else(|| anyhow!("no room beside the program for the engine's own memory"))?;
        let (frames, at) = (own + lift.statics, own + lift.statics + FRAMES);
        machine.add_region(Region::owned(own, lift.statics, "globals"));
        machine.add_region(Region::owned(frames, FRAMES, "frames"));
        machine.add_region(Region::owned(at, state_len, "state"));

        let mut vm = Vm::new(&lift.module, machine);
        vm.limit = self.limit;
        vm.set_static_region(own, lift.statics);
        vm.set_frame_region(frames, FRAMES);
        vm.place().map_err(setup)?;
        vm.memory.write(at, &state.bytes).map_err(setup)?;
        let mut host = Answers {
            calls,
            callees: &lift.callees,
            imports: &lift.imports,
            isa: self.isa,
            state: at,
            len: state.bytes.len(),
            own: (own, own + own_len),
            written: BTreeSet::new(),
        };
        vm.memory.arm();
        let result = vm.call_named(&mut host, &lift.name, vec![Value::Int(u128::from(at))]);
        host.wrote(vm.memory.disarm());

        let mut written: Vec<(u64, Vec<u8>)> = Vec::new();
        for &address in &host.written {
            let byte = vm.memory.read_u8(address).map_err(setup)?;
            match written.last_mut() {
                Some((start, run)) if *start + run.len() as u64 == address => run.push(byte),
                _ => written.push((address, vec![byte])),
            }
        }
        let bytes = vm.memory.read_vec(at, state.bytes.len() as u64).map_err(setup)?;
        Ok(Outcome { state: State { isa: self.isa, bytes }, stop: result.err(), written, cached, steps: vm.steps })
    }

    /// The lift of the function at `address` for the bytes `memory` holds there, from the cache
    /// or made now: its index among the address's lifts, and whether it was cached.
    fn lift_for(&mut self, memory: &dyn Reader, address: u64) -> Result<(usize, bool)> {
        if let Some(index) = self.lifts.get(&address).and_then(|lifts| lifts.iter().position(|lift| Self::holds(memory, lift))) {
            self.hits += 1;
            return Ok((index, true));
        }
        let lift = self.lift(memory, address)?;
        self.misses += 1;
        let lifts = self.lifts.entry(address).or_default();
        lifts.push(lift);
        Ok((lifts.len() - 1, false))
    }

    /// Whether `memory` holds the bytes `lift` was made of.
    fn holds(memory: &dyn Reader, lift: &Lift) -> bool {
        bytes_of(memory, &lift.extents).is_some_and(|bytes| ra_asm_verify::digest(&bytes) == lift.digest)
    }

    /// Lifts the function at `address` out of the image as `memory` holds it: the function, and
    /// the functions it calls, whose bodies give way to declarations the caller answers.
    fn lift(&mut self, memory: &dyn Reader, address: u64) -> Result<Lift> {
        let end_of_image = self.image.va(self.image.size_of_image());
        let sections: Vec<(u64, u64, String)> = self
            .image
            .sections
            .iter()
            .map(|s| (self.image.va(s.virtual_address), self.image.va(s.end_rva()).min(end_of_image), s.name.clone()))
            .filter(|(start, end, _)| start < end)
            .collect();
        for (start, end, name) in sections {
            let mut bytes = vec![0; (end - start) as usize];
            ensure!(memory.read(start, &mut bytes), "the memory does not hold section {name} at {start:#x}");
            self.image.write(start, &bytes)?;
        }
        let image = &self.image;
        ensure!(image.is_code(address), "{address:#x} lies in no code section of the image");

        let arch = ra_asm_analysis::arch_of(image);
        let (functions, mut warnings) = {
            let build = |entry: u64| {
                let section = image.section_of(entry)?;
                let ranges = [(image.va(section.virtual_address), image.va(section.end_rva()))];
                let mut built = builder::build(image, &*arch, entry, &ranges, &BTreeSet::new(), &HashSet::new(), None);
                built.function.name = ra_asm_analysis::function::default_name(entry);
                Some(built)
            };
            let built = build(address).ok_or_else(|| anyhow!("{address:#x} lies in no section of the image"))?;
            let warnings = built.warnings;
            let callees: Vec<u64> = built.function.calls.iter().chain(&built.function.tail_calls).copied().collect();
            let mut functions = BTreeMap::from([(address, built.function)]);
            for callee in callees {
                if functions.contains_key(&callee) || !image.is_code(callee) {
                    continue;
                }
                if let Some(built) = build(callee) {
                    functions.insert(callee, built.function);
                }
            }
            (functions, warnings)
        };
        let extents: Vec<(u64, u64)> = functions[&address].blocks.iter().map(|b| (b.start, b.end)).collect();
        let digest = ra_asm_verify::digest(
            &bytes_of(memory, &extents).ok_or_else(|| anyhow!("the memory does not hold the function at {address:#x}"))?,
        );
        let analysis = Analysis {
            image,
            arch,
            functions,
            noreturn: HashSet::new(),
            classes: Vec::new(),
            cxx_frames: BTreeMap::new(),
            seh_frames: BTreeMap::new(),
            seh_unread: BTreeMap::new(),
            cxx_frames32: BTreeMap::new(),
            cxx_unread32: BTreeMap::new(),
            program_entry: None,
            address_taken: None,
            fill: Vec::new(),
            facts: Facts::default(),
            toolset: Toolset::of(image),
            warnings: Vec::new(),
        };

        let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: false, x87: X87Unit::Double });
        let name = lifted.functions.get(&address).cloned().unwrap_or_else(|| ra_asm_lift::function_name(address));
        // The function's callers are the program's, which the raise does not see.
        lifted.roots.insert(name.clone());
        let options = lifted.raise_options(RaiseLevel::Machine);
        ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        warnings.append(&mut lifted.warnings);

        // A callee was lifted for its name and its shape only: its call is the caller's to answer.
        let callees: HashMap<String, u64> =
            lifted.functions.iter().filter(|(va, _)| **va != address).map(|(va, name)| (name.clone(), *va)).collect();
        let mut module = lifted.module;
        let (declared, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut module.functions).into_iter().partition(|f| callees.contains_key(&f.name));
        module.functions = kept;
        for f in declared {
            module.declarations.push(Declaration { name: f.name, ty: f.ty, conv: f.conv, attrs: Vec::new() });
        }
        ensure!(module.functions.iter().any(|f| f.name == name), "the lift of {address:#x} holds no function {name}");
        let statics = statics_of(&module);
        Ok(Lift { extents, digest, module, name, callees, imports: lifted.imports, statics, warnings })
    }
}

/// The bytes of `extents` in `memory`, one range after the other.
fn bytes_of(memory: &dyn Reader, extents: &[(u64, u64)]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    for &(start, end) in extents {
        let mut range = vec![0; end.checked_sub(start)? as usize];
        if !memory.read(start, &mut range) {
            return None;
        }
        bytes.extend_from_slice(&range);
    }
    Some(bytes)
}

/// The readable pages of `ranges` in `memory`, joined into runs of adjacent pages.
fn readable(memory: &dyn Reader, ranges: &[(u64, u64)]) -> Vec<(u64, Vec<u8>)> {
    let pages: BTreeSet<u64> = ranges.iter().flat_map(|&(start, end)| (start / PAGE..end.div_ceil(PAGE)).map(|page| page * PAGE)).collect();
    let mut runs: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut page = vec![0; PAGE as usize];
    for at in pages {
        if !memory.read(at, &mut page) {
            continue;
        }
        match runs.last_mut() {
            Some((start, bytes)) if *start + bytes.len() as u64 == at => bytes.extend_from_slice(&page),
            _ => runs.push((at, page.clone())),
        }
    }
    runs
}

/// The lowest address aligned to [`GRAIN`] where `len` bytes overlap none of `taken`.
fn free_range(taken: &[(u64, u64)], len: u64) -> Option<u64> {
    let mut sorted = taken.to_vec();
    sorted.sort_unstable();
    let mut at = GRAIN;
    for (start, end) in sorted {
        if at.checked_add(len)? <= start {
            return Some(at);
        }
        if end > at {
            at = end.checked_next_multiple_of(GRAIN)?;
        }
    }
    at.checked_add(len).map(|_| at)
}

/// The bytes the globals without an address, the functions and the declarations of `module`
/// take in the interpreter's static region.
fn statics_of(module: &Module) -> u64 {
    let vm = Vm::new(module, Memory::new());
    let layout = vm.layout();
    let globals: u64 = module
        .globals
        .iter()
        .filter(|g| g.address.is_none())
        .map(|g| layout.size(&g.ty) + u64::from(g.align).max(layout.align(&g.ty)).max(16))
        .sum();
    let symbols = 16 * (module.functions.len() + module.declarations.len()) as u64;
    (symbols + globals + PAGE).div_ceil(PAGE) * PAGE
}

/// A stop while the machine is set up, as an error.
fn setup(stop: Stop) -> anyhow::Error {
    match stop {
        Stop::Fault(message) => anyhow!("{message}"),
        other => anyhow!("{other:?}"),
    }
}

/// The value an answer of `bits` is at the type a call takes it at.
fn value_of(bits: u64, ty: &Type) -> Value {
    match ty {
        Type::Float => Value::F32(f32::from_bits(bits as u32)),
        Type::Double => Value::F64(f64::from_bits(bits)),
        Type::Int(width) => Value::int(u128::from(bits), u32::from(*width)),
        _ => Value::Int(u128::from(bits)),
    }
}

/// The interpreter's host: the calls of the module it cannot answer, handed to the caller.
struct Answers<'a> {
    calls: &'a mut dyn Calls,
    callees: &'a HashMap<String, u64>,
    imports: &'a BTreeSet<String>,
    isa: Isa,
    /// The state the function runs over, and its bytes.
    state: u64,
    len: usize,
    /// The engine's own memory: the globals, the frames and the state.
    own: (u64, u64),
    /// The addresses the function wrote itself, outside the engine's own memory.
    written: BTreeSet<u64>,
}

impl Answers<'_> {
    /// Takes the log of what the function wrote since the memory was armed.
    fn wrote(&mut self, access: Access) {
        let own = self.own;
        for (start, bytes) in access.writes {
            self.written.extend((start..).take(bytes.len()).filter(|address| !(own.0..own.1).contains(address)));
        }
    }

    /// Hands `call` to the caller. What the callee does is not the function's: it stays out of
    /// the log.
    fn hand_back(&mut self, vm: &mut Vm<'_>, call: Call<'_>) -> Result<u64, Stop> {
        self.wrote(vm.memory.disarm());
        let result = {
            let mut machine = Machine { vm: &mut *vm, isa: self.isa, state: self.state, len: self.len };
            self.calls.call(call, &mut machine)
        };
        vm.memory.arm();
        result
    }
}

impl Host for Answers<'_> {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let arg = |n: usize| args.get(n).map_or(0, Value::as_u64);
        match name {
            "ra_trap" => return Err(Stop::fault(format!("trap {} at {:#x}", arg(1), arg(0)))),
            "ra_unsupported" => {
                return Err(Stop::fault(format!("an instruction the lift does not cover at {:#x} (code {})", arg(0), arg(1))));
            }
            "ra_call" => {
                self.hand_back(vm, Call::Address(arg(1)))?;
                return Ok(Value::Int(0));
            }
            _ => {}
        }
        if let Some(&address) = self.callees.get(name) {
            self.hand_back(vm, Call::Address(address))?;
            return Ok(Value::Int(0));
        }
        if self.imports.contains(name) {
            let values: Vec<u64> = args.iter().map(Value::as_u64).collect();
            let result = self.hand_back(vm, Call::Import { name, args: &values })?;
            return Ok(value_of(result, &ty.ret));
        }
        Err(Stop::fault(format!("a call of {name}, which the engine does not answer")))
    }
}
