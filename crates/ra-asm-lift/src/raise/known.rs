//! Known values: a pure instruction identical to one that dominates it becomes that one (value
//! numbering over the dominator tree), a condition that a dominating branch decided is that
//! constant on the branch's side, the operand a dominating equality compared with a constant is
//! that constant, a branch on a constant becomes a jump, and the blocks nothing reaches go. The
//! lift's own guards are where this shows: its division-by-zero check repeats the program's.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::dom::{dominators, reverse_post_order};
use ra_asm_ir::{BlockId, Function, ICond, Inst, Operand, Terminator, ValueId};

/// Runs once; returns whether anything changed.
pub fn run(f: &mut Function) -> bool {
    let mut changed = number_values(f);
    changed |= apply_branch_facts(f);
    changed |= fold_branches(f);
    changed
}

/// Whether an instruction's value depends on its operands alone.
fn pure(inst: &Inst) -> bool {
    !inst.has_side_effects()
        && !matches!(
            inst,
            Inst::Alloca { .. } | Inst::Phi { .. } | Inst::Load { .. } | Inst::Call { .. } | Inst::Freeze { .. } | Inst::Store { .. }
        )
}

/// Value numbering: in reverse post-order, each pure instruction looks for an identical one in
/// its block or a dominator, and becomes it.
fn number_values(f: &mut Function) -> bool {
    let order = reverse_post_order(f);
    let preds = f.predecessors();
    let idom = dominators(&order, &preds);
    let mut tables: HashMap<BlockId, HashMap<String, ValueId>> = HashMap::new();
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let resolve = |o: &mut Operand, replace: &HashMap<ValueId, Operand>| {
        while let Operand::Local(v) = o {
            let Some(r) = replace.get(v) else { break };
            *o = r.clone();
        }
    };
    for b in &order {
        let Some(bi) = f.blocks.iter().position(|x| x.id == *b) else { continue };
        let mut local: HashMap<String, ValueId> = HashMap::new();
        let mut kept = Vec::with_capacity(f.blocks[bi].instrs.len());
        for mut i in std::mem::take(&mut f.blocks[bi].instrs) {
            for o in i.inst.operands_mut() {
                resolve(o, &replace);
            }
            let Some(v) = i.result else {
                kept.push(i);
                continue;
            };
            if !pure(&i.inst) {
                kept.push(i);
                continue;
            }
            let key = format!("{:?}", i.inst);
            let mut found = local.get(&key).copied();
            let mut up = *b;
            while found.is_none() {
                let Some(&d) = idom.get(&up) else { break };
                if d == up {
                    break;
                }
                found = tables.get(&d).and_then(|t| t.get(&key)).copied();
                up = d;
            }
            if let Some(earlier) = found {
                replace.insert(v, Operand::Local(earlier));
            } else {
                local.insert(key, v);
                kept.push(i);
            }
        }
        for o in f.blocks[bi].terminator.operands_mut() {
            resolve(o, &replace);
        }
        f.blocks[bi].instrs = kept;
        tables.insert(*b, local);
    }
    if replace.is_empty() {
        return false;
    }
    f.replace_uses(&replace);
    true
}

/// A fact a branch establishes on one side: a value is a constant there.
#[derive(Clone)]
struct Fact {
    value: ValueId,
    constant: Operand,
}

/// On the side of a branch that only the branch reaches, the condition is known, and so is the
/// operand of an equality with a constant. The facts hold in the dominator subtree of that side.
fn apply_branch_facts(f: &mut Function) -> bool {
    let order = reverse_post_order(f);
    let preds = f.predecessors();
    let idom = dominators(&order, &preds);
    let mut defs: HashMap<ValueId, Inst> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Some(v) = i.result {
                defs.insert(v, i.inst.clone());
            }
        }
    }
    // The facts that hold from each block on.
    let mut facts_at: HashMap<BlockId, Vec<Fact>> = HashMap::new();
    for b in &f.blocks {
        let Terminator::CondBr { cond: Operand::Local(c), then, els } = &b.terminator else { continue };
        for (side, truth) in [(*then, true), (*els, false)] {
            if side == b.id || preds.get(&side).is_none_or(|p| p.len() != 1) {
                continue;
            }
            let mut facts = vec![Fact { value: *c, constant: Operand::Int(i128::from(truth)) }];
            if let Some(Inst::ICmp { cond, lhs: Operand::Local(x), rhs: k @ Operand::Int(_), .. }) = defs.get(c)
                && ((*cond == ICond::Eq && truth) || (*cond == ICond::Ne && !truth))
            {
                facts.push(Fact { value: *x, constant: k.clone() });
            }
            facts_at.entry(side).or_default().extend(facts);
        }
    }
    if facts_at.is_empty() {
        return false;
    }
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        // the facts of this block and of every dominator
        let mut known: HashMap<ValueId, Operand> = HashMap::new();
        let mut x = f.blocks[bi].id;
        loop {
            if let Some(facts) = facts_at.get(&x) {
                for fact in facts {
                    known.entry(fact.value).or_insert_with(|| fact.constant.clone());
                }
            }
            let Some(&up) = idom.get(&x) else { break };
            if up == x {
                break;
            }
            x = up;
        }
        if known.is_empty() {
            continue;
        }
        let block = &mut f.blocks[bi];
        for i in &mut block.instrs {
            if matches!(i.inst, Inst::Phi { .. }) {
                continue; // an incoming value belongs to the predecessor's facts, not this block's
            }
            for o in i.inst.operands_mut() {
                if let Operand::Local(v) = o
                    && let Some(k) = known.get(v)
                {
                    *o = k.clone();
                    changed = true;
                }
            }
        }
        for o in block.terminator.operands_mut() {
            if let Operand::Local(v) = o
                && let Some(k) = known.get(v)
            {
                *o = k.clone();
                changed = true;
            }
        }
    }
    changed
}

/// A branch on a constant becomes a jump, and the blocks the entry no longer reaches go, with
/// the phi entries that came from them.
fn fold_branches(f: &mut Function) -> bool {
    let mut changed = false;
    for b in &mut f.blocks {
        let next = match &b.terminator {
            Terminator::CondBr { cond: Operand::Int(c), then, els } => Some(if *c & 1 == 1 { *then } else { *els }),
            Terminator::CondBr { cond: Operand::Undef, then, .. } => Some(*then),
            Terminator::Switch { value: Operand::Int(c), default, cases, .. } => {
                Some(cases.iter().find(|(k, _)| k == c).map_or(*default, |(_, t)| *t))
            }
            _ => None,
        };
        if let Some(target) = next {
            b.terminator = Terminator::Br(target);
            changed = true;
        }
    }
    // Reachability from the entry.
    let mut reached: HashSet<BlockId> = HashSet::new();
    let mut work = vec![f.blocks[0].id];
    while let Some(b) = work.pop() {
        if !reached.insert(b) {
            continue;
        }
        if let Some(block) = f.block(b) {
            work.extend(block.terminator.successors());
        }
    }
    if reached.len() != f.blocks.len() {
        f.blocks.retain(|b| reached.contains(&b.id));
        changed = true;
    }
    // A phi keeps the entries of its predecessors only.
    let preds = f.predecessors();
    for b in &mut f.blocks {
        let mine = preds.get(&b.id).cloned().unwrap_or_default();
        for i in &mut b.instrs {
            if let Inst::Phi { incoming, .. } = &mut i.inst {
                let before = incoming.len();
                incoming.retain(|(_, from)| mine.contains(from));
                changed |= incoming.len() != before;
            }
        }
    }
    changed
}
