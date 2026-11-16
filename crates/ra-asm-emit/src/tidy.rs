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
//! - A 64-bit result of a 32-bit program that a shared landing takes from each check as its two
//!   halves, and returns put back together, is taken whole ([`join_halves`]).
//! - A block that only falls into a block nothing else enters is one block with it, so a value
//!   computed before the seam folds into its use after it.
//! - A call through a pointer that hands on stack words past the universal six, which the raise
//!   writes as a store of each into `ra_args` and the call, is one call of the runtime's
//!   [`ARGS_CALL`] that takes them all.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FnType, Function, Inst, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::{dce, known, simplify};
use ra_asm_lift::state::ARGS;

/// The runtime's form of a call through a pointer with stack words past the universal six
/// (`rhipper.h`): the callee, the eight universal words, the count of the words past them and
/// those words, which it stores into [`ARGS`] once every argument is evaluated, before the call.
pub(crate) const ARGS_CALL: &str = "ra_args_call";

/// The runtime's adapters of a 32-bit program's imports that return with nothing a handler of the
/// program sees, by the rule of [`ra_asm_protos::calls_no_code`]: each takes only integers, reads
/// or writes no pointer of the program and calls no import that may raise. The others (the
/// initializers and exit tables, the window and message functions, the loader, `longjmp`, the
/// stdio and `strto*` adapters, the run-time type casts, `operator new` and `delete`, the
/// interlocked and critical-section functions over the program's memory, the waits, the math)
/// may, and keep the check after them.
const QUIET_SHIMS: &[&str] = &[
    "ra_x86_p_argv",
    "ra_x86_p_commode",
    "ra_x86_p_fmode",
    "ra_x86_p_argc",
    "ra_x86_errno",
    "ra_x86_controlfp",
    "ra_x86_statusfp",
    "ra_x86_clearfp",
    "ra_x86_fesetround",
];

/// The exception runtime's calls of the structured protocol: a function that makes one may run a
/// filter with an exception in flight, so its checks all stay.
const STRUCTURED: [&str; 5] = ["ra_eh_probe", "ra_eh_raise", "ra_eh_pointers", "ra_eh_handled", "ra_eh_unhandled"];

/// Whether a call of `name`, which is no function of the module, can leave no exception in
/// flight: an LLVM intrinsic, a helper of the runtime (but the exception protocol's own and the
/// call through an address), or an import known to run no code of the program. An import not
/// known so keeps its check, whatever its name says.
fn quiet_external(name: &str) -> bool {
    if name.starts_with("llvm.") {
        return true;
    }
    if name.starts_with("ra_eh_") {
        return name == "ra_eh_pending";
    }
    if name.starts_with("ra_call") || name == ARGS_CALL || name.starts_with("F_") || name.starts_with("U_") {
        return false;
    }
    if name.starts_with("ra_x86_") {
        return QUIET_SHIMS.contains(&name);
    }
    if !name.starts_with("ra_") {
        return ra_asm_protos::calls_no_code(name);
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
            let again = fold_masks(f) | join_halves(f) | simplify::run(f) | known::fold_branches(f);
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
    if fold_arg_stores(&mut f) {
        dce::run(&mut f);
        changed = true;
    }
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

/// The low half of a 64-bit value as a mask.
const LOW_HALF: i128 = 0xffff_ffff;

/// Type for the entry in a pair map: block and its incoming values.
type ValuePairEntry = (BlockId, Vec<(Operand, BlockId)>);

/// Each 64-bit value put back together from two phis, `(a & 0xffffffff) | (b << 32)`, where every
/// way in brings the halves of one value `x` (`x` or `x` masked for `a`, `x >> 32` for `b`),
/// becomes a phi of those values: a 32-bit program's 64-bit result that an exception path only
/// passes on is returned whole, not through its halves. The phis must be read only as the halves.
fn join_halves(func: &mut Function) -> bool {
    let defs: HashMap<ValueId, Inst> =
        func.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, i.inst.clone()))).collect();
    let phis: HashMap<ValueId, ValuePairEntry> = func
        .blocks
        .iter()
        .flat_map(|b| b.instrs.iter().map(move |i| (b.id, i)))
        .filter_map(|(b, i)| match (&i.inst, i.result) {
            (Inst::Phi { ty: Type::Int(64), incoming }, Some(p)) => Some((p, (b, incoming.clone()))),
            _ => None,
        })
        .collect();
    let low = |o: &Operand| match o.local().and_then(|v| defs.get(&v)) {
        Some(Inst::Bin { op: BinOp::And, ty: Type::Int(64), lhs: Operand::Local(p), rhs: Operand::Int(LOW_HALF) }) => Some(*p),
        _ => None,
    };
    let high = |o: &Operand| match o.local().and_then(|v| defs.get(&v)) {
        Some(Inst::Bin { op: BinOp::Shl, ty: Type::Int(64), lhs: Operand::Local(q), rhs: Operand::Int(32) }) => Some(*q),
        _ => None,
    };
    // The values read other than as a half: the operand of the mask or the shift above counts
    // as a half, any other read does not.
    let mut other: HashSet<ValueId> = HashSet::new();
    for b in &func.blocks {
        for i in &b.instrs {
            match &i.inst {
                Inst::Bin { op: BinOp::And, ty: Type::Int(64), lhs: Operand::Local(_), rhs: Operand::Int(LOW_HALF) }
                | Inst::Bin { op: BinOp::Shl, ty: Type::Int(64), lhs: Operand::Local(_), rhs: Operand::Int(32) } => {}
                inst => other.extend(inst.uses()),
            }
        }
        other.extend(b.terminator.uses());
    }
    // The whole value of a way in, given the halves it brings.
    let whole = |a: &Operand, b: &Operand| -> Option<Operand> {
        let x = match b.local().and_then(|v| defs.get(&v)) {
            Some(Inst::Bin { op: BinOp::LShr, ty: Type::Int(64), lhs: x @ Operand::Local(_), rhs: Operand::Int(32) }) => x.clone(),
            _ => return None,
        };
        let keeps_low = *a == x
            || matches!(a.local().and_then(|v| defs.get(&v)), Some(Inst::Bin { op: BinOp::And, ty: Type::Int(64), lhs, rhs: Operand::Int(c) })
                if *lhs == x && c & LOW_HALF == LOW_HALF);
        keeps_low.then_some(x)
    };
    // Each put-together value with the pair of phis it reads, and each pair's block and ways in.
    let mut ors: Vec<(ValueId, (ValueId, ValueId))> = Vec::new();
    let mut pairs: HashMap<(ValueId, ValueId), Option<ValuePairEntry>> = HashMap::new();
    for i in func.blocks.iter().flat_map(|b| &b.instrs) {
        let (Some(r), Inst::Bin { op: BinOp::Or, ty: Type::Int(64), lhs, rhs }) = (i.result, &i.inst) else { continue };
        let ((Some(p), Some(q), _, _) | (_, _, Some(p), Some(q))) = (low(lhs), high(rhs), low(rhs), high(lhs)) else { continue };
        if other.contains(&p) || other.contains(&q) {
            continue;
        }
        pairs.entry((p, q)).or_insert_with(|| {
            let ((block, lows), (high_block, highs)) = (phis.get(&p)?, phis.get(&q)?);
            if block != high_block {
                return None;
            }
            let incoming = lows.iter().map(|(a, from)| {
                let (b, _) = highs.iter().find(|(_, h)| h == from)?;
                Some((whole(a, b)?, *from))
            });
            Some((*block, incoming.collect::<Option<Vec<_>>>()?))
        });
        ors.push((r, (p, q)));
    }
    let mut made: HashMap<(ValueId, ValueId), ValueId> = HashMap::new();
    let mut joined: HashMap<ValueId, Operand> = HashMap::new();
    for (r, pair) in ors {
        let Some(Some((block, incoming))) = pairs.get(&pair) else { continue };
        let w = if let Some(w) = made.get(&pair) {
            *w
        } else {
            let hint = incoming.iter().find_map(|(x, _)| x.local()).map_or("wide", |x| func.value_name(x)).to_owned();
            let w = func.new_value(&hint, Type::I64);
            let phi =
                ra_asm_ir::Instruction { result: Some(w), inst: Inst::Phi { ty: Type::I64, incoming: incoming.clone() }, address: None };
            if let Some(b) = func.blocks.iter_mut().find(|b| b.id == *block) {
                b.instrs.insert(0, phi);
            }
            made.insert(pair, w);
            w
        };
        joined.insert(r, Operand::Local(w));
    }
    if joined.is_empty() {
        return false;
    }
    func.replace_uses(&joined);
    true
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

/// The byte offset into [`ARGS`] that `ptr` addresses, through its casts and constant steps.
fn args_offset(defs: &HashMap<ValueId, &Inst>, ptr: &Operand) -> Option<i64> {
    let (mut at, mut offset) = (ptr, 0i64);
    for _ in 0..16 {
        match at {
            Operand::Global(g) if g == ARGS => return Some(offset),
            Operand::Address { global, offset: k } if global == ARGS => return offset.checked_add(*k),
            Operand::Local(v) => match defs.get(v)? {
                Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt | Cast::Bitcast, value, .. } => at = value,
                Inst::Gep { base_ty: Type::Int(8), ptr, indices, .. } => {
                    let [(_, Operand::Int(k))] = indices.as_slice() else { return None };
                    offset = offset.checked_add(i64::try_from(*k).ok()?)?;
                    at = ptr;
                }
                Inst::Bin { op: BinOp::Add, lhs, rhs: Operand::Int(k), .. } => {
                    offset = offset.checked_add(i64::try_from(*k).ok()?)?;
                    at = lhs;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}

/// Whether the address `ptr` may be in [`ARGS`], by its arithmetic: a pointer that comes from
/// memory, a call or a phi is the program's, which never holds a host global's address.
fn may_point_at_args(defs: &HashMap<ValueId, &Inst>, ptr: &Operand, depth: usize) -> bool {
    match ptr {
        Operand::Global(g) | Operand::Address { global: g, .. } => g == ARGS,
        Operand::Local(v) => match defs.get(v) {
            _ if depth > 32 => true,
            Some(Inst::Cast { value, .. }) => may_point_at_args(defs, value, depth + 1),
            Some(Inst::Gep { ptr, indices, .. }) => {
                may_point_at_args(defs, ptr, depth + 1) || indices.iter().any(|(_, i)| may_point_at_args(defs, i, depth + 1))
            }
            Some(Inst::Bin { lhs, rhs, .. }) => may_point_at_args(defs, lhs, depth + 1) || may_point_at_args(defs, rhs, depth + 1),
            Some(Inst::Select { then, els, .. }) => may_point_at_args(defs, then, depth + 1) || may_point_at_args(defs, els, depth + 1),
            _ => false,
        },
        _ => false,
    }
}

/// Whether `word` is written in C as a `uint64_t`, as a word handed on through the variadic
/// [`ARGS_CALL`] must be: a constant, a parameter, or a load or an integer cast of that width.
fn plain_word(defs: &HashMap<ValueId, &Inst>, word: &Operand) -> bool {
    match word {
        Operand::Int(_) => true,
        Operand::Local(v) => match defs.get(v) {
            None
            | Some(
                Inst::Load { ty: Type::Int(64), .. }
                | Inst::Cast { cast: Cast::ZExt | Cast::SExt | Cast::Trunc | Cast::PtrToInt, to: Type::Int(64), .. },
            ) => true,
            Some(_) => false,
        },
        _ => false,
    }
}

/// Each call through a pointer whose stack words past the universal six are stored into
/// [`ARGS`] just before it, as [`ARGS_CALL`] with those words among its arguments: one statement
/// where the stores took one each. The runtime stores them after every argument is evaluated,
/// which keeps the order when nothing between the first store and the call acts, besides the
/// stores, or reads [`ARGS`].
fn fold_arg_stores(f: &mut Function) -> bool {
    if !f.blocks.iter().flat_map(|b| &b.instrs).any(|i| matches!(i.inst, Inst::Call { callee: Callee::Indirect(_), .. })) {
        return false;
    }
    let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, &i.inst))).collect();
    // The calls rewritten, by their block and index, and the stores they took.
    let mut calls: Vec<(usize, usize, Inst)> = Vec::new();
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ci, call) in b.instrs.iter().enumerate() {
            let Inst::Call { fn_ty, callee: Callee::Indirect(fp), args, tail: false } = &call.inst else { continue };
            if fn_ty.ret != Type::I64 || fn_ty.varargs || args.len() != 8 || args.iter().any(|(t, _)| *t != Type::I64) {
                continue;
            }
            let mut words: HashMap<i64, Operand> = HashMap::new();
            let mut stores = Vec::new();
            let mut whole = true;
            for (si, before) in b.instrs[..ci].iter().enumerate().rev() {
                match &before.inst {
                    Inst::Store { ty: Type::Int(64), value, ptr, volatile: false, .. } if let Some(offset) = args_offset(&defs, ptr) => {
                        // A word stored twice is no shape of the raise's.
                        whole &= words.insert(offset, value.clone()).is_none();
                        stores.push((bi, si));
                    }
                    Inst::Load { ptr, .. } if may_point_at_args(&defs, ptr, 0) => break,
                    Inst::Call { .. } | Inst::AtomicRmw { .. } | Inst::CmpXchg { .. } => break,
                    i if i.has_side_effects() => break,
                    _ => {}
                }
            }
            let n = words.len();
            let mut extra = Vec::with_capacity(n);
            for k in 0..n {
                match i64::try_from(8 * k).ok().and_then(|offset| words.get(&offset)) {
                    Some(word) if plain_word(&defs, word) => extra.push(word.clone()),
                    _ => whole = false,
                }
            }
            if !whole || n == 0 {
                continue;
            }
            let mut params = vec![Type::Ptr];
            params.extend(std::iter::repeat_n(Type::I64, 8));
            params.push(Type::I32);
            params.extend(std::iter::repeat_n(Type::I64, n));
            let mut with = vec![(Type::Ptr, fp.clone())];
            with.extend(args.iter().cloned());
            with.push((Type::I32, Operand::Int(i128::try_from(n).unwrap_or_default())));
            with.extend(extra.into_iter().map(|w| (Type::I64, w)));
            let inst =
                Inst::Call { fn_ty: FnType::new(Type::I64, params), callee: Callee::Direct(ARGS_CALL.into()), args: with, tail: false };
            calls.push((bi, ci, inst));
            taken.extend(stores);
        }
    }
    if calls.is_empty() {
        return false;
    }
    for (bi, ci, inst) in calls {
        f.blocks[bi].instrs[ci].inst = inst;
    }
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let mut at = 0;
        block.instrs.retain(|_| {
            at += 1;
            !taken.contains(&(bi, at - 1))
        });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    use ra_asm_ir::{FnType, FunctionBuilder};

    #[test]
    fn only_imports_known_to_call_nothing_back_are_quiet() {
        assert!(quiet_external("GetTickCount"));
        assert!(quiet_external("ra_x86_p_argc"));
        assert!(quiet_external("ra_pops_of"));
        assert!(quiet_external("ra_eh_pending"));
        assert!(!quiet_external("ra_eh_throw"));
        assert!(!quiet_external("ra_call"));
        assert!(!quiet_external("DispatchMessageW"));
        assert!(!quiet_external("ra_x86_dispatch_message_a"));
        assert!(!quiet_external("ra_x86_initterm"));
        assert!(!quiet_external("qsort"));
        assert!(!quiet_external("EnumWindows"));
        assert!(!quiet_external("RaiseException"));
        assert!(!quiet_external("F_00401000"));
        // No word of the name says so, but each runs code of the program: an APC, a window
        // procedure, a thread start.
        assert!(!quiet_external("SleepEx"));
        assert!(!quiet_external("SetWindowTextA"));
        assert!(!quiet_external("CreateThread"));
        // Each may raise into a handler of the program: a bad pointer, an invalid handle, a
        // corrupt heap.
        assert!(!quiet_external("memcpy"));
        assert!(!quiet_external("strcpy"));
        assert!(!quiet_external("CloseHandle"));
        assert!(!quiet_external("WaitForSingleObject"));
        assert!(!quiet_external("HeapFree"));
        assert!(!quiet_external("ra_x86_enter_critical_section"));
        assert!(!quiet_external("ra_x86_wait_for_multiple_objects"));
    }

    /// A module of two functions: `g` calls the import `import` and returns, and `f` calls `g`,
    /// asks `ra_eh_pending` after it and lands on 1 when an exception is in flight, else 0.
    fn calling_through(import: &str) -> Module {
        let void = FnType::new(Type::Void, vec![]);
        let mut gb = FunctionBuilder::new("g", void.clone(), &[]);
        let entry = gb.block("entry");
        gb.call(entry, void.clone(), Callee::Direct(import.into()), vec![], "");
        gb.terminate(entry, Terminator::Ret(None));
        let mut fb = FunctionBuilder::new("f", FnType::new(Type::I32, vec![]), &[]);
        let (entry, landing, ok) = (fb.block("entry"), fb.block("eh.landing"), fb.block("eh.ok"));
        fb.call(entry, void.clone(), Callee::Direct("g".into()), vec![], "");
        let pending = fb.call(entry, FnType::new(Type::I64, vec![]), Callee::Direct("ra_eh_pending".into()), vec![], "eh.pending");
        let pending = pending.expect("ra_eh_pending returns a value");
        let thrown = fb.icmp(entry, ra_asm_ir::ICond::Ne, Type::I64, pending, 0i64, "eh.thrown");
        fb.terminate(entry, Terminator::CondBr { cond: thrown.into(), then: landing, els: ok });
        fb.terminate(landing, Terminator::Ret(Some((Type::I32, Operand::Int(1)))));
        fb.terminate(ok, Terminator::Ret(Some((Type::I32, Operand::Int(0)))));
        let mut m = Module::x86_64_windows("test");
        for (name, ty) in [(import, void.clone()), ("ra_eh_pending", FnType::new(Type::I64, vec![]))] {
            m.declare(ra_asm_ir::Declaration { name: name.into(), ty, conv: ra_asm_ir::CallConv::default(), attrs: Vec::new() });
        }
        m.functions.push(gb.finish());
        m.functions.push(fb.finish());
        m
    }

    /// Whether `f` of `module`, tidied, still asks `ra_eh_pending`.
    fn keeps_check(module: &Module) -> bool {
        let nothrow = nothrow(module, &HashSet::new());
        let f = module.functions.iter().find(|f| f.name == "f").expect("f is in the module");
        let f = tidy(f, &nothrow).unwrap_or_else(|| f.clone());
        f.blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .any(|i| matches!(&i.inst, Inst::Call { callee: Callee::Direct(n), .. } if n == "ra_eh_pending"))
    }

    #[test]
    fn a_call_through_an_unlisted_import_keeps_its_check() {
        let unlisted = calling_through("SleepEx");
        assert!(keeps_check(&unlisted));
        let c = crate::c::emit_c(&unlisted, &crate::c::Options::default());
        assert!(c.contains("ra_eh_pending()"), "the C of a call through SleepEx asks for the exception:\n{c}");
        assert!(keeps_check(&calling_through("memcpy")));
        assert!(keeps_check(&calling_through("CloseHandle")));
        assert!(!keeps_check(&calling_through("GetTickCount")));
    }

    /// `f(target, sp)`: a call through `target` of the universal shape, with the two stack words
    /// above its six loaded from `sp + 24` and `sp + 28` and stored into `ra_args` before it, as
    /// the raise writes it; `between` is put between the stores and the call.
    fn call_with_words(between: fn(&mut FunctionBuilder, BlockId)) -> Function {
        let universal = FnType::new(Type::I64, vec![Type::I64; 8]);
        let mut fb = FunctionBuilder::new("f", FnType::new(Type::I64, vec![Type::I64, Type::I64]), &["target", "sp"]);
        let entry = fb.block("entry");
        let (target, sp) = (fb.param(0), fb.param(1));
        let fp = fb.cast(entry, Cast::IntToPtr, Type::I64, target, Type::Ptr, "fp");
        for k in 0..2i64 {
            let at = fb.bin(entry, BinOp::Add, Type::I64, sp, 24 + 4 * k, "at");
            let p = fb.cast(entry, Cast::IntToPtr, Type::I64, at, Type::Ptr, "p");
            let word = fb.load(entry, Type::I32, p, "word");
            let word = fb.cast(entry, Cast::ZExt, Type::I32, word, Type::I64, "sarg");
            let slot = Operand::Address { global: ARGS.to_owned(), offset: 8 * k };
            let slot = fb.cast(entry, Cast::IntToPtr, Type::I64, slot, Type::Ptr, "args.p");
            fb.store(entry, Type::I64, word, slot);
        }
        between(&mut fb, entry);
        let args = (0..8).map(|k| (Type::I64, Operand::Local(if k % 2 == 0 { sp } else { target }))).collect();
        let ret = fb.call(entry, universal, Callee::Indirect(fp.into()), args, "ret").expect("the call returns a value");
        fb.terminate(entry, Terminator::Ret(Some((Type::I64, ret.into()))));
        fb.finish()
    }

    /// The calls of `f` and whether it still stores into `ra_args`.
    fn calls_and_stores(f: &Function) -> (Vec<&Inst>, bool) {
        let instrs: Vec<&Inst> = f.blocks.iter().flat_map(|b| &b.instrs).map(|i| &i.inst).collect();
        let stores = instrs.iter().any(|i| matches!(i, Inst::Store { .. }));
        (instrs.into_iter().filter(|i| matches!(i, Inst::Call { .. })).collect(), stores)
    }

    #[test]
    fn a_call_through_a_pointer_takes_its_words_past_the_six() {
        let f = call_with_words(|_, _| {});
        let tidied = tidy(&f, &HashSet::new()).expect("the stores fold into the call");
        let (calls, stores) = calls_and_stores(&tidied);
        assert!(!stores, "the stores into ra_args are the call's now");
        let [Inst::Call { callee: Callee::Direct(name), args, .. }] = calls.as_slice() else { panic!("one call, {} found", calls.len()) };
        assert_eq!(name, ARGS_CALL);
        // The callee, the eight universal words, the count and the two words.
        assert_eq!(args.len(), 12);
        assert_eq!(args[9], (Type::I32, Operand::Int(2)));
        let mut m = Module::x86_64_windows("test");
        m.globals.push(ra_asm_lift::state::args_global(2));
        m.functions.push(f);
        let c = crate::c::emit_c(&m, &crate::c::Options::default());
        assert!(c.contains("ra_args_call("), "the C makes the call in one statement:\n{c}");
        assert!(!c.contains("&ra_args)"), "the C stores into ra_args nowhere:\n{c}");
    }

    #[test]
    fn a_read_of_the_words_between_keeps_the_stores() {
        let f = call_with_words(|fb, entry| {
            let p = fb.cast(entry, Cast::IntToPtr, Type::I64, Operand::Address { global: ARGS.to_owned(), offset: 0 }, Type::Ptr, "q");
            let read = fb.load(entry, Type::I64, p, "read");
            fb.store(entry, Type::I64, read, Operand::Address { global: ARGS.to_owned(), offset: 8 });
        });
        let tidied = tidy(&f, &HashSet::new()).unwrap_or_else(|| f.clone());
        let (calls, stores) = calls_and_stores(&tidied);
        assert!(stores);
        assert!(matches!(calls.as_slice(), [Inst::Call { callee: Callee::Indirect(_), .. }]), "the call stays through its pointer");
    }

    /// `i64 wide()`: two calls of 64-bit results, each followed by an exception check whose
    /// landing returns the halves the way in brings put back together. With `crossed`, the second
    /// way brings the high half of the first call's result.
    fn landing_of_halves(crossed: bool) -> (Function, [ValueId; 2], [BlockId; 2]) {
        let mut b = ra_asm_ir::FunctionBuilder::new("wide", ra_asm_ir::FnType::new(Type::I64, Vec::new()), &[]);
        let entry = b.block("entry");
        let ok = b.block("ok");
        let done = b.block("done");
        let land = b.block("land");
        let wide = || ra_asm_ir::FnType::new(Type::I64, Vec::new());
        let mut halves = Vec::new();
        let mut results = Vec::new();
        for (block, next) in [(entry, ok), (ok, done)] {
            let r = b.call(block, wide(), Callee::Direct("g".into()), Vec::new(), "ret").expect("a result");
            let lo = b.bin(block, BinOp::And, Type::I64, r, LOW_HALF, "ret.lo");
            let hi = b.bin(block, BinOp::LShr, Type::I64, r, 32i64, "ret.hi");
            let pending = b.call(block, wide(), Callee::Direct("ra_eh_pending".into()), Vec::new(), "eh.pending").expect("a result");
            let thrown = b.icmp(block, ra_asm_ir::ICond::Ne, Type::I64, pending, 0i64, "eh.thrown");
            b.terminate(block, Terminator::CondBr { cond: thrown.into(), then: land, els: next });
            halves.push((lo, hi));
            results.push(r);
        }
        b.terminate(done, Terminator::Ret(Some((Type::I64, results[1].into()))));
        let high_second = if crossed { halves[0].1 } else { halves[1].1 };
        let a =
            b.value(land, Inst::Phi { ty: Type::I64, incoming: vec![(halves[0].0.into(), entry), (halves[1].0.into(), ok)] }, "rax.phi");
        let d =
            b.value(land, Inst::Phi { ty: Type::I64, incoming: vec![(halves[0].1.into(), entry), (high_second.into(), ok)] }, "rdx.phi");
        let _ = b.call(land, ra_asm_ir::FnType::new(Type::Void, Vec::new()), Callee::Direct("ra_eh_suspend".into()), Vec::new(), "");
        let low = b.bin(land, BinOp::And, Type::I64, a, LOW_HALF, "ret.lo");
        let high = b.bin(land, BinOp::Shl, Type::I64, d, 32i64, "ret.hi");
        let whole = b.bin(land, BinOp::Or, Type::I64, low, high, "ret.wide");
        b.terminate(land, Terminator::Ret(Some((Type::I64, whole.into()))));
        (b.finish(), [results[0], results[1]], [entry, ok])
    }

    /// The value the block `b` of `f` returns, and its instruction.
    fn returned(f: &Function, b: BlockId) -> (ValueId, Option<Inst>) {
        let Some(Terminator::Ret(Some((_, Operand::Local(v))))) = f.block(b).map(|b| &b.terminator) else {
            panic!("{b:?} returns a value")
        };
        (*v, f.blocks.iter().flat_map(|b| &b.instrs).find(|i| i.result == Some(*v)).map(|i| i.inst.clone()))
    }

    #[test]
    fn a_landing_returns_the_results_whole_not_their_halves() {
        let (mut f, [first, second], [entry, ok]) = landing_of_halves(false);
        let land = BlockId(3);
        assert!(join_halves(&mut f));
        dce::run(&mut f);
        let (_, inst) = returned(&f, land);
        let incoming = vec![(Operand::Local(first), entry), (Operand::Local(second), ok)];
        assert_eq!(inst, Some(Inst::Phi { ty: Type::I64, incoming }));
        let halves = f.blocks.iter().flat_map(|b| &b.instrs).filter(|i| matches!(i.inst, Inst::Bin { op: BinOp::LShr | BinOp::Shl, .. }));
        assert_eq!(halves.count(), 0, "the halves are gone");
    }

    #[test]
    fn halves_of_two_values_stay_halves() {
        let (mut f, ..) = landing_of_halves(true);
        assert!(!join_halves(&mut f));
        assert!(matches!(returned(&f, BlockId(3)).1, Some(Inst::Bin { op: BinOp::Or, .. })));
    }
}
