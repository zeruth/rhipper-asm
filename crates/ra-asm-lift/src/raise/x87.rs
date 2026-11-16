//! The x87 stack in locals. The lift keeps the unit's stack in globals, eight doubles and a top
//! (see [`crate::x87`]), because a float crosses calls on it. Between two calls compiled code moves
//! the top by amounts its instructions fix, so each `st(i)` it touches is a known slot counted
//! from the top the stretch started at. Over such a stretch, a region, the slots become double
//! locals and the top a known offset: the region loads the slots it may read before it writes them
//! where it opens (the load of the top after a call, or the store of a constant top), and stores
//! the slots it wrote and the top where it closes (a call, a return, a join with code of another
//! region). At every call and return the globals hold what the lift's own code leaves there.
//!
//! The pass is exact or it changes nothing: a function where an address into the stack goes
//! anywhere but to a load or a store of a double, or where a top the analysis cannot count is
//! stored, stays as the lift wrote it.

use std::collections::HashMap;

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, Function, Inst, Instruction, Operand, Terminator, Type, ValueId};

use crate::state::{X87_STACK, X87_TOP};

/// The sweeps the analysis makes before it gives a function up: a region's facts settle in a few.
const ROUNDS: usize = 64;

/// What a region's top counts from: the load of the global top that opened it, or the store of a
/// constant top (`fninit`) at its block and instruction, which counts from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Base {
    Load(ValueId),
    Set(usize, usize),
}

impl Base {
    /// The value the region's counts start from.
    fn top(self) -> Operand {
        match self {
            Self::Load(v) => Operand::Local(v),
            Self::Set(..) => Operand::Int(0),
        }
    }
}

/// An integer counted from a region's top: congruent to `scale * (top + k)` modulo `8 * scale`,
/// and exactly `scale * ((top + k) & 7)` when `masked`. The lift's top, its slot indices and their
/// byte offsets are these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Count {
    base: Base,
    k: u8,
    scale: u8,
    masked: bool,
}

impl Count {
    /// The count plus `n`.
    fn plus(self, n: i128) -> Abs {
        let step = i128::from(self.scale);
        if n % step != 0 {
            return Abs::Other;
        }
        Abs::Count(Self { k: (i128::from(self.k) + n / step).rem_euclid(8) as u8, masked: false, ..self })
    }
}

/// What the analysis knows of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Abs {
    Other,
    Int(i128),
    Count(Count),
    /// The address of the global top, as an integer or a pointer.
    TopRef,
    /// The address of the stack global plus a constant.
    Stack(i64),
    /// The address of slot `k` of a region: `ra_x87_stack + 8 * ((top + k) & 7)`.
    Slot(Base, u8),
    /// Any other address into the unit's stack or top.
    Wild,
}

impl Abs {
    /// Whether the value is an address into the stack or the top, which only a load or a store
    /// the pass rewrites may take.
    fn place(self) -> bool {
        matches!(self, Self::TopRef | Self::Stack(_) | Self::Slot(..) | Self::Wild)
    }
}

/// The unit between two closes: where its counts start, the top as a count, whether the top was
/// stored, the slots written since the last close (`dirty`) and the slots whose local holds the
/// value on every path (`valid`), one bit per slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    base: Base,
    k: u8,
    top_dirty: bool,
    dirty: u8,
    valid: u8,
}

/// The unit at a point: in a region, or held by the globals alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Unknown,
    Known(Region),
}

/// The state where paths meet: one region when they agree on it, else the globals.
fn join(a: State, b: State) -> State {
    match (a, b) {
        (State::Known(x), State::Known(y)) if x.base == y.base && x.k == y.k => {
            State::Known(Region { top_dirty: x.top_dirty || y.top_dirty, dirty: x.dirty | y.dirty, valid: x.valid & y.valid, ..x })
        }
        _ => State::Unknown,
    }
}

/// What a sweep finds: the state at each block's entry and exit, by index, the slots each region
/// loads where it opens, and what each value is. The values carry from one sweep to the next, so a
/// phi sees what a back edge brings it.
#[derive(Debug, Clone, PartialEq)]
struct Facts {
    entry: Vec<Option<State>>,
    exit: Vec<Option<State>>,
    preload: HashMap<Base, u8>,
    abs: Vec<Abs>,
}

/// The blocks by index: the reverse post order from the entry, and each block's edges.
struct Cfg {
    order: Vec<usize>,
    preds: Vec<Vec<usize>>,
    succs: Vec<Vec<usize>>,
}

impl Cfg {
    fn new(blocks: &[Block]) -> Self {
        let index: HashMap<BlockId, usize> = blocks.iter().enumerate().map(|(n, b)| (b.id, n)).collect();
        let succs: Vec<Vec<usize>> =
            blocks.iter().map(|b| b.terminator.successors().iter().filter_map(|s| index.get(s).copied()).collect()).collect();
        let mut preds = vec![Vec::new(); blocks.len()];
        for (n, out) in succs.iter().enumerate() {
            for &s in out {
                preds[s].push(n);
            }
        }
        let mut order = Vec::new();
        if !blocks.is_empty() {
            let mut seen = vec![false; blocks.len()];
            seen[0] = true;
            let mut stack = vec![(0usize, 0usize)];
            while let Some(top) = stack.last_mut() {
                let (b, at) = *top;
                if let Some(&s) = succs[b].get(at) {
                    top.1 += 1;
                    if !seen[s] {
                        seen[s] = true;
                        stack.push((s, 0));
                    }
                } else {
                    order.push(b);
                    stack.pop();
                }
            }
            order.reverse();
        }
        Self { order, preds, succs }
    }
}

/// The bits of `mask`, as slots.
fn bits(mask: u8) -> impl Iterator<Item = u8> {
    (0..8u8).filter(move |j| mask & (1 << j) != 0)
}

/// What the analysis knows of an operand.
fn operand(abs: &[Abs], o: &Operand) -> Abs {
    match o {
        Operand::Local(v) => abs.get(v.0 as usize).copied().unwrap_or(Abs::Other),
        Operand::Int(n) => Abs::Int(*n),
        Operand::Address { global, offset } if global == X87_TOP => {
            if *offset == 0 {
                Abs::TopRef
            } else {
                Abs::Wild
            }
        }
        Operand::Address { global, offset } if global == X87_STACK => Abs::Stack(*offset),
        _ => Abs::Other,
    }
}

/// A binary operation of two constants, at 64 bits.
fn constant(op: BinOp, x: i128, y: i128) -> Option<i128> {
    let (x, y) = (x as i64, y as i64);
    let r = match op {
        BinOp::Add => x.wrapping_add(y),
        BinOp::Sub => x.wrapping_sub(y),
        BinOp::Mul => x.wrapping_mul(y),
        BinOp::And => x & y,
        BinOp::Or => x | y,
        BinOp::Xor => x ^ y,
        BinOp::Shl if (0..64).contains(&y) => x << y,
        _ => return None,
    };
    Some(i128::from(r))
}

/// What a binary operation of `a` and `b` gives; `None` where it takes an address of the unit
/// somewhere the pass cannot follow.
fn bin(op: BinOp, ty: &Type, a: Abs, b: Abs) -> Option<Abs> {
    if *ty != Type::I64 {
        return (!a.place() && !b.place()).then_some(Abs::Other);
    }
    let r = match (op, a, b) {
        (_, Abs::Int(x), Abs::Int(y)) => constant(op, x, y).map_or(Abs::Other, Abs::Int),
        (BinOp::Add, Abs::Count(c), Abs::Int(n)) | (BinOp::Add, Abs::Int(n), Abs::Count(c)) => c.plus(n),
        (BinOp::Sub, Abs::Count(c), Abs::Int(n)) => c.plus(n.wrapping_neg()),
        (BinOp::And, Abs::Count(c), Abs::Int(m)) | (BinOp::And, Abs::Int(m), Abs::Count(c)) if m == 8 * i128::from(c.scale) - 1 => {
            Abs::Count(Count { masked: true, ..c })
        }
        (BinOp::Shl, Abs::Count(c), Abs::Int(3)) | (BinOp::Mul, Abs::Count(c), Abs::Int(8)) | (BinOp::Mul, Abs::Int(8), Abs::Count(c))
            if c.scale == 1 =>
        {
            Abs::Count(Count { scale: 8, ..c })
        }
        (BinOp::Add, Abs::Stack(0), Abs::Count(c)) | (BinOp::Add, Abs::Count(c), Abs::Stack(0)) if c.scale == 8 && c.masked => {
            Abs::Slot(c.base, c.k)
        }
        _ if a.place() || b.place() => return None,
        _ => Abs::Other,
    };
    Some(r)
}

/// The rewrite a final sweep writes: each block's new instructions, and the locals of the slots.
struct Out<'f> {
    f: &'f mut Function,
    locals: [Option<ValueId>; 8],
    blocks: Vec<Option<Vec<Instruction>>>,
    instrs: Vec<Instruction>,
    address: Option<u64>,
}

impl Out<'_> {
    fn push(&mut self, result: Option<ValueId>, inst: Inst) {
        self.instrs.push(Instruction { result, inst, address: self.address });
    }

    fn value(&mut self, hint: &str, inst: Inst) -> ValueId {
        let r = self.f.new_value(hint, inst.result_type());
        self.push(Some(r), inst);
        r
    }

    fn bin(&mut self, hint: &str, op: BinOp, lhs: Operand, rhs: i64) -> ValueId {
        self.value(hint, Inst::Bin { op, ty: Type::I64, lhs, rhs: Operand::from(rhs) })
    }

    /// The local of slot `j`.
    fn local(&mut self, j: u8) -> ValueId {
        if let Some(v) = self.locals[usize::from(j)] {
            return v;
        }
        let v = self.f.new_value("x87", Type::Ptr);
        self.locals[usize::from(j)] = Some(v);
        v
    }

    /// `(top + k) & 7` of a region, the instruction that computes it last.
    fn top(&mut self, base: Base, k: u8) -> Inst {
        match base {
            Base::Load(_) if k == 0 => Inst::Bin { op: BinOp::And, ty: Type::I64, lhs: base.top(), rhs: Operand::Int(7) },
            Base::Load(_) => {
                let index = self.bin("top.k", BinOp::Add, base.top(), i64::from(k));
                Inst::Bin { op: BinOp::And, ty: Type::I64, lhs: Operand::Local(index), rhs: Operand::Int(7) }
            }
            Base::Set(..) => Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Int(i128::from(k)), rhs: Operand::Int(0) },
        }
    }

    fn pointer(&mut self, address: Operand) -> Operand {
        Operand::Local(self.value("st.p", Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: address, to: Type::Ptr }))
    }

    /// The global slot `j` of a region: entry `(top + j) & 7` of the stack.
    fn slot(&mut self, base: Base, j: u8) -> Operand {
        let address = match base {
            Base::Load(_) => {
                let index = if j == 0 { base.top() } else { Operand::Local(self.bin("st.i", BinOp::Add, base.top(), i64::from(j))) };
                let wrapped = self.bin("st.w", BinOp::And, index, 7);
                let offset = self.bin("st.off", BinOp::Shl, Operand::Local(wrapped), 3);
                let stack = Operand::Address { global: X87_STACK.to_owned(), offset: 0 };
                Operand::Local(self.value("st.addr", Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Local(offset), rhs: stack }))
            }
            Base::Set(..) => Operand::Address { global: X87_STACK.to_owned(), offset: 8 * i64::from(j) },
        };
        self.pointer(address)
    }

    fn load(&mut self, result: Option<ValueId>, ptr: Operand) -> ValueId {
        let inst = Inst::Load { ty: Type::Double, ptr, align: 8, volatile: false };
        match result {
            Some(r) => {
                self.push(Some(r), inst);
                r
            }
            None => self.value("st", inst),
        }
    }

    fn store(&mut self, ty: Type, value: Operand, ptr: Operand) {
        self.push(None, Inst::Store { ty, value, ptr, align: 8, volatile: false });
    }
}

/// One pass over the reachable blocks under the facts of the sweep before: the facts it finds, and
/// with `out` the rewrite.
struct Sweep<'a, 'f> {
    cfg: &'a Cfg,
    before: &'a Facts,
    next: Facts,
    bail: bool,
    out: Option<Out<'f>>,
}

impl Sweep<'_, '_> {
    fn run(&mut self, blocks: &[Block]) {
        let cfg = self.cfg;
        for &n in &cfg.order {
            let block = &blocks[n];
            let mut state = if n == 0 {
                State::Unknown
            } else {
                cfg.preds[n].iter().filter_map(|&p| self.next.exit[p]).reduce(join).unwrap_or(State::Unknown)
            };
            self.next.entry[n] = Some(state);
            for (at, i) in block.instrs.iter().enumerate() {
                if let Some(out) = &mut self.out {
                    out.address = i.address;
                }
                self.instruction(&mut state, (n, at), i);
            }
            if block.terminator.operands().into_iter().any(|o| operand(&self.next.abs, o).place()) {
                self.bail = true;
            }
            // A block closes its region where it returns or where code of another region follows.
            let closes = match block.terminator {
                Terminator::Ret(_) => true,
                Terminator::Unreachable => false,
                _ => cfg.succs[n].iter().any(|&s| self.before.entry[s] == Some(State::Unknown)),
            };
            if closes {
                self.close(&mut state);
            }
            self.next.exit[n] = Some(state);
            if let Some(out) = &mut self.out {
                out.blocks[n] = Some(std::mem::take(&mut out.instrs));
            }
        }
    }

    fn keep(&mut self, i: &Instruction) {
        if let Some(out) = &mut self.out {
            out.instrs.push(i.clone());
        }
    }

    fn set(&mut self, result: Option<ValueId>, abs: Abs) {
        if let Some(v) = result
            && let Some(slot) = self.next.abs.get_mut(v.0 as usize)
        {
            *slot = abs;
        }
    }

    /// A region of `base` opens: the slots it reads before it writes them come into their locals.
    fn open(&mut self, base: Base, k: u8, top_dirty: bool) -> State {
        let valid = self.next.preload.get(&base).copied().unwrap_or(0);
        if let Some(out) = &mut self.out {
            for j in bits(valid) {
                let p = out.slot(base, j);
                let v = out.load(None, p);
                let local = out.local(j);
                out.store(Type::Double, Operand::Local(v), Operand::Local(local));
            }
        }
        State::Known(Region { base, k, top_dirty, dirty: 0, valid })
    }

    /// The region's slots and top go back to the globals; the region goes on.
    fn close(&mut self, state: &mut State) {
        let State::Known(r) = state else { return };
        // A slot written on one path and not loaded on another must be loaded where the region
        // opens, or its store would write what the local held before.
        *self.next.preload.entry(r.base).or_default() |= r.dirty & !r.valid;
        if let Some(out) = &mut self.out {
            for j in bits(r.dirty) {
                let local = out.local(j);
                let v = out.load(None, Operand::Local(local));
                let p = out.slot(r.base, j);
                out.store(Type::Double, Operand::Local(v), p);
            }
            if r.top_dirty {
                let inst = out.top(r.base, r.k);
                let top = out.value("top", inst);
                let p = out.pointer(Operand::Address { global: X87_TOP.to_owned(), offset: 0 });
                out.store(Type::I64, Operand::Local(top), p);
            }
        }
        r.dirty = 0;
        r.top_dirty = false;
    }

    /// The region closes and the globals alone hold the unit after it.
    fn leave(&mut self, state: &mut State) {
        self.close(state);
        *state = State::Unknown;
    }

    fn instruction(&mut self, state: &mut State, at: (usize, usize), i: &Instruction) {
        match &i.inst {
            Inst::Bin { op, ty, lhs, rhs } => {
                let r = bin(*op, ty, operand(&self.next.abs, lhs), operand(&self.next.abs, rhs)).unwrap_or_else(|| {
                    self.bail = true;
                    Abs::Other
                });
                self.set(i.result, r);
                self.keep(i);
            }
            Inst::Cast { cast, value, .. } => {
                let a = operand(&self.next.abs, value);
                let r = match cast {
                    Cast::IntToPtr | Cast::PtrToInt | Cast::Bitcast if a.place() => a,
                    _ if a.place() => {
                        self.bail = true;
                        Abs::Other
                    }
                    _ => Abs::Other,
                };
                self.set(i.result, r);
                self.keep(i);
            }
            Inst::Load { ty, ptr, volatile, .. } => match operand(&self.next.abs, ptr) {
                Abs::TopRef if *ty == Type::I64 => self.load_top(state, i),
                Abs::Slot(base, k) if *ty == Type::Double && !volatile => match state {
                    State::Known(r) if r.base == base => {
                        let bit = 1u8 << k;
                        if r.valid & bit == 0 {
                            *self.next.preload.entry(base).or_default() |= bit;
                        }
                        if let Some(out) = &mut self.out {
                            let local = out.local(k);
                            out.load(i.result, Operand::Local(local));
                        }
                    }
                    _ => {
                        self.bail = true;
                        self.keep(i);
                    }
                },
                a => {
                    self.bail |= a.place();
                    self.keep(i);
                }
            },
            Inst::Store { ty, value, ptr, volatile, .. } => {
                let v = operand(&self.next.abs, value);
                self.bail |= v.place();
                match operand(&self.next.abs, ptr) {
                    Abs::TopRef if *ty == Type::I64 => self.store_top(state, at, i, v),
                    Abs::Slot(base, k) if *ty == Type::Double && !volatile => match state {
                        State::Known(r) if r.base == base => {
                            let bit = 1u8 << k;
                            r.valid |= bit;
                            r.dirty |= bit;
                            if let Some(out) = &mut self.out {
                                let local = out.local(k);
                                out.store(Type::Double, value.clone(), Operand::Local(local));
                            }
                        }
                        _ => {
                            self.bail = true;
                            self.keep(i);
                        }
                    },
                    a => {
                        self.bail |= a.place();
                        self.keep(i);
                    }
                }
            }
            Inst::Call { callee, .. } => {
                self.bail |= i.inst.operands().into_iter().any(|o| operand(&self.next.abs, o).place());
                // An intrinsic leaves memory alone; any other callee may read or move the unit.
                if !matches!(callee, Callee::Direct(name) if name.starts_with("llvm.")) {
                    self.leave(state);
                }
                self.keep(i);
            }
            inst => {
                self.bail |= inst.operands().into_iter().any(|o| operand(&self.next.abs, o).place());
                self.keep(i);
            }
        }
    }

    /// A load of the global top: inside a region its count, which the rewrite computes; outside one
    /// the top a new region counts from.
    fn load_top(&mut self, state: &mut State, i: &Instruction) {
        let Some(v) = i.result else {
            self.keep(i);
            return;
        };
        match *state {
            State::Known(r) => {
                self.set(Some(v), Abs::Count(Count { base: r.base, k: r.k, scale: 1, masked: true }));
                if let Some(out) = &mut self.out {
                    let inst = out.top(r.base, r.k);
                    out.push(Some(v), inst);
                }
            }
            State::Unknown => {
                let base = Base::Load(v);
                self.set(Some(v), Abs::Count(Count { base, k: 0, scale: 1, masked: false }));
                self.keep(i);
                *state = self.open(base, 0, false);
            }
        }
    }

    /// A store of the global top: a count of the region moves its top, a constant opens a region
    /// of its own.
    fn store_top(&mut self, state: &mut State, at: (usize, usize), i: &Instruction, value: Abs) {
        match value {
            Abs::Count(c) => {
                if let State::Known(r) = state
                    && c.base == r.base
                    && c.scale == 1
                    && c.masked
                {
                    r.k = c.k;
                    r.top_dirty = true;
                    return;
                }
            }
            Abs::Int(n) if (0..8).contains(&n) => {
                self.leave(state);
                *state = self.open(Base::Set(at.0, at.1), n as u8, true);
                return;
            }
            _ => {}
        }
        self.bail = true;
        self.keep(i);
    }
}

/// Moves the x87 stack of `f` into double locals where the analysis counts every slot it touches
/// (see the module's documentation); whether it changed `f`.
pub fn run(f: &mut Function) -> bool {
    let top = Operand::Address { global: X87_TOP.to_owned(), offset: 0 };
    if !f.blocks.iter().flat_map(|b| &b.instrs).any(|i| i.inst.operands().contains(&&top)) {
        return false;
    }
    let cfg = Cfg::new(&f.blocks);
    let n = f.blocks.len();
    let mut facts =
        Facts { entry: vec![None; n], exit: vec![None; n], preload: HashMap::new(), abs: vec![Abs::Other; f.value_types.len()] };
    let mut settled = false;
    for _ in 0..ROUNDS {
        let mut sweep = Sweep { cfg: &cfg, before: &facts, next: facts.clone(), bail: false, out: None };
        sweep.run(&f.blocks);
        let (next, bail) = (sweep.next, sweep.bail);
        if next == facts {
            settled = !bail;
            break;
        }
        facts = next;
    }
    if !settled {
        return false;
    }
    let mut blocks = std::mem::take(&mut f.blocks);
    let (rewritten, locals) = {
        let out = Out { f: &mut *f, locals: [None; 8], blocks: vec![None; n], instrs: Vec::new(), address: None };
        let mut sweep = Sweep { cfg: &cfg, before: &facts, next: facts.clone(), bail: false, out: Some(out) };
        sweep.run(&blocks);
        let out = sweep.out.expect("the rewrite has its output");
        (out.blocks, out.locals)
    };
    for (block, instrs) in blocks.iter_mut().zip(rewritten) {
        if let Some(instrs) = instrs {
            block.instrs = instrs;
        }
    }
    let mut entry: Vec<Instruction> = locals
        .iter()
        .flatten()
        .map(|&v| Instruction { result: Some(v), inst: Inst::Alloca { ty: Type::Double, align: 8 }, address: None })
        .collect();
    entry.append(&mut blocks[0].instrs);
    blocks[0].instrs = entry;
    f.blocks = blocks;
    true
}

#[cfg(test)]
mod tests {
    use ra_asm_ir::{FnType, FunctionBuilder};

    use super::*;

    fn top_address() -> Operand {
        Operand::Address { global: X87_TOP.to_owned(), offset: 0 }
    }

    /// The lift's shapes of the double unit, as `crate::x87` writes them.
    fn top(b: &mut FunctionBuilder, at: BlockId) -> ValueId {
        let p = b.cast(at, Cast::IntToPtr, Type::I64, top_address(), Type::Ptr, "top.p");
        b.load(at, Type::I64, p, "top")
    }

    fn set_top(b: &mut FunctionBuilder, at: BlockId, v: ValueId) {
        let p = b.cast(at, Cast::IntToPtr, Type::I64, top_address(), Type::Ptr, "top.p");
        b.store(at, Type::I64, v, p);
    }

    fn st_ptr(b: &mut FunctionBuilder, at: BlockId, i: i64) -> ValueId {
        let t = top(b, at);
        let index = b.bin(at, BinOp::Add, Type::I64, t, i, "st.i");
        let wrapped = b.bin(at, BinOp::And, Type::I64, index, 7i64, "st.w");
        let offset = b.bin(at, BinOp::Shl, Type::I64, wrapped, 3i64, "st.off");
        let stack = Operand::Address { global: X87_STACK.to_owned(), offset: 0 };
        let address = b.bin(at, BinOp::Add, Type::I64, offset, stack, "st.addr");
        b.cast(at, Cast::IntToPtr, Type::I64, address, Type::Ptr, "st.p")
    }

    fn st(b: &mut FunctionBuilder, at: BlockId, i: i64) -> ValueId {
        let p = st_ptr(b, at, i);
        b.load(at, Type::Double, p, "st")
    }

    fn set_st(b: &mut FunctionBuilder, at: BlockId, i: i64, v: ValueId) {
        let p = st_ptr(b, at, i);
        b.store(at, Type::Double, v, p);
    }

    fn moved(b: &mut FunctionBuilder, at: BlockId, by: i64) {
        let t = top(b, at);
        let moved = b.bin(at, BinOp::Add, Type::I64, t, by, "top.m");
        let wrapped = b.bin(at, BinOp::And, Type::I64, moved, 7i64, "top");
        set_top(b, at, wrapped);
    }

    fn push(b: &mut FunctionBuilder, at: BlockId, v: f64) {
        moved(b, at, -1);
        let p = st_ptr(b, at, 0);
        b.store(at, Type::Double, Operand::Float(v), p);
    }

    fn call(b: &mut FunctionBuilder, at: BlockId) {
        b.call(at, FnType::new(Type::Void, vec![]), Callee::Direct("F_00002000".into()), vec![], "");
    }

    /// Whether `o` is computed from the address of `global`.
    fn reaches(f: &Function, o: &Operand, global: &str) -> bool {
        let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, &i.inst))).collect();
        let mut work = vec![o.clone()];
        while let Some(o) = work.pop() {
            match o {
                Operand::Address { global: g, .. } if g == global => return true,
                Operand::Local(v) => {
                    if let Some(inst) = defs.get(&v)
                        && !matches!(inst, Inst::Load { .. } | Inst::Alloca { .. })
                    {
                        work.extend(inst.operands().into_iter().cloned());
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// The accesses of the global stack, in order: `true` a store.
    fn stack_accesses(f: &Function) -> Vec<bool> {
        let mut out = Vec::new();
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            match &i.inst {
                Inst::Load { ptr, .. } if reaches(f, ptr, X87_STACK) => out.push(false),
                Inst::Store { ptr, .. } if reaches(f, ptr, X87_STACK) => out.push(true),
                _ => {}
            }
        }
        out
    }

    fn function() -> (FunctionBuilder, BlockId) {
        let mut b = FunctionBuilder::new("F_00001000", FnType::new(Type::Double, vec![]), &[]);
        let entry = b.block("entry");
        (b, entry)
    }

    #[test]
    fn a_stretch_without_calls_keeps_its_slots_in_locals() {
        let (mut b, entry) = function();
        push(&mut b, entry, 1.5);
        push(&mut b, entry, 2.5);
        let a = st(&mut b, entry, 0);
        let c = st(&mut b, entry, 1);
        let sum = b.bin(entry, BinOp::FAdd, Type::Double, a, c, "sum");
        set_st(&mut b, entry, 1, sum);
        moved(&mut b, entry, 1);
        let r = st(&mut b, entry, 0);
        moved(&mut b, entry, 1);
        b.terminate(entry, Terminator::Ret(Some((Type::Double, Operand::Local(r)))));
        let mut f = b.finish();
        assert!(run(&mut f));
        super::super::clean(&mut f);
        // The two slots written go back at the return, and nothing reads the global stack.
        assert_eq!(stack_accesses(&f), [true, true], "{f:?}");
        let tops =
            f.blocks.iter().flat_map(|b| &b.instrs).filter(|i| matches!(&i.inst, Inst::Load { ptr, .. } if reaches(&f, ptr, X87_TOP)));
        assert_eq!(tops.count(), 1, "{f:?}");
    }

    #[test]
    fn a_call_closes_the_region_and_the_next_opens_from_the_globals() {
        let (mut b, entry) = function();
        push(&mut b, entry, 1.5);
        call(&mut b, entry);
        let r = st(&mut b, entry, 0);
        moved(&mut b, entry, 1);
        b.terminate(entry, Terminator::Ret(Some((Type::Double, Operand::Local(r)))));
        let mut f = b.finish();
        assert!(run(&mut f));
        super::super::clean(&mut f);
        // The pushed slot is stored before the call; the callee's `st(0)` is loaded after it.
        let calls = |i: &Instruction| matches!(i.inst, Inst::Call { .. });
        let at = f.blocks[0].instrs.iter().position(calls).expect("the call stays");
        let (ahead, after) = f.blocks[0].instrs.split_at(at);
        let accesses = |is: &[Instruction]| {
            is.iter()
                .filter_map(|i| match &i.inst {
                    Inst::Load { ptr, .. } if reaches(&f, ptr, X87_STACK) => Some(false),
                    Inst::Store { ptr, .. } if reaches(&f, ptr, X87_STACK) => Some(true),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(accesses(ahead), [true], "{f:?}");
        assert_eq!(accesses(after), [false], "{f:?}");
    }

    #[test]
    fn a_slot_written_on_one_path_is_loaded_where_the_region_opens() {
        let (mut b, entry) = function();
        let then = b.block("then");
        let join = b.block("join");
        let t = top(&mut b, entry);
        let is_zero = b.icmp(entry, ra_asm_ir::ICond::Eq, Type::I64, t, 0i64, "z");
        b.terminate(entry, Terminator::CondBr { cond: Operand::Local(is_zero), then, els: join });
        let p = st_ptr(&mut b, then, 0);
        b.store(then, Type::Double, Operand::Float(4.0), p);
        b.terminate(then, Terminator::Br(join));
        b.terminate(join, Terminator::Ret(Some((Type::Double, Operand::Float(0.0)))));
        let mut f = b.finish();
        assert!(run(&mut f));
        super::super::clean(&mut f);
        // `st(0)` comes in where the region opens and goes back at the return on both paths.
        assert_eq!(stack_accesses(&f), [false, true], "{f:?}");
    }

    #[test]
    fn an_address_of_a_slot_that_escapes_leaves_the_function_alone() {
        let (mut b, entry) = function();
        push(&mut b, entry, 1.5);
        let p = st_ptr(&mut b, entry, 0);
        b.call(entry, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct("F_00002000".into()), vec![(Type::Ptr, p.into())], "");
        b.terminate(entry, Terminator::Ret(Some((Type::Double, Operand::Float(0.0)))));
        let mut f = b.finish();
        let before = f.clone();
        assert!(!run(&mut f));
        assert_eq!(f, before);
    }
}
