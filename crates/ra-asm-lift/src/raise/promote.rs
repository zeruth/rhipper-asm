//! Promotion of the machine state to SSA values, in two modes.
//!
//! **Machine mode** keeps the `%State` interface: a load becomes the current value, a store sets
//! it, a block with several predecessors starts with a phi for each live slot, a call flushes the
//! dirty registers to the state and forgets them, a `ret` flushes the dirty registers. The slots
//! are the general registers and the flags; the vector registers stay in memory.
//!
//! **ABI mode** applies the inferred signatures: the function takes its parameters and returns its
//! result, the entry stack pointer is the top of a frame the function allocates, a call to a lifted
//! function passes arguments and takes a result, an import is called through its adapter, an
//! indirect call goes through the pointer itself (the driver makes every lifted function reachable
//! at its image address), and the vector registers live in a local array. No `%State` remains.
//!
//! A call through a vtable slot (see [`super::abi::virtual_calls`]) is a direct call when one
//! function fills the slot, and otherwise a dispatch over the code address the slot holds: the
//! block ends in a chain of tests against the functions' addresses, one arm per function calls
//! it by its signature, a trap takes any other address, and the block goes on in a continuation
//! whose phis join the arms' results. A call a traced run resolved (see
//! [`super::abi::traced_calls`]) is the same dispatch over the functions the run saw, with the
//! indirect call itself in place of the trap: the run proved the targets it took, not that there
//! are no others.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, FnType, Function, ICond, Inst, Instruction, Operand, Terminator, Type, ValueId};

use super::Target;
use super::abi::{Candidates, Kind, Signature};
use super::assume::{self, Assumption, Step};
use super::liveness::{
    CallKind, Liveness, NO_SLOTS, RAX, RCX, RDX, SLOTS, Slot, SlotSet, all_registers, call_kind, reverse_post_order, slot_pointers,
};
use crate::state::{X87_STACK, X87_TOP};
use crate::x87::X87Unit;
use crate::{import_of_wrapper, state::import_adapter_type};

/// What the ABI mode needs to know.
pub struct Abi<'a> {
    pub own: &'a Signature,
    pub all: &'a HashMap<String, Signature>,
    pub target: Target,
    pub x87: X87Unit,
    /// The calls through a vtable slot, by the value of the target, with the slot's functions;
    /// the calls a traced run resolved too, with the functions the run saw.
    pub virtual_calls: &'a HashMap<ValueId, Candidates>,
    /// The targets of `virtual_calls` a run saw rather than a vtable named: the dispatch keeps the
    /// indirect call for any other target.
    pub seen: &'a HashSet<ValueId>,
}

/// The trap code of a call through a vtable slot that holds an address outside the vtables.
const OUTSIDE_VTABLES: u64 = 10;

/// A call through a vtable slot several functions fill, lowered: the block so far becomes the
/// head, which ends in `terminator`, the first test of the chain; the tests, the arms and the
/// trap follow; the block goes on in the continuation, the last of `blocks`, which the rewriter
/// keeps filling.
struct Split {
    head: Vec<Instruction>,
    terminator: Terminator,
    blocks: Vec<Block>,
}

/// Pushes `v` onto the x87 stack, where an x86 function leaves a float result: the top moves
/// down and the new `st(0)` holds it, as the machine model's `fpush` does; the exact unit's slot
/// is 16 bytes and takes the double through the runtime.
fn push_x87(f: &mut Function, kind: Kind, v: ValueId, x87: X87Unit, out: &mut Vec<Instruction>) {
    fn emit(f: &mut Function, out: &mut Vec<Instruction>, hint: &str, ty: Type, inst: Inst) -> ValueId {
        let r = f.new_value(hint, ty);
        out.push(Instruction { result: Some(r), inst, address: None });
        r
    }
    let value = match kind {
        Kind::Float => emit(
            f,
            out,
            "ret.d",
            Type::Double,
            Inst::Cast { cast: Cast::FPExt, from: Type::Float, value: Operand::Local(v), to: Type::Double },
        ),
        _ => v,
    };
    let top_address = Operand::Address { global: X87_TOP.to_owned(), offset: 0 };
    let top_p = emit(f, out, "top.p", Type::Ptr, Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: top_address, to: Type::Ptr });
    let top = emit(f, out, "top", Type::I64, Inst::Load { ty: Type::I64, ptr: Operand::Local(top_p), align: 8, volatile: false });
    let down =
        emit(f, out, "top.d", Type::I64, Inst::Bin { op: BinOp::Sub, ty: Type::I64, lhs: Operand::Local(top), rhs: Operand::Int(1) });
    let wrapped =
        emit(f, out, "top", Type::I64, Inst::Bin { op: BinOp::And, ty: Type::I64, lhs: Operand::Local(down), rhs: Operand::Int(7) });
    out.push(Instruction {
        result: None,
        inst: Inst::Store { ty: Type::I64, value: Operand::Local(wrapped), ptr: Operand::Local(top_p), align: 8, volatile: false },
        address: None,
    });
    let shift = if x87 == X87Unit::Exact { 4 } else { 3 };
    let offset = emit(
        f,
        out,
        "st.off",
        Type::I64,
        Inst::Bin { op: BinOp::Shl, ty: Type::I64, lhs: Operand::Local(wrapped), rhs: Operand::Int(shift) },
    );
    let stack = Operand::Address { global: X87_STACK.to_owned(), offset: 0 };
    let address = emit(f, out, "st.addr", Type::I64, Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Local(offset), rhs: stack });
    let p = emit(
        f,
        out,
        "st.p",
        Type::Ptr,
        Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: Operand::Local(address), to: Type::Ptr },
    );
    let inst = if x87 == X87Unit::Exact {
        Inst::Call {
            fn_ty: FnType::new(Type::Void, vec![Type::Ptr, Type::Double]),
            callee: Callee::Direct("ra_f80_from_f64".into()),
            args: vec![(Type::Ptr, Operand::Local(p)), (Type::Double, Operand::Local(value))],
            tail: false,
        }
    } else {
        Inst::Store { ty: Type::Double, value: Operand::Local(value), ptr: Operand::Local(p), align: 8, volatile: false }
    };
    out.push(Instruction { result: None, inst, address: None });
}

/// The register parameter `index` of `params` arrives in.
fn param_register(params: &[Kind], index: usize, target: Target) -> Slot {
    target.param_slot(params, index).expect("a register parameter has a register")
}

/// Promotes the registers and flags of `f`, lifted for `target`, in machine mode.
pub fn promote(f: &mut Function, target: Target) {
    run(f, None, target);
}

/// Rewrites `f` by its signature: parameters, frame, calls with arguments, a returned value.
/// Returns what the rewrite assumed (see [`assumptions`]).
pub fn promote_abi(f: &mut Function, abi: &Abi<'_>) -> Vec<Assumption> {
    let assumed = assumptions(f);
    run(f, Some(abi), abi.target);
    assumed
}

/// What the ABI mode assumes of the machine function `f`, in the order of its calls. The stack
/// becomes a frame of the function's own, a local, and a callee gets its arguments by value and a
/// frame of its own: at every call into lifted code, direct or through `ra_call`, the callee is
/// assumed to reach the caller's frame only through an address it was handed, never by its own
/// stack pointer past its arguments. An import is a function of the host, which has no lifted
/// stack pointer to reach by; the machine mode keeps the one stack of the state and assumes
/// nothing.
pub fn assumptions(f: &Function) -> Vec<Assumption> {
    let mut out = Vec::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Call { callee: Callee::Direct(name), .. } = &i.inst else { continue };
        if name.starts_with("F_") || name == "ra_call" {
            out.push(Assumption { function: f.name.clone(), step: Step::Promote, kind: assume::Kind::PrivateLocal, site: i.address });
        }
    }
    out
}

/// The local array the vector registers live in, in ABI mode: as many as the target's state has.
fn xmm_array(target: Target) -> Type {
    Type::array(u64::from(target.xmms()), Type::I128)
}

/// The per-function rewrite.
struct Rewriter<'a> {
    abi: Option<&'a Abi<'a>>,
    target: Target,
    pointers: HashMap<ValueId, Slot>,
    replace: HashMap<ValueId, Operand>,
    /// ABI mode: the local vector register array, the frame, and the parameter values.
    xmm: Option<ValueId>,
    frame: Option<ValueId>,
    params: HashMap<Slot, Operand>,
    stack_param_values: Vec<ValueId>,
    /// The next free block id, for the blocks a split makes.
    next_block: u32,
}

fn run(f: &mut Function, abi: Option<&Abi<'_>>, target: Target) {
    let Some(&state) = f.params.first() else { return };
    if f.blocks.is_empty() {
        return;
    }
    let order = reverse_post_order(f);
    let preds = f.predecessors();
    let pointers = slot_pointers(f, state);
    let n = f.blocks.len();
    let index_of: HashMap<BlockId, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();

    // Liveness: the machine model has a call touch every register and a return hand all back;
    // the ABI model has a call read its callee's parameters and a return hand back the result.
    let all = all_registers(target);
    let machine_model = |_: &Inst| (all, all);
    let abi_model = |inst: &Inst| match abi {
        Some(abi) => super::abi::call_model_public(inst, abi.virtual_calls, abi.all, abi.target),
        None => (all, all),
    };
    let exit_set = match abi {
        None => all,
        Some(abi) => {
            let mut set = NO_SLOTS;
            match abi.own.returns {
                Some(Kind::Int) => set[RAX.index()] = true,
                Some(Kind::Wide) => {
                    set[RAX.index()] = true;
                    set[RDX.index()] = true;
                }
                Some(Kind::Float | Kind::Double) => set[Slot::Xmm(0).index()] = true,
                None => {}
            }
            set
        }
    };
    let liveness = if abi.is_some() {
        Liveness::with_calls(f, &pointers, &order, exit_set, &abi_model)
    } else {
        Liveness::with_calls(f, &pointers, &order, exit_set, &machine_model)
    };

    // In machine mode only the general registers and flags are promoted; in ABI mode all slots.
    let promoted = |slot: Slot| abi.is_some() || !slot.is_xmm();

    // Dirty: a slot some path wrote since the last flush (machine mode only).
    let mut dirty_in = vec![NO_SLOTS; n];
    let mut dirty_out = vec![NO_SLOTS; n];
    loop {
        let mut changed = false;
        for &bi in &order {
            let mut inn = NO_SLOTS;
            if let Some(ps) = preds.get(&f.blocks[bi].id) {
                for p in ps {
                    let pi = index_of[p];
                    for k in 0..SLOTS {
                        inn[k] |= dirty_out[pi][k];
                    }
                }
            }
            let mut out = inn;
            for (o, w) in out.iter_mut().zip(&liveness.writes[bi]) {
                *o |= *w;
            }
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

    let mut rw = Rewriter {
        abi,
        target,
        pointers,
        replace: HashMap::new(),
        xmm: None,
        frame: None,
        params: HashMap::new(),
        stack_param_values: Vec::new(),
        next_block: f.blocks.iter().map(|b| b.id.0 + 1).max().unwrap_or(0),
    };

    // ABI mode: the new parameter list, the frame and the vector array go into a prologue that
    // the entry block starts with.
    let mut prologue: Vec<Instruction> = Vec::new();
    if let Some(abi) = abi {
        let old_params = std::mem::take(&mut f.params);
        let _ = old_params;
        let mut types = Vec::new();
        for (p, kind) in abi.own.params.iter().enumerate() {
            let v = f.new_value(&format!("arg{p}"), kind.ty());
            f.params.push(v);
            types.push(kind.ty());
            rw.params.insert(param_register(&abi.own.params, p, target), Operand::Local(v));
        }
        for (k, kind) in abi.own.stack_params.iter().enumerate() {
            let v = f.new_value(&format!("arg{}", abi.own.params.len() + k), kind.ty());
            f.params.push(v);
            types.push(kind.ty());
            rw.stack_param_values.push(v);
        }
        // A variadic function takes the address of its caller's argument block; the frame pass
        // rebases what the code derives from the entry stack pointer above the return address
        // onto it, by the parameter's name.
        if abi.own.variadic {
            let v = f.new_value("varargs", Type::I64);
            f.params.push(v);
            types.push(Type::I64);
        }
        let ret = abi.own.returns.map_or(Type::Void, Kind::ty);
        f.ty = FnType::new(ret, types);

        let xmm = f.new_value("xmm", Type::Ptr);
        prologue.push(Instruction { result: Some(xmm), inst: Inst::Alloca { ty: xmm_array(target), align: 16 }, address: None });
        rw.xmm = Some(xmm);
        // The frame: the function's own space below the entry stack pointer, the return address
        // slot, the home space and the stack parameters above it. The entry stack pointer sits past
        // a 16-byte boundary as after a native call, so aligned spills land right. A function
        // whose stack pointer the analysis lost (an x86 call through a pointer pops an amount
        // known at run time) gets room below for what it may still push.
        let slack = if abi.own.dynamic_stack { 4096 } else { 0 };
        let below = abi.own.frame_below.max(16) + 16 + slack;
        let entry = target.entry_misalignment();
        let above = target.first_stack_arg() as u64 + abi.own.stack_bytes(target) as u64;
        let size = below + entry + above + 64;
        let frame = f.new_value("frame", Type::Ptr);
        prologue.push(Instruction {
            result: Some(frame),
            inst: Inst::Alloca { ty: Type::array(size, Type::I8), align: 16 },
            address: None,
        });
        rw.frame = Some(frame);
        let top = f.new_value("frame.top", Type::Ptr);
        prologue.push(Instruction {
            result: Some(top),
            inst: Inst::Gep {
                base_ty: Type::I8,
                ptr: Operand::Local(frame),
                indices: vec![(Type::I64, Operand::Int(i128::from(below + entry)))],
                inbounds: true,
            },
            address: None,
        });
        let rsp0 = f.new_value("rsp0", Type::I64);
        prologue.push(Instruction {
            result: Some(rsp0),
            inst: Inst::Cast { cast: Cast::PtrToInt, from: Type::Ptr, value: Operand::Local(top), to: Type::I64 },
            address: None,
        });
        rw.params.insert(target.sp(), Operand::Local(rsp0));
        // The stack parameters go to their slots: the code reads them from memory. A dword slot
        // takes the low half of the 64-bit value.
        let offsets = abi.own.stack_offsets(target);
        for (k, (&v, kind)) in rw.stack_param_values.clone().iter().zip(&abi.own.stack_params).enumerate() {
            let slot = f.new_value(&format!("stack.arg{k}.p"), Type::Ptr);
            prologue.push(Instruction {
                result: Some(slot),
                inst: Inst::Gep {
                    base_ty: Type::I8,
                    ptr: Operand::Local(top),
                    indices: vec![(Type::I64, Operand::Int(i128::from(target.first_stack_arg() + offsets[k])))],
                    inbounds: true,
                },
                address: None,
            });
            let width = kind.stack_width(target);
            let (ty, value) = if width == 8 {
                (Type::I64, Operand::Local(v))
            } else {
                let low = f.new_value(&format!("stack.arg{k}.lo"), Type::I32);
                prologue.push(Instruction {
                    result: Some(low),
                    inst: Inst::Cast { cast: Cast::Trunc, from: Type::I64, value: Operand::Local(v), to: Type::I32 },
                    address: None,
                });
                (Type::I32, Operand::Local(low))
            };
            prologue.push(Instruction {
                result: None,
                inst: Inst::Store { ty, value, ptr: Operand::Local(slot), align: width as u32, volatile: false },
                address: None,
            });
        }
        // The float parameters go to their vector slots.
        for (p, kind) in abi.own.params.iter().enumerate() {
            if let Kind::Float | Kind::Double = kind {
                let Slot::Xmm(n) = param_register(&abi.own.params, p, target) else { continue };
                let slot = f.new_value(&format!("xmm{n}.p"), Type::Ptr);
                prologue.push(Instruction {
                    result: Some(slot),
                    inst: Inst::Gep {
                        base_ty: xmm_array(target),
                        ptr: Operand::Local(xmm),
                        indices: vec![(Type::I64, Operand::Int(0)), (Type::I64, Operand::Int(i128::from(n)))],
                        inbounds: true,
                    },
                    address: None,
                });
                prologue.push(Instruction {
                    result: None,
                    inst: Inst::Store { ty: Type::I128, value: Operand::Int(0), ptr: Operand::Local(slot), align: 16, volatile: false },
                    address: None,
                });
                let v = rw.params[&Slot::Xmm(n)].clone();
                prologue.push(Instruction {
                    result: None,
                    inst: Inst::Store { ty: kind.ty(), value: v, ptr: Operand::Local(slot), align: 16, volatile: false },
                    address: None,
                });
            }
        }
    }

    let mut final_values: Vec<HashMap<Slot, Operand>> = vec![HashMap::new(); n];
    let mut phis: Vec<Vec<(Slot, ValueId)>> = vec![Vec::new(); n];
    let mut new_blocks: Vec<Option<Block>> = (0..n).map(|_| None).collect();
    let mut visited = vec![false; n];
    // The blocks the splits of a virtual call make, after the function's own; a split block
    // goes on under a new id, which its successors' phis name as their predecessor.
    let mut extra_blocks: Vec<Block> = Vec::new();
    let mut renamed: HashMap<BlockId, BlockId> = HashMap::new();

    for &bi in &order {
        let block = f.blocks[bi].clone();
        let mut current: HashMap<Slot, Operand> = HashMap::new();
        let mut dirty: SlotSet = dirty_in[bi];
        let mut instrs: Vec<Instruction> = Vec::with_capacity(block.instrs.len());
        let pred_list: Vec<BlockId> = preds.get(&block.id).cloned().unwrap_or_default();
        let is_entry = bi == 0;
        // The pieces of the block before its last: the head under the block's own id, then the
        // arms and the continuations of its splits.
        let mut pieces: Vec<Block> = Vec::new();
        let mut piece_id = block.id;

        if is_entry {
            instrs.append(&mut prologue);
            if rw.abi.is_some() {
                // Every slot starts as its parameter, or undefined.
                for slot in target.slots() {
                    if slot.is_xmm() {
                        continue; // the vector slots live in memory, initialized where needed
                    }
                    current.insert(slot, rw.params.get(&slot).cloned().unwrap_or(Operand::Undef));
                }
            }
        } else if pred_list.len() == 1 && visited[index_of[&pred_list[0]]] {
            current.clone_from(&final_values[index_of[&pred_list[0]]]);
        } else if !pred_list.is_empty() {
            for slot in target.slots().filter(|s| promoted(*s) && !s.is_xmm()) {
                if liveness.live_in[bi][slot.index()] {
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
                Inst::Gep { .. } if i.result.is_some_and(|v| rw.pointers.contains_key(&v)) => {
                    // A vector slot pointer in ABI mode points into the local array; in machine
                    // mode the vector slots stay in the state and keep their pointer.
                    if rw.xmm.is_none() && rw.pointers[&i.result.unwrap()].is_xmm() {
                        instrs.push(i.clone());
                    } else if let (Some(xmm), Some(v)) = (rw.xmm, i.result)
                        && let Slot::Xmm(k) = rw.pointers[&v]
                    {
                        instrs.push(Instruction {
                            result: Some(v),
                            inst: Inst::Gep {
                                base_ty: xmm_array(target),
                                ptr: Operand::Local(xmm),
                                indices: vec![(Type::I64, Operand::Int(0)), (Type::I64, Operand::Int(i128::from(k)))],
                                inbounds: true,
                            },
                            address: i.address,
                        });
                    }
                }
                Inst::Load { ptr: Operand::Local(p), .. }
                    if rw.pointers.contains_key(p) && promoted(rw.pointers[p]) && !rw.pointers[p].is_xmm() =>
                {
                    let slot = rw.pointers[p];
                    let result = i.result.unwrap();
                    match current.get(&slot) {
                        Some(value) => {
                            rw.replace.insert(result, value.clone());
                        }
                        None if rw.abi.is_some() => {
                            rw.replace.insert(result, Operand::Undef);
                            current.insert(slot, Operand::Undef);
                        }
                        None => {
                            let (gep, load) = load_slot(f, state, slot, i.address);
                            instrs.push(gep);
                            rw.replace.insert(result, Operand::Local(load.result.unwrap()));
                            current.insert(slot, Operand::Local(load.result.unwrap()));
                            instrs.push(load);
                        }
                    }
                }
                Inst::Store { ptr: Operand::Local(p), value, .. }
                    if rw.pointers.contains_key(p) && promoted(rw.pointers[p]) && !rw.pointers[p].is_xmm() =>
                {
                    let slot = rw.pointers[p];
                    current.insert(slot, value.clone());
                    dirty[slot.index()] = true;
                }
                inst => match (call_kind(inst), rw.abi) {
                    (Some(CallKind::State), None) => {
                        flush(f, state, &current, &dirty, i.address, &mut instrs);
                        instrs.push(i.clone());
                        for slot in Slot::all().filter(|s| !s.is_xmm()) {
                            current.remove(&slot);
                            dirty[slot.index()] = false;
                        }
                    }
                    (Some(CallKind::State), Some(abi)) => {
                        if let Some(split) = rw.lower_call(f, abi, i, &mut current, &mut instrs) {
                            // A piece after the first is a split's continuation: it takes a name of
                            // its own, as the last piece does, or two blocks would share a label.
                            let name = if piece_id == block.id { block.name.clone() } else { format!("{}.{}", block.name, piece_id.0) };
                            pieces.push(Block {
                                id: piece_id,
                                name,
                                instrs: split.head,
                                terminator: split.terminator,
                                address: block.address,
                            });
                            let mut blocks = split.blocks;
                            let continuation = blocks.pop().expect("a split ends in its continuation");
                            pieces.extend(blocks);
                            piece_id = continuation.id;
                            instrs = continuation.instrs;
                        }
                    }
                    (Some(CallKind::NoReturn), None) => {
                        flush(f, state, &current, &dirty, i.address, &mut instrs);
                        instrs.push(i.clone());
                    }
                    _ => instrs.push(i.clone()),
                },
            }
        }
        let mut terminator = block.terminator.clone();
        if let Terminator::Ret(_) = block.terminator {
            match rw.abi {
                None => flush(f, state, &current, &dirty, block.instrs.last().and_then(|i| i.address), &mut instrs),
                Some(abi) => {
                    terminator = match abi.own.returns {
                        None => Terminator::Ret(None),
                        Some(Kind::Int) => Terminator::Ret(Some((Type::I64, current.get(&RAX).cloned().unwrap_or(Operand::Undef)))),
                        Some(Kind::Wide) => {
                            // edx:eax as one 64-bit value.
                            let low = f.new_value("ret.lo", Type::I64);
                            instrs.push(Instruction {
                                result: Some(low),
                                inst: Inst::Bin {
                                    op: BinOp::And,
                                    ty: Type::I64,
                                    lhs: current.get(&RAX).cloned().unwrap_or(Operand::Undef),
                                    rhs: Operand::Int(0xffff_ffff),
                                },
                                address: None,
                            });
                            let high = f.new_value("ret.hi", Type::I64);
                            instrs.push(Instruction {
                                result: Some(high),
                                inst: Inst::Bin {
                                    op: BinOp::Shl,
                                    ty: Type::I64,
                                    lhs: current.get(&RDX).cloned().unwrap_or(Operand::Undef),
                                    rhs: Operand::Int(32),
                                },
                                address: None,
                            });
                            let wide = f.new_value("ret.wide", Type::I64);
                            instrs.push(Instruction {
                                result: Some(wide),
                                inst: Inst::Bin { op: BinOp::Or, ty: Type::I64, lhs: Operand::Local(low), rhs: Operand::Local(high) },
                                address: None,
                            });
                            Terminator::Ret(Some((Type::I64, Operand::Local(wide))))
                        }
                        Some(kind) => {
                            // The float result is in xmm0's slot.
                            let p = f.new_value("xmm0.ret.p", Type::Ptr);
                            instrs.push(Instruction {
                                result: Some(p),
                                inst: Inst::Gep {
                                    base_ty: xmm_array(target),
                                    ptr: Operand::Local(rw.xmm.unwrap()),
                                    indices: vec![(Type::I64, Operand::Int(0)), (Type::I64, Operand::Int(0))],
                                    inbounds: true,
                                },
                                address: None,
                            });
                            let v = f.new_value("ret.f", kind.ty());
                            instrs.push(Instruction {
                                result: Some(v),
                                inst: Inst::Load { ty: kind.ty(), ptr: Operand::Local(p), align: 8, volatile: false },
                                address: None,
                            });
                            Terminator::Ret(Some((kind.ty(), Operand::Local(v))))
                        }
                    };
                }
            }
        }
        final_values[bi] = current;
        visited[bi] = true;
        let last = Block { id: piece_id, name: format!("{}.{}", block.name, piece_id.0), instrs, terminator, address: block.address };
        if pieces.is_empty() {
            new_blocks[bi] = Some(Block { name: block.name.clone(), ..last });
        } else {
            renamed.insert(block.id, piece_id);
            new_blocks[bi] = Some(pieces.remove(0));
            extra_blocks.extend(pieces);
            extra_blocks.push(last);
        }
    }

    // The phis: one incoming per predecessor. A predecessor whose final value is unknown loads
    // the slot at its end (machine mode) or passes undef (ABI mode).
    let mut tail_loads: HashMap<usize, Vec<Instruction>> = HashMap::new();
    let mut phi_incoming: HashMap<ValueId, Vec<(Operand, BlockId)>> = HashMap::new();
    for (bi, block_phis) in phis.iter().enumerate() {
        for &(slot, phi) in block_phis {
            let mut incoming = Vec::new();
            for p in preds.get(&f.blocks[bi].id).cloned().unwrap_or_default() {
                let pi = index_of[&p];
                let value = if let Some(v) = final_values[pi].get(&slot) {
                    v.clone()
                } else if rw.abi.is_some() {
                    Operand::Undef
                } else {
                    let (gep, load) = load_slot(f, state, slot, None);
                    let v = Operand::Local(load.result.unwrap());
                    tail_loads.entry(pi).or_default().extend([gep, load]);
                    final_values[pi].insert(slot, v.clone());
                    v
                };
                incoming.push((value, renamed.get(&p).copied().unwrap_or(p)));
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
    f.blocks = new_blocks.into_iter().zip(f.blocks.drain(..)).map(|(new, old)| new.unwrap_or(old)).collect();
    f.blocks.append(&mut extra_blocks);
    f.replace_uses(&rw.replace);
}

impl Rewriter<'_> {
    /// A pointer `offset` bytes from the value `base` (an address), as a fresh instruction.
    #[allow(clippy::unused_self)]
    fn address_ptr(&self, f: &mut Function, base: &Operand, offset: i64, hint: &str, out: &mut Vec<Instruction>) -> ValueId {
        let sum = f.new_value(&format!("{hint}.a"), Type::I64);
        out.push(Instruction {
            result: Some(sum),
            inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: base.clone(), rhs: Operand::Int(i128::from(offset)) },
            address: None,
        });
        let p = f.new_value(&format!("{hint}.p"), Type::Ptr);
        out.push(Instruction {
            result: Some(p),
            inst: Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: Operand::Local(sum), to: Type::Ptr },
            address: None,
        });
        p
    }

    fn xmm_slot(&self, f: &mut Function, n: u8, out: &mut Vec<Instruction>) -> ValueId {
        let p = f.new_value(&format!("xmm{n}.p"), Type::Ptr);
        out.push(Instruction {
            result: Some(p),
            inst: Inst::Gep {
                base_ty: xmm_array(self.target),
                ptr: Operand::Local(self.xmm.unwrap()),
                indices: vec![(Type::I64, Operand::Int(0)), (Type::I64, Operand::Int(i128::from(n)))],
                inbounds: true,
            },
            address: None,
        });
        p
    }

    fn target(&self) -> Target {
        self.target
    }

    /// The current stack pointer.
    fn sp(&self, current: &HashMap<Slot, Operand>) -> Operand {
        current.get(&self.target.sp()).cloned().unwrap_or(Operand::Undef)
    }

    /// A 64-bit integer loaded from the stack slot `offset` bytes above `rsp`: the slot's own
    /// width (`width` bytes), widened when it is a dword.
    fn stack_word(&self, f: &mut Function, rsp: &Operand, offset: i64, width: i64, hint: &str, out: &mut Vec<Instruction>) -> ValueId {
        let p = self.address_ptr(f, rsp, offset, hint, out);
        if width == 8 {
            let v = f.new_value(hint, Type::I64);
            out.push(Instruction {
                result: Some(v),
                inst: Inst::Load { ty: Type::I64, ptr: Operand::Local(p), align: 8, volatile: false },
                address: None,
            });
            return v;
        }
        let narrow = f.new_value(&format!("{hint}.d"), Type::I32);
        out.push(Instruction {
            result: Some(narrow),
            inst: Inst::Load { ty: Type::I32, ptr: Operand::Local(p), align: 4, volatile: false },
            address: None,
        });
        let v = f.new_value(hint, Type::I64);
        out.push(Instruction {
            result: Some(v),
            inst: Inst::Cast { cast: Cast::ZExt, from: Type::I32, value: Operand::Local(narrow), to: Type::I64 },
            address: None,
        });
        v
    }

    /// The arguments of a call under `sig`, from the current registers, the vector slots and the
    /// outgoing stack slots at `rsp + stack_base`.
    fn arguments(
        &self,
        f: &mut Function,
        sig: &Signature,
        current: &HashMap<Slot, Operand>,
        stack_base: i64,
        out: &mut Vec<Instruction>,
    ) -> Vec<(Type, Operand)> {
        let target = self.target();
        let mut args = Vec::new();
        for (p, kind) in sig.params.iter().enumerate() {
            match kind {
                Kind::Int | Kind::Wide => {
                    args.push((Type::I64, current.get(&param_register(&sig.params, p, target)).cloned().unwrap_or(Operand::Undef)));
                }
                Kind::Float | Kind::Double => {
                    let Slot::Xmm(n) = param_register(&sig.params, p, target) else { continue };
                    let slot = self.xmm_slot(f, n, out);
                    let v = f.new_value(&format!("farg{p}"), kind.ty());
                    out.push(Instruction {
                        result: Some(v),
                        inst: Inst::Load { ty: kind.ty(), ptr: Operand::Local(slot), align: 8, volatile: false },
                        address: None,
                    });
                    args.push((kind.ty(), Operand::Local(v)));
                }
            }
        }
        let rsp = self.sp(current);
        for (k, (kind, offset)) in sig.stack_params.iter().zip(sig.stack_offsets(target)).enumerate() {
            let v = match kind {
                Kind::Int | Kind::Wide => self.stack_word(f, &rsp, stack_base + offset, kind.stack_width(target), &format!("sarg{k}"), out),
                // A float on the stack (an x86 `cdecl` double) is read as one.
                Kind::Float | Kind::Double => {
                    let p = self.address_ptr(f, &rsp, stack_base + offset, &format!("sarg{k}"), out);
                    let v = f.new_value(&format!("sarg{k}"), kind.ty());
                    out.push(Instruction {
                        result: Some(v),
                        inst: Inst::Load { ty: kind.ty(), ptr: Operand::Local(p), align: 4, volatile: false },
                        address: None,
                    });
                    v
                }
            };
            args.push((kind.ty(), Operand::Local(v)));
        }
        // A variadic callee reads its arguments through the address of the block.
        if sig.variadic {
            let p = self.address_ptr(f, &rsp, stack_base, "varargs", out);
            let v = f.new_value("varargs", Type::I64);
            out.push(Instruction {
                result: Some(v),
                inst: Inst::Cast { cast: Cast::PtrToInt, from: Type::Ptr, value: Operand::Local(p), to: Type::I64 },
                address: None,
            });
            args.push((Type::I64, Operand::Local(v)));
        }
        args
    }

    /// Eight integers for an import or an unknown callee: on Win64 the four register arguments
    /// and four stack slots; on x86 six stack dwords, then `ecx` and `edx`, the order the
    /// universal stubs and the imports take.
    fn universal_arguments(
        &self,
        f: &mut Function,
        current: &HashMap<Slot, Operand>,
        stack_base: i64,
        out: &mut Vec<Instruction>,
    ) -> Vec<(Type, Operand)> {
        let target = self.target();
        let mut args = Vec::new();
        let rsp = self.sp(current);
        if target.is_64() {
            for slot in target.int_param_slots() {
                args.push((Type::I64, current.get(slot).cloned().unwrap_or(Operand::Undef)));
            }
            // ARM64's eight argument registers are the eight already.
            let stack_words = 8 - target.int_param_slots().len() as i64;
            for k in 0..stack_words {
                let v = self.stack_word(f, &rsp, stack_base + 8 * k, 8, &format!("sarg{k}"), out);
                args.push((Type::I64, Operand::Local(v)));
            }
        } else {
            for k in 0..6 {
                let v = self.stack_word(f, &rsp, stack_base + 4 * k, 4, &format!("sarg{k}"), out);
                args.push((Type::I64, Operand::Local(v)));
            }
            for slot in [RCX, RDX] {
                args.push((Type::I64, current.get(&slot).cloned().unwrap_or(Operand::Undef)));
            }
        }
        args
    }

    /// After a call: the caller-saved registers are gone, but for the ones the callee is known to
    /// preserve; the result is in `rax` or `xmm0`.
    fn after_call(
        &self,
        f: &mut Function,
        current: &mut HashMap<Slot, Operand>,
        returns: Option<Kind>,
        result: Option<ValueId>,
        preserves: &SlotSet,
        out: &mut Vec<Instruction>,
    ) {
        let target = self.target();
        for slot in target.slots() {
            if !target.callee_saved(slot) && !slot.is_xmm() && !preserves[slot.index()] {
                current.insert(slot, Operand::Undef);
            }
        }
        match (returns, result) {
            (Some(Kind::Int), Some(v)) => {
                current.insert(RAX, Operand::Local(v));
            }
            (Some(Kind::Wide), Some(v)) => {
                // The 64-bit result lands in edx:eax.
                let low = f.new_value("ret.lo", Type::I64);
                out.push(Instruction {
                    result: Some(low),
                    inst: Inst::Bin { op: BinOp::And, ty: Type::I64, lhs: Operand::Local(v), rhs: Operand::Int(0xffff_ffff) },
                    address: None,
                });
                let high = f.new_value("ret.hi", Type::I64);
                out.push(Instruction {
                    result: Some(high),
                    inst: Inst::Bin { op: BinOp::LShr, ty: Type::I64, lhs: Operand::Local(v), rhs: Operand::Int(32) },
                    address: None,
                });
                current.insert(RAX, Operand::Local(low));
                current.insert(RDX, Operand::Local(high));
            }
            (Some(kind @ (Kind::Float | Kind::Double)), Some(v)) => {
                let slot = self.xmm_slot(f, 0, out);
                out.push(Instruction {
                    result: None,
                    inst: Inst::Store { ty: Type::I128, value: Operand::Int(0), ptr: Operand::Local(slot), align: 16, volatile: false },
                    address: None,
                });
                out.push(Instruction {
                    result: None,
                    inst: Inst::Store { ty: kind.ty(), value: Operand::Local(v), ptr: Operand::Local(slot), align: 16, volatile: false },
                    address: None,
                });
            }
            _ => {}
        }
    }

    /// A call to the lifted function `name` by its signature. The machine model pushed the return
    /// address: the outgoing arguments start a return address (and on Win64 the home space) above
    /// the current stack pointer, and the pointer comes back up by the return address and what
    /// the callee pops after, unless `pop` is off and the caller moves it itself.
    #[allow(clippy::too_many_arguments)]
    fn direct_call(
        &self,
        f: &mut Function,
        abi: &Abi<'_>,
        name: &str,
        address: Option<u64>,
        pop: bool,
        current: &mut HashMap<Slot, Operand>,
        out: &mut Vec<Instruction>,
    ) {
        let target = abi.target;
        let sig = abi.all.get(name).cloned().unwrap_or_default();
        let args = self.arguments(f, &sig, current, target.first_stack_arg(), out);
        let ret = sig.returns.map_or(Type::Void, Kind::ty);
        let result = if ret == Type::Void { None } else { Some(f.new_value("ret", ret.clone())) };
        out.push(Instruction {
            result,
            inst: Inst::Call {
                fn_ty: FnType::new(ret, args.iter().map(|(t, _)| t.clone()).collect()),
                callee: Callee::Direct(name.to_owned()),
                args,
                tail: false,
            },
            address,
        });
        let popped = pop.then(|| self.pop_after_call(f, current, sig.pops as i64, address, out));
        self.after_call(f, current, sig.returns, result, &sig.preserves, out);
        if let Some(popped) = popped {
            current.insert(self.target.sp(), Operand::Local(popped));
        }
    }

    /// `ra_call(state, target)` in ABI mode: the target `code` is an address of code; the driver
    /// makes every lifted function reachable at its address, so the call goes through it. An x86
    /// callee pops what it pops: the runtime's `ra_pops_of` says how much, from the import
    /// binding table for a native callee, else from what the universal stub recorded in
    /// `ra_pops`.
    fn indirect_call(
        &self,
        f: &mut Function,
        abi: &Abi<'_>,
        code: &Operand,
        address: Option<u64>,
        current: &mut HashMap<Slot, Operand>,
        out: &mut Vec<Instruction>,
    ) {
        let target = abi.target;
        let ptr = target.pushed_return();
        let fp = f.new_value("fp", Type::Ptr);
        out.push(Instruction {
            result: Some(fp),
            inst: Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: code.clone(), to: Type::Ptr },
            address,
        });
        let args = self.universal_arguments(f, current, target.first_stack_arg(), out);
        let result = f.new_value("ret", Type::I64);
        out.push(Instruction {
            result: Some(result),
            inst: Inst::Call { fn_ty: import_adapter_type(), callee: Callee::Indirect(Operand::Local(fp)), args, tail: false },
            address,
        });
        let rsp = self.sp(current);
        let mut up = Operand::Int(i128::from(ptr));
        if !target.is_64() {
            let pops = f.new_value("pops", Type::I64);
            out.push(Instruction {
                result: Some(pops),
                inst: Inst::Call {
                    fn_ty: FnType::new(Type::I64, vec![Type::I64]),
                    callee: Callee::Direct("ra_pops_of".into()),
                    args: vec![(Type::I64, code.clone())],
                    tail: false,
                },
                address,
            });
            let sum = f.new_value("pops.a", Type::I64);
            out.push(Instruction {
                result: Some(sum),
                inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: Operand::Local(pops), rhs: Operand::Int(i128::from(ptr)) },
                address,
            });
            up = Operand::Local(sum);
        }
        let popped = f.new_value("rsp", Type::I64);
        out.push(Instruction { result: Some(popped), inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: rsp, rhs: up }, address });
        self.after_call(f, current, Some(Kind::Int), Some(result), &NO_SLOTS, out);
        current.insert(self.target.sp(), Operand::Local(popped));
    }

    /// A block id no block has, with a name from `hint`.
    fn fresh_block(&mut self, hint: &str) -> (BlockId, String) {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        (id, format!("{hint}_{}", id.0))
    }

    /// The stack pointer after a call that popped the return address and `pops` bytes more.
    fn pop_after_call(
        &self,
        f: &mut Function,
        current: &HashMap<Slot, Operand>,
        pops: i64,
        address: Option<u64>,
        out: &mut Vec<Instruction>,
    ) -> ValueId {
        let rsp = self.sp(current);
        let popped = f.new_value("rsp", Type::I64);
        out.push(Instruction {
            result: Some(popped),
            inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: rsp, rhs: Operand::Int(i128::from(self.target.pushed_return() + pops)) },
            address,
        });
        popped
    }

    /// A call through a vtable slot that `candidates` fill: a chain of tests of the code address
    /// `code` the slot holds against each candidate's address, with a call to the candidate in
    /// its arm and a trap for any other address. The slot holds the function's own address, not
    /// a number (the data gave a code address the function's name), so the tests compare with
    /// the functions rather than switch over constants. The arms join in the continuation,
    /// where the result and, when the candidates pop differently, the stack pointer are phis;
    /// the instructions so far become the head, which ends in the first test. With `seen`, the
    /// candidates are what a run saw and any other address takes the indirect call, which pops
    /// by itself, so every arm pops its own.
    #[allow(clippy::too_many_arguments)]
    fn virtual_dispatch(
        &mut self,
        f: &mut Function,
        abi: &Abi<'_>,
        candidates: &Candidates,
        code: &Operand,
        address: Option<u64>,
        seen: bool,
        current: &mut HashMap<Slot, Operand>,
        out: &mut Vec<Instruction>,
    ) -> Split {
        let (continuation, continuation_name) = self.fresh_block("vcall.join");
        let (default, default_name) = self.fresh_block(if seen { "vcall.other" } else { "vcall.trap" });
        let pops: Vec<i64> = candidates.iter().map(|(_, name)| abi.all.get(name).map_or(0, |s| s.pops as i64)).collect();
        let same_pops = !seen && pops.iter().all(|&p| p == pops[0]);
        let arms: Vec<(BlockId, String)> = candidates.iter().map(|_| self.fresh_block("vcall")).collect();
        let tests: Vec<(BlockId, String)> = candidates.iter().skip(1).map(|_| self.fresh_block("vcall.test")).collect();
        let mut blocks = Vec::new();
        let mut finals: Vec<(BlockId, HashMap<Slot, Operand>)> = Vec::new();
        let mut terminator = None;
        for (k, (_, name)) in candidates.iter().enumerate() {
            // The test: in the head for the first candidate, in its own block after.
            let is = f.new_value("is", Type::I1);
            let test = Instruction {
                result: Some(is),
                inst: Inst::ICmp {
                    cond: ICond::Eq,
                    ty: Type::I64,
                    lhs: code.clone(),
                    rhs: Operand::Address { global: name.clone(), offset: 0 },
                },
                address,
            };
            let next = tests.get(k).map_or(default, |(id, _)| *id);
            let branch = Terminator::CondBr { cond: Operand::Local(is), then: arms[k].0, els: next };
            if k == 0 {
                out.push(test);
                terminator = Some(branch);
            } else {
                let (id, block_name) = tests[k - 1].clone();
                blocks.push(Block { id, name: block_name, instrs: vec![test], terminator: branch, address });
            }
            let mut instrs = Vec::new();
            let mut arm = current.clone();
            self.direct_call(f, abi, name, address, !same_pops, &mut arm, &mut instrs);
            let (id, block_name) = arms[k].clone();
            blocks.push(Block { id, name: block_name, instrs, terminator: Terminator::Br(continuation), address });
            finals.push((id, arm));
        }
        if seen {
            let mut instrs = Vec::new();
            let mut arm = current.clone();
            self.indirect_call(f, abi, code, address, &mut arm, &mut instrs);
            blocks.push(Block { id: default, name: default_name, instrs, terminator: Terminator::Br(continuation), address });
            finals.push((default, arm));
        } else {
            let trap = Instruction {
                result: None,
                inst: Inst::Call {
                    fn_ty: FnType::new(Type::Void, vec![Type::I64, Type::I64]),
                    callee: Callee::Direct("ra_trap".into()),
                    args: vec![
                        (Type::I64, Operand::Int(i128::from(address.unwrap_or(0)))),
                        (Type::I64, Operand::Int(i128::from(OUTSIDE_VTABLES))),
                    ],
                    tail: false,
                },
                address,
            };
            blocks.push(Block { id: default, name: default_name, instrs: vec![trap], terminator: Terminator::Unreachable, address });
        }
        // The continuation: a slot the arms agree on keeps its value, any other is a phi.
        let mut joined = Vec::new();
        for slot in Slot::all().filter(|s| !s.is_xmm()) {
            let values: Vec<Operand> = finals.iter().map(|(_, arm)| arm.get(&slot).cloned().unwrap_or(Operand::Undef)).collect();
            if values.iter().all(|v| *v == values[0]) {
                current.insert(slot, values[0].clone());
                continue;
            }
            let phi = f.new_value(&format!("{}.phi", slot.name()), slot.ty());
            let incoming = values.into_iter().zip(finals.iter().map(|(id, _)| *id)).collect();
            joined.push(Instruction { result: Some(phi), inst: Inst::Phi { ty: slot.ty(), incoming }, address });
            current.insert(slot, Operand::Local(phi));
        }
        if same_pops {
            let popped = self.pop_after_call(f, current, pops[0], address, &mut joined);
            current.insert(self.target.sp(), Operand::Local(popped));
        }
        blocks.push(Block { id: continuation, name: continuation_name, instrs: joined, terminator: Terminator::Unreachable, address });
        Split { head: std::mem::take(out), terminator: terminator.expect("a candidate"), blocks }
    }

    /// A state call in ABI mode: a lifted callee by its signature, an import through its adapter,
    /// a call through a vtable slot to the functions that fill it, an indirect call through the
    /// pointer. A call through a slot several functions fill splits the block (see [`Split`]).
    fn lower_call(
        &mut self,
        f: &mut Function,
        abi: &Abi<'_>,
        i: &Instruction,
        current: &mut HashMap<Slot, Operand>,
        out: &mut Vec<Instruction>,
    ) -> Option<Split> {
        let Inst::Call { callee, args: old_args, .. } = &i.inst else { return None };
        let address = i.address;
        let target = abi.target;
        match callee {
            Callee::Direct(name) if name.starts_with("F_") => {
                // The push of the return address, the store just before the call, has no reader
                // once the call takes its arguments: drop it.
                drop_return_address_push(out, address);
                self.direct_call(f, abi, name, address, true, current, out);
            }
            Callee::Direct(name)
                if name == "ra_call"
                    && old_args.get(1).and_then(|(_, o)| o.local()).is_some_and(|v| abi.virtual_calls.contains_key(&v)) =>
            {
                let code = old_args[1].1.clone();
                let key = code.local().unwrap();
                let candidates = &abi.virtual_calls[&key];
                let seen = abi.seen.contains(&key);
                drop_return_address_push(out, address);
                if let ([(_, name)], false) = (candidates.as_slice(), seen) {
                    self.direct_call(f, abi, name, address, true, current, out);
                } else {
                    return Some(self.virtual_dispatch(f, abi, candidates, &code, address, seen, current, out));
                }
            }
            Callee::Direct(name) if import_of_wrapper(name).is_some() => {
                let symbol = import_of_wrapper(name).unwrap().to_owned();
                let tail = name.len() > symbol.len() + "imp_".len(); // `imp_<symbol>.tail`
                let stack_base = if tail { target.first_stack_arg() } else { target.outgoing_at_call() };
                // An import with a typed prototype takes its doubles as doubles, and an x86 one
                // leaves a float result on the x87 stack.
                if let Some(proto) = super::prototypes::typed(&symbol) {
                    let sig = proto.signature(target);
                    let args = self.arguments(f, &sig, current, stack_base, out);
                    let fn_ty = proto.fn_type();
                    let result = (fn_ty.ret != Type::Void).then(|| f.new_value("ret", fn_ty.ret.clone()));
                    out.push(Instruction {
                        result,
                        inst: Inst::Call { fn_ty, callee: Callee::Direct(symbol), args, tail: false },
                        address,
                    });
                    match (proto.returns, result) {
                        (Some(kind @ (Kind::Float | Kind::Double)), Some(v)) if !target.is_64() => {
                            self.after_call(f, current, None, None, &NO_SLOTS, out);
                            push_x87(f, kind, v, abi.x87, out);
                        }
                        _ => self.after_call(f, current, proto.returns, result, &NO_SLOTS, out),
                    }
                    return None;
                }
                let mut args = self.universal_arguments(f, current, stack_base, out);
                // An import the prototype table knows takes its own number of arguments, all of
                // them from the stack past the registers: the universal eight end in `ecx` and
                // `edx` on x86 and in four stack words on Win64, so a longer prototype
                // (`CreateWindowExA` has twelve) reads the slots beyond them. ARM64's universal
                // eight are its eight argument registers.
                if let Some(arity) = super::prototypes::arity(&symbol) {
                    let (registers, universal_stack, width) = if target.is_arm64() {
                        (8, 0, 8)
                    } else if target.is_64() {
                        (4, 4, 8)
                    } else {
                        (0, 6, 4)
                    };
                    args.truncate(arity.min(registers + universal_stack));
                    let rsp = self.sp(current);
                    for k in universal_stack..arity.saturating_sub(registers) {
                        let v = self.stack_word(f, &rsp, stack_base + width * k as i64, width, &format!("sarg{k}"), out);
                        args.push((Type::I64, Operand::Local(v)));
                    }
                }
                let fn_ty = FnType::new(Type::I64, vec![Type::I64; args.len()]);
                let result = f.new_value("ret", Type::I64);
                out.push(Instruction {
                    result: Some(result),
                    inst: Inst::Call { fn_ty, callee: Callee::Direct(symbol), args, tail: false },
                    address,
                });
                self.after_call(f, current, Some(Kind::Int), Some(result), &NO_SLOTS, out);
            }
            Callee::Direct(name) if name == "ra_call" => {
                drop_return_address_push(out, address);
                let code = old_args.get(1).map_or(Operand::Undef, |(_, o)| o.clone());
                self.indirect_call(f, abi, &code, address, current, out);
            }
            _ => {
                // Something else that takes the state: keep it, with undefined registers after.
                out.push(i.clone());
                self.after_call(f, current, None, None, &NO_SLOTS, out);
            }
        }
        None
    }
}

/// Removes the store of the return address that the machine model's `call` pushed: the last
/// store emitted, of a constant a few bytes past the call's own address. In the 32-bit model the
/// widening of the new stack pointer follows the store; it stays, for the cleanup to drop.
fn drop_return_address_push(out: &mut Vec<Instruction>, call_address: Option<u64>) {
    let Some(call_address) = call_address else { return };
    let window = out.len().saturating_sub(4);
    let Some(at) = (window..out.len()).rev().find(|&k| matches!(out[k].inst, Inst::Store { .. })) else { return };
    if let Inst::Store { ty: Type::Int(_), value: Operand::Int(c), .. } = &out[at].inst
        && let Ok(c) = u64::try_from(*c)
        && c > call_address
        && c - call_address <= 16
    {
        out.remove(at);
    }
}

/// A load of `slot` from the state through a fresh pointer.
fn load_slot(f: &mut Function, state: ValueId, slot: Slot, address: Option<u64>) -> (Instruction, Instruction) {
    let (field, n) = slot.field();
    let p = f.new_value(&format!("p.{}", slot.name()), Type::Ptr);
    let gep = Instruction {
        result: Some(p),
        inst: Inst::Gep {
            base_ty: Type::Named("State".into()),
            ptr: Operand::Local(state),
            indices: vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(field)), (Type::I32, Operand::Int(n))],
            inbounds: true,
        },
        address,
    };
    let v = f.new_value(&slot.name(), slot.ty());
    let align = if slot.ty() == Type::I64 { 8 } else { 1 };
    let load = Instruction { result: Some(v), inst: Inst::Load { ty: slot.ty(), ptr: Operand::Local(p), align, volatile: false }, address };
    (gep, load)
}

/// Stores the dirty registers with a known value back to the state. The flags are dropped: no
/// convention passes them across a call or a return.
fn flush(
    f: &mut Function,
    state: ValueId,
    current: &HashMap<Slot, Operand>,
    dirty: &SlotSet,
    address: Option<u64>,
    out: &mut Vec<Instruction>,
) {
    for slot in Slot::all() {
        if slot.is_flag() || slot.is_xmm() || !dirty[slot.index()] {
            continue;
        }
        let Some(value) = current.get(&slot) else { continue };
        let (field, n) = slot.field();
        let p = f.new_value(&format!("p.{}", slot.name()), Type::Ptr);
        out.push(Instruction {
            result: Some(p),
            inst: Inst::Gep {
                base_ty: Type::Named("State".into()),
                ptr: Operand::Local(state),
                indices: vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(field)), (Type::I32, Operand::Int(n))],
                inbounds: true,
            },
            address,
        });
        out.push(Instruction {
            result: None,
            inst: Inst::Store { ty: slot.ty(), value: value.clone(), ptr: Operand::Local(p), align: 8, volatile: false },
            address,
        });
    }
}
