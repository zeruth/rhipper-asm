//! The frame as locals. After the ABI rewrite a function addresses its stack through integer
//! arithmetic on `%rsp0`, the address of its frame's top. This pass follows that arithmetic: a
//! pointer that is `rsp0 + k` for a constant `k` becomes a `getelementptr` from the frame top, and
//! a slot that is only loaded and stored at one type, with no address taken, becomes its own
//! `alloca`. A store of `undef` (a saved register the function never needed) goes.

use std::collections::{BTreeMap, HashMap};

use ra_asm_ir::{BinOp, Cast, Function, Inst, Instruction, Operand, Type, ValueId};

/// How a frame slot is used: the types of its loads and stores, and whether its address escapes.
#[derive(Default)]
struct Use {
    types: Vec<Type>,
    escapes: bool,
}

pub fn run(f: &mut Function) -> bool {
    // The frame top: `%rsp0 = ptrtoint %frame.top`, as the ABI rewrite names them.
    let mut rsp0 = None;
    let mut top = None;
    for i in &f.blocks[0].instrs {
        if let (Some(v), Inst::Cast { cast: Cast::PtrToInt, value: Operand::Local(p), .. }) = (i.result, &i.inst)
            && f.value_name(v) == "rsp0"
        {
            rsp0 = Some(v);
            top = Some(*p);
        }
    }
    let (Some(rsp0), Some(top)) = (rsp0, top) else { return false };

    // Offsets from the frame top, through adds and subs of constants. Phis of equal offsets too.
    let mut offsets: HashMap<ValueId, i64> = HashMap::new();
    offsets.insert(rsp0, 0);
    for _ in 0..4 {
        for b in &f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                let off = match &i.inst {
                    Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Local(l), rhs: Operand::Int(c) } => {
                        offsets.get(l).map(|k| k + *c as i64)
                    }
                    Inst::Bin { op: BinOp::Sub, ty: Type::I64, lhs: Operand::Local(l), rhs: Operand::Int(c) } => {
                        offsets.get(l).map(|k| k - *c as i64)
                    }
                    Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Int(c), rhs: Operand::Local(r) } => {
                        offsets.get(r).map(|k| k + *c as i64)
                    }
                    Inst::Cast { cast: Cast::IntToPtr, value: Operand::Local(p), .. } => offsets.get(p).copied(),
                    Inst::Phi { incoming, .. } => {
                        let ks: Vec<Option<i64>> = incoming.iter().map(|(o, _)| o.local().and_then(|l| offsets.get(&l).copied())).collect();
                        match ks.first().copied().flatten() {
                            Some(k) if ks.iter().all(|x| *x == Some(k)) => Some(k),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(k) = off {
                    offsets.insert(v, k);
                }
            }
        }
    }

    // How each slot is used: by the pointers (inttoptr results) at a known offset.
    let mut slots: BTreeMap<i64, Use> = BTreeMap::new();
    let mut pointer_offsets: HashMap<ValueId, i64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(v), Inst::Cast { cast: Cast::IntToPtr, .. }) = (i.result, &i.inst)
                && let Some(&k) = offsets.get(&v)
            {
                pointer_offsets.insert(v, k);
                slots.entry(k).or_default();
            }
        }
    }
    // An integer offset used by anything but the arithmetic above or an inttoptr escapes: the
    // address goes somewhere (an argument, a store, a comparison). An address that flows into
    // arithmetic the tracking cannot follow (a phi of different offsets, an add of two values) is
    // a dynamic pointer into the frame: then no slot is private, since it may reach any of them.
    let mut dynamic = false;
    for b in &f.blocks {
        for i in &b.instrs {
            let consumes_address = |o: &Operand| o.local().and_then(|l| offsets.get(&l).copied());
            let tracked = i.result.is_some_and(|v| offsets.contains_key(&v));
            match &i.inst {
                Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::I64, lhs, rhs }
                    if matches!((lhs, rhs), (Operand::Local(_), Operand::Int(_)) | (Operand::Int(_), Operand::Local(_))) => {}
                Inst::Cast { cast: Cast::IntToPtr, .. } => {}
                Inst::Phi { incoming, .. } => {
                    if !tracked && incoming.iter().any(|(o, _)| consumes_address(o).is_some()) {
                        dynamic = true;
                    }
                }
                Inst::Bin { lhs, rhs, .. } if consumes_address(lhs).is_some() || consumes_address(rhs).is_some() => dynamic = true,
                Inst::ICmp { lhs, rhs, .. } if consumes_address(lhs).is_some() || consumes_address(rhs).is_some() => dynamic = true,
                Inst::Select { then, els, .. } if consumes_address(then).is_some() || consumes_address(els).is_some() => dynamic = true,
                Inst::Load { ptr, ty, .. } => {
                    if let Some(k) = ptr.local().and_then(|p| pointer_offsets.get(&p).copied()) {
                        slots.entry(k).or_default().types.push(ty.clone());
                    }
                }
                Inst::Store { ptr, value, ty, .. } => {
                    if let Some(k) = ptr.local().and_then(|p| pointer_offsets.get(&p).copied()) {
                        slots.entry(k).or_default().types.push(ty.clone());
                    }
                    if let Some(k) = consumes_address(value) {
                        slots.entry(k).or_default().escapes = true;
                    }
                }
                inst => {
                    let mut probe = inst.clone();
                    for o in probe.operands_mut() {
                        if let Some(k) = consumes_address(o) {
                            slots.entry(k).or_default().escapes = true;
                        }
                    }
                }
            }
        }
        let mut t = b.terminator.clone();
        for o in t.operands_mut() {
            if let Some(k) = o.local().and_then(|l| offsets.get(&l).copied()) {
                slots.entry(k).or_default().escapes = true;
            }
        }
    }
    // A slot whose bytes another slot's access covers is shared: keep both in the frame. A slot
    // at or above an address that escaped is reachable through it (an array or a structure passed
    // by pointer): keep those in the frame too.
    let mut sizes: Vec<(i64, i64)> = Vec::new();
    for (k, u) in &slots {
        let size = u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64;
        sizes.push((*k, size));
    }
    let overlaps = |k: i64, size: i64| sizes.iter().any(|&(o, s)| o != k && k < o + s && o < k + size);
    let lowest_escape = slots.iter().filter(|(_, u)| u.escapes).map(|(k, _)| *k).min();
    let reachable = |k: i64| lowest_escape.is_some_and(|e| k >= e);

    // The rewrite: each pointer at a known offset becomes a gep from the frame top, or a load/store
    // of a slot's own alloca when the slot is private and single-typed.
    let mut allocas: BTreeMap<i64, (ValueId, Type)> = BTreeMap::new();
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let mut changed = false;
    let mut new_entry: Vec<Instruction> = Vec::new();
    for (k, u) in &slots {
        let size = u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64;
        let single = u.types.first().is_some_and(|t| u.types.iter().all(|x| x == t));
        if single && !dynamic && !u.escapes && !reachable(*k) && !overlaps(*k, size) {
            let ty = u.types[0].clone();
            let v = f.new_value(&format!("local_{}", offset_name(*k)), Type::Ptr);
            new_entry.push(Instruction {
                result: Some(v),
                inst: Inst::Alloca { ty: ty.clone(), align: ty.size(8).map_or(8, |s| s.clamp(1, 16) as u32) },
                address: None,
            });
            allocas.insert(*k, (v, ty));
        }
    }
    for b in &mut f.blocks {
        let mut out = Vec::with_capacity(b.instrs.len());
        for i in b.instrs.drain(..) {
            match (&i.inst, i.result) {
                (Inst::Cast { cast: Cast::IntToPtr, .. }, Some(v)) if pointer_offsets.contains_key(&v) => {
                    let k = pointer_offsets[&v];
                    changed = true;
                    if let Some((a, _)) = allocas.get(&k) {
                        replace.insert(v, Operand::Local(*a));
                    } else {
                        out.push(Instruction {
                            result: Some(v),
                            inst: Inst::Gep {
                                base_ty: Type::I8,
                                ptr: Operand::Local(top),
                                indices: vec![(Type::I64, Operand::Int(i128::from(k)))],
                                inbounds: true,
                            },
                            address: i.address,
                        });
                    }
                }
                (Inst::Store { value: Operand::Undef, .. }, _) => changed = true,
                _ => out.push(i),
            }
        }
        b.instrs = out;
    }
    if !new_entry.is_empty() {
        // After the frame's own prologue instructions, so the allocas stay in the entry block.
        let at = f.blocks[0].instrs.iter().position(|i| i.result == Some(rsp0)).map_or(0, |p| p + 1);
        let tail = f.blocks[0].instrs.split_off(at);
        f.blocks[0].instrs.extend(new_entry);
        f.blocks[0].instrs.extend(tail);
    }
    f.replace_uses(&replace);
    changed
}

fn offset_name(k: i64) -> String {
    if k < 0 { format!("m{:x}", -k) } else { format!("p{k:x}") }
}
