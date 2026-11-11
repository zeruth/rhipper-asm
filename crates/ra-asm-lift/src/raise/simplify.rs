//! Local simplifications: the identities the lift writes to materialize a constant or to keep a
//! value's type (`or x, 0`, `and x, -1`, `add x, 0`, a cast to the same type), and constant folding
//! of integer arithmetic on two constants.

use std::collections::HashMap;

use ra_asm_ir::{BinOp, Function, Inst, Operand, Type, ValueId};

/// Runs once; returns whether anything changed.
pub fn run(f: &mut Function) -> bool {
    let mut map: HashMap<ValueId, Operand> = HashMap::new();
    // The casts each value came through, for the chains the lift writes: a register read widens
    // on the write and narrows on the next read.
    let mut casts: HashMap<ValueId, (ra_asm_ir::Cast, Type, Operand)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(result), Inst::Cast { cast, from, value, .. }) = (i.result, &i.inst) {
                casts.insert(result, (*cast, from.clone(), value.clone()));
            }
        }
    }
    // The add and sub of a constant each value is, to fold chains: `(x - 56) + 52` is `x - 4`.
    let mut offsets: HashMap<ValueId, (Operand, i128)> = HashMap::new();
    let uses = f.use_counts();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(result), Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub), ty: Type::Int(64), lhs, rhs: Operand::Int(c) }) =
                (i.result, &i.inst)
            {
                let c = if *op == BinOp::Add { *c } else { -*c };
                let (base, total) = match lhs {
                    Operand::Local(v) if uses.get(v).copied() == Some(1) => match offsets.get(v) {
                        Some((base, k)) => (base.clone(), k + c),
                        None => (lhs.clone(), c),
                    },
                    _ => (lhs.clone(), c),
                };
                offsets.insert(result, (base, total));
            }
        }
    }
    let mut rewrite: HashMap<ValueId, Inst> = HashMap::new();
    let mut defs: HashMap<ValueId, &Inst> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Some(v) = i.result {
                defs.insert(v, &i.inst);
            }
        }
    }
    for b in &f.blocks {
        for i in &b.instrs {
            let Some(result) = i.result else { continue };
            if let Some(replacement) = simplify(&i.inst) {
                map.insert(result, replacement);
                continue;
            }
            // `cdq`: `(zext (x >>s 31) << 32) | zext x` is `sext x`.
            if let Some(x) = sign_extension_idiom(&i.inst, &defs) {
                rewrite.insert(result, Inst::Cast { cast: ra_asm_ir::Cast::SExt, from: Type::I32, value: x, to: Type::I64 });
                continue;
            }
            if let Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::Int(64), lhs: Operand::Local(inner), .. } = &i.inst
                && let Some((base, total)) = offsets.get(&result)
                && offsets.contains_key(inner)
                && uses.get(inner).copied() == Some(1)
            {
                let (op, c) = if *total < 0 { (BinOp::Sub, -*total) } else { (BinOp::Add, *total) };
                rewrite.insert(result, Inst::Bin { op, ty: Type::I64, lhs: base.clone(), rhs: Operand::Int(c) });
            }
            // trunc (zext x: A to B) to A  ==  x, and the same through a sext.
            if let Inst::Cast { cast: ra_asm_ir::Cast::Trunc, value: Operand::Local(v), to, .. } = &i.inst
                && let Some((ra_asm_ir::Cast::ZExt | ra_asm_ir::Cast::SExt, inner_from, inner)) = casts.get(v)
                && inner_from == to
            {
                map.insert(result, inner.clone());
            }
            // zext (zext x) and sext (sext x) are one extension.
            if let Inst::Cast { cast: outer @ (ra_asm_ir::Cast::ZExt | ra_asm_ir::Cast::SExt), value: Operand::Local(v), to, .. } = &i.inst
                && let Some((inner_cast, inner_from, inner)) = casts.get(v)
                && inner_cast == outer
            {
                rewrite.insert(result, Inst::Cast { cast: *outer, from: inner_from.clone(), value: inner.clone(), to: to.clone() });
            }
        }
    }
    if map.is_empty() && rewrite.is_empty() {
        return false;
    }
    for b in &mut f.blocks {
        b.instrs.retain(|i| !i.result.is_some_and(|v| map.contains_key(&v)));
        for i in &mut b.instrs {
            if let Some(v) = i.result
                && let Some(inst) = rewrite.remove(&v)
            {
                i.inst = inst;
            }
        }
    }
    f.replace_uses(&map);
    true
}

/// Common subexpressions within a block: a pure instruction identical to an earlier one in the
/// same block becomes that one. Within a block the earlier one dominates. Returns whether anything
/// changed.
pub fn cse(f: &mut Function) -> bool {
    let mut map: HashMap<ValueId, Operand> = HashMap::new();
    for b in &f.blocks {
        let mut seen: HashMap<String, ValueId> = HashMap::new();
        for i in &b.instrs {
            let Some(v) = i.result else { continue };
            if i.inst.has_side_effects() || matches!(i.inst, Inst::Alloca { .. } | Inst::Phi { .. } | Inst::Load { .. } | Inst::Call { .. })
            {
                continue;
            }
            let key = format!("{:?}", i.inst);
            match seen.get(&key) {
                Some(&earlier) => {
                    map.insert(v, Operand::Local(earlier));
                }
                None => {
                    seen.insert(key, v);
                }
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

/// The `x` of `or (shl (zext (ashr i32 x, 31)), 32), (zext i32 x)`: the sign extension that
/// `cdq` or a shift pair writes.
fn sign_extension_idiom(inst: &Inst, defs: &HashMap<ValueId, &Inst>) -> Option<Operand> {
    let Inst::Bin { op: BinOp::Or, ty: Type::Int(64), lhs, rhs } = inst else { return None };
    let def = |o: &Operand| defs.get(&o.local()?).copied();
    let zext32 = |o: &Operand| match def(o)? {
        Inst::Cast { cast: ra_asm_ir::Cast::ZExt, from: Type::Int(32), value, .. } => Some(value.clone()),
        _ => None,
    };
    let high_sign = |o: &Operand| -> Option<Operand> {
        let Inst::Bin { op: BinOp::Shl, lhs, rhs: Operand::Int(32), .. } = def(o)? else { return None };
        let sign = zext32(lhs)?;
        match def(&sign)? {
            Inst::Bin { op: BinOp::AShr, ty: Type::Int(32), lhs, rhs: Operand::Int(31) } => Some(lhs.clone()),
            _ => None,
        }
    };
    for (a, b) in [(lhs, rhs), (rhs, lhs)] {
        if let (Some(x), Some(low)) = (high_sign(a), zext32(b))
            && x == low
        {
            return Some(x);
        }
    }
    None
}

/// The value an instruction equals, when it equals one of its operands or a constant.
fn simplify(inst: &Inst) -> Option<Operand> {
    match inst {
        Inst::Bin { op, ty, lhs, rhs } => {
            let bits = match ty {
                Type::Int(n) => *n,
                _ => return None,
            };
            let mask = if bits >= 128 { -1i128 } else { (1i128 << bits) - 1 };
            let all_ones = |o: &Operand| matches!(o, Operand::Int(v) if (*v & mask) == mask);
            let zero = |o: &Operand| matches!(o, Operand::Int(0));
            match op {
                BinOp::And | BinOp::Or if lhs == rhs => return Some(lhs.clone()),
                BinOp::Xor | BinOp::Sub if lhs == rhs => return Some(Operand::Int(0)),
                BinOp::Or | BinOp::Xor | BinOp::Add | BinOp::Shl | BinOp::LShr | BinOp::AShr | BinOp::Sub if zero(rhs) => {
                    return Some(lhs.clone());
                }
                BinOp::Or | BinOp::Xor | BinOp::Add if zero(lhs) => return Some(rhs.clone()),
                BinOp::And if all_ones(rhs) => return Some(lhs.clone()),
                BinOp::And if all_ones(lhs) => return Some(rhs.clone()),
                BinOp::And if zero(lhs) || zero(rhs) => return Some(Operand::Int(0)),
                BinOp::Mul if matches!(rhs, Operand::Int(1)) => return Some(lhs.clone()),
                _ => {}
            }
            // Two constants: fold, keeping the result in the type's width.
            if let (Operand::Int(a), Operand::Int(b)) = (lhs, rhs) {
                let (a, b) = (*a & mask, *b & mask);
                let shift = 128 - u32::from(bits);
                let signed = |v: i128| (v << shift) >> shift;
                let folded = match op {
                    BinOp::Add => Some(a.wrapping_add(b)),
                    BinOp::Sub => Some(a.wrapping_sub(b)),
                    BinOp::Mul => Some(a.wrapping_mul(b)),
                    BinOp::And => Some(a & b),
                    BinOp::Or => Some(a | b),
                    BinOp::Xor => Some(a ^ b),
                    BinOp::Shl if b < i128::from(bits) => Some(a << b),
                    BinOp::LShr if b < i128::from(bits) => Some(a >> b),
                    BinOp::AShr if b < i128::from(bits) => Some(signed(a) >> b),
                    _ => None,
                };
                return folded.map(|v| Operand::Int(v & mask));
            }
            None
        }
        Inst::Cast { from, value, to, .. } if from == to => Some(value.clone()),
        Inst::Cast { cast, from: Type::Int(from), value: Operand::Int(v), to: Type::Int(to) } => {
            let from_mask = if *from >= 128 { -1i128 } else { (1i128 << from) - 1 };
            let to_mask = if *to >= 128 { -1i128 } else { (1i128 << to) - 1 };
            let v = *v & from_mask;
            let shift = 128 - u32::from(*from);
            match cast {
                ra_asm_ir::Cast::ZExt | ra_asm_ir::Cast::Trunc => Some(Operand::Int(v & to_mask)),
                ra_asm_ir::Cast::SExt => Some(Operand::Int(((v << shift) >> shift) & to_mask)),
                _ => None,
            }
        }
        Inst::Select { cond: Operand::Int(c), then, els, .. } => Some(if *c & 1 == 1 { then.clone() } else { els.clone() }),
        Inst::Select { then, els, .. } if then == els => Some(then.clone()),
        _ => None,
    }
}
