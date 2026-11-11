//! Promotion of the machine state to SSA values.
//!
//! A slot is a general register (an `i64` of the state) or a flag (an `i8`). Inside a function the
//! lift reads and writes them through `getelementptr %State, ptr %state, i32 0, i32 F, i32 N`. The
//! pass:
//!
//! 1. finds the slot each such pointer names;
//! 2. computes, per block, the slots read before written and the slots written, and from them the
//!    slots live at each block's entry and exit (a call reads and writes every register; a `ret`
//!    reads every register; the flags are dead at both, since no convention passes them);
//! 3. walks the blocks in reverse post-order, keeping the current value of each slot: a load
//!    becomes the current value, a store sets it, a block with several predecessors starts with a
//!    phi for each live slot, a call flushes the dirty registers to the state and forgets them,
//!    a `ret` flushes the dirty registers;
//! 4. fills the phis from the predecessors' final values.
//!
//! The vector registers stay in memory for now: they are read at several types.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{Block, BlockId, Callee, Function, Inst, Instruction, Operand, Terminator, Type, ValueId};

/// A promoted slot: a general register or a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Slot {
    Gpr(u8),
    Flag(u8),
}

const GPRS: usize = 16;
const FLAGS: usize = 8;

impl Slot {
    fn index(self) -> usize {
        match self {
            Self::Gpr(n) => n as usize,
            Self::Flag(n) => GPRS + n as usize,
        }
    }

    fn all() -> impl Iterator<Item = Slot> {
        (0..GPRS as u8).map(Slot::Gpr).chain((0..FLAGS as u8).map(Slot::Flag))
    }

    fn ty(self) -> Type {
        match self {
            Self::Gpr(_) => Type::I64,
            Self::Flag(_) => Type::I8,
        }
    }

    fn name(self) -> String {
        match self {
            Self::Gpr(n) => ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"]
                [n as usize]
                .to_owned(),
            Self::Flag(n) => ["cf", "pf", "af", "zf", "sf", "df", "of", "f7"][n as usize].to_owned(),
        }
    }
}

const SLOTS: usize = GPRS + FLAGS;

type SlotSet = [bool; SLOTS];

/// The slot a `getelementptr` into the state names, when it is one of the promoted ones.
fn slot_of(inst: &Inst, state: ValueId) -> Option<Slot> {
    let Inst::Gep { base_ty: Type::Named(name), ptr: Operand::Local(p), indices, .. } = inst else { return None };
    if name != "State" || *p != state || indices.len() != 3 {
        return None;
    }
    let [(_, Operand::Int(0)), (_, Operand::Int(field)), (_, Operand::Int(n))] = indices.as_slice() else { return None };
    match (field, n) {
        (0, n) if (0..16).contains(n) => Some(Slot::Gpr(*n as u8)),
        (2, n) if (0..8).contains(n) => Some(Slot::Flag(*n as u8)),
        _ => None,
    }
}

/// What a call does to the slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    /// A lifted function or the runtime: reads and writes every register through the state.
    State,
    /// A host function that never sees the state.
    Host,
    /// A call that does not return.
    NoReturn,
}

fn call_kind(inst: &Inst) -> Option<CallKind> {
    let Inst::Call { callee, args, .. } = inst else { return None };
    match callee {
        Callee::Direct(name) if name == "ra_trap" || name == "ra_unsupported" => Some(CallKind::NoReturn),
        Callee::Direct(name) if name.starts_with("llvm.") => Some(CallKind::Host),
        Callee::Direct(name) if name.starts_with("F_") || name == "ra_call" => Some(CallKind::State),
        Callee::Direct(_) => {
            // An import adapter takes integers; anything that takes the state pointer is state-aware.
            if args.iter().any(|(t, _)| *t == Type::Ptr) { Some(CallKind::State) } else { Some(CallKind::Host) }
        }
        Callee::Indirect(_) => Some(CallKind::State),
    }
}

/// Promotes the registers and flags of `f`.
pub fn promote(f: &mut Function) {
    let Some(&state) = f.params.first() else { return };
    if f.blocks.is_empty() {
        return;
    }
    let order = reverse_post_order(f);
    let preds = f.predecessors();

    // The slot pointers: value -> slot.
    let mut pointers: HashMap<ValueId, Slot> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let (Some(v), Some(slot)) = (i.result, slot_of(&i.inst, state)) {
                pointers.insert(v, slot);
            }
        }
    }

    // Per-block gen (read before written) and kill (written) sets.
    let n = f.blocks.len();
    let index_of: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut reads = vec![[false; SLOTS]; n];
    let mut kill = vec![[false; SLOTS]; n];
    let mut exit_reads = vec![[false; SLOTS]; n];
    for (bi, b) in f.blocks.iter().enumerate() {
        let mut written = [false; SLOTS];
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
                        kill[bi][slot.index()] = true;
                    }
                }
                inst => match call_kind(inst) {
                    Some(CallKind::State) => {
                        for slot in Slot::all() {
                            if let Slot::Gpr(_) = slot {
                                if !written[slot.index()] {
                                    reads[bi][slot.index()] = true;
                                }
                                written[slot.index()] = true;
                                kill[bi][slot.index()] = true;
                            }
                        }
                    }
                    Some(CallKind::NoReturn) => break,
                    _ => {}
                },
            }
        }
        if let Terminator::Ret(_) = b.terminator {
            for slot in Slot::all() {
                if let Slot::Gpr(_) = slot {
                    exit_reads[bi][slot.index()] = true;
                }
            }
        }
    }

    // Liveness: live_in = gen | (live_out & !kill); live_out = union of successors' live_in.
    let mut live_in = vec![[false; SLOTS]; n];
    let mut live_out = vec![[false; SLOTS]; n];
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
                inn[k] |= out[k] && !kill[bi][k];
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

    // Dirty: a slot some path wrote since the last flush to the state.
    let mut dirty_in = vec![[false; SLOTS]; n];
    let mut dirty_out = vec![[false; SLOTS]; n];
    loop {
        let mut changed = false;
        for &bi in &order {
            let mut inn = [false; SLOTS];
            if let Some(ps) = preds.get(&f.blocks[bi].id) {
                for p in ps {
                    let pi = index_of[p];
                    for k in 0..SLOTS {
                        inn[k] |= dirty_out[pi][k];
                    }
                }
            }
            let mut out = inn;
            for k in 0..SLOTS {
                out[k] |= kill[bi][k];
            }
            // A call flushes and forgets the registers; the dirty set after it is what follows it.
            if out != dirty_out[bi] || inn != dirty_in[bi] {
                dirty_in[bi] = inn;
                dirty_out[bi] = out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // The rewrite. Each block gets its entry values: phis where it has several predecessors, the
    // predecessor's final values where it has one, loads from the state at the entry block.
    let mut replace: HashMap<ValueId, Operand> = HashMap::new();
    let mut final_values: Vec<HashMap<Slot, Operand>> = vec![HashMap::new(); n];
    let mut phis: Vec<Vec<(Slot, ValueId)>> = vec![Vec::new(); n];
    let mut new_blocks: Vec<Option<Block>> = (0..n).map(|_| None).collect();
    let mut visited = vec![false; n];

    for &bi in &order {
        let block = f.blocks[bi].clone();
        let mut current: HashMap<Slot, Operand> = HashMap::new();
        let mut dirty: SlotSet = dirty_in[bi];
        let mut instrs: Vec<Instruction> = Vec::with_capacity(block.instrs.len());
        let pred_list: Vec<BlockId> = preds.get(&block.id).cloned().unwrap_or_default();
        let is_entry = bi == 0;

        if !is_entry && pred_list.len() == 1 && visited[index_of[&pred_list[0]]] {
            current.clone_from(&final_values[index_of[&pred_list[0]]]);
        } else if !is_entry && !pred_list.is_empty() {
            for slot in Slot::all() {
                if live_in[bi][slot.index()] {
                    let v = f.new_value(&format!("{}.phi", slot.name()), slot.ty());
                    instrs.push(Instruction {
                        result: Some(v),
                        inst: Inst::Phi { ty: slot.ty(), incoming: Vec::new() },
                        address: block.address,
                    });
                    phis[bi].push((slot, v));
                    current.insert(slot, Operand::Local(v));
                }
            }
        }

        for i in &block.instrs {
            match &i.inst {
                Inst::Gep { .. } if i.result.is_some_and(|v| pointers.contains_key(&v)) => {}
                Inst::Load { ptr: Operand::Local(p), .. } if pointers.contains_key(p) => {
                    let slot = pointers[p];
                    let result = i.result.unwrap();
                    if let Some(value) = current.get(&slot) {
                        replace.insert(result, value.clone());
                    } else {
                        // The value is in the state: load it here, through a fresh pointer.
                        let (gep, load) = load_slot(f, state, slot, i.address);
                        instrs.push(gep);
                        replace.insert(result, Operand::Local(load.result.unwrap()));
                        current.insert(slot, Operand::Local(load.result.unwrap()));
                        instrs.push(load);
                    }
                }
                Inst::Store { ptr: Operand::Local(p), value, .. } if pointers.contains_key(p) => {
                    let slot = pointers[p];
                    current.insert(slot, value.clone());
                    dirty[slot.index()] = true;
                }
                inst => {
                    match call_kind(inst) {
                        Some(CallKind::State) => {
                            flush(f, state, &current, &dirty, true, i.address, &mut instrs);
                            instrs.push(i.clone());
                            for slot in Slot::all() {
                                if let Slot::Gpr(_) = slot {
                                    current.remove(&slot);
                                    dirty[slot.index()] = false;
                                }
                            }
                            // The flags after a call: whatever the callee left; forget them.
                            for slot in Slot::all() {
                                if let Slot::Flag(_) = slot {
                                    current.remove(&slot);
                                    dirty[slot.index()] = false;
                                }
                            }
                            continue;
                        }
                        Some(CallKind::NoReturn) => {
                            flush(f, state, &current, &dirty, true, i.address, &mut instrs);
                        }
                        _ => {}
                    }
                    instrs.push(i.clone());
                }
            }
        }
        if let Terminator::Ret(_) = block.terminator {
            flush(f, state, &current, &dirty, true, block.instrs.last().and_then(|i| i.address), &mut instrs);
        }
        final_values[bi] = current;
        visited[bi] = true;
        new_blocks[bi] =
            Some(Block { id: block.id, name: block.name.clone(), instrs, terminator: block.terminator.clone(), address: block.address });
    }

    // The phis: one incoming per predecessor. A predecessor whose final value is unknown loads
    // the slot at its end.
    let mut tail_loads: HashMap<usize, Vec<Instruction>> = HashMap::new();
    let mut phi_incoming: HashMap<ValueId, Vec<(Operand, BlockId)>> = HashMap::new();
    for (bi, block_phis) in phis.iter().enumerate() {
        for &(slot, phi) in block_phis {
            let mut incoming = Vec::new();
            for p in preds.get(&f.blocks[bi].id).cloned().unwrap_or_default() {
                let pi = index_of[&p];
                let value = if let Some(v) = final_values[pi].get(&slot) {
                    v.clone()
                } else {
                    let (gep, load) = load_slot(f, state, slot, None);
                    let v = Operand::Local(load.result.unwrap());
                    tail_loads.entry(pi).or_default().extend([gep, load]);
                    final_values[pi].insert(slot, v.clone());
                    v
                };
                incoming.push((value, p));
            }
            phi_incoming.insert(phi, incoming);
        }
    }
    for (pi, loads) in tail_loads {
        if let Some(b) = new_blocks[pi].as_mut() {
            b.instrs.extend(loads);
        }
    }
    for b in new_blocks.iter_mut().flatten() {
        for i in &mut b.instrs {
            if let (Some(v), Inst::Phi { incoming, .. }) = (i.result, &mut i.inst)
                && let Some(list) = phi_incoming.remove(&v)
            {
                *incoming = list;
            }
        }
    }
    // Blocks the order did not reach are unreachable: they keep their old body.
    f.blocks = new_blocks.into_iter().zip(f.blocks.drain(..)).map(|(new, old)| new.unwrap_or(old)).collect();
    f.replace_uses(&replace);
    // A phi fed by a value that `replace` renamed got that through replace_uses; the self-loop
    // case where a phi's incoming is itself is left to the trivial-phi pruning.
    let _ = HashSet::<ValueId>::new();
}

/// A load of `slot` from the state through a fresh pointer.
fn load_slot(f: &mut Function, state: ValueId, slot: Slot, address: Option<u64>) -> (Instruction, Instruction) {
    let (field, n) = match slot {
        Slot::Gpr(n) => (0, n),
        Slot::Flag(n) => (2, n),
    };
    let p = f.new_value(&format!("p.{}", slot.name()), Type::Ptr);
    let gep = Instruction {
        result: Some(p),
        inst: Inst::Gep {
            base_ty: Type::Named("State".into()),
            ptr: Operand::Local(state),
            indices: vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(field)), (Type::I32, Operand::Int(i128::from(n)))],
            inbounds: true,
        },
        address,
    };
    let v = f.new_value(&slot.name(), slot.ty());
    let align = if slot.ty() == Type::I64 { 8 } else { 1 };
    let load = Instruction { result: Some(v), inst: Inst::Load { ty: slot.ty(), ptr: Operand::Local(p), align, volatile: false }, address };
    (gep, load)
}

/// Stores the dirty slots with a known value back to the state. The flags are dropped when
/// `registers_only`: no convention passes them across a call or a return.
fn flush(
    f: &mut Function,
    state: ValueId,
    current: &HashMap<Slot, Operand>,
    dirty: &SlotSet,
    registers_only: bool,
    address: Option<u64>,
    out: &mut Vec<Instruction>,
) {
    for slot in Slot::all() {
        if registers_only && matches!(slot, Slot::Flag(_)) {
            continue;
        }
        if !dirty[slot.index()] {
            continue;
        }
        let Some(value) = current.get(&slot) else { continue };
        let (field, n) = match slot {
            Slot::Gpr(n) => (0, n),
            Slot::Flag(n) => (2, n),
        };
        let p = f.new_value(&format!("p.{}", slot.name()), Type::Ptr);
        out.push(Instruction {
            result: Some(p),
            inst: Inst::Gep {
                base_ty: Type::Named("State".into()),
                ptr: Operand::Local(state),
                indices: vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(field)), (Type::I32, Operand::Int(i128::from(n)))],
                inbounds: true,
            },
            address,
        });
        let align = if slot.ty() == Type::I64 { 8 } else { 1 };
        out.push(Instruction {
            result: None,
            inst: Inst::Store { ty: slot.ty(), value: value.clone(), ptr: Operand::Local(p), align, volatile: false },
            address,
        });
    }
}

/// The blocks reachable from the entry, in reverse post-order, as indexes into `f.blocks`.
fn reverse_post_order(f: &Function) -> Vec<usize> {
    let index_of: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut seen = vec![false; f.blocks.len()];
    let mut post = Vec::with_capacity(f.blocks.len());
    // An explicit stack: (block, next successor index).
    let mut stack: Vec<(usize, Vec<usize>, usize)> = Vec::new();
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
