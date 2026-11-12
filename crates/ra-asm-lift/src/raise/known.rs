//! Known values: a pure instruction identical to one that dominates it becomes that one (value
//! numbering over the dominator tree), a condition that a dominating branch decided is that
//! constant on the branch's side, the operand a dominating equality compared with a constant is
//! that constant, a comparison of two constants is its answer, a branch on a constant becomes a
//! jump, and the blocks nothing reaches go. The
//! lift's own guards are where this shows: its division-by-zero check repeats the program's.
//! Memory too: a load from a read-only section is the bytes the image holds there, and a load of
//! a slot a store of the same block wrote, with no call and no aliasing store between, is the
//! value stored. The arguments a 32-bit program pushes for an import, and the ones a compiler
//! writes with one vector store, come back as the constants they are.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::dom::{dominators, reverse_post_order};
use ra_asm_ir::{BinOp, BlockId, Cast, Const, Function, Global, ICond, Inst, Operand, Terminator, Type, ValueId};

/// Runs once; returns whether anything changed. `globals` are the module's, for the loads from
/// its read-only sections.
pub fn run(f: &mut Function, globals: &[Global]) -> bool {
    let mut changed = number_values(f);
    changed |= fold_constant_loads(f, globals);
    changed |= forward_stores(f);
    changed |= fold_opaque(f);
    changed |= fold_compares(f);
    changed |= apply_branch_facts(f);
    changed |= fold_branches(f);
    changed
}

/// Where a pointer points: a byte offset from a root. Two places of one root are the same memory
/// when their ranges overlap; an `alloca` never overlaps another or a global; an opaque root (a
/// parameter, a loaded pointer) may overlap anything.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Root {
    Global(String),
    Alloca(ValueId),
    Opaque(ValueId),
}

impl Root {
    fn may_alias(&self, offset: i64, size: u64, other: &Self, other_offset: i64, other_size: u64) -> bool {
        if self == other {
            return offset < other_offset.saturating_add(other_size as i64) && other_offset < offset.saturating_add(size as i64);
        }
        matches!(self, Self::Opaque(_)) || matches!(other, Self::Opaque(_))
    }
}

/// The place a pointer operand names, through the address arithmetic that built it.
pub(super) fn place(o: &Operand, defs: &HashMap<ValueId, Inst>) -> Option<(Root, i64)> {
    match o {
        Operand::Address { global, offset } => Some((Root::Global(global.clone()), *offset)),
        Operand::Global(global) => Some((Root::Global(global.clone()), 0)),
        Operand::Local(v) => match defs.get(v) {
            Some(Inst::Alloca { .. }) => Some((Root::Alloca(*v), 0)),
            Some(Inst::Gep { base_ty, ptr, indices, .. }) => {
                let (root, base) = place(ptr, defs)?;
                Some((root, base.checked_add(gep_offset(base_ty, indices)?)?))
            }
            Some(Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt | Cast::Bitcast | Cast::ZExt, value, .. }) => place(value, defs),
            // A 32-bit program reads its stack pointer as the low half of the register, and its
            // frame and data lie below 4 GiB in the port, so the low half is the whole address.
            Some(Inst::Cast { cast: Cast::Trunc, value, to: Type::Int(32), .. }) => place(value, defs),
            Some(Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub), ty, lhs, rhs: Operand::Int(k) }) => {
                let (root, base) = place(lhs, defs)?;
                let k = signed(ty, *k)?;
                Some((root, if *op == BinOp::Add { base.checked_add(k)? } else { base.checked_sub(k)? }))
            }
            Some(_) => Some((Root::Opaque(*v), 0)),
            None => Some((Root::Opaque(*v), 0)),
        },
        _ => None,
    }
}

/// The byte offset of a `getelementptr` with constant indices over the integer and array types
/// the lift addresses memory with.
fn gep_offset(base_ty: &Type, indices: &[(Type, Operand)]) -> Option<i64> {
    let mut ty = base_ty;
    let mut offset = 0i64;
    for (n, (_, index)) in indices.iter().enumerate() {
        let Operand::Int(k) = index else { return None };
        let k = i64::try_from(*k).ok()?;
        if n > 0 {
            let Type::Array(_, element) = ty else { return None };
            ty = element;
        }
        offset = offset.checked_add(k.checked_mul(i64::try_from(ty.size(8)?).ok()?)?)?;
    }
    Some(offset)
}

/// A constant of `ty` as the signed number it is.
fn signed(ty: &Type, k: i128) -> Option<i64> {
    let bits = ty.bits()?;
    let k = if bits < 128 && k >= 1i128 << (bits - 1) { k - (1i128 << bits) } else { k };
    i64::try_from(k).ok()
}

pub(super) fn definitions(f: &Function) -> HashMap<ValueId, Inst> {
    f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| i.result.map(|v| (v, i.inst.clone()))).collect()
}

/// Replaces the values of `replace` everywhere and drops the instructions that defined them.
fn substitute(f: &mut Function, replace: &HashMap<ValueId, Operand>) -> bool {
    if replace.is_empty() {
        return false;
    }
    for b in &mut f.blocks {
        b.instrs.retain(|i| !i.result.is_some_and(|v| replace.contains_key(&v)));
    }
    f.replace_uses(replace);
    true
}

/// A load from a read-only section at a known address is the bytes the image holds there. The
/// words the loader patches (relocated addresses, the import slots) are not bytes in the global's
/// initializer and stay loads.
fn fold_constant_loads(f: &mut Function, globals: &[Global]) -> bool {
    let defs = definitions(f);
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let (Some(v), Inst::Load { ty: Type::Int(bits), ptr, volatile: false, .. }) = (i.result, &i.inst) else { continue };
        if *bits % 8 != 0 || *bits > 128 {
            continue;
        }
        let Some((Root::Global(name), offset)) = place(ptr, &defs) else { continue };
        let Some(global) = globals.iter().find(|g| g.name == name && g.constant) else { continue };
        let Some(bytes) = bytes_of(global, offset, u64::from(*bits / 8)) else { continue };
        let mut value = 0u128;
        for (n, byte) in bytes.iter().enumerate() {
            value |= u128::from(*byte) << (8 * n);
        }
        replace.insert(v, Operand::Int(value as i128));
    }
    substitute(f, &replace)
}

/// `size` verbatim bytes of a global's initializer at `offset`, when they lie in one byte run.
fn bytes_of(global: &Global, offset: i64, size: u64) -> Option<&[u8]> {
    let offset = u64::try_from(offset).ok()?;
    match (&global.ty, global.init.as_ref()?) {
        (_, Const::Bytes(bytes)) => run_of(bytes, offset, size),
        (Type::Packed(fields), Const::Struct(items)) => {
            let mut at = 0u64;
            for (field, item) in fields.iter().zip(items) {
                let field_size = field.size(8)?;
                if offset < at + field_size {
                    return match item {
                        Const::Bytes(bytes) if offset >= at => run_of(bytes, offset - at, size),
                        _ => None,
                    };
                }
                at += field_size;
            }
            None
        }
        _ => None,
    }
}

/// `size` bytes of a run at `start`, when the run has them.
fn run_of(bytes: &[u8], start: u64, size: u64) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    bytes.get(start..start.checked_add(usize::try_from(size).ok()?)?)
}

/// A slot a block has stored to: where, how wide, and what.
struct Slot {
    root: Root,
    offset: i64,
    size: u64,
    ty: Type,
    value: Operand,
}

/// A load of a slot the same block stored to, with no call and no store that may alias between,
/// is the value stored: the whole value for a load of the slot's own width and type, and the
/// bytes it covers for a narrower load of a stored constant.
fn forward_stores(f: &mut Function) -> bool {
    let defs = definitions(f);
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    for b in &f.blocks {
        let mut slots: Vec<Slot> = Vec::new();
        for i in &b.instrs {
            match &i.inst {
                Inst::Store { ty, value, ptr, volatile: false, .. } => match (place(ptr, &defs), ty.size(8)) {
                    (Some((root, offset)), Some(size)) => {
                        slots.retain(|s| !s.root.may_alias(s.offset, s.size, &root, offset, size));
                        slots.push(Slot { root, offset, size, ty: ty.clone(), value: value.clone() });
                    }
                    _ => slots.clear(),
                },
                Inst::Store { .. } | Inst::Call { .. } => slots.clear(),
                Inst::Load { ty: ty @ Type::Int(bits), ptr, volatile: false, .. } => {
                    let Some(v) = i.result else { continue };
                    let Some((root, offset)) = place(ptr, &defs) else { continue };
                    let size = u64::from(bits.div_ceil(8));
                    let covering = slots.iter().rev().find(|s| {
                        s.root == root && s.offset <= offset && offset.saturating_add(size as i64) <= s.offset.saturating_add(s.size as i64)
                    });
                    let Some(slot) = covering else { continue };
                    if slot.offset == offset && slot.size == size && slot.ty == *ty {
                        replace.insert(v, slot.value.clone());
                    } else if let Operand::Int(c) = slot.value
                        && *bits < 128
                    {
                        let shift = (offset - slot.offset) * 8;
                        replace.insert(v, Operand::Int((c >> shift) & ((1i128 << bits) - 1)));
                    }
                }
                _ => {}
            }
        }
    }
    substitute(f, &replace)
}

/// Whether an instruction's value depends on its operands alone.
pub(super) fn pure(inst: &Inst) -> bool {
    !inst.has_side_effects()
        && !matches!(
            inst,
            Inst::Alloca { .. } | Inst::Phi { .. } | Inst::Load { .. } | Inst::Call { .. } | Inst::Freeze { .. } | Inst::Store { .. }
        )
}

/// Value numbering: in reverse post-order, each pure instruction looks for an identical one in
/// its block or a dominator, and becomes it.
fn number_values(f: &mut Function) -> bool {
    let order = reverse_post_order(f);
    let preds = f.predecessors();
    let idom = dominators(&order, &preds);
    let mut tables: HashMap<BlockId, HashMap<String, ValueId>> = HashMap::new();
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let resolve = |o: &mut Operand, replace: &HashMap<ValueId, Operand>| {
        while let Operand::Local(v) = o {
            let Some(r) = replace.get(v) else { break };
            *o = r.clone();
        }
    };
    for b in &order {
        let Some(bi) = f.blocks.iter().position(|x| x.id == *b) else { continue };
        let mut local: HashMap<String, ValueId> = HashMap::new();
        let mut kept = Vec::with_capacity(f.blocks[bi].instrs.len());
        for mut i in std::mem::take(&mut f.blocks[bi].instrs) {
            for o in i.inst.operands_mut() {
                resolve(o, &replace);
            }
            let Some(v) = i.result else {
                kept.push(i);
                continue;
            };
            if !pure(&i.inst) {
                kept.push(i);
                continue;
            }
            let key = format!("{:?}", i.inst);
            let mut found = local.get(&key).copied();
            let mut up = *b;
            while found.is_none() {
                let Some(&d) = idom.get(&up) else { break };
                if d == up {
                    break;
                }
                found = tables.get(&d).and_then(|t| t.get(&key)).copied();
                up = d;
            }
            if let Some(earlier) = found {
                replace.insert(v, Operand::Local(earlier));
            } else {
                local.insert(key, v);
                kept.push(i);
            }
        }
        for o in f.blocks[bi].terminator.operands_mut() {
            resolve(o, &replace);
        }
        f.blocks[bi].instrs = kept;
        tables.insert(*b, local);
    }
    if replace.is_empty() {
        return false;
    }
    f.replace_uses(&replace);
    true
}

/// An expression in one or two unknowns over the operations whose low bits depend on their
/// operands' low bits alone.
enum Expr {
    Var(usize),
    Const(u128),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// A sign extension from this many bits.
    SExt(Box<Expr>, u32),
}

impl Expr {
    fn eval(&self, inputs: &[u128; 2]) -> u128 {
        match self {
            Self::Var(i) => inputs[*i],
            Self::Const(c) => *c,
            Self::Bin(op, a, b) => {
                let (a, b) = (a.eval(inputs), b.eval(inputs));
                match op {
                    BinOp::Add => a.wrapping_add(b),
                    BinOp::Sub => a.wrapping_sub(b),
                    BinOp::Mul => a.wrapping_mul(b),
                    BinOp::And => a & b,
                    BinOp::Or => a | b,
                    BinOp::Xor => a ^ b,
                    BinOp::Shl => {
                        if b < 128 {
                            a << b
                        } else {
                            0
                        }
                    }
                    _ => 0,
                }
            }
            Self::SExt(e, bits) => {
                let v = e.eval(inputs) & low_mask(*bits);
                if *bits < 128 && *bits > 0 && (v >> (bits - 1)) & 1 == 1 { v | !low_mask(*bits) } else { v }
            }
        }
    }
}

fn low_mask(bits: u32) -> u128 {
    if bits >= 128 { u128::MAX } else { (1u128 << bits) - 1 }
}

/// The expression tree of `o`; a value outside the closed operations is an unknown.
fn expression(o: &Operand, defs: &HashMap<ValueId, Inst>, vars: &mut Vec<ValueId>, depth: usize) -> Option<Expr> {
    if depth > 24 {
        return None;
    }
    let Operand::Local(v) = o else {
        return match o {
            Operand::Int(c) => Some(Expr::Const(*c as u128)),
            _ => None,
        };
    };
    let mut unknown = |v: ValueId| {
        if let Some(i) = vars.iter().position(|x| *x == v) {
            return Some(Expr::Var(i));
        }
        if vars.len() == 2 {
            return None;
        }
        vars.push(v);
        Some(Expr::Var(vars.len() - 1))
    };
    match defs.get(v) {
        Some(Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::And | BinOp::Or | BinOp::Xor), lhs, rhs, .. }) => {
            Some(Expr::Bin(*op, Box::new(expression(lhs, defs, vars, depth + 1)?), Box::new(expression(rhs, defs, vars, depth + 1)?)))
        }
        Some(Inst::Bin { op: BinOp::Shl, lhs, rhs: Operand::Int(c), .. }) => {
            Some(Expr::Bin(BinOp::Shl, Box::new(expression(lhs, defs, vars, depth + 1)?), Box::new(Expr::Const(*c as u128))))
        }
        Some(Inst::Cast { cast: Cast::Trunc | Cast::ZExt, value, .. }) => expression(value, defs, vars, depth + 1),
        Some(Inst::Cast { cast: Cast::SExt, from, value, .. }) => {
            Some(Expr::SExt(Box::new(expression(value, defs, vars, depth + 1)?), from.bits()?))
        }
        _ => unknown(*v),
    }
}

/// An opaque predicate is a test whose answer the obfuscator knows: `x * (x + 1)` is even for
/// every `x`, a square is 0 or 1 modulo 4. The low bits of a sum, a product, a shift left or a
/// bitwise combination depend on the low bits of the operands alone, so the low `k` bits of such
/// an expression in one or two unknowns take their values over the `2^k` or `4^k` low-bit
/// assignments, which the pass tries: an `and` with a low mask, a remainder by a power of two or
/// a truncation to a few bits that comes out the same for all of them is that constant.
fn fold_opaque(f: &mut Function) -> bool {
    let defs = definitions(f);
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Some(v) = i.result else { continue };
        let low = |m: i128| (m > 0 && m < 256 && m & (m + 1) == 0).then(|| m.count_ones());
        let (expr, bits) = match &i.inst {
            Inst::Bin { op: BinOp::And, lhs, rhs: Operand::Int(m), .. } => match low(*m) {
                Some(bits) => (lhs, bits),
                None => continue,
            },
            Inst::Bin { op: BinOp::And, lhs: Operand::Int(m), rhs, .. } => match low(*m) {
                Some(bits) => (rhs, bits),
                None => continue,
            },
            Inst::Bin { op: BinOp::URem, lhs, rhs: Operand::Int(m), .. } => match low(*m - 1) {
                Some(bits) => (lhs, bits),
                None => continue,
            },
            Inst::Cast { cast: Cast::Trunc, value, to: Type::Int(k), .. } if *k <= 8 => (value, u32::from(*k)),
            _ => continue,
        };
        let mut vars: Vec<ValueId> = Vec::new();
        let Some(tree) = expression(expr, &defs, &mut vars, 0) else { continue };
        if vars.is_empty() {
            continue;
        }
        let count = 1u32 << (bits * vars.len() as u32);
        let mut folded: Option<u128> = None;
        for a in 0..count {
            let inputs = [u128::from(a & ((1 << bits) - 1)), u128::from(a >> bits)];
            let value = tree.eval(&inputs) & low_mask(bits);
            match folded {
                None => folded = Some(value),
                Some(r) if r == value => {}
                Some(_) => {
                    folded = None;
                    break;
                }
            }
        }
        if let Some(r) = folded {
            replace.insert(v, Operand::Int(r as i128));
        }
    }
    substitute(f, &replace)
}

/// A comparison of two constants is its answer. The opaque predicate's test is one once its
/// operand folded, and the cleanup folds the arithmetic of constants but not their comparison.
fn fold_compares(f: &mut Function) -> bool {
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let (Some(v), Inst::ICmp { cond, ty: Type::Int(bits), lhs: Operand::Int(a), rhs: Operand::Int(b) }) = (i.result, &i.inst) else {
            continue;
        };
        let bits = u32::from(*bits);
        if bits == 0 {
            continue;
        }
        let (a, b) = (*a as u128 & low_mask(bits), *b as u128 & low_mask(bits));
        let signed = |x: u128| if bits < 128 && (x >> (bits - 1)) & 1 == 1 { (x | !low_mask(bits)) as i128 } else { x as i128 };
        let (sa, sb) = (signed(a), signed(b));
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
        replace.insert(v, Operand::Int(i128::from(r)));
    }
    substitute(f, &replace)
}

/// A fact a branch establishes on one side: a value is a constant there.
#[derive(Clone)]
struct Fact {
    value: ValueId,
    constant: Operand,
}

/// On the side of a branch that only the branch reaches, the condition is known, and so is the
/// operand of an equality with a constant. The facts hold in the dominator subtree of that side.
fn apply_branch_facts(f: &mut Function) -> bool {
    let order = reverse_post_order(f);
    let preds = f.predecessors();
    let idom = dominators(&order, &preds);
    let mut defs: HashMap<ValueId, Inst> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Some(v) = i.result {
                defs.insert(v, i.inst.clone());
            }
        }
    }
    // The facts that hold from each block on.
    let mut facts_at: HashMap<BlockId, Vec<Fact>> = HashMap::new();
    for b in &f.blocks {
        let Terminator::CondBr { cond: Operand::Local(c), then, els } = &b.terminator else { continue };
        for (side, truth) in [(*then, true), (*els, false)] {
            if side == b.id || preds.get(&side).is_none_or(|p| p.len() != 1) {
                continue;
            }
            let mut facts = vec![Fact { value: *c, constant: Operand::Int(i128::from(truth)) }];
            if let Some(Inst::ICmp { cond, lhs: Operand::Local(x), rhs: k @ Operand::Int(_), .. }) = defs.get(c)
                && ((*cond == ICond::Eq && truth) || (*cond == ICond::Ne && !truth))
            {
                facts.push(Fact { value: *x, constant: k.clone() });
            }
            facts_at.entry(side).or_default().extend(facts);
        }
    }
    if facts_at.is_empty() {
        return false;
    }
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        // the facts of this block and of every dominator
        let mut known: HashMap<ValueId, Operand> = HashMap::new();
        let mut x = f.blocks[bi].id;
        loop {
            if let Some(facts) = facts_at.get(&x) {
                for fact in facts {
                    known.entry(fact.value).or_insert_with(|| fact.constant.clone());
                }
            }
            let Some(&up) = idom.get(&x) else { break };
            if up == x {
                break;
            }
            x = up;
        }
        if known.is_empty() {
            continue;
        }
        let block = &mut f.blocks[bi];
        for i in &mut block.instrs {
            if matches!(i.inst, Inst::Phi { .. }) {
                continue; // an incoming value belongs to the predecessor's facts, not this block's
            }
            for o in i.inst.operands_mut() {
                if let Operand::Local(v) = o
                    && let Some(k) = known.get(v)
                {
                    *o = k.clone();
                    changed = true;
                }
            }
        }
        for o in block.terminator.operands_mut() {
            if let Operand::Local(v) = o
                && let Some(k) = known.get(v)
            {
                *o = k.clone();
                changed = true;
            }
        }
    }
    changed
}

/// A branch on a constant becomes a jump, and the blocks the entry no longer reaches go, with
/// the phi entries that came from them.
pub(super) fn fold_branches(f: &mut Function) -> bool {
    let mut changed = false;
    for b in &mut f.blocks {
        let next = match &b.terminator {
            Terminator::CondBr { cond: Operand::Int(c), then, els } => Some(if *c & 1 == 1 { *then } else { *els }),
            Terminator::CondBr { cond: Operand::Undef, then, .. } => Some(*then),
            Terminator::Switch { value: Operand::Int(c), default, cases, .. } => {
                Some(cases.iter().find(|(k, _)| k == c).map_or(*default, |(_, t)| *t))
            }
            _ => None,
        };
        if let Some(target) = next {
            b.terminator = Terminator::Br(target);
            changed = true;
        }
    }
    // Reachability from the entry.
    let mut reached: HashSet<BlockId> = HashSet::new();
    let mut work = vec![f.blocks[0].id];
    while let Some(b) = work.pop() {
        if !reached.insert(b) {
            continue;
        }
        if let Some(block) = f.block(b) {
            work.extend(block.terminator.successors());
        }
    }
    if reached.len() != f.blocks.len() {
        f.blocks.retain(|b| reached.contains(&b.id));
        changed = true;
    }
    // A phi keeps the entries of its predecessors only.
    let preds = f.predecessors();
    for b in &mut f.blocks {
        let mine = preds.get(&b.id).cloned().unwrap_or_default();
        for i in &mut b.instrs {
            if let Inst::Phi { incoming, .. } = &mut i.inst {
                let before = incoming.len();
                incoming.retain(|(_, from)| mine.contains(from));
                changed |= incoming.len() != before;
            }
        }
    }
    changed
}
