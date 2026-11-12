//! Outlining: an inlined helper read back as a function. A helper the compiler inlined into
//! several callers leaves the same blocks in each of them, over different values. The pass finds
//! the regions whose shape repeats across the module's functions, writes the shape once as a
//! function `H_<address>` whose parameters are the values the copies differ in, and replaces each
//! copy by a call to it.
//!
//! A region is a natural loop, with the pure instructions of its preheader that feed only it and
//! the phis of its one exit block: a single entry, a single exit, and at most one value leaving.
//! An exit the guard that skips the loop also reaches keeps its phis in the caller, the call in
//! the loop's place among their incomings. Two regions match when their blocks correspond one to one with the same instructions in each,
//! every operand either a value of the region, mapped by the correspondence, or an outside value:
//! a value of the enclosing function, a constant or an address. The outside values are bound
//! place by place; one that differs between the copies, or is a local of the caller, becomes a
//! parameter, and a constant that is the same in every copy stays in the body. A region of fewer
//! than [`MIN_WORK`] instructions is not worth a function (the walk to a string's end is the usual
//! one) and stays where it is.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_ir::dom;
use ra_asm_ir::{
    Block, BlockId, CallConv, Callee, FnType, Function, Inst, Instruction, Linkage, Module, Operand, Terminator, Type, ValueId,
};

/// The instructions a region needs, phis and casts not counted, to be outlined: the walk to a
/// string's end is four or five, an inlined helper with a body ten or more.
pub const MIN_WORK: usize = 6;

/// The bound of the rounds: each outlines one shape over all its copies.
const MAX_ROUNDS: usize = 32;

/// Runs the pass over the module; returns whether anything was outlined.
pub fn run(module: &mut Module) -> bool {
    let mut changed = false;
    for _ in 0..MAX_ROUNDS {
        if !outline_one(module) {
            break;
        }
        changed = true;
    }
    changed
}

/// Where a value is used: the block and the index, `None` for the terminator.
type Site = (BlockId, Option<usize>);

/// A candidate region of one function.
#[derive(Clone)]
struct Region {
    /// The index of the function in the module.
    function: usize,
    /// The preheader: the one block outside the loop that enters it.
    pre: BlockId,
    /// The instructions of the preheader the region takes, by index, in order.
    tail: Vec<usize>,
    header: BlockId,
    /// The loop's blocks, in the function's block order.
    body: Vec<BlockId>,
    /// The one block the loop leaves to; its leading phis belong to the region unless it is shared.
    exit: BlockId,
    /// Whether the exit is reached from outside the region as well, as when the compiler sends a
    /// guard that skips the loop to the loop's own exit: its phis then stay the caller's, and the
    /// call takes the place of the loop's one edge in them.
    shared_exit: bool,
    /// The values the region defines.
    defined: HashSet<ValueId>,
    /// The value the region hands out, when one is used after it.
    output: Option<ValueId>,
    /// The instructions counted as work.
    work: usize,
    /// What two regions must share before a match is tried: the block count, the instructions
    /// of the body, and whether a value leaves.
    shape: (usize, usize, bool),
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Role {
    Pre,
    Body,
    Exit,
    Outside,
}

impl Region {
    fn role(&self, b: BlockId) -> Role {
        if b == self.pre {
            Role::Pre
        } else if b == self.exit {
            Role::Exit
        } else if self.body.contains(&b) {
            Role::Body
        } else {
            Role::Outside
        }
    }

    fn blocks(&self) -> BTreeSet<BlockId> {
        let mut set: BTreeSet<BlockId> = self.body.iter().copied().collect();
        set.insert(self.pre);
        set.insert(self.exit);
        set
    }

    /// Whether a site is inside the region.
    fn holds(&self, f: &Function, site: Site) -> bool {
        let (block, index) = site;
        match self.role(block) {
            Role::Body => true,
            Role::Pre => index.is_none_or(|i| self.tail.contains(&i)),
            Role::Exit => index.is_some_and(|i| i < self.exit_phis(f)),
            Role::Outside => false,
        }
    }

    /// The count of the exit's leading phis the region takes: all of them, none of a shared exit.
    fn exit_phis(&self, f: &Function) -> usize {
        if self.shared_exit { 0 } else { f.block(self.exit).map_or(0, phi_count) }
    }
}

fn is_phi(x: &Instruction) -> bool {
    matches!(x.inst, Inst::Phi { .. })
}

/// The leading phis of a block.
fn phi_count(b: &Block) -> usize {
    b.instrs.iter().take_while(|x| is_phi(x)).count()
}

fn use_sites(f: &Function) -> HashMap<ValueId, Vec<Site>> {
    let mut sites: HashMap<ValueId, Vec<Site>> = HashMap::new();
    for b in &f.blocks {
        for (i, x) in b.instrs.iter().enumerate() {
            for v in x.inst.uses() {
                sites.entry(v).or_default().push((b.id, Some(i)));
            }
        }
        for v in b.terminator.uses() {
            sites.entry(v).or_default().push((b.id, None));
        }
    }
    sites
}

/// The candidate regions of a function: each natural loop with one preheader and one exit.
fn regions_of(f: &Function, index: usize) -> Vec<Region> {
    let preds = f.predecessors();
    let order = dom::reverse_post_order(f);
    let reachable: HashSet<BlockId> = order.iter().copied().collect();
    let idom = dom::dominators(&order, &preds);
    let mut loops: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();
    for b in &f.blocks {
        if !reachable.contains(&b.id) {
            continue;
        }
        for s in b.terminator.successors() {
            if !dom::dominates(&idom, s, b.id) {
                continue;
            }
            let set = loops.entry(s).or_default();
            set.insert(s);
            let mut stack = vec![b.id];
            while let Some(x) = stack.pop() {
                if !set.insert(x) {
                    continue;
                }
                stack.extend(preds.get(&x).into_iter().flatten().copied());
            }
        }
    }
    let sites = use_sites(f);
    let mut out = Vec::new();
    for (header, set) in loops {
        let outside: Vec<BlockId> = preds.get(&header).into_iter().flatten().copied().filter(|p| !set.contains(p)).collect();
        if outside.len() != 1 {
            continue;
        }
        let pre = outside[0];
        let exits: BTreeSet<BlockId> =
            set.iter().filter_map(|b| f.block(*b)).flat_map(|b| b.terminator.successors()).filter(|s| !set.contains(s)).collect();
        let Some(&exit) = exits.first() else { continue };
        if exits.len() != 1 || exit == pre || exit == f.entry().id {
            continue;
        }
        let Some(pre_block) = f.block(pre) else { continue };
        if pre_block.terminator.successors().iter().any(|s| *s != header && *s != exit) {
            continue;
        }
        let shared_exit = preds.get(&exit).into_iter().flatten().any(|p| *p != pre && !set.contains(p));
        if shared_exit {
            // The call replaces the loop's edge in the exit's phis: one edge, and the preheader
            // not a second way in.
            let edges: usize = set
                .iter()
                .filter_map(|b| f.block(*b))
                .map(|b| b.terminator.successors().into_iter().filter(|s| *s == exit).count())
                .sum();
            if edges != 1 || pre_block.terminator.successors().contains(&exit) {
                continue;
            }
        }
        let body: Vec<BlockId> = f.blocks.iter().map(|b| b.id).filter(|b| set.contains(b)).collect();
        let mut region = Region {
            function: index,
            pre,
            tail: Vec::new(),
            header,
            body,
            exit,
            shared_exit,
            defined: HashSet::new(),
            output: None,
            work: 0,
            shape: (0, 0, false),
        };
        region.tail = tail_of(f, &region, &sites);
        if finish(f, &mut region, &sites) {
            out.push(region);
        }
    }
    out
}

/// The instructions of the preheader the region can take: pure, not a phi or an alloca, every use
/// inside the region, and, for a load, nothing with a side effect after it in the block (a load
/// moved to the call would read after the stores between).
fn tail_of(f: &Function, region: &Region, sites: &HashMap<ValueId, Vec<Site>>) -> Vec<usize> {
    let Some(block) = f.block(region.pre) else { return Vec::new() };
    let mut taken: Vec<usize> = Vec::new();
    let mut effects_after = false;
    for (i, x) in block.instrs.iter().enumerate().rev() {
        if x.inst.has_side_effects() {
            effects_after = true;
            continue;
        }
        let movable = match &x.inst {
            Inst::Phi { .. } | Inst::Alloca { .. } => false,
            Inst::Load { .. } => !effects_after,
            _ => true,
        };
        let Some(v) = x.result else { continue };
        if !movable {
            continue;
        }
        let inside = sites.get(&v).into_iter().flatten().all(|&(b, at)| match region.role(b) {
            Role::Pre => at.is_none_or(|j| taken.contains(&j)),
            _ => region.holds(f, (b, at)),
        });
        if inside {
            taken.push(i);
        }
    }
    taken.reverse();
    taken
}

/// Settles the values a region defines and the one it hands out; false when more than one leaves,
/// a phi names a block outside, or the region is too little work.
fn finish(f: &Function, region: &mut Region, sites: &HashMap<ValueId, Vec<Site>>) -> bool {
    let mut defined = HashSet::new();
    let (mut work, mut body_instrs) = (0, 0);
    let exit_phis = region.exit_phis(f);
    let counts = |inst: &Inst| usize::from(!matches!(inst, Inst::Phi { .. } | Inst::Cast { .. }));
    let inside_block = |b: BlockId| b == region.pre || region.body.contains(&b);
    let phis_inside = |x: &Instruction| match &x.inst {
        Inst::Phi { incoming, .. } => incoming.iter().all(|(_, b)| inside_block(*b)),
        _ => true,
    };
    if let Some(pre) = f.block(region.pre) {
        for &i in &region.tail {
            let x = &pre.instrs[i];
            defined.extend(x.result);
            work += counts(&x.inst);
        }
    }
    for b in region.body.iter().filter_map(|b| f.block(*b)) {
        for x in &b.instrs {
            if !phis_inside(x) {
                return false;
            }
            defined.extend(x.result);
            work += counts(&x.inst);
            body_instrs += 1;
        }
    }
    if let Some(exit) = f.block(region.exit) {
        for x in exit.instrs.iter().take(exit_phis) {
            if !phis_inside(x) {
                return false;
            }
            defined.extend(x.result);
        }
    }
    let mut outputs: Vec<ValueId> =
        defined.iter().copied().filter(|v| sites.get(v).into_iter().flatten().any(|&site| !region.holds(f, site))).collect();
    outputs.sort_unstable();
    if outputs.len() > 1 || work < MIN_WORK {
        return false;
    }
    region.shape = (region.body.len(), body_instrs, !outputs.is_empty());
    region.defined = defined;
    region.output = outputs.first().copied();
    region.work = work;
    true
}

/// The correspondence a match built: the template's values to the copy's, and the outside values
/// of the template, each with its type, to what the copy has in their places.
#[derive(Default)]
struct Match {
    sigma: HashMap<ValueId, ValueId>,
    sigma_rev: HashMap<ValueId, ValueId>,
    bindings: Vec<((Operand, Type), Operand)>,
}

impl Match {
    fn bind_value(&mut self, a: ValueId, b: ValueId) -> bool {
        match (self.sigma.get(&a), self.sigma_rev.get(&b)) {
            (Some(x), Some(y)) => *x == b && *y == a,
            (None, None) => {
                self.sigma.insert(a, b);
                self.sigma_rev.insert(b, a);
                true
            }
            _ => false,
        }
    }

    fn bind_outside(&mut self, key: (Operand, Type), value: Operand) -> bool {
        match self.bindings.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => *v == value,
            None => {
                self.bindings.push((key, value));
                true
            }
        }
    }

    /// Pairs an operand of the template with the copy's in the same place.
    fn pair(&mut self, t: Side<'_>, s: Side<'_>, a: &Operand, b: &Operand, ty: Option<&Type>) -> bool {
        let inside = |side: Side<'_>, o: &Operand| matches!(o, Operand::Local(v) if side.r.defined.contains(v));
        match (inside(t, a), inside(s, b)) {
            (true, true) => {
                let (Operand::Local(x), Operand::Local(y)) = (a, b) else { return false };
                self.bind_value(*x, *y)
            }
            (false, false) => {
                let ty = match (a, ty) {
                    (Operand::Local(v), _) => t.f.value_type(*v).clone(),
                    (_, Some(ty)) => ty.clone(),
                    (_, None) => return a == b,
                };
                if let Operand::Local(v) = b
                    && *s.f.value_type(*v) != ty
                {
                    return false;
                }
                self.bind_outside((a.clone(), ty), b.clone())
            }
            _ => false,
        }
    }
}

/// A region with its function.
#[derive(Clone, Copy)]
struct Side<'a> {
    f: &'a Function,
    r: &'a Region,
}

/// The types of an instruction's operands, in the order of [`Inst::operands`]; `None` where the
/// instruction does not say.
fn operand_types(inst: &Inst) -> Vec<Option<Type>> {
    match inst {
        Inst::Alloca { .. } => Vec::new(),
        Inst::Load { .. } => vec![Some(Type::Ptr)],
        Inst::Store { ty, .. } => vec![Some(ty.clone()), Some(Type::Ptr)],
        Inst::Bin { ty, .. } | Inst::ICmp { ty, .. } | Inst::FCmp { ty, .. } | Inst::ShuffleVector { ty, .. } => vec![Some(ty.clone()); 2],
        Inst::Select { ty, .. } => vec![Some(Type::I1), Some(ty.clone()), Some(ty.clone())],
        Inst::Cast { from, .. } => vec![Some(from.clone())],
        Inst::Freeze { ty, .. } => vec![Some(ty.clone())],
        Inst::Gep { indices, .. } => {
            let mut v = vec![Some(Type::Ptr)];
            v.extend(indices.iter().map(|(t, _)| Some(t.clone())));
            v
        }
        Inst::ExtractElement { ty, .. } => vec![Some(ty.clone()), None],
        Inst::InsertElement { ty, .. } => vec![Some(ty.clone()), None, None],
        Inst::Call { callee, args, .. } => {
            let mut v = Vec::new();
            if matches!(callee, Callee::Indirect(_)) {
                v.push(Some(Type::Ptr));
            }
            v.extend(args.iter().map(|(t, _)| Some(t.clone())));
            v
        }
        Inst::Phi { ty, incoming } => vec![Some(ty.clone()); incoming.len()],
    }
}

fn terminator_operand_type(t: &Terminator) -> Option<Type> {
    match t {
        Terminator::CondBr { .. } => Some(Type::I1),
        Terminator::Switch { ty, .. } | Terminator::Ret(Some((ty, _))) => Some(ty.clone()),
        Terminator::Ret(None) | Terminator::Br(_) | Terminator::Unreachable => None,
    }
}

/// The instruction with its operands blanked: what two instructions must agree on besides them.
fn skeleton(inst: &Inst) -> Inst {
    let mut s = inst.clone();
    for o in s.operands_mut() {
        *o = Operand::Undef;
    }
    if let Inst::Phi { incoming, .. } = &mut s {
        for (_, b) in incoming {
            *b = BlockId(0);
        }
    }
    s
}

fn same_terminator_shape(a: &Terminator, b: &Terminator) -> bool {
    match (a, b) {
        (Terminator::Br(_), Terminator::Br(_)) | (Terminator::CondBr { .. }, Terminator::CondBr { .. }) => true,
        (Terminator::Switch { ty: ta, cases: ca, .. }, Terminator::Switch { ty: tb, cases: cb, .. }) => {
            ta == tb && ca.len() == cb.len() && ca.iter().zip(cb).all(|((x, _), (y, _))| x == y)
        }
        (Terminator::Ret(Some((ta, _))), Terminator::Ret(Some((tb, _)))) => ta == tb,
        (Terminator::Ret(None), Terminator::Ret(None)) | (Terminator::Unreachable, Terminator::Unreachable) => true,
        _ => false,
    }
}

/// The blocks of the copy that correspond to the template's, walked from the preheaders along
/// the successors in order.
fn correspond(ft: &Function, t: &Region, fs: &Function, s: &Region) -> Option<HashMap<BlockId, BlockId>> {
    if t.body.len() != s.body.len() {
        return None;
    }
    let mut map: HashMap<BlockId, BlockId> = HashMap::new();
    let mut rev: HashMap<BlockId, BlockId> = HashMap::new();
    let mut work = vec![(t.pre, s.pre)];
    while let Some((a, b)) = work.pop() {
        match (map.get(&a), rev.get(&b)) {
            (Some(x), Some(y)) if *x == b && *y == a => continue,
            (None, None) => {}
            _ => return None,
        }
        let role = t.role(a);
        if role != s.role(b) || role == Role::Outside {
            return None;
        }
        map.insert(a, b);
        rev.insert(b, a);
        if role == Role::Exit {
            continue;
        }
        let (ta, tb) = (&ft.block(a)?.terminator, &fs.block(b)?.terminator);
        if !same_terminator_shape(ta, tb) {
            return None;
        }
        work.extend(ta.successors().into_iter().zip(tb.successors()));
    }
    (map.get(&t.header) == Some(&s.header) && t.body.iter().all(|b| map.contains_key(b))).then_some(map)
}

/// Matches the copy `s` against the template `t`: the copy as the match takes it (its preheader
/// tail cut to the template's) and the correspondence. The copy's tail may hold more than the
/// template's: the rest stays in the caller, and what the region reads of it comes in as a
/// parameter.
fn matches(module: &Module, t: &Region, s: &Region) -> Option<(Region, Match)> {
    let (ft, fs) = (&module.functions[t.function], &module.functions[s.function]);
    let blocks = correspond(ft, t, fs, s)?;
    let (t_pre, s_pre) = (ft.block(t.pre)?, fs.block(s.pre)?);
    // The template's tail as a subsequence of the copy's, by shape.
    let mut tail: Vec<usize> = Vec::new();
    let mut from = 0;
    for &i in &t.tail {
        let want = skeleton(&t_pre.instrs[i].inst);
        let j = s.tail[from..].iter().position(|&j| skeleton(&s_pre.instrs[j].inst) == want)?;
        tail.push(s.tail[from + j]);
        from += j + 1;
    }
    let mut s = Region { tail, ..s.clone() };
    let sites = use_sites(fs);
    if !finish(fs, &mut s, &sites) {
        return None;
    }
    let mut m = Match::default();
    let (ts, ss) = (Side { f: ft, r: t }, Side { f: fs, r: &s });
    let pair_instr = |m: &mut Match, a: &Instruction, b: &Instruction| -> bool {
        if skeleton(&a.inst) != skeleton(&b.inst) {
            return false;
        }
        match (a.result, b.result) {
            (Some(x), Some(y)) => {
                if !m.bind_value(x, y) {
                    return false;
                }
            }
            (None, None) => {}
            _ => return false,
        }
        if let (Inst::Phi { ty, incoming: ia }, Inst::Phi { incoming: ib, .. }) = (&a.inst, &b.inst) {
            return ia.iter().all(|(o, blk)| {
                blocks.get(blk).is_some_and(|mapped| ib.iter().any(|(p, b2)| b2 == mapped && m.pair(ts, ss, o, p, Some(ty))))
            });
        }
        let types = operand_types(&a.inst);
        let (oa, ob) = (a.inst.operands(), b.inst.operands());
        oa.iter().zip(&ob).zip(&types).all(|((x, y), ty)| m.pair(ts, ss, x, y, ty.as_ref()))
    };
    let pair_terminator = |m: &mut Match, a: &Terminator, b: &Terminator| -> bool {
        let ty = terminator_operand_type(a);
        let (oa, ob) = (a.operands(), b.operands());
        oa.iter().zip(&ob).all(|(x, y)| m.pair(ts, ss, x, y, ty.as_ref()))
    };
    for (&i, &j) in t.tail.iter().zip(&s.tail) {
        if !pair_instr(&mut m, &t_pre.instrs[i], &s_pre.instrs[j]) {
            return None;
        }
    }
    if !pair_terminator(&mut m, &t_pre.terminator, &s_pre.terminator) {
        return None;
    }
    // The phis of a block come in the order of the machine's slots, which differs between copies
    // that keep a value in different registers: the rest of each block is paired first, and a
    // phi then with the copy's phi its uses bound it to.
    let mut phis: Vec<(&[Instruction], &[Instruction])> = Vec::new();
    for &a in &t.body {
        let (ba, bb) = (ft.block(a)?, fs.block(*blocks.get(&a)?)?);
        let n = phi_count(ba);
        if ba.instrs.len() != bb.instrs.len() || n != phi_count(bb) {
            return None;
        }
        for (x, y) in ba.instrs[n..].iter().zip(&bb.instrs[n..]) {
            if !pair_instr(&mut m, x, y) {
                return None;
            }
        }
        if !pair_terminator(&mut m, &ba.terminator, &bb.terminator) {
            return None;
        }
        phis.push((&ba.instrs[..n], &bb.instrs[..n]));
    }
    let (xa, xb) = (ft.block(t.exit)?, fs.block(s.exit)?);
    let exit_phis = t.exit_phis(ft);
    if exit_phis != s.exit_phis(fs) {
        return None;
    }
    phis.push((&xa.instrs[..exit_phis], &xb.instrs[..exit_phis]));
    match (t.output, s.output) {
        (None, None) => {}
        (Some(x), Some(y)) if m.bind_value(x, y) => {}
        _ => return None,
    }
    let mut pending: Vec<(usize, usize)> = phis.iter().enumerate().flat_map(|(g, (a, _))| (0..a.len()).map(move |i| (g, i))).collect();
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    loop {
        let before = pending.len();
        let mut unbound = Vec::new();
        for (g, i) in pending {
            let (ta, sb) = phis[g];
            let Some(y) = ta[i].result.and_then(|v| m.sigma.get(&v).copied()) else {
                unbound.push((g, i));
                continue;
            };
            let j = sb.iter().position(|p| p.result == Some(y))?;
            if !taken.insert((g, j)) || !pair_instr(&mut m, &ta[i], &sb[j]) {
                return None;
            }
        }
        pending = unbound;
        if pending.is_empty() || pending.len() == before {
            break;
        }
    }
    // The phis no use bound: in order, over the copy's phis left.
    for (g, i) in pending {
        let (ta, sb) = phis[g];
        let j = (0..sb.len()).find(|j| !taken.contains(&(g, *j)))?;
        taken.insert((g, j));
        if !pair_instr(&mut m, &ta[i], &sb[j]) {
            return None;
        }
    }
    Some((s, m))
}

/// Finds one shape with a copy elsewhere and outlines every copy; false when none is left.
fn outline_one(module: &mut Module) -> bool {
    let regions: Vec<Region> = module.functions.iter().enumerate().flat_map(|(i, f)| regions_of(f, i)).collect();
    // The largest template first: a loop nest is outlined whole before its inner loop alone.
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(regions[i].work));
    for &ti in &order {
        let t = &regions[ti];
        let mut used: Vec<(usize, BTreeSet<BlockId>)> = vec![(t.function, t.blocks())];
        let mut copies: Vec<(Region, Match)> = Vec::new();
        for &si in &order {
            let s = &regions[si];
            if si == ti || s.shape != t.shape {
                continue;
            }
            let blocks = s.blocks();
            if used.iter().any(|(function, taken)| *function == s.function && !taken.is_disjoint(&blocks)) {
                continue;
            }
            if let Some(found) = matches(module, t, s) {
                used.push((s.function, blocks));
                copies.push(found);
            }
        }
        if copies.is_empty() {
            continue;
        }
        outline(module, t, &copies);
        return true;
    }
    false
}

/// Writes the template as a function and replaces it and its copies by calls.
fn outline(module: &mut Module, t: &Region, copies: &[(Region, Match)]) {
    // The parameters: the template's outside values in the order the walk met them, those that
    // are locals or that any copy has differently.
    let keys: Vec<(Operand, Type)> = copies[0].1.bindings.iter().map(|(k, _)| k.clone()).collect();
    let params: Vec<(Operand, Type)> = keys
        .into_iter()
        .filter(|key| {
            matches!(key.0, Operand::Local(_)) || copies.iter().any(|(_, m)| m.bindings.iter().any(|(k, v)| *k == *key && *v != key.0))
        })
        .collect();
    let f = &module.functions[t.function];
    let name = helper_name(module, f, t);
    let helper = Copier::new(f, t, &params, &name).build();
    let fn_ty = helper.ty.clone();
    let template_args: Vec<(Type, Operand)> = params.iter().map(|(k, ty)| (ty.clone(), k.clone())).collect();
    let address = region_address(f, t);
    // The outputs are renamed once every copy is a call: a copy that feeds another names a
    // value the other's replacement removed.
    let mut renames: HashMap<usize, HashMap<ValueId, Operand>> = HashMap::new();
    let mut note = |function: usize, renamed: Option<(ValueId, ValueId)>| {
        if let Some((out, result)) = renamed {
            renames.entry(function).or_default().insert(out, Operand::Local(result));
        }
    };
    note(t.function, replace(&mut module.functions[t.function], t, &name, &fn_ty, template_args, address));
    for (s, m) in copies {
        let args = params
            .iter()
            .map(|key| {
                let value = m.bindings.iter().find(|(k, _)| *k == *key).map_or(&key.0, |(_, v)| v);
                (key.1.clone(), value.clone())
            })
            .collect();
        let address = region_address(&module.functions[s.function], s);
        note(s.function, replace(&mut module.functions[s.function], s, &name, &fn_ty, args, address));
    }
    for (function, map) in renames {
        module.functions[function].replace_uses(&map);
    }
    module.functions.push(helper);
}

/// The first machine address of a region, for its name and its call.
fn region_address(f: &Function, r: &Region) -> Option<u64> {
    let pre = f.block(r.pre)?;
    r.tail
        .iter()
        .find_map(|&i| pre.instrs[i].address)
        .or_else(|| r.body.iter().filter_map(|b| f.block(*b)).find_map(|b| b.address.or_else(|| b.instrs.iter().find_map(|x| x.address))))
}

/// `H_<address>`, unique in the module.
fn helper_name(module: &Module, f: &Function, r: &Region) -> String {
    let base = format!("H_{:08x}", region_address(f, r).or(f.address).unwrap_or(0));
    let mut name = base.clone();
    let mut n = 0;
    while module.functions.iter().any(|g| g.name == name) {
        n += 1;
        name = format!("{base}.{n}");
    }
    name
}

/// Copies the template region into the helper: the tail as its entry block, the loop's blocks,
/// and the exit's phis in a block that returns the output.
struct Copier<'a> {
    f: &'a Function,
    t: &'a Region,
    h: Function,
    /// The parameter of each outside value.
    by_key: Vec<((Operand, Type), ValueId)>,
    blocks: HashMap<BlockId, BlockId>,
    values: HashMap<ValueId, ValueId>,
}

impl<'a> Copier<'a> {
    fn new(f: &'a Function, t: &'a Region, params: &[(Operand, Type)], name: &str) -> Self {
        let ret = t.output.map_or(Type::Void, |v| f.value_type(v).clone());
        let mut h = Function {
            name: name.to_owned(),
            ty: FnType::new(ret, params.iter().map(|(_, ty)| ty.clone()).collect()),
            params: Vec::new(),
            blocks: Vec::new(),
            linkage: Linkage::Internal,
            conv: CallConv::C,
            attrs: Vec::new(),
            address: None,
            value_names: Vec::new(),
            value_types: Vec::new(),
        };
        let mut by_key = Vec::new();
        for (key, ty) in params {
            let hint = match key {
                Operand::Local(v) => f.value_name(*v).to_owned(),
                _ => "k".to_owned(),
            };
            let p = h.new_value(&hint, ty.clone());
            h.params.push(p);
            by_key.push(((key.clone(), ty.clone()), p));
        }
        let mut blocks: HashMap<BlockId, BlockId> = HashMap::new();
        blocks.insert(t.pre, BlockId(0));
        for (n, b) in t.body.iter().enumerate() {
            blocks.insert(*b, BlockId(n as u32 + 1));
        }
        blocks.insert(t.exit, BlockId(t.body.len() as u32 + 1));
        Self { f, t, h, by_key, blocks, values: HashMap::new() }
    }

    fn local(&mut self, v: ValueId) -> ValueId {
        if let Some(&n) = self.values.get(&v) {
            return n;
        }
        let n = self.h.new_value(self.f.value_name(v), self.f.value_type(v).clone());
        self.values.insert(v, n);
        n
    }

    fn rewrite(&mut self, o: &mut Operand, ty: Option<&Type>) {
        let key_ty = if let Operand::Local(v) = *o {
            if self.t.defined.contains(&v) {
                *o = Operand::Local(self.local(v));
                return;
            }
            self.f.value_type(v).clone()
        } else if let Some(ty) = ty {
            ty.clone()
        } else {
            return;
        };
        if let Some((_, p)) = self.by_key.iter().find(|(k, _)| k.0 == *o && k.1 == key_ty) {
            *o = Operand::Local(*p);
        }
    }

    fn copy(&mut self, x: &Instruction) -> Instruction {
        let mut inst = x.inst.clone();
        let types = operand_types(&inst);
        for (o, ty) in inst.operands_mut().into_iter().zip(&types) {
            self.rewrite(o, ty.as_ref());
        }
        if let Inst::Phi { incoming, .. } = &mut inst {
            for (_, b) in incoming {
                *b = self.blocks[&*b];
            }
        }
        let result = x.result.map(|v| self.local(v));
        Instruction { result, inst, address: x.address }
    }

    fn terminator(&mut self, term: &Terminator) -> Terminator {
        let mut term = term.clone();
        let ty = terminator_operand_type(&term);
        for o in term.operands_mut() {
            self.rewrite(o, ty.as_ref());
        }
        match &mut term {
            Terminator::Br(b) => *b = self.blocks[&*b],
            Terminator::CondBr { then, els, .. } => {
                *then = self.blocks[&*then];
                *els = self.blocks[&*els];
            }
            Terminator::Switch { default, cases, .. } => {
                *default = self.blocks[&*default];
                for (_, b) in cases {
                    *b = self.blocks[&*b];
                }
            }
            Terminator::Ret(_) | Terminator::Unreachable => {}
        }
        term
    }

    fn build(mut self) -> Function {
        let (f, t) = (self.f, self.t);
        let pre = f.block(t.pre).expect("the preheader is a block of the function");
        let instrs: Vec<Instruction> = t.tail.iter().map(|&i| self.copy(&pre.instrs[i])).collect();
        let terminator = self.terminator(&pre.terminator);
        self.h.blocks.push(Block { id: BlockId(0), name: "h.entry".to_owned(), instrs, terminator, address: None });
        for b in t.body.iter().filter_map(|b| f.block(*b)) {
            let instrs: Vec<Instruction> = b.instrs.iter().map(|x| self.copy(x)).collect();
            let terminator = self.terminator(&b.terminator);
            self.h.blocks.push(Block { id: self.blocks[&b.id], name: b.name.clone(), instrs, terminator, address: b.address });
        }
        let exit = f.block(t.exit).expect("the exit is a block of the function");
        let instrs: Vec<Instruction> = exit.instrs.iter().take(t.exit_phis(f)).map(|x| self.copy(x)).collect();
        let returned = t.output.map(|v| (self.h.ty.ret.clone(), Operand::Local(self.local(v))));
        self.h.blocks.push(Block {
            id: self.blocks[&t.exit],
            name: "h.exit".to_owned(),
            instrs,
            terminator: Terminator::Ret(returned),
            address: None,
        });
        self.h
    }
}

/// Replaces a region by a call: the tail and the loop go, the exit loses its phis, and what the
/// region handed out is the call's result, returned with the value it stands for so the caller
/// renames the uses.
fn replace(
    f: &mut Function,
    r: &Region,
    name: &str,
    fn_ty: &FnType,
    args: Vec<(Type, Operand)>,
    address: Option<u64>,
) -> Option<(ValueId, ValueId)> {
    let result = (fn_ty.ret != Type::Void).then(|| f.new_value("outlined", fn_ty.ret.clone()));
    let leaving: Vec<BlockId> =
        f.blocks.iter().filter(|b| r.body.contains(&b.id) && b.terminator.successors().contains(&r.exit)).map(|b| b.id).collect();
    f.blocks.retain(|b| !r.body.contains(&b.id));
    if let Some(exit) = f.block_mut(r.exit) {
        let phis = phi_count(exit);
        if r.shared_exit {
            // The exit's phis stay; the value the loop brought now comes from the call's block.
            for x in &mut exit.instrs[..phis] {
                if let Inst::Phi { incoming, .. } = &mut x.inst {
                    for (_, b) in incoming.iter_mut().filter(|(_, b)| leaving.contains(b)) {
                        *b = r.pre;
                    }
                }
            }
        } else {
            let rest = exit.instrs.split_off(phis);
            exit.instrs = rest;
        }
    }
    if let Some(pre) = f.block_mut(r.pre) {
        let taken: HashSet<usize> = r.tail.iter().copied().collect();
        let mut kept: Vec<Instruction> =
            std::mem::take(&mut pre.instrs).into_iter().enumerate().filter(|(i, _)| !taken.contains(i)).map(|(_, x)| x).collect();
        kept.push(Instruction {
            result,
            inst: Inst::Call { fn_ty: fn_ty.clone(), callee: Callee::Direct(name.to_owned()), args, tail: false },
            address,
        });
        pre.instrs = kept;
        pre.terminator = Terminator::Br(r.exit);
    }
    r.output.zip(result)
}
