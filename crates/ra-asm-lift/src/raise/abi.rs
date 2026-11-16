//! Signatures: what each lifted function takes and returns, by the convention of its target.
//!
//! On Win64 a function's parameters are the argument registers it reads before writing (`rcx`,
//! `rdx`, `r8`, `r9` for integers, `xmm0` to `xmm3` for floats, by position) and the stack slots
//! past the home space it reads. On x86 they are `ecx` and `edx` when it reads them first
//! (`fastcall`, or `thiscall` with `ecx` as `this`) and the dword slots above the return address
//! it reads; a slot read as 64 bits is one wide parameter. It returns a value when a caller reads
//! `rax` (or `xmm0`, on Win64) after calling it, or when it is a root that writes `rax`. An x86
//! function also says what it pops on return: a `stdcall` or `thiscall` function pops its stack
//! arguments. The inference iterates: a call reads its callee's parameters and writes the
//! caller-saved registers, and the callee's parameters are what is being inferred.
//!
//! A call through a vtable slot is a call to the functions that fill the slot (see
//! [`virtual_calls`]): it reads the union of their parameters and writes what any of them
//! writes, so the caller's frame and parameters are what those functions need rather than what
//! an unknown callee might.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ra_asm_analysis::rtti::{self, Vtable};
use ra_asm_ir::{BinOp, Callee, Cast, Function, Inst, Module, Operand, Type, ValueId};

use super::Target;
use super::assume::{self, Assumption, Step};
use super::liveness::{CallKind, Liveness, NO_SLOTS, RCX, SLOTS, Slot, SlotSet, call_kind, reverse_post_order, slot_pointers};
use super::stack::{CalleeStack, StackInfo, analyze as analyze_stack};

pub use ra_asm_protos::kind::Kind;

/// The IR type of a [`Kind`], which `ra-asm-protos` keeps free of the IR.
pub trait KindType {
    /// The IR type: every integer is 64 bits until the width pass narrows it.
    fn ty(self) -> Type;
}

impl KindType for Kind {
    fn ty(self) -> Type {
        match self {
            Self::Int | Self::Wide => Type::I64,
            Self::Float => Type::Float,
            Self::Double => Type::Double,
        }
    }
}

/// A function's signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The register parameters by position: `rcx`/`xmm0`, `rdx`/`xmm1`, `r8`/`xmm2`, `r9`/`xmm3`
    /// on Win64; `ecx`, `edx` on x86.
    pub params: Vec<Kind>,
    /// The stack parameters after them, from the first stack argument up.
    pub stack_params: Vec<Kind>,
    pub returns: Option<Kind>,
    /// Whether the function never returns.
    pub noreturn: bool,
    /// The bytes the frame needs below the entry stack pointer.
    pub frame_below: u64,
    /// The function moves the stack pointer by a run-time amount.
    pub dynamic_stack: bool,
    /// The bytes the function pops on return beyond the return address (x86 `stdcall`, `thiscall`).
    pub pops: u64,
    /// The caller-saved registers the function leaves as they were: neither it nor anything it
    /// calls writes them. MSVC's `__security_check_cookie` keeps `eax`, and a function sets its
    /// result before calling it; the convention alone would have the call clobber it.
    pub preserves: SlotSet,
    /// An x86 function that reads its stack arguments through their address (a `va_list`): in
    /// place of stack parameters it takes the address of its caller's argument block, as the
    /// trailing parameter `varargs`, and reads every stack argument through it.
    pub variadic: bool,
}

impl Default for Signature {
    fn default() -> Self {
        Self {
            params: Vec::new(),
            stack_params: Vec::new(),
            returns: None,
            noreturn: false,
            frame_below: 0,
            dynamic_stack: false,
            pops: 0,
            preserves: NO_SLOTS,
            variadic: false,
        }
    }
}

impl Signature {
    pub fn param_count(&self) -> usize {
        self.params.len() + self.stack_params.len()
    }

    /// The register slots the function reads as parameters.
    pub fn param_slots(&self, target: Target) -> SlotSet {
        let mut set = NO_SLOTS;
        for p in 0..self.params.len() {
            if let Some(slot) = target.param_slot(&self.params, p) {
                set[slot.index()] = true;
            }
        }
        set
    }

    /// The byte offset of each stack parameter from the first stack argument.
    pub fn stack_offsets(&self, target: Target) -> Vec<i64> {
        let mut offsets = Vec::with_capacity(self.stack_params.len());
        let mut at = 0;
        for kind in &self.stack_params {
            offsets.push(at);
            at += kind.stack_width(target.ptr());
        }
        offsets
    }

    /// The bytes the stack parameters take.
    pub fn stack_bytes(&self, target: Target) -> i64 {
        self.stack_params.iter().map(|k| k.stack_width(target.ptr())).sum()
    }
}

/// The registers a call clobbers by the convention: everything the callee need not keep.
pub fn caller_saved(target: Target) -> SlotSet {
    let mut set = NO_SLOTS;
    for slot in target.slots() {
        if !target.callee_saved(slot) {
            set[slot.index()] = true;
        }
    }
    set
}

/// The registers an unknown callee may read. On Win64 every argument position, as integers and as
/// floats. On x86 only `ecx`: the `this` of a `thiscall` method, which a call through a vtable
/// reaches; `edx` and the vector registers carry arguments only under clang's convention for a
/// local function, which no pointer names. On ARM64 `x0` to `x7`, `v0` to `v7`, and `x8`, the
/// address a callee that returns an aggregate writes it to. On MIPS the `a` registers, the float
/// parameter registers of the convention, and the stack pointer, above which the universal call
/// reads its four stack words: a function that keeps nothing else on the stack holds it live for
/// that call alone.
pub fn all_argument_slots(target: Target) -> SlotSet {
    let mut set = NO_SLOTS;
    if target.register_arguments() {
        for slot in target.int_param_slots() {
            set[slot.index()] = true;
        }
        for n in 0..target.float_params() {
            set[target.float_param_slot(n).index()] = true;
        }
        if target.is_arm64() {
            set[super::X8.index()] = true;
        }
        if target.is_mips() {
            set[target.sp().index()] = true;
        }
    } else {
        set[RCX.index()] = true;
    }
    set
}

/// The most argument registers of one class a target has: ARM64's eight.
const ARGUMENT_REGISTERS: usize = 8;

/// The evidence one function gives about itself and its callees.
struct Facts {
    name: String,
    int_positions: [bool; ARGUMENT_REGISTERS],
    float_positions: [Option<Kind>; ARGUMENT_REGISTERS],
    stack: StackInfo,
    /// Whether the function leaves a value in `rax` at a return: one it writes itself, or the one
    /// a call in the returning block left there.
    writes_rax: bool,
    /// The float kind the function writes to `xmm0`, when it does.
    writes_xmm0: Option<Kind>,
    /// The slots the function writes, itself or through what it calls, as far as the signatures
    /// of the callees say so far.
    writes: SlotSet,
    /// Callees whose `rax` this function reads after the call.
    uses_rax_of: Vec<String>,
    /// Callees whose `rdx` this function reads after the call: on x86, a 64-bit result in `edx:eax`.
    uses_rdx_of: Vec<String>,
    /// Callees whose `xmm0` this function reads after the call.
    uses_xmm0_of: Vec<String>,
    /// Each direct call to a lifted function, with the private registers past the convention's
    /// ([`Target::private_int_param_slots`]) the block sets before the call, by position from the
    /// first past the convention's: what the callee may take in them.
    sets_before_calls: Vec<(String, u8)>,
    noreturn: bool,
}

/// The functions a call through a vtable slot can reach, by address and IR name.
/// Shared: a slot of a large program's vtables is filled by hundreds of functions (every class's
/// destructor fills slot 0), and every call through the slot reads the same list.
pub type Candidates = std::sync::Arc<[(u64, String)]>;

/// The most functions a call through a vtable slot is resolved to. Past it the call stays an
/// indirect call through the runtime's function table: the raise lowers a resolved call to a test
/// and an arm per function, and a call through slot 0 of a large program's every vtable (the
/// destructors) became thousands of blocks in each caller, minutes of raising and gigabytes of IR.
pub const MAX_DISPATCH: usize = 16;

/// The vtables, indexed for the calls through their slots: which addresses are vtables, and the
/// functions of each slot, made once and shared by every call that reads them, across the
/// functions and the threads.
pub struct VtableIndex<'a> {
    vtables: &'a [Vtable],
    addresses: std::collections::HashSet<u64>,
    slots: std::sync::Mutex<HashMap<(Option<u64>, usize), Candidates>>,
}

impl<'a> VtableIndex<'a> {
    pub fn new(vtables: &'a [Vtable]) -> Self {
        Self { vtables, addresses: vtables.iter().map(|v| v.address).collect(), slots: Default::default() }
    }

    fn is_empty(&self) -> bool {
        self.vtables.is_empty()
    }

    fn is_vtable(&self, address: u64) -> bool {
        self.addresses.contains(&address)
    }

    /// The functions that fill `slot`: of the vtable at `vtable`, or of every vtable without one.
    fn candidates(&self, vtable: Option<u64>, slot: usize) -> Candidates {
        let mut slots = self.slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        slots
            .entry((vtable, slot))
            .or_insert_with(|| {
                let functions = match vtable {
                    Some(address) => {
                        let own: Vec<Vtable> = self.vtables.iter().filter(|v| v.address == address).cloned().collect();
                        rtti::slot_functions(&own, slot)
                    }
                    None => rtti::slot_functions(self.vtables, slot),
                };
                functions.iter().map(|&va| (va, crate::function_name(va))).collect()
            })
            .clone()
    }
}

/// The calls of `f` that go through a vtable slot, by the value the call's target is, each with
/// the functions that fill the slot. Such a target is a load at `vptr + slot * ptr` where `vptr`
/// is itself a load, of the object's first word. The receiver's class is not known, so the
/// candidates are the slot's functions across every vtable that fills it; when the vtable is a
/// constant, or was stored to the object earlier in the block (a constructor), that vtable's
/// alone. A register's load reads the last store to it within the block, so the two loads are
/// found through the state. A slot some function of which is not lifted (`lifted` says) stays
/// an indirect call.
pub fn virtual_calls(
    f: &Function,
    vtables: &VtableIndex<'_>,
    target: Target,
    lifted: &dyn Fn(&str) -> bool,
) -> HashMap<ValueId, Candidates> {
    let mut out = HashMap::new();
    let Some(&state) = f.params.first() else { return out };
    if vtables.is_empty() {
        return out;
    }
    let pointers = slot_pointers(f, state);
    let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| i.result.map(|r| (r, &i.inst))).collect();
    let ptr = target.ptr();
    for b in &f.blocks {
        // The registers' values so far in the block, by the register and by the load that read it.
        let mut registers: HashMap<Slot, Operand> = HashMap::new();
        let mut forwarded: HashMap<ValueId, Operand> = HashMap::new();
        // The vtable addresses stored to objects so far in the block, by the object's address.
        let mut stored: Vec<((Operand, i128), u64)> = Vec::new();
        // A value through the register loads and the width changes of 32-bit code.
        let strip = |o: &Operand, forwarded: &HashMap<ValueId, Operand>| -> Operand {
            let mut o = o.clone();
            for _ in 0..64 {
                let Operand::Local(v) = &o else { break };
                if let Some(r) = forwarded.get(v) {
                    o = r.clone();
                    continue;
                }
                match defs.get(v) {
                    Some(Inst::Cast { cast: Cast::ZExt | Cast::Trunc, value, .. }) => o = value.clone(),
                    _ => break,
                }
            }
            o
        };
        // An address as a base and an offset: `base + k`, or the value itself at 0.
        let base_offset = |o: &Operand, forwarded: &HashMap<ValueId, Operand>| -> (Operand, i128) {
            let o = strip(o, forwarded);
            if let Operand::Local(v) = &o
                && let Some(Inst::Bin { op: BinOp::Add, lhs, rhs: Operand::Int(k), .. }) = defs.get(v)
            {
                return (strip(lhs, forwarded), *k);
            }
            (o, 0)
        };
        // The integer address a pointer was made from.
        let pointer_address = |p: &ValueId| -> Option<Operand> {
            match defs.get(p) {
                Some(Inst::Cast { cast: Cast::IntToPtr, value, .. }) => Some(value.clone()),
                _ => None,
            }
        };
        for i in &b.instrs {
            match &i.inst {
                Inst::Store { ptr: Operand::Local(p), value, .. } if pointers.contains_key(p) => {
                    registers.insert(pointers[p], value.clone());
                }
                Inst::Load { ptr: Operand::Local(p), .. } if pointers.contains_key(p) => {
                    let slot = pointers[p];
                    let result = i.result.unwrap();
                    match registers.get(&slot) {
                        Some(v) => {
                            forwarded.insert(result, v.clone());
                        }
                        None => {
                            registers.insert(slot, Operand::Local(result));
                        }
                    }
                }
                Inst::Store { ptr: to, value, .. } => {
                    // A vtable address stored to an object is remembered for the object; any
                    // other store forgets what it may overwrite.
                    let at = match to {
                        Operand::Local(p) => pointer_address(p).map(|a| base_offset(&a, &forwarded)),
                        _ => None,
                    };
                    let vtable = match strip(value, &forwarded) {
                        Operand::Int(c) => u64::try_from(c).ok().filter(|c| vtables.is_vtable(*c)),
                        _ => None,
                    };
                    match (at, vtable) {
                        (Some(at), Some(vtable)) => {
                            stored.retain(|(a, _)| *a != at);
                            stored.push((at, vtable));
                        }
                        (Some((base, k)), None) => {
                            stored.retain(|((other, j), _)| *other == base && (k - j).abs() >= i128::from(ptr));
                        }
                        (None, _) => stored.clear(),
                    }
                }
                Inst::Call { callee: Callee::Direct(name), args, .. } if name == "ra_call" => {
                    let Some((_, Operand::Local(key))) = args.get(1) else { continue };
                    // The slot's load, at `vptr + k`.
                    let target_value = strip(&Operand::Local(*key), &forwarded);
                    let Operand::Local(t) = &target_value else { continue };
                    let Some(Inst::Load { ptr: Operand::Local(p), .. }) = defs.get(t) else { continue };
                    if pointers.contains_key(p) {
                        continue;
                    }
                    let Some(slot_address) = pointer_address(p) else { continue };
                    let (vptr, k) = base_offset(&slot_address, &forwarded);
                    if k < 0 || k % i128::from(ptr) != 0 {
                        continue;
                    }
                    let slot = (k / i128::from(ptr)) as usize;
                    // The vtable pointer: a constant, or the object's first word.
                    let vtable = match &vptr {
                        Operand::Int(c) => match u64::try_from(*c) {
                            Ok(c) if vtables.is_vtable(c) => Some(c),
                            _ => continue,
                        },
                        Operand::Local(v) => {
                            let Some(Inst::Load { ptr: Operand::Local(q), .. }) = defs.get(v) else { continue };
                            if pointers.contains_key(q) {
                                continue;
                            }
                            let Some(object) = pointer_address(q) else { continue };
                            let object = base_offset(&object, &forwarded);
                            stored.iter().find(|(a, _)| *a == object).map(|(_, vtable)| *vtable)
                        }
                        _ => continue,
                    };
                    let candidates = vtables.candidates(vtable, slot);
                    if !candidates.is_empty() && candidates.len() <= MAX_DISPATCH && candidates.iter().all(|(_, name)| lifted(name)) {
                        out.insert(*key, candidates);
                    }
                    registers.clear();
                    stored.clear();
                }
                inst if call_kind(inst) == Some(CallKind::State) => {
                    registers.clear();
                    stored.clear();
                }
                Inst::AtomicRmw { .. } | Inst::CmpXchg { .. } => stored.clear(),
                _ => {}
            }
        }
    }
    out
}

/// The calls of `f` a traced run resolved (`traced`, by the address of the call, from
/// [`ra_asm_analysis::facts`]), by the value the call's target is, each with the lifted functions
/// the run saw it reach. The run proves nothing about the targets it did not take, so the caller
/// keeps the indirect call for them (see [`super::promote`]); a target that is not lifted leaves
/// the call indirect altogether.
pub fn traced_calls(f: &Function, traced: &BTreeMap<u64, Vec<u64>>, lifted: &dyn Fn(&str) -> bool) -> HashMap<ValueId, Candidates> {
    let mut out = HashMap::new();
    if traced.is_empty() {
        return out;
    }
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Call { callee: Callee::Direct(name), args, .. } = &i.inst else { continue };
        if name != "ra_call" {
            continue;
        }
        let (Some(targets), Some((_, Operand::Local(key)))) = (i.address.and_then(|a| traced.get(&a)), args.get(1)) else { continue };
        let candidates: Candidates = targets.iter().map(|&va| (va, crate::function_name(va))).collect::<Vec<_>>().into();
        if !candidates.is_empty() && candidates.iter().all(|(_, name)| lifted(name)) {
            out.insert(*key, candidates);
        }
    }
    out
}

/// The candidates of `inst` when it is a call through a vtable slot that `virtual_calls` resolved.
pub fn virtual_candidates<'a>(inst: &Inst, virtual_calls: &'a HashMap<ValueId, Candidates>) -> Option<&'a Candidates> {
    if let Inst::Call { callee: Callee::Direct(name), args, .. } = inst
        && name == "ra_call"
        && let Some((_, Operand::Local(v))) = args.get(1)
    {
        return virtual_calls.get(v);
    }
    None
}

/// Infers the signature of every `F_` function of `module`. A root (the entry, an export, a
/// function a pointer names) returns a value when it writes `rax`, or returns from a block where
/// a callee wrote it (a tail call hands back the callee's result), since its callers are not seen.
/// A call through a slot of one of `vtables` is a call to the functions that fill the slot.
pub fn infer(module: &Module, roots: &BTreeSet<String>, target: Target, vtables: &[Vtable]) -> HashMap<String, Signature> {
    infer_with(module, roots, target, vtables, &BTreeMap::new())
}

/// [`infer`] with the calls a traced run resolved (`traced`, see [`traced_calls`]): such a call
/// is a call to the functions the run saw, as a call through a vtable slot is to the slot's.
pub fn infer_with(
    module: &Module,
    roots: &BTreeSet<String>,
    target: Target,
    vtables: &[Vtable],
    traced: &BTreeMap<u64, Vec<u64>>,
) -> HashMap<String, Signature> {
    infer_from(&Source::of(module), roots, target, vtables, traced)
}

/// [`infer_with`] over functions already simplified in place (the `F_` ones of `functions`, and
/// `frozen`): the raise simplifies them first, so the inference reads them without a copy.
pub(crate) fn infer_simplified(
    functions: &[Function],
    frozen: &[ra_asm_ir::freeze::Frozen],
    roots: &BTreeSet<String>,
    target: Target,
    vtables: &[Vtable],
    traced: &BTreeMap<u64, Vec<u64>>,
) -> HashMap<String, Signature> {
    let owned: Vec<&Function> = functions.iter().filter(|f| f.name.starts_with("F_")).collect();
    let source = if frozen.is_empty() {
        Source::Borrowed(owned)
    } else {
        // A lift that freezes freezes every function it lifts: the module's own are its wrappers.
        let mut all: Vec<ra_asm_ir::freeze::Frozen> = owned.into_iter().map(ra_asm_ir::freeze::freeze).collect();
        if all.is_empty() {
            return infer_from(&Source::FrozenRef(frozen), roots, target, vtables, traced);
        }
        all.extend_from_slice(frozen);
        Source::Frozen(all)
    };
    infer_from(&source, roots, target, vtables, traced)
}

/// The lifted functions the inference reads, each simplified: the stack analysis reads constants,
/// and the machine IR materializes them. Owned, borrowed, or frozen and thawed one at a time when
/// the lift froze them, so the machine IR of a large program is never all in memory at once.
pub(crate) enum Source<'a> {
    Owned(Vec<Function>),
    Borrowed(Vec<&'a Function>),
    Frozen(Vec<ra_asm_ir::freeze::Frozen>),
    FrozenRef(&'a [ra_asm_ir::freeze::Frozen]),
}

impl Source<'_> {
    /// The `F_` functions of `module`, its frozen ones included, simplified copies.
    pub(crate) fn of(module: &Module) -> Source<'static> {
        let simplified = |f: &Function| {
            let mut f = f.clone();
            super::simplify::run(&mut f);
            f
        };
        let owned = module.functions.iter().filter(|f| f.name.starts_with("F_"));
        if module.frozen.is_empty() {
            return Source::Owned(owned.map(simplified).collect());
        }
        let mut frozen: Vec<_> = owned.map(|f| ra_asm_ir::freeze::freeze(&simplified(f))).collect();
        frozen.extend(par_map(&module.frozen, |_, frozen| {
            let mut f = ra_asm_ir::freeze::thaw(frozen);
            super::simplify::run(&mut f);
            ra_asm_ir::freeze::freeze(&f)
        }));
        Source::Frozen(frozen)
    }

    fn names(&self) -> Vec<String> {
        match self {
            Self::Owned(functions) => functions.iter().map(|f| f.name.clone()).collect(),
            Self::Borrowed(functions) => functions.iter().map(|f| f.name.clone()).collect(),
            Self::Frozen(frozen) => frozen.iter().map(|f| f.name.clone()).collect(),
            Self::FrozenRef(frozen) => frozen.iter().map(|f| f.name.clone()).collect(),
        }
    }

    /// `each` of the functions `select` marks, in order, on the host's threads; the others stay
    /// frozen and give `None`.
    fn map_where<T: Send>(&self, select: &[bool], each: impl Fn(usize, &Function) -> T + Sync) -> Vec<Option<T>> {
        let thawed = |i: usize, frozen: &ra_asm_ir::freeze::Frozen| select[i].then(|| each(i, &ra_asm_ir::freeze::thaw(frozen)));
        match self {
            Self::Owned(functions) => par_map(functions, |i, f| select[i].then(|| each(i, f))),
            Self::Borrowed(functions) => par_map(functions, |i, f| select[i].then(|| each(i, f))),
            Self::Frozen(frozen) => par_map(frozen, thawed),
            Self::FrozenRef(frozen) => par_map(frozen, thawed),
        }
    }

    /// `each` of every function, in order, on the host's threads.
    fn map<T: Send>(&self, each: impl Fn(usize, &Function) -> T + Sync) -> Vec<T> {
        let thawed = |i: usize, frozen: &ra_asm_ir::freeze::Frozen| each(i, &ra_asm_ir::freeze::thaw(frozen));
        match self {
            Self::Owned(functions) => par_map(functions, |i, f| each(i, f)),
            Self::Borrowed(functions) => par_map(functions, |i, f| each(i, f)),
            Self::Frozen(frozen) => par_map(frozen, thawed),
            Self::FrozenRef(frozen) => par_map(frozen, thawed),
        }
    }
}

/// `each` of every item of `items` with its index, in order, on the host's threads: a run of
/// consecutive items a thread.
pub(crate) fn par_map<I: Sync, T: Send>(items: &[I], each: impl Fn(usize, &I) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(items.len().max(1));
    if threads <= 1 {
        return items.iter().enumerate().map(|(i, x)| each(i, x)).collect();
    }
    // The threads take small batches from a shared counter: a program's large functions sit
    // together, and a thread given a fixed run of them would finish long after the others.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (each, next) = (&each, &next);
    let mut done: Vec<(usize, T)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let start = next.fetch_add(BATCH, std::sync::atomic::Ordering::Relaxed);
                        if start >= items.len() {
                            return out;
                        }
                        for (i, x) in items.iter().enumerate().skip(start).take(BATCH) {
                            out.push((i, each(i, x)));
                        }
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, t)| t).collect()
}

/// The items a thread of [`par_map`] or [`par_each_mut`] takes at a time.
const BATCH: usize = 16;

/// What the ABI step infers and what it assumed on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inferred {
    pub signatures: HashMap<String, Signature>,
    /// What the signatures and the calls rewritten by them rely on (see [`assumptions`]), those
    /// the lifted callees prove discharged (see [`assume::discharge`]).
    pub assumptions: Vec<Assumption>,
}

/// [`infer_with`], with the assumptions the step made beside the signatures.
pub fn infer_assuming(
    module: &Module,
    roots: &BTreeSet<String>,
    target: Target,
    vtables: &[Vtable],
    traced: &BTreeMap<u64, Vec<u64>>,
) -> Inferred {
    let source = Source::of(module);
    let signatures = infer_from(&source, roots, target, vtables, traced);
    let Source::Owned(functions) = source else { unreachable!("a module the lift did not freeze is read as it is") };
    let mut assumptions = Vec::new();
    for f in &functions {
        assumed(f, &signatures, target, &mut assumptions);
    }
    assume::sort(&mut assumptions);
    assume::discharge(&functions, &signatures, target, &mut assumptions);
    Inferred { signatures, assumptions }
}

/// `each` of every item of `items` with its index, in place, on the host's threads.
pub(crate) fn par_each_mut<I: Send>(items: &mut [I], each: impl Fn(usize, &mut I) + Sync) {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(items.len().max(1));
    if threads <= 1 {
        items.iter_mut().enumerate().for_each(|(i, x)| each(i, x));
        return;
    }
    // Batches from a shared queue, as `par_map` takes them: each a disjoint run of the items.
    let batches: std::sync::Mutex<std::iter::Enumerate<std::slice::ChunksMut<'_, I>>> =
        std::sync::Mutex::new(items.chunks_mut(BATCH).enumerate());
    let (each, batches) = (&each, &batches);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(move || {
                loop {
                    let Some((b, batch)) = batches.lock().unwrap_or_else(std::sync::PoisonError::into_inner).next() else { return };
                    batch.iter_mut().enumerate().for_each(|(i, x)| each(b * BATCH + i, x));
                }
            });
        }
    });
}

/// The signatures of the functions of `source`, to the fixpoint of their calls.
fn infer_from(
    source: &Source,
    roots: &BTreeSet<String>,
    target: Target,
    vtables: &[Vtable],
    traced: &BTreeMap<u64, Vec<u64>>,
) -> HashMap<String, Signature> {
    let names = source.names();
    let mut signatures: HashMap<String, Signature> = names.iter().map(|n| (n.clone(), Signature::default())).collect();
    let lifted_names: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    let lifted = |name: &str| lifted_names.contains(name);
    let index = VtableIndex::new(vtables);
    // Each function's calls through vtable slots, and what its facts depend on: the signatures of
    // its direct callees and of the members of the candidate lists it calls through.
    let mut virtuals = Vec::with_capacity(names.len());
    let mut slots = Vec::with_capacity(names.len());
    let mut lists_of: Vec<Vec<Candidates>> = Vec::with_capacity(names.len());
    let mut callers: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (calls, slot_calls, callees)) in source
        .map(|_, f| {
            let mut calls = virtual_calls(f, &index, target, &lifted);
            calls.extend(traced_calls(f, traced, &lifted));
            let slot_calls = virtual_calls(f, &index, target, &|_: &str| true);
            let mut callees: Vec<String> = f
                .blocks
                .iter()
                .flat_map(|b| &b.instrs)
                .filter_map(|i| match &i.inst {
                    Inst::Call { callee: Callee::Direct(name), .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            callees.push(f.name.clone());
            callees.sort_unstable();
            callees.dedup();
            (calls, slot_calls, callees)
        })
        .into_iter()
        .enumerate()
    {
        for callee in callees {
            callers.entry(callee).or_default().push(i);
        }
        let mut lists: Vec<Candidates> = calls.values().chain(slot_calls.values()).cloned().collect();
        lists.sort_unstable_by_key(|l| std::sync::Arc::as_ptr(l).cast::<()>() as usize);
        lists.dedup_by_key(|l| std::sync::Arc::as_ptr(l).cast::<()>() as usize);
        lists_of.push(lists);
        virtuals.push(calls);
        slots.push(slot_calls);
    }
    let ints = target.int_param_slots().len();
    let private_ints = target.private_int_param_slots().len();
    let floats = target.float_params();
    // A round gathers again only the functions whose dependencies' signatures the last round
    // changed: the others' facts are the same, and their frozen bytes stay frozen.
    let mut facts: Vec<Option<Facts>> = (0..names.len()).map(|_| None).collect();
    let mut dirty = vec![true; names.len()];
    for _ in 0..8 {
        let summaries = SummaryCache::new(&signatures, target);
        let fresh = source.map_where(&dirty, |i, f| gather(f, &virtuals[i], &slots[i], &signatures, target, &summaries));
        for (slot, fresh) in facts.iter_mut().zip(fresh) {
            if fresh.is_some() {
                *slot = fresh;
            }
        }
        let facts: Vec<&Facts> =
            facts.iter().map(|f| f.as_ref().expect("every function's facts are gathered in the first round")).collect();
        let xmm0_kinds: HashMap<&str, Option<Kind>> = facts.iter().map(|f| (f.name.as_str(), f.writes_xmm0)).collect();
        // Through the callees as well: `call _allrem; ret` hands back the 64 bits its callee left.
        let (low, high) = (target.int_return().index(), target.high_return().index());
        let writes_rdx: HashMap<&str, bool> = facts.iter().map(|f| (f.name.as_str(), f.writes[high])).collect();
        let writes_rax: HashMap<&str, bool> = facts.iter().map(|f| (f.name.as_str(), f.writes[low])).collect();
        // What the function itself leaves in rax, not what its callees clobber there.
        let stores_rax: HashMap<&str, bool> = facts.iter().map(|f| (f.name.as_str(), f.writes_rax)).collect();
        // A float result wins when a caller reads xmm0 after the call: the callee wrote it as one.
        // A float an x86 function returns on the x87 stack travels in the lift's globals instead.
        let mut returns: BTreeMap<String, Option<Kind>> = facts.iter().map(|f| (f.name.clone(), None)).collect();
        for fact in &facts {
            // A caller that reads rax after a call to a callee that leaves rax alone reads its
            // own value through the call.
            for callee in &fact.uses_rax_of {
                if let Some(r) = returns.get_mut(callee)
                    && r.is_none()
                    && writes_rax.get(callee.as_str()).copied().unwrap_or(true)
                {
                    *r = Some(Kind::Int);
                }
            }
            // A caller that reads xmm0 after the call takes a float result there: the Win64
            // convention, and clang's own convention for a local x86 function.
            // A callee that writes no xmm0 and whose rax the caller reads returns in rax: the
            // xmm0 the caller reads is its own, through the call, as an indirect call after it
            // passes every vector register (computed's `resolve`, whose address result a call
            // then takes, came out a double and its call went through 0).
            // Nor does a candidate of a virtual call that writes no xmm0 and stores rax itself
            // return a float because another caller of the slot reads xmm0: stl-clang64's
            // `Rect::area` (`mov eax, ..; imul eax, ..; ret`) came out a double, the port read its
            // result from xmm0 and printed `weak locked = 0` for 121.
            for callee in &fact.uses_xmm0_of {
                let wrote = xmm0_kinds.get(callee.as_str()).copied().flatten();
                if wrote.is_none() && (fact.uses_rax_of.contains(callee) || stores_rax.get(callee.as_str()).copied().unwrap_or(false)) {
                    continue;
                }
                if let Some(r) = returns.get_mut(callee) {
                    *r = Some(wrote.unwrap_or(Kind::Double));
                }
            }
            if !target.is_64() {
                // A caller that reads edx after the call takes a 64-bit result in edx:eax, from
                // a callee that writes edx.
                for callee in &fact.uses_rdx_of {
                    if let Some(r) = returns.get_mut(callee)
                        && fact.uses_rax_of.contains(callee)
                        && writes_rdx.get(callee.as_str()).copied().unwrap_or(false)
                    {
                        *r = Some(Kind::Wide);
                    }
                }
            }
            if roots.contains(&fact.name) {
                if let (true, Some(kind)) = (target.register_arguments(), fact.writes_xmm0) {
                    returns.insert(fact.name.clone(), Some(kind));
                } else if fact.writes_rax {
                    returns.insert(fact.name.clone(), Some(Kind::Int));
                }
            }
        }
        // The private registers every direct call to a function sets before it.
        let mut set_by_callers: HashMap<&str, u8> = HashMap::new();
        for fact in &facts {
            for (callee, mask) in &fact.sets_before_calls {
                *set_by_callers.entry(callee.as_str()).or_insert(u8::MAX) &= mask;
            }
        }
        let mut next: HashMap<String, Signature> = HashMap::new();
        for fact in facts {
            let params: Vec<Kind> = if target.shares_positions() {
                // A position is one register of either class.
                let positions = (0..ints.max(floats))
                    .rev()
                    .find(|&p| (p < ints && fact.int_positions[p]) || (p < floats && fact.float_positions[p].is_some()))
                    .map_or(0, |p| p + 1);
                (0..positions)
                    .map(|p| match (p < ints && fact.int_positions[p], p < floats && fact.float_positions[p].is_some()) {
                        (false, true) => fact.float_positions[p].unwrap(),
                        _ => Kind::Int,
                    })
                    .collect()
            } else {
                // The integer registers, then the float registers, each class up to its last read:
                // x86 and ARM64 count each class on its own.
                // A function the lift does not see every caller of takes the convention's: its
                // callers outside the lift pass nothing in the private registers. One it does
                // takes a private register its code reads when every call to it sets it first:
                // a `push eax` that only makes room reads `eax` too, and no caller sets it for it.
                let confirmed = if roots.contains(&fact.name) { 0 } else { set_by_callers.get(fact.name.as_str()).copied().unwrap_or(0) };
                let read = |p: usize| fact.int_positions[p] && (p < ints || confirmed & (1 << (p - ints)) != 0);
                let int_count = (0..private_ints).rev().find(|&p| read(p)).map_or(0, |p| p + 1);
                let float_count = (0..floats).rev().find(|&p| fact.float_positions[p].is_some()).map_or(0, |p| p + 1);
                (0..int_count).map(|_| Kind::Int).chain((0..float_count).map(|p| fact.float_positions[p].unwrap_or(Kind::Double))).collect()
            };
            // The stack parameters: one per slot from the first stack argument to the end of the
            // widest read; on x86 a slot read as 64 bits is one wide parameter over two slots,
            // and a 16-byte copy of the arguments (clang moves two 64-bit ones at once) covers
            // one slot per dword.
            let first = target.first_stack_arg();
            let slot = Kind::Int.stack_width(target.ptr());
            let mut stack_params = Vec::new();
            if let Some(end) = fact.stack.argument_reads.iter().map(|(&at, &bits)| at + i64::from(bits / 8).max(slot)).max() {
                let mut at = first;
                while at < end {
                    let bits = fact.stack.argument_reads.get(&at).copied().unwrap_or(0);
                    let kind = if !target.is_64() && bits == 64 { Kind::Wide } else { Kind::Int };
                    at += kind.stack_width(target.ptr());
                    stack_params.push(kind);
                }
            }
            let returns = if fact.noreturn { None } else { returns.get(&fact.name).copied().flatten() };
            let frame_below = (-fact.stack.min_offset).max(0) as u64;
            let mut preserves = caller_saved(target);
            for (kept, written) in preserves.iter_mut().zip(fact.writes) {
                *kept &= !written;
            }
            // A function that hands the address of its arguments on reads them through it; one
            // that also writes into their slots reuses a dead parameter's slot for a local.
            let variadic = !target.is_64() && fact.stack.argument_address_escapes && !fact.stack.argument_area_written;
            next.insert(
                fact.name.clone(),
                Signature {
                    params,
                    stack_params: if variadic { Vec::new() } else { stack_params },
                    returns,
                    noreturn: fact.noreturn,
                    frame_below: (frame_below + 15) & !15,
                    dynamic_stack: fact.stack.dynamic,
                    pops: if target.is_64() { 0 } else { fact.stack.pops.max(0) as u64 },
                    preserves,
                    variadic,
                },
            );
        }
        let settled = next == signatures;
        // The next round's work: the functions that call, directly or through a list, one whose
        // signature changed.
        let changed: std::collections::HashSet<&str> =
            next.iter().filter(|(name, s)| signatures.get(*name) != Some(*s)).map(|(name, _)| name.as_str()).collect();
        let key = |l: &Candidates| std::sync::Arc::as_ptr(l).cast::<()>() as usize;
        let mut dirty_lists: HashMap<usize, bool> = HashMap::new();
        dirty.iter_mut().for_each(|d| *d = false);
        for name in &changed {
            for &caller in callers.get(*name).into_iter().flatten() {
                dirty[caller] = true;
            }
        }
        for (d, lists) in dirty.iter_mut().zip(&lists_of) {
            if !*d {
                *d = lists
                    .iter()
                    .any(|l| *dirty_lists.entry(key(l)).or_insert_with(|| l.iter().any(|(_, member)| changed.contains(member.as_str()))));
            }
        }
        signatures = next;
        if settled {
            break;
        }
    }
    signatures
}

/// What the ABI step assumes of the `F_` functions of `module` under `signatures`, in a stable
/// order. At every call of the machine (to a lifted function, or through `ra_call` to whatever
/// the target is) that returns: the callee keeps the callee-saved registers, since the call
/// writes only the others in [`call_model`]; it leaves the return address the call pushed alone,
/// where the target pushes one; and it returns with the stack pointer restored, past what its
/// signature pops. An import is called as a function of the host, which cannot reach the lifted
/// registers or the lifted stack, so its call assumes nothing; a function without calls assumes
/// nothing either.
pub fn assumptions(module: &Module, signatures: &HashMap<String, Signature>, target: Target) -> Vec<Assumption> {
    let mut out = Vec::new();
    for f in module.functions.iter().filter(|f| f.name.starts_with("F_")) {
        assumed(f, signatures, target, &mut out);
    }
    assume::sort(&mut out);
    out
}

/// The assumptions of one function, in the order of its calls.
fn assumed(f: &Function, signatures: &HashMap<String, Signature>, target: Target, out: &mut Vec<Assumption>) {
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Call { callee: Callee::Direct(name), .. } = &i.inst else { continue };
        if !name.starts_with("F_") && name != "ra_call" {
            continue;
        }
        // Nothing is read after a call that never returns.
        if signatures.get(name).is_some_and(|s| s.noreturn) {
            continue;
        }
        let returns_through_stack = target.pushed_return() != 0;
        for kind in [assume::Kind::CalleeSaved, assume::Kind::ReturnAddress, assume::Kind::StackPointer] {
            if kind == assume::Kind::ReturnAddress && !returns_through_stack {
                continue;
            }
            out.push(Assumption { function: f.name.clone(), step: Step::Abi, kind, site: i.address, discharged: None });
        }
    }
}

/// [`call_model`] for the promotion pass.
pub fn call_model_public(
    inst: &Inst,
    virtual_calls: &HashMap<ValueId, Candidates>,
    signatures: &HashMap<String, Signature>,
    target: Target,
    summaries: Option<&SummaryCache<'_>>,
) -> (SlotSet, SlotSet) {
    call_model(inst, virtual_calls, signatures, target, summaries)
}

/// What a state call reads and writes under the current signatures. An import reads as many
/// argument registers as its prototype says (none on x86, where they are on the stack); a
/// variadic C function reads the integer argument registers, where the Win64 convention puts a
/// float argument of its too; an unknown import or an indirect call reads every argument
/// register. A lifted callee writes the caller-saved registers it does not preserve; a call
/// through a vtable slot reads what any of the slot's functions reads and writes what any
/// writes; anything else writes them all.
/// What a call through a vtable slot does to the registers, from its candidates: what any of them
/// reads as a parameter, and what all of them preserve. A candidate without a signature reads
/// nothing and preserves nothing.
fn summarize(candidates: &Candidates, signatures: &HashMap<String, Signature>, target: Target) -> (SlotSet, SlotSet) {
    let mut reads = NO_SLOTS;
    let mut kept_by_all = caller_saved(target);
    for (_, name) in candidates.iter() {
        match signatures.get(name) {
            Some(s) => {
                for (r, read) in reads.iter_mut().zip(s.param_slots(target)) {
                    *r |= read;
                }
                for (k, kept) in kept_by_all.iter_mut().zip(s.preserves) {
                    *k &= kept;
                }
            }
            None => kept_by_all = NO_SLOTS,
        }
    }
    (reads, kept_by_all)
}

/// The summaries ([`summarize`]) of the candidate lists under one set of signatures, by the
/// list's address: a slot's list is shared by every call through it, so each is summarized once
/// rather than at every call of every liveness pass of every function. Made anew whenever the
/// signatures change (each round of the inference), and once for the raise.
pub struct SummaryCache<'a> {
    signatures: &'a HashMap<String, Signature>,
    target: Target,
    lists: std::sync::RwLock<HashMap<usize, (SlotSet, SlotSet)>>,
}

impl<'a> SummaryCache<'a> {
    pub fn new(signatures: &'a HashMap<String, Signature>, target: Target) -> Self {
        Self { signatures, target, lists: Default::default() }
    }

    fn get(&self, candidates: &Candidates) -> (SlotSet, SlotSet) {
        let key = std::sync::Arc::as_ptr(candidates).cast::<()>() as usize;
        if let Some(summary) = self.lists.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
            return *summary;
        }
        let summary = summarize(candidates, self.signatures, self.target);
        self.lists.write().unwrap_or_else(std::sync::PoisonError::into_inner).insert(key, summary);
        summary
    }
}

fn call_model(
    inst: &Inst,
    virtual_calls: &HashMap<ValueId, Candidates>,
    signatures: &HashMap<String, Signature>,
    target: Target,
    summaries: Option<&SummaryCache<'_>>,
) -> (SlotSet, SlotSet) {
    let mut writes = caller_saved(target);
    if let Inst::Call { callee: Callee::Direct(name), .. } = inst
        && let Some(s) = signatures.get(name)
    {
        for (w, kept) in writes.iter_mut().zip(s.preserves) {
            *w &= !kept;
        }
    }
    if let Some(candidates) = virtual_candidates(inst, virtual_calls) {
        let (reads, kept_by_all) = summaries.map_or_else(|| summarize(candidates, signatures, target), |s| s.get(candidates));
        for (w, kept) in writes.iter_mut().zip(kept_by_all) {
            *w &= !kept;
        }
        return (reads, writes);
    }
    let int_slots = |count: usize| {
        let mut set = NO_SLOTS;
        if target.register_arguments() {
            for slot in target.int_param_slots().iter().take(count) {
                set[slot.index()] = true;
            }
        }
        set
    };
    let reads = match inst {
        Inst::Call { callee: Callee::Direct(name), .. } => match signatures.get(name) {
            Some(s) => s.param_slots(target),
            None => match crate::import_of_wrapper(name) {
                Some(symbol) if super::prototypes::is_variadic(symbol) => int_slots(target.int_param_slots().len()),
                Some(symbol) => match super::prototypes::arity(symbol) {
                    Some(arity) => int_slots(arity),
                    None => all_argument_slots(target),
                },
                None => all_argument_slots(target),
            },
        },
        _ => all_argument_slots(target),
    };
    (reads, writes)
}

/// The facts of `f`. `slots` is every call through a vtable slot with the slot's functions, the
/// ones outside the lift included, which `virtual_calls` leaves out.
fn gather(
    f: &Function,
    virtual_calls: &HashMap<ValueId, Candidates>,
    slots: &HashMap<ValueId, Candidates>,
    signatures: &HashMap<String, Signature>,
    target: Target,
    summaries: &SummaryCache<'_>,
) -> Facts {
    let state = f.params[0];
    let pointers = slot_pointers(f, state);
    let order = reverse_post_order(f);
    let model = |inst: &Inst| call_model(inst, virtual_calls, signatures, target, Some(summaries));
    // Two views: one where a `ret` hands back this function's own result register, for what the
    // callers of its callees read after a call (a call in tail position returns the callee's
    // result); one where it hands back nothing, for what the function itself reads at entry.
    // Without the second, the result register would look like a parameter.
    let (low, high) = (target.int_return(), target.high_return());
    let mut exit_set = NO_SLOTS;
    match signatures.get(&f.name).and_then(|s| s.returns) {
        Some(Kind::Int) => exit_set[low.index()] = true,
        Some(Kind::Wide) => {
            exit_set[low.index()] = true;
            exit_set[high.index()] = true;
        }
        Some(Kind::Float | Kind::Double) => exit_set[Slot::Xmm(0).index()] = true,
        None => {}
    }
    let liveness = Liveness::with_calls(f, &pointers, &order, exit_set, &model);
    let own_reads = Liveness::with_calls(f, &pointers, &order, NO_SLOTS, &model);
    let live_in = own_reads.live_in.first().copied().unwrap_or([false; SLOTS]);
    let mut int_positions = [false; ARGUMENT_REGISTERS];
    for (p, slot) in target.int_param_slots().iter().enumerate() {
        int_positions[p] = live_in[slot.index()];
    }
    let xmm_kind = |n: usize| -> Option<Kind> {
        let slot = target.float_param_slot(n);
        (n < target.float_params() && live_in[slot.index()]).then(|| xmm_kind(f, &pointers, slot))
    };
    let float_positions: [Option<Kind>; ARGUMENT_REGISTERS] = std::array::from_fn(xmm_kind);
    // What a lifted callee may pop and read: a call through a vtable slot does what one of its
    // functions does, which the stack analysis narrows to those that can take the arguments at
    // the call.
    let stack_of = |name: &str| {
        signatures.get(name).map_or(CalleeStack { pops: Some(0), arguments: 0 }, |s| CalleeStack {
            pops: Some(s.pops as i64),
            arguments: s.stack_bytes(target),
        })
    };
    // A slot a runtime class's method fills too stays an indirect call, but its lifted functions
    // still say what arguments it may take.
    let slot_stack_of = |name: &str| if signatures.contains_key(name) { stack_of(name) } else { CalleeStack { pops: None, arguments: 0 } };
    let call_pops = |inst: &Inst| -> Option<Vec<CalleeStack>> {
        if let Inst::Call { callee: Callee::Direct(name), .. } = inst
            && name.starts_with("F_")
        {
            return Some(vec![stack_of(name)]);
        }
        if let Some(candidates) = virtual_candidates(inst, virtual_calls) {
            return Some(candidates.iter().map(|(_, name)| stack_of(name)).collect());
        }
        let candidates = virtual_candidates(inst, slots)?;
        Some(candidates.iter().map(|(_, name)| slot_stack_of(name)).collect())
    };
    let stack = analyze_stack(f, state, &pointers, target, &call_pops);
    // The registers past the convention's that a function only its own code calls may take
    // arguments in (see [`Target::private_int_param_slots`]): the ones it reads before writing,
    // where the save of a callee-saved register it writes and puts back is no read.
    let conventional = target.int_param_slots().len();
    let private = target.private_int_param_slots();
    if private.len() > conventional {
        let saves = saved_registers(f, &pointers, &stack, &private[conventional..]);
        let reads = if saves.is_empty() {
            live_in
        } else {
            let own = Liveness::with_calls_ignoring(f, &pointers, &order, NO_SLOTS, &model, &saves);
            own.live_in.first().copied().unwrap_or([false; SLOTS])
        };
        for (p, slot) in private.iter().enumerate().skip(conventional) {
            int_positions[p] = reads[slot.index()];
        }
    }
    // What the block of each direct call sets of those registers before it, by its own code: the
    // result a call before it left in `eax` is no argument set for the next (a `push eax` making
    // room in that one would read it as one), and a call clears what was set before it.
    let mut sets_before_calls = Vec::new();
    if private.len() > conventional {
        for b in &f.blocks {
            let mut set = NO_SLOTS;
            for i in &b.instrs {
                match &i.inst {
                    Inst::Store { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            set[slot.index()] = true;
                        }
                    }
                    inst if super::liveness::call_kind(inst) == Some(super::liveness::CallKind::State) => {
                        if let Inst::Call { callee: Callee::Direct(name), .. } = inst
                            && name.starts_with("F_")
                        {
                            let mask = private[conventional..]
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| set[s.index()])
                                .fold(0u8, |m, (k, _)| m | 1 << k);
                            sets_before_calls.push((name.clone(), mask));
                        }
                        for (s, written) in set.iter_mut().zip(model(inst).1) {
                            if written {
                                *s = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let mut uses_rax_of = Vec::new();
    let mut uses_rdx_of = Vec::new();
    let mut uses_xmm0_of = Vec::new();
    let mut writes_rax = false;
    let mut writes_xmm0: Option<Kind> = None;
    let mut writes = NO_SLOTS;
    for (bi, b) in f.blocks.iter().enumerate() {
        // A block that ends in `unreachable` (a call that never returns, such as the report after
        // a failed cookie check) hands nothing back: what it writes reaches no return, so a caller
        // reading a register after the call reads its own value through it.
        let dead_end = matches!(b.terminator, ra_asm_ir::Terminator::Unreachable);
        let returning = matches!(b.terminator, ra_asm_ir::Terminator::Ret(_));
        let mut calls = 0usize;
        for i in &b.instrs {
            // The slots the function writes, itself or through a call.
            match &i.inst {
                _ if dead_end => {}
                Inst::Store { ptr: Operand::Local(p), .. } => {
                    if let Some(slot) = pointers.get(p) {
                        writes[slot.index()] = true;
                    }
                }
                inst if super::liveness::call_kind(inst) == Some(super::liveness::CallKind::State) => {
                    let called = model(inst).1;
                    // `jmp target`, or `call target; ret`: the function hands back what its
                    // callee left in `rax`, as the thunk of an import a vtable names does.
                    writes_rax |= returning && called[low.index()];
                    for (w, called) in writes.iter_mut().zip(called) {
                        *w |= called;
                    }
                }
                _ => {}
            }
            // The lifted callees of a call: one, or the functions that fill a vtable slot.
            let callees: Vec<&String> = match &i.inst {
                Inst::Call { callee: Callee::Direct(name), .. } if name.starts_with("F_") => vec![name],
                inst => virtual_candidates(inst, virtual_calls).map(|c| c.iter().map(|(_, name)| name).collect()).unwrap_or_default(),
            };
            match &i.inst {
                Inst::Store { ptr: Operand::Local(p), .. } if !dead_end && pointers.get(p) == Some(&low) => writes_rax = true,
                Inst::Store { ty, ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&Slot::Xmm(0)) => match ty {
                    Type::Double => writes_xmm0 = Some(Kind::Double),
                    Type::Float if writes_xmm0 != Some(Kind::Double) => writes_xmm0 = Some(Kind::Float),
                    _ => {}
                },
                Inst::Call { .. } if !callees.is_empty() => {
                    if let Some(live) = liveness.live_after_call[bi].get(calls) {
                        for name in callees {
                            if live[low.index()] {
                                uses_rax_of.push(name.clone());
                            }
                            if live[high.index()] {
                                uses_rdx_of.push(name.clone());
                            }
                            if live[Slot::Xmm(0).index()] {
                                uses_xmm0_of.push(name.clone());
                            }
                        }
                    }
                    calls += 1;
                }
                inst if super::liveness::call_kind(inst) == Some(super::liveness::CallKind::State) => calls += 1,
                _ => {}
            }
        }
    }
    Facts {
        name: f.name.clone(),
        int_positions,
        float_positions,
        stack,
        writes_xmm0,
        uses_xmm0_of,
        writes_rax,
        writes,
        uses_rax_of,
        uses_rdx_of,
        sets_before_calls,
        noreturn: f.attrs.iter().any(|a| a == "noreturn"),
    }
}

/// The loads of `f` that save a callee-saved register of `slots` for its caller: the register is
/// one the function writes and holds its entry value again at every return ([`StackInfo::kept`]),
/// and the load's value goes only into memory (a `push esi`), whole or truncated to its low bits.
/// Such a load keeps the register's value rather than use it: it makes no parameter.
fn saved_registers(f: &Function, pointers: &HashMap<ValueId, Slot>, stack: &StackInfo, slots: &[Slot]) -> std::collections::HashSet<ValueId> {
    let mut out = std::collections::HashSet::new();
    let saved = |slot: Slot| match slot {
        Slot::Gpr(r) => stack.kept & (1u64 << r) != 0,
        _ => false,
    };
    let written: std::collections::HashSet<Slot> = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(|i| match &i.inst {
            Inst::Store { ptr: Operand::Local(p), .. } => pointers.get(p).copied(),
            _ => None,
        })
        .collect();
    let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, &i.inst))).collect();
    // Whether `o` is a value read from memory that is no register's (a `pop`), widened or not.
    let popped = |o: &Operand| {
        let mut at = o.local();
        for _ in 0..3 {
            match at.and_then(|v| defs.get(&v)) {
                Some(Inst::Load { ptr: Operand::Local(p), .. }) => return !pointers.contains_key(p),
                Some(Inst::Cast { value, .. }) => at = value.local(),
                _ => return false,
            }
        }
        false
    };
    // Whether every return puts `slot` back from memory or never changed it: on no path to a
    // return is the last write of it anything but a `pop`. Where the stack analysis loses the
    // offsets (a frame aligned with `and esp, -8`, an indirect call that pops what it pops) it
    // cannot follow the save to the restore, but the restore shows by itself. The returns of an
    // exception leaving the function (the lift's `eh.` blocks past a call's `eh.ok`), after which
    // no caller reads a register, say nothing.
    let restored = |slot: Slot| {
        let index_of: HashMap<ra_asm_ir::BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
        // Per block: whether its own last write of the slot is something else than a pop.
        let own: Vec<Option<bool>> = f
            .blocks
            .iter()
            .map(|b| {
                b.instrs.iter().rev().find_map(|i| match &i.inst {
                    Inst::Store { value, ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&slot) => Some(!popped(value)),
                    _ => None,
                })
            })
            .collect();
        let mut bad_out: Vec<bool> = own.iter().map(|o| o.unwrap_or(false)).collect();
        loop {
            let mut changed = false;
            for (bi, b) in f.blocks.iter().enumerate() {
                if !bad_out[bi] {
                    continue;
                }
                for s in b.terminator.successors() {
                    let si = index_of[&s];
                    if own[si].is_none() && !bad_out[si] {
                        bad_out[si] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let leaving = |name: &str| name.starts_with("eh.") && !name.starts_with("eh.ok.");
        f.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b.terminator, ra_asm_ir::Terminator::Ret(_)) && !leaving(&b.name))
            .all(|(bi, _)| !bad_out[bi])
    };
    let candidates: Vec<Slot> = slots.iter().copied().filter(|s| written.contains(s) && (saved(*s) || restored(*s))).collect();
    if candidates.is_empty() {
        return out;
    }
    let mut users: HashMap<ValueId, Vec<&ra_asm_ir::Instruction>> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        for v in i.inst.uses() {
            users.entry(v).or_default().push(i);
        }
    }
    // Whether `v` goes only into memory that is no register's: the value stored, never the
    // address, through a truncation to its low bits or not.
    fn stored(v: ValueId, users: &HashMap<ValueId, Vec<&ra_asm_ir::Instruction>>, pointers: &HashMap<ValueId, Slot>, depth: u8) -> bool {
        users.get(&v).is_some_and(|us| {
            us.iter().all(|u| match (&u.inst, u.result) {
                (Inst::Store { value: Operand::Local(x), ptr, .. }, _) => {
                    *x == v && ptr.local() != Some(v) && ptr.local().is_none_or(|p| !pointers.contains_key(&p))
                }
                (Inst::Cast { cast: Cast::Trunc, .. }, Some(r)) if depth == 0 => stored(r, users, pointers, 1),
                _ => false,
            })
        })
    }
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let (Some(v), Inst::Load { ptr: Operand::Local(p), .. }) = (i.result, &i.inst) else { continue };
        if pointers.get(p).is_some_and(|s| candidates.contains(s)) && stored(v, &users, pointers, 0) {
            out.insert(v);
        }
    }
    out
}

/// The kind of a float parameter in vector slot `slot`: `float` when the function reads it as a
/// float and never as a double, else `double`. A read at a vector type says nothing either way.
fn xmm_kind(f: &Function, pointers: &HashMap<ValueId, Slot>, slot: Slot) -> Kind {
    let mut saw_float = false;
    for b in &f.blocks {
        for i in &b.instrs {
            if let Inst::Load { ty, ptr: Operand::Local(p), .. } = &i.inst
                && pointers.get(p) == Some(&slot)
            {
                match ty {
                    Type::Double => return Kind::Double,
                    Type::Float => saw_float = true,
                    _ => {}
                }
            }
        }
    }
    if saw_float { Kind::Float } else { Kind::Double }
}
