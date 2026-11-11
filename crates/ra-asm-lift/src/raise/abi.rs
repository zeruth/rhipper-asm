//! Signatures: what each lifted function takes and returns, by the Win64 convention.
//!
//! A function's parameters are the argument registers it reads before writing (`rcx`, `rdx`,
//! `r8`, `r9` for integers, `xmm0` to `xmm3` for floats, by position) and the stack slots past the
//! home space it reads. It returns a value when a caller reads `rax` (or `xmm0`) after calling it,
//! or when it is a root that writes `rax`. The inference iterates: a call reads its callee's
//! parameters and writes the caller-saved registers, and the callee's parameters are what is being
//! inferred.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ra_asm_ir::{Callee, Function, Inst, Module, Operand, Type, ValueId};

use super::liveness::{Liveness, NO_SLOTS, R8, R9, RAX, RCX, RDX, SLOTS, Slot, SlotSet, abi_exit_set, reverse_post_order, slot_pointers};
use super::stack::{FIRST_STACK_ARG, StackInfo, analyze as analyze_stack};

/// The kind of a parameter or a return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Int,
    Float,
    Double,
}

impl Kind {
    pub fn ty(self) -> Type {
        match self {
            Self::Int => Type::I64,
            Self::Float => Type::Float,
            Self::Double => Type::Double,
        }
    }
}

/// A function's signature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Signature {
    /// The register parameters by position: `rcx`/`xmm0`, `rdx`/`xmm1`, `r8`/`xmm2`, `r9`/`xmm3`.
    pub params: Vec<Kind>,
    /// The stack parameters after them, from offset `0x28` up.
    pub stack_params: Vec<Kind>,
    pub returns: Option<Kind>,
    /// Whether the function never returns.
    pub noreturn: bool,
    /// The bytes the frame needs below the entry stack pointer.
    pub frame_below: u64,
    /// The function moves the stack pointer by a run-time amount.
    pub dynamic_stack: bool,
}

impl Signature {
    pub fn param_count(&self) -> usize {
        self.params.len() + self.stack_params.len()
    }

    /// The register slots the function reads as parameters.
    pub fn param_slots(&self) -> SlotSet {
        let mut set = NO_SLOTS;
        for (p, kind) in self.params.iter().enumerate() {
            let slot = match kind {
                Kind::Int => [RCX, RDX, R8, R9][p],
                Kind::Float | Kind::Double => Slot::Xmm(p as u8),
            };
            set[slot.index()] = true;
        }
        set
    }
}

/// The registers a call clobbers by the convention: everything the callee need not keep.
pub fn caller_saved() -> SlotSet {
    let mut set = NO_SLOTS;
    for slot in Slot::all() {
        if !slot.callee_saved() {
            set[slot.index()] = true;
        }
    }
    set
}

/// The registers an unknown callee may read: the four positions, as integers and as floats.
pub fn all_argument_slots() -> SlotSet {
    let mut set = NO_SLOTS;
    for slot in [RCX, RDX, R8, R9, Slot::Xmm(0), Slot::Xmm(1), Slot::Xmm(2), Slot::Xmm(3)] {
        set[slot.index()] = true;
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
    /// Callees whose `rax` this function reads after the call.
    uses_rax_of: Vec<String>,
    noreturn: bool,
}

/// Infers the signature of every `F_` function of `module`. A root (the entry, an export, a
/// function a pointer names) returns a value when it writes `rax`, since its callers are not seen.
pub fn infer(module: &Module, roots: &BTreeSet<String>) -> HashMap<String, Signature> {
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
    for _round in 0..8 {
        let facts: Vec<Facts> = functions.iter().map(|f| gather(f, &signatures)).collect();
        let mut returns: BTreeMap<String, bool> = facts.iter().map(|f| (f.name.clone(), false)).collect();
        for fact in &facts {
            for callee in &fact.uses_rax_of {
                if let Some(r) = returns.get_mut(callee) {
                    *r = true;
                }
            }
            if roots.contains(&fact.name) && fact.writes_rax {
                returns.insert(fact.name.clone(), true);
            }
        }
        let mut next: HashMap<String, Signature> = HashMap::new();
        for fact in facts {
            let positions = (0..4).rev().find(|&p| fact.int_positions[p] || fact.float_positions[p].is_some()).map_or(0, |p| p + 1);
            let params = (0..positions)
                .map(|p| match (fact.int_positions[p], fact.float_positions[p]) {
                    (false, Some(kind)) => kind,
                    _ => Kind::Int,
                })
                .collect();
            let stack_count = fact.stack.argument_reads.keys().last().map_or(0, |&k| ((k - FIRST_STACK_ARG) / 8 + 1) as usize);
            let stack_params = (0..stack_count).map(|_| Kind::Int).collect();
            let returns = returns.get(&fact.name).copied().unwrap_or(false) && !fact.noreturn;
            let frame_below = (-fact.stack.min_offset).max(0) as u64;
            next.insert(
                fact.name.clone(),
                Signature {
                    params,
                    stack_params,
                    returns: returns.then_some(Kind::Int),
                    noreturn: fact.noreturn,
                    frame_below: (frame_below + 15) & !15,
                    dynamic_stack: fact.stack.dynamic,
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

/// What a state call reads and writes under the current signatures. An import reads as many
/// argument registers as its prototype says; an unknown import or an indirect call reads all four.
fn call_model(inst: &Inst, signatures: &HashMap<String, Signature>) -> (SlotSet, SlotSet) {
    let writes = caller_saved();
    let reads = match inst {
        Inst::Call { callee: Callee::Direct(name), .. } => match signatures.get(name) {
            Some(s) => s.param_slots(),
            None => match crate::import_of_wrapper(name).and_then(super::prototypes::arity) {
                Some(arity) => {
                    let mut set = NO_SLOTS;
                    for slot in [RCX, RDX, R8, R9].into_iter().take(arity) {
                        set[slot.index()] = true;
                    }
                    set
                }
                None => all_argument_slots(),
            },
        },
        _ => all_argument_slots(),
    };
    (reads, writes)
}

fn gather(f: &Function, signatures: &HashMap<String, Signature>) -> Facts {
    let state = f.params[0];
    let pointers = slot_pointers(f, state);
    let order = reverse_post_order(f);
    let model = |inst: &Inst| call_model(inst, signatures);
    // Two views: one where a `ret` hands back the convention's registers, for what callers read
    // after a call; one where it hands back nothing, for what the function itself reads at entry.
    // Without the second, `xmm0` and the callee-saved registers would look like parameters.
    let liveness = Liveness::with_calls(f, &pointers, &order, abi_exit_set(), &model);
    let own_reads = Liveness::with_calls(f, &pointers, &order, NO_SLOTS, &model);
    let live_in = own_reads.live_in.first().copied().unwrap_or([false; SLOTS]);
    let int_positions = [live_in[RCX.index()], live_in[RDX.index()], live_in[R8.index()], live_in[R9.index()]];
    let xmm_kind = |n: u8| -> Option<Kind> { live_in[Slot::Xmm(n).index()].then(|| xmm_kind(f, &pointers, n)) };
    let float_positions = [xmm_kind(0), xmm_kind(1), xmm_kind(2), xmm_kind(3)];
    let stack = analyze_stack(f, state, &pointers);

    let mut uses_rax_of = Vec::new();
    let mut writes_rax = false;
    for (bi, b) in f.blocks.iter().enumerate() {
        let mut calls = 0usize;
        for i in &b.instrs {
            match &i.inst {
                Inst::Store { ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&RAX) => writes_rax = true,
                Inst::Call { callee: Callee::Direct(name), .. } if name.starts_with("F_") => {
                    if let Some(live) = liveness.live_after_call[bi].get(calls)
                        && live[RAX.index()]
                    {
                        uses_rax_of.push(name.clone());
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
        writes_rax,
        uses_rax_of,
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
