//! The frame as locals. After the ABI rewrite a function addresses its stack through integer
//! arithmetic on `%rsp0`, the address of its frame's top. This pass follows that arithmetic: a
//! pointer that is `rsp0 + k` for a constant `k` becomes a `getelementptr` from the frame top, and
//! a slot that is only loaded and stored at one type, with no address taken, becomes its own
//! `alloca`, and one whose address escapes too when it is bounded: the next slot starts where it
//! ends. A store of `undef` (a saved register the function never needed) goes. The locals then
//! become values ([`super::locals`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use ra_asm_ir::{BinOp, Cast, Function, Inst, Instruction, Operand, Type, ValueId};

use super::assume::{self, Assumption, Kind, Step};

/// How a frame slot is used: the types of its loads and stores, and whether its address escapes.
#[derive(Default)]
struct Use {
    types: Vec<Type>,
    escapes: bool,
    /// A load of it something reads, or a store of a value to it: more than a register saved or
    /// pushed to make room, whose value is `undef` and whose restore nothing reads.
    held: bool,
    /// A load of it something reads.
    read: bool,
    /// Its address escapes other than as the argument block of a variadic call.
    escapes_otherwise: bool,
}

pub fn run(f: &mut Function) -> bool {
    run_assuming(f).0
}

/// The bytes a slot's access covers: its widest type, 8 when it has none.
fn size_of(u: &Use) -> i64 {
    u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64
}

/// Where the argument block that starts at slot `k` ends: up through the slots right above it the
/// function writes and never reads. A slot inside what the block already covers (a dword another
/// call pushed into a double's bytes) goes on with it; a gap, a slot read or whose address goes
/// elsewhere, or one the function holds nothing in (a saved register) ends it. Stopping at the
/// covered slot cut a 24-byte `Con_Printf(fmt, n, time, fps)` block after its first double, and
/// the callee read the second from the next block's bytes.
fn block_end(slots: &BTreeMap<i64, Use>, k: i64, u: &Use) -> i64 {
    let mut end = k + size_of(u);
    for (n, above) in slots.range(k + 1..) {
        if *n > end || above.read || above.escapes_otherwise || above.types.is_empty() || (*n == end && !above.held) {
            break;
        }
        end = end.max(n + size_of(above));
    }
    end
}

/// [`run`], with what the pass assumed beside whether it changed `f`: for each slot it made a
/// local of its own, at the address of the slot's first use, that no pointer to another slot
/// reaches it. The slots at or above an address that escapes stay in the frame, so the pointers
/// assumed of are the ones that would have to run down the frame, or come from outside it.
pub fn run_assuming(f: &mut Function) -> (bool, Vec<Assumption>) {
    let mut assumptions = Vec::new();
    let mut changed = rewrite(f, &mut assumptions);
    // The locals the rewrite made, loaded and stored and no more, as values.
    changed |= super::locals::run(f);
    assume::sort(&mut assumptions);
    (changed, assumptions)
}

fn rewrite(f: &mut Function, assumptions: &mut Vec<Assumption>) -> bool {
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
    let mut changed = false;

    // A variadic x86 function (`varargs` is the address of its caller's argument block): what the
    // code derives from the entry stack pointer at or past the first stack argument is rebased
    // onto the block, so a `va_list` walks the caller's arguments and not this frame.
    if let Some(args) = f.params.iter().copied().find(|v| f.value_name(*v) == "varargs") {
        let offsets = track(f, rsp0);
        changed |= rebase_arguments(f, &offsets, args, 4);
    }
    let offsets = track(f, rsp0);

    // How each slot is used: by the pointers at a known offset, the inttoptr results and the
    // prologue's own geps from the frame top (the stack parameter slots).
    let mut slots: BTreeMap<i64, Use> = BTreeMap::new();
    let mut pointer_offsets: HashMap<ValueId, i64> = HashMap::new();
    // Where the code first names each slot: the prologue's own pointers have no address.
    let mut first_use: HashMap<i64, u64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            let Some(v) = i.result else { continue };
            let k = match &i.inst {
                Inst::Cast { cast: Cast::IntToPtr, .. } => offsets.get(&v).copied(),
                Inst::Gep { base_ty: Type::I8, ptr: Operand::Local(p), indices, .. } if *p == top => match indices.as_slice() {
                    [(_, Operand::Int(k))] => Some(*k as i64),
                    _ => None,
                },
                _ => None,
            };
            let Some(k) = k else { continue };
            pointer_offsets.insert(v, k);
            slots.entry(k).or_default();
            if let Some(address) = i.address {
                first_use.entry(k).or_insert(address);
            }
        }
    }
    // An integer offset used by anything but the arithmetic above or an inttoptr escapes: the
    // address goes somewhere (an argument, a store, a comparison). An address that flows into
    // arithmetic the tracking cannot follow (a phi of different offsets, an add of two values) is
    // a dynamic pointer into the frame: then no slot is private, since it may reach any of them.
    // The values a store of which is no value: `undef` cast to a width (a register pushed to
    // make room or saved, whose value the lift does not know).
    let mut undefs: HashSet<ValueId> = HashSet::new();
    // The values something reads.
    let mut used: HashSet<ValueId> = HashSet::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(v), Inst::Cast { value: Operand::Undef, .. }) = (i.result, &i.inst) {
                undefs.insert(v);
            }
            used.extend(i.inst.uses());
        }
        used.extend(b.terminator.uses());
    }
    let is_undef = |o: &Operand| matches!(o, Operand::Undef) || o.local().is_some_and(|l| undefs.contains(&l));
    let mut dynamic = false;
    for b in &f.blocks {
        for i in &b.instrs {
            let consumes_address = |o: &Operand| o.local().and_then(|l| offsets.get(&l).copied());
            let tracked = i.result.is_some_and(|v| offsets.contains_key(&v));
            match &i.inst {
                Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::Int(_), lhs, rhs }
                    if matches!((lhs, rhs), (Operand::Local(_), Operand::Int(_)) | (Operand::Int(_), Operand::Local(_))) => {}
                Inst::Cast { cast: Cast::IntToPtr | Cast::Trunc | Cast::ZExt | Cast::SExt, .. } => {}
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
                        let slot = slots.entry(k).or_default();
                        slot.types.push(ty.clone());
                        slot.held |= i.result.is_some_and(|v| used.contains(&v));
                        // Not the reads the ABI rewrite makes of a call's stack arguments (`sarg0`,
                        // `stack.arg0`): the outgoing area serves every call, and one call's
                        // arguments are another's block.
                        slot.read |= i.result.is_some_and(|v| {
                            let name = f.value_name(v);
                            used.contains(&v) && !name.starts_with("sarg") && !name.starts_with("stack.arg")
                        });
                    }
                }
                Inst::Store { ptr, value, ty, .. } => {
                    if let Some(k) = ptr.local().and_then(|p| pointer_offsets.get(&p).copied()) {
                        let slot = slots.entry(k).or_default();
                        slot.types.push(ty.clone());
                        slot.held |= !is_undef(value);
                    }
                    if let Some(k) = consumes_address(value) {
                        let slot = slots.entry(k).or_default();
                        slot.escapes = true;
                        slot.escapes_otherwise = true;
                    }
                }
                inst => {
                    // The address of the argument block a variadic callee reads, which the ABI
                    // rewrite names `varargs` as it makes the call.
                    let block = matches!(inst, Inst::Cast { cast: Cast::PtrToInt, .. })
                        && i.result.is_some_and(|v| f.value_name(v).starts_with("varargs"));
                    let mut probe = inst.clone();
                    for o in probe.operands_mut() {
                        if let Some(k) = consumes_address(o) {
                            let slot = slots.entry(k).or_default();
                            slot.escapes = true;
                            slot.escapes_otherwise |= !block;
                        }
                    }
                }
            }
        }
        let mut t = b.terminator.clone();
        for o in t.operands_mut() {
            if let Some(k) = o.local().and_then(|l| offsets.get(&l).copied()) {
                let slot = slots.entry(k).or_default();
                slot.escapes = true;
                slot.escapes_otherwise = true;
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
    let single_typed = |u: &Use| u.types.first().is_some_and(|t| u.types.iter().all(|x| x == t));
    let size_of = |u: &Use| u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64;

    // The argument blocks of variadic calls (`error(fmt, x)`, `sscanf(s, fmt, &v)`): from a slot
    // whose address goes only to such a call as its block, up through the slots right above it
    // that the function writes and never reads, the arguments it pushed. Each block, merged with
    // those it overlaps (the outgoing area serves call after call), is an array of its own: the
    // callee reads what was pushed for it, not the frame above, whose slots are then the
    // function's own to make locals of.
    let mut blocks: Vec<(i64, i64)> = Vec::new();
    if !dynamic {
        for (k, u) in slots.iter().filter(|(_, u)| u.escapes && !u.escapes_otherwise && !u.read && !u.types.is_empty()) {
            blocks.push((*k, block_end(&slots, *k, u)));
        }
        blocks.sort();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (s, e) in blocks {
            match merged.last_mut() {
                Some((_, last)) if s < *last => *last = (*last).max(e),
                _ => merged.push((s, e)),
            }
        }
        // A slot only partly inside a block (an access across its edge) leaves it in the frame,
        // and so does one inside it that the function reads or whose address goes elsewhere (a
        // byte local within a pushed dword's slot, whose integer address a callee writes through).
        blocks = merged
            .into_iter()
            .filter(|&(s, e)| {
                !slots.iter().any(|(k, u)| {
                    (*k < s && k + size_of(u) > s)
                        || (*k < e && k + size_of(u) > e)
                        || ((s..e).contains(k) && (u.read || u.escapes_otherwise))
                })
            })
            .collect();
    }
    let in_block = |k: i64| blocks.iter().copied().find(|&(s, e)| s <= k && k < e);

    // An escaping slot that is one variable of its own (`int size; get_base(&size)`, MSVC's
    // `lea eax, [ebp-4]`): accessed at one type, with the next slot of the frame (a saved
    // register, another local) or the frame's top right where it ends, so the callee its address
    // goes to has no room past it to read; and no other escape below it, whose array or structure
    // may cover it. It gets its own local, whose address goes where the slot's went. The code's
    // own arithmetic on addresses (the stack pointer passing through the slot's offset on its way
    // down) needs nothing: every pointer it makes at a known offset is rewritten all the same.
    let mut own: HashSet<i64> = HashSet::new();
    if !dynamic {
        for (k, u) in slots.iter().filter(|(k, u)| u.escapes && in_block(**k).is_none()) {
            let size = u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64;
            let end = k + size;
            // What lies right above it is the frame's top, or a slot the function holds nothing
            // in (a saved register): a slot above that holds a value (the next argument of a
            // block a variadic callee reads, the next field of a structure) may be the callee's
            // to read through the address too.
            let bounded =
                end <= 0 && slots.range(k + 1..).next().map_or(end == 0, |(n, above)| (*n == end && !above.held) || (end == 0 && *n >= 0));
            if !(single_typed(u) && bounded && !overlaps(*k, size)) {
                // The lowest escape that is no variable of its own: the slots above it stay.
                break;
            }
            own.insert(*k);
        }
    }
    let lowest_escape = slots.iter().filter(|(k, u)| u.escapes && !own.contains(k) && in_block(**k).is_none()).map(|(k, _)| *k).min();
    let reachable = |k: i64| lowest_escape.is_some_and(|e| k >= e);

    // The rewrite: each pointer at a known offset becomes a gep from the frame top, or a load/store
    // of a slot's own alloca when the slot is private and single-typed.
    let mut allocas: BTreeMap<i64, (ValueId, Type)> = BTreeMap::new();
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let mut new_entry: Vec<Instruction> = Vec::new();
    for (k, u) in &slots {
        let size = u.types.iter().filter_map(|t| t.size(8)).max().unwrap_or(8) as i64;
        if single_typed(u) && !dynamic && in_block(*k).is_none() && (!u.escapes || own.contains(k)) && !reachable(*k) && !overlaps(*k, size)
        {
            let ty = u.types[0].clone();
            let v = f.new_value(&format!("local_{}", offset_name(*k)), Type::Ptr);
            new_entry.push(Instruction {
                result: Some(v),
                inst: Inst::Alloca { ty: ty.clone(), align: ty.size(8).map_or(8, |s| s.clamp(1, 16) as u32) },
                address: None,
            });
            allocas.insert(*k, (v, ty));
            assumptions.push(Assumption {
                function: f.name.clone(),
                step: Step::Frame,
                kind: Kind::PrivateLocal,
                site: first_use.get(k).copied(),
                discharged: None,
            });
        }
    }
    // An array per argument block, its pointers rebased onto it.
    let mut block_allocas: HashMap<i64, ValueId> = HashMap::new();
    for &(s, e) in &blocks {
        let v = f.new_value(&format!("va_{}", offset_name(s)), Type::Ptr);
        new_entry.push(Instruction {
            result: Some(v),
            inst: Inst::Alloca { ty: Type::Array(u64::try_from(e - s).unwrap_or(0), Box::new(Type::I8)), align: 16 },
            address: None,
        });
        block_allocas.insert(s, v);
        assumptions.push(Assumption {
            function: f.name.clone(),
            step: Step::Frame,
            kind: Kind::PrivateLocal,
            site: first_use.get(&s).copied(),
            discharged: None,
        });
    }
    for b in &mut f.blocks {
        let mut out = Vec::with_capacity(b.instrs.len());
        for i in b.instrs.drain(..) {
            match (&i.inst, i.result) {
                (Inst::Cast { cast: Cast::IntToPtr, .. } | Inst::Gep { .. }, Some(v))
                    if pointer_offsets.get(&v).is_some_and(|&k| in_block(k).is_some()) =>
                {
                    let k = pointer_offsets[&v];
                    let (s, _) = in_block(k).expect("a block's pointer");
                    changed = true;
                    out.push(Instruction {
                        result: Some(v),
                        inst: Inst::Gep {
                            base_ty: Type::I8,
                            ptr: Operand::Local(block_allocas[&s]),
                            indices: vec![(Type::I64, Operand::Int(i128::from(k - s)))],
                            inbounds: true,
                        },
                        address: i.address,
                    });
                }
                (Inst::Cast { cast: Cast::IntToPtr, .. } | Inst::Gep { .. }, Some(v)) if pointer_offsets.contains_key(&v) => {
                    let k = pointer_offsets[&v];
                    if let Some((a, _)) = allocas.get(&k) {
                        changed = true;
                        replace.insert(v, Operand::Local(*a));
                    } else if matches!(i.inst, Inst::Gep { .. }) {
                        out.push(i);
                    } else {
                        changed = true;
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
                (Inst::Store { value, .. }, _) if is_undef(value) => changed = true,
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
    // Where the integer address of an escaping slot that got its own local escapes (a call's
    // argument, a stored value, a returned one), its local's address goes, at the width the code
    // had it (the low dword in the 32-bit model).
    let own_slot = |l: ValueId| offsets.get(&l).and_then(|k| allocas.get(k).filter(|_| own.contains(k))).map(|(a, _)| *a);
    if !own.is_empty() {
        for bi in 0..f.blocks.len() {
            let instrs = std::mem::take(&mut f.blocks[bi].instrs);
            let mut out = Vec::with_capacity(instrs.len());
            for mut i in instrs {
                let arithmetic = matches!(
                    &i.inst,
                    Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::Int(_), .. }
                        | Inst::Cast { cast: Cast::IntToPtr | Cast::Trunc | Cast::ZExt | Cast::SExt, .. }
                        // A phi of addresses at one offset is one: its own uses are replaced.
                        | Inst::Phi { .. }
                );
                if !arithmetic {
                    let address = i.address;
                    let operands: Vec<&mut Operand> = match &mut i.inst {
                        Inst::Store { value, .. } => vec![value],
                        inst => inst.operands_mut(),
                    };
                    for o in operands {
                        changed |= own_address(f, o, &own_slot, &mut out, address);
                    }
                }
                out.push(i);
            }
            let mut t = std::mem::replace(&mut f.blocks[bi].terminator, ra_asm_ir::Terminator::Unreachable);
            for o in t.operands_mut() {
                changed |= own_address(f, o, &own_slot, &mut out, None);
            }
            f.blocks[bi].terminator = t;
            f.blocks[bi].instrs = out;
        }
    }
    changed
}

/// `o` as its local's address when it is the integer address of an escaping slot that got a local
/// of its own (`own_slot`): the address computed into `out` ahead of its use. Whether it was.
fn own_address(
    f: &mut Function,
    o: &mut Operand,
    own_slot: &impl Fn(ValueId) -> Option<ValueId>,
    out: &mut Vec<Instruction>,
    address: Option<u64>,
) -> bool {
    let Some(l) = o.local() else { return false };
    let Type::Int(bits) = *f.value_type(l) else { return false };
    let Some(local) = own_slot(l) else { return false };
    let wide = f.new_value("addr.w", Type::I64);
    out.push(Instruction {
        result: Some(wide),
        inst: Inst::Cast { cast: Cast::PtrToInt, from: Type::Ptr, value: Operand::Local(local), to: Type::I64 },
        address,
    });
    if bits == 64 {
        *o = Operand::Local(wide);
    } else {
        let narrow = f.new_value("addr", Type::Int(bits));
        out.push(Instruction {
            result: Some(narrow),
            inst: Inst::Cast { cast: Cast::Trunc, from: Type::I64, value: Operand::Local(wide), to: Type::Int(bits) },
            address,
        });
        *o = Operand::Local(narrow);
    }
    true
}

/// Offsets from the frame top, through adds and subs of constants. Phis of equal offsets too,
/// taken optimistically: a phi whose tracked incomings agree gets their offset (the stack pointer
/// round a loop depends on itself), and one that an incoming contradicts in the end is dropped,
/// with what derived from it, by a fresh pass.
fn track(f: &Function, rsp0: ValueId) -> HashMap<ValueId, i64> {
    let mut dropped: HashSet<ValueId> = HashSet::new();
    loop {
        let offsets = track_with(f, rsp0, &dropped);
        let mut contradicted = Vec::new();
        for b in &f.blocks {
            for i in &b.instrs {
                if let (Some(v), Inst::Phi { incoming, .. }) = (i.result, &i.inst)
                    && let Some(&k) = offsets.get(&v)
                    && incoming.iter().any(|(o, _)| o.local().and_then(|l| offsets.get(&l).copied()) != Some(k))
                {
                    contradicted.push(v);
                }
            }
        }
        if contradicted.is_empty() {
            return offsets;
        }
        dropped.extend(contradicted);
    }
}

fn track_with(f: &Function, rsp0: ValueId, dropped: &HashSet<ValueId>) -> HashMap<ValueId, i64> {
    let mut offsets: HashMap<ValueId, i64> = HashMap::new();
    offsets.insert(rsp0, 0);
    loop {
        let mut changed = false;
        for b in &f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                if offsets.contains_key(&v) {
                    continue;
                }
                let off = match &i.inst {
                    Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs: Operand::Local(l), rhs: Operand::Int(c) } => {
                        offsets.get(l).map(|k| k + signed(*c, *bits))
                    }
                    Inst::Bin { op: BinOp::Sub, ty: Type::Int(bits), lhs: Operand::Local(l), rhs: Operand::Int(c) } => {
                        offsets.get(l).map(|k| k - signed(*c, *bits))
                    }
                    Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs: Operand::Int(c), rhs: Operand::Local(r) } => {
                        offsets.get(r).map(|k| k + signed(*c, *bits))
                    }
                    // The 32-bit model narrows the address to a dword and widens it back.
                    Inst::Cast { cast: Cast::IntToPtr | Cast::Trunc | Cast::ZExt | Cast::SExt, value: Operand::Local(p), .. } => {
                        offsets.get(p).copied()
                    }
                    Inst::Phi { incoming, .. } if !dropped.contains(&v) => {
                        let ks: Vec<i64> = incoming.iter().filter_map(|(o, _)| o.local().and_then(|l| offsets.get(&l).copied())).collect();
                        match ks.first() {
                            Some(&k) if ks.iter().all(|x| *x == k) => Some(k),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(k) = off {
                    offsets.insert(v, k);
                    changed = true;
                }
            }
        }
        if !changed {
            return offsets;
        }
    }
}

/// Redefines every value that is `rsp0 + k` with `k >= first` as `args + (k - first)`, at the
/// value's own type: an integer of either width, or a pointer. A phi of such values keeps its
/// form, its incoming values being rebased.
fn rebase_arguments(f: &mut Function, offsets: &HashMap<ValueId, i64>, args: ValueId, first: i64) -> bool {
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        let instrs = std::mem::take(&mut f.blocks[bi].instrs);
        let mut out = Vec::with_capacity(instrs.len());
        for i in instrs {
            let rebased = i.result.and_then(|v| offsets.get(&v)).copied().filter(|&k| k >= first && !matches!(i.inst, Inst::Phi { .. }));
            let (Some(v), Some(k)) = (i.result, rebased) else {
                out.push(i);
                continue;
            };
            let delta = Operand::Int(i128::from(k - first));
            match i.inst.result_type() {
                Type::Ptr => {
                    let p = f.new_value("args.p", Type::Ptr);
                    let cast = Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: Operand::Local(args), to: Type::Ptr };
                    out.push(Instruction { result: Some(p), inst: cast, address: i.address });
                    let gep = Inst::Gep { base_ty: Type::I8, ptr: Operand::Local(p), indices: vec![(Type::I64, delta)], inbounds: true };
                    out.push(Instruction { result: Some(v), inst: gep, address: i.address });
                }
                Type::Int(64) => {
                    let add = Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Local(args), rhs: delta };
                    out.push(Instruction { result: Some(v), inst: add, address: i.address });
                }
                Type::Int(bits) => {
                    let narrow = f.new_value("args.n", Type::Int(bits));
                    let cast = Inst::Cast { cast: Cast::Trunc, from: Type::I64, value: Operand::Local(args), to: Type::Int(bits) };
                    out.push(Instruction { result: Some(narrow), inst: cast, address: i.address });
                    let add = Inst::Bin { op: BinOp::Add, ty: Type::Int(bits), lhs: Operand::Local(narrow), rhs: delta };
                    out.push(Instruction { result: Some(v), inst: add, address: i.address });
                }
                _ => {
                    out.push(i);
                    continue;
                }
            }
            changed = true;
        }
        f.blocks[bi].instrs = out;
    }
    changed
}

/// A constant of a `bits`-wide integer operation as a signed number: a 32-bit `-16` may be held
/// as `4294967280`.
pub fn signed(c: i128, bits: u16) -> i64 {
    if bits >= 64 {
        return c as i64;
    }
    let mask = (1i128 << bits) - 1;
    let v = c & mask;
    if v >> (bits - 1) & 1 == 1 { (v - (1i128 << bits)) as i64 } else { v as i64 }
}

fn offset_name(k: i64) -> String {
    if k < 0 { format!("m{:x}", -k) } else { format!("p{k:x}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(types: Vec<Type>, escapes: bool) -> Use {
        Use { types, escapes, held: true, read: false, escapes_otherwise: false }
    }

    /// `Con_Printf(fmt, frames, time, fps)` at -28 with `printf(fmt2, frames)` at -12 through the
    /// same outgoing area, as MSVC lays it out: the dword at -16 lies inside `time`'s double, and
    /// the block runs on through `fps` to the saved register at -4.
    #[test]
    fn an_argument_block_runs_over_a_slot_inside_a_double_to_its_last_argument() {
        let mut slots = BTreeMap::new();
        slots.insert(-28, held(vec![Type::I32], true));
        slots.insert(-24, held(vec![Type::I32], false));
        slots.insert(-20, held(vec![Type::Double, Type::I32], false));
        slots.insert(-16, held(vec![Type::I32], false));
        slots.insert(-12, held(vec![Type::Double, Type::I32], true));
        slots.insert(-8, held(vec![Type::I32], false));
        slots.insert(-4, Use { types: vec![Type::I32], ..Use::default() });
        assert_eq!(block_end(&slots, -28, &slots[&-28]), -4);
        // a gap ends it, and so does a slot the function reads
        slots.remove(&-16);
        slots.remove(&-12);
        slots.remove(&-8);
        assert_eq!(block_end(&slots, -28, &slots[&-28]), -12);
        slots.insert(-12, Use { types: vec![Type::Double], held: true, read: true, ..Use::default() });
        assert_eq!(block_end(&slots, -28, &slots[&-28]), -12);
    }
}
