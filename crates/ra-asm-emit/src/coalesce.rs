//! The phis that share their variable with the values they take: a phi assigned at each way in
//! ([`crate::c`]'s direct phis) and a value one of its moves copies need one C variable between
//! them when neither is live where the other is assigned, and the copy goes. Liveness is the
//! source's: a value written inline at its use is read where the use is written.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BlockId, Function, Inst, Operand, Terminator, ValueId};

/// Where a value is read or assigned in a block, in the order the C runs them: the
/// instructions, then the moves of the block's successors' phis, then its branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Site {
    Instr(usize),
    /// The move to this phi.
    Move(ValueId),
    Term,
}

impl Site {
    /// Whether the site runs after the instruction at `i`.
    fn after(self, i: usize) -> bool {
        match self {
            Site::Instr(j) => j > i,
            Site::Move(_) | Site::Term => true,
        }
    }
}

/// Where a value is defined: a phi at its block's head, an instruction at its index.
#[derive(Debug, Clone, Copy)]
enum Def {
    Head,
    Instr(usize),
}

/// What the coalescing reads of the printer.
pub(crate) struct Printer<'a> {
    pub f: &'a Function,
    /// The phis assigned directly at each way in.
    pub direct: &'a HashSet<ValueId>,
    /// The values written inline wherever they are read (inlined values and field pointers).
    pub inline: &'a dyn Fn(ValueId) -> bool,
    /// The values that keep their own variable (parameters, allocations, struct pointers, the
    /// loops' own, the checks').
    pub keep: &'a dyn Fn(ValueId) -> bool,
    /// The moves of each block: phi and operand.
    pub moves: &'a HashMap<BlockId, Vec<(ValueId, Operand)>>,
}

/// The most values one variable holds.
const MOST: usize = 16;

/// The variable of each value that shares another's: value to the value whose name it takes.
pub(crate) fn coalesce(p: &Printer<'_>) -> HashMap<ValueId, ValueId> {
    let f = p.f;
    let mut defs: HashMap<ValueId, (BlockId, Def)> = HashMap::new();
    for b in &f.blocks {
        for (i, inst) in b.instrs.iter().enumerate() {
            if let Some(v) = inst.result {
                defs.insert(v, (b.id, if matches!(inst.inst, Inst::Phi { .. }) { Def::Head } else { Def::Instr(i) }));
            }
        }
    }
    let candidate = |v: ValueId| {
        let Some(&(_, def)) = defs.get(&v) else { return false };
        if (p.keep)(v) || (p.inline)(v) {
            return false;
        }
        match def {
            Def::Head => p.direct.contains(&v),
            Def::Instr(_) => true,
        }
    };
    // The phis' moves with a value that may share the phi's variable.
    let mut pairs: Vec<(ValueId, ValueId)> = Vec::new();
    // The pairs taken, by which a pair is taken once: a search of the list made it quadratic.
    let mut taken: HashSet<(ValueId, ValueId)> = HashSet::new();
    for b in &f.blocks {
        for (phi, o) in p.moves.get(&b.id).into_iter().flatten() {
            if let Operand::Local(v) = o
                && v != phi
                && p.direct.contains(phi)
                && candidate(*phi)
                && candidate(*v)
                && f.value_type(*phi) == f.value_type(*v)
                && taken.insert((*phi, *v))
            {
                pairs.push((*phi, *v));
            }
        }
    }
    // The phis of a `switch`'s targets that take one value from it share a variable too: the
    // block assigns it once before the `switch`, not once for each target.
    for b in f.blocks.iter().filter(|b| matches!(b.terminator, Terminator::Switch { .. })) {
        let mut first: Vec<(&Operand, ValueId)> = Vec::new();
        for (phi, o) in p.moves.get(&b.id).into_iter().flatten() {
            if !candidate(*phi) {
                continue;
            }
            match first.iter().find(|(w, q)| *w == o && f.value_type(*q) == f.value_type(*phi)) {
                Some(&(_, q)) => {
                    if q != *phi && taken.insert((q, *phi)) {
                        pairs.push((q, *phi));
                    }
                }
                None => first.push((o, *phi)),
            }
        }
    }
    if pairs.is_empty() {
        return HashMap::new();
    }
    let mut live = Live::new(p, defs);
    let mut class: HashMap<ValueId, usize> = HashMap::new();
    let mut members: Vec<Vec<ValueId>> = Vec::new();
    let class_of = |v: ValueId, class: &mut HashMap<ValueId, usize>, members: &mut Vec<Vec<ValueId>>| -> usize {
        *class.entry(v).or_insert_with(|| {
            members.push(vec![v]);
            members.len() - 1
        })
    };
    for (phi, v) in pairs {
        let (a, b) = (class_of(phi, &mut class, &mut members), class_of(v, &mut class, &mut members));
        if a == b || members[a].len() + members[b].len() > MOST {
            continue;
        }
        let clash = members[a].iter().any(|&x| members[b].iter().any(|&y| live.interfere(x, y)));
        if clash {
            continue;
        }
        let moved = std::mem::take(&mut members[b]);
        for &m in &moved {
            class.insert(m, a);
        }
        members[a].extend(moved);
    }
    let mut rename = HashMap::new();
    for group in members.iter().filter(|g| g.len() > 1) {
        // The variable is named after a phi of the group: the first in the function's order.
        let head = group.iter().copied().filter(|v| matches!(live.defs[v].1, Def::Head)).min_by_key(|v| v.0).unwrap_or(group[0]);
        for &m in group {
            if m != head {
                rename.insert(m, head);
            }
        }
    }
    rename
}

struct Live<'a, 'p> {
    p: &'p Printer<'a>,
    defs: HashMap<ValueId, (BlockId, Def)>,
    preds: HashMap<BlockId, Vec<BlockId>>,
    /// Where each value is read, as written.
    sites: HashMap<ValueId, Vec<(BlockId, Site)>>,
    /// The blocks each value is live into, as asked.
    into: HashMap<ValueId, HashSet<BlockId>>,
    /// The moves that assign each direct phi: the block and the operand.
    assigned: HashMap<ValueId, Vec<(BlockId, Operand)>>,
}

impl<'a, 'p> Live<'a, 'p> {
    fn new(p: &'p Printer<'a>, defs: HashMap<ValueId, (BlockId, Def)>) -> Self {
        let f = p.f;
        // The reads of each instruction's operands, before the inlining moves them.
        let mut direct: HashMap<ValueId, Vec<(BlockId, Site)>> = HashMap::new();
        // The values each inlined value is read by, to follow it to where it is written.
        let mut readers: HashMap<ValueId, Vec<ValueId>> = HashMap::new();
        for b in &f.blocks {
            for (i, inst) in b.instrs.iter().enumerate() {
                match (&inst.inst, inst.result) {
                    (Inst::Phi { incoming, .. }, Some(phi)) => {
                        for (o, from) in incoming {
                            if let Operand::Local(v) = o {
                                direct.entry(*v).or_default().push((*from, Site::Move(phi)));
                            }
                        }
                    }
                    (other, result) => {
                        for v in other.uses() {
                            match result {
                                Some(r) if (p.inline)(r) => readers.entry(v).or_default().push(r),
                                _ => direct.entry(v).or_default().push((b.id, Site::Instr(i))),
                            }
                        }
                    }
                }
            }
            for v in b.terminator.uses() {
                direct.entry(v).or_default().push((b.id, Site::Term));
            }
        }
        // A value read by an inlined one is read wherever that one is written.
        let mut sites: HashMap<ValueId, Vec<(BlockId, Site)>> = HashMap::new();
        let mut memo: HashMap<ValueId, Vec<(BlockId, Site)>> = HashMap::new();
        let keys: Vec<ValueId> = direct.keys().chain(readers.keys()).copied().collect::<HashSet<_>>().into_iter().collect();
        for v in keys {
            let s = Self::sites_of(v, &direct, &readers, &mut memo, 0);
            sites.insert(v, s);
        }
        let mut assigned: HashMap<ValueId, Vec<(BlockId, Operand)>> = HashMap::new();
        for (x, moves) in p.moves {
            for (phi, o) in moves {
                if p.direct.contains(phi) {
                    assigned.entry(*phi).or_default().push((*x, o.clone()));
                }
            }
        }
        Self { p, defs, preds: f.predecessors(), sites, into: HashMap::new(), assigned }
    }

    fn sites_of(
        v: ValueId,
        direct: &HashMap<ValueId, Vec<(BlockId, Site)>>,
        readers: &HashMap<ValueId, Vec<ValueId>>,
        memo: &mut HashMap<ValueId, Vec<(BlockId, Site)>>,
        depth: usize,
    ) -> Vec<(BlockId, Site)> {
        if let Some(s) = memo.get(&v) {
            return s.clone();
        }
        let mut out = direct.get(&v).cloned().unwrap_or_default();
        if depth < 64 {
            for r in readers.get(&v).into_iter().flatten() {
                out.extend(Self::sites_of(*r, direct, readers, memo, depth + 1));
            }
        }
        memo.insert(v, out.clone());
        out
    }

    /// The blocks `v` is live into.
    fn live_in(&mut self, v: ValueId) -> &HashSet<BlockId> {
        if !self.into.contains_key(&v) {
            let mut work = Vec::new();
            if let Some(&(home, def)) = self.defs.get(&v) {
                for &(b, site) in self.sites.get(&v).into_iter().flatten() {
                    let local = b == home
                        && match def {
                            Def::Head => true,
                            Def::Instr(i) => site.after(i),
                        };
                    if !local {
                        work.push(b);
                    }
                }
                let mut live = HashSet::new();
                while let Some(b) = work.pop() {
                    if live.insert(b) {
                        work.extend(self.preds.get(&b).into_iter().flatten().filter(|p| **p != home));
                    }
                }
                self.into.insert(v, live);
            } else {
                self.into.insert(v, HashSet::new());
            }
        }
        &self.into[&v]
    }

    fn live_out(&mut self, v: ValueId, b: BlockId) -> bool {
        let succs = self.p.f.block(b).map(|x| x.terminator.successors()).unwrap_or_default();
        let live = self.live_in(v);
        succs.iter().any(|s| live.contains(s))
    }

    /// Whether `v` holds a value at the site `at` of `b`: defined before it in the block, or live
    /// into the block.
    fn available(&mut self, v: ValueId, b: BlockId, at: Option<usize>) -> bool {
        let (home, def) = self.defs[&v];
        if home == b {
            return match (def, at) {
                (Def::Head, _) | (Def::Instr(_), None) => true,
                (Def::Instr(d), Some(i)) => d <= i,
            };
        }
        self.live_in(v).contains(&b)
    }

    /// Whether `v` is read after the instruction at `i` of `b`.
    fn live_after(&mut self, v: ValueId, b: BlockId, i: usize) -> bool {
        if !self.available(v, b, Some(i)) {
            return false;
        }
        let later = self.sites.get(&v).is_some_and(|s| s.iter().any(|&(x, site)| x == b && site.after(i)));
        later || self.live_out(v, b)
    }

    /// Whether `v` is read at the moves of `b` (by a move other than `phi`'s) or after them.
    fn live_at_moves(&mut self, v: ValueId, b: BlockId, phi: ValueId) -> bool {
        if !self.available(v, b, None) {
            return false;
        }
        let later = self.sites.get(&v).is_some_and(|s| {
            s.iter().any(|&(x, site)| {
                x == b
                    && match site {
                        Site::Move(q) => q != phi,
                        Site::Term => true,
                        Site::Instr(_) => false,
                    }
            })
        });
        later || self.live_out(v, b)
    }

    /// Whether `a` and `b` cannot share a variable: one is read past a place the other is
    /// assigned, but where a move copies the one into the other.
    fn interfere(&mut self, a: ValueId, b: ValueId) -> bool {
        self.clobbers(a, b) || self.clobbers(b, a)
    }

    /// Whether an assignment of `a` comes where `b` is still read, or where `b` is assigned
    /// another value too.
    fn clobbers(&mut self, a: ValueId, b: ValueId) -> bool {
        let (home, def) = self.defs[&a];
        match def {
            Def::Instr(i) => self.live_after(b, home, i),
            Def::Head => {
                let assigned = self.assigned.get(&a).cloned().unwrap_or_default();
                assigned.into_iter().any(|(x, o)| {
                    let both = self.assigned.get(&b).is_some_and(|m| m.iter().any(|(y, w)| *y == x && *w != o));
                    both || (o != Operand::Local(b) && self.live_at_moves(b, x, a))
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ra_asm_ir::{BinOp, FnType, FunctionBuilder, Module, Type};

    use super::*;

    /// `int f(int n, int m)`: a `switch` on `n` whose two cases and default each take one value
    /// into a phi of their own (`m` with `param`, else 7) and return it plus `m`, times a factor
    /// of their own.
    fn switch_handing_one_value(param: bool) -> Function {
        let mut fb = FunctionBuilder::new("f", FnType::new(Type::I32, vec![Type::I32, Type::I32]), &["n", "m"]);
        let (n, m) = (fb.param(0), fb.param(1));
        let k = if param { Operand::Local(m) } else { Operand::Int(7) };
        let entry = fb.block("entry");
        let targets = [fb.block("one"), fb.block("two"), fb.block("other")];
        fb.terminate(
            entry,
            Terminator::Switch { ty: Type::I32, value: n.into(), default: targets[2], cases: vec![(1, targets[0]), (2, targets[1])] },
        );
        for (add, t) in (3i64..).zip(targets) {
            let x = fb.value(t, Inst::Phi { ty: Type::I32, incoming: vec![(k.clone(), entry)] }, "x");
            let y = fb.bin(t, BinOp::Add, Type::I32, x, m, "y");
            let r = fb.bin(t, BinOp::Mul, Type::I32, y, add, "r");
            fb.terminate(t, Terminator::Ret(Some((Type::I32, r.into()))));
        }
        fb.finish()
    }

    fn c_of(f: Function) -> String {
        let mut m = Module::x86_64_windows("test");
        m.functions.push(f);
        crate::c::emit_c(&m, &crate::c::Options::default())
    }

    #[test]
    fn switch_targets_taking_one_constant_take_one_copy() {
        let c = c_of(switch_handing_one_value(false));
        assert_eq!(c.matches("= 7u;").count(), 1, "{c}");
        assert!(c.find("= 7u;") < c.find("switch ("), "{c}");
    }

    #[test]
    fn switch_targets_taking_one_parameter_take_one_copy() {
        let c = c_of(switch_handing_one_value(true));
        assert_eq!(c.matches(" = m;").count(), 1, "{c}");
    }
}
