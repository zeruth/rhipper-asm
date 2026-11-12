//! Structuring: a function's blocks as a tree of statements.
//!
//! The walk goes block by block from the entry. A conditional branch becomes an `if` whose arms
//! run to the branch's immediate post-dominator, the join; a `switch` likewise, where a case whose
//! region runs into another case's block falls through to it; a loop header (the
//! target of a back edge) wraps its body in a loop, where an edge back to the header is `continue`
//! and an edge to the loop's exit is `break`. A block that never continues (a trap, a call that
//! does not return) is no exit for the joins: the paths that go on meet without it. A short block
//! reached a second time whose own exit the context can name (a shared return block, a loop's
//! latch) is written again in place; any other block reached a second time, or by an edge the
//! rules do not cover, is reached by `goto`. The fixtures structure with no `goto`; code with
//! irreducible control flow keeps them, as C allows.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::dom::{dominators, reverse_post_order};
use ra_asm_ir::{BlockId, Function, Operand, Terminator, Type};

/// A condition: an `i1` value, or a short-circuit combination of them, as the source wrote it
/// before the compiler split it into branches.
#[derive(Debug, Clone)]
pub enum Cond {
    Is(Operand),
    Not(Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

/// One statement of the structured body.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// The straight-line instructions of a block, with its label when something jumps to it.
    Block(BlockId),
    /// The instructions of a block written a second time, in place of a jump to it; no label.
    Copy(BlockId),
    /// The values the successors' phis take from a block whose condition was folded into the
    /// branch above it: the head of the arm the block fell through to.
    Moves(BlockId),
    If {
        cond: Cond,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    /// `for (;;) { body }`.
    Loop {
        header: BlockId,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Goto(BlockId),
    /// The cases in the order they are written: a case that falls through sits right before the
    /// case it runs into.
    Switch {
        ty: Type,
        value: Operand,
        cases: Vec<Case>,
    },
    Return(Option<(Type, Operand)>),
    Unreachable,
}

/// One arm of a `switch`: the values that select it (and whether the default does too), its
/// body, and whether the body runs on into the next case's instead of leaving the `switch`.
#[derive(Debug, Clone)]
pub struct Case {
    pub values: Vec<i128>,
    pub default: bool,
    pub body: Vec<Stmt>,
    pub falls_through: bool,
}

/// A structured function: the statements, and the blocks that need a label.
pub struct Structured {
    pub body: Vec<Stmt>,
    pub labels: HashSet<BlockId>,
}

impl Structured {
    /// The blocks written a second time in place of a jump: the function table counts them.
    pub fn copies(&self) -> usize {
        count(&self.body, &|s| matches!(s, Stmt::Copy(_)))
    }

    /// The jumps the structuring could not turn into a statement: the grade counts them.
    pub fn gotos(&self) -> usize {
        count(&self.body, &|s| matches!(s, Stmt::Goto(_)))
    }
}

/// The statements of `stmts` and of every statement nested in them that are `this`.
fn count(stmts: &[Stmt], this: &dyn Fn(&Stmt) -> bool) -> usize {
    stmts
        .iter()
        .map(|s| {
            let nested = match s {
                Stmt::If { then, els, .. } => count(then, this) + count(els, this),
                Stmt::Loop { body, .. } => count(body, this),
                Stmt::Switch { cases, .. } => cases.iter().map(|c| count(&c.body, this)).sum::<usize>(),
                _ => 0,
            };
            nested + usize::from(this(s))
        })
        .sum()
}

/// The most instructions a block may have to be written a second time instead of reached by
/// `goto`: a few source statements, which unoptimized code spends several loads and stores on
/// each. A longer block keeps its `goto` in C and stops the Rust port, which names it.
pub const COPY_LIMIT: usize = 16;

/// The dominance and loop facts of a graph.
pub struct Graph {
    pub order: Vec<BlockId>,
    pub preds: HashMap<BlockId, Vec<BlockId>>,
    pub succs: HashMap<BlockId, Vec<BlockId>>,
    pub idom: HashMap<BlockId, BlockId>,
    pub ipdom: HashMap<BlockId, Option<BlockId>>,
    /// The post-dominators of the paths that do not return: the join of a branch one of whose
    /// arms returns early, which `ipdom` has none for.
    pub ipdom_live: HashMap<BlockId, Option<BlockId>>,
    /// Loop header -> the blocks of its natural loop.
    pub loops: HashMap<BlockId, HashSet<BlockId>>,
}

impl Graph {
    pub fn of(f: &Function) -> Self {
        let order = reverse_post_order(f);
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        let mut succs: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for b in &f.blocks {
            let s = b.terminator.successors();
            for t in &s {
                let p = preds.entry(*t).or_default();
                if !p.contains(&b.id) {
                    p.push(b.id);
                }
            }
            succs.insert(b.id, s);
        }
        let idom = dominators(&order, &preds);
        let ipdom = post_dominators(f, &order, &succs, false);
        let ipdom_live = post_dominators(f, &order, &succs, true);
        // Natural loops: a back edge b -> h where h dominates b; the body is h plus what reaches b
        // without passing h.
        let mut loops: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
        for &b in &order {
            for &h in succs.get(&b).map_or(&Vec::new(), |s| s) {
                if dominates(&idom, h, b) {
                    let body = loops.entry(h).or_default();
                    body.insert(h);
                    let mut stack = vec![b];
                    while let Some(x) = stack.pop() {
                        if body.insert(x) {
                            stack.extend(preds.get(&x).map_or(&Vec::new(), |p| p).iter().copied());
                        }
                    }
                }
            }
        }
        Self { order, preds, succs, idom, ipdom, ipdom_live, loops }
    }

    /// The block a loop leaves to: the one block outside the loop its blocks branch to, if one.
    pub fn loop_exit(&self, header: BlockId) -> Option<BlockId> {
        let body = self.loops.get(&header)?;
        let mut exits: Vec<BlockId> = Vec::new();
        for b in body {
            for s in self.succs.get(b).map_or(&Vec::new(), |s| s) {
                if !body.contains(s) && !exits.contains(s) {
                    exits.push(*s);
                }
            }
        }
        if let [one] = exits.as_slice() {
            return Some(*one);
        }
        // Several exits: the header's first post-dominator outside the loop, when the loop
        // leaves to it (a trap inside the loop is no exit, so the chain starts in the body).
        let mut x = self.ipdom.get(&header).copied().flatten();
        while let Some(j) = x {
            if !body.contains(&j) {
                return exits.contains(&j).then_some(j);
            }
            x = self.ipdom.get(&j).copied().flatten();
        }
        None
    }
}

fn dominates(idom: &HashMap<BlockId, BlockId>, a: BlockId, mut b: BlockId) -> bool {
    loop {
        if a == b {
            return true;
        }
        match idom.get(&b) {
            Some(&d) if d != b => b = d,
            _ => return false,
        }
    }
}

/// The context of a walk.
struct Walk<'a> {
    f: &'a Function,
    g: &'a Graph,
    emitted: HashSet<BlockId>,
    labels: HashSet<BlockId>,
    /// The enclosing loops, innermost last: header and exit.
    loops: Vec<(BlockId, Option<BlockId>)>,
}

/// Structures `f`.
pub fn structure(f: &Function, g: &Graph) -> Structured {
    let mut w = Walk { f, g, emitted: HashSet::new(), labels: HashSet::new(), loops: Vec::new() };
    let entry = f.blocks[0].id;
    let body = w.run(entry, None);
    Structured { body, labels: w.labels }
}

impl Walk<'_> {
    /// The jump to `target` the context names without a label: nothing at the region's end,
    /// `continue` or `break` inside a loop. `None` when it has no such name.
    fn jump(&self, target: BlockId, stop: Option<BlockId>) -> Option<Vec<Stmt>> {
        if Some(target) == stop {
            return Some(Vec::new());
        }
        if let Some(&(header, exit)) = self.loops.last() {
            if target == header {
                return Some(vec![Stmt::Continue]);
            }
            if Some(target) == exit {
                return Some(vec![Stmt::Break]);
            }
        }
        None
    }

    /// How an edge to `target` is written from inside the current context: the jump the context
    /// names, or for a block placed already, a copy of it when it is short and its own exit has a
    /// name here, else a `goto`. `None` when the target is the region's end or is not placed yet.
    fn edge(&mut self, target: BlockId, stop: Option<BlockId>) -> Option<Vec<Stmt>> {
        if let Some(stmts) = self.jump(target, stop) {
            return (!stmts.is_empty()).then_some(stmts);
        }
        if !self.emitted.contains(&target) {
            return None;
        }
        if let Some(tail) = self.copy_tail(target, stop, 0) {
            let mut out = vec![Stmt::Copy(target)];
            out.extend(tail);
            return Some(out);
        }
        self.labels.insert(target);
        Some(vec![Stmt::Goto(target)])
    }

    /// What follows a copy of `target`: its return, the jump its branch makes from here, or for a
    /// conditional branch an `if` of the two; a jump to a short block placed already is a copy of
    /// that block in turn, a few deep (the shared tail of a `switch` with fall-through cases, a
    /// loop's latch that tests the count). `None` when the block is long, a loop header, or
    /// leaves to a place this context cannot name.
    fn copy_tail(&self, target: BlockId, stop: Option<BlockId>, depth: usize) -> Option<Vec<Stmt>> {
        let block = self.f.block(target)?;
        if self.g.loops.contains_key(&target) || block.instrs.len() > COPY_LIMIT {
            return None;
        }
        match &block.terminator {
            Terminator::Ret(v) => Some(vec![Stmt::Return(v.clone())]),
            Terminator::Unreachable => Some(vec![Stmt::Unreachable]),
            Terminator::Br(t) => self.tail_to(*t, stop, depth),
            Terminator::CondBr { cond, then, els } => {
                let then = self.tail_to(*then, stop, depth)?;
                let els = self.tail_to(*els, stop, depth)?;
                Some(vec![Stmt::If { cond: Cond::Is(cond.clone()), then, els }])
            }
            Terminator::Switch { .. } => None,
        }
    }

    /// The way from a copied block to `t`: the jump the context names, or a copy of `t` when it
    /// is placed already and the chain stays short.
    fn tail_to(&self, t: BlockId, stop: Option<BlockId>, depth: usize) -> Option<Vec<Stmt>> {
        const CHAIN: usize = 3;
        if let Some(stmts) = self.jump(t, stop) {
            return Some(stmts);
        }
        if depth + 1 < CHAIN && self.emitted.contains(&t) {
            let mut out = vec![Stmt::Copy(t)];
            out.extend(self.copy_tail(t, stop, depth + 1)?);
            return Some(out);
        }
        None
    }

    /// The statements from `start` up to (not including) `stop`.
    fn run(&mut self, start: BlockId, stop: Option<BlockId>) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut cur = start;
        loop {
            if Some(cur) == stop {
                return out;
            }
            // A jump the rules cover: continue, break, a copy of or a goto to a block already placed.
            if let Some(stmts) = self.edge(cur, stop) {
                out.extend(stmts);
                return out;
            }
            // A loop header not yet entered: the loop, then its exit.
            if self.g.loops.contains_key(&cur) && !self.loops.iter().any(|(h, _)| *h == cur) {
                let exit = self.g.loop_exit(cur);
                self.loops.push((cur, exit));
                let body = self.run_block_and_after(cur, None);
                self.loops.pop();
                out.push(Stmt::Loop { header: cur, body });
                match exit {
                    Some(e) if Some(e) != stop => {
                        cur = e;
                        continue;
                    }
                    _ => return out,
                }
            }
            let mut tail = self.run_block_and_after(cur, stop);
            out.append(&mut tail);
            return out;
        }
    }

    /// `cur` itself, then what follows it until `stop`.
    fn run_block_and_after(&mut self, cur: BlockId, stop: Option<BlockId>) -> Vec<Stmt> {
        let mut out = Vec::new();
        self.emitted.insert(cur);
        out.push(Stmt::Block(cur));
        let block = self.f.block(cur).expect("the block exists");
        match &block.terminator {
            Terminator::Ret(v) => out.push(Stmt::Return(v.clone())),
            Terminator::Unreachable => out.push(Stmt::Unreachable),
            Terminator::Br(t) => {
                let t = *t;
                match self.edge(t, stop) {
                    Some(stmts) => out.extend(stmts),
                    None => {
                        if Some(t) != stop {
                            out.extend(self.run(t, stop));
                        }
                    }
                }
            }
            Terminator::CondBr { cond, then, els } => {
                let (cond, then, els, last, mut then_body, mut els_body) = self.fold_condition(Cond::Is(cond.clone()), *then, *els, cur);
                // The join of the folded chain is the last condition's: an earlier one may be
                // post-dominated by a condition the chain absorbed.
                let join = self.join_of(last, stop);
                then_body.extend(self.arm(then, join, stop));
                els_body.extend(self.arm(els, join, stop));
                out.push(Stmt::If { cond, then: then_body, els: els_body });
                if let Some(j) = join
                    && Some(j) != stop
                {
                    out.extend(self.run(j, stop));
                }
            }
            Terminator::Switch { ty, value, default, cases } => {
                let (ty, value, default, cases) = (ty.clone(), value.clone(), *default, cases.clone());
                let join = self.join_of(cur, stop);
                // Cases that share a target share an arm; the default is the last arm, or the
                // arm of the case that shares its target.
                let mut arms: Vec<(Vec<i128>, bool, BlockId)> = Vec::new();
                for (v, b) in cases {
                    match arms.iter_mut().find(|(_, _, t)| *t == b) {
                        Some((vs, _, _)) => vs.push(v),
                        None => arms.push((vec![v], false, b)),
                    }
                }
                match arms.iter_mut().find(|(_, _, t)| *t == default) {
                    Some((_, is_default, _)) => *is_default = true,
                    None => arms.push((Vec::new(), true, default)),
                }
                let targets: Vec<BlockId> = arms.iter().map(|(_, _, t)| *t).collect();
                // An arm whose region runs into another arm's block falls through to it: that arm
                // is written right after it, and nothing else falls into it (a second one copies).
                let mut next: Vec<Option<usize>> = vec![None; arms.len()];
                let mut entered: HashSet<usize> = HashSet::new();
                for (i, &t) in targets.iter().enumerate() {
                    if Some(t) == join || self.emitted.contains(&t) {
                        continue;
                    }
                    if let Some(j) = self.falls_into(t, &targets, join, stop)
                        && j != i
                        && entered.insert(j)
                    {
                        next[i] = Some(j);
                    }
                }
                let mut order: Vec<usize> = Vec::new();
                for head in (0..arms.len()).filter(|i| !entered.contains(i)) {
                    let mut at = Some(head);
                    while let Some(i) = at.filter(|i| !order.contains(i)) {
                        order.push(i);
                        at = next[i];
                    }
                }
                let mut case_stmts = Vec::new();
                for i in order {
                    let (values, is_default, target) = arms[i].clone();
                    let into = next[i].map(|j| targets[j]);
                    let body = self.arm(target, into.or(join), stop);
                    case_stmts.push(Case { values, default: is_default, body, falls_through: into.is_some() });
                }
                out.push(Stmt::Switch { ty, value, cases: case_stmts });
                if let Some(j) = join
                    && Some(j) != stop
                {
                    out.extend(self.run(j, stop));
                }
            }
        }
        out
    }

    /// Folds the short-circuit chains: an arm that is only a condition whose branch rejoins the
    /// other arm becomes part of this branch's condition. `if (a) { if (b) T else E } else E` is
    /// `if (a && b) T else E`; `if (a) T else { if (b) T else E }` is `if (a || b) T else E`.
    /// A condition block that hands values to the phis of the arm it falls through to (not the
    /// arm it rejoins) folds too: the hand-over heads that arm. Returns the condition, its arms,
    /// the last block folded in (`last` when none was), and the heads of the two arms.
    #[allow(clippy::type_complexity)]
    fn fold_condition(
        &mut self,
        mut cond: Cond,
        mut then: BlockId,
        mut els: BlockId,
        mut last: BlockId,
    ) -> (Cond, BlockId, BlockId, BlockId, Vec<Stmt>, Vec<Stmt>) {
        let mut then_head = Vec::new();
        let mut els_head = Vec::new();
        for _ in 0..16 {
            if let Some((c2, t2, e2, feeds_t, feeds_e)) = self.condition_only(then) {
                if e2 == els && !feeds_e {
                    cond = Cond::And(Box::new(cond), Box::new(Cond::Is(c2)));
                    self.emitted.insert(then);
                    if feeds_t {
                        then_head.push(Stmt::Moves(then));
                    }
                    last = then;
                    then = t2;
                    continue;
                }
                if t2 == els && !feeds_t {
                    cond = Cond::And(Box::new(cond), Box::new(Cond::Not(Box::new(Cond::Is(c2)))));
                    self.emitted.insert(then);
                    if feeds_e {
                        then_head.push(Stmt::Moves(then));
                    }
                    last = then;
                    then = e2;
                    continue;
                }
            }
            if let Some((c2, t2, e2, feeds_t, feeds_e)) = self.condition_only(els) {
                if t2 == then && !feeds_t {
                    cond = Cond::Or(Box::new(cond), Box::new(Cond::Is(c2)));
                    self.emitted.insert(els);
                    if feeds_e {
                        els_head.push(Stmt::Moves(els));
                    }
                    last = els;
                    els = e2;
                    continue;
                }
                if e2 == then && !feeds_e {
                    cond = Cond::Or(Box::new(cond), Box::new(Cond::Not(Box::new(Cond::Is(c2)))));
                    self.emitted.insert(els);
                    if feeds_t {
                        els_head.push(Stmt::Moves(els));
                    }
                    last = els;
                    els = t2;
                    continue;
                }
            }
            break;
        }
        (cond, then, els, last, then_head, els_head)
    }

    /// A block that only computes the condition of its own branch: pure instructions, each used
    /// once inside the block, one predecessor, not a loop header. With the condition and the two
    /// targets, whether each target's phis take a value from the block.
    fn condition_only(&self, b: BlockId) -> Option<(Operand, BlockId, BlockId, bool, bool)> {
        let block = self.f.block(b)?;
        let Terminator::CondBr { cond, then, els } = &block.terminator else { return None };
        if self.emitted.contains(&b) || self.g.loops.contains_key(&b) || self.g.preds.get(&b).map_or(0, Vec::len) != 1 {
            return None;
        }
        let uses = self.f.use_counts();
        for i in &block.instrs {
            let v = i.result?;
            if i.inst.has_side_effects()
                || matches!(i.inst, ra_asm_ir::Inst::Phi { .. } | ra_asm_ir::Inst::Alloca { .. })
                || uses.get(&v).copied() != Some(1)
            {
                return None;
            }
            let used_here = block.instrs.iter().any(|n| n.inst.uses().contains(&v)) || block.terminator.uses().contains(&v);
            if !used_here {
                return None;
            }
        }
        // A phi in a successor that takes a value from this block needs the block's edge.
        let feeds = |s: BlockId| {
            self.f.block(s).is_some_and(|succ| {
                succ.instrs
                    .iter()
                    .any(|i| matches!(&i.inst, ra_asm_ir::Inst::Phi { incoming, .. } if incoming.iter().any(|(_, p)| *p == b)))
            })
        };
        Some((cond.clone(), *then, *els, feeds(*then), feeds(*els)))
    }

    /// The join of a branch at `b`: the nearest block every arm reaches that only `b` leads to,
    /// else its immediate post-dominator, inside the current region. When an arm returns early
    /// there is none, and the paths that go on meet at the join of the others: `if (a) { ...
    /// return; } rest`.
    fn join_of(&self, b: BlockId, stop: Option<BlockId>) -> Option<BlockId> {
        let pdom = self.g.ipdom.get(&b).copied().flatten().or_else(|| self.g.ipdom_live.get(&b).copied().flatten());
        // A block both arms reach before the post-dominator (a loop one arm enters directly and
        // the other after a guard, clang's vectorized loop with its scalar remainder) is where
        // they meet; the post-dominator is where the arm that skips it goes.
        let j = self.common_target(b, pdom).or(pdom);
        // The region's end is reached from the branch short of the join: an arm leaves the
        // region early, and the branch closes at the end.
        let j = match (j, stop) {
            (Some(j), Some(s)) if j != s && self.reaches(b, s, j) => Some(s),
            _ => j,
        };
        // A join beyond the current loop or region is reached by break or by the caller.
        match j {
            Some(j) if Some(j) == stop => stop,
            Some(j) if self.loops.last().is_some_and(|(_, exit)| Some(j) == *exit) => Some(j),
            other => other,
        }
    }

    /// The nearest block after `b` that every arm of its branch reaches on its own (not by
    /// coming round through `b` again) and that only `b` leads to, short of the post-dominator
    /// `pdom`; not an arm itself, and not a block placed already.
    fn common_target(&self, b: BlockId, pdom: Option<BlockId>) -> Option<BlockId> {
        let succs = self.g.succs.get(&b)?;
        if succs.len() < 2 {
            return None;
        }
        for &j in self.g.order.iter().skip_while(|x| **x != b).skip(1) {
            if Some(j) == pdom {
                return None;
            }
            if self.emitted.contains(&j) || self.g.idom.get(&j) != Some(&b) || succs.contains(&j) {
                continue;
            }
            if succs.iter().all(|&s| self.reaches(s, j, b)) {
                return Some(j);
            }
        }
        None
    }

    /// Whether `to` is reachable from `from` on a path that does not pass through `avoid`.
    fn reaches(&self, from: BlockId, to: BlockId, avoid: BlockId) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(x) = stack.pop() {
            if x == to {
                return true;
            }
            if x == avoid || !seen.insert(x) {
                continue;
            }
            stack.extend(self.g.succs.get(&x).into_iter().flatten().copied());
        }
        false
    }

    /// The arm of a `switch` the region of its arm at `t` runs into: the first of `targets` on
    /// `t`'s post-dominator chain short of the join, when the walk has not placed it and the
    /// context has no jump for it. Post-dominance is what makes the fall-through sound: every
    /// path from `t` (through a nested branch, a loop, a second edge from deeper in the arm)
    /// passes the target before anything after it, so the arm walked with the target as its
    /// stop reaches it only as the end of its region, never by a `goto` to a placed block, and
    /// the next case takes over where every path arrives.
    fn falls_into(&self, t: BlockId, targets: &[BlockId], join: Option<BlockId>, stop: Option<BlockId>) -> Option<usize> {
        let mut x = self.g.ipdom.get(&t).copied().flatten();
        while let Some(b) = x {
            if Some(b) == join || Some(b) == stop {
                return None;
            }
            if let Some(j) = targets.iter().position(|u| *u == b) {
                let placed = self.emitted.contains(&b) || self.g.loops.contains_key(&b) || self.jump(b, stop).is_some();
                return (!placed).then_some(j);
            }
            x = self.g.ipdom.get(&b).copied().flatten();
        }
        None
    }

    /// One arm of a branch: the statements from `target` to `join`, or the jump that gets there.
    fn arm(&mut self, target: BlockId, join: Option<BlockId>, stop: Option<BlockId>) -> Vec<Stmt> {
        if Some(target) == join {
            return Vec::new();
        }
        // The arm ends at the join: a copy of a placed block that leaves to it needs no jump.
        if let Some(stmts) = self.edge(target, join.or(stop)) {
            return stmts;
        }
        if let Some(j) = join {
            if let Some(stmts) = self.edge(j, stop)
                && matches!(stmts.first(), Some(Stmt::Goto(_) | Stmt::Copy(_)))
            {
                // The join is placed already: the arm runs to it and jumps, or writes it again.
                let mut body = self.run(target, join);
                if !ends_in_jump(&body) {
                    body.extend(stmts);
                }
                return body;
            }
            return self.run(target, join);
        }
        self.run(target, stop)
    }
}

/// A loop whose body ends in the test of its back edge, as the compiler rotates one: the
/// statements before the test and the condition that goes round again, `do { body } while
/// (cond)`. A `continue` elsewhere in the body would skip the test, so only a body without one
/// has the form.
pub fn do_while(body: &[Stmt]) -> Option<(&[Stmt], Cond)> {
    let (rest, cond, then, els, tail) = match body {
        [rest @ .., Stmt::If { cond, then, els }, tail @ (Stmt::Break | Stmt::Continue)] => (rest, cond, then, els, Some(tail)),
        [rest @ .., Stmt::If { cond, then, els }] => (rest, cond, then, els, None),
        _ => return None,
    };
    // Where an arm goes: its one jump, or the jump after the `if` when it is empty.
    let goes_round = |arm: &[Stmt]| match (arm, tail) {
        ([Stmt::Continue], _) | ([], Some(Stmt::Continue)) => Some(true),
        ([Stmt::Break], _) | ([], Some(Stmt::Break)) => Some(false),
        _ => None,
    };
    let (then_round, els_round) = (goes_round(then)?, goes_round(els)?);
    if then_round == els_round || has_continue(rest) {
        return None;
    }
    Some((rest, if then_round { cond.clone() } else { Cond::Not(Box::new(cond.clone())) }))
}

/// Whether a `continue` of the enclosing loop sits in `stmts`, at any depth short of a nested loop.
fn has_continue(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Continue => true,
        Stmt::If { then, els, .. } => has_continue(then) || has_continue(els),
        Stmt::Switch { cases, .. } => cases.iter().any(|c| has_continue(&c.body)),
        _ => false,
    })
}

/// Whether a body leaves by its last statement, so nothing after it would run.
fn ends_in_jump(body: &[Stmt]) -> bool {
    matches!(body.last(), Some(Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Goto(_) | Stmt::Unreachable))
}

/// Immediate post-dominators over the reversed graph with a virtual exit after every block that
/// returns. A block that never continues (a trap, a call that does not return) is left out, with
/// the edges into it: the joins are where the paths that go on meet, and a `switch` whose default
/// traps still joins after its cases. With `live`, the blocks that return are left out too, and
/// the exit sits after the last block before them. `None` for a block from which no exit is
/// reached.
fn post_dominators(
    f: &Function,
    order: &[BlockId],
    succs: &HashMap<BlockId, Vec<BlockId>>,
    live: bool,
) -> HashMap<BlockId, Option<BlockId>> {
    let exit = BlockId(u32::MAX);
    let never_continues = |b: BlockId| match f.block(b).map(|b| &b.terminator) {
        Some(Terminator::Unreachable) => true,
        Some(Terminator::Ret(_)) => live,
        _ => false,
    };
    let mut rsuccs: HashMap<BlockId, Vec<BlockId>> = HashMap::new(); // the reversed graph's successors = preds
    let mut rpreds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for &b in order {
        if never_continues(b) {
            continue;
        }
        let mut s: Vec<BlockId> = succs.get(&b).cloned().unwrap_or_default();
        s.retain(|t| !never_continues(*t));
        if s.is_empty() {
            s = vec![exit];
        }
        for t in s {
            rpreds.entry(b).or_default().push(t);
            rsuccs.entry(t).or_default().push(b);
        }
    }
    // Post-order of the reversed graph from the exit, then the dominator algorithm on it.
    let mut seen = HashSet::new();
    let mut post = Vec::new();
    let mut stack: Vec<(BlockId, Vec<BlockId>, usize)> = vec![(exit, rsuccs.get(&exit).cloned().unwrap_or_default(), 0)];
    seen.insert(exit);
    while let Some((b, ss, k)) = stack.last_mut() {
        if *k < ss.len() {
            let next = ss[*k];
            *k += 1;
            if seen.insert(next) {
                let ns = rsuccs.get(&next).cloned().unwrap_or_default();
                stack.push((next, ns, 0));
            }
        } else {
            post.push(*b);
            stack.pop();
        }
    }
    post.reverse();
    let ipdom = dominators(&post, &rpreds);
    order.iter().map(|b| (*b, ipdom.get(b).copied().filter(|d| *d != exit && d != b))).collect()
}

#[cfg(test)]
mod tests {
    use ra_asm_ir::{BinOp, FnType, FunctionBuilder, ICond, Module, Type};

    use super::*;

    /// Whether any statement of `stmts`, at any depth, is a `goto`.
    fn has_goto(stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::Goto(_) => true,
            Stmt::If { then, els, .. } => has_goto(then) || has_goto(els),
            Stmt::Loop { body, .. } => has_goto(body),
            Stmt::Switch { cases, .. } => cases.iter().any(|c| has_goto(&c.body)),
            _ => false,
        })
    }

    /// `int f(int n, int *p)`: an early return when `n` is zero, then a loop over `*p` up to `n`
    /// that leaves through two blocks (the count reached, a value found) to one return block,
    /// which the entry's early return shares. The return block has `tail` instructions.
    fn two_exits_to_one_return(tail: usize) -> Function {
        let mut fb = FunctionBuilder::new("f", FnType::new(Type::I32, vec![Type::I32, Type::Ptr]), &["n", "p"]);
        let (n, p) = (fb.param(0), fb.param(1));
        let entry = fb.block("entry");
        let header = fb.block("header");
        let body = fb.block("body");
        let latch = fb.block("latch");
        let done = fb.block("done");
        let found = fb.block("found");
        let ret = fb.block("ret");
        let none = fb.icmp(entry, ICond::Eq, Type::I32, n, 0i64, "none");
        fb.terminate(entry, Terminator::CondBr { cond: none.into(), then: ret, els: header });
        let count = fb.load(header, Type::I32, p, "i");
        let more = fb.icmp(header, ICond::Slt, Type::I32, count, n, "more");
        fb.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: done });
        let hit = fb.icmp(body, ICond::Eq, Type::I32, count, 7i64, "hit");
        fb.terminate(body, Terminator::CondBr { cond: hit.into(), then: found, els: latch });
        let next = fb.bin(latch, BinOp::Add, Type::I32, count, 1i64, "next");
        fb.store(latch, Type::I32, next, p);
        fb.terminate(latch, Terminator::Br(header));
        fb.store(done, Type::I32, 0i64, p);
        fb.terminate(done, Terminator::Br(ret));
        fb.store(found, Type::I32, 1i64, p);
        fb.terminate(found, Terminator::Br(ret));
        for k in 1..tail {
            fb.store(ret, Type::I32, i64::try_from(k).unwrap(), p);
        }
        let result = fb.load(ret, Type::I32, p, "r");
        fb.terminate(ret, Terminator::Ret(Some((Type::I32, result.into()))));
        fb.finish()
    }

    /// `int f(int n, int *p)`: a `switch` on `n` whose case 1 stores and runs into case 2's block,
    /// cases 2 and 3 and the default store and leave, all to one return of `*p`. With `shared`,
    /// cases 1 and 2 instead both run into a block that is no case, which stores before leaving.
    /// With `branching`, case 1 tests `*p` and stores one of two values before it runs on.
    fn switch_with_fall_through(shared: bool, branching: bool) -> Function {
        let mut fb = FunctionBuilder::new("f", FnType::new(Type::I32, vec![Type::I32, Type::Ptr]), &["n", "p"]);
        let (n, p) = (fb.param(0), fb.param(1));
        let entry = fb.block("entry");
        let c1 = fb.block("c1");
        let c2 = fb.block("c2");
        let c3 = fb.block("c3");
        let d = fb.block("d");
        let tail = fb.block("tail");
        let ret = fb.block("ret");
        fb.terminate(entry, Terminator::Switch { ty: Type::I32, value: n.into(), default: d, cases: vec![(1, c1), (2, c2), (3, c3)] });
        let after_c1 = if shared { tail } else { c2 };
        if branching {
            let (yes, no) = (fb.block("yes"), fb.block("no"));
            let old = fb.load(c1, Type::I32, p, "old");
            let zero = fb.icmp(c1, ICond::Eq, Type::I32, old, 0i64, "zero");
            fb.terminate(c1, Terminator::CondBr { cond: zero.into(), then: yes, els: no });
            fb.store(yes, Type::I32, 1i64, p);
            fb.terminate(yes, Terminator::Br(after_c1));
            fb.store(no, Type::I32, 11i64, p);
            fb.terminate(no, Terminator::Br(after_c1));
        } else {
            fb.store(c1, Type::I32, 1i64, p);
            fb.terminate(c1, Terminator::Br(after_c1));
        }
        fb.store(c2, Type::I32, 2i64, p);
        fb.terminate(c2, Terminator::Br(if shared { tail } else { ret }));
        fb.store(c3, Type::I32, 3i64, p);
        fb.terminate(c3, Terminator::Br(ret));
        fb.store(d, Type::I32, 9i64, p);
        fb.terminate(d, Terminator::Br(ret));
        fb.store(tail, Type::I32, 4i64, p);
        fb.terminate(tail, Terminator::Br(ret));
        let result = fb.load(ret, Type::I32, p, "r");
        fb.terminate(ret, Terminator::Ret(Some((Type::I32, result.into()))));
        fb.finish()
    }

    fn c_of(f: Function) -> String {
        let mut m = Module::x86_64_windows("test");
        m.functions.push(f);
        crate::c::emit_c(&m, &crate::c::Options::default())
    }

    fn rust_of(f: Function) -> String {
        let mut m = Module::x86_64_windows("test");
        m.functions.push(f);
        let out = crate::rust::emit_rust(&m, &crate::rust::Options::default());
        out.functions.into_iter().map(|f| f.text).collect()
    }

    #[test]
    fn case_that_runs_into_the_next_case_falls_through() {
        let f = switch_with_fall_through(false, false);
        let g = Graph::of(&f);
        let s = structure(&f, &g);
        assert_eq!(s.copies(), 0, "{:#?}", s.body);
        let Some(Stmt::Switch { cases, .. }) = s.body.get(1) else { panic!("{:#?}", s.body) };
        let order: Vec<(Vec<i128>, bool, bool)> = cases.iter().map(|c| (c.values.clone(), c.default, c.falls_through)).collect();
        assert_eq!(order, vec![(vec![1], false, true), (vec![2], false, false), (vec![3], false, false), (vec![], true, false)]);
        let c = c_of(f.clone());
        assert!(c.contains("case F_1:\n        *(uint32_t *)(p) = 1u;\n        /* falls through */\n    case F_2:\n"), "{c}");
        assert_eq!(c.matches("= 2u;").count(), 1, "{c}");
        let r = rust_of(f);
        assert!(r.contains("1u32 | 2u32 => {\n            if matches!(n, 1u32) {\n"), "{r}");
        assert_eq!(r.matches(", 2u32);").count(), 1, "{r}");
        assert!(r.contains("        _ => {\n"), "{r}");
    }

    /// A case that branches before it runs into the next case: both arms of its `if` end at the
    /// next case's block, which is written once, after it, with no `goto` and no copy.
    #[test]
    fn branching_case_falls_through_from_both_arms() {
        let f = switch_with_fall_through(false, true);
        let g = Graph::of(&f);
        let s = structure(&f, &g);
        assert!(!has_goto(&s.body), "{:#?}", s.body);
        assert!(s.labels.is_empty(), "{:#?}", s.body);
        assert_eq!(s.copies(), 0, "{:#?}", s.body);
        let Some(Stmt::Switch { cases, .. }) = s.body.get(1) else { panic!("{:#?}", s.body) };
        assert!(cases[0].falls_through && matches!(cases[0].body.as_slice(), [Stmt::Block(_), Stmt::If { .. }]), "{:#?}", cases[0]);
        assert_eq!(cases[1].values, vec![2]);
        let c = c_of(f.clone());
        assert!(c.contains("        }\n        /* falls through */\n    case F_2:\n"), "{c}");
        assert_eq!(c.matches("= 2u;").count(), 1, "{c}");
        let r = rust_of(f);
        assert!(r.contains("1u32 | 2u32 => {\n            if matches!(n, 1u32) {\n                if "), "{r}");
        assert_eq!(r.matches(", 2u32);").count(), 1, "{r}");
    }

    #[test]
    fn tail_two_cases_run_into_that_is_no_case_stays_a_copy() {
        let f = switch_with_fall_through(true, false);
        let g = Graph::of(&f);
        let s = structure(&f, &g);
        assert!(!has_goto(&s.body), "{:#?}", s.body);
        assert_eq!(s.copies(), 1, "{:#?}", s.body);
        let c = c_of(f);
        assert!(!c.contains("falls through"), "{c}");
        assert_eq!(c.matches("= 4u;").count(), 2, "{c}");
    }

    #[test]
    fn loop_with_two_exits_to_one_return_block_has_no_goto() {
        let f = two_exits_to_one_return(2);
        let g = Graph::of(&f);
        let s = structure(&f, &g);
        assert!(!has_goto(&s.body), "{:#?}", s.body);
        assert!(s.labels.is_empty());
        assert_eq!(s.copies(), 1, "{:#?}", s.body);
        let c = c_of(f);
        assert!(!c.contains("goto "), "{c}");
        assert_eq!(c.matches("return ").count(), 2, "{c}");
    }

    #[test]
    fn block_over_the_copy_limit_keeps_its_goto() {
        let f = two_exits_to_one_return(COPY_LIMIT + 1);
        let g = Graph::of(&f);
        let s = structure(&f, &g);
        assert!(has_goto(&s.body), "{:#?}", s.body);
        assert_eq!(s.copies(), 0);
        assert_eq!(s.labels.len(), 1);
        let c = c_of(f);
        assert!(c.contains("goto L_ret;") && c.contains("L_ret:;"), "{c}");
        assert_eq!(c.matches("return ").count(), 1, "{c}");
    }
}
