//! C++ exceptions, from the x64 unwind data and the frame handler's tables.
//!
//! The native runtime finds a handler by walking the stack with the unwind codes and the
//! `FuncInfo` of each frame; a port has no such walk, so the lift writes the walk into the code
//! as a protocol over the runtime's `ra_eh_*` functions. A throw site hands the object and its
//! catchable types to `ra_eh_throw`, which keeps a copy as the exception in flight. After every
//! call to lifted code a function asks `ra_eh_pending` and, when an exception is in flight, lands
//! in the block of the state the call was made in. The landing tries each try block that covers
//! the state, innermost first, with `ra_eh_catches` for each handler (which copies the object into
//! the catch's slot on a match); a match destroys what the try body built since its start (the
//! unwind map's actions, down to the try's low state) and enters the catch funclet; no match
//! destroys everything (down to state -1), undoes the prolog from the unwind codes, and returns
//! with the exception still in flight, so the caller's own check lands next. A catch funclet runs
//! inline in its parent, entered as the handler would call it (`rdx` the establisher frame, a
//! return address on the stack), and its `ret` resumes at the continuation in `rax`. The actions
//! run with the exception suspended, so a destructor that throws and catches of its own does not
//! see the one passing it.
//!
//! What the lift refuses, as a trap at the throw site: a thrown object with a copy constructor,
//! a destructor or a virtual base, which the runtime would have to call back into the port for.

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, Function};
use ra_asm_ir::{BinOp, BlockId, Callee, FnType, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;
use ra_asm_pe::exception::{CxxFrame, ThrowInfo, UnwindOp, type_descriptor_name};
use ra_asm_x86::iced_x86::{Instruction, Mnemonic, OpKind, Register as R};

use crate::x86::{Lifter, trap};

/// What the lift knows of a function's frame for an exception that passes it.
pub(crate) struct Frame {
    /// The C++ frame handler's tables, when the function is under it.
    pub cxx: Option<CxxFrame>,
    /// The prolog's operations, undone when an exception leaves the function.
    pub ops: Vec<UnwindOp>,
    /// The frame pointer's register, by x64 number, and its offset from the establisher frame.
    pub fp: Option<(u8, u16)>,
    /// Whether the exception table has an entry for the function.
    pub known: bool,
    /// The blocks of each funclet, by its entry, with the size of the funclet's own frame.
    pub funclets: HashMap<u64, (Vec<Block>, u64)>,
}

impl Frame {
    /// Nothing: a function of a program without exceptions.
    pub(crate) fn none() -> Self {
        Self { cxx: None, ops: Vec::new(), fp: None, known: false, funclets: HashMap::new() }
    }

    pub(crate) fn of(image: &Image, analysis: &Analysis<'_>, f: &Function) -> Self {
        let mut frame = Self::none();
        if let Some(entry) = image.runtime_functions.iter().find(|r| r.begin == f.entry)
            && let Ok(unwind) = entry.unwind_info(image)
        {
            frame.known = true;
            frame.ops = unwind.operations().unwrap_or_default();
            frame.fp = unwind.frame_register.map(|r| (r, unwind.frame_offset));
        }
        if let Some(cxx) = analysis.cxx_frames.get(&f.entry) {
            for funclet in &cxx.funclets {
                let blocks = f.blocks.iter().filter(|b| funclet.contains(b.start)).cloned().collect();
                let depth = funclet.unwind_info(image).and_then(|u| u.frame_size()).unwrap_or(0);
                frame.funclets.insert(funclet.begin, (blocks, depth));
            }
            frame.cxx = Some(cxx.clone());
        }
        frame
    }
}

/// The copy of a funclet being lifted inline.
pub(crate) struct FuncletCopy {
    pub kind: FuncletKind,
    /// The funclet's own frame size, which stands between its code and the establisher frame.
    pub depth: u64,
}

pub(crate) enum FuncletKind {
    /// A catch body: its `ret` resumes the parent at the continuation in `rax`, one of these.
    Catch { continuations: Vec<(u64, BlockId)> },
    /// An unwind action: its `ret` goes on with the unwind.
    Cleanup { exit: BlockId },
}

/// What a throw site throws.
enum Throw {
    /// A new object, described by the `ThrowInfo` at the address.
    Object(u64),
    /// `throw;`: the exception being handled.
    Rethrow,
}

/// The key the runtime matches a thrown type against a handler by: a hash of the decorated name
/// of the `TypeDescriptor`, the same for a type however many images define it.
pub fn type_key(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// An x64 general register by its number in the unwind codes (rax is 0).
fn gpr(number: u8) -> R {
    R::try_from(R::RAX as usize + usize::from(number)).unwrap_or(R::RAX)
}

fn xmm(number: u8) -> R {
    R::try_from(R::XMM0 as usize + usize::from(number)).unwrap_or(R::XMM0)
}

impl Lifter<'_, '_> {
    /// Whether the block at `start` is a funclet's: lifted in copies from the landings, never in
    /// the function's own walk.
    pub(crate) fn is_funclet_block(&self, start: u64) -> bool {
        self.frame.funclets.values().any(|(blocks, _)| blocks.iter().any(|b| b.start == start))
    }

    /// Whether `address` is a continuation of the catch funclet being copied.
    pub(crate) fn is_continuation(&self, address: u64) -> bool {
        matches!(&self.funclet, Some(FuncletCopy { kind: FuncletKind::Catch { continuations }, .. }) if continuations.iter().any(|(a, _)| *a == address))
    }

    /// A call to the exception runtime with integer arguments and no result.
    fn eh_call(&mut self, block: BlockId, name: &str, args: Vec<Operand>) {
        let fn_ty = FnType::new(Type::Void, vec![Type::I64; args.len()]);
        let args = args.into_iter().map(|a| (Type::I64, a)).collect();
        self.m.b.call(block, fn_ty, Callee::Direct(name.to_owned()), args, "");
    }

    /// After a call to lifted code: when an exception is in flight, land in the block of the
    /// state the call was made in; else go on in a fresh block.
    pub(crate) fn after_call(&mut self, block: BlockId, ip: u64) -> BlockId {
        if !self.lifted.eh {
            return block;
        }
        let pending = self.m.intrinsic(block, "ra_eh_pending", Type::I64, vec![], "eh.pending");
        let thrown = self.m.is_true(block, Type::I64, pending, "eh.thrown");
        let landing = self.landing(ip);
        let next = self.m.b.block(&format!("eh.ok.{ip:x}"));
        self.m.b.terminate(block, Terminator::CondBr { cond: thrown.into(), then: landing, els: next });
        next
    }

    /// The `ret` of the code being lifted: the function's own, or the end of a funclet copy.
    pub(crate) fn ret(&mut self, block: BlockId) {
        let continuations = match &self.funclet {
            None => {
                self.m.b.terminate(block, Terminator::Ret(None));
                return;
            }
            Some(FuncletCopy { kind: FuncletKind::Cleanup { exit }, .. }) => {
                self.m.b.terminate(block, Terminator::Br(*exit));
                return;
            }
            Some(FuncletCopy { kind: FuncletKind::Catch { continuations }, .. }) => continuations.clone(),
        };
        // The catch is over: its exception goes, and the parent resumes where the funclet says.
        self.eh_call(block, "ra_eh_leave_catch", vec![]);
        let address = self.m.b.address.unwrap_or(0);
        let target = self.m.read_gpr(block, R::RAX);
        let bad = self.m.b.block("eh.resume.bad");
        self.m.trap(bad, address, trap::BAD_SWITCH);
        let cases = continuations.into_iter().map(|(a, b)| (i128::from(a), b)).collect();
        self.m.b.terminate(block, Terminator::Switch { ty: Type::I64, value: target.into(), default: bad, cases });
    }

    /// The throw site, `_CxxThrowException(object, throw_info)`: the exception goes in flight and
    /// the function lands in the block of the throw's state.
    pub(crate) fn throw_site(&mut self, block: BlockId, ip: u64) {
        match self.throw_before() {
            None => {
                self.warn(format!("{ip:#x}: a throw whose ThrowInfo is not a constant"));
                self.m.trap(block, ip, trap::INVALID);
                return;
            }
            Some(Throw::Rethrow) => self.eh_call(block, "ra_eh_rethrow", vec![]),
            Some(Throw::Object(info_at)) => {
                let info = match ThrowInfo::parse(self.image, info_at) {
                    Ok(info) => info,
                    Err(e) => {
                        self.warn(format!("{ip:#x}: {e:#}"));
                        self.m.trap(block, ip, trap::INVALID);
                        return;
                    }
                };
                if info.destructor.is_some() || info.catchable.iter().any(|c| c.copy.is_some() || c.pdisp != -1) {
                    self.warn(format!("{ip:#x}: a thrown object with a constructor, a destructor or a virtual base is not lifted yet"));
                    self.m.trap(block, ip, trap::INVALID);
                    return;
                }
                let mut types = Vec::with_capacity(info.catchable.len());
                for c in &info.catchable {
                    match type_descriptor_name(self.image, c.type_descriptor) {
                        Ok(name) => types.push((type_key(&name), c.mdisp, c.size)),
                        Err(e) => {
                            self.warn(format!("{ip:#x}: {e:#}"));
                            self.m.trap(block, ip, trap::INVALID);
                            return;
                        }
                    }
                }
                let size = info.catchable.first().map_or(0, |c| c.size);
                let object = self.m.read_gpr(block, R::RCX);
                self.eh_call(block, "ra_eh_throw", vec![object.into(), i64::from(size).into()]);
                for (key, disp, size) in types {
                    self.eh_call(block, "ra_eh_throw_type", vec![(key as i64).into(), i64::from(disp).into(), i64::from(size).into()]);
                }
            }
        }
        let landing = self.landing(ip);
        self.m.b.terminate(block, Terminator::Br(landing));
    }

    /// What the current call throws, from the `ThrowInfo` it passes in `rdx`: the last write of
    /// `rdx` before it in the block, `lea rdx, [rip + info]`, or `xor edx, edx` for a rethrow.
    /// `None` when the block does not say.
    fn throw_before(&self) -> Option<Throw> {
        for i in self.block_instrs[..self.block_index].iter().rev() {
            if i.mnemonic() == Mnemonic::Call {
                return None;
            }
            let writes_rdx = i.op_count() > 0 && i.op_kind(0) == OpKind::Register && i.op_register(0).full_register() == R::RDX;
            if !writes_rdx {
                continue;
            }
            return match i.mnemonic() {
                Mnemonic::Lea if i.is_ip_rel_memory_operand() => Some(Throw::Object(i.ip_rel_memory_address())),
                Mnemonic::Xor if i.op_kind(1) == OpKind::Register && i.op_register(1) == i.op_register(0) => Some(Throw::Rethrow),
                Mnemonic::Mov if matches!(i.op_kind(1), OpKind::Immediate32 | OpKind::Immediate32to64 | OpKind::Immediate64) => {
                    let value = i.immediate(1);
                    Some(if value == 0 { Throw::Rethrow } else { Throw::Object(value) })
                }
                _ => None,
            };
        }
        None
    }

    /// The landing block for an exception in flight at `ip`: by the state of `ip` in the C++
    /// frame's tables, and by the funclet the code is a copy of, whose frame stands between the
    /// code and the function's.
    fn landing(&mut self, ip: u64) -> BlockId {
        let state = self.frame.cxx.as_ref().map_or(-1, |c| c.info.state_at(ip));
        let depth = self.funclet.as_ref().map(|c| c.depth);
        if let Some(&b) = self.landings.get(&(state, depth)) {
            return b;
        }
        let b = self.m.b.block(&format!("eh.state{}", if state < 0 { "_none".to_owned() } else { state.to_string() }));
        self.landings.insert((state, depth), b);
        self.build_landing(b, state);
        b
    }

    fn build_landing(&mut self, block: BlockId, state: i32) {
        let Some(cxx) = self.frame.cxx.clone() else {
            self.leave_function(block);
            return;
        };
        let mut cur = block;
        // The try blocks that cover the state, innermost first.
        let mut tries: Vec<usize> = (0..cxx.info.try_blocks.len()).filter(|&t| cxx.info.try_blocks[t].covers(state)).collect();
        tries.sort_by_key(|&t| cxx.info.try_blocks[t].high - cxx.info.try_blocks[t].low);
        for t in tries {
            let try_block = &cxx.info.try_blocks[t];
            for (h, handler) in try_block.handlers.iter().enumerate() {
                let key = match handler.type_descriptor.map(|td| type_descriptor_name(self.image, td)) {
                    None => 0,
                    Some(Ok(name)) => type_key(&name),
                    Some(Err(e)) => {
                        self.warn(format!("{:#x}: {e:#}", cxx.entry.begin));
                        self.m.trap(cur, cxx.entry.begin, trap::INVALID);
                        return;
                    }
                };
                let slot: Operand = if handler.catch_object == 0 {
                    0i64.into()
                } else {
                    let frame = self.establisher(cur);
                    self.m.b.bin(cur, BinOp::Add, Type::I64, frame, i64::from(handler.catch_object), "eh.catch.obj").into()
                };
                let flags = i64::from(handler.adjectives) | i64::from(self.m.bits) << 8;
                let args = vec![(Type::I64, (key as i64).into()), (Type::I64, slot), (Type::I64, flags.into())];
                let caught = self.m.intrinsic(cur, "ra_eh_catches", Type::I64, args, "eh.caught");
                let hit = self.m.is_true(cur, Type::I64, caught, "eh.hit");
                let taken = self.m.b.block(&format!("eh.catch{t}_{h}.s{state}"));
                let next = self.m.b.block(&format!("eh.next{t}_{h}.s{state}"));
                self.m.b.terminate(cur, Terminator::CondBr { cond: hit.into(), then: taken, els: next });
                // The catch: undo the try body since its start, then run the catch body.
                if let Some(after) = self.unwind_to(taken, state, try_block.low) {
                    let entry = self.catch_entry(t, h);
                    self.m.b.terminate(after, Terminator::Br(entry));
                }
                cur = next;
            }
        }
        // Nothing here catches it: the exception leaves the function, suspended while the
        // destructors of the way out run.
        self.eh_call(cur, "ra_eh_suspend", vec![]);
        if let Some(after) = self.unwind_to(cur, state, -1) {
            self.eh_call(after, "ra_eh_resume", vec![]);
            self.leave_function(after);
        }
    }

    /// The unwind map's way from state `from` to state `to`: the action of each state on the way,
    /// and the end of each catch body the way leaves. `None` when the map does not lead there,
    /// after a trap.
    fn unwind_to(&mut self, block: BlockId, from: i32, to: i32) -> Option<BlockId> {
        let cxx = self.frame.cxx.clone()?;
        let mut cur = from;
        let mut blk = block;
        for _ in 0..=cxx.info.unwind_map.len() {
            if cur == to {
                return Some(blk);
            }
            let Some(entry) = usize::try_from(cur).ok().and_then(|s| cxx.info.unwind_map.get(s)) else { break };
            if let Some(action) = entry.action {
                blk = self.run_cleanup(blk, action)?;
            }
            for t in &cxx.info.try_blocks {
                if t.catches_at(cur) && !t.catches_at(entry.to_state) {
                    self.eh_call(blk, "ra_eh_leave_catch", vec![]);
                }
            }
            cur = entry.to_state;
        }
        self.warn(format!("{:#x}: the unwind map does not lead from state {from} to state {to}", cxx.entry.begin));
        self.m.trap(blk, cxx.entry.begin, trap::INVALID);
        None
    }

    /// Runs the unwind action at `action` inline: a copy of the funclet, called as the handler
    /// calls it. The block the copy returns to, or `None` after a trap.
    fn run_cleanup(&mut self, block: BlockId, action: u64) -> Option<BlockId> {
        let exit = self.m.b.block(&format!("eh.cleaned.{action:x}"));
        self.enter_funclet(block, action);
        let Some(entry) = self.copy_funclet(action, FuncletKind::Cleanup { exit }) else {
            self.warn(format!("{action:#x}: an unwind action that is no funclet of the function"));
            self.m.trap(block, action, trap::INVALID);
            return None;
        };
        self.m.b.terminate(block, Terminator::Br(entry));
        Some(exit)
    }

    /// The block that enters the catch funclet of handler `h` of try block `t`, from code at the
    /// current funclet depth: the exception becomes the caught one, and the funclet runs inline.
    fn catch_entry(&mut self, t: usize, h: usize) -> BlockId {
        let depth = self.funclet.as_ref().map(|c| c.depth);
        if let Some(&b) = self.catch_entries.get(&(t, h, depth)) {
            return b;
        }
        let b = self.m.b.block(&format!("eh.enter{t}_{h}"));
        self.catch_entries.insert((t, h, depth), b);
        let handler = self.frame.cxx.as_ref().map(|c| c.info.try_blocks[t].handlers[h].handler).unwrap_or_default();
        self.eh_call(b, "ra_eh_enter_catch", vec![]);
        self.enter_funclet(b, handler);
        let continuations = self.continuations_of(handler);
        if let Some(entry) = self.copy_funclet(handler, FuncletKind::Catch { continuations }) {
            self.m.b.terminate(b, Terminator::Br(entry));
        } else {
            self.warn(format!("{handler:#x}: a catch handler that is no funclet of the function"));
            self.m.trap(b, handler, trap::INVALID);
        }
        b
    }

    /// What the handler's call of a funclet leaves for it: a return address on the stack, the
    /// establisher frame in `rdx`, nothing in `rcx`.
    fn enter_funclet(&mut self, block: BlockId, funclet: u64) {
        let frame = self.establisher(block);
        self.m.push(block, 0i64);
        self.m.write_gpr(block, R::RDX, frame);
        self.m.write_gpr(block, R::RCX, 0i64);
        let _ = funclet;
    }

    /// The continuations a catch funclet hands back: the function's blocks its `lea` name.
    fn continuations_of(&self, funclet: u64) -> Vec<(u64, BlockId)> {
        let Some((blocks, _)) = self.frame.funclets.get(&funclet) else { return Vec::new() };
        let mut out = Vec::new();
        for instr in blocks.iter().flat_map(|b| &b.instrs).filter(|i| i.mnemonic == "lea") {
            for operand in &instr.operands {
                if let ra_asm_arch::Operand::Mem(m) = operand
                    && let Some(va) = m.absolute
                    && let Some(&b) = self.function_blocks.get(&va)
                    && !out.iter().any(|(a, _)| *a == va)
                {
                    out.push((va, b));
                }
            }
        }
        out
    }

    /// Lifts a copy of the funclet at `entry` into fresh blocks and hands back the entry block.
    /// The copy's branches resolve among its own blocks; the function's blocks come back after.
    fn copy_funclet(&mut self, entry: u64, kind: FuncletKind) -> Option<BlockId> {
        let (blocks, depth) = self.frame.funclets.get(&entry).cloned()?;
        self.copies += 1;
        let copy = self.copies;
        let mut map = HashMap::new();
        for b in &blocks {
            map.insert(b.start, self.m.b.block(&format!("f{copy}_{:x}", b.start)));
        }
        let first = *map.get(&entry)?;
        let saved_blocks = std::mem::replace(&mut self.blocks, map);
        let saved_mode = self.funclet.replace(FuncletCopy { kind, depth });
        let saved_instrs = std::mem::take(&mut self.block_instrs);
        let saved_index = self.block_index;
        for b in &blocks {
            self.lift_block(b);
        }
        self.block_instrs = saved_instrs;
        self.block_index = saved_index;
        self.funclet = saved_mode;
        self.blocks = saved_blocks;
        Some(first)
    }

    /// The establisher frame: the stack pointer at the end of the prolog, which the unwind data's
    /// offsets count from. From the frame register when the function keeps one; else the stack
    /// pointer itself, above the frame of the funclet copy the code is in.
    fn establisher(&mut self, block: BlockId) -> ValueId {
        let ty = self.addr_ty();
        if let Some((reg, offset)) = self.frame.fp {
            let fp = self.m.read_gpr(block, gpr(reg));
            return self.m.b.bin(block, BinOp::Sub, ty, fp, i64::from(offset), "eh.frame");
        }
        let sp = self.m.read_sp(block);
        match &self.funclet {
            Some(copy) => self.m.b.bin(block, BinOp::Add, ty, sp, (copy.depth + 8) as i64, "eh.frame"),
            None => sp,
        }
    }

    /// Leaves the function with the exception in flight: the prolog undone as the unwind codes
    /// say, as the native unwinder would, then the return the caller's check follows.
    fn leave_function(&mut self, block: BlockId) {
        let address = self.frame.cxx.as_ref().map_or(0, |c| c.entry.begin);
        if !self.frame.known {
            self.warn(format!("{address:#x}: an exception passes a function without unwind information"));
            self.m.trap(block, address, trap::INVALID);
            return;
        }
        let ty = self.addr_ty();
        let mut sp = if self.frame.fp.is_some() { self.m.read_sp(block) } else { self.establisher(block) };
        for op in self.frame.ops.clone() {
            match op {
                UnwindOp::PushNonvol(reg) => {
                    let v = self.m.load(block, ty.clone(), sp, "eh.saved");
                    self.m.write_gpr(block, gpr(reg), v);
                    sp = self.m.b.bin(block, BinOp::Add, ty.clone(), sp, 8i64, "eh.sp");
                }
                UnwindOp::Alloc(n) => sp = self.m.b.bin(block, BinOp::Add, ty.clone(), sp, i64::from(n), "eh.sp"),
                UnwindOp::SetFpReg => {
                    let (reg, offset) = self.frame.fp.unwrap_or((5, 0));
                    let fp = self.m.read_gpr(block, gpr(reg));
                    sp = self.m.b.bin(block, BinOp::Sub, ty.clone(), fp, i64::from(offset), "eh.sp");
                }
                UnwindOp::SaveNonvol(reg, offset) => {
                    let at = self.m.b.bin(block, BinOp::Add, ty.clone(), sp, i64::from(offset), "eh.slot");
                    let v = self.m.load(block, ty.clone(), at, "eh.saved");
                    self.m.write_gpr(block, gpr(reg), v);
                }
                UnwindOp::SaveXmm128(reg, offset) => {
                    let at = self.m.b.bin(block, BinOp::Add, ty.clone(), sp, i64::from(offset), "eh.slot");
                    let v = self.m.load(block, Type::I128, at, "eh.saved");
                    self.m.write_xmm(block, xmm(reg), Type::I128, v, false);
                }
                UnwindOp::PushMachFrame(_) | UnwindOp::Epilog => {
                    self.warn(format!("{address:#x}: an exception passes a frame with {op:?}, which is not lifted yet"));
                    self.m.trap(block, address, trap::INVALID);
                    return;
                }
            }
        }
        self.m.write_sp(block, sp);
        self.m.pop(block, "ret.addr");
        self.m.b.terminate(block, Terminator::Ret(None));
    }

    /// The import a call instruction reaches, directly, through a thunk, through its slot, or
    /// through a register that holds the slot's value.
    pub(crate) fn called_import(&self, i: &Instruction, ip: u64) -> Option<String> {
        match i.op_kind(0) {
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => {
                let target = i.near_branch_target();
                self.lifted.thunks.get(&target).or_else(|| self.lifted.runtime.get(&target)).cloned()
            }
            OpKind::Memory if i.is_ip_rel_memory_operand() || (i.memory_base() == R::None && i.memory_index() == R::None) => {
                let slot = if i.is_ip_rel_memory_operand() { i.ip_rel_memory_address() } else { i.memory_displacement64() };
                self.image.import_at(slot).map(ra_asm_pe::Import::display_name)
            }
            _ => self.register_import_calls.get(&ip).cloned(),
        }
    }
}
