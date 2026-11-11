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
use super::liveness::{CallKind, RSP, Slot, call_kind, reverse_post_order};

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
    /// For each call site (the value of the call instruction's position, by block and index), the
    /// stack offset at the call: the caller's outgoing arguments start above it.
    pub call_offsets: HashMap<(usize, usize), i64>,
    /// The bytes the function pops on return beyond the return address: the `N` of `ret N`.
    pub pops: i64,
}

/// Analyzes `f`, whose state pointer is `state`. `pops_of` is what a lifted callee pops beyond its
/// return address, by name, as far as the inference knows it so far.
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
    // The offset of the stack pointer slot at each block's entry: None = unknown, Some(None) = dynamic.
    let mut entry_offset: Vec<Option<Option<i64>>> = vec![None; n];
    let mut exit_offset: Vec<Option<Option<i64>>> = vec![None; n];
    entry_offset[0] = Some(Some(0));
    let mut info = StackInfo::default();
    let mut symbolic: HashMap<ValueId, i64> = HashMap::new();
    let _ = state;

    for _round in 0..8 {
        let mut changed = false;
        for &bi in &order {
            // Join the predecessors.
            if bi != 0 {
                let mut joined: Option<Option<i64>> = None;
                for p in preds.get(&f.blocks[bi].id).cloned().unwrap_or_default() {
                    let pi = index_of[&p];
                    joined = match (joined, exit_offset[pi]) {
                        (None, x) | (x, None) => x,
                        (Some(Some(a)), Some(Some(b))) if a == b => Some(Some(a)),
                        _ => Some(None),
                    };
                }
                if joined != entry_offset[bi] {
                    entry_offset[bi] = joined;
                    changed = true;
                }
            }
            let Some(mut sp) = entry_offset[bi] else { continue };
            for (ii, i) in f.blocks[bi].instrs.iter().enumerate() {
                match &i.inst {
                    Inst::Load { ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&RSP) => {
                        if let (Some(result), Some(k)) = (i.result, sp) {
                            symbolic.insert(result, k);
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), value, .. } if pointers.get(p) == Some(&RSP) => {
                        sp = match value {
                            Operand::Local(v) => symbolic.get(v).copied(),
                            _ => None,
                        };
                        if sp.is_none() {
                            info.dynamic = true;
                        }
                    }
                    Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::Int(_), lhs: Operand::Local(l), rhs: Operand::Int(c), .. } => {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(l)) {
                            let c = *c as i64;
                            let off =
                                if matches!(i.inst, Inst::Bin { op: BinOp::Add, .. }) { k.wrapping_add(c) } else { k.wrapping_sub(c) };
                            symbolic.insert(result, off);
                        }
                    }
                    Inst::Bin { op: BinOp::Add, ty: Type::Int(_), lhs: Operand::Int(c), rhs: Operand::Local(r), .. } => {
                        if let (Some(result), Some(&k)) = (i.result, symbolic.get(r)) {
                            symbolic.insert(result, k.wrapping_add(*c as i64));
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
                        }
                    }
                    inst => {
                        if let Some(CallKind::State) = call_kind(inst) {
                            let callee = match inst {
                                Inst::Call { callee: Callee::Direct(name), .. } => Some(name.as_str()),
                                _ => None,
                            };
                            if let Some(k) = sp {
                                info.call_offsets.insert((bi, ii), k);
                            }
                            // An import wrapper runs in place of the native call: nothing was
                            // pushed and the stack pointer stays. For a lifted callee the machine
                            // model pushed the return address before the call and the callee
                            // popped it, and what else it pops: the stack pointer after the call is
                            // that much above the one the push left. An indirect x86 call pops what
                            // its callee says at run time.
                            match callee {
                                Some(name) if crate::import_of_wrapper(name).is_some() => {}
                                Some(name) if name.starts_with("F_") => sp = sp.map(|k| k + ptr + pops_of(name)),
                                _ if target.is_64() => sp = sp.map(|k| k + ptr),
                                _ => {
                                    sp = None;
                                    info.dynamic = true;
                                }
                            }
                        }
                    }
                }
            }
            if exit_offset[bi] != Some(sp) {
                exit_offset[bi] = Some(sp);
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
        if let Some(Some(k)) = exit_offset[bi] {
            info.min_offset = info.min_offset.min(k);
            if let Terminator::Ret(_) = f.blocks[bi].terminator {
                info.pops = info.pops.max(k - ptr);
            }
        }
    }
    info
}

fn touch(info: &mut StackInfo, offset: i64, bits: u16) {
    info.min_offset = info.min_offset.min(offset);
    info.max_offset = info.max_offset.max(offset + i64::from(bits.div_ceil(8)));
}
