//! Conditions: the flag formulas of a `cmp` or `sub` followed by a conditional become one direct
//! comparison of the operands. `cmp a, b; jl` lifts to `sf = (a - b) < 0`, `of = ...`, `l = sf ^ of`;
//! here `l` becomes `icmp slt a, b`. The pass rewrites the defining instruction of each condition
//! in place, and the flag arithmetic dies with dead-code elimination.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_ir::{BinOp, Function, ICond, Inst, Operand, Type, ValueId};

/// A comparison of two operands.
#[derive(Debug, Clone, PartialEq)]
struct Cmp {
    cond: ICond,
    ty: Type,
    lhs: Operand,
    rhs: Operand,
}

/// A flag of a subtraction, before it combines into a comparison.
#[derive(Debug, Clone, PartialEq)]
enum Flag {
    /// `sf`: the difference is negative.
    Sign { ty: Type, lhs: Operand, rhs: Operand },
    /// `of`: the subtraction overflowed.
    Overflow { ty: Type, lhs: Operand, rhs: Operand },
}

#[derive(Debug, Clone, PartialEq)]
enum Known {
    Cmp(Cmp),
    Flag(Flag),
}

pub fn run(f: &mut Function) -> bool {
    let mut defs: HashMap<ValueId, Inst> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Some(v) = i.result {
                defs.insert(v, i.inst.clone());
            }
        }
    }
    // Every value that is a comparison in disguise.
    let mut rewrites: HashMap<ValueId, Cmp> = HashMap::new();
    let mut memo: HashMap<ValueId, Option<Known>> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            let Some(v) = i.result else { continue };
            if let Inst::ICmp { .. } = i.inst {
                continue; // already direct, or a flag that only matters in a combination
            }
            if let Some(Known::Cmp(cmp)) = classify(v, &defs, &mut memo) {
                rewrites.insert(v, cmp);
            }
        }
    }
    if rewrites.is_empty() {
        return false;
    }
    for b in &mut f.blocks {
        for i in &mut b.instrs {
            if let Some(cmp) = i.result.and_then(|v| rewrites.get(&v)) {
                i.inst = Inst::ICmp { cond: cmp.cond, ty: cmp.ty.clone(), lhs: cmp.lhs.clone(), rhs: cmp.rhs.clone() };
            }
        }
    }
    true
}

/// The subtraction a value is, as `(lhs, rhs)`.
fn as_sub<'a>(o: &Operand, defs: &'a HashMap<ValueId, Inst>) -> Option<(&'a Type, &'a Operand, &'a Operand)> {
    let Operand::Local(v) = o else { return None };
    match defs.get(v)? {
        Inst::Bin { op: BinOp::Sub, ty, lhs, rhs } => Some((ty, lhs, rhs)),
        _ => None,
    }
}

fn classify(v: ValueId, defs: &HashMap<ValueId, Inst>, memo: &mut HashMap<ValueId, Option<Known>>) -> Option<Known> {
    if let Some(k) = memo.get(&v) {
        return k.clone();
    }
    memo.insert(v, None); // a guard against cycles through phis
    let known = classify_inner(v, defs, memo);
    memo.insert(v, known.clone());
    known
}

fn classify_operand(o: &Operand, defs: &HashMap<ValueId, Inst>, memo: &mut HashMap<ValueId, Option<Known>>) -> Option<Known> {
    match o {
        Operand::Local(v) => classify(*v, defs, memo),
        _ => None,
    }
}

fn classify_inner(v: ValueId, defs: &HashMap<ValueId, Inst>, memo: &mut HashMap<ValueId, Option<Known>>) -> Option<Known> {
    let inst = defs.get(&v)?;
    match inst {
        // zf of a subtraction: `(a - b) == 0` is `a == b`; sf: `(a - b) < 0` is a flag; of:
        // `((a ^ b) & (a ^ d)) < 0` with `d = a - b` is a flag.
        Inst::ICmp { cond, ty: _, lhs, rhs: Operand::Int(0) } => {
            if *cond == ICond::Slt
                && let Some(of) = overflow_of(lhs, defs)
            {
                return Some(Known::Flag(of));
            }
            let (ty, a, b) = as_sub(lhs, defs)?;
            match cond {
                ICond::Eq => Some(Known::Cmp(Cmp { cond: ICond::Eq, ty: ty.clone(), lhs: a.clone(), rhs: b.clone() })),
                ICond::Ne => Some(Known::Cmp(Cmp { cond: ICond::Ne, ty: ty.clone(), lhs: a.clone(), rhs: b.clone() })),
                ICond::Slt => Some(Known::Flag(Flag::Sign { ty: ty.clone(), lhs: a.clone(), rhs: b.clone() })),
                _ => None,
            }
        }
        // A direct comparison, such as cf of a subtraction: `a <u b`.
        Inst::ICmp { cond, ty, lhs, rhs } if *ty != Type::I1 => {
            Some(Known::Cmp(Cmp { cond: *cond, ty: ty.clone(), lhs: lhs.clone(), rhs: rhs.clone() }))
        }
        // `not c`.
        Inst::Bin { op: BinOp::Xor, ty: Type::I1, lhs, rhs: Operand::Int(1) } => match classify_operand(lhs, defs, memo)? {
            Known::Cmp(c) => Some(Known::Cmp(Cmp { cond: c.cond.inverse(), ..c })),
            Known::Flag(_) => None,
        },
        // `sf ^ of` is `a <s b`.
        Inst::Bin { op: BinOp::Xor, ty: Type::I1, lhs, rhs } => {
            let (x, y) = (classify_operand(lhs, defs, memo)?, classify_operand(rhs, defs, memo)?);
            match (x, y) {
                (Known::Flag(Flag::Sign { ty, lhs: a, rhs: b }), Known::Flag(Flag::Overflow { lhs: oa, rhs: ob, .. }))
                | (Known::Flag(Flag::Overflow { lhs: oa, rhs: ob, .. }), Known::Flag(Flag::Sign { ty, lhs: a, rhs: b }))
                    if a == oa && b == ob =>
                {
                    Some(Known::Cmp(Cmp { cond: ICond::Slt, ty, lhs: a, rhs: b }))
                }
                _ => None,
            }
        }
        // `l | zf` is `a <=s b`; `cf | zf` is `a <=u b`.
        Inst::Bin { op: BinOp::Or, ty: Type::I1, lhs, rhs } => {
            let (x, y) = (classify_operand(lhs, defs, memo)?, classify_operand(rhs, defs, memo)?);
            let (Known::Cmp(p), Known::Cmp(q)) = (x, y) else { return None };
            for (c, z) in [(&p, &q), (&q, &p)] {
                if z.cond == ICond::Eq && c.lhs == z.lhs && c.rhs == z.rhs && c.ty == z.ty {
                    let cond = match c.cond {
                        ICond::Slt => ICond::Sle,
                        ICond::Ult => ICond::Ule,
                        ICond::Sgt => ICond::Sge,
                        ICond::Ugt => ICond::Uge,
                        _ => continue,
                    };
                    return Some(Known::Cmp(Cmp { cond, ty: c.ty.clone(), lhs: c.lhs.clone(), rhs: c.rhs.clone() }));
                }
            }
            None
        }
        _ => None,
    }
}

/// The overflow flag of a subtraction when `o` is `(a ^ b) & (a ^ d)` with `d = a - b`.
fn overflow_of(o: &Operand, defs: &HashMap<ValueId, Inst>) -> Option<Flag> {
    let Inst::Bin { op: BinOp::And, lhs: x, rhs: y, .. } = defs.get(&o.local()?)? else { return None };
    let (xs, ys) = (xor_of(x, defs)?, xor_of(y, defs)?);
    for (p, q) in [(&xs, &ys), (&ys, &xs)] {
        if let Some((sty, sa, sb)) = as_sub(&q.1, defs)
            && p.0 == *sa
            && p.1 == *sb
            && q.0 == *sa
        {
            return Some(Flag::Overflow { ty: sty.clone(), lhs: sa.clone(), rhs: sb.clone() });
        }
    }
    None
}

/// The operands of a value that is `x ^ y`.
fn xor_of(o: &Operand, defs: &HashMap<ValueId, Inst>) -> Option<(Operand, Operand)> {
    let Operand::Local(v) = o else { return None };
    match defs.get(v)? {
        Inst::Bin { op: BinOp::Xor, lhs, rhs, .. } => Some((lhs.clone(), rhs.clone())),
        _ => None,
    }
}
