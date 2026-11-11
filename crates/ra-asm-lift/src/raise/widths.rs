//! Widths: the registers are 64 bits wide, so every value the lift makes of one is an `i64`, and a
//! 32-bit operation reads as a `trunc`, the operation, and a `zext` back. This pass gives each value
//! the width its uses need. A demand analysis finds the low bits each `i64` value's uses read (a
//! `trunc` to `i32` reads 32, an addition reads what its result's uses read, a store reads all); a
//! known-zero analysis finds how far up a value is zero (a `zext` from `i32` is zero above 32). A
//! value whose demand is below 64 and whose definition is an extension, an addition and its kin, a
//! phi or a select is defined again at the demanded width, and the `trunc`s that read it go. An
//! unsigned comparison of two values zero above the same width compares at that width.
//!
//! Across functions, a parameter of a function that nothing outside the module calls and nothing
//! takes the address of narrows to what the body demands, and a result narrows to what every
//! return gives and every caller demands. The pass runs to a fixpoint with the cleanup passes, since
//! each narrowing exposes the next.
#![allow(clippy::many_single_char_names)]

use std::collections::{BTreeSet, HashMap, HashSet};

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, Const, Function, ICond, Inst, Instruction, Module, Operand, Terminator, Type, ValueId};

const FULL: u16 = 64;

/// The width a demand of `bits` rounds to: the C integer types.
fn round(bits: u16) -> u16 {
    match bits {
        0..=8 => 8,
        9..=16 => 16,
        17..=32 => 32,
        _ => FULL,
    }
}

/// The bits a constant occupies, as an unsigned 64-bit value.
fn const_bits(c: i128) -> u16 {
    let v = c as u64;
    (64 - v.leading_zeros()) as u16
}

/// What the demand and the known-zero analyses found for the `i64` values of a function.
struct Analysis {
    /// The low bits each value's uses read; a value not in the map is not an `i64`.
    demand: HashMap<ValueId, u16>,
    /// The width above which each value is zero; 64 when unknown.
    zero: HashMap<ValueId, u16>,
}

impl Analysis {
    fn zero_of(&self, o: &Operand) -> u16 {
        match o {
            Operand::Local(v) => self.zero.get(v).copied().unwrap_or(FULL),
            Operand::Int(c) => const_bits(*c),
            Operand::Undef | Operand::Zero => 0,
            _ => FULL,
        }
    }

    /// The width a switch on `value` can run at: the value and every case fit in it.
    fn switch_width(&self, value: &Operand, cases: &[(i128, BlockId)]) -> u16 {
        let cases = cases.iter().map(|(c, _)| const_bits(*c)).max().unwrap_or(0);
        round(self.zero_of(value).max(cases).max(1))
    }

    /// The width an unsigned comparison of `lhs` and `rhs` can run at.
    fn compare_width(&self, cond: ICond, lhs: &Operand, rhs: &Operand) -> u16 {
        match cond {
            ICond::Eq | ICond::Ne | ICond::Ult | ICond::Ule | ICond::Ugt | ICond::Uge => {
                round(self.zero_of(lhs).max(self.zero_of(rhs)).max(1))
            }
            _ => FULL,
        }
    }
}

/// The widths the module-level step decided: the parameters and the result of each function.
#[derive(Default, PartialEq, Eq)]
struct Plan {
    params: HashMap<String, Vec<Option<u16>>>,
    results: HashMap<String, u16>,
}

fn is_i64(f: &Function, v: ValueId) -> bool {
    *f.value_type(v) == Type::I64
}

fn analyze(f: &Function, plan: &Plan) -> Analysis {
    // The known-zero widths: optimistic (0) for every i64 value, raised to a fixpoint.
    let mut zero: HashMap<ValueId, u16> = HashMap::new();
    for (i, ty) in f.value_types.iter().enumerate() {
        if *ty == Type::I64 {
            zero.insert(ValueId(i as u32), 0);
        }
    }
    for p in &f.params {
        if is_i64(f, *p) {
            zero.insert(*p, FULL);
        }
    }
    let mut a = Analysis { demand: HashMap::new(), zero };
    loop {
        let mut changed = false;
        for b in &f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                if !is_i64(f, v) {
                    continue;
                }
                let z = match &i.inst {
                    Inst::Cast { cast: Cast::ZExt, from: Type::Int(k), .. } => *k,
                    Inst::Bin { op: BinOp::And, lhs, rhs, .. } => a.zero_of(lhs).min(a.zero_of(rhs)),
                    Inst::Bin { op: BinOp::Or | BinOp::Xor, lhs, rhs, .. } => a.zero_of(lhs).max(a.zero_of(rhs)),
                    Inst::Bin { op: BinOp::Add, lhs, rhs, .. } => (a.zero_of(lhs).max(a.zero_of(rhs)) + 1).min(FULL),
                    Inst::Bin { op: BinOp::Mul, lhs, rhs, .. } => (a.zero_of(lhs) + a.zero_of(rhs)).min(FULL),
                    Inst::Bin { op: BinOp::Shl, lhs, rhs: Operand::Int(c), .. } if (0..64).contains(c) => {
                        (a.zero_of(lhs) + *c as u16).min(FULL)
                    }
                    Inst::Bin { op: BinOp::LShr, lhs, rhs: Operand::Int(c), .. } if (0..64).contains(c) => {
                        a.zero_of(lhs).saturating_sub(*c as u16)
                    }
                    Inst::Phi { incoming, .. } => incoming.iter().map(|(o, _)| a.zero_of(o)).max().unwrap_or(0),
                    Inst::Select { then, els, .. } => a.zero_of(then).max(a.zero_of(els)),
                    Inst::Call { callee: Callee::Direct(name), .. } => plan.results.get(name).copied().unwrap_or(FULL),
                    _ => FULL,
                };
                let old = a.zero[&v];
                if z > old {
                    a.zero.insert(v, z);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // The demands: nothing (0) for every i64 value, raised by each use to a fixpoint.
    for v in a.zero.keys() {
        a.demand.insert(*v, 0);
    }
    loop {
        let mut changed = false;
        let mut raise = |a: &mut Analysis, o: &Operand, bits: u16| {
            if let Operand::Local(v) = o
                && let Some(d) = a.demand.get_mut(v)
                && *d < bits
            {
                *d = bits;
                changed = true;
            }
        };
        for b in &f.blocks {
            for i in &b.instrs {
                let own = i.result.and_then(|v| a.demand.get(&v).copied());
                match &i.inst {
                    Inst::Cast { cast: Cast::Trunc, from: Type::I64, value, to: Type::Int(n), .. } => raise(&mut a, value, *n),
                    Inst::Bin {
                        op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl,
                        ty: Type::I64,
                        lhs,
                        rhs,
                    } => {
                        let d = own.unwrap_or(FULL);
                        raise(&mut a, lhs, d);
                        // the shift amount is read whole
                        raise(&mut a, rhs, if matches!(i.inst, Inst::Bin { op: BinOp::Shl, .. }) { FULL } else { d });
                    }
                    Inst::Phi { ty: Type::I64, incoming } => {
                        let d = own.unwrap_or(FULL);
                        for (o, _) in incoming {
                            raise(&mut a, o, d);
                        }
                    }
                    Inst::Select { ty: Type::I64, then, els, .. } => {
                        let d = own.unwrap_or(FULL);
                        raise(&mut a, then, d);
                        raise(&mut a, els, d);
                    }
                    Inst::ICmp { cond, ty: Type::I64, lhs, rhs } => {
                        let w = a.compare_width(*cond, lhs, rhs);
                        raise(&mut a, lhs, w);
                        raise(&mut a, rhs, w);
                    }
                    Inst::Call { callee: Callee::Direct(name), args, .. } if plan.params.contains_key(name) => {
                        let widths = &plan.params[name];
                        for (k, (_, arg)) in args.iter().enumerate() {
                            raise(&mut a, arg, widths.get(k).copied().flatten().unwrap_or(FULL));
                        }
                    }
                    inst => {
                        for v in inst.uses() {
                            raise(&mut a, &Operand::Local(v), FULL);
                        }
                    }
                }
            }
            match &b.terminator {
                Terminator::Ret(Some((Type::I64, o))) => raise(&mut a, o, plan.results.get(&f.name).copied().unwrap_or(FULL)),
                Terminator::Switch { ty: Type::I64, value, cases, .. } => {
                    let w = a.switch_width(value, cases);
                    raise(&mut a, value, w);
                }
                t => {
                    for v in t.uses() {
                        raise(&mut a, &Operand::Local(v), FULL);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    a
}

/// Whether the instruction can define its value at a narrower width.
fn narrowable(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Cast { cast: Cast::ZExt | Cast::SExt, from: Type::Int(_), .. }
            | Inst::Bin { op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor, .. }
            | Inst::Bin { op: BinOp::Shl, rhs: Operand::Int(_), .. }
            | Inst::Phi { .. }
            | Inst::Select { .. }
    )
}

/// Runs the pass over the module. `roots` are the functions outside callers reach; their
/// signatures stay. Returns whether anything changed.
pub fn run(module: &mut Module, roots: &BTreeSet<String>) -> bool {
    let taken = address_taken(module);
    let names: HashSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    let eligible = |name: &str| names.contains(name) && !roots.contains(name) && !taken.contains(name);

    // The plan is a fixpoint across the functions, from an optimistic start: a result's width
    // depends on what its callers demand, and a caller that returns the result as it is demands
    // what its own result's width allows. Each round widens, so the rounds end.
    let mut plan = Plan::default();
    for f in &module.functions {
        if !eligible(&f.name) {
            continue;
        }
        let params: Vec<Option<u16>> = f.params.iter().map(|p| is_i64(f, *p).then_some(8)).collect();
        if params.iter().any(Option::is_some) {
            plan.params.insert(f.name.clone(), params);
        }
        if f.ty.ret == Type::I64 {
            plan.results.insert(f.name.clone(), 8);
        }
    }
    for _ in 0..16 {
        let analyses: HashMap<String, Analysis> = module.functions.iter().map(|f| (f.name.clone(), analyze(f, &plan))).collect();
        let mut next = Plan::default();
        for f in &module.functions {
            if !eligible(&f.name) {
                continue;
            }
            let a = &analyses[&f.name];
            let params: Vec<Option<u16>> = f
                .params
                .iter()
                .map(|p| {
                    let d = *a.demand.get(p)?;
                    let w = round(d);
                    (d > 0 && w < FULL).then_some(w)
                })
                .collect();
            if params.iter().any(Option::is_some) {
                next.params.insert(f.name.clone(), params);
            }
            if f.ty.ret == Type::I64 {
                let returned = f.blocks.iter().filter_map(|b| match &b.terminator {
                    Terminator::Ret(Some((_, o))) => Some(a.zero_of(o)),
                    _ => None,
                });
                let w = round(returned.max().unwrap_or(FULL).max(1));
                let callers_agree = module.functions.iter().all(|g| {
                    let ga = &analyses[&g.name];
                    g.blocks.iter().flat_map(|b| &b.instrs).all(|i| match (&i.inst, i.result) {
                        (Inst::Call { callee: Callee::Direct(name), .. }, Some(r)) if *name == f.name => {
                            ga.demand.get(&r).is_none_or(|d| round(*d) <= w)
                        }
                        _ => true,
                    })
                });
                if w < FULL && callers_agree {
                    next.results.insert(f.name.clone(), w);
                }
            }
        }
        if next == plan {
            break;
        }
        plan = next;
    }

    let mut changed = false;
    for f in &mut module.functions {
        let a = analyze(f, &plan);
        changed |= rewrite(f, &a, &plan);
    }
    changed
}

/// The functions whose address the module takes: in the data, or as an operand.
fn address_taken(module: &Module) -> HashSet<String> {
    fn consts(c: &Const, out: &mut HashSet<String>) {
        match c {
            Const::Address { global, .. } | Const::Global(global) => {
                out.insert(global.clone());
            }
            Const::Array(items) | Const::Struct(items) => items.iter().for_each(|i| consts(i, out)),
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for g in &module.globals {
        // `ra_functions` maps image addresses to functions for the machine-mode resolver; it is
        // not a pointer the program holds.
        if g.name == "ra_functions" {
            continue;
        }
        if let Some(init) = &g.init {
            consts(init, &mut out);
        }
    }
    for f in &module.functions {
        for b in &f.blocks {
            for i in &b.instrs {
                let mut inst = i.inst.clone();
                for o in inst.operands_mut() {
                    match o {
                        Operand::Address { global, .. } | Operand::Global(global) => {
                            out.insert(global.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

/// The instructions a rewrite adds: before an instruction, or at the end of a block.
struct Edits {
    before: Vec<Instruction>,
    at_end: HashMap<BlockId, Vec<Instruction>>,
}

/// Narrows the candidates of `f` in place. Returns whether anything changed.
fn rewrite(f: &mut Function, a: &Analysis, plan: &Plan) -> bool {
    // The candidates: the width each one takes.
    let mut widths: HashMap<ValueId, u16> = HashMap::new();
    if let Some(params) = plan.params.get(&f.name) {
        for (p, w) in f.params.iter().zip(params) {
            if let Some(w) = w {
                widths.insert(*p, *w);
            }
        }
    }
    for b in &f.blocks {
        for i in &b.instrs {
            let Some(v) = i.result else { continue };
            if let Inst::Call { callee: Callee::Direct(name), .. } = &i.inst
                && let Some(w) = plan.results.get(name)
                && is_i64(f, v)
            {
                widths.insert(v, *w);
                continue;
            }
            if !is_i64(f, v) || !narrowable(&i.inst) {
                continue;
            }
            let d = a.demand.get(&v).copied().unwrap_or(FULL);
            let w = round(d);
            if d > 0 && w < FULL {
                widths.insert(v, w);
            }
        }
    }
    let own_result = plan.results.get(&f.name).copied();
    let narrow_switch = f.blocks.iter().any(|b| match &b.terminator {
        Terminator::Switch { ty, value, cases, .. } => *ty == Type::I64 && a.switch_width(value, cases) < FULL,
        _ => false,
    });
    if widths.is_empty() && own_result.is_none() && !narrow_switch && !calls_planned(f, plan) {
        return false;
    }
    for (v, w) in &widths {
        f.value_types[v.0 as usize] = Type::Int(*w);
    }
    if let Some(params) = plan.params.get(&f.name) {
        for (k, w) in params.iter().enumerate() {
            if let Some(w) = w {
                f.ty.params[k] = Type::Int(*w);
            }
        }
    }
    if let Some(w) = own_result {
        f.ty.ret = Type::Int(w);
    }

    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let mut edits = Edits { before: Vec::new(), at_end: HashMap::new() };
    let block_ids: Vec<BlockId> = f.blocks.iter().map(|b| b.id).collect();
    for bi in 0..block_ids.len() {
        let mut instrs = std::mem::take(&mut f.blocks[bi].instrs);
        let mut out = Vec::with_capacity(instrs.len());
        for mut i in instrs.drain(..) {
            let keep = rewrite_instruction(f, a, plan, &widths, &mut i, &mut edits, &mut replace);
            out.append(&mut edits.before);
            if keep {
                out.push(i);
            }
        }
        f.blocks[bi].instrs = out;
        // the return narrows to the planned width
        if let Some(w) = own_result
            && let Terminator::Ret(Some((ty, o))) = &f.blocks[bi].terminator
            && *ty == Type::I64
        {
            let o = o.clone();
            let narrowed = narrow(f, &o, w, &mut edits.before, "ret");
            let block = f.blocks[bi].id;
            edits.at_end.entry(block).or_default().append(&mut edits.before);
            f.blocks[bi].terminator = Terminator::Ret(Some((Type::Int(w), narrowed)));
        } else if let Terminator::Ret(Some((Type::Int(w), Operand::Local(v)))) = &f.blocks[bi].terminator
            && let Type::Int(have) = f.value_type(*v).clone()
            && have != *w
        {
            // a value narrowed by its own definition comes back to the function's width
            let (w, o) = (*w, Operand::Local(*v));
            let widened = narrow(f, &o, w, &mut edits.before, "ret");
            let block = f.blocks[bi].id;
            edits.at_end.entry(block).or_default().append(&mut edits.before);
            f.blocks[bi].terminator = Terminator::Ret(Some((Type::Int(w), widened)));
        }
        // a switch runs at the width its value and its cases fit in
        if let Terminator::Switch { ty, value, default, cases } = &f.blocks[bi].terminator
            && *ty == Type::I64
        {
            let w = a.switch_width(value, cases);
            if w < FULL {
                let (value, default, cases) = (value.clone(), *default, cases.clone());
                let narrowed = narrow(f, &value, w, &mut edits.before, "sw");
                let block = f.blocks[bi].id;
                edits.at_end.entry(block).or_default().append(&mut edits.before);
                f.blocks[bi].terminator = Terminator::Switch { ty: Type::Int(w), value: narrowed, default, cases };
            }
        }
    }
    for (block, mut added) in edits.at_end {
        if let Some(b) = f.block_mut(block) {
            b.instrs.append(&mut added);
        }
    }
    f.replace_uses(&replace);
    true
}

fn calls_planned(f: &Function, plan: &Plan) -> bool {
    f.blocks.iter().flat_map(|b| &b.instrs).any(|i| match &i.inst {
        Inst::Call { callee: Callee::Direct(name), .. } => plan.params.contains_key(name) || plan.results.contains_key(name),
        _ => false,
    })
}

/// Rewrites one instruction. Returns `false` when the instruction goes (its value is in `replace`).
fn rewrite_instruction(
    f: &mut Function,
    a: &Analysis,
    plan: &Plan,
    widths: &HashMap<ValueId, u16>,
    i: &mut Instruction,
    edits: &mut Edits,
    replace: &mut HashMap<ValueId, Operand>,
) -> bool {
    let block_of_pred = |f: &Function, pred: BlockId| f.block(pred).map(|b| b.id);
    if let Some(v) = i.result
        && let Some(&w) = widths.get(&v)
    {
        match &mut i.inst {
            Inst::Cast { cast, from: Type::Int(k), value, to } => {
                let k = *k;
                if k == w {
                    replace.insert(v, value.clone());
                    return false;
                }
                if k > w {
                    *cast = Cast::Trunc;
                }
                *to = Type::Int(w);
            }
            Inst::Bin { op, ty, lhs, rhs } => {
                let (l, r) = (lhs.clone(), rhs.clone());
                if *op == BinOp::Shl {
                    // a shift by the new width or more leaves nothing in the low bits
                    if let Operand::Int(c) = r
                        && c >= i128::from(w)
                    {
                        replace.insert(v, Operand::Int(0));
                        return false;
                    }
                    *rhs = r;
                } else {
                    *rhs = narrow(f, &r, w, &mut edits.before, "n");
                }
                *lhs = narrow(f, &l, w, &mut edits.before, "n");
                *ty = Type::Int(w);
            }
            Inst::Select { ty, then, els, .. } => {
                let (t, e) = (then.clone(), els.clone());
                *then = narrow(f, &t, w, &mut edits.before, "n");
                *els = narrow(f, &e, w, &mut edits.before, "n");
                *ty = Type::Int(w);
            }
            Inst::Phi { ty, incoming } => {
                let old = incoming.clone();
                for ((o, pred), slot) in old.iter().zip(incoming.iter_mut()) {
                    let mut added = Vec::new();
                    slot.0 = narrow(f, o, w, &mut added, "n");
                    if !added.is_empty()
                        && let Some(block) = block_of_pred(f, *pred)
                    {
                        edits.at_end.entry(block).or_default().append(&mut added);
                    }
                }
                *ty = Type::Int(w);
            }
            Inst::Call { fn_ty, .. } => {
                fn_ty.ret = Type::Int(w);
            }
            _ => {}
        }
    }
    match &mut i.inst {
        // A trunc of a narrowed value: gone when the widths agree, else from the new width.
        Inst::Cast { cast: Cast::Trunc, from, value: Operand::Local(src), to } => {
            let actual = f.value_type(*src).clone();
            if actual != *from {
                if actual == *to {
                    if let Some(v) = i.result {
                        replace.insert(v, Operand::Local(*src));
                    }
                    return false;
                }
                *from = actual;
            }
        }
        Inst::ICmp { cond, ty, lhs, rhs } if *ty == Type::I64 => {
            let w = a.compare_width(*cond, lhs, rhs);
            if w < FULL {
                let (l, r) = (lhs.clone(), rhs.clone());
                *lhs = narrow(f, &l, w, &mut edits.before, "c");
                *rhs = narrow(f, &r, w, &mut edits.before, "c");
                *ty = Type::Int(w);
            }
        }
        Inst::Call { fn_ty, callee: Callee::Direct(name), args, .. } if plan.params.contains_key(name) => {
            let params = &plan.params[name];
            for (k, (ty, arg)) in args.iter_mut().enumerate() {
                if let Some(Some(w)) = params.get(k) {
                    let o = arg.clone();
                    *arg = narrow(f, &o, *w, &mut edits.before, "a");
                    *ty = Type::Int(*w);
                    if let Some(p) = fn_ty.params.get_mut(k) {
                        *p = Type::Int(*w);
                    }
                }
            }
        }
        _ => {}
    }
    true
}

/// `o` at width `w`: a constant masked, a value of that width as it is, a wider value through a
/// `trunc` added to `out`, a narrower one through a `zext`.
fn narrow(f: &mut Function, o: &Operand, w: u16, out: &mut Vec<Instruction>, hint: &str) -> Operand {
    match o {
        Operand::Int(_) => mask_constant(o, w),
        Operand::Local(v) => {
            let actual = f.value_type(*v).clone();
            let Type::Int(have) = actual else { return o.clone() };
            if have == w {
                return o.clone();
            }
            let cast = if have > w { Cast::Trunc } else { Cast::ZExt };
            let new = f.new_value(hint, Type::Int(w));
            out.push(Instruction {
                result: Some(new),
                inst: Inst::Cast { cast, from: Type::Int(have), value: o.clone(), to: Type::Int(w) },
                address: None,
            });
            Operand::Local(new)
        }
        other => other.clone(),
    }
}

fn mask_constant(o: &Operand, w: u16) -> Operand {
    match o {
        Operand::Int(c) if w < 128 => Operand::Int(c & ((1i128 << w) - 1)),
        other => other.clone(),
    }
}
