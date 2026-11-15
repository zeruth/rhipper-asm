//! Generated vectors: inputs for a function the recorded runs never entered.
//!
//! A function no run entered has no vector, and so no evidence. Where the function at the ABI
//! level reads and writes only its arguments, its frame and its result ([`summary`]), what it was
//! given is known in full, and inputs can be made for it: [`COUNT`] of them, the first half of
//! boundary values (zero, one, all ones, the edges of each signed and unsigned width), the rest
//! random from a seed, so a run gives the same vectors again. A parameter the function reads or
//! writes memory through points into a page of its own, filled the same way.
//!
//! The machine level is the reference: each input runs there, in a machine of its own over a
//! fresh state, the recorder ([`crate::record`]) watching, and what the recorder keeps of the one
//! activation is the vector. The levels above then replay it as they replay a recorded one. An
//! input the machine level does not return from (a division by zero of a boundary value, an access
//! past the page) is no vector, and another input is made in its place, [`ATTEMPTS`] at most.
//!
//! What the summary does not take for pure, and generates nothing for: a call of anything but an
//! LLVM intrinsic, a global or an address of the image named anywhere, an access through a value
//! read from memory, a parameter on the stack of a variadic function, and a stack pointer moved by
//! a run-time amount.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Result, anyhow, bail};
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{BinOp, Callee, FnType, Function, Inst, Module, Operand, ValueId};
use ra_asm_lift::Lifted;
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::run::Watch;
use ra_asm_pe::Image;

use crate::record::{GLOBALS, Recorder, Shape};
use crate::replay::LIMIT;
use crate::vector::{Left, Vector};

/// The vectors generated for a function.
pub const COUNT: usize = 16;

/// The inputs made for a function at most, the ones it did not return from included.
pub const ATTEMPTS: usize = 4 * COUNT;

/// The seed `ras verify --generate` makes its inputs from.
pub const SEED: u64 = 0x5eed_c0de_f00d_0001;

/// The inputs of the boundary values, ahead of the random ones.
const BOUNDARY: usize = COUNT / 2;

/// The integers an input of the boundary values takes its values from.
const INTS: [u64; 14] =
    [0, 1, u64::MAX, 2, 0x7f, 0x80, 0xff, 0x7fff, 0x8000, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, i64::MAX as u64, i64::MIN as u64];

/// The floating values an input of the boundary values takes its values from. No NaN: its bits
/// are the interpreter's to choose where an operation makes one.
const FLOATS: [f64; 8] = [0.0, 1.0, -1.0, -0.0, 0.5, 1e10, -2.5, f64::MIN_POSITIVE];

const PAGE: u64 = 0x1000;
const GRAIN: u64 = 0x1_0000;

/// The bytes of the stack an input runs on, and the bytes of it above the stack pointer: the
/// return address, the home of the register parameters and the stack parameters.
const STACK: u64 = 0x1_0000;
const ABOVE: u64 = 0x800;

/// The bytes of the interpreter's frames an input runs with.
const FRAMES: u64 = 1 << 20;

/// What the ABI level says a function touches: the parameters it reads or writes memory
/// through, by their place among the parameters. Its frame and its result are its own besides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub pointers: BTreeSet<usize>,
}

/// What an address of a function is derived from: its frame, or one of its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    Frame,
    Param(usize),
}

/// What `f`, a function of the ABI level, touches, when it reads and writes nothing but its
/// arguments, the memory a parameter points to, its frame and its result; none when it calls
/// anything but an LLVM intrinsic, names a global or an address of the image, or reads or writes
/// memory through an address no parameter and no `alloca` of its own gives.
#[must_use]
pub fn summary(f: &Function) -> Option<Summary> {
    let named = |operand: &&Operand| matches!(operand, Operand::Global(_) | Operand::Address { .. });
    for block in &f.blocks {
        if block.terminator.operands().iter().any(named) {
            return None;
        }
        for i in &block.instrs {
            if i.inst.operands().iter().any(named) {
                return None;
            }
            if let Inst::Call { callee, .. } = &i.inst
                && !matches!(callee, Callee::Direct(name) if name.starts_with("llvm."))
            {
                return None;
            }
        }
    }
    let mut bases: HashMap<ValueId, Base> = f.params.iter().enumerate().map(|(n, v)| (*v, Base::Param(n))).collect();
    // A value takes the base of what it is computed from; a loop's value through a phi takes none
    // until the values around the loop have one, which may be never, and an access through it
    // then makes the function impure: the summary errs on the side of generating nothing.
    loop {
        let mut changed = false;
        for i in f.blocks.iter().flat_map(|block| &block.instrs) {
            let Some(result) = i.result else { continue };
            if bases.contains_key(&result) {
                continue;
            }
            if let Some(base) = derived(&i.inst, &bases) {
                bases.insert(result, base);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut pointers = BTreeSet::new();
    for i in f.blocks.iter().flat_map(|block| &block.instrs) {
        let (Inst::Load { ptr, .. } | Inst::Store { ptr, .. }) = &i.inst else { continue };
        match ptr.local().and_then(|v| bases.get(&v)) {
            Some(Base::Frame) => {}
            Some(Base::Param(n)) => {
                pointers.insert(*n);
            }
            None => return None,
        }
    }
    Some(Summary { pointers })
}

/// The base of the value `inst` gives, from the bases of its operands.
fn derived(inst: &Inst, bases: &HashMap<ValueId, Base>) -> Option<Base> {
    let base = |operand: &Operand| operand.local().and_then(|v| bases.get(&v).copied());
    match inst {
        Inst::Alloca { .. } => Some(Base::Frame),
        Inst::Gep { ptr, .. } => base(ptr),
        Inst::Cast { value, .. } | Inst::Freeze { value, .. } => base(value),
        // An offset or an alignment of an address keeps its base; the sum of two addresses is none.
        Inst::Bin { op: BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or, lhs, rhs, .. } => match (base(lhs), base(rhs)) {
            (Some(b), None) | (None, Some(b)) => Some(b),
            _ => None,
        },
        Inst::Phi { incoming, .. } => same(incoming.iter().map(|(operand, _)| operand), base),
        Inst::Select { then, els, .. } => same([then, els].into_iter(), base),
        _ => None,
    }
}

/// The one base of `operands`, when every local among them has it and the others are constants.
fn same<'a>(operands: impl Iterator<Item = &'a Operand>, base: impl Fn(&Operand) -> Option<Base>) -> Option<Base> {
    let mut found = None;
    for operand in operands {
        if operand.local().is_none() {
            continue;
        }
        let here = base(operand)?;
        if found.is_some_and(|b| b != here) {
            return None;
        }
        found = Some(here);
    }
    found
}

/// The vectors generated for the functions of `machine`, the machine level, that `recorded`
/// holds none of and that `abi`, the same image at the ABI level with its `signatures`, says are
/// pure ([`summary`]): [`COUNT`] each at most, made from `seed`, in the order of the functions'
/// addresses. The machine level runs each input and gives the vector the levels above replay.
pub fn generate(
    image: &Image,
    machine: &Lifted,
    abi: &Lifted,
    signatures: &HashMap<String, Signature>,
    recorded: &[Vector],
    seed: u64,
) -> Result<Vec<Vector>> {
    let entered: HashSet<u64> = recorded.iter().map(|vector| vector.address).collect();
    let generator = Generator::new(image, machine)?;
    let mut out = Vec::new();
    for (&address, name) in &machine.functions {
        if entered.contains(&address) {
            continue;
        }
        let (Some(f), Some(signature)) = (abi.module.function(name), signatures.get(name)) else { continue };
        let Some(summary) = summary(f) else { continue };
        if f.params.len() != signature.param_count() || !generator.takes(signature) {
            continue;
        }
        out.extend(generator.vectors(name, signature, &summary, seed ^ address)?);
    }
    Ok(out)
}

/// Where one parameter of an input goes.
#[derive(Debug, Clone, Copy)]
enum Place<'a> {
    Register(&'a str),
    /// The bytes above the stack pointer it lies at, and how many it takes.
    Stack(u64, usize),
}

/// The machine level made ready to run inputs: the layout of the memory each run has.
struct Generator<'a> {
    image: &'a Image,
    machine: &'a Lifted,
    shape: Shape,
    target: Target,
    /// The bytes the functions' addresses and the globals without one take.
    statics: u64,
    /// The first address after the image the runs' own memory starts at.
    low: u64,
}

/// A run of an input, as laid out: its regions and where the stack pointer and the parameters'
/// pages are.
struct Layout {
    regions: Vec<(u64, u64, &'static str)>,
    sp: u64,
    pages: Vec<u64>,
    statics: u64,
    frames: u64,
    state: u64,
}

impl<'a> Generator<'a> {
    fn new(image: &'a Image, machine: &'a Lifted) -> Result<Self> {
        if !machine.module.frozen.is_empty() {
            bail!("the module holds frozen functions: inputs run once the raise has thawed them");
        }
        let module: &Module = &machine.module;
        let statics = {
            let vm = Vm::new(module, Memory::new());
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
        let base = image.image_base();
        let end = base.saturating_add(u64::from(image.size_of_image()).max(image.mapped().len() as u64));
        let low = end.div_ceil(GRAIN) * GRAIN + GRAIN;
        let target = Target { bits: machine.bits, isa: machine.isa };
        Ok(Self { image, machine, shape: Shape::of(machine.isa, machine.bits), target, statics, low })
    }

    /// Whether an input can be made for a function of `signature`: one that returns, takes no
    /// argument block and keeps its stack pointer where the code says.
    fn takes(&self, signature: &Signature) -> bool {
        let above = self.target.first_stack_arg() + signature.stack_bytes(self.target);
        !signature.noreturn && !signature.variadic && !signature.dynamic_stack && u64::try_from(above).is_ok_and(|above| above < ABOVE)
    }

    /// Where each parameter of `signature` goes, in the order of the parameters.
    fn places(&self, signature: &Signature) -> Result<Vec<Place<'_>>> {
        let mut places = Vec::with_capacity(signature.param_count());
        for index in 0..signature.params.len() {
            let slot =
                self.target.param_slot(&signature.params, index).ok_or_else(|| anyhow!("parameter {} has no register", index + 1))?;
            let name = self.shape.name(slot).ok_or_else(|| anyhow!("the state has no register for {slot:?}"))?;
            places.push(Place::Register(name));
        }
        let first = self.target.first_stack_arg();
        for (kind, offset) in signature.stack_params.iter().zip(signature.stack_offsets(self.target)) {
            let at = u64::try_from(first + offset).map_err(|_| anyhow!("a stack parameter below the stack pointer"))?;
            places.push(Place::Stack(at, usize::try_from(kind.stack_width(self.target)).unwrap_or(8)));
        }
        Ok(places)
    }

    /// The regions of a run with `pointers` pages of parameters: the stack, the pages, the
    /// interpreter's globals, its frames and the state, each on a boundary of its own.
    fn layout(&self, pointers: usize) -> Result<Layout> {
        let mut at = self.low;
        let mut take = |len: u64| {
            let start = at;
            at = (at + len).div_ceil(GRAIN) * GRAIN + GRAIN;
            start
        };
        let stack = take(STACK);
        let pages: Vec<u64> = (0..pointers).map(|_| take(PAGE)).collect();
        let statics = take(self.statics);
        let frames = take(FRAMES);
        let state_len = (self.shape.size() as u64).div_ceil(PAGE) * PAGE;
        let state = take(state_len);
        let end = at;
        if self.target.bits < 64 && end >> self.target.bits != 0 {
            bail!("no room below {:#x} for an input's memory", 1u64 << self.target.bits);
        }
        if self.low < GLOBALS + self.statics && GLOBALS < end {
            bail!("an input's memory would lie where a vector has the interpreter's globals");
        }
        let mut regions =
            vec![(stack, STACK, "stack"), (statics, self.statics, "globals"), (frames, FRAMES, "frames"), (state, state_len, "state")];
        regions.extend(pages.iter().map(|page| (*page, PAGE, "argument")));
        Ok(Layout { regions, sp: stack + STACK - ABOVE, pages, statics, frames, state })
    }

    /// The vectors of the function `name`, [`COUNT`] at most, its inputs made from `seed`.
    fn vectors(&self, name: &str, signature: &Signature, summary: &Summary, seed: u64) -> Result<Vec<Vector>> {
        let places = self.places(signature)?;
        let kinds: Vec<Kind> = signature.params.iter().chain(&signature.stack_params).copied().collect();
        let layout = self.layout(summary.pointers.len())?;
        let pages: HashMap<usize, u64> = summary.pointers.iter().copied().zip(layout.pages.iter().copied()).collect();
        let mut rng = Rng(seed);
        let mut out = Vec::with_capacity(COUNT);
        for n in 0..ATTEMPTS {
            if out.len() == COUNT {
                break;
            }
            let boundary = n < BOUNDARY;
            let mut memory: Vec<(u64, Vec<u8>)> = Vec::new();
            let mut values = Vec::with_capacity(kinds.len());
            for (p, kind) in kinds.iter().enumerate() {
                let value = if let Some(&page) = pages.get(&p) {
                    memory.push((page, bytes(&mut rng, boundary, n)));
                    u128::from(page + PAGE / 2)
                } else {
                    argument(&mut rng, *kind, self.target.bits, boundary, n + 5 * p)
                };
                values.push(value);
            }
            if let Some(vector) = self.run(name, &layout, &places, &values, &memory)?
                && vector.left == Left::Return
            {
                out.push(vector);
            }
        }
        Ok(out)
    }

    /// Runs `name` at the machine level, its parameters `values` at `places` and the bytes of
    /// `memory` in place, and gives the vector the recorder kept of the activation.
    fn run(&self, name: &str, layout: &Layout, places: &[Place<'_>], values: &[u128], memory: &[(u64, Vec<u8>)]) -> Result<Option<Vector>> {
        let fault = |stop: Stop| anyhow!("{name}: an input's machine: {stop:?}");
        let base = self.image.image_base();
        let mapped = self.image.mapped();
        let image_len = (u64::from(self.image.size_of_image()).max(mapped.len() as u64)).div_ceil(PAGE) * PAGE;
        let mut regions = Memory::new();
        regions.add_region(Region::owned(base, image_len, "image"));
        for &(start, len, what) in &layout.regions {
            regions.add_region(Region::owned(start, len, what));
        }
        let mut vm = Vm::new(&self.machine.module, regions);
        vm.limit = LIMIT;
        vm.set_static_region(layout.statics, self.statics);
        vm.set_frame_region(layout.frames, FRAMES);
        if !self.machine.standalone {
            vm.memory.write(base, mapped).map_err(fault)?;
        }
        vm.place().map_err(fault)?;
        let width = (self.target.bits / 8) as usize;
        let register = |vm: &mut Vm<'_>, slot: &str, value: u128| -> Result<()> {
            let (offset, size) = self.shape.place(slot).ok_or_else(|| anyhow!("{name}: the state has no register {slot}"))?;
            vm.memory.write(layout.state + offset as u64, &value.to_le_bytes()[..size]).map_err(fault)
        };
        let sp = self.shape.name(self.target.sp()).ok_or_else(|| anyhow!("{name}: the state has no stack pointer"))?;
        register(&mut vm, sp, u128::from(layout.sp))?;
        for (place, value) in places.iter().zip(values) {
            match *place {
                Place::Register(slot) => register(&mut vm, slot, *value)?,
                Place::Stack(above, size) => vm.memory.write(layout.sp + above, &value.to_le_bytes()[..size.min(16)]).map_err(fault)?,
            }
        }
        // The return address the function returns to is no address of the image.
        vm.memory.write(layout.sp, &0u64.to_le_bytes()[..width]).map_err(fault)?;
        for (address, bytes) in memory {
            vm.memory.write(*address, bytes).map_err(fault)?;
        }
        let recorder = Arc::new(Mutex::new(Recorder::of(self.machine)));
        vm.observer = Some(Box::new(Watch(recorder.clone())));
        // How the run ended is the vector's: its `left` says it.
        let _ = vm.call_named(&mut Unanswered, name, vec![Value::Int(u128::from(layout.state))]);
        vm.observer = None;
        let recording = recorder.lock().unwrap_or_else(PoisonError::into_inner).finish();
        Ok(recording.vectors.into_iter().rev().find(|vector| vector.function == name))
    }
}

/// The host of a generated input: a pure function calls nothing, so a call ends the run.
struct Unanswered;

impl Host for Unanswered {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("a generated input has no answer for the call of {name}")))
    }
}

/// The bits of an argument of `kind` for an image of `bits` address bits: the boundary value
/// `n` of its kind when `boundary`, else a random one.
#[allow(clippy::cast_possible_truncation)]
fn argument(rng: &mut Rng, kind: Kind, bits: u32, boundary: bool, n: usize) -> u128 {
    let width = if kind == Kind::Wide { 64 } else { bits };
    let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
    let floating = |rng: &mut Rng| if boundary { FLOATS[n % FLOATS.len()] } else { float(rng) };
    match kind {
        Kind::Int | Kind::Wide => u128::from(if boundary { INTS[n % INTS.len()] } else { integer(rng) } & mask),
        Kind::Double => u128::from(floating(rng).to_bits()),
        Kind::Float => u128::from((floating(rng) as f32).to_bits()),
    }
}

/// A random integer: a small signed one, one next to a boundary, or any.
fn integer(rng: &mut Rng) -> u64 {
    let r = rng.next();
    match r % 4 {
        0 => ((r >> 8) % 256).wrapping_sub(128),
        1 => INTS[((r >> 8) % INTS.len() as u64) as usize].wrapping_add((r >> 16) % 3).wrapping_sub(1),
        _ => rng.next(),
    }
}

/// A random finite floating value, of any sign and over many magnitudes.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn float(rng: &mut Rng) -> f64 {
    let r = rng.next();
    (rng.next() as i64 as f64) / f64::from(1u32 << (r % 32)) / 4_294_967_296.0
}

/// The bytes of a parameter's page: words of the boundary values from the `n`th on when
/// `boundary`, else random bytes.
fn bytes(rng: &mut Rng, boundary: bool, n: usize) -> Vec<u8> {
    (0..PAGE as usize / 8).flat_map(|word| if boundary { INTS[(n + word) % INTS.len()] } else { rng.next() }.to_le_bytes()).collect()
}

/// `SplitMix64`: a small generator whose sequence the seed alone decides.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
