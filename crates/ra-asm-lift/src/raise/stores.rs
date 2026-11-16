//! The stores to the frame that nothing reads. After the ABI rewrite a call takes its arguments
//! by value, and the known values forward each pushed argument to the call: the push itself, a
//! store to the outgoing area of the frame, is left with no reader. A store to the frame goes when
//! every path from it overwrites the slot or leaves the function before anything may read it.
//!
//! What may read a slot of the frame, and so keeps a store:
//!
//! - a load whose address may overlap it: one at a known offset from the same base that overlaps,
//!   or one at a pointer into the frame whose offset is unknown;
//! - anything a frame address escapes to (a call's argument, a stored value, the result): it may
//!   read the slot when the slot lies at or above the escaped address, as an array or a structure
//!   passed by pointer is read upward. A variadic callee is handed the address of its arguments
//!   this way, so the arguments it reads are kept;
//! - a callee that reads the caller's stack by its own stack pointer: a function with no
//!   signature, which keeps the machine model's stack, or the runtime's exception machinery,
//!   which walks frames. A function that calls one keeps every store, as does one that calls
//!   `setjmp`, which comes back by an edge the graph does not show, or whose frame address escapes
//!   in a form the offsets do not follow.
//!
//! The addresses are offsets from the frame's `alloca` plus the values a call through a pointer
//! popped (`ra_pops_of`), which are not negative: two addresses with the same such terms compare
//! by their offsets, and one with more terms lies above the other's offset. A popped value is at
//! most what the runtime can hand back for it, the largest of its tables of a native callee's pops
//! and of what a universal entry point records: so an address whose terms the other lacks lies at
//! most that much above its offset, and a store after an indirect call still compares with the
//! slots and the escapes before it. A phi of stack pointers that differ by such terms (the arms of
//! a dispatch, a loop's rounds) is the lowest of them plus a term of its own that is not negative,
//! which for a loop holds by induction once what comes back round it is the phi plus what is not
//! negative; the term of a dispatch's phi is at most what its arms lie above the lowest. A walk
//! that comes back to where a term of the store's address is computed again keeps the store, since
//! one value of the term is not every value; so does an escape that a term computed in a loop may
//! have placed elsewhere.
//!
//! Two things are taken as C has them, as the frame's own locals are ([`super::frame`]): an
//! address is read upward from where it points, never below; and the arithmetic no compiler forms
//! an address by (an `xor`, a shift, the difference of two addresses) yields no address.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, Const, Function, Global, Inst, Operand, Terminator, Type, ValueId};

use super::abi::Signature;
use crate::state::{IMPORT_POPS, PROC_POPS};

/// The most instructions one store's walk reads before it gives up and keeps the store.
const BUDGET: usize = 20_000;

/// Where an address points: the object it is in (the frame, another `alloca`, a global, or none
/// for an integer the arithmetic does not follow), the opaque values added to it, and an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    root: Option<Root>,
    terms: Vec<ValueId>,
    offset: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Root {
    Alloca(ValueId),
    Global(String),
}

impl Place {
    fn opaque(v: ValueId) -> Self {
        Self { root: None, terms: vec![v], offset: 0 }
    }

    fn plus(mut self, k: i64) -> Option<Self> {
        self.offset = self.offset.checked_add(k)?;
        Some(self)
    }

    /// `self + other`, when at most one of them is in an object.
    fn add(self, other: Self) -> Option<Self> {
        let root = match (self.root, other.root) {
            (Some(_), Some(_)) => return None,
            (a, b) => a.or(b),
        };
        let mut terms = self.terms;
        terms.extend(other.terms);
        terms.sort_unstable();
        Some(Self { root, terms, offset: self.offset.checked_add(other.offset)? })
    }
}

/// The places of a function's values, computed on demand.
struct Places<'a> {
    defs: HashMap<ValueId, &'a Inst>,
    memo: HashMap<ValueId, Option<Place>>,
    /// The terms that are not negative: what a call through a pointer popped, and how far a
    /// phi lies above the lowest of the places it joins.
    nonneg: HashSet<ValueId>,
    /// The most a term that is not negative can be: what a call through a pointer popped, and how
    /// far a dispatch's phi lies above the lowest of its arms when each arm's distance is bounded.
    most: HashMap<ValueId, i64>,
    /// The phis of a loop taken to lie at or above the place that enters it, with that place.
    assumed: HashMap<ValueId, Place>,
    /// The phis whose assumption what came back round the loop contradicted: values of their own.
    opaque: HashSet<ValueId>,
}

impl<'a> Places<'a> {
    /// The places of `f`'s values, each computed after the values it reads in reverse post-order,
    /// so no chain of arithmetic recurses deeper than one step. A phi of a loop is taken to lie at
    /// or above the place entering it, and the assumption checked once the place coming back is
    /// known: it holds when that place is the phi's plus what is not negative, by induction over
    /// the rounds. A phi it does not hold for is a value of its own, and the places are built again.
    /// `most_pops` is the most a call through a pointer pops, when the runtime's tables bound it.
    fn new(f: &'a Function, most_pops: Option<i64>) -> Self {
        let defs = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| i.result.map(|v| (v, &i.inst))).collect();
        let pops: HashSet<ValueId> = f
            .blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .filter(|i| matches!(&i.inst, Inst::Call { callee: Callee::Direct(name), .. } if name == "ra_pops_of"))
            .filter_map(|i| i.result)
            .collect();
        let popped: HashMap<ValueId, i64> = most_pops.map(|most| pops.iter().map(|&v| (v, most)).collect()).unwrap_or_default();
        let mut places = Self {
            defs,
            memo: HashMap::new(),
            nonneg: pops.clone(),
            most: popped.clone(),
            assumed: HashMap::new(),
            opaque: HashSet::new(),
        };
        let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(n, b)| (b.id, n)).collect();
        let order = ra_asm_ir::dom::reverse_post_order(f);
        for round in 0.. {
            for id in &order {
                let Some(&bi) = index.get(id) else { continue };
                for i in &f.blocks[bi].instrs {
                    if let Some(v) = i.result {
                        places.local(v);
                    }
                }
            }
            let mut contradicted = Vec::new();
            for (&v, low) in &places.assumed {
                let Some(Inst::Phi { incoming, .. }) = places.defs.get(&v).copied() else { continue };
                let holds = incoming.iter().all(|(o, _)| match o {
                    Operand::Local(u) => places
                        .memo
                        .get(u)
                        .cloned()
                        .flatten()
                        .is_some_and(|p| p.root == low.root && p.offset >= low.offset && extra(&low.terms, &p.terms, &places.nonneg)),
                    _ => false,
                });
                if !holds {
                    contradicted.push(v);
                }
            }
            if contradicted.is_empty() {
                break;
            }
            // Past a few rounds every loop phi is a value of its own, which assumes nothing.
            if round >= 8 {
                contradicted.extend(places.assumed.keys().copied());
            }
            places.opaque.extend(contradicted);
            places.memo.clear();
            places.assumed.clear();
            places.nonneg.clone_from(&pops);
            places.most.clone_from(&popped);
        }
        places
    }

    /// What the terms are known to lie in, once the places are built.
    fn bounds(&self) -> Bounds {
        Bounds { nonneg: self.nonneg.clone(), most: self.most.clone() }
    }

    fn of(&mut self, o: &Operand) -> Option<Place> {
        match o {
            Operand::Int(k) => Some(Place { root: None, terms: Vec::new(), offset: i64::try_from(*k).ok()? }),
            Operand::Address { global, offset } => {
                Some(Place { root: Some(Root::Global(global.clone())), terms: Vec::new(), offset: *offset })
            }
            Operand::Global(global) => Some(Place { root: Some(Root::Global(global.clone())), terms: Vec::new(), offset: 0 }),
            Operand::Local(v) => self.local(*v),
            _ => None,
        }
    }

    fn local(&mut self, v: ValueId) -> Option<Place> {
        if let Some(p) = self.memo.get(&v) {
            return p.clone();
        }
        // A cycle through phis reads the value as opaque.
        self.memo.insert(v, Some(Place::opaque(v)));
        let place = self.compute(v);
        self.memo.insert(v, place.clone());
        place
    }

    fn compute(&mut self, v: ValueId) -> Option<Place> {
        let Some(inst) = self.defs.get(&v).copied() else { return Some(Place::opaque(v)) };
        let signed = |ty: &Type, k: i128| -> Option<i64> {
            let bits = ty.bits()?;
            let k = if bits < 128 { k & ((1i128 << bits) - 1) } else { k };
            let k = if bits < 128 && k >= 1i128 << (bits - 1) { k - (1i128 << bits) } else { k };
            i64::try_from(k).ok()
        };
        match inst {
            Inst::Alloca { .. } => Some(Place { root: Some(Root::Alloca(v)), terms: Vec::new(), offset: 0 }),
            Inst::Gep { base_ty: Type::I8, ptr, indices, .. } => match indices.as_slice() {
                [(_, Operand::Int(k))] => self.of(ptr)?.plus(i64::try_from(*k).ok()?),
                _ => Some(Place::opaque(v)),
            },
            // The 32-bit model narrows an address to a dword and widens it back; its frame and data
            // lie below 4 GiB in the port, so the low half is the whole address.
            Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt | Cast::Bitcast | Cast::ZExt, value, .. }
            | Inst::Cast { cast: Cast::Trunc, value, to: Type::Int(32), .. } => self.of(value),
            Inst::Bin { op: BinOp::And, ty: Type::Int(64), lhs, rhs: Operand::Int(0xffff_ffff) } => self.of(lhs),
            Inst::Bin { op: BinOp::Add, ty, lhs, rhs } => {
                let (l, r) = (self.term(lhs, ty)?, self.term(rhs, ty)?);
                l.add(r)
            }
            Inst::Bin { op: BinOp::Sub, ty, lhs, rhs: Operand::Int(k) } => self.term(lhs, ty)?.plus(signed(ty, *k)?.checked_neg()?),
            // A phi agrees with its incoming values when they are all one place already known; one
            // round a loop is not known yet, and the phi is a value of its own.
            Inst::Phi { incoming, .. } => {
                if self.opaque.contains(&v) {
                    return Some(Place::opaque(v));
                }
                let known = |o: &Operand, memo: &HashMap<ValueId, Option<Place>>| match o {
                    Operand::Local(u) => memo.get(u).cloned().flatten(),
                    _ => None,
                };
                let all: Vec<Option<Place>> = incoming.iter().map(|(o, _)| known(o, &self.memo)).collect();
                let ins: Vec<Place> = all.iter().flatten().cloned().collect();
                if ins.is_empty() || ins.iter().any(|p| p.terms.contains(&v)) {
                    return Some(Place::opaque(v));
                }
                let complete = ins.len() == all.len();
                if complete && ins.iter().all(|p| *p == ins[0]) {
                    return Some(ins[0].clone());
                }
                // Places of one object that differ by what the calls before popped (the arms of a
                // dispatch pop differently): the phi is the lowest of them plus a value of its own
                // that is not negative. Round a loop the value coming back is not known yet: the
                // phi is taken to lie at or above what enters the loop, which [`Places::new`]
                // checks of what comes back once it is known.
                if ins[0].root.is_none() {
                    return Some(Place::opaque(v));
                }
                let mut low = ins.iter().min_by_key(|p| p.terms.len()).unwrap().clone();
                low.offset = ins.iter().map(|p| p.offset).min().unwrap();
                let bounded = ins.iter().all(|p| p.root == low.root && extra(&low.terms, &p.terms, &self.nonneg));
                if !bounded {
                    return Some(Place::opaque(v));
                }
                self.nonneg.insert(v);
                if complete {
                    // How far the arms lie above the lowest, at most: their offsets past its and
                    // the most their extra terms are.
                    let distance = |p: &Place| {
                        let mut rest = p.terms.clone();
                        for t in &low.terms {
                            let at = rest.iter().position(|x| x == t)?;
                            rest.remove(at);
                        }
                        rest.iter().try_fold(p.offset.checked_sub(low.offset)?, |d, t| d.checked_add(*self.most.get(t)?))
                    };
                    let most = ins.iter().map(distance).collect::<Option<Vec<i64>>>().and_then(|d| d.into_iter().max());
                    if let Some(most) = most {
                        self.most.insert(v, most);
                    }
                } else {
                    self.assumed.insert(v, low.clone());
                }
                let mut terms = low.terms;
                terms.push(v);
                terms.sort_unstable();
                Some(Place { root: low.root, terms, offset: low.offset })
            }
            _ => Some(Place::opaque(v)),
        }
    }

    /// An operand of an addition of `ty`: a constant as the signed number it is.
    fn term(&mut self, o: &Operand, ty: &Type) -> Option<Place> {
        match o {
            Operand::Int(k) => {
                let bits = ty.bits()?;
                let k = if bits < 128 { k & ((1i128 << bits) - 1) } else { *k };
                let k = if bits < 128 && k >= 1i128 << (bits - 1) { k - (1i128 << bits) } else { k };
                Some(Place { root: None, terms: Vec::new(), offset: i64::try_from(k).ok()? })
            }
            o => self.of(o),
        }
    }
}

/// Whether the values each instruction derives from a frame address are frame addresses too: the
/// address arithmetic (an addition, a subtraction, the masks that align or narrow), the casts,
/// the phis and the selects pass the taint on. A load, a call and a comparison do not, nor does
/// arithmetic no compiler forms an address by: the `xor` of the stack cookie with the stack
/// pointer, the flags' parity and sign of a stack pointer's update.
fn passes_taint(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Bin { op: BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or, .. }
            | Inst::Cast { .. }
            | Inst::Gep { .. }
            | Inst::Phi { .. }
            | Inst::Select { .. }
            | Inst::Freeze { .. }
            | Inst::InsertElement { .. }
            | Inst::ExtractElement { .. }
            | Inst::ShuffleVector { .. }
    )
}

/// How `a`, at `a_size` bytes, and `b`, at `b_size`, of one object may relate: `Some(false)` when
/// they are apart, `Some(true)` when they overlap, `None` when the offsets cannot tell.
fn overlap(a: &Place, a_size: i64, b: &Place, b_size: i64, bounds: &Bounds) -> Option<bool> {
    if a.terms == b.terms {
        return Some(a.offset < b.offset + b_size && b.offset < a.offset + a_size);
    }
    // Apart when one lies past the other's end whatever values their terms take.
    if bounds.gap(b, a).is_some_and(|g| g >= a_size) || bounds.gap(a, b).is_some_and(|g| g >= b_size) {
        return Some(false);
    }
    None
}

/// What the terms of a function's places lie in: those that are not negative, and the most some
/// of them can be.
struct Bounds {
    nonneg: HashSet<ValueId>,
    most: HashMap<ValueId, i64>,
}

impl Bounds {
    /// The least `high - low` can be over the values of their terms, when their terms bound it: a
    /// term of `high` alone adds at least nothing when it is not negative, and one of `low` alone
    /// takes away at most what it can be.
    fn gap(&self, high: &Place, low: &Place) -> Option<i64> {
        let mut only_low = low.terms.clone();
        for t in &high.terms {
            if let Some(at) = only_low.iter().position(|x| x == t) {
                only_low.remove(at);
            } else if !self.nonneg.contains(t) {
                return None;
            }
        }
        only_low.iter().try_fold(high.offset.checked_sub(low.offset)?, |g, t| g.checked_sub(*self.most.get(t)?))
    }
}

/// The most a call through a pointer of the module pops, by what the runtime's `ra_pops_of` can
/// hand back: a native callee's pops from the binding table ([`IMPORT_POPS`]) or from the table of
/// the functions the program asks `GetProcAddress` for ([`PROC_POPS`]), else what a universal entry
/// point recorded, which is its function's signature's pops. `None` without the tables, which the
/// lift of a 32-bit program writes ahead of the raise.
pub fn most_pops(globals: &[Global], signatures: &HashMap<String, Signature>) -> Option<i64> {
    let init = |name: &str| globals.iter().find(|g| g.name == name).and_then(|g| g.init.as_ref());
    let Some(Const::Struct(imports)) = init(IMPORT_POPS) else { return None };
    let Some(Const::Bytes(procs)) = init(PROC_POPS) else { return None };
    let mut most: i64 = 0;
    // Three words an import: its symbol, its names, its pops.
    for import in imports.chunks(3) {
        let [_, _, Const::Int(pops)] = import else { return None };
        most = most.max(i64::try_from(*pops).ok()?);
    }
    // `name\0` and a little-endian u16 each, then an empty name.
    let mut at = 0;
    while *procs.get(at)? != 0 {
        let end = at + procs[at..].iter().position(|&b| b == 0)?;
        let pops = u16::from_le_bytes([*procs.get(end + 1)?, *procs.get(end + 2)?]);
        most = most.max(i64::from(pops));
        at = end + 3;
    }
    for s in signatures.values() {
        most = most.max(i64::try_from(s.pops).ok()?);
    }
    Some(most)
}

/// Whether `more` is `fewer` with terms added that are not negative.
fn extra(fewer: &[ValueId], more: &[ValueId], nonneg: &HashSet<ValueId>) -> bool {
    let mut rest = more.to_vec();
    for t in fewer {
        let Some(at) = rest.iter().position(|x| x == t) else { return false };
        rest.remove(at);
    }
    rest.iter().all(|t| nonneg.contains(t))
}

/// Removes the stores to the frame of `f` that nothing reads; returns whether it removed any.
/// `signatures` are the lifted functions': a call to one without is a machine call, which reads
/// the stack. `most_pops` is what [`most_pops`] found.
pub fn run(f: &mut Function, signatures: &HashMap<String, Signature>, most_pops: Option<i64>) -> bool {
    let dead = dead_stores(f, signatures, most_pops);
    if dead.is_empty() {
        return false;
    }
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        let mut k = 0;
        b.instrs.retain(|_| {
            let keep = !dead.contains(&(bi, k));
            k += 1;
            keep
        });
    }
    true
}

/// The stores of `f` nothing reads, by block index and instruction index.
fn dead_stores(f: &Function, signatures: &HashMap<String, Signature>, most_pops: Option<i64>) -> HashSet<(usize, usize)> {
    let mut dead = HashSet::new();
    // The frame: the `alloca` under `%rsp0`, as the ABI rewrite names the frame top.
    let mut places = Places::new(f, most_pops);
    let Some(rsp0) = f.blocks.first().and_then(|b| b.instrs.iter().find_map(|i| i.result.filter(|v| f.value_name(*v) == "rsp0"))) else {
        return dead;
    };
    let Some(Place { root: Some(frame @ Root::Alloca(_)), .. }) = places.local(rsp0) else { return dead };

    // The calls that read the stack by a stack pointer of their own, or come back by an edge the
    // graph lacks.
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        if let Inst::Call { callee: Callee::Direct(name), .. } = &i.inst {
            let machine = name.starts_with("F_") && !signatures.contains_key(name);
            let walks = (name.starts_with("ra_eh_") && name != "ra_eh_pending") || name.starts_with("ra_seh") || name == "ra_call";
            if machine || walks || name.contains("setjmp") {
                return dead;
            }
        }
    }

    // The frame addresses: every value derived from the frame by arithmetic.
    let mut tainted: HashSet<ValueId> = HashSet::new();
    loop {
        let before = tainted.len();
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            let Some(v) = i.result else { continue };
            if tainted.contains(&v) {
                continue;
            }
            let from_frame = matches!(i.inst, Inst::Alloca { .. }) && places.local(v).is_some_and(|p| p.root.as_ref() == Some(&frame));
            let derived = match &i.inst {
                // The difference of two frame addresses is a length (`strlen` over a buffer of
                // the frame), no address.
                Inst::Bin { op: BinOp::Sub, lhs, rhs, .. } => {
                    let t = |o: &Operand| o.local().is_some_and(|u| tainted.contains(&u));
                    t(lhs) && !t(rhs)
                }
                inst => passes_taint(inst) && inst.uses().iter().any(|u| tainted.contains(u)),
            };
            if from_frame || derived {
                tainted.insert(v);
            }
        }
        if tainted.len() == before {
            break;
        }
    }
    // Every value's place is known once the places are built: the bounds of the terms are settled.
    let bounds = places.bounds();

    // Where the frame escapes: a frame address any instruction takes as other than an address
    // to load or store at, or than arithmetic on it; an escape whose offset is unknown keeps
    // every store.
    let mut escapes: Vec<Place> = Vec::new();
    let mut escape = |o: &Operand, places: &mut Places<'_>| -> bool {
        let Operand::Local(v) = o else { return true };
        if !tainted.contains(v) {
            return true;
        }
        match places.local(*v) {
            Some(p) if p.root.as_ref() == Some(&frame) => {
                escapes.push(p);
                true
            }
            _ => false,
        }
    };
    for b in &f.blocks {
        for i in &b.instrs {
            let ok = match &i.inst {
                Inst::Load { .. } | Inst::ICmp { .. } | Inst::FCmp { .. } | Inst::Alloca { .. } | Inst::Bin { .. } => true,
                // An intrinsic of LLVM computes on its operands and reads no memory.
                Inst::Call { callee: Callee::Direct(name), .. } if name.starts_with("llvm.") && !name.starts_with("llvm.mem") => true,
                // The address of a variadic callee's arguments is the callee's during the call
                // alone, as C's arguments are: the walk reads it at the call.
                Inst::Call { callee, args, .. } => {
                    let callee_ok = match callee {
                        Callee::Indirect(p) => escape(p, &mut places),
                        Callee::Direct(_) => true,
                    };
                    callee_ok && args.iter().all(|(_, a)| is_varargs(f, a) || escape(a, &mut places))
                }
                Inst::Store { value, .. } | Inst::AtomicRmw { value, .. } => escape(value, &mut places),
                Inst::CmpXchg { expected, new, .. } => escape(expected, &mut places) && escape(new, &mut places),
                inst if passes_taint(inst) => true,
                inst => inst.operands().into_iter().all(|o| escape(o, &mut places)),
            };
            if !ok {
                return dead;
            }
        }
        let ok = b.terminator.operands().into_iter().all(|o| match &b.terminator {
            Terminator::Ret(_) => escape(o, &mut places),
            _ => !o.local().is_some_and(|v| tainted.contains(&v)) || escape(o, &mut places),
        });
        if !ok {
            return dead;
        }
    }

    // The blocks in a cycle: a term defined there takes another value each time round.
    let index: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(n, b)| (b.id, n)).collect();
    let cyclic = blocks_in_cycles(f, &index);
    let mut defined_in: HashMap<ValueId, usize> = HashMap::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.instrs {
            if let Some(v) = i.result {
                defined_in.insert(v, bi);
            }
        }
    }

    for (bi, b) in f.blocks.iter().enumerate() {
        for (k, i) in b.instrs.iter().enumerate() {
            let Inst::Store { ty, ptr, volatile: false, .. } = &i.inst else { continue };
            let Some(size) = ty.size(8).and_then(|s| i64::try_from(s).ok()) else { continue };
            let Some(place) = places.of(ptr).filter(|p| p.root.as_ref() == Some(&frame)) else { continue };
            // The blocks that compute a term of the address again: past one, the address is
            // another. A term a loop computes may have had another value where the frame escaped.
            let Some(redefined) = place.terms.iter().map(|t| defined_in.get(t).copied()).collect::<Option<HashSet<usize>>>() else {
                continue;
            };
            let varying: Vec<ValueId> =
                place.terms.iter().copied().filter(|t| defined_in.get(t).is_some_and(|d| cyclic.contains(d))).collect();
            let reachable =
                escapes.iter().any(|e| bounds.gap(e, &place).is_none_or(|g| g < size) || varying.iter().any(|t| e.terms.contains(t)));
            if !reachable && unread(f, &index, &mut places, &frame, &tainted, &bounds, (bi, k + 1), &place, size, &redefined).is_ok() {
                dead.insert((bi, k));
            }
        }
    }
    dead
}

/// Whether no path from `from` reads the `size` bytes at `place` before a store covers them or the
/// function returns. A path that enters one of the `redefined` blocks, which compute a term of the
/// address again, reads it as far as the walk can tell.
#[allow(clippy::too_many_arguments)]
fn unread(
    f: &Function,
    index: &HashMap<BlockId, usize>,
    places: &mut Places<'_>,
    frame: &Root,
    tainted: &HashSet<ValueId>,
    bounds: &Bounds,
    from: (usize, usize),
    place: &Place,
    size: i64,
    redefined: &HashSet<usize>,
) -> Result<(), &'static str> {
    let mut budget = BUDGET;
    let mut seen: HashSet<usize> = HashSet::new();
    let mut work = vec![from];
    while let Some((bi, start)) = work.pop() {
        let b = &f.blocks[bi];
        if start == 0 && redefined.contains(&bi) {
            return Err("its address changes round a loop");
        }
        let mut killed = false;
        for i in &b.instrs[start.min(b.instrs.len())..] {
            if budget == 0 {
                return Err("the walk ran out of budget");
            }
            budget -= 1;
            // A variadic callee reads its arguments upward from the address it is handed.
            if let Inst::Call { args, .. } = &i.inst {
                for (_, a) in args.iter().filter(|(_, a)| is_varargs(f, a)) {
                    match places.of(a) {
                        Some(q) if q.root.as_ref() == Some(frame) && bounds.gap(&q, place).is_some_and(|g| g >= size) => {}
                        _ => return Err("a variadic call reads it"),
                    }
                }
                continue;
            }
            let read = match &i.inst {
                Inst::Load { ty, ptr, .. } | Inst::AtomicRmw { ty, ptr, .. } | Inst::CmpXchg { ty, ptr, .. } => Some((ptr, ty.size(8))),
                Inst::Store { ty, ptr, .. } => {
                    let q = places.of(ptr);
                    if let (Some(q), Some(s)) = (q, ty.size(8).and_then(|s| i64::try_from(s).ok()))
                        && q.root.as_ref() == Some(frame)
                        && q.terms == place.terms
                        && q.offset <= place.offset
                        && place.offset + size <= q.offset + s
                    {
                        killed = true;
                        break;
                    }
                    None
                }
                _ => None,
            };
            let Some((ptr, read_size)) = read else { continue };
            let Some(read_size) = read_size.and_then(|s| i64::try_from(s).ok()) else { return Err("a read of no size") };
            let q = places.of(ptr);
            match q {
                Some(q) if q.root.as_ref() == Some(frame) => match overlap(place, size, &q, read_size, bounds) {
                    Some(false) => {}
                    Some(true) => return Err("a load reads it"),
                    None => return Err("a load from another base"),
                },
                // A frame address the offsets do not follow may be any slot.
                _ if ptr.local().is_some_and(|v| tainted.contains(&v)) => return Err("a load from an unknown frame address"),
                // Another object, or a pointer from elsewhere: the escapes are below the store.
                _ => {}
            }
        }
        if killed {
            continue;
        }
        for s in b.terminator.successors() {
            let Some(&si) = index.get(&s) else { return Err("a branch to no block") };
            if seen.insert(si) {
                work.push((si, 0));
            }
        }
    }
    Ok(())
}

/// Whether `o` is the address of a variadic callee's arguments, which the ABI rewrite names
/// `varargs`.
fn is_varargs(f: &Function, o: &Operand) -> bool {
    o.local().is_some_and(|v| f.value_name(v).starts_with("varargs"))
}

/// The blocks of `f` that lie on a cycle of the graph, by index.
fn blocks_in_cycles(f: &Function, index: &HashMap<BlockId, usize>) -> HashSet<usize> {
    let succ: Vec<Vec<usize>> =
        f.blocks.iter().map(|b| b.terminator.successors().iter().filter_map(|s| index.get(s).copied()).collect()).collect();
    // Tarjan's strongly connected components, iteratively.
    let n = succ.len();
    let mut idx = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter = 0;
    let mut out = HashSet::new();
    for root in 0..n {
        if idx[root] != usize::MAX {
            continue;
        }
        let mut call: Vec<(usize, usize)> = vec![(root, 0)];
        idx[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        on[root] = true;
        while let Some(&mut (v, ref mut next)) = call.last_mut() {
            if *next < succ[v].len() {
                let w = succ[v][*next];
                *next += 1;
                if idx[w] == usize::MAX {
                    idx[w] = counter;
                    low[w] = counter;
                    counter += 1;
                    stack.push(w);
                    on[w] = true;
                    call.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(idx[w]);
                }
                continue;
            }
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == idx[v] {
                let mut component = Vec::new();
                loop {
                    let w = stack.pop().unwrap();
                    on[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                if component.len() > 1 || succ[v].contains(&v) {
                    out.extend(component);
                }
            }
        }
    }
    out
}
