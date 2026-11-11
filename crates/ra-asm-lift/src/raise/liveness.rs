//! The slots of the machine state and their liveness over a function's blocks.
//!
//! A slot is a general register, a flag or a vector register of `%State`. The lift reaches them
//! through `getelementptr %State, ptr %state, i32 0, i32 F, i32 N`; this module names the slot a
//! pointer stands for and computes, per block, which slots are read before written and which are
//! written, and from those the slots live at each block's entry and exit.

use std::collections::HashMap;

use ra_asm_ir::{BlockId, Callee, Function, Inst, Operand, Terminator, Type, ValueId};

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

/// The liveness of the slots over a function.
pub struct Liveness {
    pub index_of: HashMap<BlockId, usize>,
    /// The slots read before written, per block.
    pub reads: Vec<SlotSet>,
    /// The slots written, per block.
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
        let mut reads = vec![NO_SLOTS; n];
        let mut writes = vec![NO_SLOTS; n];
        let mut exit_reads = vec![NO_SLOTS; n];
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut written = NO_SLOTS;
            for i in &b.instrs {
                match &i.inst {
                    Inst::Load { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p)
                            && !written[slot.index()]
                        {
                            reads[bi][slot.index()] = true;
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            written[slot.index()] = true;
                            writes[bi][slot.index()] = true;
                        }
                    }
                    inst => match call_kind(inst) {
                        Some(CallKind::State) => {
                            let (call_reads, call_writes) = calls(inst);
                            for slot in Slot::all() {
                                if call_reads[slot.index()] && !written[slot.index()] {
                                    reads[bi][slot.index()] = true;
                                }
                                if call_writes[slot.index()] {
                                    written[slot.index()] = true;
                                    writes[bi][slot.index()] = true;
                                }
                            }
                        }
                        Some(CallKind::NoReturn) => break,
                        _ => {}
                    },
                }
            }
            if let Terminator::Ret(_) = b.terminator {
                exit_reads[bi] = exit_set;
            }
        }

        let mut live_in = vec![NO_SLOTS; n];
        let mut live_out = vec![NO_SLOTS; n];
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
                    inn[k] |= out[k] && !writes[bi][k];
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
            for i in b.instrs.iter().rev() {
                match &i.inst {
                    Inst::Load { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            live[slot.index()] = true;
                        }
                    }
                    Inst::Store { ptr: Operand::Local(p), .. } => {
                        if let Some(slot) = pointers.get(p) {
                            live[slot.index()] = false;
                        }
                    }
                    inst => match call_kind(inst) {
                        Some(CallKind::State) => {
                            at_calls.push(live);
                            let (call_reads, call_writes) = calls(inst);
                            for slot in Slot::all() {
                                if call_writes[slot.index()] {
                                    live[slot.index()] = false;
                                }
                                if call_reads[slot.index()] {
                                    live[slot.index()] = true;
                                }
                            }
                        }
                        Some(CallKind::NoReturn) => live = NO_SLOTS,
                        _ => {}
                    },
                }
            }
            at_calls.reverse();
            live_after_call[bi] = at_calls;
        }
        Self { index_of, reads, writes, live_in, live_out, live_after_call }
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
