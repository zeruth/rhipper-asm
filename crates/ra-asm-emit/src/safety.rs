//! The proofs behind the safe Rust: which pointers address a data section or a local at a known
//! offset, which accesses through them need no raw pointer, and which locals never escape.
//!
//! A place is a root, a data section or an `alloca`, plus a byte offset: a constant, or for a
//! section one with terms the program computes. A value whose definition is a pure address
//! computation over a place (a `getelementptr`, a cast between pointer and integer, an add of a
//! constant or a term) is a place itself. A load or a store of a plain type through a place is a
//! bounds-checked access of the section or the local, which stops the port where the machine
//! would have read past it. An `alloca` every value of which is a place used only by such
//! accesses never escapes, and is a value or a byte array; one with any other use (its address
//! passed on, a dynamic index, a phi) keeps its raw pointer.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, Cast, Function, Inst, Operand, Type, ValueId};

use crate::common::field_offset;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Root {
    Local(ValueId),
    Section(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Place {
    pub root: Root,
    pub offset: i64,
    /// Terms the program computes, each with its scale, added to the offset.
    pub terms: Vec<(Type, Operand, i128)>,
    /// Where the first term entered, block and index. An access reads the terms where it stands,
    /// so nothing with a side effect may lie between the two.
    anchor: Option<(usize, usize)>,
}

impl Place {
    fn section(global: &str, offset: i64) -> Self {
        Self { root: Root::Section(global.to_owned()), offset, terms: Vec::new(), anchor: None }
    }

    /// The place `scale * index` further on; a term only into a section, and only within the
    /// block of the first term.
    fn advanced(mut self, ty: &Type, index: &Operand, scale: i128, at: (usize, usize)) -> Option<Self> {
        match index {
            Operand::Int(c) => {
                self.offset = self.offset.wrapping_add(i64::try_from(signed(*c, ty).checked_mul(scale)?).ok()?);
                Some(self)
            }
            Operand::Local(_) if matches!(self.root, Root::Section(_)) && self.anchor.is_none_or(|(b, _)| b == at.0) => {
                self.anchor.get_or_insert(at);
                self.terms.push((ty.clone(), index.clone(), scale));
                Some(self)
            }
            _ => None,
        }
    }
}

/// `c` as a signed value of `ty`'s width: a 32-bit program adds its negative offsets as dwords.
fn signed(c: i128, ty: &Type) -> i128 {
    match ty.bits() {
        Some(n) if (1..128).contains(&n) => {
            let shift = 128 - n;
            (c << shift) >> shift
        }
        _ => c,
    }
}

/// The shape of a local that never escapes: one plain value, or bytes read at several offsets or
/// as several types.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Local {
    Value(Type),
    Bytes(u64),
}

pub(crate) struct Safety {
    /// Each proven access by the address of its instruction, with its place.
    accesses: HashMap<usize, Place>,
    /// The locals that never escape, by their `alloca`.
    pub locals: HashMap<ValueId, Local>,
    /// The place values that are never written out: every use is a proven access or another
    /// such value, so the accesses name the root and the offset instead.
    pub folded: HashSet<ValueId>,
}

/// Whether every bit pattern is a value of `ty` in Rust, as the runtime's `Pod` has it.
fn plain(ty: &Type) -> bool {
    match ty {
        Type::Int(8 | 16 | 32 | 64 | 128) | Type::Half | Type::Float | Type::Double | Type::Ptr => true,
        Type::Vector(_, e) => plain(e) && !matches!(**e, Type::Ptr),
        _ => false,
    }
}

impl Safety {
    /// No proofs: every access raw.
    pub(crate) fn none() -> Self {
        Self { accesses: HashMap::new(), locals: HashMap::new(), folded: HashSet::new() }
    }

    /// The place `inst`, a load or a store, accesses, when proven.
    pub(crate) fn access(&self, inst: &Inst) -> Option<&Place> {
        self.accesses.get(&(std::ptr::from_ref(inst) as usize))
    }

    pub(crate) fn analyze(f: &Function, sections: &HashSet<String>, types: &[(String, Vec<Type>)]) -> Self {
        let section_place = |o: &Operand| match o {
            Operand::Address { global, offset } if sections.contains(global) => Some(Place::section(global, *offset)),
            Operand::Global(g) if sections.contains(g) => Some(Place::section(g, 0)),
            _ => None,
        };
        // The places, to a fixpoint: a definition may come before its operand's in block order.
        let mut places: HashMap<ValueId, Place> = HashMap::new();
        let mut sizes: HashMap<ValueId, u64> = HashMap::new();
        loop {
            let mut changed = false;
            for (bi, b) in f.blocks.iter().enumerate() {
                for (ii, i) in b.instrs.iter().enumerate() {
                    let Some(v) = i.result else { continue };
                    if places.contains_key(&v) {
                        continue;
                    }
                    let of = |o: &Operand| match o {
                        Operand::Local(u) => places.get(u).cloned(),
                        other => section_place(other),
                    };
                    let place = match &i.inst {
                        Inst::Alloca { ty, .. } => {
                            sizes.insert(v, ty.size(8).unwrap_or(0));
                            Some(Place { root: Root::Local(v), offset: 0, terms: Vec::new(), anchor: None })
                        }
                        Inst::Gep { base_ty, ptr, indices, .. } => of(ptr).and_then(|p| gep(p, base_ty, indices, types, (bi, ii))),
                        Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt, value, .. } | Inst::Freeze { value, .. } => of(value),
                        Inst::Bin { op: BinOp::Add, ty, lhs, rhs } => match (of(lhs), of(rhs)) {
                            (Some(p), None) => p.advanced(ty, rhs, 1, (bi, ii)),
                            (None, Some(p)) => p.advanced(ty, lhs, 1, (bi, ii)),
                            _ => None,
                        },
                        Inst::Bin { op: BinOp::Sub, ty, lhs, rhs: Operand::Int(c) } => {
                            of(lhs).and_then(|p| p.advanced(ty, &Operand::Int(-c), 1, (bi, ii)))
                        }
                        _ => None,
                    };
                    if let Some(p) = place {
                        places.insert(v, p);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // The accesses through places: of a plain type, in bounds of a local, and when a term
        // reads memory, past no side effect since it was computed.
        let positions: HashMap<ValueId, &Inst> =
            f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| i.result.map(|v| (v, &i.inst))).collect();
        let mut accesses: HashMap<usize, Place> = HashMap::new();
        let mut local_accesses: Vec<(ValueId, i64, Type)> = Vec::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.instrs.iter().enumerate() {
                let (Inst::Load { ty, ptr, .. } | Inst::Store { ty, ptr, .. }) = &i.inst else { continue };
                let place = match ptr {
                    Operand::Local(u) => places.get(u).cloned(),
                    other => section_place(other),
                };
                let Some(p) = place else { continue };
                if !plain(ty) {
                    continue;
                }
                if let Some((ab, ai)) = p.anchor
                    && !p.terms.iter().all(|(_, term, _)| pure(&positions, term, 0))
                    && (ab != bi || b.instrs[ai + 1..ii].iter().any(|n| n.inst.has_side_effects()))
                {
                    continue;
                }
                if let Root::Local(a) = &p.root {
                    let size = ty.size(8).unwrap_or(u64::MAX);
                    if p.offset < 0 || (p.offset as u64).saturating_add(size) > sizes.get(a).copied().unwrap_or(0) {
                        continue;
                    }
                    local_accesses.push((*a, p.offset, ty.clone()));
                }
                accesses.insert(std::ptr::from_ref(&i.inst) as usize, p);
            }
        }

        // A place value is folded when every use is a proven access through it or a folded place
        // value; anything else needs its name, to a fixpoint back along the chains.
        let mut folded: HashSet<ValueId> = places.keys().copied().collect();
        loop {
            let mut changed = false;
            for b in &f.blocks {
                for i in &b.instrs {
                    let proven = accesses.contains_key(&(std::ptr::from_ref(&i.inst) as usize));
                    let self_folded = i.result.is_some_and(|v| folded.contains(&v));
                    for o in i.inst.operands() {
                        let Operand::Local(u) = o else { continue };
                        if !folded.contains(u) {
                            continue;
                        }
                        let ok = match &i.inst {
                            Inst::Load { .. } => proven,
                            Inst::Store { ptr, value, .. } => proven && ptr == o && value != o,
                            _ => self_folded,
                        };
                        if !ok {
                            folded.remove(u);
                            changed = true;
                        }
                    }
                }
                for u in b.terminator.uses() {
                    changed |= folded.remove(&u);
                }
            }
            if !changed {
                break;
            }
        }

        // A local escapes when any value rooted at it is not folded; then all of it stays raw.
        let escaped: HashSet<ValueId> = places
            .iter()
            .filter_map(|(v, p)| match &p.root {
                Root::Local(a) if !folded.contains(v) => Some(*a),
                _ => None,
            })
            .collect();
        folded.retain(|v| !matches!(&places[v].root, Root::Local(a) if escaped.contains(a)));
        accesses.retain(|_, p| !matches!(&p.root, Root::Local(a) if escaped.contains(a)));
        let mut locals: HashMap<ValueId, Local> = HashMap::new();
        for (a, size) in &sizes {
            if escaped.contains(a) {
                continue;
            }
            let mut tys = local_accesses.iter().filter(|(root, _, _)| root == a).map(|(_, offset, ty)| (*offset, ty));
            let form = match tys.next() {
                Some((0, first)) if tys.all(|(offset, ty)| offset == 0 && ty == first) => Local::Value(first.clone()),
                _ => Local::Bytes(*size),
            };
            locals.insert(*a, form);
        }
        Self { accesses, locals, folded }
    }
}

/// Whether `term` computes the same wherever it is written: a constant, a parameter, a phi (which
/// has its name), or arithmetic over such; not a load, whose value a store in between may change.
fn pure(positions: &HashMap<ValueId, &Inst>, term: &Operand, depth: usize) -> bool {
    let Operand::Local(v) = term else { return true };
    let Some(inst) = positions.get(v) else { return true };
    match inst {
        Inst::Phi { .. } => true,
        Inst::Load { .. } | Inst::Call { .. } | Inst::Alloca { .. } => false,
        _ => depth < 16 && inst.operands().iter().all(|o| pure(positions, o, depth + 1)),
    }
}

/// The place a `getelementptr` of `base_ty` from `p` addresses.
fn gep(mut p: Place, base_ty: &Type, indices: &[(Type, Operand)], types: &[(String, Vec<Type>)], at: (usize, usize)) -> Option<Place> {
    if let Some(offset) = field_offset(types, base_ty, indices) {
        p.offset = p.offset.wrapping_add(i64::try_from(offset).ok()?);
        return Some(p);
    }
    let size = i128::from(base_ty.size(8)?);
    for (k, (ty, index)) in indices.iter().enumerate() {
        let scale = if k == 0 {
            size
        } else {
            match base_ty {
                Type::Array(_, e) => i128::from(e.size(8)?),
                _ => return None,
            }
        };
        p = p.advanced(ty, index, scale, at)?;
    }
    Some(p)
}
