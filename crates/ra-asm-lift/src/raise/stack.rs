//! The stack pointer as a symbol: every value that is `rsp at entry + k` for a constant `k`, over
//! the machine IR of a function. From it: the frame the function uses below its entry stack
//! pointer, the stack slots above it that it reads, which are its stack-passed arguments, and the
//! bytes it pops on return beyond the return address, which an x86 `stdcall` or `thiscall`
//! function does.
//!
//! The analysis is a forward dataflow over the blocks. Its state is the offset of the stack
//! pointer slot (`None` when the function moved it by a run-time amount) and a map from SSA values
//! to offsets. A call to a lifted function moves the offset by the return address the callee pops
//! (none on ARM64, whose `bl` leaves it in the link register) and by what else it pops; the pre-
//! and post-indexed ARM64 forms are an add and a store to `sp`, followed like any other; an import
//! wrapper, which runs in place of the native call, moves it
//! by nothing; an indirect x86 call moves it by an amount only the callee knows. A call through
//! a vtable slot whose functions pop different amounts moves it by the amount of those that can
//! take the arguments there: a callee pops only what its caller passed, and the stack bytes that
//! hold a callee-saved register's value from the entry (the prologue's `push esi`) are no
//! argument, since that value means nothing to the function. By the same rule an indirect x86
//! call whose callee no one knows pops nothing when such a save lies right above its return
//! address. A callee whose stack arguments lie in the function's own (a tail jump through a vtable
//! slot, `jmp [eax + 4]`, hands the caller's arguments on as they are) reads them there: they are
//! the function's parameters, as many as the widest of its possible callees takes, even when a
//! runtime class's method shares the slot and keeps the call indirect.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ra_asm_ir::{BinOp, Callee, Cast, Function, Inst, Operand, Terminator, Type, ValueId};

use super::Target;
use super::assume::{self, Assumption, Step};
use super::liveness::{CallKind, GPRS, Slot, call_kind, reverse_post_order};

/// What the stack analysis found.
#[derive(Debug, Clone, Default)]
pub struct StackInfo {
    /// The lowest offset below the entry stack pointer the function touches (a negative number
    /// or zero), through a push, a frame or a store.
    pub min_offset: i64,
    /// The highest offset above the entry stack pointer the function touches, exclusive.
    pub max_offset: i64,
    /// The offsets at or past the first stack argument the function reads, with the width of the
    /// widest read, in bits.
    pub argument_reads: BTreeMap<i64, u16>,
    /// The function moves the stack pointer by a run-time amount somewhere.
    pub dynamic: bool,
    /// The address of a stack argument leaves the stack arithmetic: stored to memory, passed on,
    /// or added to a register. A `va_list`, or a pointer to the arguments: the function reads
    /// them through it, not only by their slots.
    pub argument_address_escapes: bool,
    /// The function stores into a stack argument's slot: a parameter changed in place, or a dead
    /// parameter's slot reused for a local (MSVC does, and takes its address), not a `va_list`.
    pub argument_area_written: bool,
    /// For each call site (the value of the call instruction's position, by block and index), the
    /// stack offset at the call: the caller's outgoing arguments start above it.
    pub call_offsets: HashMap<(usize, usize), i64>,
    /// The bytes the function pops on return beyond the return address: the `N` of `ret N`.
    pub pops: i64,
    /// What the analysis relied on without seeing it hold, in the order of the function's code.
    /// That pushes and pops balance, where they do not visibly: at a block the paths reach with
    /// the stack at different depths, after which the analysis no longer knows the depth, and at
    /// a return whose depth is unknown or is not the one `pops` says, which the signature takes
    /// for every return. That the frame pointer is one, at a call into lifted code made while a
    /// callee-saved register holds an address of the frame: the register is taken to hold it
    /// after, and what is read through it later to be the frame and the arguments.
    pub assumptions: Vec<Assumption>,
    /// The callee-saved registers, as a mask by number, that hold their value from the entry at
    /// every return: the ones no path to a return writes, and the ones it writes after a save to
    /// the stack and reloads from it. A save is taken to stay as it was stored: a store through a
    /// pointer the analysis cannot place, and a callee, are taken not to reach it, which is the
    /// privacy of the frame the promotion lists on its own. None when the analysis did not
    /// settle; all of them for a function no path of which returns.
    pub kept: u64,
}

/// What a lifted callee does to the stack of its caller, as far as the inference knows it so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalleeStack {
    /// The bytes it pops beyond its return address; `None` for a function outside the lift (a
    /// runtime class's method in the same vtable slot), which pops what it pops.
    pub pops: Option<i64>,
    /// The bytes of stack arguments it reads, from the first stack argument up.
    pub arguments: i64,
}

/// Analyzes `f`, whose state pointer is `state`. `call_pops` is what the lifted callee of a call
/// may do to the stack: one callee for a direct call, each function a vtable slot holds; `None`
/// for a callee it does not know.
// The names are the ones the analysis is written in: `f`, `n` blocks, `sp`, a block `b` and an
// instruction `i`.
#[allow(clippy::many_single_char_names)]
pub fn analyze(
    f: &Function,
    state: ValueId,
    pointers: &HashMap<ValueId, Slot>,
    target: Target,
    call_pops: &dyn Fn(&Inst) -> Option<Vec<CalleeStack>>,
) -> StackInfo {
    let n = f.blocks.len();
    let order = reverse_post_order(f);
    let index_of: HashMap<_, _> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let preds = f.predecessors();
    let ptr = target.ptr();
    let pushed = target.pushed_return();
    let Slot::Gpr(sp) = target.sp() else { unreachable!() };
    let sp = usize::from(sp);
    // The registers at each block's entry: None = not reached yet.
    let mut entry: Vec<Option<Regs>> = vec![None; n];
    let mut exit: Vec<Option<Regs>> = vec![None; n];
    let mut at_entry = [None; GPRS];
    at_entry[sp] = Some(0);
    entry[0] = Some(at_entry);
    let mut saves_entry: Vec<Option<Saves>> = vec![None; n];
    let mut saves_exit: Vec<Option<Saves>> = vec![None; n];
    let intact = (0..GPRS).filter(|&r| r != sp && target.callee_saved(Slot::Gpr(r as u8))).fold(0u64, |m, r| m | 1 << r);
    saves_entry[0] = Some(Saves { intact, kept: intact, bytes: BTreeSet::new(), slots: BTreeMap::new() });
    let mut info = StackInfo::default();
    let mut symbolic: HashMap<ValueId, i64> = HashMap::new();
    // The values that are a callee-saved register's value from the entry, by the register.
    let mut entry_values: HashMap<ValueId, u8> = HashMap::new();
    // The values read back from where such a value was saved, by the register it was.
    let mut restored: HashMap<ValueId, u8> = HashMap::new();
    let mut settled = false;
    // The calls into lifted code, by block and index: whether a callee-saved register holds a
    // frame address there, as of the last round.
    let mut frame_pointers: BTreeMap<(usize, usize), bool> = BTreeMap::new();
    let _ = state;

    for _round in 0..8 {
        let mut changed = false;
        for &bi in &order {
            // Join the predecessors.
            if bi != 0 {
                let mut joined: Option<Regs> = None;
                let mut joined_saves: Option<Saves> = None;
                for p in preds.get(&f.blocks[bi].id).cloned().unwrap_or_default() {
                    let pi = index_of[&p];
                    joined = match (joined, exit[pi]) {
                        (None, x) | (x, None) => x,
                        (Some(a), Some(b)) => Some(join(a, b)),
                    };
                    joined_saves = match (joined_saves, &saves_exit[pi]) {
                        (None, x) => x.clone(),
                        (x, None) => x,
                        (Some(a), Some(b)) => Some(a.join(b)),
                    };
                }
                if joined != entry[bi] {
                    entry[bi] = joined;
                    changed = true;
                }
                if joined_saves != saves_entry[bi] {
                    saves_entry[bi] = joined_saves;
                    changed = true;
                }
            }
            let Some(mut regs) = entry[bi] else { continue };
            let mut saves = saves_entry[bi].clone().unwrap_or_default();
            for (ii, i) in f.blocks[bi].instrs.iter().enumerate() {
                let first = target.first_stack_arg();
                let symbolic_arg = |o: &Operand| matches!(o, Operand::Local(v) if symbolic.get(v).is_some_and(|&k| k >= first));
                let escapes = match &i.inst {
                    Inst::Store { ptr, value, .. } => !matches!(ptr, Operand::Local(p) if pointers.contains_key(p)) && symbolic_arg(value),
                    Inst::Bin { op: BinOp::Add | BinOp::Sub, lhs, rhs, .. } => {
                        matches!((lhs, rhs), (Operand::Local(_), Operand::Local(_))) && (symbolic_arg(lhs) || symbolic_arg(rhs))
                    }
                    Inst::Load { .. } | Inst::Cast { .. } | Inst::ICmp { .. } | Inst::Bin { .. } => false,
                    other => other.uses().iter().any(|v| symbolic.get(v).is_some_and(|&k| k >= first)),
                };
                if escapes {
                    info.argument_address_escapes = true;
                }
                match &i.inst {
                    // A register read: symbolic when the register holds a stack offset. A frame
                    // pointer is one, from `mov ebp, esp`; the stale answer of an earlier round
                    // is dropped.
                    Inst::Load { ptr: Operand::Local(p), .. } if matches!(pointers.get(p), Some(Slot::Gpr(_))) => {
                        let Some(Slot::Gpr(r)) = pointers.get(p) else { unreachable!() };
                        if let Some(result) = i.result {
                            match regs[usize::from(*r)] {
                                Some(k) => symbolic.insert(result, k),
                                None => symbolic.remove(&result),
                            };
                            if saves.intact & (1u64 << *r) != 0 {
                                entry_values.insert(result, *r);
                            } else {
                                entry_values.remove(&result);
                            }
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), value, .. } if matches!(pointers.get(p), Some(Slot::Gpr(_))) => {
                        let Some(Slot::Gpr(r)) = pointers.get(p) else { unreachable!() };
                        let k = match value {
                            Operand::Local(v) => symbolic.get(v).copied(),
                            _ => None,
                        };
                        regs[usize::from(*r)] = k;
                        saves.intact &= !(1u64 << *r);
                        // A register reloaded from where its entry value was saved holds it again.
                        if matches!(value, Operand::Local(v) if restored.get(v) == Some(r)) {
                            saves.kept |= 1u64 << *r;
                        } else {
                            saves.kept &= !(1u64 << *r);
                        }
                        if usize::from(*r) == sp && k.is_none() {
                            info.dynamic = true;
                            // A push through a stack pointer the analysis lost writes where it cannot see.
                            saves.bytes.clear();
                            saves.slots.clear();
                        }
                    }
                    Inst::Bin {
                        op: BinOp::Add | BinOp::Sub, ty: Type::Int(bits), lhs: Operand::Local(l), rhs: Operand::Int(c), ..
                    } => {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(l)) {
                            let c = super::frame::signed(*c, *bits);
                            let off =
                                if matches!(i.inst, Inst::Bin { op: BinOp::Add, .. }) { k.wrapping_add(c) } else { k.wrapping_sub(c) };
                            symbolic.insert(result, off);
                        }
                    }
                    Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs: Operand::Int(c), rhs: Operand::Local(r), .. } => {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(r)) {
                            symbolic.insert(result, k.wrapping_add(super::frame::signed(*c, *bits)));
                        }
                    }
                    // The 32-bit model reads the pointer as a dword and widens it back: the same
                    // offset throughout.
                    Inst::Cast { cast, value: Operand::Local(v), .. }
                        if matches!(cast, Cast::IntToPtr | Cast::Trunc | Cast::ZExt | Cast::SExt) =>
                    {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(v)) {
                            symbolic.insert(result, k);
                        }
                        // The 32-bit push of a register stores its low dword.
                        if let Some(result) = i.result {
                            match entry_values.get(v).copied().filter(|_| matches!(cast, Cast::Trunc | Cast::ZExt)) {
                                Some(r) => entry_values.insert(result, r),
                                None => entry_values.remove(&result),
                            };
                            // The 32-bit pop widens the dword it read; nothing reads the half above.
                            match restored.get(v).copied().filter(|_| matches!(cast, Cast::ZExt)) {
                                Some(r) => restored.insert(result, r),
                                None => restored.remove(&result),
                            };
                        }
                    }
                    Inst::Load { ty, ptr: Operand::Local(p), .. } => {
                        // A read of a whole save: the register's value from the entry.
                        if let Some(result) = i.result {
                            let whole = ty.bits().is_some_and(|bits| i64::from(bits) == ptr * 8);
                            match symbolic.get(p).and_then(|k| saves.slots.get(k)).copied().filter(|_| whole) {
                                Some(r) => restored.insert(result, r),
                                None => restored.remove(&result),
                            };
                        }
                        if let Some(&k) = symbolic.get(p) {
                            let bits = ty.bits().unwrap_or(64) as u16;
                            touch(&mut info, k, bits);
                            if k >= target.first_stack_arg() {
                                let e = info.argument_reads.entry(k).or_insert(0);
                                *e = (*e).max(bits);
                            }
                        }
                    }
                    Inst::Store { ty, ptr: Operand::Local(p), value, .. } => {
                        if let Some(&k) = symbolic.get(p) {
                            let bits = ty.bits().unwrap_or(64) as u16;
                            touch(&mut info, k, bits);
                            if k >= target.first_stack_arg() {
                                info.argument_area_written = true;
                            }
                            let bytes = k..k + i64::from(bits.div_ceil(8));
                            let saved = match value {
                                Operand::Local(v) if i64::from(bits / 8) == ptr => entry_values.get(v).copied(),
                                _ => None,
                            };
                            // A store over a save, or over a part of one, ends it.
                            saves.slots.retain(|&at, _| at + ptr <= bytes.start || at >= bytes.end);
                            if let Some(r) = saved {
                                saves.bytes.extend(bytes);
                                saves.slots.insert(k, r);
                            } else {
                                saves.bytes.retain(|b| !bytes.contains(b));
                            }
                        }
                    }
                    inst => {
                        if let Some(CallKind::State) = call_kind(inst) {
                            let callee = match inst {
                                Inst::Call { callee: Callee::Direct(name), .. } => Some(name.as_str()),
                                _ => None,
                            };
                            if let Some(k) = regs[sp] {
                                info.call_offsets.insert((bi, ii), k);
                            }
                            // A host function cannot write a lifted register; lifted code can.
                            if callee.is_none_or(|name| name.starts_with("F_") || name == "ra_call") {
                                let held = (0..GPRS).any(|r| r != sp && target.callee_saved(Slot::Gpr(r as u8)) && regs[r].is_some());
                                frame_pointers.insert((bi, ii), held);
                            }
                            // The callee keeps the callee-saved registers and may write the rest.
                            for (r, offset) in regs.iter_mut().enumerate() {
                                if r != sp && !target.callee_saved(Slot::Gpr(r as u8)) {
                                    *offset = None;
                                }
                            }
                            // An import wrapper runs in place of the native call: nothing was
                            // pushed and the stack pointer stays. For a lifted callee the machine
                            // model pushed the return address before the call and the callee
                            // popped it, and what else it pops: the stack pointer after the call is
                            // that much above the one the push left. An indirect x86 call pops what
                            // its callee says at run time.
                            match callee {
                                Some(name) if crate::import_of_wrapper(name).is_some() => {
                                    // A tail call to an import hands the function's own stack
                                    // arguments on: the wrapper reads them, so they are the
                                    // function's parameters.
                                    if let Some(symbol) = crate::import_of_wrapper(name)
                                        && name.len() > "imp_".len() + symbol.len()
                                    {
                                        // An import the table does not know takes the universal
                                        // arguments, so whatever the caller passed flows on; a
                                        // typed one reads its doubles as two dwords on x86.
                                        let kinds = match super::prototypes::typed(symbol) {
                                            Some(proto) => proto.params,
                                            None => vec![super::abi::Kind::Int; super::prototypes::arity(symbol).unwrap_or(8)],
                                        };
                                        let registers = if target.is_64() { target.int_param_slots().len() } else { 0 };
                                        let mut offset = target.first_stack_arg();
                                        for kind in kinds.iter().skip(registers) {
                                            let width = kind.stack_width(target);
                                            for k in (0..width).step_by(ptr as usize) {
                                                let e = info.argument_reads.entry(offset + k).or_insert(0);
                                                *e = (*e).max((ptr * 8) as u16);
                                            }
                                            offset += width;
                                        }
                                    }
                                }
                                _ => {
                                    // The amounts a callee that can take the arguments here pops:
                                    // the bytes it pops hold none of the entry values saved. A
                                    // callee no one knows (a slot a runtime class's vtable fills
                                    // too, a function pointer) pops nothing when the word above
                                    // the return address is such a save.
                                    let unknown = regs[sp].filter(|&k| saves.bytes.contains(&(k + pushed))).map(|_| 0);
                                    let pops = match call_pops(inst) {
                                        Some(callees) => {
                                            let possible: Vec<CalleeStack> = callees
                                                .into_iter()
                                                .filter(|&CalleeStack { pops, .. }| {
                                                    pops.is_none_or(|pops| {
                                                        regs[sp].is_none_or(|k| {
                                                            !(k + pushed..k + pushed + pops).any(|b| saves.bytes.contains(&b))
                                                        })
                                                    })
                                                })
                                                .collect();
                                            // The arguments in the function's own argument area,
                                            // above its return address.
                                            if let Some(k) = regs[sp] {
                                                let bytes = possible.iter().map(|c| c.arguments).max().unwrap_or(0);
                                                for at in ((k + first).max(first)..k + first + bytes).step_by(ptr as usize) {
                                                    touch(&mut info, at, (ptr * 8) as u16);
                                                    let e = info.argument_reads.entry(at).or_insert(0);
                                                    *e = (*e).max((ptr * 8) as u16);
                                                }
                                            }
                                            match possible.iter().map(|c| c.pops).collect::<Option<Vec<i64>>>() {
                                                Some(all) => all.first().copied().filter(|&pops| all.iter().all(|&p| p == pops)),
                                                None => unknown,
                                            }
                                        }
                                        None => unknown,
                                    };
                                    match pops {
                                        Some(pops) => regs[sp] = regs[sp].map(|k| k + pushed + pops),
                                        None if target.is_64() => regs[sp] = regs[sp].map(|k| k + pushed),
                                        None => {
                                            regs[sp] = None;
                                            info.dynamic = true;
                                        }
                                    }
                                }
                            }
                            // The callee's own frame lies below the stack pointer it returns with.
                            if let Some(top) = regs[sp] {
                                saves.bytes.retain(|&b| b >= top);
                                saves.slots.retain(|&at, _| at >= top);
                            } else {
                                saves.bytes.clear();
                                saves.slots.clear();
                            }
                        }
                    }
                }
            }
            if exit[bi] != Some(regs) {
                exit[bi] = Some(regs);
                changed = true;
            }
            if saves_exit[bi].as_ref() != Some(&saves) {
                saves_exit[bi] = Some(saves);
                changed = true;
            }
        }
        if !changed {
            settled = true;
            break;
        }
    }
    // What every return leaves as the entry had it. A round that still changed something left
    // the blocks' states half joined, which proves nothing.
    if settled {
        info.kept = intact;
        for &bi in &order {
            if let Terminator::Ret(_) = f.blocks[bi].terminator {
                info.kept &= saves_exit[bi].as_ref().map_or(0, |saves| saves.kept);
            }
        }
    }
    // The lowest stack pointer value itself counts: a push writes below it. The offset at a
    // return, past the return address, is what the function pops.
    for &bi in &order {
        if let Some(Some(k)) = exit[bi].map(|regs| regs[sp]) {
            info.min_offset = info.min_offset.min(k);
            if let Terminator::Ret(_) = f.blocks[bi].terminator {
                info.pops = info.pops.max(k - pushed);
            }
        }
    }
    // What the signature says of every return: the return address popped, and on x86 the bytes
    // of `ret N`.
    let at_return = pushed + if target.is_64() { 0 } else { info.pops.max(0) };
    let mut assumed = |kind: assume::Kind, site: Option<u64>| {
        info.assumptions.push(Assumption { function: f.name.clone(), step: Step::Stack, kind, site, discharged: None });
    };
    for (bi, block) in f.blocks.iter().enumerate() {
        let Some(regs) = exit[bi] else { continue };
        let mut depths = preds.get(&block.id).into_iter().flatten().filter_map(|p| exit[index_of[p]]).filter_map(|regs| regs[sp]);
        if bi != 0
            && let Some(first) = depths.next()
            && depths.any(|k| k != first)
        {
            assumed(assume::Kind::BalancedStack, block.address.or_else(|| block.instrs.iter().find_map(|i| i.address)));
        }
        for (ii, i) in block.instrs.iter().enumerate() {
            if frame_pointers.get(&(bi, ii)).copied().unwrap_or(false) {
                assumed(assume::Kind::FramePointer, i.address);
            }
        }
        if matches!(block.terminator, Terminator::Ret(_)) && regs[sp] != Some(at_return) {
            assumed(assume::Kind::BalancedStack, block.instrs.iter().rev().find_map(|i| i.address).or(block.address));
        }
    }
    info
}

/// The offset from the entry stack pointer each general register holds at a point, `None` for a
/// register that holds something else or an offset the paths disagree on.
type Regs = [Option<i64>; GPRS];

fn join(a: Regs, b: Regs) -> Regs {
    let mut out = [None; GPRS];
    for (r, offset) in out.iter_mut().enumerate() {
        if a[r] == b[r] {
            *offset = a[r];
        }
    }
    out
}

/// The callee-saved registers that still hold their value from the entry, as a mask by number,
/// and the stack bytes that hold a copy of one: what the prologue saved. Beside them, the
/// registers that hold that value now, intact or reloaded from their save (`kept`), and each
/// save by its offset with the register it is of (`slots`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Saves {
    intact: u64,
    kept: u64,
    bytes: BTreeSet<i64>,
    slots: BTreeMap<i64, u8>,
}

impl Saves {
    fn join(&self, other: &Self) -> Self {
        Self {
            intact: self.intact & other.intact,
            kept: self.kept & other.kept,
            bytes: self.bytes.intersection(&other.bytes).copied().collect(),
            slots: self.slots.iter().filter(|&(at, r)| other.slots.get(at) == Some(r)).map(|(&at, &r)| (at, r)).collect(),
        }
    }
}

fn touch(info: &mut StackInfo, offset: i64, bits: u16) {
    info.min_offset = info.min_offset.min(offset);
    info.max_offset = info.max_offset.max(offset + i64::from(bits.div_ceil(8)));
}
