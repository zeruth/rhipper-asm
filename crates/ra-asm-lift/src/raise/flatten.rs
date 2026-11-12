//! Control-flow flattening undone. The obfuscators (OLLVM and its forks) turn a function into a
//! dispatcher: a loop header that switches on a state variable, one case per original block, each
//! case ending by setting the state to the number of its successor and jumping back. After the
//! compiler's own passes and the promotion the state is a phi at the header, the dispatcher a
//! `switch` or a tree of compares on it, and a state is set to a constant, to a `select` between
//! two constants (a conditional branch), or to a phi joining the constants of several cases in a
//! latch block the cases share.
//!
//! The pass follows each predecessor's state through the dispatcher to the case it selects and
//! sends the predecessor there directly: a constant reaches one block, a select becomes a branch
//! between two, a latch that joins constants is duplicated into each of its own predecessors. The
//! header's other phis carried the program's variables around the dispatcher; the blocks now
//! reached directly get phis of the values their new predecessors brought, the joins below them
//! phis over those, and every use is renamed to the definition that reaches it, so the result is
//! the original graph in SSA form. A state the pass cannot follow keeps its edge to the header,
//! which dispatches that case alone: every step is exact, and the rewrite is partial where the
//! shape is not the tools'.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::dom::{dominators, reverse_post_order};
use ra_asm_ir::{BinOp, Block, BlockId, Cast, Function, ICond, Inst, Instruction, Operand, Terminator, Type, ValueId};

use super::known;

/// Runs over `f`; returns whether a dispatcher was rebuilt.
pub fn run(f: &mut Function) -> bool {
    let mut changed = false;
    // One dispatcher at a time: a rewrite changes the graph the next is found in.
    for _ in 0..8 {
        let candidates = find_dispatchers(f);
        if !candidates.iter().any(|d| rebuild(f, d)) {
            break;
        }
        changed = true;
    }
    if changed {
        free_entry(f);
    }
    changed
}

/// Gives `f` an entry no block branches to. A rebuilt graph can loop back to the function's
/// first block, which LLVM refuses as an entry: a new entry goes before it, holding the
/// allocations, which run once.
fn free_entry(f: &mut Function) {
    let entry = f.blocks[0].id;
    if !f.blocks.iter().any(|b| b.terminator.successors().contains(&entry)) {
        return;
    }
    let id = BlockId(f.blocks.iter().map(|b| b.id.0).max().unwrap_or(0) + 1);
    let mut name = "flat.entry".to_owned();
    let mut n = 0;
    while f.blocks.iter().any(|b| b.name == name) || f.value_names.contains(&name) {
        n += 1;
        name = format!("flat.entry.{n}");
    }
    let first = &mut f.blocks[0];
    let (allocas, rest): (Vec<Instruction>, Vec<Instruction>) =
        std::mem::take(&mut first.instrs).into_iter().partition(|i| matches!(i.inst, Inst::Alloca { .. }));
    first.instrs = rest;
    // A join at the old entry comes in from the start with nothing defined yet.
    for i in &mut first.instrs {
        if let Inst::Phi { incoming, .. } = &mut i.inst {
            incoming.push((Operand::Undef, id));
        }
    }
    f.blocks.insert(0, Block { id, name, instrs: allocas, terminator: Terminator::Br(entry), address: None });
}

/// A dispatcher: the header, its state phi, the blocks that only test the state, and the header's
/// phis with what each predecessor brings.
struct Dispatcher {
    header: BlockId,
    state: ValueId,
    bits: u32,
    tree: HashSet<BlockId>,
    phis: Vec<(ValueId, Type, Vec<(Operand, BlockId)>)>,
    /// The state each predecessor of the header sets.
    state_from: HashMap<BlockId, Operand>,
    /// The values the header and the tree compute, which no longer run once the dispatcher goes.
    inside: HashSet<ValueId>,
}

/// What a state resolves to: a constant, or a choice between two constants.
enum State {
    Const(u128),
    Two(Operand, u128, u128),
}

/// Where a state leads once the dispatcher is followed: a case block, entered from the tree block
/// `via`, or a branch between two decisions, for a state set by a select. A branch on the state a
/// tree block sets has that block as its `key`: every predecessor that comes to it shares one
/// block for it, so the rebuilt loop has the one entry the original had.
enum Decision {
    Leaf { block: BlockId, via: BlockId },
    Branch { key: Option<BlockId>, cond: Operand, then: Box<Decision>, els: Box<Decision> },
}

/// Where a walk through the tree from the header arrives: a case block and the tree block it is
/// entered from, or the header again through the tree block that sets the next state.
enum Arrive {
    Leaf(BlockId, BlockId),
    Header(BlockId),
}

/// How a predecessor of the header is sent on: directly, or through copies of itself, one per
/// predecessor of its own, when its state is a phi of constants.
enum Thread {
    Direct(Decision),
    Duplicate(Vec<(BlockId, Decision)>),
}

fn bit_mask(bits: u32) -> u128 {
    if bits >= 128 { u128::MAX } else { (1u128 << bits) - 1 }
}

fn masked(k: u128, bits: u32) -> u128 {
    k & bit_mask(bits)
}

fn sign_extended(k: u128, bits: u32) -> i128 {
    if bits < 128 && bits > 0 && (k >> (bits - 1)) & 1 == 1 { (k | !bit_mask(bits)) as i128 } else { k as i128 }
}

/// The header candidates: a phi with two or more constant or select incomings in a block that
/// dispatches on it.
fn find_dispatchers(f: &Function) -> Vec<Dispatcher> {
    let defs = known::definitions(f);
    let preds = f.predecessors();
    let mut out = Vec::new();
    for b in &f.blocks {
        if preds.get(&b.id).is_none_or(|p| p.len() < 2) {
            continue;
        }
        for i in &b.instrs {
            let (Some(s), Inst::Phi { ty: Type::Int(bits), incoming }) = (i.result, &i.inst) else { continue };
            let bits = u32::from(*bits);
            if incoming.iter().filter(|(o, _)| follow(o, bits, &defs, 0).is_some()).count() < 2 {
                continue;
            }
            if let Some(d) = dispatcher_at(f, b.id, s, bits, &preds) {
                out.push(d);
            }
        }
    }
    out
}

/// Whether `o` is a constant, the state, or a test the tree computed.
fn depends(o: &Operand, state: ValueId, tests: &HashSet<ValueId>) -> bool {
    match o {
        Operand::Int(_) => true,
        Operand::Local(v) => *v == state || tests.contains(v),
        _ => false,
    }
}

/// Whether `inst` is a test of the state: a compare, a cast or a bit operation over it.
fn is_test(inst: &Inst, state: ValueId, tests: &HashSet<ValueId>) -> bool {
    match inst {
        Inst::ICmp { lhs, rhs, .. } => depends(lhs, state, tests) && depends(rhs, state, tests),
        Inst::Cast { cast: Cast::Trunc | Cast::ZExt | Cast::SExt, value, .. } => depends(value, state, tests),
        Inst::Bin { op: BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Add | BinOp::Sub, lhs, rhs, .. } => {
            depends(lhs, state, tests) && depends(rhs, state, tests)
        }
        _ => false,
    }
}

fn dispatches(t: &Terminator, state: ValueId, tests: &HashSet<ValueId>) -> bool {
    match t {
        Terminator::CondBr { cond: Operand::Local(c), .. } => tests.contains(c),
        Terminator::Switch { value, .. } => depends(value, state, tests),
        _ => false,
    }
}

/// Whether an instruction may be skipped on a path that no longer runs its block: no effect, no
/// memory, no phi.
fn skippable(inst: &Inst) -> bool {
    known::pure(inst) && !matches!(inst, Inst::Alloca { .. } | Inst::Phi { .. })
}

/// The dispatcher at `header` over `state`, when the block holds nothing but phis and tests, and
/// its tree the blocks below it that test the state alone.
fn dispatcher_at(f: &Function, header: BlockId, state: ValueId, bits: u32, preds: &HashMap<BlockId, Vec<BlockId>>) -> Option<Dispatcher> {
    let hb = f.block(header)?;
    let mut phis = Vec::new();
    let mut tests: HashSet<ValueId> = HashSet::new();
    for i in &hb.instrs {
        match (&i.inst, i.result) {
            (Inst::Phi { ty, incoming }, Some(v)) => phis.push((v, ty.clone(), incoming.clone())),
            (inst, Some(v)) if is_test(inst, state, &tests) => {
                tests.insert(v);
            }
            _ => return None,
        }
    }
    if !dispatches(&hb.terminator, state, &tests) {
        return None;
    }
    let mut state_from: HashMap<BlockId, Operand> = HashMap::new();
    for (o, b) in &phis.iter().find(|(v, ..)| *v == state)?.2 {
        state_from.entry(*b).or_insert_with(|| o.clone());
    }

    // Where each value is used: by a block's instructions or terminator, or as the state.
    let mut uses: HashMap<ValueId, Vec<(BlockId, bool)>> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            let as_state = i.result == Some(state);
            for v in i.inst.uses() {
                uses.entry(v).or_default().push((b.id, as_state));
            }
        }
        for v in b.terminator.uses() {
            uses.entry(v).or_default().push((b.id, false));
        }
    }
    // The header's tests serve the header and its tree alone; a test the function keeps for
    // itself makes the block a case, not a dispatcher.
    let mut candidates: HashSet<BlockId> = HashSet::from([header]);
    let mut work = hb.terminator.successors();
    while let Some(b) = work.pop() {
        if b == header || candidates.contains(&b) {
            continue;
        }
        let Some(block) = f.block(b) else { continue };
        let structural = block.instrs.iter().all(|i| i.result.is_some() && skippable(&i.inst))
            && matches!(block.terminator, Terminator::Br(_) | Terminator::CondBr { .. } | Terminator::Switch { .. });
        if !structural {
            continue;
        }
        candidates.insert(b);
        work.extend(block.terminator.successors());
    }
    // To a fixpoint: a tree block is reached from tree blocks only, its values serve the tree or
    // the state, and the variables pass through it unchanged.
    loop {
        let mut drop: Vec<BlockId> = Vec::new();
        for &b in &candidates {
            if b == header {
                continue;
            }
            let Some(block) = f.block(b) else { continue };
            let entered_from_tree = preds.get(&b).is_some_and(|ps| ps.iter().all(|p| candidates.contains(p)));
            let values_stay = block
                .instrs
                .iter()
                .filter_map(|i| i.result)
                .all(|v| uses.get(&v).is_none_or(|us| us.iter().all(|(at, as_state)| *as_state || candidates.contains(at))));
            let passes_variables = phis
                .iter()
                .filter(|(v, ..)| *v != state)
                .all(|(v, _, incoming)| incoming.iter().filter(|(_, from)| *from == b).all(|(o, _)| *o == Operand::Local(*v)));
            if !(entered_from_tree && values_stay && passes_variables) {
                drop.push(b);
            }
        }
        if drop.is_empty() {
            break;
        }
        for b in drop {
            candidates.remove(&b);
        }
    }
    let header_tests_stay =
        tests.iter().all(|v| uses.get(v).is_none_or(|us| us.iter().all(|(at, as_state)| *as_state || candidates.contains(at))));
    if !header_tests_stay {
        return None;
    }
    let inside = candidates
        .iter()
        .filter_map(|b| f.block(*b))
        .flat_map(|b| &b.instrs)
        .filter(|i| !matches!(i.inst, Inst::Phi { .. }))
        .filter_map(|i| i.result)
        .collect();
    Some(Dispatcher { header, state, bits, tree: candidates, phis, state_from, inside })
}

/// The state an operand sets, through casts: a constant, or a select between two.
fn follow(o: &Operand, bits: u32, defs: &HashMap<ValueId, Inst>, depth: usize) -> Option<State> {
    match o {
        Operand::Int(k) => Some(State::Const(masked(*k as u128, bits))),
        Operand::Local(v) if depth < 8 => match defs.get(v)? {
            Inst::Cast { cast, from, value, to } if matches!(cast, Cast::Trunc | Cast::ZExt | Cast::SExt) => {
                let (from_bits, to_bits) = (from.bits()?, to.bits()?);
                let through = |k: u128| masked(if *cast == Cast::SExt { sign_extended(k, from_bits) as u128 } else { k }, to_bits);
                Some(match follow(value, from_bits, defs, depth + 1)? {
                    State::Const(k) => State::Const(through(k)),
                    State::Two(c, a, b) => State::Two(c, through(a), through(b)),
                })
            }
            Inst::Select { cond, ty, then, els } => {
                let bits = ty.bits()?;
                match (follow(then, bits, defs, depth + 1)?, follow(els, bits, defs, depth + 1)?) {
                    (State::Const(a), State::Const(b)) => Some(State::Two(cond.clone(), a, b)),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// The value of a test when the state is `k`, masked to the test's width.
fn eval(o: &Operand, k: u128, d: &Dispatcher, defs: &HashMap<ValueId, Inst>, depth: usize) -> Option<u128> {
    if depth > 16 {
        return None;
    }
    match o {
        Operand::Int(c) => Some(*c as u128),
        Operand::Local(v) if *v == d.state => Some(k),
        Operand::Local(v) => match defs.get(v)? {
            Inst::Cast { cast, from, value, to } => {
                let (from_bits, to_bits) = (from.bits()?, to.bits()?);
                let x = masked(eval(value, k, d, defs, depth + 1)?, from_bits);
                Some(match cast {
                    Cast::Trunc | Cast::ZExt => masked(x, to_bits),
                    Cast::SExt => masked(sign_extended(x, from_bits) as u128, to_bits),
                    _ => return None,
                })
            }
            Inst::ICmp { cond, ty, lhs, rhs } => {
                let bits = ty.bits()?;
                let (a, b) = (masked(eval(lhs, k, d, defs, depth + 1)?, bits), masked(eval(rhs, k, d, defs, depth + 1)?, bits));
                let (sa, sb) = (sign_extended(a, bits), sign_extended(b, bits));
                let r = match cond {
                    ICond::Eq => a == b,
                    ICond::Ne => a != b,
                    ICond::Ugt => a > b,
                    ICond::Uge => a >= b,
                    ICond::Ult => a < b,
                    ICond::Ule => a <= b,
                    ICond::Sgt => sa > sb,
                    ICond::Sge => sa >= sb,
                    ICond::Slt => sa < sb,
                    ICond::Sle => sa <= sb,
                };
                Some(u128::from(r))
            }
            Inst::Bin { op, ty, lhs, rhs } => {
                let bits = ty.bits()?;
                let (a, b) = (masked(eval(lhs, k, d, defs, depth + 1)?, bits), masked(eval(rhs, k, d, defs, depth + 1)?, bits));
                Some(masked(
                    match op {
                        BinOp::And => a & b,
                        BinOp::Or => a | b,
                        BinOp::Xor => a ^ b,
                        BinOp::Add => a.wrapping_add(b),
                        BinOp::Sub => a.wrapping_sub(b),
                        _ => return None,
                    },
                    bits,
                ))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Walks the tree from the header with the state `k`.
fn walk(f: &Function, d: &Dispatcher, defs: &HashMap<ValueId, Inst>, k: u128) -> Option<Arrive> {
    let mut at = d.header;
    for _ in 0..64 {
        let block = f.block(at)?;
        let next = match &block.terminator {
            Terminator::Br(t) => *t,
            Terminator::CondBr { cond, then, els } => {
                if eval(cond, k, d, defs, 0)? & 1 == 1 {
                    *then
                } else {
                    *els
                }
            }
            Terminator::Switch { ty, value, default, cases } => {
                let bits = ty.bits()?;
                let x = masked(eval(value, k, d, defs, 0)?, bits);
                cases.iter().find(|(c, _)| masked(*c as u128, bits) == x).map_or(*default, |(_, t)| *t)
            }
            _ => return None,
        };
        if next == d.header {
            return Some(Arrive::Header(at));
        }
        if !d.tree.contains(&next) {
            return Some(Arrive::Leaf(next, at));
        }
        at = next;
    }
    None
}

/// Where `state` leads: through the tree, and through the states the tree sets on its way back
/// to the header. A case block's own phis take on a new edge what they took from the tree block
/// the edge replaces.
fn decide(f: &Function, d: &Dispatcher, defs: &HashMap<ValueId, Inst>, state: State, visited: &mut Vec<u128>) -> Option<Decision> {
    match state {
        State::Const(k) => {
            if visited.contains(&k) || visited.len() > 16 {
                return None;
            }
            visited.push(k);
            match walk(f, d, defs, k)? {
                Arrive::Leaf(leaf, via) => {
                    let block = f.block(leaf)?;
                    let joins_from_via = block.instrs.iter().all(|i| match &i.inst {
                        Inst::Phi { incoming, .. } => incoming.iter().any(|(_, from)| *from == via),
                        _ => true,
                    });
                    joins_from_via.then_some(Decision::Leaf { block: leaf, via })
                }
                Arrive::Header(from) => match follow(d.state_from.get(&from)?, d.bits, defs, 0)? {
                    State::Two(cond, a, b) => branch(f, d, defs, Some(from), cond, (a, b), visited),
                    next => decide(f, d, defs, next, visited),
                },
            }
        }
        State::Two(cond, a, b) => branch(f, d, defs, None, cond, (a, b), visited),
    }
}

/// The branch a select between two states makes. Its condition must outlive the dispatcher.
fn branch(
    f: &Function,
    d: &Dispatcher,
    defs: &HashMap<ValueId, Inst>,
    key: Option<BlockId>,
    cond: Operand,
    (a, b): (u128, u128),
    visited: &mut Vec<u128>,
) -> Option<Decision> {
    if let Operand::Local(v) = &cond
        && d.inside.contains(v)
    {
        return None;
    }
    let mut other = visited.clone();
    let then = decide(f, d, defs, State::Const(a), visited)?;
    let els = decide(f, d, defs, State::Const(b), &mut other)?;
    Some(Decision::Branch { key, cond, then: Box::new(then), els: Box::new(els) })
}

/// Whether `block` has phis: an edge into it from a block that already enters it would need two
/// entries from one predecessor.
fn has_phis(f: &Function, block: BlockId) -> bool {
    f.block(block).is_some_and(|b| b.instrs.iter().any(|i| matches!(i.inst, Inst::Phi { .. })))
}

/// A predecessor's way on: by its own state, or by the states its predecessors bring when it
/// only joins them (a latch of pure instructions the cases share).
fn thread_of(f: &Function, d: &Dispatcher, defs: &HashMap<ValueId, Inst>, p: BlockId) -> Option<Thread> {
    let block = f.block(p)?;
    let state = d.state_from.get(&p)?;
    if let Some(s) = follow(state, d.bits, defs, 0) {
        let decision = decide(f, d, defs, s, &mut Vec::new())?;
        let fits = match &block.terminator {
            Terminator::Br(_) => true,
            Terminator::CondBr { then, els, .. } => {
                let doubled = matches!(decision, Decision::Leaf { block: l, .. } if (*then == l || *els == l) && has_phis(f, l));
                (*then == d.header || *els == d.header) && !doubled
            }
            _ => false,
        };
        return fits.then_some(Thread::Direct(decision));
    }
    // A phi of this block: duplicate the block into each predecessor whose state resolves.
    let Operand::Local(v) = state else { return None };
    let Some(Inst::Phi { incoming, .. }) = defs.get(v) else { return None };
    if block.terminator != Terminator::Br(d.header) || block.instrs.len() > 48 {
        return None;
    }
    if !block.instrs.iter().all(|i| matches!(i.inst, Inst::Phi { .. }) || (i.result.is_some() && skippable(&i.inst))) {
        return None;
    }
    if !block.instrs.iter().any(|i| i.result == Some(*v)) {
        return None;
    }
    let mut copies = Vec::new();
    let mut seen: HashSet<BlockId> = HashSet::new();
    for (o, q) in incoming {
        if !seen.insert(*q) || *q == p || d.tree.contains(q) {
            continue;
        }
        let Some(s) = follow(o, d.bits, defs, 0) else { continue };
        let Some(decision) = decide(f, d, defs, s, &mut Vec::new()) else { continue };
        // The duplicate is a successor of `q` in place of the block: `q` must name it.
        let fits = f.block(*q).is_some_and(|qb| matches!(qb.terminator, Terminator::Br(_) | Terminator::CondBr { .. }));
        if fits {
            copies.push((*q, decision));
        }
    }
    (!copies.is_empty()).then_some(Thread::Duplicate(copies))
}

/// The predecessor of the header a path of the rewrite starts from, and the substitution of its
/// values; none on the way out of a shared block, which every origin enters.
type Origin = Option<(BlockId, usize)>;

/// The new blocks and the edges into the case blocks a rewrite makes.
struct Materializer {
    next: u32,
    blocks: Vec<Block>,
    /// `(the block that jumps, the case block or shared block, the origin predecessor, its
    /// substitution)`: where what an origin brought arrives.
    arrivals: Vec<(BlockId, BlockId, BlockId, usize)>,
    /// `(the block that jumps, the case block, the tree block the edge replaces)`.
    edges: Vec<(BlockId, BlockId, BlockId)>,
    /// The block of each keyed branch.
    shared: HashMap<BlockId, BlockId>,
}

impl Materializer {
    fn fresh(&mut self) -> BlockId {
        let id = BlockId(self.next);
        self.next += 1;
        id
    }

    /// The terminator a block gets to carry out `decision`.
    fn terminator(&mut self, decision: &Decision, owner: BlockId, origin: Origin) -> Terminator {
        match decision {
            Decision::Branch { key: None, cond, then, els } => self.branch(cond, then, els, owner, origin),
            _ => Terminator::Br(self.target(decision, owner, origin)),
        }
    }

    fn branch(&mut self, cond: &Operand, then: &Decision, els: &Decision, owner: BlockId, origin: Origin) -> Terminator {
        let then = self.target(then, owner, origin);
        let els = self.target(els, owner, origin);
        if then == els { Terminator::Br(then) } else { Terminator::CondBr { cond: cond.clone(), then, els } }
    }

    /// The block an edge from `owner` goes to for `decision`: the case block, the shared block of
    /// a keyed branch, or a new block that branches.
    fn target(&mut self, decision: &Decision, owner: BlockId, origin: Origin) -> BlockId {
        match decision {
            Decision::Leaf { block, via } => {
                self.edges.push((owner, *block, *via));
                if let Some((from, subst)) = origin {
                    self.arrivals.push((owner, *block, from, subst));
                }
                *block
            }
            Decision::Branch { key: Some(key), cond, then, els } => {
                let id = if let Some(&id) = self.shared.get(key) {
                    id
                } else {
                    let id = self.fresh();
                    self.shared.insert(*key, id);
                    let terminator = self.branch(cond, then, els, id, None);
                    self.blocks.push(Block { id, name: format!("flat{}", id.0), instrs: Vec::new(), terminator, address: None });
                    id
                };
                if let Some((from, subst)) = origin {
                    self.arrivals.push((owner, id, from, subst));
                }
                id
            }
            Decision::Branch { key: None, .. } => {
                let id = self.fresh();
                let terminator = self.terminator(decision, id, origin);
                self.blocks.push(Block { id, name: format!("flat{}", id.0), instrs: Vec::new(), terminator, address: None });
                id
            }
        }
    }
}

fn retarget(t: &mut Terminator, from: BlockId, to: BlockId) {
    match t {
        Terminator::Br(x) => {
            if *x == from {
                *x = to;
            }
        }
        Terminator::CondBr { then, els, .. } => {
            if *then == from {
                *then = to;
            }
            if *els == from {
                *els = to;
            }
        }
        Terminator::Switch { default, cases, .. } => {
            if *default == from {
                *default = to;
            }
            for (_, x) in cases {
                if *x == from {
                    *x = to;
                }
            }
        }
        _ => {}
    }
}

fn substitute(o: &mut Operand, subst: &HashMap<ValueId, Operand>) {
    if let Operand::Local(v) = o
        && let Some(r) = subst.get(v)
    {
        *o = r.clone();
    }
}

/// Rewrites the graph around `d`; returns whether any predecessor was sent on.
fn rebuild(f: &mut Function, d: &Dispatcher) -> bool {
    let defs = known::definitions(f);
    let mut threads: Vec<(BlockId, Thread)> = Vec::new();
    let mut seen: HashSet<BlockId> = HashSet::new();
    for (_, p) in &d.phis.iter().find(|(v, ..)| *v == d.state).map(|(_, _, i)| i.clone()).unwrap_or_default() {
        if !seen.insert(*p) || d.tree.contains(p) || *p == d.header {
            continue;
        }
        if let Some(t) = thread_of(f, d, &defs, *p) {
            threads.push((*p, t));
        }
    }
    if threads.is_empty() {
        return false;
    }

    let mut m = Materializer {
        next: f.blocks.iter().map(|b| b.id.0).max().unwrap_or(0) + 1,
        blocks: Vec::new(),
        arrivals: Vec::new(),
        edges: Vec::new(),
        shared: HashMap::new(),
    };
    // The substitutions of the duplicates: 0 is the identity, for a predecessor sent on as is.
    let mut substs: Vec<HashMap<ValueId, Operand>> = vec![HashMap::new()];
    // The duplicated blocks: the original, its copies with their substitutions, and its values.
    let mut duplicated: Vec<(BlockId, Vec<(BlockId, usize)>)> = Vec::new();
    for (p, thread) in &threads {
        match thread {
            Thread::Direct(decision) => {
                let Some(block) = f.block(*p) else { continue };
                let terminator = match &block.terminator {
                    Terminator::Br(_) => m.terminator(decision, *p, Some((*p, 0))),
                    other => {
                        let target = m.target(decision, *p, Some((*p, 0)));
                        let mut t = other.clone();
                        retarget(&mut t, d.header, target);
                        t
                    }
                };
                if let Some(block) = f.block_mut(*p) {
                    block.terminator = terminator;
                }
            }
            Thread::Duplicate(copies) => {
                let Some(original) = f.block(*p).cloned() else { continue };
                let mut made = Vec::new();
                for (q, decision) in copies {
                    let id = m.fresh();
                    let mut subst: HashMap<ValueId, Operand> = HashMap::new();
                    let mut instrs = Vec::new();
                    for i in &original.instrs {
                        let Some(v) = i.result else { continue };
                        match &i.inst {
                            Inst::Phi { incoming, .. } => {
                                let brought = incoming.iter().find(|(_, from)| from == q).map_or(Operand::Undef, |(o, _)| o.clone());
                                subst.insert(v, brought);
                            }
                            inst => {
                                let mut inst = inst.clone();
                                for o in inst.operands_mut() {
                                    substitute(o, &subst);
                                }
                                let (hint, ty) = (f.value_name(v).to_owned(), f.value_type(v).clone());
                                let copy = f.new_value(&hint, ty);
                                subst.insert(v, Operand::Local(copy));
                                instrs.push(Instruction { result: Some(copy), inst, address: i.address });
                            }
                        }
                    }
                    substs.push(subst);
                    let index = substs.len() - 1;
                    let terminator = m.terminator(decision, id, Some((*p, index)));
                    m.blocks.push(Block { id, name: format!("{}.{}", original.name, id.0), instrs, terminator, address: None });
                    if let Some(qb) = f.block_mut(*q) {
                        retarget(&mut qb.terminator, *p, id);
                    }
                    made.push((id, index));
                }
                duplicated.push((*p, made));
            }
        }
    }
    if m.arrivals.is_empty() {
        return false;
    }
    f.blocks.append(&mut m.blocks);
    // A case block's own phis take on each new edge what they took from the tree block it
    // replaces; the renaming below gives a variable there the value that reaches the new edge.
    for (jumper, leaf, via) in &m.edges {
        let Some(block) = f.block_mut(*leaf) else { continue };
        for i in &mut block.instrs {
            if let Inst::Phi { incoming, .. } = &mut i.inst
                && !incoming.iter().any(|(_, from)| from == jumper)
                && let Some((o, _)) = incoming.iter().find(|(_, from)| from == via).cloned()
            {
                incoming.push((o, *jumper));
            }
        }
    }
    // The header and the tree go when nothing reaches them any more; the phis lose the entries
    // of the predecessors sent on.
    known::fold_branches(f);

    // The variables to rebuild: the header's phis, defined again at every case block reached
    // directly with what the arriving predecessor brought; and the values of a duplicated block,
    // defined again in each copy.
    let order = reverse_post_order(f);
    let reachable: HashSet<BlockId> = order.iter().copied().collect();
    let preds = f.predecessors();
    let idom = dominators(&order, &preds);
    let frontier = dominance_frontier(&order, &preds, &idom);
    let mut children: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for (&b, &dom) in &idom {
        if b != dom {
            children.entry(dom).or_default().push(b);
        }
    }
    struct Variable {
        value: ValueId,
        defs: HashMap<BlockId, Operand>,
        fill: HashSet<ValueId>,
    }
    let mut variables: Vec<Variable> = Vec::new();
    for (v, ty, incoming) in &d.phis {
        let name = f.value_name(*v).to_owned();
        let mut defs: HashMap<BlockId, Operand> = HashMap::new();
        let mut fill: HashSet<ValueId> = HashSet::new();
        if reachable.contains(&d.header) {
            defs.insert(d.header, Operand::Local(*v));
        }
        let brought_from = |origin: BlockId| incoming.iter().find(|(_, from)| *from == origin).map_or(Operand::Undef, |(o, _)| o.clone());
        for (jumper, leaf, origin, subst) in &m.arrivals {
            if !reachable.contains(leaf) {
                continue;
            }
            let phi = if let Some(Operand::Local(p)) = defs.get(leaf).cloned() {
                p
            } else {
                let p = new_phi(f, *leaf, &name, ty);
                defs.insert(*leaf, Operand::Local(p));
                fill.insert(p);
                p
            };
            let mut brought = brought_from(*origin);
            substitute(&mut brought, &substs[*subst]);
            if let Some(Inst::Phi { incoming, .. }) =
                f.blocks.iter_mut().flat_map(|b| &mut b.instrs).find(|i| i.result == Some(phi)).map(|i| &mut i.inst)
                && !incoming.iter().any(|(_, from)| from == jumper)
            {
                incoming.push((brought, *jumper));
            }
        }
        place_joins(f, &name, ty, &mut defs, &mut fill, &frontier, &reachable);
        variables.push(Variable { value: *v, defs, fill });
    }
    // A duplicated block's values, from the copies' substitutions (the original may be gone).
    for (p, copies) in &duplicated {
        let Some((_, first)) = copies.first() else { continue };
        let values: Vec<ValueId> = substs[*first].keys().copied().collect();
        for v in values {
            let name = f.value_name(v).to_owned();
            let ty = f.value_type(v).clone();
            let mut defs: HashMap<BlockId, Operand> = HashMap::new();
            let mut fill: HashSet<ValueId> = HashSet::new();
            if reachable.contains(p) {
                defs.insert(*p, Operand::Local(v));
            }
            for (copy, subst) in copies {
                if let Some(o) = substs[*subst].get(&v) {
                    defs.insert(*copy, o.clone());
                }
            }
            place_joins(f, &name, &ty, &mut defs, &mut fill, &frontier, &reachable);
            variables.push(Variable { value: v, defs, fill });
        }
    }
    let entry = f.blocks[0].id;
    for var in &variables {
        rename(f, var.value, &var.defs, &var.fill, &children, entry);
    }
    true
}

/// A phi for a rebuilt variable at the front of `at`, its entries to be filled.
fn new_phi(f: &mut Function, at: BlockId, hint: &str, ty: &Type) -> ValueId {
    let v = f.new_value(hint, ty.clone());
    if let Some(block) = f.block_mut(at) {
        block.instrs.insert(0, Instruction { result: Some(v), inst: Inst::Phi { ty: ty.clone(), incoming: Vec::new() }, address: None });
    }
    v
}

/// Phis at the iterated dominance frontier of a variable's definitions.
fn place_joins(
    f: &mut Function,
    name: &str,
    ty: &Type,
    defs: &mut HashMap<BlockId, Operand>,
    fill: &mut HashSet<ValueId>,
    frontier: &HashMap<BlockId, Vec<BlockId>>,
    reachable: &HashSet<BlockId>,
) {
    let mut work: Vec<BlockId> = defs.keys().copied().collect();
    while let Some(x) = work.pop() {
        for &y in frontier.get(&x).into_iter().flatten() {
            if defs.contains_key(&y) || !reachable.contains(&y) {
                continue;
            }
            let p = new_phi(f, y, name, ty);
            defs.insert(y, Operand::Local(p));
            fill.insert(p);
            work.push(y);
        }
    }
}

/// Renames every use of `v` to the definition that reaches it, down the dominator tree, and
/// fills the entries of the phis in `fill` from each predecessor.
fn rename(
    f: &mut Function,
    v: ValueId,
    defs: &HashMap<BlockId, Operand>,
    fill: &HashSet<ValueId>,
    children: &HashMap<BlockId, Vec<BlockId>>,
    entry: BlockId,
) {
    let mut stack: Vec<(BlockId, Operand)> = vec![(entry, Operand::Undef)];
    while let Some((b, mut current)) = stack.pop() {
        if let Some(def) = defs.get(&b) {
            current = def.clone();
        }
        let Some(block) = f.block_mut(b) else { continue };
        for i in &mut block.instrs {
            if matches!(i.inst, Inst::Phi { .. }) {
                continue;
            }
            for o in i.inst.operands_mut() {
                if *o == Operand::Local(v) {
                    *o = current.clone();
                }
            }
        }
        for o in block.terminator.operands_mut() {
            if *o == Operand::Local(v) {
                *o = current.clone();
            }
        }
        let successors = block.terminator.successors();
        for s in successors {
            let Some(sb) = f.block_mut(s) else { continue };
            for i in &mut sb.instrs {
                let fills = i.result.is_some_and(|r| fill.contains(&r));
                let Inst::Phi { incoming, .. } = &mut i.inst else { continue };
                if fills {
                    if let Some(e) = incoming.iter_mut().find(|(_, from)| *from == b) {
                        if e.0 == Operand::Local(v) {
                            e.0 = current.clone();
                        }
                    } else {
                        incoming.push((current.clone(), b));
                    }
                } else {
                    for (o, from) in incoming.iter_mut() {
                        if *from == b && *o == Operand::Local(v) {
                            *o = current.clone();
                        }
                    }
                }
            }
        }
        for &c in children.get(&b).into_iter().flatten() {
            stack.push((c, current.clone()));
        }
    }
}

/// The dominance frontier of each block (Cooper, Harvey and Kennedy).
fn dominance_frontier(
    order: &[BlockId],
    preds: &HashMap<BlockId, Vec<BlockId>>,
    idom: &HashMap<BlockId, BlockId>,
) -> HashMap<BlockId, Vec<BlockId>> {
    let mut df: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for &b in order {
        let Some(ps) = preds.get(&b) else { continue };
        let live: Vec<BlockId> = ps.iter().copied().filter(|p| idom.contains_key(p)).collect();
        if live.len() < 2 {
            continue;
        }
        let Some(&dom) = idom.get(&b) else { continue };
        for p in live {
            let mut runner = p;
            while runner != dom {
                let entry = df.entry(runner).or_default();
                if !entry.contains(&b) {
                    entry.push(b);
                }
                let Some(&up) = idom.get(&runner) else { break };
                if up == runner {
                    break;
                }
                runner = up;
            }
        }
    }
    df
}
