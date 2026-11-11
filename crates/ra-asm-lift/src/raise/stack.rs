//! The stack pointer as a symbol: every value that is `rsp at entry + k` for a constant `k`, over
//! the machine IR of a function. From it: the frame the function uses below its entry stack
//! pointer, the stack slots above it that it reads, which are its stack-passed arguments, and the
//! bytes it pops on return beyond the return address, which an x86 `stdcall` or `thiscall`
//! function does.
//!
//! The analysis is a forward dataflow over the blocks. Its state is the offset of the stack
//! pointer slot (`None` when the function moved it by a run-time amount) and a map from SSA values
//! to offsets. A call to a lifted function moves the offset by the return address the callee pops
//! and by what else it pops; an import wrapper, which runs in place of the native call, moves it
//! by nothing; an indirect x86 call moves it by an amount only the callee knows.

use std::collections::{BTreeMap, HashMap};

use ra_asm_ir::{BinOp, Callee, Cast, Function, Inst, Operand, Terminator, Type, ValueId};

use super::Target;
use super::liveness::{CallKind, GPRS, RSP, Slot, call_kind, reverse_post_order};

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
}

/// Analyzes `f`, whose state pointer is `state`. `pops_of` is what a lifted callee pops beyond its
/// return address, by name, as far as the inference knows it so far.
// The names are the ones the analysis is written in: `f`, `n` blocks, `sp`, a block `b` and an
// instruction `i`.
#[allow(clippy::many_single_char_names)]
pub fn analyze(
    f: &Function,
    state: ValueId,
    pointers: &HashMap<ValueId, Slot>,
    target: Target,
    pops_of: &dyn Fn(&str) -> i64,
) -> StackInfo {
    let n = f.blocks.len();
    let order = reverse_post_order(f);
    let index_of: HashMap<_, _> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let preds = f.predecessors();
    let ptr = target.ptr();
    let Slot::Gpr(sp) = RSP else { unreachable!() };
    let sp = usize::from(sp);
    // The registers at each block's entry: None = not reached yet.
    let mut entry: Vec<Option<Regs>> = vec![None; n];
    let mut exit: Vec<Option<Regs>> = vec![None; n];
    let mut at_entry = [None; GPRS];
    at_entry[sp] = Some(0);
    entry[0] = Some(at_entry);
    let mut info = StackInfo::default();
    let mut symbolic: HashMap<ValueId, i64> = HashMap::new();
    let _ = state;

    for _round in 0..8 {
        let mut changed = false;
        for &bi in &order {
            // Join the predecessors.
            if bi != 0 {
                let mut joined: Option<Regs> = None;
                for p in preds.get(&f.blocks[bi].id).cloned().unwrap_or_default() {
                    let pi = index_of[&p];
                    joined = match (joined, exit[pi]) {
                        (None, x) | (x, None) => x,
                        (Some(a), Some(b)) => Some(join(a, b)),
                    };
                }
                if joined != entry[bi] {
                    entry[bi] = joined;
                    changed = true;
                }
            }
            let Some(mut regs) = entry[bi] else { continue };
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
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), value, .. } if matches!(pointers.get(p), Some(Slot::Gpr(_))) => {
                        let Some(Slot::Gpr(r)) = pointers.get(p) else { unreachable!() };
                        let k = match value {
                            Operand::Local(v) => symbolic.get(v).copied(),
                            _ => None,
                        };
                        regs[usize::from(*r)] = k;
                        if usize::from(*r) == sp && k.is_none() {
                            info.dynamic = true;
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
                    Inst::Cast { cast: Cast::IntToPtr | Cast::Trunc | Cast::ZExt | Cast::SExt, value: Operand::Local(v), .. } => {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(v)) {
                            symbolic.insert(result, k);
                        }
                    }
                    Inst::Load { ty, ptr: Operand::Local(p), .. } => {
                        if let Some(&k) = symbolic.get(p) {
                            let bits = ty.bits().unwrap_or(64) as u16;
                            touch(&mut info, k, bits);
                            if k >= target.first_stack_arg() {
                                let e = info.argument_reads.entry(k).or_insert(0);
                                *e = (*e).max(bits);
                            }
                        }
                    }
                    Inst::Store { ty, ptr: Operand::Local(p), .. } => {
                        if let Some(&k) = symbolic.get(p) {
                            touch(&mut info, k, ty.bits().unwrap_or(64) as u16);
                            if k >= target.first_stack_arg() {
                                info.argument_area_written = true;
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
                                        let registers = if target.is_64() { 4 } else { 0 };
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
                                Some(name) if name.starts_with("F_") => regs[sp] = regs[sp].map(|k| k + ptr + pops_of(name)),
                                _ if target.is_64() => regs[sp] = regs[sp].map(|k| k + ptr),
                                _ => {
                                    regs[sp] = None;
                                    info.dynamic = true;
                                }
                            }
                        }
                    }
                }
            }
            if exit[bi] != Some(regs) {
                exit[bi] = Some(regs);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // The lowest stack pointer value itself counts: a push writes below it. The offset at a
    // return, past the return address, is what the function pops.
    for &bi in &order {
        if let Some(Some(k)) = exit[bi].map(|regs| regs[sp]) {
            info.min_offset = info.min_offset.min(k);
            if let Terminator::Ret(_) = f.blocks[bi].terminator {
                info.pops = info.pops.max(k - ptr);
            }
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

fn touch(info: &mut StackInfo, offset: i64, bits: u16) {
    info.min_offset = info.min_offset.min(offset);
    info.max_offset = info.max_offset.max(offset + i64::from(bits.div_ceil(8)));
}
