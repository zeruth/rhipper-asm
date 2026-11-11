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
    for b in &f.blocks {
        for i in &b.instrs {
            let Some(result) = i.result else { continue };
            if let Some(replacement) = simplify(&i.inst) {
                map.insert(result, replacement);
                continue;
            }
            // trunc (zext x: A to B) to A  ==  x, and the same through a sext.
            if let Inst::Cast { cast: ra_asm_ir::Cast::Trunc, value: Operand::Local(v), to, .. } = &i.inst
                && let Some((ra_asm_ir::Cast::ZExt | ra_asm_ir::Cast::SExt, inner_from, inner)) = casts.get(v)
                && inner_from == to
            {
                map.insert(result, inner.clone());
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
