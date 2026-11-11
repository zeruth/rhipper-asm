//! Dead-code elimination: an instruction is live when it has a side effect, when a terminator
//! uses it, or when a live instruction uses it; everything else goes, cycles of phis included. A
//! phi whose incoming values are all one value (or itself) becomes that value.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{Function, Inst, Operand, ValueId};

/// Runs to a fixpoint.
pub fn run(f: &mut Function) {
    loop {
        let mut changed = prune_trivial_phis(f);
        changed |= sweep(f);
        if !changed {
            return;
        }
    }
}

/// Removes the pure instructions nothing live reaches.
fn sweep(f: &mut Function) -> bool {
    let mut uses_of: HashMap<ValueId, Vec<ValueId>> = HashMap::new(); // value -> the values it uses
    let mut roots: Vec<ValueId> = Vec::new();
    let mut root_uses: Vec<ValueId> = Vec::new();
    for b in &f.blocks {
        for i in &b.instrs {
            match i.result {
                Some(v) => {
                    uses_of.insert(v, i.inst.uses());
                    if i.inst.has_side_effects() {
                        roots.push(v);
                    }
                }
                None => root_uses.extend(i.inst.uses()),
            }
        }
        root_uses.extend(b.terminator.uses());
    }
    let mut live: HashSet<ValueId> = HashSet::new();
    let mut work: Vec<ValueId> = roots;
    work.extend(root_uses);
    while let Some(v) = work.pop() {
        if !live.insert(v) {
            continue;
        }
        if let Some(used) = uses_of.get(&v) {
            work.extend(used.iter().copied());
        }
    }
    let mut changed = false;
    for b in &mut f.blocks {
        let before = b.instrs.len();
        b.instrs.retain(|i| match i.result {
            Some(v) => live.contains(&v) || i.inst.has_side_effects(),
            None => true,
        });
        changed |= b.instrs.len() != before;
    }
    changed
}

/// Replaces each phi whose incoming values are one value (ignoring itself) by that value.
fn prune_trivial_phis(f: &mut Function) -> bool {
    let mut map: HashMap<ValueId, Operand> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            let (Some(result), Inst::Phi { incoming, .. }) = (i.result, &i.inst) else { continue };
            let mut unique: Option<&Operand> = None;
            let mut trivial = true;
            for (o, _) in incoming {
                if *o == Operand::Local(result) {
                    continue;
                }
                match unique {
                    None => unique = Some(o),
                    Some(u) if u == o => {}
                    Some(_) => {
                        trivial = false;
                        break;
                    }
                }
            }
            if trivial {
                map.insert(result, unique.cloned().unwrap_or(Operand::Undef));
            }
        }
    }
    if map.is_empty() {
        return false;
    }
    for b in &mut f.blocks {
        b.instrs.retain(|i| !i.result.is_some_and(|v| map.contains_key(&v)));
    }
    f.replace_uses(&map);
    true
}
