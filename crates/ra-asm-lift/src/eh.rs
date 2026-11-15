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
//!
//! The x86 frame handler's frames ([`ra_asm_analysis::eh32`]) take the same protocol with the
//! state the code keeps itself: a landing reads the try level from the function's registration,
//! addressed through the frame register the body uses (so that the raise sees the frame's slots
//! the body sees), checks that it names the handler, and goes on in that state's block.
//! The funclets run with `ebp` just past the registration, each in the state the handler would
//! store first: an action in the state its entry leaves to, a catch in the state past its try
//! block. A catch resumes with the `esp` saved under the registration when it began; a function
//! the exception leaves unlinks its registration and restores no register, as the x86 unwind
//! does. The thrown object's destructor runs when the end of its catch ends the exception, the
//! runtime saying which; a catch by value of an object with a copy constructor stops the run.
//!
//! A 32-bit `__try` leaves a scope table and a registration frame, which the analysis reads into
//! the ranges each scope protects ([`ra_asm_analysis::seh32`]). The lift maps them onto the same
//! protocol: an access to memory in a protected range asks `ra_eh_probe` first, which puts the
//! access violation in flight when the access would fault, and the function lands in the block of
//! the instruction. The landing does what the frame handler does: for each scope around the
//! instruction, innermost first, the `EXCEPTION_POINTERS` of `ra_eh_pointers` go below the
//! registration node, the filter runs as a copy with `ebp` at the node's end, and its verdict
//! picks the way on: the `__except` block (`ra_eh_handled`, the enclosing try level stored), the
//! faulting instruction again, or the next scope. An exception no scope of the function handles
//! stops the run at `ra_eh_unhandled`: the frames of its callers are not walked yet, nor a
//! `__finally` the exception passes, which traps.

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, CxxFrame32, Function, Scope, SehFrame};
use ra_asm_ir::{BinOp, BlockId, Callee, FnType, ICond, Operand, Terminator, Type, ValueId};
use ra_asm_pe::exception::{CxxFrame, ThrowInfo, UnwindOp, type_descriptor_name};
use ra_asm_pe::{FuncInfo, Image};
use ra_asm_x86::iced_x86::{Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register as R};

use crate::x86::{Lifter, trap};

/// The offset of the try level in an x86 registration.
const STATE: i64 = 8;

/// The general register an x86 C++ frame's body addresses its registration through.
fn base_register(cxx: &CxxFrame32) -> Option<R> {
    Some(match cxx.base {
        "eax" => R::EAX,
        "ecx" => R::ECX,
        "edx" => R::EDX,
        "ebx" => R::EBX,
        "ebp" => R::EBP,
        "esi" => R::ESI,
        "edi" => R::EDI,
        _ => return None,
    })
}

/// The offset from the registration of the `ebp` the x86 handler runs a funclet with.
const FUNCLET_FRAME: i64 = 12;

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
    /// The 32-bit registration frame and the scopes of its `__try`s, when the function has one.
    pub seh: Option<SehFrame>,
    /// The x86 frame handler's registration and tables, when the function registers them.
    pub cxx32: Option<CxxFrame32>,
    /// Why the function's x86 C++ frame was not read, when it registers one the analysis could
    /// not read: an exception that passes it stops there.
    pub unread32: Option<String>,
}

impl Frame {
    /// Nothing: a function of a program without exceptions.
    pub(crate) fn none() -> Self {
        Self { cxx: None, ops: Vec::new(), fp: None, known: false, funclets: HashMap::new(), seh: None, cxx32: None, unread32: None }
    }

    /// Takes the function's x86 C++ frame, with its unwind actions and catch handlers as
    /// funclets: blocks of the function, each up to its `ret`.
    pub(crate) fn add_cxx32(&mut self, analysis: &Analysis<'_>, f: &Function, local: impl Fn(&Function, u64) -> Vec<Block>) {
        self.unread32 = analysis.cxx_unread32.get(&f.entry).cloned();
        let Some(cxx) = analysis.cxx_frames32.get(&f.entry) else { return };
        let actions = cxx.info.unwind_map.iter().filter_map(|e| e.action);
        let handlers = cxx.info.try_blocks.iter().flat_map(|t| t.handlers.iter().map(|h| h.handler));
        for entry in actions.chain(handlers) {
            if f.block_at(entry).is_some() && !self.funclets.contains_key(&entry) {
                self.funclets.insert(entry, (local(f, entry), 0));
            }
        }
        self.cxx32 = Some(cxx.clone());
    }

    /// Takes the function's 32-bit registration frame, with each scope's filter as a funclet: a
    /// block of the function's own, or the function clang outlines it into.
    pub(crate) fn add_seh(&mut self, analysis: &Analysis<'_>, f: &Function, local: impl Fn(&Function, u64) -> Vec<Block>) {
        let Some(seh) = analysis.seh_frames.get(&f.entry) else { return };
        for filter in seh.scopes.iter().filter_map(|s| s.filter) {
            if f.block_at(filter).is_some() {
                self.funclets.insert(filter, (local(f, filter), 0));
            } else if let Some(outlined) = analysis.functions.get(&filter) {
                self.funclets.insert(filter, (outlined.blocks.clone(), 0));
            }
        }
        self.seh = Some(seh.clone());
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
    /// A catch body: its `ret` resumes the parent at the continuation in `rax`, one of these. At
    /// 32 bits with the registration and the `esp` saved under it when the catch began, which
    /// the x86 handler puts back over what the catch stored there and resumes with.
    Catch { continuations: Vec<(u64, BlockId)>, resume32: Option<(ValueId, ValueId)> },
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

/// How the instruction reaches its explicit memory operand, when it has one it reaches: `lea`
/// and the hinting `nop`s name an address and touch nothing.
fn explicit_access(i: &Instruction) -> Option<OpAccess> {
    (0..i.op_count()).find(|&n| i.op_kind(n) == OpKind::Memory)?;
    let mut factory = InstructionInfoFactory::new();
    let info = factory.info(i);
    let used = info
        .used_memory()
        .iter()
        .find(|m| m.base() == i.memory_base() && m.index() == i.memory_index() && m.displacement() == i.memory_displacement64());
    used.map(ra_asm_x86::iced_x86::UsedMemory::access).filter(|&access| access != OpAccess::NoMemAccess)
}

/// The scopes around `ip`, innermost first: the one no other around it encloses, then its
/// parents out to the outermost.
fn seh_chain(seh: &SehFrame, ip: u64) -> Result<Vec<Scope>, String> {
    let around: Vec<&Scope> = seh.scopes.iter().filter(|s| s.protects(ip)).collect();
    let inner =
        around.iter().find(|s| !around.iter().any(|t| t.parent == Some(s.index))).ok_or("no innermost scope protects the instruction")?;
    let mut chain = vec![(*inner).clone()];
    while let Some(parent) = chain.last().and_then(|s| s.parent) {
        let scope = seh.scopes.iter().find(|s| s.index == parent).ok_or_else(|| format!("the scope table has no level {parent}"))?;
        if chain.len() > seh.scopes.len() {
            return Err("the scope table's levels form a loop".to_owned());
        }
        chain.push(scope.clone());
    }
    Ok(chain)
}

impl Lifter<'_, '_> {
    /// Whether the block at `start` is a funclet's: lifted in copies from the landings, never in
    /// the function's own walk.
    pub(crate) fn is_funclet_block(&self, start: u64) -> bool {
        self.frame.funclets.values().any(|(blocks, _)| blocks.iter().any(|b| b.start == start))
    }

    /// Whether `address` is a continuation of the catch funclet being copied.
    pub(crate) fn is_continuation(&self, address: u64) -> bool {
        matches!(&self.funclet, Some(FuncletCopy { kind: FuncletKind::Catch { continuations, .. }, .. }) if continuations.iter().any(|(a, _)| *a == address))
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
        let (continuations, resume32) = match &self.funclet {
            None => {
                self.m.b.terminate(block, Terminator::Ret(None));
                return;
            }
            Some(FuncletCopy { kind: FuncletKind::Cleanup { exit }, .. }) => {
                self.m.b.terminate(block, Terminator::Br(*exit));
                return;
            }
            Some(FuncletCopy { kind: FuncletKind::Catch { continuations, resume32 }, .. }) => (continuations.clone(), *resume32),
        };
        // The catch is over: its exception goes, and the parent resumes where the funclet says.
        let address = self.m.b.address.unwrap_or(0);
        let target = self.m.read_gpr(block, if self.m.bits == 32 { R::EAX } else { R::RAX });
        let block = self.leave_catch(block, address);
        if let Some((reg, sp)) = resume32 {
            // As the x86 handler resumes: the saved `esp` back in its slot and in `esp`, and
            // `ebp` past the registration.
            let slot = self.m.b.bin(block, BinOp::Sub, Type::I32, reg, 4i64, "eh.esp.p");
            self.m.store(block, Type::I32, sp, slot);
            self.m.write_sp(block, sp);
            let frame = self.m.b.bin(block, BinOp::Add, Type::I32, reg, FUNCLET_FRAME, "eh.ebp");
            self.m.write_gpr(block, R::EBP, frame);
        }
        let bad = self.m.b.block("eh.resume.bad");
        self.m.trap(bad, address, trap::BAD_SWITCH);
        let cases = continuations.into_iter().map(|(a, b)| (i128::from(a), b)).collect();
        let ty = self.addr_ty();
        self.m.b.terminate(block, Terminator::Switch { ty, value: target.into(), default: bad, cases });
    }

    /// Leaves the catch being run: at 32 bits, when that ends the exception, its object's
    /// destructor runs on it, called as a `thiscall` with no argument. The block after.
    fn leave_catch(&mut self, block: BlockId, address: u64) -> BlockId {
        if self.m.bits != 32 {
            self.eh_call(block, "ra_eh_leave_catch", vec![]);
            return block;
        }
        let object = self.m.intrinsic(block, "ra_eh_ending_object", Type::I64, vec![], "eh.ending");
        let destructor = self.m.intrinsic(block, "ra_eh_ending_destructor", Type::I64, vec![], "eh.dtor");
        self.eh_call(block, "ra_eh_leave_catch", vec![]);
        let dies = self.m.is_true(block, Type::I64, destructor, "eh.dies");
        let destroy = self.m.b.block(&format!("eh.destroy.{address:x}"));
        let after = self.m.b.block(&format!("eh.destroyed.{address:x}"));
        self.m.b.terminate(block, Terminator::CondBr { cond: dies.into(), then: destroy, els: after });
        let this = self.m.trunc(destroy, Type::I64, object, Type::I32, "eh.this");
        self.m.write_gpr(destroy, R::ECX, this);
        self.m.push(destroy, 0i64);
        self.runtime_call(destroy, destructor);
        self.m.b.terminate(destroy, Terminator::Br(after));
        after
    }

    /// The registration the x86 frame handler would be called with: the function's own, as the
    /// landing being built took it from the frame register. Outside a landing, from the frame
    /// register here, or else the head of the chain at `fs:[0]`, which a function leaving with an
    /// exception in flight unlinks its own from.
    fn registration(&mut self, block: BlockId) -> ValueId {
        if let Some(reg) = self.registration32 {
            return reg;
        }
        if let Some(cxx) = self.frame.cxx32.clone()
            && let Some(reg) = self.own_registration(block, &cxx)
        {
            return reg;
        }
        let teb = self.m.intrinsic(block, "ra_teb", Type::I64, vec![], "teb");
        let teb = self.m.trunc(block, Type::I64, teb, Type::I32, "teb");
        self.m.load(block, Type::I32, teb, "eh.reg")
    }

    /// The function's registration, from the register its body addresses it through: an address
    /// the frame's slots are reached by, where the head of `fs:[0]` is a word loaded from memory.
    fn own_registration(&mut self, block: BlockId, cxx: &CxxFrame32) -> Option<ValueId> {
        let base = base_register(cxx)?;
        let base = self.m.read_gpr(block, base);
        Some(self.m.b.bin(block, BinOp::Add, Type::I32, base, cxx.registration, "eh.reg"))
    }

    /// At a landing, the frame register as the function's body has it, put back from the head of
    /// `fs:[0]`: a callee that left with an exception in flight ran no epilogue, so the register
    /// still holds the callee's frame, while the unwind unlinked the callee's registration and the
    /// head is this function's own, as the x86 handler finds it.
    fn restore_frame_register(&mut self, block: BlockId, cxx: &CxxFrame32) {
        let Some(base) = base_register(cxx) else { return };
        let teb = self.m.intrinsic(block, "ra_teb", Type::I64, vec![], "teb");
        let teb = self.m.trunc(block, Type::I64, teb, Type::I32, "teb");
        let head = self.m.load(block, Type::I32, teb, "eh.head");
        let frame = self.m.b.bin(block, BinOp::Sub, Type::I32, head, cxx.registration, "eh.frame");
        self.m.write_gpr(block, base, frame);
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
                // At 32 bits the runtime keeps the destructor and the copy constructors: it hands
                // the one back at the end of the catch, and stops at a catch by value of the other.
                let wide = self.m.bits == 64;
                let calls = wide && (info.destructor.is_some() || info.catchable.iter().any(|c| c.copy.is_some()));
                if calls || info.catchable.iter().any(|c| c.pdisp != -1) {
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
                let object = if wide {
                    self.m.read_gpr(block, R::RCX)
                } else {
                    // `_CxxThrowException(object, info)` is `stdcall`: the object is on top.
                    let sp = self.m.read_sp(block);
                    let object = self.m.load(block, Type::I32, sp, "eh.object");
                    self.m.zext(block, Type::I32, object, Type::I64, "eh.object")
                };
                self.eh_call(block, "ra_eh_throw", vec![object.into(), i64::from(size).into()]);
                if !wide && let Some(destructor) = info.destructor {
                    self.eh_call(block, "ra_eh_throw_destructor", vec![(destructor as i64).into()]);
                }
                for ((key, disp, size), c) in types.into_iter().zip(&info.catchable) {
                    self.eh_call(block, "ra_eh_throw_type", vec![(key as i64).into(), i64::from(disp).into(), i64::from(size).into()]);
                    if !wide && let Some(copy) = c.copy {
                        self.eh_call(block, "ra_eh_throw_copy", vec![(copy as i64).into()]);
                    }
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
        if self.m.bits == 32 {
            return self.throw_pushed();
        }
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

    /// [`Self::throw_before`] at 32 bits, where the `ThrowInfo` is the second push back from the
    /// call: `push offset info` after the object's, or `push 0` for a rethrow.
    fn throw_pushed(&self) -> Option<Throw> {
        // Back to the last call or the last other move of `esp`, past which a push is no argument.
        let moves_sp = |i: &&Instruction| {
            i.mnemonic() != Mnemonic::Push && i.op_count() > 0 && i.op_kind(0) == OpKind::Register && i.op_register(0) == R::ESP
        };
        let mut pushes = self.block_instrs[..self.block_index].iter().rev().take_while(|i| i.mnemonic() != Mnemonic::Call && !moves_sp(i));
        pushes.find(|i| i.mnemonic() == Mnemonic::Push)?;
        let info = pushes.find(|i| i.mnemonic() == Mnemonic::Push)?;
        match info.op_kind(0) {
            OpKind::Immediate8to32 | OpKind::Immediate32 => {
                let value = u64::from(info.immediate(0) as u32);
                Some(if value == 0 { Throw::Rethrow } else { Throw::Object(value) })
            }
            _ => None,
        }
    }

    /// The landing block for an exception in flight at `ip`: by the state of `ip` in the C++
    /// frame's tables, and by the funclet the code is a copy of, whose frame stands between the
    /// code and the function's.
    fn landing(&mut self, ip: u64) -> BlockId {
        if self.m.bits == 32 {
            return self.landing32();
        }
        let state = self.frame.cxx.as_ref().map_or(-1, |c| c.info.state_at(ip));
        self.state_landing(state)
    }

    /// The landing block of an exception in flight in state `state`.
    fn state_landing(&mut self, state: i32) -> BlockId {
        let depth = self.funclet.as_ref().map(|c| c.depth);
        if let Some(&b) = self.landings.get(&(state, depth)) {
            return b;
        }
        let b = self.m.b.block(&format!("eh.state{}", if state < 0 { "_none".to_owned() } else { state.to_string() }));
        self.landings.insert((state, depth), b);
        self.build_landing(b, state);
        b
    }

    /// The landing of an exception in flight in a 32-bit function: the state is the try level the
    /// code last stored in its registration, and the landing goes on in that state's block. The
    /// registration is the function's own, addressed through its frame register as the body
    /// addresses it, so the frame's slots stay the ones the body uses; it must name the handler.
    fn landing32(&mut self) -> BlockId {
        let depth = self.funclet.as_ref().map(|c| c.depth);
        if let Some(&b) = self.landings.get(&(i32::MIN, depth)) {
            return b;
        }
        let b = self.m.b.block("eh.landing");
        self.landings.insert((i32::MIN, depth), b);
        let address = self.m.b.address.unwrap_or(0);
        if let Some(reason) = self.frame.unread32.clone() {
            self.warn(format!("{address:#x}: an exception passes a C++ frame that was not read: {reason}"));
            self.m.trap(b, address, trap::INVALID);
            return b;
        }
        let Some(cxx) = self.frame.cxx32.clone() else {
            if self.frame.seh.is_some() {
                self.warn(format!("{address:#x}: a C++ exception passes a structured exception frame, which is not lifted yet"));
                self.m.trap(b, address, trap::INVALID);
            } else {
                // No frame of its own: x86 code keeps nothing for the unwind to restore.
                self.m.b.terminate(b, Terminator::Ret(None));
            }
            return b;
        };
        self.restore_frame_register(b, &cxx);
        let Some(reg) = self.own_registration(b, &cxx) else {
            self.warn(format!("{address:#x}: the registration is addressed through {}, which is no general register", cxx.base));
            self.m.trap(b, address, trap::INVALID);
            return b;
        };
        let at = self.m.b.bin(b, BinOp::Add, Type::I32, reg, 4i64, "eh.handler.p");
        let handler = self.m.load(b, Type::I32, at, "eh.handler");
        let ours = self.m.b.icmp(b, ICond::Eq, Type::I32, handler, cxx.handler as i64, "eh.ours");
        let dispatch = self.m.b.block("eh.dispatch");
        let foreign = self.m.b.block("eh.foreign");
        self.m.b.terminate(b, Terminator::CondBr { cond: ours.into(), then: dispatch, els: foreign });
        self.m.trap(foreign, cxx.function, trap::INVALID);
        let at = self.m.b.bin(dispatch, BinOp::Add, Type::I32, reg, STATE, "eh.state.p");
        let state = self.m.load(dispatch, Type::I32, at, "eh.state");
        let bad = self.m.b.block("eh.state.bad");
        self.m.trap(bad, cxx.function, trap::INVALID);
        let states = i32::try_from(cxx.info.unwind_map.len()).unwrap_or(i32::MAX);
        // The funclets the states' blocks run move the frame register; what they need of the
        // registration is this block's, and a landing within them takes its own.
        let outer = self.registration32.replace(reg);
        let cases = (-1..states).map(|s| (i128::from(s), self.state_landing(s))).collect();
        self.registration32 = outer;
        self.m.b.terminate(dispatch, Terminator::Switch { ty: Type::I32, value: state.into(), default: bad, cases });
        b
    }

    /// The tables of the function's C++ frame, x64's or x86's, with the function's address.
    fn tables(&self) -> Option<(FuncInfo, u64)> {
        match (&self.frame.cxx, &self.frame.cxx32) {
            (Some(c), _) => Some((c.info.clone(), c.entry.begin)),
            (None, Some(c)) => Some((c.info.clone(), c.function)),
            (None, None) => None,
        }
    }

    /// At 32 bits, stores `state` as the try level of the registration, as the frame handler
    /// does before it runs a funclet.
    fn set_state(&mut self, block: BlockId, state: i32) {
        if self.m.bits != 32 {
            return;
        }
        let reg = self.registration(block);
        let at = self.m.b.bin(block, BinOp::Add, Type::I32, reg, STATE, "eh.state.p");
        self.m.store(block, Type::I32, i64::from(state), at);
    }

    fn build_landing(&mut self, block: BlockId, state: i32) {
        let Some((info, begin)) = self.tables() else {
            self.leave_function(block);
            return;
        };
        let mut cur = block;
        // The try blocks that cover the state, innermost first.
        let mut tries: Vec<usize> = (0..info.try_blocks.len()).filter(|&t| info.try_blocks[t].covers(state)).collect();
        tries.sort_by_key(|&t| info.try_blocks[t].high - info.try_blocks[t].low);
        for t in tries {
            let try_block = &info.try_blocks[t];
            for (h, handler) in try_block.handlers.iter().enumerate() {
                let key = match handler.type_descriptor.map(|td| type_descriptor_name(self.image, td)) {
                    None => 0,
                    Some(Ok(name)) => type_key(&name),
                    Some(Err(e)) => {
                        self.warn(format!("{begin:#x}: {e:#}"));
                        self.m.trap(cur, begin, trap::INVALID);
                        return;
                    }
                };
                let slot: Operand = if handler.catch_object == 0 {
                    0i64.into()
                } else {
                    let frame = self.establisher(cur);
                    let ty = self.addr_ty();
                    let slot = self.m.b.bin(cur, BinOp::Add, ty.clone(), frame, i64::from(handler.catch_object), "eh.catch.obj");
                    if ty == Type::I64 { slot } else { self.m.zext(cur, ty, slot, Type::I64, "eh.catch.obj") }.into()
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
        let (info, begin) = self.tables()?;
        let mut cur = from;
        let mut blk = block;
        for _ in 0..=info.unwind_map.len() {
            if cur == to {
                self.set_state(blk, to);
                return Some(blk);
            }
            let Some(entry) = usize::try_from(cur).ok().and_then(|s| info.unwind_map.get(s)) else { break };
            if let Some(action) = entry.action {
                self.set_state(blk, entry.to_state);
                blk = self.run_cleanup(blk, action)?;
            }
            for t in &info.try_blocks {
                if t.catches_at(cur) && !t.catches_at(entry.to_state) {
                    blk = self.leave_catch(blk, begin);
                }
            }
            cur = entry.to_state;
        }
        self.warn(format!("{begin:#x}: the unwind map does not lead from state {from} to state {to}"));
        self.m.trap(blk, begin, trap::INVALID);
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
        let tables = self.tables();
        let handler = tables.as_ref().map(|(info, _)| info.try_blocks[t].handlers[h].handler).unwrap_or_default();
        self.eh_call(b, "ra_eh_enter_catch", vec![]);
        // The x86 handler runs a catch in the state past its try block's body.
        if let Some((info, _)) = &tables {
            self.set_state(b, info.try_blocks[t].high + 1);
        }
        let resume32 = (self.m.bits == 32).then(|| {
            let reg = self.registration(b);
            let slot = self.m.b.bin(b, BinOp::Sub, Type::I32, reg, 4i64, "eh.esp.p");
            (reg, self.m.load(b, Type::I32, slot, "eh.esp"))
        });
        self.enter_funclet(b, handler);
        let continuations = self.continuations_of(handler);
        if let Some(entry) = self.copy_funclet(handler, FuncletKind::Catch { continuations, resume32 }) {
            self.m.b.terminate(b, Terminator::Br(entry));
        } else {
            self.warn(format!("{handler:#x}: a catch handler that is no funclet of the function"));
            self.m.trap(b, handler, trap::INVALID);
        }
        b
    }

    /// What the handler's call of a funclet leaves for it: a return address on the stack, the
    /// establisher frame in `rdx`, nothing in `rcx`; at 32 bits the frame in `ebp`.
    fn enter_funclet(&mut self, block: BlockId, funclet: u64) {
        let frame = self.establisher(block);
        self.m.push(block, 0i64);
        if self.m.bits == 32 {
            self.m.write_gpr(block, R::EBP, frame);
        } else {
            self.m.write_gpr(block, R::RDX, frame);
            self.m.write_gpr(block, R::RCX, 0i64);
        }
        let _ = funclet;
    }

    /// The continuations a catch funclet hands back: the function's blocks its `lea` name.
    /// At 32 bits the `mov eax, offset` before its `ret`.
    fn continuations_of(&self, funclet: u64) -> Vec<(u64, BlockId)> {
        let Some((blocks, _)) = self.frame.funclets.get(&funclet) else { return Vec::new() };
        let mut out = Vec::new();
        let names = if self.m.bits == 32 { "mov" } else { "lea" };
        for instr in blocks.iter().flat_map(|b| &b.instrs).filter(|i| i.mnemonic == names) {
            for operand in &instr.operands {
                let named = match operand {
                    ra_asm_arch::Operand::Mem(m) if self.m.bits == 64 => m.absolute,
                    ra_asm_arch::Operand::Imm { value, .. } if self.m.bits == 32 => Some(u64::from(*value as u32)),
                    _ => None,
                };
                if let Some(va) = named
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
    pub(crate) fn copy_funclet(&mut self, entry: u64, kind: FuncletKind) -> Option<BlockId> {
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
    /// At 32 bits, the `ebp` the x86 handler runs a funclet with, just past the registration.
    fn establisher(&mut self, block: BlockId) -> ValueId {
        let ty = self.addr_ty();
        if self.m.bits == 32 {
            let reg = self.registration(block);
            return self.m.b.bin(block, BinOp::Add, ty, reg, FUNCLET_FRAME, "eh.frame");
        }
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
    /// At 32 bits, the registration unlinked: the x86 unwind restores no register, and the code
    /// that lands sets the ones it needs from its own registration.
    fn leave_function(&mut self, block: BlockId) {
        if self.m.bits == 32 {
            if self.frame.cxx32.is_some() {
                let reg = self.registration(block);
                let next = self.m.load(block, Type::I32, reg, "eh.next");
                let teb = self.m.intrinsic(block, "ra_teb", Type::I64, vec![], "teb");
                let teb = self.m.trunc(block, Type::I64, teb, Type::I32, "teb");
                self.m.store(block, Type::I32, next, teb);
            }
            self.m.b.terminate(block, Terminator::Ret(None));
            return;
        }
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

    /// Before an access to memory in a range a 32-bit `__try` protects: the probe that puts the
    /// fault in flight when the access would fault, and the landing it goes to then. The
    /// instruction itself goes on in the block handed back.
    pub(crate) fn seh_probe(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        let ip = i.ip();
        let protected = self.funclet.is_none() && self.frame.seh.as_ref().is_some_and(|s| s.scopes.iter().any(|scope| scope.protects(ip)));
        if !protected {
            return block;
        }
        let Some(access) = explicit_access(i) else { return block };
        let probe = self.m.b.block(&format!("eh.probe.{ip:x}"));
        self.m.b.terminate(block, Terminator::Br(probe));
        let ty = self.addr_ty();
        let address = self.mem_address(probe, i);
        let address = self.m.zext(probe, ty, address, Type::I64, "eh.at");
        let size = i64::try_from(i.memory_size().size()).unwrap_or(0);
        let write = i64::from(matches!(access, OpAccess::Write | OpAccess::CondWrite));
        let args = vec![(Type::I64, address.into()), (Type::I64, size.into()), (Type::I64, write.into()), (Type::I64, (ip as i64).into())];
        let fault = self.m.intrinsic(probe, "ra_eh_probe", Type::I64, args, "eh.fault");
        let faults = self.m.is_true(probe, Type::I64, fault, "eh.faults");
        let landing = self.seh_landing(ip, probe);
        let next = self.m.b.block(&format!("eh.access.{ip:x}"));
        self.m.b.terminate(probe, Terminator::CondBr { cond: faults.into(), then: landing, els: next });
        next
    }

    /// The landing of an exception at `ip` in a protected range: the frame handler's walk of the
    /// scopes around it, with `retry` the block that runs the faulting instruction again.
    fn seh_landing(&mut self, ip: u64, retry: BlockId) -> BlockId {
        let b = self.m.b.block(&format!("eh.seh.{ip:x}"));
        let Some(seh) = self.frame.seh.clone() else {
            self.m.trap(b, ip, trap::INVALID);
            return b;
        };
        let chain = match seh_chain(&seh, ip) {
            Ok(chain) => chain,
            Err(e) => {
                self.warn(format!("{ip:#x}: {e}"));
                self.m.trap(b, ip, trap::INVALID);
                return b;
            }
        };
        // The registration node: the next link, the handler, the table and the try level, with
        // the exception's pointers in the slot below it.
        let ty = self.addr_ty();
        let ebp = self.m.read_gpr(b, R::EBP);
        let node = self.m.b.bin(b, BinOp::Add, ty.clone(), ebp, seh.level_slot - 12, "eh.node");
        let pointers_at = self.m.b.bin(b, BinOp::Sub, ty.clone(), node, 4i64, "eh.pointers.at");
        let level_at = self.m.b.bin(b, BinOp::Add, ty.clone(), ebp, seh.level_slot, "eh.level.at");
        let handler_frame = self.m.b.bin(b, BinOp::Add, ty.clone(), node, 16i64, "eh.handler.frame");
        let mut cur = b;
        for scope in chain {
            let Some(filter) = scope.filter else {
                self.warn(format!("{ip:#x}: an exception passes the __finally at {:#x}, which is not lifted yet", scope.handler));
                self.m.trap(cur, ip, trap::INVALID);
                return b;
            };
            let pointers = self.m.intrinsic(cur, "ra_eh_pointers", Type::I64, vec![], "eh.pointers");
            let pointers = self.m.trunc(cur, Type::I64, pointers, ty.clone(), "eh.pointers");
            self.m.store(cur, ty.clone(), pointers, pointers_at);
            // The filter runs as the frame handler calls it: `ebp` at the node's end, a return
            // address on the stack, its verdict in `eax`.
            self.m.write_gpr(cur, R::EBP, handler_frame);
            self.m.push(cur, 0i64);
            let filtered = self.m.b.block(&format!("eh.filtered.{ip:x}.{}", scope.index));
            let Some(entry) = self.copy_funclet(filter, FuncletKind::Cleanup { exit: filtered }) else {
                self.warn(format!("{ip:#x}: the filter at {filter:#x} is no code the lift has"));
                self.m.trap(cur, ip, trap::INVALID);
                return b;
            };
            self.m.b.terminate(cur, Terminator::Br(entry));
            self.m.write_gpr(filtered, R::EBP, ebp);
            let verdict = self.m.read_gpr(filtered, R::EAX);
            let execute = self.m.b.icmp(filtered, ICond::Sgt, Type::I32, verdict, 0i64, "eh.execute");
            let handled = self.m.b.block(&format!("eh.except.{ip:x}.{}", scope.index));
            let other = self.m.b.block(&format!("eh.verdict.{ip:x}.{}", scope.index));
            self.m.b.terminate(filtered, Terminator::CondBr { cond: execute.into(), then: handled, els: other });
            // EXCEPTION_EXECUTE_HANDLER: the `__except` block, at the enclosing try level.
            self.eh_call(handled, "ra_eh_handled", vec![]);
            let enclosing = scope.parent.unwrap_or_else(|| seh.layout.outermost());
            self.m.store(handled, Type::I32, i64::from(enclosing as i32), level_at);
            self.m.write_gpr(handled, R::EBP, handler_frame);
            if let Some(&h) = self.function_blocks.get(&scope.handler) {
                self.m.b.terminate(handled, Terminator::Br(h));
            } else {
                self.warn(format!("{ip:#x}: the __except block at {:#x} is no block of the function", scope.handler));
                self.m.trap(handled, scope.handler, trap::INVALID);
            }
            // EXCEPTION_CONTINUE_EXECUTION runs the instruction again; EXCEPTION_CONTINUE_SEARCH
            // asks the next scope out.
            let again = self.m.b.icmp(other, ICond::Slt, Type::I32, verdict, 0i64, "eh.continue");
            let resume = self.m.b.block(&format!("eh.resume.{ip:x}.{}", scope.index));
            let next = self.m.b.block(&format!("eh.search.{ip:x}.{}", scope.index));
            self.m.b.terminate(other, Terminator::CondBr { cond: again.into(), then: resume, els: next });
            self.eh_call(resume, "ra_eh_handled", vec![]);
            self.m.b.terminate(resume, Terminator::Br(retry));
            cur = next;
        }
        self.eh_call(cur, "ra_eh_unhandled", vec![(ip as i64).into()]);
        self.m.b.terminate(cur, Terminator::Unreachable);
        b
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
