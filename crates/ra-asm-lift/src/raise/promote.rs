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

use std::collections::HashMap;

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, FnType, Function, Inst, Instruction, Operand, Terminator, Type, ValueId};

use super::Target;
use super::abi::{Kind, Signature};
use super::liveness::{
    CallKind, Liveness, NO_SLOTS, RAX, RCX, RDX, RSP, SLOTS, Slot, SlotSet, all_registers, call_kind, reverse_post_order, slot_pointers,
};
use crate::state::POPS;
use crate::{import_of_wrapper, state::import_adapter_type};

/// What the ABI mode needs to know.
pub struct Abi<'a> {
    pub own: &'a Signature,
    pub all: &'a HashMap<String, Signature>,
    pub target: Target,
}

/// The register parameter `index` of `params` arrives in.
fn param_register(params: &[Kind], index: usize, target: Target) -> Slot {
    target.param_slot(params, index).expect("a register parameter has a register")
}

/// Promotes the registers and flags of `f` in machine mode.
pub fn promote(f: &mut Function) {
    run(f, None);
}

/// Rewrites `f` by its signature: parameters, frame, calls with arguments, a returned value.
pub fn promote_abi(f: &mut Function, abi: &Abi<'_>) {
    run(f, Some(abi));
}

/// The per-function rewrite.
struct Rewriter<'a> {
    abi: Option<&'a Abi<'a>>,
    pointers: HashMap<ValueId, Slot>,
    replace: HashMap<ValueId, Operand>,
    /// ABI mode: the local vector register array, the frame, and the parameter values.
    xmm: Option<ValueId>,
    frame: Option<ValueId>,
    params: HashMap<Slot, Operand>,
    stack_param_values: Vec<ValueId>,
}

fn run(f: &mut Function, abi: Option<&Abi<'_>>) {
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
    let all = all_registers();
    let machine_model = |_: &Inst| (all, all);
    let abi_model = |inst: &Inst| match abi {
        Some(abi) => super::abi::call_model_public(inst, abi.all, abi.target),
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

    let mut rw =
        Rewriter { abi, pointers, replace: HashMap::new(), xmm: None, frame: None, params: HashMap::new(), stack_param_values: Vec::new() };

    // ABI mode: the new parameter list, the frame and the vector array go into a prologue that
    // the entry block starts with.
    let mut prologue: Vec<Instruction> = Vec::new();
    if let Some(abi) = abi {
        let old_params = std::mem::take(&mut f.params);
        let _ = old_params;
        let target = abi.target;
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
        let ret = abi.own.returns.map_or(Type::Void, Kind::ty);
        f.ty = FnType::new(ret, types);

        let xmm = f.new_value("xmm", Type::Ptr);
        prologue.push(Instruction { result: Some(xmm), inst: Inst::Alloca { ty: Type::array(16, Type::I128), align: 16 }, address: None });
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
        rw.params.insert(RSP, Operand::Local(rsp0));
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
                        base_ty: Type::array(16, Type::I128),
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

    for &bi in &order {
        let block = f.blocks[bi].clone();
        let mut current: HashMap<Slot, Operand> = HashMap::new();
        let mut dirty: SlotSet = dirty_in[bi];
        let mut instrs: Vec<Instruction> = Vec::with_capacity(block.instrs.len());
        let pred_list: Vec<BlockId> = preds.get(&block.id).cloned().unwrap_or_default();
        let is_entry = bi == 0;

        if is_entry {
            instrs.append(&mut prologue);
            if rw.abi.is_some() {
                // Every slot starts as its parameter, or undefined.
                for slot in Slot::all() {
                    if slot.is_xmm() {
                        continue; // the vector slots live in memory, initialized where needed
                    }
                    current.insert(slot, rw.params.get(&slot).cloned().unwrap_or(Operand::Undef));
                }
            }
        } else if pred_list.len() == 1 && visited[index_of[&pred_list[0]]] {
            current.clone_from(&final_values[index_of[&pred_list[0]]]);
        } else if !pred_list.is_empty() {
            for slot in Slot::all().filter(|s| promoted(*s) && !s.is_xmm()) {
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
                                base_ty: Type::array(16, Type::I128),
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
                        rw.lower_call(f, abi, i, &mut current, &mut instrs);
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
                                    base_ty: Type::array(16, Type::I128),
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
        new_blocks[bi] = Some(Block { id: block.id, name: block.name.clone(), instrs, terminator, address: block.address });
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
    f.blocks = new_blocks.into_iter().zip(f.blocks.drain(..)).map(|(new, old)| new.unwrap_or(old)).collect();
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
                base_ty: Type::array(16, Type::I128),
                ptr: Operand::Local(self.xmm.unwrap()),
                indices: vec![(Type::I64, Operand::Int(0)), (Type::I64, Operand::Int(i128::from(n)))],
                inbounds: true,
            },
            address: None,
        });
        p
    }

    fn target(&self) -> Target {
        self.abi.map_or(Target::WIN64, |a| a.target)
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
        let rsp = current.get(&RSP).cloned().unwrap_or(Operand::Undef);
        for (k, (kind, offset)) in sig.stack_params.iter().zip(sig.stack_offsets(target)).enumerate() {
            let v = self.stack_word(f, &rsp, stack_base + offset, kind.stack_width(target), &format!("sarg{k}"), out);
            args.push((kind.ty(), Operand::Local(v)));
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
        let rsp = current.get(&RSP).cloned().unwrap_or(Operand::Undef);
        if target.is_64() {
            for slot in target.int_param_slots() {
                args.push((Type::I64, current.get(slot).cloned().unwrap_or(Operand::Undef)));
            }
            for k in 0..4 {
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

    /// After a call: the caller-saved registers are gone, the result is in `rax` or `xmm0`.
    fn after_call(
        &self,
        f: &mut Function,
        current: &mut HashMap<Slot, Operand>,
        returns: Option<Kind>,
        result: Option<ValueId>,
        out: &mut Vec<Instruction>,
    ) {
        let target = self.target();
        for slot in Slot::all() {
            if !target.callee_saved(slot) && !slot.is_xmm() {
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

    /// A state call in ABI mode: a lifted callee by its signature, an import through its adapter,
    /// an indirect call through the pointer.
    fn lower_call(
        &mut self,
        f: &mut Function,
        abi: &Abi<'_>,
        i: &Instruction,
        current: &mut HashMap<Slot, Operand>,
        out: &mut Vec<Instruction>,
    ) {
        let Inst::Call { callee, args: old_args, .. } = &i.inst else { return };
        let address = i.address;
        let target = abi.target;
        let ptr = target.ptr();
        match callee {
            Callee::Direct(name) if name.starts_with("F_") => {
                let sig = abi.all.get(name).cloned().unwrap_or_default();
                // The machine model pushed the return address: the outgoing arguments start a
                // return address (and on Win64 the home space) above the current stack pointer,
                // and the pointer comes back up by the return address and what the callee pops
                // after. The push itself, the store just before the call, has no reader: drop it.
                drop_return_address_push(out, address);
                let args = self.arguments(f, &sig, current, target.first_stack_arg(), out);
                let ret = sig.returns.map_or(Type::Void, Kind::ty);
                let result = if ret == Type::Void { None } else { Some(f.new_value("ret", ret.clone())) };
                out.push(Instruction {
                    result,
                    inst: Inst::Call {
                        fn_ty: FnType::new(ret, args.iter().map(|(t, _)| t.clone()).collect()),
                        callee: Callee::Direct(name.clone()),
                        args,
                        tail: false,
                    },
                    address,
                });
                let rsp = current.get(&RSP).cloned().unwrap_or(Operand::Undef);
                let popped = f.new_value("rsp", Type::I64);
                out.push(Instruction {
                    result: Some(popped),
                    inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: rsp, rhs: Operand::Int(i128::from(ptr + sig.pops as i64)) },
                    address,
                });
                self.after_call(f, current, sig.returns, result, out);
                current.insert(RSP, Operand::Local(popped));
            }
            Callee::Direct(name) if import_of_wrapper(name).is_some() => {
                let symbol = import_of_wrapper(name).unwrap().to_owned();
                let tail = name.len() > symbol.len() + "imp_".len(); // `imp_<symbol>.tail`
                let stack_base = if tail { target.first_stack_arg() } else { target.outgoing_at_call() };
                let mut args = self.universal_arguments(f, current, stack_base, out);
                // An import the prototype table knows takes its own number of arguments.
                if let Some(arity) = super::prototypes::arity(&symbol) {
                    args.truncate(arity.min(8));
                }
                let fn_ty = FnType::new(Type::I64, vec![Type::I64; args.len()]);
                let result = f.new_value("ret", Type::I64);
                out.push(Instruction {
                    result: Some(result),
                    inst: Inst::Call { fn_ty, callee: Callee::Direct(symbol), args, tail: false },
                    address,
                });
                self.after_call(f, current, Some(Kind::Int), Some(result), out);
            }
            Callee::Direct(name) if name == "ra_call" => {
                // `ra_call(state, target)`: the target is an address of code; the driver makes
                // every lifted function reachable at its address, so the call goes through it. An
                // x86 callee pops what it pops; its universal stub says how much in `ra_pops`.
                drop_return_address_push(out, address);
                let code = old_args.get(1).map_or(Operand::Undef, |(_, o)| o.clone());
                let fp = f.new_value("fp", Type::Ptr);
                out.push(Instruction {
                    result: Some(fp),
                    inst: Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value: code, to: Type::Ptr },
                    address,
                });
                let args = self.universal_arguments(f, current, target.first_stack_arg(), out);
                let result = f.new_value("ret", Type::I64);
                out.push(Instruction {
                    result: Some(result),
                    inst: Inst::Call { fn_ty: import_adapter_type(), callee: Callee::Indirect(Operand::Local(fp)), args, tail: false },
                    address,
                });
                let rsp = current.get(&RSP).cloned().unwrap_or(Operand::Undef);
                let mut up = Operand::Int(i128::from(ptr));
                if !target.is_64() {
                    let pops_ptr = f.new_value("pops.p", Type::Ptr);
                    out.push(Instruction {
                        result: Some(pops_ptr),
                        inst: Inst::Cast {
                            cast: Cast::IntToPtr,
                            from: Type::I64,
                            value: Operand::Address { global: POPS.to_owned(), offset: 0 },
                            to: Type::Ptr,
                        },
                        address,
                    });
                    let pops = f.new_value("pops", Type::I64);
                    out.push(Instruction {
                        result: Some(pops),
                        inst: Inst::Load { ty: Type::I64, ptr: Operand::Local(pops_ptr), align: 8, volatile: false },
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
                out.push(Instruction {
                    result: Some(popped),
                    inst: Inst::Bin { op: BinOp::Add, ty: Type::I64, lhs: rsp, rhs: up },
                    address,
                });
                self.after_call(f, current, Some(Kind::Int), Some(result), out);
                current.insert(RSP, Operand::Local(popped));
            }
            _ => {
                // Something else that takes the state: keep it, with undefined registers after.
                out.push(i.clone());
                self.after_call(f, current, None, None, out);
            }
        }
    }
}

/// Removes the store of the return address that the machine model's `call` pushed: the last
/// instruction emitted, a store of a constant a few bytes past the call's own address.
fn drop_return_address_push(out: &mut Vec<Instruction>, call_address: Option<u64>) {
    let Some(call_address) = call_address else { return };
    if let Some(Instruction { inst: Inst::Store { ty: Type::Int(_), value: Operand::Int(c), .. }, .. }) = out.last()
        && let Ok(c) = u64::try_from(*c)
        && c > call_address
        && c - call_address <= 16
    {
        out.pop();
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
