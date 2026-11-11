//! Structuring: a function's blocks as a tree of statements.
//!
//! The walk goes block by block from the entry. A conditional branch becomes an `if` whose arms
//! run to the branch's immediate post-dominator, the join; a `switch` likewise; a loop header (the
//! target of a back edge) wraps its body in a loop, where an edge back to the header is `continue`
//! and an edge to the loop's exit is `break`. A block reached a second time, or by an edge the
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
    Switch {
        ty: Type,
        value: Operand,
        cases: Vec<(Vec<i128>, Vec<Stmt>)>,
        default: Vec<Stmt>,
    },
    Return(Option<(Type, Operand)>),
    Unreachable,
}

/// A structured function: the statements, and the blocks that need a label.
pub struct Structured {
    pub body: Vec<Stmt>,
    pub labels: HashSet<BlockId>,
}

/// The dominance and loop facts of a graph.
pub struct Graph {
    pub order: Vec<BlockId>,
    pub preds: HashMap<BlockId, Vec<BlockId>>,
    pub succs: HashMap<BlockId, Vec<BlockId>>,
    pub idom: HashMap<BlockId, BlockId>,
    pub ipdom: HashMap<BlockId, Option<BlockId>>,
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
        let ipdom = post_dominators(f, &order, &succs);
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
        Self { order, preds, succs, idom, ipdom, loops }
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
        match exits.as_slice() {
            [one] => Some(*one),
            // Several exits: the header's post-dominator, when the loop leaves to it.
            _ => self.ipdom.get(&header).copied().flatten().filter(|j| exits.contains(j)),
        }
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
    /// How an edge to `target` is written from inside the current context.
    fn edge(&mut self, target: BlockId, stop: Option<BlockId>) -> Option<Stmt> {
        if Some(target) == stop {
            return None;
        }
        if let Some(&(header, exit)) = self.loops.last() {
            if target == header {
                return Some(Stmt::Continue);
            }
            if Some(target) == exit {
                return Some(Stmt::Break);
            }
        }
        if self.emitted.contains(&target) {
            self.labels.insert(target);
            return Some(Stmt::Goto(target));
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
            // A jump the rules cover: continue, break, or a goto to a block already placed.
            if let Some(stmt) = self.edge(cur, stop) {
                out.push(stmt);
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
                    Some(stmt) => out.push(stmt),
                    None => {
                        if Some(t) != stop {
                            out.extend(self.run(t, stop));
                        }
                    }
                }
            }
            Terminator::CondBr { cond, then, els } => {
                let (cond, then, els) = self.fold_condition(Cond::Is(cond.clone()), *then, *els);
                let join = self.join_of(cur, stop);
                let then_body = self.arm(then, join, stop);
                let els_body = self.arm(els, join, stop);
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
                // Cases that share a target share an arm.
                let mut arms: Vec<(Vec<i128>, BlockId)> = Vec::new();
                for (v, b) in cases {
                    match arms.iter_mut().find(|(_, t)| *t == b) {
                        Some((vs, _)) => vs.push(v),
                        None => arms.push((vec![v], b)),
                    }
                }
                let mut case_stmts = Vec::new();
                for (values, target) in arms {
                    let body = self.arm(target, join, stop);
                    case_stmts.push((values, body));
                }
                let default_body = self.arm(default, join, stop);
                out.push(Stmt::Switch { ty, value, cases: case_stmts, default: default_body });
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
    fn fold_condition(&mut self, mut cond: Cond, mut then: BlockId, mut els: BlockId) -> (Cond, BlockId, BlockId) {
        for _ in 0..16 {
            if let Some((c2, t2, e2)) = self.condition_only(then) {
                if e2 == els {
                    cond = Cond::And(Box::new(cond), Box::new(Cond::Is(c2)));
                    self.emitted.insert(then);
                    then = t2;
                    continue;
                }
                if t2 == els {
                    cond = Cond::And(Box::new(cond), Box::new(Cond::Not(Box::new(Cond::Is(c2)))));
                    self.emitted.insert(then);
                    then = e2;
                    continue;
                }
            }
            if let Some((c2, t2, e2)) = self.condition_only(els) {
                if t2 == then {
                    cond = Cond::Or(Box::new(cond), Box::new(Cond::Is(c2)));
                    self.emitted.insert(els);
                    els = e2;
                    continue;
                }
                if e2 == then {
                    cond = Cond::Or(Box::new(cond), Box::new(Cond::Not(Box::new(Cond::Is(c2)))));
                    self.emitted.insert(els);
                    els = t2;
                    continue;
                }
            }
            break;
        }
        (cond, then, els)
    }

    /// A block that only computes the condition of its own branch: pure instructions, each used
    /// once inside the block, one predecessor, no phi fed from it, not a loop header.
    fn condition_only(&self, b: BlockId) -> Option<(Operand, BlockId, BlockId)> {
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
        for s in [*then, *els] {
            if let Some(succ) = self.f.block(s)
                && succ
                    .instrs
                    .iter()
                    .any(|i| matches!(&i.inst, ra_asm_ir::Inst::Phi { incoming, .. } if incoming.iter().any(|(_, p)| *p == b)))
            {
                return None;
            }
        }
        Some((cond.clone(), *then, *els))
    }

    /// The join of a branch at `b`: its immediate post-dominator, inside the current region.
    fn join_of(&self, b: BlockId, stop: Option<BlockId>) -> Option<BlockId> {
        let j = self.g.ipdom.get(&b).copied().flatten();
        // A join beyond the current loop or region is reached by break or by the caller.
        match j {
            Some(j) if Some(j) == stop => stop,
            Some(j) if self.loops.last().is_some_and(|(_, exit)| Some(j) == *exit) => Some(j),
            other => other,
        }
    }

    /// One arm of a branch: the statements from `target` to `join`, or the jump that gets there.
    fn arm(&mut self, target: BlockId, join: Option<BlockId>, stop: Option<BlockId>) -> Vec<Stmt> {
        if Some(target) == join {
            return Vec::new();
        }
        if let Some(stmt) = self.edge(target, stop) {
            return vec![stmt];
        }
        if let Some(j) = join {
            if let Some(stmt) = self.edge(j, stop)
                && matches!(stmt, Stmt::Goto(_))
            {
                // The join is placed already: the arm runs to it and jumps.
                let mut body = self.run(target, join);
                body.push(stmt);
                return body;
            }
            return self.run(target, join);
        }
        self.run(target, stop)
    }
}

/// Reverse post-order of the reachable blocks.
/// Immediate post-dominators over the reversed graph with a virtual exit after every block that
/// returns or never continues. `None` for a block from which no exit is reached.
fn post_dominators(f: &Function, order: &[BlockId], succs: &HashMap<BlockId, Vec<BlockId>>) -> HashMap<BlockId, Option<BlockId>> {
    let exit = BlockId(u32::MAX);
    let mut rsuccs: HashMap<BlockId, Vec<BlockId>> = HashMap::new(); // the reversed graph's successors = preds
    let mut rpreds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for &b in order {
        let mut s = succs.get(&b).cloned().unwrap_or_default();
        if s.is_empty() || matches!(f.block(b).map(|b| &b.terminator), Some(Terminator::Ret(_) | Terminator::Unreachable)) {
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
