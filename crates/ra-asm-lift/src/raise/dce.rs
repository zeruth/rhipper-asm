//! Dead-code elimination: an instruction with no side effect whose value nothing uses goes, and
//! a phi whose incoming values are all the same (or itself) becomes that value.

use std::collections::HashMap;

use ra_asm_ir::{Function, Inst, Operand, ValueId};

/// Runs to a fixpoint.
pub fn run(f: &mut Function) {
    loop {
        let mut changed = prune_trivial_phis(f);
        let counts = f.use_counts();
        for b in &mut f.blocks {
            let before = b.instrs.len();
            b.instrs.retain(|i| match i.result {
                Some(v) if !i.inst.has_side_effects() => counts.get(&v).is_some_and(|&c| c > 0),
                _ => true,
            });
            changed |= b.instrs.len() != before;
        }
        if !changed {
            return;
        }
    }
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
