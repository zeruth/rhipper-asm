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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ra_asm_ir::{Callee, Function, Inst, Module, Operand, Type, ValueId};

use super::Target;
use super::liveness::{Liveness, NO_SLOTS, RAX, RCX, RDX, SLOTS, Slot, SlotSet, reverse_post_order, slot_pointers};
use super::stack::{StackInfo, analyze as analyze_stack};

/// The kind of a parameter or a return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An integer of the address width.
    Int,
    /// A 64-bit integer on the x86 stack: two slots.
    Wide,
    Float,
    Double,
}

impl Kind {
    /// The IR type: every integer is 64 bits until the width pass narrows it.
    pub fn ty(self) -> Type {
        match self {
            Self::Int | Self::Wide => Type::I64,
            Self::Float => Type::Float,
            Self::Double => Type::Double,
        }
    }

    /// The bytes a parameter of this kind takes on the stack.
    pub fn stack_width(self, target: Target) -> i64 {
        match self {
            Self::Wide | Self::Double => 8,
            Self::Int | Self::Float => target.ptr(),
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
            at += kind.stack_width(target);
        }
        offsets
    }

    /// The bytes the stack parameters take.
    pub fn stack_bytes(&self, target: Target) -> i64 {
        self.stack_params.iter().map(|k| k.stack_width(target)).sum()
    }
}

/// The registers a call clobbers by the convention: everything the callee need not keep.
pub fn caller_saved(target: Target) -> SlotSet {
    let mut set = NO_SLOTS;
    for slot in Slot::all() {
        if !target.callee_saved(slot) {
            set[slot.index()] = true;
        }
    }
    set
}

/// The registers an unknown callee may read. On Win64 every argument position, as integers and as
/// floats. On x86 only `ecx`: the `this` of a `thiscall` method, which a call through a vtable
/// reaches; `edx` and the vector registers carry arguments only under clang's convention for a
/// local function, which no pointer names.
pub fn all_argument_slots(target: Target) -> SlotSet {
    let mut set = NO_SLOTS;
    if target.is_64() {
        for slot in target.int_param_slots() {
            set[slot.index()] = true;
        }
        for n in 0..target.float_params() {
            set[Slot::Xmm(n as u8).index()] = true;
        }
    } else {
        set[RCX.index()] = true;
    }
    set
}

/// The evidence one function gives about itself and its callees.
struct Facts {
    name: String,
    int_positions: [bool; 4],
    float_positions: [Option<Kind>; 4],
    stack: StackInfo,
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
    noreturn: bool,
}

/// Infers the signature of every `F_` function of `module`. A root (the entry, an export, a
/// function a pointer names) returns a value when it writes `rax`, since its callers are not seen.
pub fn infer(module: &Module, roots: &BTreeSet<String>, target: Target) -> HashMap<String, Signature> {
    // The stack analysis reads constants; the machine IR materializes them, so fold them first.
    let functions: Vec<Function> = module
        .functions
        .iter()
        .filter(|f| f.name.starts_with("F_"))
        .map(|f| {
            let mut f = f.clone();
            super::simplify::run(&mut f);
            f
        })
        .collect();
    let mut signatures: HashMap<String, Signature> = functions.iter().map(|f| (f.name.clone(), Signature::default())).collect();
    let ints = target.int_param_slots().len();
    let floats = target.float_params();
    for _round in 0..8 {
        let facts: Vec<Facts> = functions.iter().map(|f| gather(f, &signatures, target)).collect();
        let xmm0_kinds: HashMap<&str, Option<Kind>> = facts.iter().map(|f| (f.name.as_str(), f.writes_xmm0)).collect();
        // Through the callees as well: `call _allrem; ret` hands back the 64 bits its callee left.
        let writes_rdx: HashMap<&str, bool> = facts.iter().map(|f| (f.name.as_str(), f.writes[RDX.index()])).collect();
        let writes_rax: HashMap<&str, bool> = facts.iter().map(|f| (f.name.as_str(), f.writes[RAX.index()])).collect();
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
            for callee in &fact.uses_xmm0_of {
                if let Some(r) = returns.get_mut(callee) {
                    *r = Some(xmm0_kinds.get(callee.as_str()).copied().flatten().unwrap_or(Kind::Double));
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
                if let (true, Some(kind)) = (target.is_64(), fact.writes_xmm0) {
                    returns.insert(fact.name.clone(), Some(kind));
                } else if fact.writes_rax {
                    returns.insert(fact.name.clone(), Some(Kind::Int));
                }
            }
        }
        let mut next: HashMap<String, Signature> = HashMap::new();
        for fact in facts {
            let params: Vec<Kind> = if target.is_64() {
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
                // The integer registers, then the float registers, each class up to its last read.
                let int_count = (0..ints).rev().find(|&p| fact.int_positions[p]).map_or(0, |p| p + 1);
                let float_count = (0..floats).rev().find(|&p| fact.float_positions[p].is_some()).map_or(0, |p| p + 1);
                (0..int_count).map(|_| Kind::Int).chain((0..float_count).map(|p| fact.float_positions[p].unwrap_or(Kind::Double))).collect()
            };
            // The stack parameters: one per slot from the first stack argument to the end of the
            // widest read; on x86 a slot read as 64 bits is one wide parameter over two slots,
            // and a 16-byte copy of the arguments (clang moves two 64-bit ones at once) covers
            // one slot per dword.
            let first = target.first_stack_arg();
            let slot = Kind::Int.stack_width(target);
            let mut stack_params = Vec::new();
            if let Some(end) = fact.stack.argument_reads.iter().map(|(&at, &bits)| at + i64::from(bits / 8).max(slot)).max() {
                let mut at = first;
                while at < end {
                    let bits = fact.stack.argument_reads.get(&at).copied().unwrap_or(0);
                    let kind = if !target.is_64() && bits == 64 { Kind::Wide } else { Kind::Int };
                    at += kind.stack_width(target);
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
        signatures = next;
        if settled {
            break;
        }
    }
    signatures
}

/// [`call_model`] for the promotion pass.
pub fn call_model_public(inst: &Inst, signatures: &HashMap<String, Signature>, target: Target) -> (SlotSet, SlotSet) {
    call_model(inst, signatures, target)
}

/// What a state call reads and writes under the current signatures. An import reads as many
/// argument registers as its prototype says (none on x86, where they are on the stack); a
/// variadic C function reads the integer argument registers, where the Win64 convention puts a
/// float argument of its too; an unknown import or an indirect call reads every argument
/// register. A lifted callee writes the caller-saved registers it does not preserve; anything
/// else writes them all.
fn call_model(inst: &Inst, signatures: &HashMap<String, Signature>, target: Target) -> (SlotSet, SlotSet) {
    let mut writes = caller_saved(target);
    if let Inst::Call { callee: Callee::Direct(name), .. } = inst
        && let Some(s) = signatures.get(name)
    {
        for (w, kept) in writes.iter_mut().zip(s.preserves) {
            *w &= !kept;
        }
    }
    let int_slots = |count: usize| {
        let mut set = NO_SLOTS;
        if target.is_64() {
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
                Some(symbol) if super::prototypes::is_variadic(symbol) => int_slots(4),
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

fn gather(f: &Function, signatures: &HashMap<String, Signature>, target: Target) -> Facts {
    let state = f.params[0];
    let pointers = slot_pointers(f, state);
    let order = reverse_post_order(f);
    let model = |inst: &Inst| call_model(inst, signatures, target);
    // Two views: one where a `ret` hands back this function's own result register, for what the
    // callers of its callees read after a call (a call in tail position returns the callee's
    // result); one where it hands back nothing, for what the function itself reads at entry.
    // Without the second, the result register would look like a parameter.
    let mut exit_set = NO_SLOTS;
    match signatures.get(&f.name).and_then(|s| s.returns) {
        Some(Kind::Int) => exit_set[RAX.index()] = true,
        Some(Kind::Wide) => {
            exit_set[RAX.index()] = true;
            exit_set[RDX.index()] = true;
        }
        Some(Kind::Float | Kind::Double) => exit_set[Slot::Xmm(0).index()] = true,
        None => {}
    }
    let liveness = Liveness::with_calls(f, &pointers, &order, exit_set, &model);
    let own_reads = Liveness::with_calls(f, &pointers, &order, NO_SLOTS, &model);
    let live_in = own_reads.live_in.first().copied().unwrap_or([false; SLOTS]);
    let mut int_positions = [false; 4];
    for (p, slot) in target.int_param_slots().iter().enumerate() {
        int_positions[p] = live_in[slot.index()];
    }
    let xmm_kind = |n: u8| -> Option<Kind> {
        (usize::from(n) < target.float_params() && live_in[Slot::Xmm(n).index()]).then(|| xmm_kind(f, &pointers, n))
    };
    let float_positions = [xmm_kind(0), xmm_kind(1), xmm_kind(2), xmm_kind(3)];
    let pops_of = |name: &str| signatures.get(name).map_or(0, |s| s.pops as i64);
    let stack = analyze_stack(f, state, &pointers, target, &pops_of);

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
                    for (w, called) in writes.iter_mut().zip(model(inst).1) {
                        *w |= called;
                    }
                }
                _ => {}
            }
            match &i.inst {
                Inst::Store { ptr: Operand::Local(p), .. } if !dead_end && pointers.get(p) == Some(&RAX) => writes_rax = true,
                Inst::Store { ty, ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&Slot::Xmm(0)) => match ty {
                    Type::Double => writes_xmm0 = Some(Kind::Double),
                    Type::Float if writes_xmm0 != Some(Kind::Double) => writes_xmm0 = Some(Kind::Float),
                    _ => {}
                },
                Inst::Call { callee: Callee::Direct(name), .. } if name.starts_with("F_") => {
                    if let Some(live) = liveness.live_after_call[bi].get(calls) {
                        if live[RAX.index()] {
                            uses_rax_of.push(name.clone());
                        }
                        if live[RDX.index()] {
                            uses_rdx_of.push(name.clone());
                        }
                        if live[Slot::Xmm(0).index()] {
                            uses_xmm0_of.push(name.clone());
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
        noreturn: f.attrs.iter().any(|a| a == "noreturn"),
    }
}

/// The kind of a float parameter in `xmmN`: `float` when the function reads it as a float and
/// never as a double, else `double`. A read at a vector type says nothing either way.
fn xmm_kind(f: &Function, pointers: &HashMap<ValueId, Slot>, n: u8) -> Kind {
    let mut saw_float = false;
    for b in &f.blocks {
        for i in &b.instrs {
            if let Inst::Load { ty, ptr: Operand::Local(p), .. } = &i.inst
                && pointers.get(p) == Some(&Slot::Xmm(n))
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
