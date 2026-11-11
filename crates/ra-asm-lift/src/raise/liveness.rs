//! The slots of the machine state and their liveness over a function's blocks.
//!
//! A slot is a general register, a flag or a vector register of `%State`. The lift reaches them
//! through `getelementptr %State, ptr %state, i32 0, i32 F, i32 N`; this module names the slot a
//! pointer stands for and computes, per block, which slots are read before written and which are
//! written, and from those the slots live at each block's entry and exit.

use std::collections::HashMap;

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, Function, Inst, Operand, Terminator, Type, ValueId};

pub const GPRS: usize = 16;
pub const FLAGS: usize = 8;
pub const XMMS: usize = 16;
pub const SLOTS: usize = GPRS + FLAGS + XMMS;

/// A slot of the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    Gpr(u8),
    Flag(u8),
    Xmm(u8),
}

pub const GPR_NAMES: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
pub const FLAG_NAMES: [&str; 8] = ["cf", "pf", "af", "zf", "sf", "df", "of", "f7"];

pub const RAX: Slot = Slot::Gpr(0);
pub const RCX: Slot = Slot::Gpr(1);
pub const RDX: Slot = Slot::Gpr(2);
pub const RSP: Slot = Slot::Gpr(4);
pub const R8: Slot = Slot::Gpr(8);
pub const R9: Slot = Slot::Gpr(9);

impl Slot {
    pub fn index(self) -> usize {
        match self {
            Self::Gpr(n) => n as usize,
            Self::Flag(n) => GPRS + n as usize,
            Self::Xmm(n) => GPRS + FLAGS + n as usize,
        }
    }

    pub fn from_index(i: usize) -> Self {
        if i < GPRS {
            Self::Gpr(i as u8)
        } else if i < GPRS + FLAGS {
            Self::Flag((i - GPRS) as u8)
        } else {
            Self::Xmm((i - GPRS - FLAGS) as u8)
        }
    }

    pub fn all() -> impl Iterator<Item = Slot> {
        (0..SLOTS).map(Self::from_index)
    }

    pub fn ty(self) -> Type {
        match self {
            Self::Gpr(_) => Type::I64,
            Self::Flag(_) => Type::I8,
            Self::Xmm(_) => Type::I128,
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::Gpr(n) => GPR_NAMES[n as usize].to_owned(),
            Self::Flag(n) => FLAG_NAMES[n as usize].to_owned(),
            Self::Xmm(n) => format!("xmm{n}"),
        }
    }

    /// The field of `%State` and the index in it.
    pub fn field(self) -> (i128, i128) {
        match self {
            Self::Gpr(n) => (0, i128::from(n)),
            Self::Flag(n) => (2, i128::from(n)),
            Self::Xmm(n) => (3, i128::from(n)),
        }
    }

    pub fn is_gpr(self) -> bool {
        matches!(self, Self::Gpr(_))
    }

    pub fn is_flag(self) -> bool {
        matches!(self, Self::Flag(_))
    }

    pub fn is_xmm(self) -> bool {
        matches!(self, Self::Xmm(_))
    }
}

pub type SlotSet = [bool; SLOTS];

pub const NO_SLOTS: SlotSet = [false; SLOTS];

/// The slot a `getelementptr` into the state names.
pub fn slot_of(inst: &Inst, state: ValueId) -> Option<Slot> {
    let Inst::Gep { base_ty: Type::Named(name), ptr: Operand::Local(p), indices, .. } = inst else { return None };
    if name != "State" || *p != state || indices.len() != 3 {
        return None;
    }
    let [(_, Operand::Int(0)), (_, Operand::Int(field)), (_, Operand::Int(n))] = indices.as_slice() else { return None };
    match (field, n) {
        (0, n) if (0..16).contains(n) => Some(Slot::Gpr(*n as u8)),
        (2, n) if (0..8).contains(n) => Some(Slot::Flag(*n as u8)),
        (3, n) if (0..16).contains(n) => Some(Slot::Xmm(*n as u8)),
        _ => None,
    }
}

/// The slot pointers of a function: value -> slot.
pub fn slot_pointers(f: &Function, state: ValueId) -> HashMap<ValueId, Slot> {
    let mut pointers = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(v), Some(slot)) = (i.result, slot_of(&i.inst, state)) {
                pointers.insert(v, slot);
            }
        }
    }
    pointers
}

/// What a call does to the slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// A lifted function or the runtime: reads and writes the state.
    State,
    /// A host function that never sees the state.
    Host,
    /// A call that does not return.
    NoReturn,
}

pub fn call_kind(inst: &Inst) -> Option<CallKind> {
    let Inst::Call { callee, args, .. } = inst else { return None };
    match callee {
        Callee::Direct(name) if name == "ra_trap" || name == "ra_unsupported" => Some(CallKind::NoReturn),
        Callee::Direct(name) if name.starts_with("llvm.") => Some(CallKind::Host),
        Callee::Direct(name) if name.starts_with("F_") || name == "ra_call" => Some(CallKind::State),
        // The runtime's helpers that take pointers into the lift's own memory, never the state.
        Callee::Direct(name) if name.starts_with("ra_f80_") || name == "ra_cpuid" => Some(CallKind::Host),
        Callee::Direct(_) => {
            if args.iter().any(|(t, _)| *t == Type::Ptr) {
                Some(CallKind::State)
            } else {
                Some(CallKind::Host)
            }
        }
        Callee::Indirect(_) => Some(CallKind::State),
    }
}

/// The slots a state call reads and writes: every register, no flag.
pub fn call_touches(slot: Slot) -> bool {
    !slot.is_flag()
}

/// Every register: what a `ret` hands back in the machine model, where the caller may read any.
pub fn all_registers() -> SlotSet {
    let mut set = NO_SLOTS;
    for slot in Slot::all().filter(|s| call_touches(*s)) {
        set[slot.index()] = true;
    }
    set
}

/// The bytes of each slot: bit `i` is byte `i` of a general register; a flag or a vector register
/// is all or nothing.
type ByteSet = [u8; SLOTS];

const NO_BYTES: ByteSet = [0; SLOTS];
const ALL_BYTES: u8 = 0xff;

fn any_byte(bytes: &ByteSet) -> SlotSet {
    bytes.map(|b| b != 0)
}

fn every_byte(set: &SlotSet) -> ByteSet {
    set.map(|b| if b { ALL_BYTES } else { 0 })
}

/// The low `bits / 8` bytes.
fn low_bytes(bits: u16) -> u8 {
    if bits >= 64 { ALL_BYTES } else { ((1u16 << (bits / 8)) - 1) as u8 }
}

/// The bytes the loads and stores of the general registers touch. The lift reads a sub-register
/// as a `trunc` of the whole (through a `lshr 8` for `ah`) and writes one by merging into the
/// whole: `load; and MASK; or placed; store`. The merge's load passes the kept bytes through
/// rather than reading them, and its store defines only the others; a load used any other way
/// reads the whole register, a store of any other value writes it.
struct Accesses {
    /// A load's result -> the bytes read.
    loads: HashMap<ValueId, u8>,
    /// A merge's store, by block and position -> the bytes written.
    stores: HashMap<(usize, usize), u8>,
}

impl Accesses {
    fn of(f: &Function, pointers: &HashMap<ValueId, Slot>) -> Self {
        let mut users: HashMap<ValueId, Vec<(usize, usize)>> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.instrs.iter().enumerate() {
                for v in i.inst.uses() {
                    users.entry(v).or_default().push((bi, ii));
                }
            }
        }
        let at = |(bi, ii): (usize, usize)| &f.blocks[bi].instrs[ii];
        let users_of = |v: ValueId| users.get(&v).map_or(&[][..], Vec::as_slice);
        // The one store a merge's value feeds, when it goes back to `slot`.
        let merge_store = |v: Option<ValueId>, slot: Slot| -> Option<(usize, usize)> {
            let [u] = users_of(v?) else { return None };
            match &at(*u).inst {
                Inst::Store { ptr: Operand::Local(p), .. } if pointers.get(p) == Some(&slot) => Some(*u),
                _ => None,
            }
        };
        let mut loads = HashMap::new();
        let mut stores = HashMap::new();
        for b in &f.blocks {
            for i in &b.instrs {
                let (Some(whole), Inst::Load { ptr: Operand::Local(p), .. }) = (i.result, &i.inst) else { continue };
                let Some(&slot) = pointers.get(p).filter(|s| s.is_gpr()) else { continue };
                let mut read = 0u8;
                for &u in users_of(whole) {
                    read |= match &at(u).inst {
                        Inst::Cast { cast: Cast::Trunc, to: Type::Int(bits), .. } => low_bytes(*bits),
                        Inst::Bin { op: BinOp::LShr, lhs: Operand::Local(w), rhs: Operand::Int(8), .. } if *w == whole => {
                            let high = at(u).result.map_or(&[][..], |h| users_of(h));
                            let truncs: Option<u8> = high
                                .iter()
                                .map(|&t| match &at(t).inst {
                                    Inst::Cast { cast: Cast::Trunc, to: Type::Int(bits), .. } if *bits < 64 => Some(low_bytes(*bits) << 1),
                                    _ => None,
                                })
                                .try_fold(0u8, |acc, b| Some(acc | b?));
                            truncs.unwrap_or(ALL_BYTES)
                        }
                        Inst::Bin { op: BinOp::And, lhs: Operand::Local(w), rhs: Operand::Int(mask), .. } if *w == whole => {
                            let kept = at(u).result;
                            let store = merge_store(kept, slot).or_else(|| {
                                let [o] = users_of(kept?) else { return None };
                                match &at(*o).inst {
                                    Inst::Bin { op: BinOp::Or, lhs, rhs, .. } if lhs.local() == kept || rhs.local() == kept => {
                                        merge_store(at(*o).result, slot)
                                    }
                                    _ => None,
                                }
                            });
                            match (store, written_bytes(*mask as u64)) {
                                (Some(pos), Some(written)) => {
                                    stores.insert(pos, written);
                                    0
                                }
                                _ => ALL_BYTES,
                            }
                        }
                        _ => ALL_BYTES,
                    };
                }
                loads.insert(whole, read);
            }
        }
        Self { loads, stores }
    }

    fn load(&self, v: Option<ValueId>) -> u8 {
        v.and_then(|v| self.loads.get(&v).copied()).unwrap_or(ALL_BYTES)
    }

    fn store(&self, at: (usize, usize)) -> u8 {
        self.stores.get(&at).copied().unwrap_or(ALL_BYTES)
    }
}

/// The bytes a merge with `mask` replaces: those the mask clears. A byte the mask splits is not
/// a register's.
fn written_bytes(mask: u64) -> Option<u8> {
    let mut written = 0u8;
    for i in 0..8 {
        match (mask >> (8 * i)) & 0xff {
            0xff => {}
            0 => written |= 1 << i,
            _ => return None,
        }
    }
    Some(written)
}

/// The liveness of the slots over a function. A general register is tracked by the byte: a write
/// of `cl` defines one, a read of `ecx` reads the four not yet defined, so a function that only
/// ever defines `cl` before it uses it has no `ecx` live at its entry. The sets here are the
/// any-byte view of that.
pub struct Liveness {
    pub index_of: HashMap<BlockId, usize>,
    /// The slots some byte of which is read before written, per block.
    pub reads: Vec<SlotSet>,
    /// The slots some byte of which is written, per block.
    pub writes: Vec<SlotSet>,
    pub live_in: Vec<SlotSet>,
    pub live_out: Vec<SlotSet>,
    /// Per block, per state call (in order), the slots live right after the call.
    pub live_after_call: Vec<Vec<SlotSet>>,
}

impl Liveness {
    /// Computes the liveness of `f`. A `ret` reads the slots of `exit_set`; a state call reads and
    /// writes every register.
    pub fn of(f: &Function, pointers: &HashMap<ValueId, Slot>, order: &[usize], exit_set: SlotSet) -> Self {
        let all = all_registers();
        Self::with_calls(f, pointers, order, exit_set, &|_| (all, all))
    }

    /// Computes the liveness of `f` with `calls` saying, for each state call, which slots it reads
    /// and which it writes: the callee's parameters and the caller-saved registers, by the
    /// convention, once the signatures are known.
    pub fn with_calls(
        f: &Function,
        pointers: &HashMap<ValueId, Slot>,
        order: &[usize],
        exit_set: SlotSet,
        calls: &dyn Fn(&Inst) -> (SlotSet, SlotSet),
    ) -> Self {
        let n = f.blocks.len();
        let index_of: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
        let accesses = Accesses::of(f, pointers);
        let calls = |inst: &Inst| {
            let (r, w) = calls(inst);
            (every_byte(&r), every_byte(&w))
        };
        let mut reads = vec![NO_BYTES; n];
        let mut writes = vec![NO_BYTES; n];
        let mut exit_reads = vec![NO_BYTES; n];
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut written = NO_BYTES;
            for (ii, i) in b.instrs.iter().enumerate() {
                match &i.inst {
                    Inst::Load { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            reads[bi][slot.index()] |= accesses.load(i.result) & !written[slot.index()];
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            written[slot.index()] |= accesses.store((bi, ii));
                            writes[bi][slot.index()] |= accesses.store((bi, ii));
                        }
                    }
                    inst => match call_kind(inst) {
                        Some(CallKind::State) => {
                            let (call_reads, call_writes) = calls(inst);
                            for k in 0..SLOTS {
                                reads[bi][k] |= call_reads[k] & !written[k];
                                written[k] |= call_writes[k];
                                writes[bi][k] |= call_writes[k];
                            }
                        }
                        Some(CallKind::NoReturn) => break,
                        _ => {}
                    },
                }
            }
            if let Terminator::Ret(_) = b.terminator {
                exit_reads[bi] = every_byte(&exit_set);
            }
        }

        let mut live_in = vec![NO_BYTES; n];
        let mut live_out = vec![NO_BYTES; n];
        loop {
            let mut changed = false;
            for &bi in order.iter().rev() {
                let b = &f.blocks[bi];
                let mut out = exit_reads[bi];
                for s in b.terminator.successors() {
                    let si = index_of[&s];
                    for k in 0..SLOTS {
                        out[k] |= live_in[si][k];
                    }
                }
                let mut inn = reads[bi];
                for k in 0..SLOTS {
                    inn[k] |= out[k] & !writes[bi][k];
                }
                if inn != live_in[bi] || out != live_out[bi] {
                    live_in[bi] = inn;
                    live_out[bi] = out;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Backward over each block: the slots live after each state call.
        let mut live_after_call = vec![Vec::new(); n];
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut live = live_out[bi];
            let mut at_calls = Vec::new();
            for (ii, i) in b.instrs.iter().enumerate().rev() {
                match &i.inst {
                    Inst::Load { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            live[slot.index()] |= accesses.load(i.result);
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            live[slot.index()] &= !accesses.store((bi, ii));
                        }
                    }
                    inst => match call_kind(inst) {
                        Some(CallKind::State) => {
                            at_calls.push(any_byte(&live));
                            let (call_reads, call_writes) = calls(inst);
                            for k in 0..SLOTS {
                                live[k] = (live[k] & !call_writes[k]) | call_reads[k];
                            }
                        }
                        Some(CallKind::NoReturn) => live = NO_BYTES,
                        _ => {}
                    },
                }
            }
            at_calls.reverse();
            live_after_call[bi] = at_calls;
        }
        let view = |sets: &[ByteSet]| sets.iter().map(any_byte).collect();
        Self { index_of, reads: view(&reads), writes: view(&writes), live_in: view(&live_in), live_out: view(&live_out), live_after_call }
    }
}

/// The blocks reachable from the entry, in reverse post-order, as indexes into `f.blocks`.
pub fn reverse_post_order(f: &Function) -> Vec<usize> {
    let index_of: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut seen = vec![false; f.blocks.len()];
    let mut post = Vec::with_capacity(f.blocks.len());
    let mut stack: Vec<(usize, Vec<usize>, usize)> = Vec::new();
    if f.blocks.is_empty() {
        return post;
    }
    seen[0] = true;
    let succs = |i: usize| -> Vec<usize> { f.blocks[i].terminator.successors().iter().map(|s| index_of[s]).collect() };
    stack.push((0, succs(0), 0));
    while let Some((b, ss, k)) = stack.last_mut() {
        if *k < ss.len() {
            let next = ss[*k];
            *k += 1;
            if !seen[next] {
                seen[next] = true;
                let next_succs = succs(next);
                stack.push((next, next_succs, 0));
            }
        } else {
            post.push(*b);
            stack.pop();
        }
    }
    post.reverse();
    post
}
