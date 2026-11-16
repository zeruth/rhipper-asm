//! The IR tidied for the C printer, a function at a time, before it is written: what the raise
//! leaves for the machine's sake and the source reads better without. The meaning stays exact.
//!
//! - The exception check after a call (`ra_eh_pending` and the branch to the landing) goes where
//!   the callee cannot leave an exception in flight: a function of the module that throws nothing,
//!   calls nothing through a pointer and calls only such functions, the runtime's helpers and the
//!   imports that call no code back ([`nothrow`]). A call is made with no exception in flight (a
//!   check after every call that may throw lands before the next), so after such a callee the
//!   check reads zero.
//! - A chain of constant steps over one value is one step from its base: `(sp - 4) - 4` is
//!   `sp - 8`, so each push of an argument reads the stack pointer it started from and the steps
//!   between fold into their one use.
//! - A block that only falls into a block nothing else enters is one block with it, so a value
//!   computed before the seam folds into its use after it.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, BlockId, Callee, Function, Inst, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::{dce, known, simplify};

/// The parts of an import's name, lowercased and without its underscores, that say it may call
/// code of the program back: a window procedure, an enumeration's callback, a comparator, the
/// initializers and exit handlers, a raise. An exception such code leaves in flight passes the
/// import, so a function that calls one may leave it in flight too.
const CALLS_BACK: [&str; 32] = [
    "dispatch",
    "message",
    "proc",
    "enum",
    "qsort",
    "bsearch",
    "callback",
    "initterm",
    "onexit",
    "atexit",
    "once",
    "longjmp",
    "raise",
    "throw",
    "rethrow",
    "except",
    "unwind",
    "dialog",
    "hook",
    "createwindow",
    "showwindow",
    "setwindowpos",
    "destroywindow",
    "updatewindow",
    "redrawwindow",
    "movewindow",
    "setfocus",
    "invoke",
    "signal",
    "accelerator",
    "trackpopupmenu",
    "apc",
];

/// The exception runtime's calls of the structured protocol: a function that makes one may run a
/// filter with an exception in flight, so its checks all stay.
const STRUCTURED: [&str; 5] = ["ra_eh_probe", "ra_eh_raise", "ra_eh_pointers", "ra_eh_handled", "ra_eh_unhandled"];

/// Whether a call of `name`, which is no function of the module, can leave no exception in
/// flight: an LLVM intrinsic, a helper of the runtime (but the exception protocol's own and the
/// call through an address), or an import that calls nothing back.
fn quiet_external(name: &str) -> bool {
    if name.starts_with("llvm.") {
        return true;
    }
    if name.starts_with("ra_eh_") {
        return name == "ra_eh_pending";
    }
    if name.starts_with("ra_call") || name.starts_with("F_") || name.starts_with("U_") {
        return false;
    }
    if name.starts_with("ra_x86_") || !name.starts_with("ra_") {
        let plain: String = name.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect();
        return !CALLS_BACK.iter().any(|part| plain.contains(part));
    }
    true
}

/// The functions of `module` that never return with an exception in flight: no throw, no call
/// through a pointer, and every call to such a function, a quiet helper or a quiet import. A
/// function the hand-written sources own may do anything, and so may a function outside the
/// module.
pub(crate) fn nothrow(module: &Module, hand: &HashSet<String>) -> HashSet<String> {
    let index: HashMap<&str, usize> = module.functions.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let n = module.functions.len();
    let mut throws = vec![false; n];
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, f) in module.functions.iter().enumerate() {
        if hand.contains(&f.name) {
            throws[i] = true;
        }
        for inst in f.blocks.iter().flat_map(|b| &b.instrs) {
            let Inst::Call { callee, .. } = &inst.inst else { continue };
            match callee {
                Callee::Direct(name) => match index.get(name.as_str()) {
                    Some(&j) => callers[j].push(i),
                    None if quiet_external(name) => {}
                    None => throws[i] = true,
                },
                Callee::Indirect(_) => throws[i] = true,
            }
        }
    }
    let mut work: Vec<usize> = (0..n).filter(|&i| throws[i]).collect();
    while let Some(j) = work.pop() {
        for &i in &callers[j] {
            if !throws[i] {
                throws[i] = true;
                work.push(i);
            }
        }
    }
    module.functions.iter().zip(&throws).filter(|(_, t)| !**t).map(|(f, _)| f.name.clone()).collect()
}

/// `f` tidied, when anything changed.
pub(crate) fn tidy(f: &Function, nothrow: &HashSet<String>) -> Option<Function> {
    let mut f = f.clone();
    let clean = |f: &mut Function| -> bool {
        let mut any = false;
        loop {
            let again = fold_masks(f) | simplify::run(f) | known::fold_branches(f);
            dce::run(f);
            any |= again;
            if !again {
                return any;
            }
        }
    };
    let mut changed = drop_checks(&mut f, nothrow);
    changed |= fold_offsets(&mut f);
    changed |= clean(&mut f);
    if split_returns(&mut f) {
        clean(&mut f);
        changed = true;
    }
    changed |= merge_straight(&mut f);
    changed |= sink(&mut f);
    changed.then_some(f)
}

/// Moves each pure value that reads no memory, and that only a successor reads, into that
/// successor when nothing else enters it: the value is computed on the way that reads it, where
/// it folds into its use. `ret & 0xffffffff` returned when an exception is in flight is computed
/// in the return.
fn sink(f: &mut Function) -> bool {
    let preds = f.predecessors();
    let uses = f.use_counts();
    let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    // Where each value is read: the reading blocks, a phi's read counted at the block it takes
    // the value from and marked so that nothing moves it.
    let mut readers: HashMap<ValueId, Vec<BlockId>> = HashMap::new();
    let mut pinned: HashSet<ValueId> = HashSet::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Inst::Phi { incoming, .. } = &i.inst {
                pinned.extend(incoming.iter().filter_map(|(o, _)| o.local()));
                continue;
            }
            for v in i.inst.uses() {
                readers.entry(v).or_default().push(b.id);
            }
        }
        for v in b.terminator.uses() {
            readers.entry(v).or_default().push(b.id);
        }
    }
    let mut changed = false;
    let ids: Vec<BlockId> = f.blocks.iter().map(|b| b.id).collect();
    for n in ids {
        let Some([p]) = preds.get(&n).map(Vec::as_slice) else { continue };
        let (p, n_at) = (*p, index[&n]);
        let p_at = index[&p];
        if p == n {
            continue;
        }
        // The values a call's result reaches through single uses: each folds into the next, and
        // the chain stays by the call.
        let mut call_fed: HashSet<ValueId> = HashSet::new();
        for i in &f.blocks[p_at].instrs {
            let Some(v) = i.result else { continue };
            let fed = matches!(i.inst, Inst::Call { .. }) || i.inst.uses().iter().any(|o| call_fed.contains(o));
            if fed && uses.get(&v).copied() == Some(1) {
                call_fed.insert(v);
            }
        }
        let mut moved: Vec<ra_asm_ir::Instruction> = Vec::new();
        let mut at = f.blocks[p_at].instrs.len();
        while at > 0 {
            at -= 1;
            let i = &f.blocks[p_at].instrs[at];
            let Some(v) = i.result else { continue };
            let pure = !i.inst.has_side_effects() && !matches!(i.inst, Inst::Phi { .. } | Inst::Alloca { .. } | Inst::Load { .. });
            let only_n = readers.get(&v).is_some_and(|r| !r.is_empty() && r.iter().all(|b| *b == n));
            // A value over the result of a call it alone reads stays by the call, where the call
            // folds into it.
            let by_call = i.inst.uses().iter().any(|o| call_fed.contains(o));
            if !pure || pinned.contains(&v) || !only_n || by_call {
                continue;
            }
            let i = f.blocks[p_at].instrs.remove(at);
            for o in i.inst.uses() {
                if let Some(r) = readers.get_mut(&o)
                    && let Some(slot) = r.iter_mut().find(|b| **b == p)
                {
                    *slot = n;
                }
            }
            moved.push(i);
        }
        if !moved.is_empty() {
            moved.reverse();
            let block = &mut f.blocks[n_at];
            let phis = block.instrs.iter().take_while(|i| matches!(i.inst, Inst::Phi { .. })).count();
            block.instrs.splice(phis..phis, moved);
            changed = true;
        }
    }
    changed
}

/// Each block that only returns one of its phis returns the phi's value at each way in instead:
/// a jump to it is the return of that value, and a branch to it goes to a block of its own that
/// returns it. The exception checks' shared exit, and a function's shared return, take no copy.
fn split_returns(f: &mut Function) -> bool {
    let mut returns: HashMap<BlockId, (Type, Vec<(Operand, BlockId)>)> = HashMap::new();
    let preds = f.predecessors();
    // A block of phis and a little pure arithmetic over them that returns it (the halves of a
    // 32-bit result put back together) first gets one returning block per way in, each with the
    // arithmetic over the values that way brings.
    let mut changed = expand_returns(f, &preds);
    let preds = if changed { f.predecessors() } else { preds };
    for b in f.blocks.iter().skip(1) {
        let Terminator::Ret(Some((ty, Operand::Local(v)))) = &b.terminator else { continue };
        let [phi] = b.instrs.as_slice() else { continue };
        let (Some(p), Inst::Phi { incoming, .. }) = (phi.result, &phi.inst) else { continue };
        let every_way = preds.get(&b.id).is_some_and(|ps| ps.iter().all(|p| incoming.iter().any(|(_, from)| from == p)));
        if p == *v && every_way && incoming.iter().all(|(o, _)| *o != Operand::Local(p)) {
            returns.insert(b.id, (ty.clone(), incoming.clone()));
        }
    }
    if returns.is_empty() {
        return changed;
    }
    changed = true;
    let mut next_id = f.blocks.iter().map(|b| b.id.0).max().unwrap_or(0) + 1;
    let mut added: Vec<ra_asm_ir::Block> = Vec::new();
    let names: HashMap<BlockId, String> = f.blocks.iter().filter(|b| returns.contains_key(&b.id)).map(|b| (b.id, b.name.clone())).collect();
    for b in &mut f.blocks {
        let value = |target: BlockId| {
            let (ty, incoming) = returns.get(&target)?;
            let (o, _) = incoming.iter().find(|(_, from)| *from == b.id)?;
            Some((ty.clone(), o.clone()))
        };
        if let Terminator::Br(t) = b.terminator
            && let Some(ret) = value(t)
        {
            b.terminator = Terminator::Ret(Some(ret));
            continue;
        }
        // One block per target and way in: a `switch` that names the block twice goes to one.
        let mut made: HashMap<BlockId, BlockId> = HashMap::new();
        let mut targets: Vec<BlockId> = b.terminator.successors();
        targets.sort_unstable_by_key(|t| t.0);
        targets.dedup();
        for t in targets {
            let Some(ret) = value(t) else { continue };
            let id = BlockId(next_id);
            next_id += 1;
            added.push(ra_asm_ir::Block {
                id,
                name: format!("{}.{}", names[&t], id.0),
                instrs: Vec::new(),
                terminator: Terminator::Ret(Some(ret)),
                address: None,
            });
            made.insert(t, id);
        }
        let to = |t: &mut BlockId| {
            if let Some(n) = made.get(t) {
                *t = *n;
            }
        };
        match &mut b.terminator {
            Terminator::CondBr { then, els, .. } => {
                to(then);
                to(els);
            }
            Terminator::Switch { default, cases, .. } => {
                to(default);
                for (_, t) in cases.iter_mut() {
                    to(t);
                }
            }
            _ => {}
        }
    }
    f.blocks.retain(|b| !returns.contains_key(&b.id));
    f.blocks.extend(added);
    changed
}

/// The most pure instructions a returning block of phis may hold for [`expand_returns`] to
/// write it once per way in.
const RETURN_ARITHMETIC: usize = 6;

/// Each block of phis followed by a few pure instructions, which returns, becomes a block per way
/// in: the instructions copied over the values that way brings, the phis gone. A way in by a jump
/// takes the copy into its own block.
fn expand_returns(f: &mut Function, preds: &HashMap<BlockId, Vec<BlockId>>) -> bool {
    let mut targets: Vec<BlockId> = Vec::new();
    for b in f.blocks.iter().skip(1) {
        if !matches!(b.terminator, Terminator::Ret(_)) {
            continue;
        }
        let phis = b.instrs.iter().take_while(|i| matches!(i.inst, Inst::Phi { .. })).count();
        let rest = &b.instrs[phis..];
        if phis == 0 || rest.is_empty() || rest.len() > RETURN_ARITHMETIC {
            continue;
        }
        if rest.iter().any(|i| i.inst.has_side_effects() || matches!(i.inst, Inst::Load { .. } | Inst::Alloca { .. } | Inst::Phi { .. })) {
            continue;
        }
        let Some(ways) = preds.get(&b.id) else { continue };
        let mut distinct = ways.clone();
        distinct.sort_unstable_by_key(|p| p.0);
        distinct.dedup();
        // Every phi names every way in, and nothing outside the block reads its values.
        let complete = b.instrs[..phis].iter().all(|i| match &i.inst {
            Inst::Phi { incoming, .. } => distinct.iter().all(|p| incoming.iter().any(|(_, from)| from == p)),
            _ => false,
        });
        let own: HashSet<ValueId> = b.instrs.iter().filter_map(|i| i.result).collect();
        let read_outside = f.blocks.iter().filter(|o| o.id != b.id).any(|o| {
            o.instrs.iter().any(|i| i.inst.uses().iter().any(|v| own.contains(v))) || o.terminator.uses().iter().any(|v| own.contains(v))
        });
        if complete && !read_outside && distinct.iter().any(|p| checks_exception(f, *p)) {
            targets.push(b.id);
        }
    }
    if targets.is_empty() {
        return false;
    }
    let mut next_id = f.blocks.iter().map(|b| b.id.0).max().unwrap_or(0) + 1;
    for target in targets {
        let Some(at) = f.blocks.iter().position(|b| b.id == target) else { continue };
        let block = f.blocks[at].clone();
        let mut ways = preds.get(&target).cloned().unwrap_or_default();
        ways.sort_unstable_by_key(|p| p.0);
        ways.dedup();
        let all = ways.len();
        // Only the ways out of an exception check: the others keep the block, as the source's.
        ways.retain(|p| checks_exception(f, *p));
        let whole = ways.len() == all;
        if !whole {
            let gone: HashSet<BlockId> = ways.iter().copied().collect();
            for i in &mut f.blocks[at].instrs {
                if let Inst::Phi { incoming, .. } = &mut i.inst {
                    incoming.retain(|(_, from)| !gone.contains(from));
                }
            }
        }
        for way in ways {
            // The phis' values from this way, then a copy of each instruction over them.
            let mut map: HashMap<ValueId, Operand> = HashMap::new();
            let mut instrs = Vec::new();
            for i in &block.instrs {
                match (&i.inst, i.result) {
                    (Inst::Phi { incoming, .. }, Some(p)) => {
                        let o = incoming.iter().find(|(_, from)| *from == way).map_or(Operand::Undef, |(o, _)| o.clone());
                        map.insert(p, o);
                    }
                    (inst, result) => {
                        let mut copy = inst.clone();
                        for o in copy.operands_mut() {
                            if let Operand::Local(v) = o
                                && let Some(r) = map.get(v)
                            {
                                *o = r.clone();
                            }
                        }
                        let result = result.map(|r| {
                            let (hint, ty) = (f.value_name(r).to_owned(), f.value_type(r).clone());
                            let fresh = f.new_value(&hint, ty);
                            map.insert(r, Operand::Local(fresh));
                            fresh
                        });
                        instrs.push(ra_asm_ir::Instruction { result, inst: copy, address: i.address });
                    }
                }
            }
            let mut terminator = block.terminator.clone();
            for o in terminator.operands_mut() {
                if let Operand::Local(v) = o
                    && let Some(r) = map.get(v)
                {
                    *o = r.clone();
                }
            }
            let Some(w) = f.blocks.iter().position(|b| b.id == way) else { continue };
            if f.blocks[w].terminator == Terminator::Br(target) {
                f.blocks[w].instrs.extend(instrs);
                f.blocks[w].terminator = terminator;
                continue;
            }
            let id = BlockId(next_id);
            next_id += 1;
            f.blocks.push(ra_asm_ir::Block { id, name: format!("{}.{}", block.name, id.0), instrs, terminator, address: None });
            let to = |t: &mut BlockId| {
                if *t == target {
                    *t = id;
                }
            };
            match &mut f.blocks[w].terminator {
                Terminator::Br(t) => to(t),
                Terminator::CondBr { then, els, .. } => {
                    to(then);
                    to(els);
                }
                Terminator::Switch { default, cases, .. } => {
                    to(default);
                    for (_, t) in cases.iter_mut() {
                        to(t);
                    }
                }
                _ => {}
            }
        }
        if whole {
            f.blocks.retain(|b| b.id != target);
        }
    }
    true
}

/// Whether the block `b` ends in the branch of an exception check: its last call asks
/// `ra_eh_pending` and a conditional branch ends it.
fn checks_exception(f: &Function, b: BlockId) -> bool {
    f.block(b).is_some_and(|block| {
        matches!(block.terminator, Terminator::CondBr { .. })
            && block.instrs.iter().rev().find_map(|i| match &i.inst {
                Inst::Call { callee: Callee::Direct(n), .. } => Some(n == "ra_eh_pending"),
                Inst::Call { .. } => Some(false),
                _ => None,
            }) == Some(true)
    })
}

/// Drops each exception check whose call before it cannot leave an exception in flight: the
/// check's value is zero, and the branch on it folds.
fn drop_checks(f: &mut Function, nothrow: &HashSet<String>) -> bool {
    let calls = |f: &Function, name: &str| {
        f.blocks.iter().flat_map(|b| &b.instrs).any(|i| matches!(&i.inst, Inst::Call { callee: Callee::Direct(n), .. } if n == name))
    };
    if !calls(f, "ra_eh_pending") || STRUCTURED.iter().any(|n| calls(f, n)) {
        return false;
    }
    let mut zero: HashMap<ValueId, Operand> = HashMap::new();
    for b in &mut f.blocks {
        let mut dropped: HashSet<usize> = HashSet::new();
        for (at, i) in b.instrs.iter().enumerate() {
            let (Some(pending), Inst::Call { callee: Callee::Direct(name), .. }) = (i.result, &i.inst) else { continue };
            if name != "ra_eh_pending" {
                continue;
            }
            // Back to the call the check follows, past what cannot put an exception in flight.
            let quiet = b.instrs[..at].iter().rev().find_map(|before| match &before.inst {
                Inst::Call { callee: Callee::Direct(n), .. } if nothrow.contains(n) => Some(true),
                Inst::Call { callee: Callee::Direct(n), .. }
                    if (n.starts_with("llvm.") || n.starts_with("ra_")) && !n.starts_with("ra_eh_") && quiet_external(n) =>
                {
                    None
                }
                Inst::Call { .. } | Inst::AtomicRmw { .. } | Inst::CmpXchg { .. } => Some(false),
                _ => None,
            });
            if quiet == Some(true) {
                dropped.insert(at);
                zero.insert(pending, Operand::Int(0));
            }
        }
        if !dropped.is_empty() {
            let mut at = 0;
            b.instrs.retain(|_| {
                at += 1;
                !dropped.contains(&(at - 1))
            });
        }
    }
    if zero.is_empty() {
        return false;
    }
    f.replace_uses(&zero);
    true
}

/// The constant mask of an integer type's width.
fn width_mask(bits: u16) -> i128 {
    if bits >= 128 { -1 } else { (1i128 << bits) - 1 }
}

/// The masks the halves of a 32-bit program's 64-bit values go through, folded: `(x & a) & b`
/// is `x & (a & b)`, `(x >> k) << k` is `x` with its low `k` bits cleared, and `(x & a) | (x &
/// b)` is `x & (a | b)`, so a value split into its halves and put back together is the value.
fn fold_masks(f: &mut Function) -> bool {
    let defs: HashMap<ValueId, Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, i.inst.clone()))).collect();
    let masked = |o: &Operand, bits: u16| -> Option<(Operand, i128)> {
        match o.local().and_then(|v| defs.get(&v)) {
            Some(Inst::Bin { op: BinOp::And, ty: Type::Int(b), lhs, rhs: Operand::Int(c) }) if *b == bits => {
                Some((lhs.clone(), *c & width_mask(bits)))
            }
            Some(Inst::Bin { op: BinOp::Shl, ty: Type::Int(b), lhs, rhs: Operand::Int(k) })
                if *b == bits && (1..i128::from(bits)).contains(k) =>
            {
                match lhs.local().and_then(|v| defs.get(&v)) {
                    Some(Inst::Bin { op: BinOp::LShr, ty: Type::Int(b2), lhs: x, rhs: Operand::Int(k2) }) if *b2 == bits && k2 == k => {
                        Some((x.clone(), (width_mask(bits) << *k) & width_mask(bits)))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    };
    let mut changed = false;
    // An undefined integer reads as zero in the C (a register no way in set): as the constant,
    // the arithmetic over it folds.
    for b in &mut f.blocks {
        for i in &mut b.instrs {
            let operands: Vec<&mut Operand> = match &mut i.inst {
                Inst::Bin { ty: Type::Int(_), lhs, rhs, .. } => vec![lhs, rhs],
                Inst::Cast { from: Type::Int(_), value, to: Type::Int(_), .. } => vec![value],
                _ => Vec::new(),
            };
            for o in operands {
                if *o == Operand::Undef {
                    *o = Operand::Int(0);
                    changed = true;
                }
            }
        }
    }
    for b in &mut f.blocks {
        for i in &mut b.instrs {
            let Inst::Bin { op, ty: Type::Int(bits), lhs, rhs } = &i.inst else { continue };
            let bits = *bits;
            if !(8..=64).contains(&bits) {
                continue;
            }
            let new = match (op, rhs) {
                (BinOp::And, Operand::Int(c)) => {
                    masked(lhs, bits).filter(|(_, a)| *a != width_mask(bits)).map(|(x, a)| (x, a & *c & width_mask(bits)))
                }
                (BinOp::Or, _) => match (masked(lhs, bits), masked(rhs, bits)) {
                    (Some((x, low)), Some((y, high))) if x == y => Some((x, low | high)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((x, mask)) = new {
                i.inst = Inst::Bin { op: BinOp::And, ty: Type::Int(bits), lhs: x, rhs: Operand::Int(mask) };
                changed = true;
            }
        }
    }
    changed
}

/// Each add or subtract of a constant over a value that is itself one, written over the first
/// value of the chain: `(x - 4) - 4` is `x - 8`, whatever else reads `x - 4`.
fn fold_offsets(f: &mut Function) -> bool {
    let order = ra_asm_ir::dom::reverse_post_order(f);
    let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut offsets: HashMap<ValueId, (Operand, i128)> = HashMap::new();
    // The same for an element step of a pointer: `getelementptr i8, (getelementptr i8, p, a), b`
    // is one of `a + b` from `p`.
    let mut steps: HashMap<ValueId, (Type, Type, Operand, i128)> = HashMap::new();
    let mut changed = false;
    for id in order {
        let Some(&bi) = index.get(&id) else { continue };
        for i in &mut f.blocks[bi].instrs {
            let Some(result) = i.result else { continue };
            if let Inst::Gep { base_ty, ptr, indices, .. } = &i.inst
                && let [(index_ty @ Type::Int(64), Operand::Int(k))] = indices.as_slice()
            {
                let (base, total) = match ptr.local().and_then(|v| steps.get(&v)) {
                    Some((t, it, base, a)) if t == base_ty && it == index_ty => (base.clone(), a.wrapping_add(*k)),
                    _ => (ptr.clone(), *k),
                };
                steps.insert(result, (base_ty.clone(), index_ty.clone(), base.clone(), total));
                if base != *ptr
                    && let Inst::Gep { ptr, indices, .. } = &mut i.inst
                {
                    *ptr = base;
                    indices[0].1 = Operand::Int(total);
                    changed = true;
                }
                continue;
            }
            let Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub), ty: Type::Int(bits), lhs, rhs: Operand::Int(c) } = &i.inst else {
                continue;
            };
            let (bits, mask) = (*bits, width_mask(*bits));
            if bits < 8 {
                continue;
            }
            let step = if *op == BinOp::Add { *c } else { c.wrapping_neg() };
            let (base, total) = match lhs.local().and_then(|v| offsets.get(&v)) {
                Some((base, k)) => (base.clone(), k.wrapping_add(step) & mask),
                None => (lhs.clone(), step & mask),
            };
            offsets.insert(result, (base.clone(), total));
            if base == *lhs {
                continue;
            }
            // A step past the half of the width reads as the subtraction it is.
            let negative = bits < 128 && total > mask / 2;
            let (op, k) = if negative { (BinOp::Sub, total.wrapping_neg() & mask) } else { (BinOp::Add, total) };
            i.inst = Inst::Bin { op, ty: Type::Int(bits), lhs: base, rhs: Operand::Int(k) };
            changed = true;
        }
    }
    changed
}

/// Merges each block that ends in a jump to a block nothing else enters, and that has no phi,
/// with it; the phis after it take their values from the merged block.
fn merge_straight(f: &mut Function) -> bool {
    let Some(entry) = f.blocks.first().map(|b| b.id) else { return false };
    let mut entries: HashMap<BlockId, usize> = HashMap::new();
    for b in &f.blocks {
        for s in b.terminator.successors() {
            *entries.entry(s).or_default() += 1;
        }
    }
    let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut gone: HashSet<BlockId> = HashSet::new();
    for i in 0..f.blocks.len() {
        if gone.contains(&f.blocks[i].id) {
            continue;
        }
        while let Terminator::Br(next) = f.blocks[i].terminator {
            let me = f.blocks[i].id;
            if next == me || next == entry || entries.get(&next).copied() != Some(1) || gone.contains(&next) {
                break;
            }
            let Some(&j) = index.get(&next) else { break };
            if f.blocks[j].instrs.iter().any(|x| matches!(x.inst, Inst::Phi { .. })) {
                break;
            }
            let instrs = std::mem::take(&mut f.blocks[j].instrs);
            let terminator = std::mem::replace(&mut f.blocks[j].terminator, Terminator::Unreachable);
            f.blocks[i].instrs.extend(instrs);
            gone.insert(next);
            for s in terminator.successors() {
                let Some(&k) = index.get(&s) else { continue };
                for x in &mut f.blocks[k].instrs {
                    if let Inst::Phi { incoming, .. } = &mut x.inst {
                        for (_, from) in incoming.iter_mut() {
                            if *from == next {
                                *from = me;
                            }
                        }
                    }
                }
            }
            f.blocks[i].terminator = terminator;
        }
    }
    if gone.is_empty() {
        return false;
    }
    f.blocks.retain(|b| !gone.contains(&b.id));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_that_call_back_are_not_quiet() {
        assert!(quiet_external("memcpy"));
        assert!(quiet_external("HeapAlloc"));
        assert!(quiet_external("ra_pops_of"));
        assert!(quiet_external("ra_eh_pending"));
        assert!(!quiet_external("ra_eh_throw"));
        assert!(!quiet_external("ra_call"));
        assert!(!quiet_external("DispatchMessageW"));
        assert!(!quiet_external("ra_x86_dispatch_message_a"));
        assert!(!quiet_external("qsort"));
        assert!(!quiet_external("EnumWindows"));
        assert!(!quiet_external("RaiseException"));
        assert!(!quiet_external("F_00401000"));
    }
}
