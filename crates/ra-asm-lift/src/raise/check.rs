//! The invariants of a level, as code: what a function of that level may hold, checked rather
//! than assumed, so a pass that breaks its input's shape is caught where it happens and not by
//! the compiler of the output or by a run that goes wrong.
//!
//! The machine level is what the lift writes: `F_x(ptr %state)` with every register in memory.
//! Its rules: every block ends in a terminator; the state is touched only through a field its
//! layout names; a call is direct, to a lifted function with the caller's state or to the
//! runtime, and a computed target goes through `ra_call`; no value is `undef`. The raised levels
//! have no rule written yet, and [`check`] passes them.

use std::collections::HashMap;
use std::fmt;

use ra_asm_ir::{Block, BlockId, Callee, Function, Inst, Instruction, Operand, Terminator, Type, ValueId};

use super::Level;
use crate::state::{STATE_TYPE, state_fields};

/// The rule a function broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A block ends in a terminator, and the terminator names blocks of the function.
    Terminator,
    /// A register access is a field the state's layout names.
    Register,
    /// A call is direct to a lifted function, or through `ra_call`.
    Call,
    /// Nothing is `undef`.
    Undef,
}

/// The first place a function breaks a rule of its level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The name of the block it is in.
    pub block: String,
    pub rule: Rule,
    /// The instruction and what is wrong with it, for a person.
    pub what: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block `{}`: {}", self.block, self.what)
    }
}

impl std::error::Error for Violation {}

/// Checks `function` against the invariants of `level`, with the x86 `%State`: the first
/// violation in block order, then instruction order, with its block.
pub fn check(function: &Function, level: Level) -> Result<(), Violation> {
    check_with(function, level, &state_fields())
}

/// [`check`] against the state whose fields are `state` (see [`crate::state::state_fields_for`]).
pub fn check_with(function: &Function, level: Level, state: &[Type]) -> Result<(), Violation> {
    match level {
        Level::Machine => machine(function, state),
        Level::Promoted | Level::Abi => Ok(()),
    }
}

fn machine(f: &Function, fields: &[Type]) -> Result<(), Violation> {
    // A lifted function or an import's wrapper takes the state and nothing else; the stub of an
    // import the host lacks has none.
    let state = if f.ty.ret == Type::Void && f.ty.params == [Type::Ptr] { f.params.first().copied() } else { None };
    let preds = f.predecessors();
    for block in &f.blocks {
        let fail = |rule: Rule, what: String| Violation { block: block.name.clone(), rule, what };
        for i in &block.instrs {
            let fault = if undef(&i.inst) {
                Some((Rule::Undef, "an operand is `undef`".to_owned()))
            } else if let Some(what) = register(&i.inst, state, fields) {
                Some((Rule::Register, what))
            } else {
                call(f, &i.inst, state).map(|what| (Rule::Call, what))
            };
            if let Some((rule, what)) = fault {
                return Err(fail(rule, format!("{}: {what}", subject(f, i))));
            }
        }
        if block.terminator.operands().iter().any(|o| matches!(o, Operand::Undef)) {
            return Err(fail(Rule::Undef, "the terminator's operand is `undef`".to_owned()));
        }
        if let Some(what) = terminator(f, block, &preds) {
            return Err(fail(Rule::Terminator, what));
        }
    }
    Ok(())
}

/// How a violation names an instruction: the value it defines, or its kind.
fn subject(f: &Function, i: &Instruction) -> String {
    match (i.result, &i.inst) {
        (Some(v), _) => format!("`%{}`", f.value_name(v)),
        (None, Inst::Store { .. }) => "a store".to_owned(),
        (None, Inst::Call { .. }) => "a call".to_owned(),
        (None, _) => "an instruction".to_owned(),
    }
}

/// Whether an operand the instruction reads is `undef`. The second vector of a shuffle that
/// takes every lane from the first is never read, which is how the lift writes a permutation.
fn undef(inst: &Inst) -> bool {
    let is_undef = |o: &Operand| matches!(o, Operand::Undef);
    match inst {
        Inst::ShuffleVector { ty: Type::Vector(lanes, _), a, b: Operand::Undef, mask }
            if mask.iter().all(|&lane| u32::try_from(lane).is_ok_and(|lane| lane < *lanes)) =>
        {
            is_undef(a)
        }
        other => other.operands().into_iter().any(is_undef),
    }
}

/// What is wrong with the instruction's use of the state, if anything: the state is the base of
/// an address of one of its fields, or an argument of a call, and nothing else.
fn register(inst: &Inst, state: Option<ValueId>, fields: &[Type]) -> Option<String> {
    let is_state = |o: &Operand| state.is_some() && o.local() == state;
    match inst {
        Inst::Gep { base_ty: Type::Named(name), ptr, indices, .. } if name == STATE_TYPE => {
            if is_state(ptr) {
                field(indices, fields).err()
            } else {
                Some("a field of `%State` is taken from a pointer that is not the function's state".to_owned())
            }
        }
        // The call's own rule judges the state it passes.
        Inst::Call { .. } => None,
        other => other.operands().into_iter().any(is_state).then(|| "the state is used outside a field of its layout".to_owned()),
    }
}

/// Whether the indices of an address into the state name a field of `fields`: the state itself,
/// the field, and for an array of registers a slot it has, each a constant.
fn field(indices: &[(Type, Operand)], fields: &[Type]) -> Result<(), String> {
    let index = |n: usize| match indices.get(n) {
        Some((_, Operand::Int(v))) => Some(*v),
        _ => None,
    };
    if index(0) != Some(0) {
        return Err("the address does not start at the state itself, index 0".to_owned());
    }
    let Some(number) = index(1) else {
        return Err("the field of the state is not a constant".to_owned());
    };
    let Some(ty) = usize::try_from(number).ok().and_then(|n| fields.get(n)) else {
        return Err(format!("the state has no field {number}: its layout has {}", fields.len()));
    };
    match ty {
        Type::Array(count, _) => match index(2) {
            Some(slot) if indices.len() == 3 && u64::try_from(slot).is_ok_and(|s| s < *count) => Ok(()),
            Some(slot) if indices.len() == 3 => Err(format!("field {number} of the state has no slot {slot}: it holds {count}")),
            _ => Err(format!("field {number} of the state is {count} slots, and the address names no constant one")),
        },
        _ if indices.len() == 2 => Ok(()),
        _ => Err(format!("field {number} of the state is one value, and the address goes inside it")),
    }
}

/// What is wrong with a call, if anything. A lifted function and an import's wrapper are
/// `void (ptr %state)` and take the caller's state; `ra_call` takes it with the computed target;
/// the rest of the runtime and LLVM's intrinsics take values; and a wrapper calls its import.
fn call(f: &Function, inst: &Inst, state: Option<ValueId>) -> Option<String> {
    let Inst::Call { fn_ty, callee, args, .. } = inst else { return None };
    let name = match callee {
        Callee::Indirect(_) => return Some("a call through a pointer: a computed target goes through `ra_call`".to_owned()),
        Callee::Direct(name) => name.as_str(),
    };
    let passes_state = state.is_some() && args.first().and_then(|(_, a)| a.local()) == state;
    if name == "ra_call" {
        return (!passes_state || args.len() != 2).then(|| "`ra_call` takes the caller's state and the target".to_owned());
    }
    if name.starts_with("ra_") || name.starts_with("llvm.") || crate::import_of_wrapper(&f.name) == Some(name) {
        return None;
    }
    let lifted = fn_ty.ret == Type::Void && fn_ty.params == [Type::Ptr] && args.len() == 1;
    (!lifted || !passes_state).then(|| format!("`@{name}` is not called as a lifted function, `void (ptr %state)` with the caller's state"))
}

/// What is wrong with how the block ends, if anything. The IR has no block without a terminator:
/// the builder starts each one with `unreachable` and the lift replaces it, so the block never
/// ended is an `unreachable` that follows no call. A call that does not return is in the block,
/// or in every block that leads into an empty one (the path past a throw that never comes back).
fn terminator(f: &Function, block: &Block, preds: &HashMap<BlockId, Vec<BlockId>>) -> Option<String> {
    if let Some(missing) = block.terminator.successors().into_iter().find(|s| f.block(*s).is_none()) {
        return Some(format!("the terminator names block {}, which the function does not have", missing.0));
    }
    if !matches!(block.terminator, Terminator::Unreachable) {
        return None;
    }
    let calls = |b: &Block| b.instrs.iter().any(|i| matches!(i.inst, Inst::Call { .. }));
    let after_call = calls(block)
        || (block.instrs.is_empty() && preds.get(&block.id).is_some_and(|from| from.iter().all(|id| f.block(*id).is_some_and(calls))));
    (!after_call).then(|| "the block has no terminator: its `unreachable` follows no call".to_owned())
}
