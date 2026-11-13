//! The invariants of a level, as code: what a function of that level may hold, checked rather
//! than assumed, so a pass that breaks its input's shape is caught where it happens and not by
//! the compiler of the output or by a run that goes wrong.
//!
//! The machine level is what the lift writes: `F_x(ptr %state)` with every register in memory.
//! Its rules: every block ends in a terminator; the state is touched only through a field its
//! layout names; a call is direct, to a lifted function with the caller's state or to the
//! runtime, and a computed target goes through `ra_call`; no value is `undef`.
//!
//! The promoted level keeps the `%State` interface, so the machine level's rules hold, and the
//! general registers and the flags travel in values: the state is read and written only where
//! the function meets code that reads the state itself. A register is loaded where the block does
//! not hold its value yet (at most once between two calls that take the state), and stored only
//! in the run of stores right before such a call, a stop or the return; a flag is never stored,
//! since no convention carries one across a call. The vector registers stay in memory.
//!
//! The ABI level has no state: no `%State` parameter and no field of one. A direct call passes
//! what the function or the declaration it names takes, which needs the module ([`check_in`]). The
//! stack is the frame the function allocates: an access at a known offset lies inside it, and
//! every access through the frame, rather than through a local of its own, is flagged. The count
//! of the flagged accesses is what a passing check returns, since they are what the raise left
//! of the machine's stack; the levels below flag nothing.

use std::collections::{HashMap, HashSet};
use std::fmt;

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, FnType, Function, Inst, Instruction, Module, Operand, Terminator, Type, ValueId};

use super::Level;
use super::frame::signed;
use super::liveness::{CallKind, Slot, call_kind, slot_pointers};
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
    /// Promoted: a register is loaded and stored only where the state is synchronized.
    Sync,
    /// ABI: the function has no state, neither the parameter nor a field of one.
    State,
    /// ABI: a direct call passes what its callee takes and takes what it returns.
    Signature,
    /// ABI: a stack access at a known offset lies inside the frame.
    Stack,
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
/// violation in block order, then instruction order, with its block, or the count of the stack
/// accesses the level flags. Alone, a function's direct calls at the ABI level have no callee to
/// be judged against: [`check_in`] has the module.
pub fn check(function: &Function, level: Level) -> Result<usize, Violation> {
    check_with(function, level, &state_fields())
}

/// [`check`] against the state whose fields are `state` (see [`crate::state::state_fields_for`]).
pub fn check_with(function: &Function, level: Level, state: &[Type]) -> Result<usize, Violation> {
    rules(function, level, state, None)
}

/// [`check`] of a function of `module`: the state is the module's `%State`, and at the ABI level
/// a direct call is judged against the function or the declaration it names.
pub fn check_in(module: &Module, function: &Function, level: Level) -> Result<usize, Violation> {
    let state = module.types.iter().find(|(name, _)| name == STATE_TYPE).map_or_else(state_fields, |(_, fields)| fields.clone());
    rules(function, level, &state, Some(module))
}

fn rules(f: &Function, level: Level, state: &[Type], module: Option<&Module>) -> Result<usize, Violation> {
    match level {
        Level::Machine => machine(f, state).map(|()| 0),
        Level::Promoted => machine(f, state).and_then(|()| promoted(f)).map(|()| 0),
        Level::Abi => abi(f, module),
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

/// The promoted level's own rule, over a function the machine level's rules pass: the general
/// registers and the flags meet the state only where it is synchronized.
fn promoted(f: &Function) -> Result<(), Violation> {
    let Some(&state) = f.params.first() else { return Ok(()) };
    let pointers = slot_pointers(f, state);
    let register = |ptr: &Operand| ptr.local().and_then(|p| pointers.get(&p)).copied().filter(|slot| !slot.is_xmm());
    for block in &f.blocks {
        // The registers whose value the block has, loaded or stored, since the state last changed
        // under it.
        let mut held: HashSet<Slot> = HashSet::new();
        for (at, i) in block.instrs.iter().enumerate() {
            let fault = match &i.inst {
                Inst::Load { ptr, .. } => register(ptr)
                    .filter(|slot| !held.insert(*slot))
                    .map(|slot| format!("`{}` is loaded where the block already holds its value", slot.name())),
                Inst::Store { ptr, .. } => register(ptr).and_then(|slot| {
                    held.insert(slot);
                    if slot.is_flag() {
                        Some(format!("the flag `{}` is stored, and no call or return takes a flag", slot.name()))
                    } else if flushed(block, at, &pointers) {
                        None
                    } else {
                        Some(format!("`{}` is stored away from a call that takes the state and from the return", slot.name()))
                    }
                }),
                inst => {
                    if call_kind(inst) == Some(CallKind::State) {
                        held.clear();
                    }
                    None
                }
            };
            if let Some(what) = fault {
                return Err(Violation { block: block.name.clone(), rule: Rule::Sync, what: format!("{}: {what}", subject(f, i)) });
            }
        }
    }
    Ok(())
}

/// Whether the store at `at` is one of the run of register stores that ends in a call that reads
/// the state, a stop, or the block's return.
fn flushed(block: &Block, at: usize, pointers: &HashMap<ValueId, Slot>) -> bool {
    let names_register = |v: ValueId| pointers.contains_key(&v);
    for i in &block.instrs[at + 1..] {
        match &i.inst {
            Inst::Gep { .. } if i.result.is_some_and(names_register) => {}
            Inst::Store { ptr, .. } if ptr.local().is_some_and(names_register) => {}
            inst => return matches!(call_kind(inst), Some(CallKind::State | CallKind::NoReturn)),
        }
    }
    matches!(block.terminator, Terminator::Ret(_))
}

/// The ABI level's rules, and the count of the stack accesses that stay in the frame.
fn abi(f: &Function, module: Option<&Module>) -> Result<usize, Violation> {
    if f.params.iter().any(|p| f.value_name(*p) == "state") {
        let block = f.blocks.first().map_or_else(String::new, |b| b.name.clone());
        return Err(Violation { block, rule: Rule::State, what: "the function takes the state".to_owned() });
    }
    let frame = Frame::of(f);
    let mut flagged = 0;
    for block in &f.blocks {
        for i in &block.instrs {
            let fault = match &i.inst {
                Inst::Gep { base_ty: Type::Named(name), .. } if name == STATE_TYPE => {
                    Some((Rule::State, "a field of `%State`, which the level does not have".to_owned()))
                }
                Inst::Call { fn_ty, callee: Callee::Direct(name), args, .. } => {
                    module.and_then(|m| signature(m, name, fn_ty, args)).map(|what| (Rule::Signature, what))
                }
                Inst::Load { ty, ptr, .. } | Inst::Store { ty, ptr, .. } => match frame.access(ptr, ty) {
                    Ok(in_frame) => {
                        flagged += usize::from(in_frame);
                        None
                    }
                    Err(what) => Some((Rule::Stack, what)),
                },
                _ => None,
            };
            if let Some((rule, what)) = fault {
                return Err(Violation { block: block.name.clone(), rule, what: format!("{}: {what}", subject(f, i)) });
            }
        }
    }
    Ok(flagged)
}

/// What is wrong with a direct call against the function or the declaration of `module` it
/// names, if anything: the arguments are the callee's parameters, with more after them only for
/// a variadic one, and the result is what it returns.
fn signature(module: &Module, name: &str, fn_ty: &FnType, args: &[(Type, Operand)]) -> Option<String> {
    let Some(callee) = module.function(name).map(|f| &f.ty).or_else(|| module.declaration(name).map(|d| &d.ty)) else {
        return Some(format!("`@{name}` is neither a function nor a declaration of the module"));
    };
    let arity = if callee.varargs { args.len() >= callee.params.len() } else { args.len() == callee.params.len() };
    let fits = arity && fn_ty.ret == callee.ret && callee.params.iter().zip(args).all(|(param, (arg, _))| param == arg);
    (!fits).then(|| {
        let called = FnType::new(fn_ty.ret.clone(), args.iter().map(|(ty, _)| ty.clone()).collect());
        format!("`@{name}` is called as `{called}` and is `{callee}`")
    })
}

/// The frame of an ABI function: the block of bytes the rewrite allocates for the machine's
/// stack, the only `[n x i8]` a function allocates, with the values that are addresses into it
/// and, for those at a constant distance from its start, the distance.
struct Frame {
    size: u64,
    derived: HashSet<ValueId>,
    offsets: HashMap<ValueId, i64>,
}

impl Frame {
    fn of(f: &Function) -> Self {
        let mut frame = Self { size: 0, derived: HashSet::new(), offsets: HashMap::new() };
        let instrs = || f.blocks.iter().flat_map(|b| &b.instrs);
        let found = instrs().find_map(|i| match (&i.inst, i.result) {
            (Inst::Alloca { ty: Type::Array(size, element), .. }, Some(v)) if **element == Type::I8 => Some((v, *size)),
            _ => None,
        });
        let Some((base, size)) = found else { return frame };
        frame.size = size;
        frame.derived.insert(base);
        frame.offsets.insert(base, 0);
        // An address into the frame is whatever the frame's own address flows into: the casts and
        // the arithmetic of the machine's stack pointer, and the joins of those.
        loop {
            let mut changed = false;
            for i in instrs() {
                let Some(v) = i.result.filter(|v| !frame.derived.contains(v)) else { continue };
                let from = |o: &Operand| o.local().is_some_and(|l| frame.derived.contains(&l));
                let carries = match &i.inst {
                    Inst::Gep { ptr, .. } => from(ptr),
                    Inst::Cast { value, .. } | Inst::Freeze { value, .. } => from(value),
                    Inst::Bin { op: BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or, lhs, rhs, .. } => from(lhs) || from(rhs),
                    Inst::Select { then, els, .. } => from(then) || from(els),
                    Inst::Phi { incoming, .. } => incoming.iter().any(|(o, _)| from(o)),
                    _ => false,
                };
                if carries {
                    frame.derived.insert(v);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // The distance, where every step is a constant; a join keeps it when its paths agree.
        loop {
            let mut changed = false;
            for i in instrs() {
                let Some(v) = i.result.filter(|v| frame.derived.contains(v) && !frame.offsets.contains_key(v)) else { continue };
                let at = |o: &Operand| o.local().and_then(|l| frame.offsets.get(&l).copied());
                let offset = match &i.inst {
                    Inst::Gep { base_ty: Type::I8, ptr, indices, .. } => match indices.as_slice() {
                        [(_, Operand::Int(k))] => at(ptr).zip(i64::try_from(*k).ok()).and_then(|(base, k)| base.checked_add(k)),
                        _ => None,
                    },
                    Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt | Cast::Trunc | Cast::ZExt | Cast::SExt, value, .. } => at(value),
                    Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs, rhs: Operand::Int(c) }
                    | Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs: Operand::Int(c), rhs: lhs } => {
                        at(lhs).and_then(|base| base.checked_add(signed(*c, *bits)))
                    }
                    Inst::Bin { op: BinOp::Sub, ty: Type::Int(bits), lhs, rhs: Operand::Int(c) } => {
                        at(lhs).and_then(|base| base.checked_sub(signed(*c, *bits)))
                    }
                    Inst::Phi { incoming, .. } => {
                        let mut paths = incoming.iter().map(|(o, _)| at(o));
                        paths.next().flatten().filter(|first| paths.all(|path| path == Some(*first)))
                    }
                    _ => None,
                };
                if let Some(offset) = offset {
                    frame.offsets.insert(v, offset);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        frame
    }

    /// Whether a load or a store of a `ty` through `ptr` is an access of the frame, or what is
    /// wrong with it: its bytes, at a known offset, are not all the frame's.
    fn access(&self, ptr: &Operand, ty: &Type) -> Result<bool, String> {
        let Some(p) = ptr.local().filter(|p| self.derived.contains(p)) else { return Ok(false) };
        if let Some(&offset) = self.offsets.get(&p) {
            let bytes = ty.size(8).unwrap_or(1);
            let inside = u64::try_from(offset).ok().and_then(|start| start.checked_add(bytes)).is_some_and(|end| end <= self.size);
            if !inside {
                return Err(format!("{bytes} bytes at offset {offset} of the frame, which has {}", self.size));
            }
        }
        Ok(true)
    }
}
