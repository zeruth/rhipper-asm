//! The frame's locals as values. The frame step gives each private slot an `alloca` of its own;
//! one that is only loaded and stored at its own type, with its address going nowhere else, is a
//! variable no call or pointer can reach, so its loads become the values stored (`mem2reg`): a phi
//! where stores from different paths meet, `undef` where none was. A parameter the code spills to
//! its home and reads back after a call is the parameter again.

use std::collections::{BTreeSet, HashMap, HashSet};

use ra_asm_ir::{BlockId, Function, Inst, Instruction, Operand, Type, ValueId};

/// Promotes the promotable allocas of the entry block; whether anything changed.
pub fn run(f: &mut Function) -> bool {
    let Some(entry) = f.blocks.first() else { return false };
    let mut candidates: HashMap<ValueId, Type> = entry
        .instrs
        .iter()
        .filter_map(|i| match (&i.inst, i.result) {
            (Inst::Alloca { ty, .. }, Some(v)) if matches!(ty, Type::Int(_) | Type::Half | Type::Float | Type::Double | Type::Ptr) => {
                Some((v, ty.clone()))
            }
            _ => None,
        })
        .collect();
    if candidates.is_empty() {
        return false;
    }
    // Every use must be a plain load or store of the alloca's own type through it.
    for b in &f.blocks {
        for i in &b.instrs {
            match &i.inst {
                Inst::Load { ty, ptr: Operand::Local(p), volatile: false, .. } if candidates.contains_key(p) => {
                    if candidates[p] != *ty {
                        candidates.remove(p);
                    }
                }
                Inst::Store { ty, ptr: Operand::Local(p), value, volatile: false, .. } if candidates.contains_key(p) => {
                    if candidates[p] != *ty {
                        candidates.remove(p);
                    }
                    if let Some(v) = value.local() {
                        candidates.remove(&v);
                    }
                }
                inst => {
                    for v in inst.uses() {
                        candidates.remove(&v);
                    }
                }
            }
        }
        for v in b.terminator.uses() {
            candidates.remove(&v);
        }
    }
    if candidates.is_empty() {
        return false;
    }

    let order = ra_asm_ir::dom::reverse_post_order(f);
    let preds = f.predecessors();
    let idom = ra_asm_ir::dom::dominators(&order, &preds);
    let reachable: HashSet<BlockId> = order.iter().copied().collect();
    let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(n, b)| (b.id, n)).collect();

    // The dominance frontiers (Cooper, Harvey and Kennedy).
    let mut frontier: HashMap<BlockId, BTreeSet<BlockId>> = HashMap::new();
    for &b in &order {
        let ps: Vec<BlockId> = preds.get(&b).map(|ps| ps.iter().copied().filter(|p| reachable.contains(p)).collect()).unwrap_or_default();
        if ps.len() < 2 {
            continue;
        }
        let Some(&up) = idom.get(&b) else { continue };
        for p in ps {
            let mut runner = p;
            while runner != up {
                frontier.entry(runner).or_default().insert(b);
                match idom.get(&runner) {
                    Some(&next) if next != runner => runner = next,
                    _ => break,
                }
            }
        }
    }

    // A phi for each alloca at the iterated frontier of the blocks that store it.
    let allocas: Vec<ValueId> = {
        let mut v: Vec<ValueId> = candidates.keys().copied().collect();
        v.sort();
        v
    };
    let mut phis: HashMap<(BlockId, ValueId), ValueId> = HashMap::new();
    let mut phi_order: HashMap<BlockId, Vec<ValueId>> = HashMap::new();
    for &a in &allocas {
        let mut work: Vec<BlockId> = f
            .blocks
            .iter()
            .filter(|b| reachable.contains(&b.id))
            .filter(|b| b.instrs.iter().any(|i| matches!(&i.inst, Inst::Store { ptr: Operand::Local(p), .. } if *p == a)))
            .map(|b| b.id)
            .collect();
        let mut placed: HashSet<BlockId> = HashSet::new();
        while let Some(b) = work.pop() {
            for &d in frontier.get(&b).into_iter().flatten() {
                if placed.insert(d) {
                    let name = format!("{}.phi", f.value_name(a));
                    let v = f.new_value(&name, candidates[&a].clone());
                    phis.insert((d, a), v);
                    phi_order.entry(d).or_default().push(a);
                    work.push(d);
                }
            }
        }
    }

    // Renaming down the dominator tree: each load is the value its alloca holds there.
    let mut children: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for &b in &order {
        if let Some(&up) = idom.get(&b)
            && up != b
        {
            children.entry(up).or_default().push(b);
        }
    }
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let mut incoming: HashMap<ValueId, Vec<(Operand, BlockId)>> = HashMap::new();
    let resolve = |o: &Operand, replace: &HashMap<ValueId, Operand>| -> Operand {
        let mut o = o.clone();
        while let Some(next) = o.local().and_then(|l| replace.get(&l)) {
            o = next.clone();
        }
        o
    };
    // (block, the values held on entry to it)
    let start: HashMap<ValueId, Operand> = allocas.iter().map(|&a| (a, Operand::Undef)).collect();
    let mut stack: Vec<(BlockId, HashMap<ValueId, Operand>)> = vec![(order[0], start)];
    while let Some((b, mut held)) = stack.pop() {
        for &a in phi_order.get(&b).into_iter().flatten() {
            held.insert(a, Operand::Local(phis[&(b, a)]));
        }
        let block = &mut f.blocks[index[&b]];
        let mut kept = Vec::with_capacity(block.instrs.len());
        for i in block.instrs.drain(..) {
            match &i.inst {
                Inst::Load { ptr: Operand::Local(p), .. } if candidates.contains_key(p) => {
                    if let Some(v) = i.result {
                        replace.insert(v, resolve(&held[p], &replace));
                    }
                }
                Inst::Store { ptr: Operand::Local(p), value, .. } if candidates.contains_key(p) => {
                    held.insert(*p, resolve(value, &replace));
                }
                Inst::Alloca { .. } if i.result.is_some_and(|v| candidates.contains_key(&v)) => {}
                _ => kept.push(i),
            }
        }
        block.instrs = kept;
        for s in block.terminator.successors() {
            for &a in phi_order.get(&s).into_iter().flatten() {
                incoming.entry(phis[&(s, a)]).or_default().push((held[&a].clone(), b));
            }
        }
        for &c in children.get(&b).into_iter().flatten() {
            stack.push((c, held.clone()));
        }
    }
    // A block no path reaches loses its loads and stores of the promoted allocas too.
    for b in f.blocks.iter_mut().filter(|b| !reachable.contains(&b.id)) {
        b.instrs.retain(|i| match &i.inst {
            Inst::Load { ptr: Operand::Local(p), .. } => {
                if candidates.contains_key(p)
                    && let Some(v) = i.result
                {
                    replace.insert(v, Operand::Undef);
                }
                !candidates.contains_key(p)
            }
            Inst::Store { ptr: Operand::Local(p), .. } => !candidates.contains_key(p),
            _ => true,
        });
    }
    // The phis, first in their blocks, with the values resolved through the loads replaced.
    for (&(b, a), &v) in &phis {
        let mut ins: Vec<(Operand, BlockId)> =
            incoming.remove(&v).unwrap_or_default().into_iter().map(|(o, p)| (resolve(&o, &replace), p)).collect();
        // A predecessor no path reaches brings nothing.
        for &p in preds.get(&b).into_iter().flatten().filter(|p| !reachable.contains(p)) {
            ins.push((Operand::Undef, p));
        }
        let ty = candidates[&a].clone();
        f.blocks[index[&b]].instrs.insert(0, Instruction { result: Some(v), inst: Inst::Phi { ty, incoming: ins }, address: None });
    }
    let resolved: HashMap<ValueId, Operand> = replace.keys().map(|&k| (k, resolve(&Operand::Local(k), &replace))).collect();
    f.replace_uses(&resolved);
    true
}
