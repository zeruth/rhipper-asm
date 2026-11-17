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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use ra_asm_ir::{FnType, FunctionBuilder, ICond, Inst, Operand, Terminator, Type};

    /// `f(x, c)`: `diamonds` ifs in a row, each joining in a phi of the value before it from both
    /// arms, as the promotion writes a register no arm touches; the last phi is returned.
    fn chain_of_trivial_phis(diamonds: usize) -> ra_asm_ir::Function {
        let mut b = FunctionBuilder::new("f", FnType::new(Type::I64, vec![Type::I64, Type::I64]), &["x", "c"]);
        let entry = b.block("entry");
        let (x, c) = (b.param(0), b.param(1));
        let cond = b.icmp(entry, ICond::Ne, Type::I64, c, 0i64, "cond");
        let mut head = entry;
        let mut value = Operand::Local(x);
        for k in 0..diamonds {
            let (then, els, join) = (b.block(&format!("then.{k}")), b.block(&format!("else.{k}")), b.block(&format!("join.{k}")));
            b.terminate(head, Terminator::CondBr { cond: cond.into(), then, els });
            b.terminate(then, Terminator::Br(join));
            b.terminate(els, Terminator::Br(join));
            let phi = b.value(join, Inst::Phi { ty: Type::I64, incoming: vec![(value.clone(), then), (value, els)] }, "rsi.phi");
            value = Operand::Local(phi);
            head = join;
        }
        b.terminate(head, Terminator::Ret(Some((Type::I64, value))));
        b.finish()
    }

    /// A chain of a hundred trivial phis, each the one before it, collapses to the parameter: no
    /// use is left of a phi the pass removed, however far down the chain it is.
    #[test]
    fn a_long_chain_of_trivial_phis_leaves_no_use_of_a_removed_phi() {
        let mut f = chain_of_trivial_phis(100);
        super::run(&mut f);
        let defined: HashSet<_> =
            f.params.iter().copied().chain(f.blocks.iter().flat_map(|b| b.instrs.iter().filter_map(|i| i.result))).collect();
        for b in &f.blocks {
            for v in b.instrs.iter().flat_map(|i| i.inst.uses()).chain(b.terminator.uses()) {
                assert!(defined.contains(&v), "block `{}` uses `{}`, which nothing defines", b.name, f.value_name(v));
            }
        }
        let last = f.blocks.iter().find(|b| matches!(b.terminator, Terminator::Ret(_))).expect("a return");
        assert_eq!(last.terminator, Terminator::Ret(Some((Type::I64, Operand::Local(f.params[0])))));
    }

    /// Two phis that are only each other have no value: both go, and their use reads `undef`.
    #[test]
    fn phis_that_are_only_each_other_become_undef() {
        let mut b = FunctionBuilder::new("g", FnType::new(Type::I64, vec![Type::I64]), &["c"]);
        let (entry, l1, l2, out) = (b.block("entry"), b.block("l1"), b.block("l2"), b.block("out"));
        let c = b.param(0);
        let cond = b.icmp(entry, ICond::Ne, Type::I64, c, 0i64, "cond");
        b.terminate(entry, Terminator::Br(l1));
        let p1 = b.value(l1, Inst::Phi { ty: Type::I64, incoming: Vec::new() }, "p1");
        b.terminate(l1, Terminator::Br(l2));
        let p2 = b.value(l2, Inst::Phi { ty: Type::I64, incoming: vec![(Operand::Local(p1), l1)] }, "p2");
        b.terminate(l2, Terminator::CondBr { cond: cond.into(), then: l1, els: out });
        b.terminate(out, Terminator::Ret(Some((Type::I64, Operand::Local(p2)))));
        let mut f = b.finish();
        // The first phi takes the second from both ways in, which the builder cannot name ahead.
        for blk in &mut f.blocks {
            for i in &mut blk.instrs {
                if i.result == Some(p1)
                    && let Inst::Phi { incoming, .. } = &mut i.inst
                {
                    *incoming = vec![(Operand::Local(p2), entry), (Operand::Local(p2), l2)];
                }
            }
        }
        super::run(&mut f);
        let last = f.blocks.iter().find(|b| b.name == "out").expect("the exit");
        assert_eq!(last.terminator, Terminator::Ret(Some((Type::I64, Operand::Undef))));
    }
}
