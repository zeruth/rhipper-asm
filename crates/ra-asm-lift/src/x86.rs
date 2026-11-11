//! The x86 semantics: one iced instruction becomes IR over the [`Machine`] state.
//!
//! Each analysis block becomes an IR block. An instruction that needs a loop or a branch of its own
//! (a `rep` string operation, a division) adds blocks and continues in the last one. The block's
//! terminator comes from the analysis: the condition of a `jcc` or the target of an indirect jump
//! is computed by the instruction and consumed when the block ends.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, Function, Terminator as T};
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FCond, FnType, FunctionBuilder, ICond, Inst, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;
use ra_asm_x86::X86;
use ra_asm_x86::iced_x86::{ConditionCode, Instruction, Mnemonic, OpKind, Register as R};

use crate::state::{Flag, Machine};
use crate::{Lifted, function_name};

/// The trap codes the runtime prints.
pub mod trap {
    pub const INT3: u64 = 1;
    pub const UD2: u64 = 2;
    pub const HLT: u64 = 3;
    pub const DIVIDE: u64 = 4;
    pub const DIVIDE_WIDE: u64 = 5;
    pub const FASTFAIL: u64 = 6;
    pub const BAD_SWITCH: u64 = 7;
    pub const INVALID: u64 = 8;
    pub const INT: u64 = 9;
}

/// Lifts one function.
pub fn lift_function(image: &Image, analysis: &Analysis<'_>, lifted: &mut Lifted, f: &Function) -> ra_asm_ir::Function {
    let name = function_name(f.entry);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_address(f.entry);
    if f.noreturn {
        b.add_attr("noreturn");
    }
    let state = b.param(0);
    let bits = if image.is_64() { 64 } else { 32 };
    let mut blocks = HashMap::new();
    blocks.insert(f.entry, b.block(&format!("b_{:x}", f.entry)));
    for blk in &f.blocks {
        if blk.start != f.entry {
            blocks.insert(blk.start, b.block(&format!("b_{:x}", blk.start)));
        }
    }
    let arch = X86::for_pointer_size(image.pointer_size());
    let _ = analysis;
    let mut lifter = Lifter {
        m: Machine { b: &mut b, state, bits },
        image,
        lifted,
        arch,
        blocks,
        register_import_calls: &f.register_import_calls,
        cond: None,
        jump_target: None,
        switch_index: None,
        switch_load_at: None,
    };
    for blk in &f.blocks {
        lifter.lift_block(blk);
    }
    let mut built = b.finish();
    // A function that never returns (every path ends in a call to one that never returns) has
    // no `ret` to come back through: a tail call to a no-return import ends in `unreachable`.
    if f.noreturn {
        for block in &mut built.blocks {
            if let Terminator::Ret(_) = block.terminator {
                block.terminator = Terminator::Unreachable;
            }
        }
    }
    built
}

pub(crate) struct Lifter<'a, 'b> {
    pub(crate) m: Machine<'b>,
    pub(crate) image: &'a Image,
    pub(crate) lifted: &'a mut Lifted,
    arch: X86,
    blocks: HashMap<u64, BlockId>,
    /// The calls through a register that holds an import, by address, from the analysis.
    register_import_calls: &'a HashMap<u64, String>,
    /// The condition of the conditional jump that ends the current block.
    cond: Option<ValueId>,
    /// The target of the indirect jump that ends the current block.
    jump_target: Option<ValueId>,
    /// The index of the switch that ends the current block, read before the table entry is.
    switch_index: Option<ValueId>,
    /// The instruction that reads the table entry, and the register that holds the index there.
    switch_load_at: Option<(u64, ra_asm_arch::Register)>,
}

impl Lifter<'_, '_> {
    pub(crate) fn warn(&mut self, text: String) {
        self.lifted.warnings.push(text);
    }

    pub(crate) fn addr_ty(&self) -> Type {
        self.m.address_type()
    }

    // region Blocks

    fn lift_block(&mut self, blk: &Block) {
        let mut block = self.blocks[&blk.start];
        self.m.b.set_block_address(block, blk.start);
        self.cond = None;
        self.jump_target = None;
        self.switch_index = None;
        self.switch_load_at = match &blk.terminator {
            T::Switch { index, load_at, .. } => Some((*load_at, *index)),
            _ => None,
        };
        for i in &blk.instrs {
            let Ok(bytes) = self.image.read(i.address, i.len as usize) else { break };
            let Some(d) = self.arch.decode_full(bytes, i.address) else { break };
            self.m.b.address = Some(i.address);
            if self.m.b.has_terminator(block) {
                break;
            }
            // The switch index, read before the instruction that turns it into a table entry.
            if let Some((at, index)) = self.switch_load_at
                && at == i.address
                && let Ok(reg) = R::try_from(index.id as usize)
            {
                let v = self.m.read_gpr(block, reg);
                let wide = self.m.zext(block, Type::Int(index.bits), v, Type::I64, "switch.index");
                self.switch_index = Some(wide);
            }
            block = self.lift_instr(block, &d.iced);
        }
        if self.m.b.has_terminator(block) {
            return;
        }
        let last = blk.instrs.last().map_or(blk.start, |i| i.address);
        self.m.b.address = Some(last);
        match &blk.terminator {
            T::FallThrough(t) | T::Jump(t) => self.branch(block, *t),
            T::CondJump { taken, fallthrough } => {
                let Some(cond) = self.cond else {
                    self.warn(format!("{last:#x}: a conditional jump with no condition"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                if let (Some(&then), Some(&els)) = (self.blocks.get(taken), self.blocks.get(fallthrough)) {
                    self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then, els });
                } else {
                    // One side leaves the function: a conditional tail call. Split it.
                    let then_block = self.target_block(*taken);
                    let els_block = self.target_block(*fallthrough);
                    self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then: then_block, els: els_block });
                }
            }
            T::Switch { targets, cases: index_cases, .. } => {
                let default = self.m.b.block("switch.bad");
                self.m.trap(default, last, trap::BAD_SWITCH);
                // By the index when it was read, so the table itself is never needed; else by the
                // computed target, which only the mapped image makes right.
                if let Some(index) = self.switch_index {
                    let cases = index_cases.iter().filter_map(|(n, t)| self.blocks.get(t).map(|b| (i128::from(*n), *b))).collect();
                    self.m.b.terminate(block, Terminator::Switch { ty: Type::I64, value: index.into(), default, cases });
                    return;
                }
                let Some(target) = self.jump_target else {
                    self.warn(format!("{last:#x}: a switch with no computed target"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                if self.lifted.standalone {
                    self.warn(format!("{last:#x}: a switch by its computed target reads the table of the image"));
                }
                let cases = targets.iter().filter_map(|t| self.blocks.get(t).map(|b| (i128::from(*t), *b))).collect();
                self.m.b.terminate(block, Terminator::Switch { ty: self.addr_ty(), value: target.into(), default, cases });
            }
            T::IndirectJump => {
                let Some(target) = self.jump_target else {
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                // A jump the analysis did not resolve: most often a tail call through a pointer.
                let wide = self.m.zext(block, self.addr_ty(), target, Type::I64, "target");
                self.runtime_call(block, wide);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::TailCall(t) => {
                self.call_lifted(block, *t);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::ImportTailCall(import) => {
                // The caller's return address is on the stack; the import sees the arguments past it.
                self.import_call(block, import, true);
                self.m.pop(block, "ret.addr");
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::NoReturnCall(_) | T::NoReturnImportCall(_) => self.m.b.terminate(block, Terminator::Unreachable),
            T::Return => self.m.b.terminate(block, Terminator::Ret(None)),
            T::Trap => self.m.trap(block, last, trap::INT3),
            T::Halt => self.m.trap(block, last, trap::HLT),
            T::Invalid => self.m.trap(block, last, trap::INVALID),
        }
    }

    fn branch(&mut self, block: BlockId, target: u64) {
        let to = self.target_block(target);
        self.m.b.terminate(block, Terminator::Br(to));
    }

    /// The block for a target inside the function, or a new block that tail-calls it.
    fn target_block(&mut self, target: u64) -> BlockId {
        if let Some(&b) = self.blocks.get(&target) {
            return b;
        }
        let b = self.m.b.block(&format!("tail_{target:x}"));
        if self.lifted.functions.contains_key(&target) {
            self.call_lifted(b, target);
            self.m.b.terminate(b, Terminator::Ret(None));
        } else {
            self.m.trap(b, target, trap::INVALID);
        }
        b
    }

    // endregion

    // region Calls

    /// `call F_target(state)`: the lifted callee pops the return address its caller pushed.
    fn call_lifted(&mut self, block: BlockId, target: u64) {
        let name = function_name(target);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// `call @ra_call(state, target)`: the runtime finds the lifted function or calls native code.
    fn runtime_call(&mut self, block: BlockId, target: ValueId) {
        self.m.b.call(
            block,
            FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
            Callee::Direct("ra_call".into()),
            vec![(Type::Ptr, self.m.state.into()), (Type::I64, target.into())],
            "",
        );
    }

    /// Calls an import through its wrapper, `imp_<name>(ptr %state)` (or `imp_<name>.tail` when
    /// the caller's return address is still on the stack). The wrapper reads the argument
    /// registers and the stack slots, calls the import, and stores the result to `rax`; see
    /// [`crate::import_wrapper`]. A wrapper keeps the register reads out of the calling function,
    /// so the signature inference can model the import by its prototype. In 32 bits the arguments
    /// start at the stack pointer (past the return address for a tail call), and a `stdcall`
    /// import pops them: the caller's code expects that, so the lift moves `esp` by the import's
    /// argument bytes after the call.
    fn import_call(&mut self, block: BlockId, import: &str, tail: bool) {
        let mut symbol = import_symbol(import);
        // A C runtime import that walks or hands back a table of the program's pointers goes to
        // the runtime's shim, which keeps the program's pointer width.
        if self.m.bits == 32
            && let Some(shim) = crate::raise::prototypes::x86_shim(&symbol)
        {
            symbol = shim.to_owned();
        }
        self.lifted.imports.insert(symbol.clone());
        let stack_offset = match (self.m.bits, tail) {
            (64, false) => 0x20,
            (64, true) => 0x28,
            (_, false) => 0,
            (_, true) => 4,
        };
        let wrapper = crate::import_wrapper_name(&symbol, tail);
        self.lifted.wrappers.insert((symbol.clone(), stack_offset, tail));
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(wrapper), vec![(Type::Ptr, self.m.state.into())], "");
        if self.m.bits == 32 {
            let dll = self.image.imports.iter().find(|i| i.display_name() == import).map(|i| i.dll.clone()).unwrap_or_default();
            let known = crate::raise::prototypes::stdcall_pops(&dll, &symbol);
            if known.is_none() {
                self.warn(format!("import {import}: an unknown arity; the call is taken as cdecl, with nothing popped"));
            }
            let pops = known.unwrap_or(0);
            if pops > 0 {
                let sp = self.m.read_sp(block);
                let adjusted = self.m.b.bin(block, BinOp::Add, Type::I32, sp, pops as i64, "sp");
                self.m.write_sp(block, adjusted);
            }
        }
    }

    // endregion

    // region Operands

    /// The width in bits of operand `n`.
    pub(crate) fn op_bits(&self, i: &Instruction, n: u32) -> u16 {
        match i.op_kind(n) {
            OpKind::Register => (i.op_register(n).size() * 8) as u16,
            OpKind::Memory => {
                let bytes = i.memory_size().size();
                if bytes == 0 { self.m.bits as u16 } else { (bytes * 8) as u16 }
            }
            OpKind::Immediate8 | OpKind::Immediate8_2nd => 8,
            OpKind::Immediate16 | OpKind::Immediate8to16 | OpKind::NearBranch16 => 16,
            OpKind::Immediate32 | OpKind::Immediate8to32 | OpKind::NearBranch32 => 32,
            OpKind::Immediate64 | OpKind::Immediate8to64 | OpKind::Immediate32to64 | OpKind::NearBranch64 => 64,
            _ => self.m.bits as u16,
        }
    }

    fn immediate(i: &Instruction, n: u32) -> i64 {
        match i.op_kind(n) {
            OpKind::Immediate8 => i64::from(i.immediate8()),
            OpKind::Immediate8_2nd => i64::from(i.immediate8_2nd()),
            OpKind::Immediate16 => i64::from(i.immediate16()),
            OpKind::Immediate32 => i64::from(i.immediate32()),
            OpKind::Immediate64 => i.immediate64() as i64,
            OpKind::Immediate8to16 => i64::from(i.immediate8to16()),
            OpKind::Immediate8to32 => i64::from(i.immediate8to32()),
            OpKind::Immediate8to64 => i.immediate8to64(),
            OpKind::Immediate32to64 => i.immediate32to64(),
            _ => 0,
        }
    }

    /// A constant address as an operand: in standalone mode, the address of the byte of the data
    /// global or the function it falls in; otherwise, the number.
    fn address_operand(&self, address: u64) -> Operand {
        if self.lifted.standalone
            && let Some(op) = self.lifted.address_of(address)
        {
            return op;
        }
        Operand::Int(i128::from(address))
    }

    /// The constant memory address of an operand, when it has one: RIP-relative, or absolute.
    fn absolute_address(i: &Instruction) -> Option<u64> {
        if i.is_ip_rel_memory_operand() {
            Some(i.ip_rel_memory_address())
        } else if i.memory_base() == R::None && i.memory_index() == R::None {
            Some(i.memory_displacement64())
        } else {
            None
        }
    }

    /// The address of the memory operand, at the address width. An `fs:` or `gs:` access lands in
    /// the thread environment block the runtime keeps.
    pub(crate) fn mem_address(&mut self, block: BlockId, i: &Instruction) -> ValueId {
        let ty = self.addr_ty();
        let segment = i.segment_prefix();
        if segment == R::FS || segment == R::GS {
            let flat = self.flat_address(block, i);
            let teb = Operand::Address { global: crate::state::TEB.to_owned(), offset: 0 };
            return self.m.b.bin(block, BinOp::Add, ty, flat, teb, "teb.addr");
        }
        self.flat_address(block, i)
    }

    /// The bytes of the immediates at the end of the instruction's encoding.
    fn immediate_bytes(i: &Instruction) -> u64 {
        (0..i.op_count())
            .map(|n| match i.op_kind(n) {
                OpKind::Immediate8 | OpKind::Immediate8_2nd | OpKind::Immediate8to16 | OpKind::Immediate8to32 | OpKind::Immediate8to64 => 1,
                OpKind::Immediate16 => 2,
                OpKind::Immediate32 | OpKind::Immediate32to64 => 4,
                OpKind::Immediate64 => 8,
                _ => 0,
            })
            .sum()
    }

    /// Whether the instruction's immediate is an address of the image: a base relocation names it,
    /// or, in an image whose relocations were stripped, it falls in a section (the headers do
    /// not count: `0x400000` is a size more often than an address).
    fn immediate_is_relocated(&self, i: &Instruction, value: u64) -> bool {
        let bytes = Self::immediate_bytes(i);
        if bytes < 4 {
            return false;
        }
        if self.lifted.relocated.is_empty() {
            self.image.section_of(value).is_some()
        } else {
            self.lifted.relocated.contains(&(i.next_ip() - bytes))
        }
    }

    /// Whether the instruction's displacement is an address of the image, by the same evidence. A
    /// displacement is an address by nature, so one in the headers counts too (the startup code
    /// reads `__ImageBase` for the PE header).
    fn displacement_is_relocated(&self, i: &Instruction, value: u64) -> bool {
        let disp = u64::from(i.memory_displ_size());
        if disp < 4 {
            return false;
        }
        if self.lifted.relocated.is_empty() {
            self.image.is_mapped(value)
        } else {
            self.lifted.relocated.contains(&(i.next_ip() - Self::immediate_bytes(i) - disp))
        }
    }

    /// The address of the memory operand without its segment. In standalone mode a relocated
    /// displacement is an address of the image, the base of an indexed access into the data.
    fn flat_address(&mut self, block: BlockId, i: &Instruction) -> ValueId {
        let ty = self.addr_ty();
        if let Some(address) = Self::absolute_address(i) {
            let op = self.address_operand(address);
            return self.m.value_of(block, ty, op);
        }
        let disp = i.memory_displacement64() as i64;
        let disp_address = if self.m.bits == 32 { disp as u64 & 0xffff_ffff } else { disp as u64 };
        let relocated_base = self.lifted.standalone && disp != 0 && self.displacement_is_relocated(i, disp_address);
        let mut acc: Option<ValueId> = None;
        if relocated_base {
            let op = self.address_operand(disp_address);
            acc = Some(self.m.value_of(block, ty.clone(), op));
        }
        let base = i.memory_base();
        if base != R::None {
            let v = self.m.read_gpr(block, base);
            let v = self.m.zext(block, Type::Int((base.size() * 8) as u16), v, ty.clone(), "base");
            acc = Some(v);
        }
        let index = i.memory_index();
        if index != R::None {
            let v = self.m.read_gpr(block, index);
            let v = self.m.zext(block, Type::Int((index.size() * 8) as u16), v, ty.clone(), "index");
            let scale = i.memory_index_scale();
            let scaled = if scale > 1 { self.m.b.bin(block, BinOp::Mul, ty.clone(), v, i64::from(scale), "scaled") } else { v };
            acc = Some(match acc {
                Some(a) => self.m.b.bin(block, BinOp::Add, ty.clone(), a, scaled, "addr"),
                None => scaled,
            });
        }
        match acc {
            Some(a) if disp != 0 && !relocated_base => self.m.b.bin(block, BinOp::Add, ty, a, disp, "addr"),
            Some(a) => a,
            None => self.m.value_of(block, ty, disp),
        }
    }

    /// The integer value of operand `n`, at its own width.
    pub(crate) fn read(&mut self, block: BlockId, i: &Instruction, n: u32) -> ValueId {
        let bits = self.op_bits(i, n);
        match i.op_kind(n) {
            OpKind::Register => {
                let r = i.op_register(n);
                if r.is_gpr() {
                    self.m.read_gpr(block, r)
                } else if r.is_xmm() {
                    self.m.read_xmm(block, r, Type::I128)
                } else if r.is_segment_register() {
                    self.m.value_of(block, Type::I16, 0i64)
                } else {
                    self.warn(format!("{:#x}: a read of {:?} is not lifted", i.ip(), r));
                    self.m.value_of(block, Type::Int(bits), 0i64)
                }
            }
            OpKind::Memory => {
                // A read of an import slot in standalone mode is the import's own address.
                if self.lifted.standalone
                    && bits == 64
                    && let Some(address) = Self::absolute_address(i)
                    && let Some(import) = self.image.import_at(address)
                {
                    let symbol = import_symbol(&import.display_name());
                    self.lifted.imports.insert(symbol.clone());
                    return self.m.value_of(block, Type::I64, Operand::Address { global: symbol, offset: 0 });
                }
                let addr = self.mem_address(block, i);
                self.m.load(block, Type::Int(bits), addr, "m")
            }
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => {
                self.m.value_of(block, Type::Int(bits), i.near_branch_target() as i64)
            }
            _ => {
                let v = Self::immediate(i, n);
                // `push offset string`, `mov eax, offset table`: in standalone mode the address of
                // the data global, which a base relocation marks.
                let value = v as u64 & if bits == 32 { 0xffff_ffff } else { u64::MAX };
                if self.lifted.standalone && bits >= 32 && self.immediate_is_relocated(i, value) {
                    let op = self.address_operand(value);
                    return self.m.value_of(block, Type::Int(bits), op);
                }
                self.m.value_of(block, Type::Int(bits), v)
            }
        }
    }

    /// Writes `value`, of the operand's width, to operand `n`.
    pub(crate) fn write(&mut self, block: BlockId, i: &Instruction, n: u32, value: ValueId) {
        match i.op_kind(n) {
            OpKind::Register => {
                let r = i.op_register(n);
                if r.is_gpr() {
                    self.m.write_gpr(block, r, value);
                } else if r.is_xmm() {
                    self.m.write_xmm(block, r, Type::I128, value, true);
                } else {
                    self.warn(format!("{:#x}: a write of {:?} is not lifted", i.ip(), r));
                }
            }
            OpKind::Memory => {
                let bits = self.op_bits(i, n);
                let addr = self.mem_address(block, i);
                self.m.store(block, Type::Int(bits), value, addr);
            }
            _ => self.warn(format!("{:#x}: a write to an immediate", i.ip())),
        }
    }

    // endregion

    // region Conditions

    pub(crate) fn condition(&mut self, block: BlockId, cc: ConditionCode) -> ValueId {
        let flag = |s: &mut Self, f: Flag| s.m.get_flag(block, f);
        let not = |s: &mut Self, v: ValueId| s.m.b.bin(block, BinOp::Xor, Type::I1, v, 1i64, "not");
        match cc {
            ConditionCode::None => self.m.value_of(block, Type::I1, 1i64),
            ConditionCode::o => flag(self, Flag::Of),
            ConditionCode::no => {
                let v = flag(self, Flag::Of);
                not(self, v)
            }
            ConditionCode::b => flag(self, Flag::Cf),
            ConditionCode::ae => {
                let v = flag(self, Flag::Cf);
                not(self, v)
            }
            ConditionCode::e => flag(self, Flag::Zf),
            ConditionCode::ne => {
                let v = flag(self, Flag::Zf);
                not(self, v)
            }
            ConditionCode::be => {
                let c = flag(self, Flag::Cf);
                let z = flag(self, Flag::Zf);
                self.m.b.bin(block, BinOp::Or, Type::I1, c, z, "be")
            }
            ConditionCode::a => {
                let c = flag(self, Flag::Cf);
                let z = flag(self, Flag::Zf);
                let be = self.m.b.bin(block, BinOp::Or, Type::I1, c, z, "be");
                not(self, be)
            }
            ConditionCode::s => flag(self, Flag::Sf),
            ConditionCode::ns => {
                let v = flag(self, Flag::Sf);
                not(self, v)
            }
            ConditionCode::p => flag(self, Flag::Pf),
            ConditionCode::np => {
                let v = flag(self, Flag::Pf);
                not(self, v)
            }
            ConditionCode::l => {
                let s = flag(self, Flag::Sf);
                let o = flag(self, Flag::Of);
                self.m.b.bin(block, BinOp::Xor, Type::I1, s, o, "l")
            }
            ConditionCode::ge => {
                let s = flag(self, Flag::Sf);
                let o = flag(self, Flag::Of);
                let l = self.m.b.bin(block, BinOp::Xor, Type::I1, s, o, "l");
                not(self, l)
            }
            ConditionCode::le => {
                let s = flag(self, Flag::Sf);
                let o = flag(self, Flag::Of);
                let l = self.m.b.bin(block, BinOp::Xor, Type::I1, s, o, "l");
                let z = flag(self, Flag::Zf);
                self.m.b.bin(block, BinOp::Or, Type::I1, l, z, "le")
            }
            ConditionCode::g => {
                let s = flag(self, Flag::Sf);
                let o = flag(self, Flag::Of);
                let l = self.m.b.bin(block, BinOp::Xor, Type::I1, s, o, "l");
                let z = flag(self, Flag::Zf);
                let le = self.m.b.bin(block, BinOp::Or, Type::I1, l, z, "le");
                not(self, le)
            }
        }
    }

    // endregion

    // region Arithmetic helpers

    /// `lhs + rhs (+ carry)` with every flag.
    fn add(&mut self, block: BlockId, ty: Type, lhs: ValueId, rhs: ValueId, carry: Option<ValueId>) -> ValueId {
        let sum = self.m.b.bin(block, BinOp::Add, ty.clone(), lhs, rhs, "sum");
        let (result, cf) = match carry {
            None => {
                let cf = self.m.b.icmp(block, ICond::Ult, ty.clone(), sum, lhs, "cf");
                (sum, cf)
            }
            Some(c) => {
                let c_wide = self.m.zext(block, Type::I1, c, ty.clone(), "c");
                let result = self.m.b.bin(block, BinOp::Add, ty.clone(), sum, c_wide, "sum.c");
                let c1 = self.m.b.icmp(block, ICond::Ult, ty.clone(), sum, lhs, "cf.a");
                let c2 = self.m.b.icmp(block, ICond::Ult, ty.clone(), result, sum, "cf.b");
                let cf = self.m.b.bin(block, BinOp::Or, Type::I1, c1, c2, "cf");
                (result, cf)
            }
        };
        self.m.set_flag(block, Flag::Cf, cf);
        // OF: the operands agree in sign and the result does not.
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), lhs, result, "of.x");
        let y = self.m.b.bin(block, BinOp::Xor, ty.clone(), rhs, result, "of.y");
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "of.and");
        let of = self.m.b.icmp(block, ICond::Slt, ty.clone(), both, 0i64, "of");
        self.m.set_flag(block, Flag::Of, of);
        self.m.set_af(block, ty.clone(), lhs, rhs, result);
        self.m.set_result_flags(block, ty, result);
        result
    }

    /// `lhs - rhs (- borrow)` with every flag.
    fn sub(&mut self, block: BlockId, ty: Type, lhs: ValueId, rhs: ValueId, borrow: Option<ValueId>) -> ValueId {
        let diff = self.m.b.bin(block, BinOp::Sub, ty.clone(), lhs, rhs, "diff");
        let (result, cf) = match borrow {
            None => {
                let cf = self.m.b.icmp(block, ICond::Ult, ty.clone(), lhs, rhs, "cf");
                (diff, cf)
            }
            Some(c) => {
                let c_wide = self.m.zext(block, Type::I1, c, ty.clone(), "c");
                let result = self.m.b.bin(block, BinOp::Sub, ty.clone(), diff, c_wide, "diff.c");
                let c1 = self.m.b.icmp(block, ICond::Ult, ty.clone(), lhs, rhs, "cf.a");
                let c2 = self.m.b.icmp(block, ICond::Ult, ty.clone(), diff, c_wide, "cf.b");
                let cf = self.m.b.bin(block, BinOp::Or, Type::I1, c1, c2, "cf");
                (result, cf)
            }
        };
        self.m.set_flag(block, Flag::Cf, cf);
        // OF: the operands differ in sign and the result's sign differs from the minuend's.
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), lhs, rhs, "of.x");
        let y = self.m.b.bin(block, BinOp::Xor, ty.clone(), lhs, result, "of.y");
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "of.and");
        let of = self.m.b.icmp(block, ICond::Slt, ty.clone(), both, 0i64, "of");
        self.m.set_flag(block, Flag::Of, of);
        self.m.set_af(block, ty.clone(), lhs, rhs, result);
        self.m.set_result_flags(block, ty, result);
        result
    }

    /// A logic result: CF and OF clear, AF clear, the rest from the result.
    fn logic_flags(&mut self, block: BlockId, ty: Type, result: ValueId) {
        self.m.set_flag_const(block, Flag::Cf, false);
        self.m.set_flag_const(block, Flag::Of, false);
        self.m.set_flag_const(block, Flag::Af, false);
        self.m.set_result_flags(block, ty, result);
    }

    /// A shift or rotate count, masked as the machine masks it, and whether it is known nonzero.
    fn shift_count(&mut self, block: BlockId, i: &Instruction, bits: u16) -> (ValueId, bool) {
        let ty = Type::Int(bits);
        let mask = if bits == 64 { 0x3f } else { 0x1f };
        if i.op_kind(1) == OpKind::Register {
            let cl = self.m.read_gpr(block, R::CL);
            let masked = self.m.b.bin(block, BinOp::And, Type::I8, cl, mask, "count.m");
            let wide = self.m.zext(block, Type::I8, masked, ty, "count");
            (wide, false)
        } else {
            let v = Self::immediate(i, 1) & mask;
            (self.m.value_of(block, ty, v), v != 0)
        }
    }

    /// Keeps a flag's old value when `count` is zero.
    fn flag_if_counted(&mut self, block: BlockId, flag: Flag, nonzero: ValueId, known: bool, value: ValueId) {
        if known {
            self.m.set_flag(block, flag, value);
            return;
        }
        let old = self.m.get_flag(block, flag);
        let v = self.m.select(block, nonzero, Type::I1, value, old, "flag.sel");
        self.m.set_flag(block, flag, v);
    }

    fn shift(&mut self, block: BlockId, i: &Instruction, op: BinOp) {
        let bits = self.op_bits(i, 0);
        let ty = Type::Int(bits);
        let src = self.read(block, i, 0);
        let (count, known) = self.shift_count(block, i, bits);
        let nonzero = self.m.is_true(block, ty.clone(), count, "counted");
        let result = self.m.b.bin(block, op, ty.clone(), src, count, "shifted");
        // When the count is zero the value and the flags stay; the write of the same value is harmless.
        let value = if known { result } else { self.m.select(block, nonzero, ty.clone(), result, src, "shift.v") };
        self.write(block, i, 0, value);
        // CF: the last bit shifted out.
        let last = if op == BinOp::Shl {
            let back = self.m.b.bin(block, BinOp::Sub, ty.clone(), i64::from(bits), count, "back");
            self.m.b.bin(block, BinOp::LShr, ty.clone(), src, back, "cf.bits")
        } else {
            let less = self.m.b.bin(block, BinOp::Sub, ty.clone(), count, 1i64, "less");
            self.m.b.bin(block, BinOp::LShr, ty.clone(), src, less, "cf.bits")
        };
        let cf_wide = self.m.b.bin(block, BinOp::And, ty.clone(), last, 1i64, "cf.w");
        let cf = self.m.is_true(block, ty.clone(), cf_wide, "cf");
        self.flag_if_counted(block, Flag::Cf, nonzero, known, cf);
        let of = match op {
            BinOp::Shl => {
                let msb = self.m.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "msb");
                self.m.b.bin(block, BinOp::Xor, Type::I1, msb, cf, "of")
            }
            BinOp::LShr => self.m.b.icmp(block, ICond::Slt, ty.clone(), src, 0i64, "of"),
            _ => self.m.value_of(block, Type::I1, 0i64),
        };
        self.flag_if_counted(block, Flag::Of, nonzero, known, of);
        let zf = self.m.b.icmp(block, ICond::Eq, ty.clone(), result, 0i64, "zf");
        self.flag_if_counted(block, Flag::Zf, nonzero, known, zf);
        let sf = self.m.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "sf");
        self.flag_if_counted(block, Flag::Sf, nonzero, known, sf);
        let low = self.m.trunc(block, ty, result, Type::I8, "low");
        let ones = self.m.intrinsic(block, "llvm.ctpop.i8", Type::I8, vec![(Type::I8, low.into())], "ones");
        let odd = self.m.b.bin(block, BinOp::And, Type::I8, ones, 1i64, "odd");
        let pf = self.m.b.icmp(block, ICond::Eq, Type::I8, odd, 0i64, "pf");
        self.flag_if_counted(block, Flag::Pf, nonzero, known, pf);
    }

    fn rotate(&mut self, block: BlockId, i: &Instruction, left: bool) {
        let bits = self.op_bits(i, 0);
        let ty = Type::Int(bits);
        let src = self.read(block, i, 0);
        let (count, known) = self.shift_count(block, i, bits);
        let nonzero = self.m.is_true(block, ty.clone(), count, "counted");
        let name = format!("llvm.fsh{}.i{bits}", if left { "l" } else { "r" });
        let result = self.m.intrinsic(
            block,
            &name,
            ty.clone(),
            vec![(ty.clone(), src.into()), (ty.clone(), src.into()), (ty.clone(), count.into())],
            "rot",
        );
        self.write(block, i, 0, result);
        let cf = if left {
            let low = self.m.b.bin(block, BinOp::And, ty.clone(), result, 1i64, "cf.w");
            self.m.is_true(block, ty.clone(), low, "cf")
        } else {
            self.m.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "cf")
        };
        self.flag_if_counted(block, Flag::Cf, nonzero, known, cf);
        let msb = self.m.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "msb");
        let of = if left {
            self.m.b.bin(block, BinOp::Xor, Type::I1, msb, cf, "of")
        } else {
            let shifted = self.m.b.bin(block, BinOp::Shl, ty.clone(), result, 1i64, "next");
            let next = self.m.b.icmp(block, ICond::Slt, ty, shifted, 0i64, "msb1");
            self.m.b.bin(block, BinOp::Xor, Type::I1, msb, next, "of")
        };
        self.flag_if_counted(block, Flag::Of, nonzero, known, of);
    }

    // endregion

    // region Instructions

    /// Lifts one instruction into `block`; returns the block to continue in.
    fn lift_instr(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        use Mnemonic as M;
        let ip = i.ip();
        let next_ip = i.next_ip();
        match i.mnemonic() {
            M::Nop
            | M::Endbr64
            | M::Endbr32
            | M::Pause
            | M::Wait
            | M::Prefetcht0
            | M::Prefetcht1
            | M::Prefetcht2
            | M::Prefetchnta
            | M::Lfence
            | M::Mfence
            | M::Sfence => {}
            M::Int3 => self.m.trap(block, ip, trap::INT3),
            M::Ud2 | M::Ud1 | M::Ud0 => self.m.trap(block, ip, trap::UD2),
            M::Hlt => self.m.trap(block, ip, trap::HLT),
            M::Int => {
                let n = Self::immediate(i, 0);
                self.m.trap(block, ip, if n == 0x29 { trap::FASTFAIL } else { trap::INT });
            }

            M::Mov | M::Movsxd | M::Movzx | M::Movsx => {
                if i.op_register(0).is_xmm() || i.op_register(1).is_xmm() {
                    return self.vector_move(block, i);
                }
                let dst_bits = self.op_bits(i, 0);
                let src_bits = self.op_bits(i, 1);
                let v = self.read(block, i, 1);
                let v = match i.mnemonic() {
                    M::Movsx | M::Movsxd => self.m.sext(block, Type::Int(src_bits), v, Type::Int(dst_bits), "sx"),
                    M::Movzx => self.m.zext(block, Type::Int(src_bits), v, Type::Int(dst_bits), "zx"),
                    _ if src_bits < dst_bits => self.m.sext(block, Type::Int(src_bits), v, Type::Int(dst_bits), "imm"),
                    _ if src_bits > dst_bits => self.m.trunc(block, Type::Int(src_bits), v, Type::Int(dst_bits), "t"),
                    _ => v,
                };
                self.write(block, i, 0, v);
            }
            M::Lea => {
                let addr = self.mem_address(block, i);
                let bits = self.op_bits(i, 0);
                let v = self.m.trunc(block, self.addr_ty(), addr, Type::Int(bits), "lea");
                let v = if bits > self.m.bits as u16 { self.m.zext(block, self.addr_ty(), addr, Type::Int(bits), "lea") } else { v };
                self.write(block, i, 0, v);
            }
            M::Xchg => {
                let a = self.read(block, i, 0);
                let b = self.read(block, i, 1);
                self.write(block, i, 0, b);
                self.write(block, i, 1, a);
            }
            M::Cmovo
            | M::Cmovno
            | M::Cmovb
            | M::Cmovae
            | M::Cmove
            | M::Cmovne
            | M::Cmovbe
            | M::Cmova
            | M::Cmovs
            | M::Cmovns
            | M::Cmovp
            | M::Cmovnp
            | M::Cmovl
            | M::Cmovge
            | M::Cmovle
            | M::Cmovg => {
                let bits = self.op_bits(i, 0);
                let cond = self.condition(block, i.condition_code());
                let old = self.read(block, i, 0);
                let new = self.read(block, i, 1);
                let v = self.m.select(block, cond, Type::Int(bits), new, old, "cmov");
                self.write(block, i, 0, v);
            }
            M::Seto
            | M::Setno
            | M::Setb
            | M::Setae
            | M::Sete
            | M::Setne
            | M::Setbe
            | M::Seta
            | M::Sets
            | M::Setns
            | M::Setp
            | M::Setnp
            | M::Setl
            | M::Setge
            | M::Setle
            | M::Setg => {
                let cond = self.condition(block, i.condition_code());
                let v = self.m.zext(block, Type::I1, cond, Type::I8, "set");
                self.write(block, i, 0, v);
            }

            // `xor r, r` and `sub r, r` zero a register without reading it; a lift that read it
            // would see a parameter where there is none.
            M::Xor | M::Sub if same_register(i) => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let zero = self.m.value_of(block, ty.clone(), 0i64);
                self.write(block, i, 0, zero);
                self.logic_flags(block, ty, zero);
            }
            M::Add | M::Adc | M::Sub | M::Sbb | M::Cmp => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let lhs = self.read(block, i, 0);
                let rhs = self.read(block, i, 1);
                let rhs = self.widen_to(block, i, 1, rhs, bits);
                let carry = matches!(i.mnemonic(), M::Adc | M::Sbb).then(|| self.m.get_flag(block, Flag::Cf));
                let result = match i.mnemonic() {
                    M::Add | M::Adc => self.add(block, ty, lhs, rhs, carry),
                    _ => self.sub(block, ty, lhs, rhs, carry),
                };
                if i.mnemonic() != M::Cmp {
                    self.write(block, i, 0, result);
                }
            }
            M::And | M::Or | M::Xor | M::Test => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let lhs = self.read(block, i, 0);
                let rhs = self.read(block, i, 1);
                let rhs = self.widen_to(block, i, 1, rhs, bits);
                let op = match i.mnemonic() {
                    M::And | M::Test => BinOp::And,
                    M::Or => BinOp::Or,
                    _ => BinOp::Xor,
                };
                let result = self.m.b.bin(block, op, ty.clone(), lhs, rhs, "logic");
                self.logic_flags(block, ty, result);
                if i.mnemonic() != M::Test {
                    self.write(block, i, 0, result);
                }
            }
            M::Inc | M::Dec => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let v = self.read(block, i, 0);
                let one = self.m.value_of(block, ty.clone(), 1i64);
                let cf = self.m.get_flag(block, Flag::Cf);
                let result = if i.mnemonic() == M::Inc { self.add(block, ty, v, one, None) } else { self.sub(block, ty, v, one, None) };
                self.m.set_flag(block, Flag::Cf, cf); // inc and dec keep CF
                self.write(block, i, 0, result);
            }
            M::Neg => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let v = self.read(block, i, 0);
                let zero = self.m.value_of(block, ty.clone(), 0i64);
                let result = self.sub(block, ty, zero, v, None);
                self.write(block, i, 0, result);
            }
            M::Not => {
                let bits = self.op_bits(i, 0);
                let v = self.read(block, i, 0);
                let result = self.m.b.bin(block, BinOp::Xor, Type::Int(bits), v, -1i64, "not");
                self.write(block, i, 0, result);
            }
            M::Shl | M::Sal => self.shift(block, i, BinOp::Shl),
            M::Shr => self.shift(block, i, BinOp::LShr),
            M::Sar => self.shift(block, i, BinOp::AShr),
            M::Rol => self.rotate(block, i, true),
            M::Ror => self.rotate(block, i, false),
            M::Shld | M::Shrd => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let a = self.read(block, i, 0);
                let b = self.read(block, i, 1);
                let count = match i.op_kind(2) {
                    OpKind::Register => {
                        let cl = self.m.read_gpr(block, R::CL);
                        let m = self.m.b.bin(block, BinOp::And, Type::I8, cl, if bits == 64 { 0x3f } else { 0x1f }, "count.m");
                        self.m.zext(block, Type::I8, m, ty.clone(), "count")
                    }
                    _ => self.m.value_of(block, ty.clone(), Self::immediate(i, 2) & if bits == 64 { 0x3f } else { 0x1f }),
                };
                let name = format!("llvm.fsh{}.i{bits}", if i.mnemonic() == M::Shld { "l" } else { "r" });
                let (x, y) = if i.mnemonic() == M::Shld { (a, b) } else { (b, a) };
                let result = self.m.intrinsic(
                    block,
                    &name,
                    ty.clone(),
                    vec![(ty.clone(), x.into()), (ty.clone(), y.into()), (ty.clone(), count.into())],
                    "dshift",
                );
                self.write(block, i, 0, result);
                self.m.set_result_flags(block, ty, result);
            }
            M::Bswap => {
                let bits = self.op_bits(i, 0);
                let v = self.read(block, i, 0);
                let result =
                    self.m.intrinsic(block, &format!("llvm.bswap.i{bits}"), Type::Int(bits), vec![(Type::Int(bits), v.into())], "bswap");
                self.write(block, i, 0, result);
            }
            M::Bsf | M::Bsr | M::Tzcnt | M::Lzcnt | M::Popcnt => {
                let bits = self.op_bits(i, 1);
                let ty = Type::Int(bits);
                let v = self.read(block, i, 1);
                let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), v, 0i64, "src.zero");
                let result = match i.mnemonic() {
                    M::Popcnt => self.m.intrinsic(block, &format!("llvm.ctpop.i{bits}"), ty.clone(), vec![(ty.clone(), v.into())], "pop"),
                    M::Bsf | M::Tzcnt => self.m.intrinsic(
                        block,
                        &format!("llvm.cttz.i{bits}"),
                        ty.clone(),
                        vec![(ty.clone(), v.into()), (Type::I1, 0i64.into())],
                        "tz",
                    ),
                    _ => {
                        let lz = self.m.intrinsic(
                            block,
                            &format!("llvm.ctlz.i{bits}"),
                            ty.clone(),
                            vec![(ty.clone(), v.into()), (Type::I1, 0i64.into())],
                            "lz",
                        );
                        if i.mnemonic() == M::Lzcnt {
                            lz
                        } else {
                            self.m.b.bin(block, BinOp::Sub, ty.clone(), i64::from(bits) - 1, lz, "bsr")
                        }
                    }
                };
                match i.mnemonic() {
                    M::Popcnt => {
                        self.logic_flags(block, ty, result);
                        self.write(block, i, 0, result);
                    }
                    M::Tzcnt | M::Lzcnt => {
                        self.m.set_flag(block, Flag::Cf, zero);
                        let zf = self.m.b.icmp(block, ICond::Eq, ty, result, 0i64, "zf");
                        self.m.set_flag(block, Flag::Zf, zf);
                        self.write(block, i, 0, result);
                    }
                    _ => {
                        // bsf and bsr leave the destination alone when the source is zero.
                        self.m.set_flag(block, Flag::Zf, zero);
                        let old = self.read(block, i, 0);
                        let v = self.m.select(block, zero, ty, old, result, "bs");
                        self.write(block, i, 0, v);
                    }
                }
            }
            M::Bt | M::Bts | M::Btr | M::Btc => return self.bit_test(block, i),

            M::Imul => return self.imul(block, i),
            M::Mul => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let wide = Type::Int(bits * 2);
                let a = self.m.read_gpr(block, accumulator(bits));
                let b = self.read(block, i, 0);
                let a_w = self.m.zext(block, ty.clone(), a, wide.clone(), "a");
                let b_w = self.m.zext(block, ty.clone(), b, wide.clone(), "b");
                let product = self.m.b.bin(block, BinOp::Mul, wide.clone(), a_w, b_w, "product");
                let low = self.m.trunc(block, wide.clone(), product, ty.clone(), "low");
                let shifted = self.m.b.bin(block, BinOp::LShr, wide.clone(), product, i64::from(bits), "high.s");
                let high = self.m.trunc(block, wide, shifted, ty.clone(), "high");
                self.write_product(block, bits, low, high);
                let overflow = self.m.is_true(block, ty, high, "of");
                self.m.set_flag(block, Flag::Cf, overflow);
                self.m.set_flag(block, Flag::Of, overflow);
            }
            M::Div | M::Idiv => return self.divide(block, i, i.mnemonic() == M::Idiv),
            M::Cdq | M::Cqo | M::Cwd => {
                let bits = match i.mnemonic() {
                    M::Cwd => 16,
                    M::Cdq => 32,
                    _ => 64,
                };
                let ty = Type::Int(bits);
                let a = self.m.read_gpr(block, accumulator(bits));
                let sign = self.m.b.bin(block, BinOp::AShr, ty, a, i64::from(bits) - 1, "sign");
                self.m.write_gpr(block, data_register(bits), sign);
            }
            M::Cbw | M::Cwde | M::Cdqe => {
                let (from, to) = match i.mnemonic() {
                    M::Cbw => (8, 16),
                    M::Cwde => (16, 32),
                    _ => (32, 64),
                };
                let a = self.m.read_gpr(block, accumulator(from));
                let v = self.m.sext(block, Type::Int(from), a, Type::Int(to), "sx");
                self.m.write_gpr(block, accumulator(to), v);
            }

            M::Push => {
                let bits = self.op_bits(i, 0);
                let v = self.read(block, i, 0);
                let v = self.m.sext(block, Type::Int(bits), v, self.addr_ty(), "push");
                // push rsp pushes the value before the decrement, which `read` already took.
                self.m.push(block, v);
            }
            M::Pop => {
                let v = self.m.pop(block, "pop");
                self.write(block, i, 0, v);
            }
            M::Pushfq | M::Pushfd | M::Pushf => {
                let flags = self.pack_flags(block);
                self.m.push(block, flags);
            }
            M::Popfq | M::Popfd | M::Popf => {
                let v = self.m.pop(block, "flags");
                self.unpack_flags(block, v);
            }
            M::Leave => {
                let bp = if self.m.bits == 64 { R::RBP } else { R::EBP };
                let v = self.m.read_gpr(block, bp);
                self.m.write_sp(block, v);
                let old = self.m.pop(block, "bp");
                self.m.write_gpr(block, bp, old);
            }
            M::Call => {
                let ret = self.m.value_of(block, self.addr_ty(), next_ip as i64);
                match i.op_kind(0) {
                    OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64 => {
                        let target = i.near_branch_target();
                        if self.lifted.functions.contains_key(&target) {
                            self.m.push(block, ret);
                            self.call_lifted(block, target);
                        } else {
                            self.warn(format!("{ip:#x}: a call to {target:#x}, which is no function"));
                            self.m.trap(block, ip, trap::INVALID);
                        }
                    }
                    OpKind::Memory if i.is_ip_rel_memory_operand() || (i.memory_base() == R::None && i.memory_index() == R::None) => {
                        let slot = if i.is_ip_rel_memory_operand() { i.ip_rel_memory_address() } else { i.memory_displacement64() };
                        if let Some(import) = self.image.import_at(slot) {
                            let name = import.display_name();
                            self.import_call(block, &name, false);
                        } else {
                            let target = self.read(block, i, 0);
                            let wide = self.m.zext(block, self.addr_ty(), target, Type::I64, "target");
                            self.m.push(block, ret);
                            self.runtime_call(block, wide);
                        }
                    }
                    _ => {
                        // `call reg` with the register holding an import's slot value: the
                        // analysis resolved it, and the import's own convention applies.
                        if let Some(import) = self.register_import_calls.get(&ip).cloned() {
                            self.import_call(block, &import, false);
                        } else {
                            let target = self.read(block, i, 0);
                            let wide = self.m.zext(block, self.addr_ty(), target, Type::I64, "target");
                            self.m.push(block, ret);
                            self.runtime_call(block, wide);
                        }
                    }
                }
            }
            M::Ret | M::Retf => {
                self.m.pop(block, "ret.addr");
                if i.op_count() > 0 {
                    let extra = Self::immediate(i, 0);
                    let sp = self.m.read_sp(block);
                    let adjusted = self.m.b.bin(block, BinOp::Add, self.addr_ty(), sp, extra, "sp");
                    self.m.write_sp(block, adjusted);
                }
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            M::Jmp => {
                if !matches!(i.op_kind(0), OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64) {
                    let target = self.read(block, i, 0);
                    self.jump_target = Some(target);
                }
            }
            M::Jo
            | M::Jno
            | M::Jb
            | M::Jae
            | M::Je
            | M::Jne
            | M::Jbe
            | M::Ja
            | M::Js
            | M::Jns
            | M::Jp
            | M::Jnp
            | M::Jl
            | M::Jge
            | M::Jle
            | M::Jg => {
                let cond = self.condition(block, i.condition_code());
                self.cond = Some(cond);
            }
            M::Jrcxz | M::Jecxz | M::Jcxz => {
                let (reg, bits) = match i.mnemonic() {
                    M::Jrcxz => (R::RCX, 64),
                    M::Jecxz => (R::ECX, 32),
                    _ => (R::CX, 16),
                };
                let v = self.m.read_gpr(block, reg);
                let cond = self.m.b.icmp(block, ICond::Eq, Type::Int(bits), v, 0i64, "cx.zero");
                self.cond = Some(cond);
            }
            M::Loop | M::Loope | M::Loopne => {
                let reg = if self.m.bits == 64 { R::RCX } else { R::ECX };
                let ty = self.addr_ty();
                let v = self.m.read_gpr(block, reg);
                let dec = self.m.b.bin(block, BinOp::Sub, ty.clone(), v, 1i64, "cx");
                self.m.write_gpr(block, reg, dec);
                let nonzero = self.m.is_true(block, ty, dec, "cx.nz");
                let cond = match i.mnemonic() {
                    M::Loope => {
                        let z = self.m.get_flag(block, Flag::Zf);
                        self.m.b.bin(block, BinOp::And, Type::I1, nonzero, z, "loope")
                    }
                    M::Loopne => {
                        let z = self.m.get_flag(block, Flag::Zf);
                        let nz = self.m.b.bin(block, BinOp::Xor, Type::I1, z, 1i64, "nz");
                        self.m.b.bin(block, BinOp::And, Type::I1, nonzero, nz, "loopne")
                    }
                    _ => nonzero,
                };
                self.cond = Some(cond);
            }

            M::Cld => self.m.set_flag_const(block, Flag::Df, false),
            M::Std => self.m.set_flag_const(block, Flag::Df, true),
            M::Clc => self.m.set_flag_const(block, Flag::Cf, false),
            M::Stc => self.m.set_flag_const(block, Flag::Cf, true),
            M::Cmc => {
                let c = self.m.get_flag(block, Flag::Cf);
                let n = self.m.b.bin(block, BinOp::Xor, Type::I1, c, 1i64, "cmc");
                self.m.set_flag(block, Flag::Cf, n);
            }
            M::Sahf => {
                let ah = self.m.read_gpr(block, R::AH);
                for (flag, bit) in [(Flag::Cf, 0), (Flag::Pf, 2), (Flag::Af, 4), (Flag::Zf, 6), (Flag::Sf, 7)] {
                    let shifted = self.m.b.bin(block, BinOp::LShr, Type::I8, ah, bit, "f.s");
                    let one = self.m.b.bin(block, BinOp::And, Type::I8, shifted, 1i64, "f.b");
                    let v = self.m.is_true(block, Type::I8, one, "f");
                    self.m.set_flag(block, flag, v);
                }
            }
            M::Lahf => {
                let mut acc = self.m.value_of(block, Type::I8, 2i64);
                for (flag, bit) in [(Flag::Cf, 0), (Flag::Pf, 2), (Flag::Af, 4), (Flag::Zf, 6), (Flag::Sf, 7)] {
                    let f = self.m.get_flag(block, flag);
                    let wide = self.m.zext(block, Type::I1, f, Type::I8, "f.w");
                    let placed = self.m.b.bin(block, BinOp::Shl, Type::I8, wide, bit, "f.p");
                    acc = self.m.b.bin(block, BinOp::Or, Type::I8, acc, placed, "ah");
                }
                self.m.write_gpr(block, R::AH, acc);
            }
            M::Xadd => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let dst = self.read(block, i, 0);
                let src = self.read(block, i, 1);
                let sum = self.add(block, ty, dst, src, None);
                self.write(block, i, 1, dst);
                self.write(block, i, 0, sum);
            }
            M::Cmpxchg => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let acc = accumulator(bits);
                let a = self.m.read_gpr(block, acc);
                let dst = self.read(block, i, 0);
                let src = self.read(block, i, 1);
                self.sub(block, ty.clone(), a, dst, None);
                let equal = self.m.get_flag(block, Flag::Zf);
                let new_dst = self.m.select(block, equal, ty.clone(), src, dst, "cx.dst");
                let new_acc = self.m.select(block, equal, ty, a, dst, "cx.acc");
                self.write(block, i, 0, new_dst);
                self.m.write_gpr(block, acc, new_acc);
            }

            // The string operations; `movsd` and `cmpsd` with an xmm operand are the scalar double
            // forms below.
            M::Movsb
            | M::Movsw
            | M::Movsd
            | M::Movsq
            | M::Stosb
            | M::Stosw
            | M::Stosd
            | M::Stosq
            | M::Lodsb
            | M::Lodsw
            | M::Lodsd
            | M::Lodsq
            | M::Scasb
            | M::Scasw
            | M::Scasd
            | M::Scasq
            | M::Cmpsb
            | M::Cmpsw
            | M::Cmpsd
            | M::Cmpsq
                if !i.op_register(0).is_xmm() && !i.op_register(1).is_xmm() =>
            {
                return self.string_op(block, i);
            }

            // SSE
            M::Movd
            | M::Movq
            | M::Movdqa
            | M::Movdqu
            | M::Movaps
            | M::Movups
            | M::Movapd
            | M::Movupd
            | M::Movss
            | M::Movsd
            | M::Movlps
            | M::Movlpd
            | M::Movhps
            | M::Movhpd
            | M::Lddqu => {
                return self.vector_move(block, i);
            }
            M::Pshufd => {
                let imm = Self::immediate(i, 2) as i32;
                let v4 = Type::vector(4, Type::I32);
                let src = self.read_vec(block, i, 1, v4.clone());
                let mask = (0..4).map(|k| (imm >> (2 * k)) & 3).collect();
                let r = self.m.b.value(block, Inst::ShuffleVector { ty: v4.clone(), a: src.into(), b: Operand::Undef, mask }, "pshufd");
                self.write_vec(block, i, 0, v4, r);
            }
            // The vector zeroing idioms, and `pcmpeq x, x` for all ones: no read of the register.
            M::Pxor | M::Xorps | M::Xorpd | M::Psubb | M::Psubw | M::Psubd | M::Psubq | M::Pandn | M::Andnps | M::Andnpd
                if same_register(i) =>
            {
                let zero = self.m.value_of(block, Type::I128, 0i64);
                self.m.write_xmm(block, i.op_register(0), Type::I128, zero, true);
            }
            M::Pcmpeqb | M::Pcmpeqw | M::Pcmpeqd if same_register(i) => {
                let ones = self.m.value_of(block, Type::I128, -1i64);
                self.m.write_xmm(block, i.op_register(0), Type::I128, ones, true);
            }
            M::Paddb
            | M::Paddw
            | M::Paddd
            | M::Paddq
            | M::Psubb
            | M::Psubw
            | M::Psubd
            | M::Psubq
            | M::Pand
            | M::Pandn
            | M::Por
            | M::Pxor
            | M::Xorps
            | M::Xorpd
            | M::Andps
            | M::Andpd
            | M::Orps
            | M::Orpd
            | M::Andnps
            | M::Andnpd
            | M::Pmulld
            | M::Pmullw => {
                let ty = match i.mnemonic() {
                    M::Paddb | M::Psubb => Type::vector(16, Type::I8),
                    M::Paddw | M::Psubw | M::Pmullw => Type::vector(8, Type::I16),
                    M::Paddd | M::Psubd | M::Pmulld => Type::vector(4, Type::I32),
                    _ => Type::vector(2, Type::I64),
                };
                let a = self.read_vec(block, i, 0, ty.clone());
                let b = self.read_vec(block, i, 1, ty.clone());
                let r = match i.mnemonic() {
                    M::Paddb | M::Paddw | M::Paddd | M::Paddq => self.m.b.bin(block, BinOp::Add, ty.clone(), a, b, "padd"),
                    M::Psubb | M::Psubw | M::Psubd | M::Psubq => self.m.b.bin(block, BinOp::Sub, ty.clone(), a, b, "psub"),
                    M::Pmulld | M::Pmullw => self.m.b.bin(block, BinOp::Mul, ty.clone(), a, b, "pmul"),
                    M::Pand | M::Andps | M::Andpd => self.m.b.bin(block, BinOp::And, ty.clone(), a, b, "pand"),
                    M::Pandn | M::Andnps | M::Andnpd => {
                        let na = self.m.b.bin(block, BinOp::Xor, ty.clone(), a, -1i64, "pnot");
                        self.m.b.bin(block, BinOp::And, ty.clone(), na, b, "pandn")
                    }
                    M::Por | M::Orps | M::Orpd => self.m.b.bin(block, BinOp::Or, ty.clone(), a, b, "por"),
                    _ => self.m.b.bin(block, BinOp::Xor, ty.clone(), a, b, "pxor"),
                };
                self.write_vec(block, i, 0, ty, r);
            }
            M::Pcmpeqb | M::Pcmpeqw | M::Pcmpeqd | M::Pcmpgtb | M::Pcmpgtw | M::Pcmpgtd => {
                let ty = match i.mnemonic() {
                    M::Pcmpeqb | M::Pcmpgtb => Type::vector(16, Type::I8),
                    M::Pcmpeqw | M::Pcmpgtw => Type::vector(8, Type::I16),
                    _ => Type::vector(4, Type::I32),
                };
                let a = self.read_vec(block, i, 0, ty.clone());
                let b = self.read_vec(block, i, 1, ty.clone());
                let cond = if matches!(i.mnemonic(), M::Pcmpeqb | M::Pcmpeqw | M::Pcmpeqd) { ICond::Eq } else { ICond::Sgt };
                let bits = self.m.b.icmp(block, cond, ty.clone(), a, b, "pcmp");
                let Type::Vector(n, _) = &ty else { unreachable!() };
                let r = self.m.b.cast(block, Cast::SExt, Type::vector(*n, Type::I1), bits, ty.clone(), "pcmp.m");
                self.write_vec(block, i, 0, ty, r);
            }
            M::Punpckldq
            | M::Punpcklqdq
            | M::Punpcklbw
            | M::Punpcklwd
            | M::Punpckhdq
            | M::Punpckhqdq
            | M::Unpcklps
            | M::Unpcklpd
            | M::Unpckhps
            | M::Unpckhpd => {
                let (ty, n) = match i.mnemonic() {
                    M::Punpcklbw => (Type::vector(16, Type::I8), 16),
                    M::Punpcklwd => (Type::vector(8, Type::I16), 8),
                    M::Punpckldq | M::Punpckhdq | M::Unpcklps | M::Unpckhps => (Type::vector(4, Type::I32), 4),
                    _ => (Type::vector(2, Type::I64), 2),
                };
                let high = matches!(i.mnemonic(), M::Punpckhdq | M::Punpckhqdq | M::Unpckhps | M::Unpckhpd);
                let a = self.read_vec(block, i, 0, ty.clone());
                let b = self.read_vec(block, i, 1, ty.clone());
                let half = n / 2;
                let base = if high { half } else { 0 };
                let mask = (0..half).flat_map(|k| [base + k, n + base + k]).collect();
                let r = self.m.b.value(block, Inst::ShuffleVector { ty: ty.clone(), a: a.into(), b: b.into(), mask }, "unpck");
                self.write_vec(block, i, 0, ty, r);
            }
            M::Psrldq | M::Pslldq => {
                let count = Self::immediate(i, 1).clamp(0, 16) * 8;
                let v = self.read_vec(block, i, 0, Type::I128);
                let r = if i.mnemonic() == M::Psrldq {
                    self.m.b.bin(block, BinOp::LShr, Type::I128, v, count, "psrldq")
                } else {
                    self.m.b.bin(block, BinOp::Shl, Type::I128, v, count, "pslldq")
                };
                self.write_vec(block, i, 0, Type::I128, r);
            }
            M::Psrld | M::Psrlq | M::Psrlw | M::Pslld | M::Psllq | M::Psllw | M::Psrad | M::Psraw => {
                let (ty, lanes, lane_bits) = match i.mnemonic() {
                    M::Psrlw | M::Psllw | M::Psraw => (Type::vector(8, Type::I16), 8, 16),
                    M::Psrld | M::Pslld | M::Psrad => (Type::vector(4, Type::I32), 4, 32),
                    _ => (Type::vector(2, Type::I64), 2, 64),
                };
                let v = self.read_vec(block, i, 0, ty.clone());
                let count = if i.op_kind(1) == OpKind::Immediate8 {
                    Self::immediate(i, 1)
                } else {
                    self.warn(format!("{ip:#x}: a vector shift by a register count is not lifted"));
                    0
                };
                let count = count.clamp(0, i64::from(lane_bits));
                let splat = self.splat(block, &ty, lanes, count);
                let op = match i.mnemonic() {
                    M::Psrld | M::Psrlq | M::Psrlw => BinOp::LShr,
                    M::Psrad | M::Psraw => BinOp::AShr,
                    _ => BinOp::Shl,
                };
                let r = if count >= i64::from(lane_bits) && op != BinOp::AShr {
                    self.m.value_of(block, ty.clone(), 0i64)
                } else {
                    self.m.b.bin(block, op, ty.clone(), v, splat, "pshift")
                };
                self.write_vec(block, i, 0, ty, r);
            }
            M::Addss
            | M::Addsd
            | M::Subss
            | M::Subsd
            | M::Mulss
            | M::Mulsd
            | M::Divss
            | M::Divsd
            | M::Minss
            | M::Minsd
            | M::Maxss
            | M::Maxsd
            | M::Sqrtss
            | M::Sqrtsd => {
                let single = matches!(i.mnemonic(), M::Addss | M::Subss | M::Mulss | M::Divss | M::Minss | M::Maxss | M::Sqrtss);
                let fty = if single { Type::Float } else { Type::Double };
                let a = self.read_vec(block, i, 0, fty.clone());
                let b = self.read_vec(block, i, 1, fty.clone());
                let r = match i.mnemonic() {
                    M::Addss | M::Addsd => self.m.b.bin(block, BinOp::FAdd, fty.clone(), a, b, "fadd"),
                    M::Subss | M::Subsd => self.m.b.bin(block, BinOp::FSub, fty.clone(), a, b, "fsub"),
                    M::Mulss | M::Mulsd => self.m.b.bin(block, BinOp::FMul, fty.clone(), a, b, "fmul"),
                    M::Divss | M::Divsd => self.m.b.bin(block, BinOp::FDiv, fty.clone(), a, b, "fdiv"),
                    M::Minss | M::Minsd => {
                        let lt =
                            self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, "lt");
                        self.m.b.select(block, lt, fty.clone(), a, b, "fmin")
                    }
                    M::Maxss | M::Maxsd => {
                        let gt =
                            self.m.b.value(block, Inst::FCmp { cond: FCond::Ogt, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, "gt");
                        self.m.b.select(block, gt, fty.clone(), a, b, "fmax")
                    }
                    _ => self.m.intrinsic(
                        block,
                        if single { "llvm.sqrt.f32" } else { "llvm.sqrt.f64" },
                        fty.clone(),
                        vec![(fty.clone(), b.into())],
                        "fsqrt",
                    ),
                };
                self.m.write_xmm(block, i.op_register(0), fty, r, false);
            }
            M::Cvtsi2ss | M::Cvtsi2sd => {
                let bits = self.op_bits(i, 1);
                let v = self.read(block, i, 1);
                let fty = if i.mnemonic() == M::Cvtsi2ss { Type::Float } else { Type::Double };
                let r = self.m.b.cast(block, Cast::SIToFP, Type::Int(bits), v, fty.clone(), "cvt");
                self.m.write_xmm(block, i.op_register(0), fty, r, false);
            }
            M::Cvttss2si | M::Cvttsd2si | M::Cvtss2si | M::Cvtsd2si => {
                let bits = self.op_bits(i, 0);
                let single = matches!(i.mnemonic(), M::Cvttss2si | M::Cvtss2si);
                let fty = if single { Type::Float } else { Type::Double };
                let v = self.read_vec(block, i, 1, fty.clone());
                let v = if matches!(i.mnemonic(), M::Cvtss2si | M::Cvtsd2si) {
                    self.m.intrinsic(
                        block,
                        if single { "llvm.rint.f32" } else { "llvm.rint.f64" },
                        fty.clone(),
                        vec![(fty.clone(), v.into())],
                        "round",
                    )
                } else {
                    v
                };
                let r = self.m.b.cast(block, Cast::FPToSI, fty, v, Type::Int(bits), "cvt");
                self.write(block, i, 0, r);
            }
            M::Cvtss2sd | M::Cvtsd2ss => {
                let (from, to) = if i.mnemonic() == M::Cvtss2sd { (Type::Float, Type::Double) } else { (Type::Double, Type::Float) };
                let v = self.read_vec(block, i, 1, from.clone());
                let cast = if i.mnemonic() == M::Cvtss2sd { Cast::FPExt } else { Cast::FPTrunc };
                let r = self.m.b.cast(block, cast, from, v, to.clone(), "cvt");
                self.m.write_xmm(block, i.op_register(0), to, r, false);
            }
            M::Ucomiss | M::Ucomisd | M::Comiss | M::Comisd => {
                let fty = if matches!(i.mnemonic(), M::Ucomiss | M::Comiss) { Type::Float } else { Type::Double };
                let a = self.read_vec(block, i, 0, fty.clone());
                let b = self.read_vec(block, i, 1, fty.clone());
                let unordered =
                    self.m.b.value(block, Inst::FCmp { cond: FCond::Uno, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, "uno");
                let less = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, "lt");
                let equal = self.m.b.value(block, Inst::FCmp { cond: FCond::Oeq, ty: fty, lhs: a.into(), rhs: b.into() }, "eq");
                let zf = self.m.b.bin(block, BinOp::Or, Type::I1, equal, unordered, "zf");
                let cf = self.m.b.bin(block, BinOp::Or, Type::I1, less, unordered, "cf");
                self.m.set_flag(block, Flag::Zf, zf);
                self.m.set_flag(block, Flag::Pf, unordered);
                self.m.set_flag(block, Flag::Cf, cf);
                self.m.set_flag_const(block, Flag::Of, false);
                self.m.set_flag_const(block, Flag::Sf, false);
                self.m.set_flag_const(block, Flag::Af, false);
            }
            M::Cmpss | M::Cmpsd | M::Cmpps | M::Cmppd => {
                // The predicate: eq, lt, le, unord, neq, nlt, nle, ord. The result is an all-ones mask.
                let predicate = Self::immediate(i, 2) & 7;
                let scalar = matches!(i.mnemonic(), M::Cmpss | M::Cmpsd);
                let single = matches!(i.mnemonic(), M::Cmpss | M::Cmpps);
                let (fty, ity) = match (scalar, single) {
                    (true, true) => (Type::Float, Type::I32),
                    (true, false) => (Type::Double, Type::I64),
                    (false, true) => (Type::vector(4, Type::Float), Type::vector(4, Type::I32)),
                    (false, false) => (Type::vector(2, Type::Double), Type::vector(2, Type::I64)),
                };
                let a = self.read_vec(block, i, 0, fty.clone());
                let b = self.read_vec(block, i, 1, fty.clone());
                let cond = match predicate {
                    0 => FCond::Oeq,
                    1 => FCond::Olt,
                    2 => FCond::Ole,
                    3 => FCond::Uno,
                    4 => FCond::Une,
                    5 => FCond::Uge,
                    6 => FCond::Ugt,
                    _ => FCond::Ord,
                };
                let bits = self.m.b.value(block, Inst::FCmp { cond, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, "fcmp");
                let from = match &fty {
                    Type::Vector(n, _) => Type::vector(*n, Type::I1),
                    _ => Type::I1,
                };
                let mask = self.m.b.cast(block, Cast::SExt, from, bits, ity.clone(), "fmask");
                self.m.write_xmm(block, i.op_register(0), ity, mask, false);
            }
            M::Movmskps | M::Movmskpd => {
                let (vty, lanes, lane_ty) = if i.mnemonic() == M::Movmskps {
                    (Type::vector(4, Type::I32), 4, Type::I32)
                } else {
                    (Type::vector(2, Type::I64), 2, Type::I64)
                };
                let v = self.read_vec(block, i, 1, vty.clone());
                let mut acc = self.m.value_of(block, Type::I32, 0i64);
                for k in 0..lanes {
                    let lane = self.m.b.value(
                        block,
                        Inst::ExtractElement { ty: vty.clone(), vector: v.into(), index: Operand::Int(i128::from(k)) },
                        "lane",
                    );
                    let neg = self.m.b.icmp(block, ICond::Slt, lane_ty.clone(), lane, 0i64, "neg");
                    let bit = self.m.zext(block, Type::I1, neg, Type::I32, "bit");
                    let placed = self.m.b.bin(block, BinOp::Shl, Type::I32, bit, i64::from(k), "placed");
                    acc = self.m.b.bin(block, BinOp::Or, Type::I32, acc, placed, "mask");
                }
                self.write_as(block, i, 0, Type::I32, acc);
            }
            M::Cvtdq2ps | M::Cvtdq2pd | M::Cvtps2pd | M::Cvtpd2ps | M::Cvttps2dq | M::Cvttpd2dq => {
                let (from, to, cast) = match i.mnemonic() {
                    M::Cvtdq2ps => (Type::vector(4, Type::I32), Type::vector(4, Type::Float), Cast::SIToFP),
                    M::Cvtdq2pd => (Type::vector(2, Type::I32), Type::vector(2, Type::Double), Cast::SIToFP),
                    M::Cvtps2pd => (Type::vector(2, Type::Float), Type::vector(2, Type::Double), Cast::FPExt),
                    M::Cvtpd2ps => (Type::vector(2, Type::Double), Type::vector(2, Type::Float), Cast::FPTrunc),
                    M::Cvttps2dq => (Type::vector(4, Type::Float), Type::vector(4, Type::I32), Cast::FPToSI),
                    _ => (Type::vector(2, Type::Double), Type::vector(2, Type::I32), Cast::FPToSI),
                };
                let v = self.read_vec(block, i, 1, from.clone());
                let r = self.m.b.cast(block, cast, from, v, to.clone(), "cvtp");
                self.m.write_xmm(block, i.op_register(0), to, r, true);
            }
            M::Shufps | M::Shufpd => {
                let imm = Self::immediate(i, 2);
                let (ty, mask): (Type, Vec<i32>) = if i.mnemonic() == M::Shufps {
                    let sel = |k: i64| ((imm >> (2 * k)) & 3) as i32;
                    (Type::vector(4, Type::I32), vec![sel(0), sel(1), 4 + sel(2), 4 + sel(3)])
                } else {
                    (Type::vector(2, Type::I64), vec![(imm & 1) as i32, 2 + ((imm >> 1) & 1) as i32])
                };
                let a = self.read_vec(block, i, 0, ty.clone());
                let b = self.read_vec(block, i, 1, ty.clone());
                let r = self.m.b.value(block, Inst::ShuffleVector { ty: ty.clone(), a: a.into(), b: b.into(), mask }, "shuf");
                self.write_vec(block, i, 0, ty, r);
            }
            M::Addps | M::Addpd | M::Subps | M::Subpd | M::Mulps | M::Mulpd | M::Divps | M::Divpd => {
                let ty = if matches!(i.mnemonic(), M::Addps | M::Subps | M::Mulps | M::Divps) {
                    Type::vector(4, Type::Float)
                } else {
                    Type::vector(2, Type::Double)
                };
                let a = self.read_vec(block, i, 0, ty.clone());
                let b = self.read_vec(block, i, 1, ty.clone());
                let op = match i.mnemonic() {
                    M::Addps | M::Addpd => BinOp::FAdd,
                    M::Subps | M::Subpd => BinOp::FSub,
                    M::Mulps | M::Mulpd => BinOp::FMul,
                    _ => BinOp::FDiv,
                };
                let r = self.m.b.bin(block, op, ty.clone(), a, b, "fp");
                self.write_vec(block, i, 0, ty, r);
            }

            m if crate::x87::is_x87(m) => self.x87(block, i),

            // The processor's answers come from the host: the port runs on a processor at least
            // as capable as the one the program asks about.
            M::Cpuid => {
                let leaf = self.m.read_gpr(block, R::EAX);
                let leaf = self.m.zext(block, Type::I32, leaf, Type::I64, "leaf");
                let sub = self.m.read_gpr(block, R::ECX);
                let sub = self.m.zext(block, Type::I32, sub, Type::I64, "subleaf");
                let out = self.m.b.alloca(block, Type::array(4, Type::I32), "cpuid");
                self.m.b.call(
                    block,
                    FnType::new(Type::Void, vec![Type::I64, Type::I64, Type::Ptr]),
                    Callee::Direct("ra_cpuid".into()),
                    vec![(Type::I64, leaf.into()), (Type::I64, sub.into()), (Type::Ptr, out.into())],
                    "",
                );
                for (n, reg) in [R::EAX, R::EBX, R::ECX, R::EDX].into_iter().enumerate() {
                    let p = self.m.b.gep(
                        block,
                        Type::array(4, Type::I32),
                        out,
                        vec![(Type::I64, 0i64.into()), (Type::I64, (n as i64).into())],
                        "cpuid.p",
                    );
                    let v = self.m.b.load(block, Type::I32, p, "cpuid.v");
                    self.m.write_gpr(block, reg, v);
                }
            }
            M::Xgetbv => {
                let which = self.m.read_gpr(block, R::ECX);
                let which = self.m.zext(block, Type::I32, which, Type::I64, "xcr");
                let v = self.m.intrinsic(block, "ra_xgetbv", Type::I64, vec![(Type::I64, which.into())], "xgetbv");
                let low = self.m.trunc(block, Type::I64, v, Type::I32, "xcr.lo");
                let shifted = self.m.b.bin(block, BinOp::LShr, Type::I64, v, 32i64, "xcr.hs");
                let high = self.m.trunc(block, Type::I64, shifted, Type::I32, "xcr.hi");
                self.m.write_gpr(block, R::EAX, low);
                self.m.write_gpr(block, R::EDX, high);
            }
            M::Rdtsc => {
                let v = self.m.intrinsic(block, "ra_rdtsc", Type::I64, vec![], "tsc");
                let low = self.m.trunc(block, Type::I64, v, Type::I32, "tsc.lo");
                let shifted = self.m.b.bin(block, BinOp::LShr, Type::I64, v, 32i64, "tsc.hs");
                let high = self.m.trunc(block, Type::I64, shifted, Type::I32, "tsc.hi");
                self.m.write_gpr(block, R::EAX, low);
                self.m.write_gpr(block, R::EDX, high);
            }
            M::Stmxcsr | M::Ldmxcsr => {
                let mxcsr = Operand::Address { global: crate::state::MXCSR.to_owned(), offset: 0 };
                let p = self.m.b.cast(block, Cast::IntToPtr, Type::I64, mxcsr, Type::Ptr, "mxcsr.p");
                let address = self.mem_address(block, i);
                if i.mnemonic() == M::Stmxcsr {
                    let v = self.m.b.load(block, Type::I32, p, "mxcsr");
                    self.m.store(block, Type::I32, v, address);
                } else {
                    let v = self.m.load(block, Type::I32, address, "mxcsr.m");
                    self.m.b.store(block, Type::I32, v, p);
                }
            }

            other => {
                self.warn(format!("{ip:#x}: {other:?} is not lifted yet"));
                self.m.unsupported(block, ip, i.code() as u64);
            }
        }
        block
    }

    /// Sign-extends an immediate or a narrower source to the destination width of a two-operand
    /// instruction (`add rax, imm32` holds a 32-bit immediate for a 64-bit add).
    fn widen_to(&mut self, block: BlockId, i: &Instruction, n: u32, value: ValueId, bits: u16) -> ValueId {
        let have = self.op_bits(i, n);
        match have.cmp(&bits) {
            std::cmp::Ordering::Equal => value,
            std::cmp::Ordering::Less => self.m.sext(block, Type::Int(have), value, Type::Int(bits), "w"),
            std::cmp::Ordering::Greater => self.m.trunc(block, Type::Int(have), value, Type::Int(bits), "n"),
        }
    }

    fn write_product(&mut self, block: BlockId, bits: u16, low: ValueId, high: ValueId) {
        if bits == 8 {
            let low_w = self.m.zext(block, Type::I8, low, Type::I16, "l");
            let high_w = self.m.zext(block, Type::I8, high, Type::I16, "h");
            let shifted = self.m.b.bin(block, BinOp::Shl, Type::I16, high_w, 8i64, "h.s");
            let ax = self.m.b.bin(block, BinOp::Or, Type::I16, low_w, shifted, "ax");
            self.m.write_gpr(block, R::AX, ax);
        } else {
            self.m.write_gpr(block, accumulator(bits), low);
            self.m.write_gpr(block, data_register(bits), high);
        }
    }

    #[allow(clippy::single_match_else)]
    fn imul(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        match i.op_count() {
            1 => {
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let wide = Type::Int(bits * 2);
                let a = self.m.read_gpr(block, accumulator(bits));
                let b = self.read(block, i, 0);
                let a_w = self.m.sext(block, ty.clone(), a, wide.clone(), "a");
                let b_w = self.m.sext(block, ty.clone(), b, wide.clone(), "b");
                let product = self.m.b.bin(block, BinOp::Mul, wide.clone(), a_w, b_w, "product");
                let low = self.m.trunc(block, wide.clone(), product, ty.clone(), "low");
                let shifted = self.m.b.bin(block, BinOp::LShr, wide.clone(), product, i64::from(bits), "high.s");
                let high = self.m.trunc(block, wide.clone(), shifted, ty.clone(), "high");
                self.write_product(block, bits, low, high);
                let back = self.m.sext(block, ty, low, wide.clone(), "back");
                let overflow = self.m.b.icmp(block, ICond::Ne, wide, back, product, "of");
                self.m.set_flag(block, Flag::Cf, overflow);
                self.m.set_flag(block, Flag::Of, overflow);
            }
            _ => {
                // imul r, r/m  or  imul r, r/m, imm
                let bits = self.op_bits(i, 0);
                let ty = Type::Int(bits);
                let wide = Type::Int(bits * 2);
                let (a, b) = if i.op_count() == 2 {
                    (self.read(block, i, 0), self.read(block, i, 1))
                } else {
                    let b = self.read(block, i, 2);
                    let b = self.widen_to(block, i, 2, b, bits);
                    (self.read(block, i, 1), b)
                };
                let a_w = self.m.sext(block, ty.clone(), a, wide.clone(), "a");
                let b_w = self.m.sext(block, ty.clone(), b, wide.clone(), "b");
                let product = self.m.b.bin(block, BinOp::Mul, wide.clone(), a_w, b_w, "product");
                let low = self.m.trunc(block, wide.clone(), product, ty.clone(), "low");
                self.write(block, i, 0, low);
                let back = self.m.sext(block, ty.clone(), low, wide.clone(), "back");
                let overflow = self.m.b.icmp(block, ICond::Ne, wide, back, product, "of");
                self.m.set_flag(block, Flag::Cf, overflow);
                self.m.set_flag(block, Flag::Of, overflow);
                self.m.set_result_flags(block, ty, low);
            }
        }
        block
    }

    /// `div` and `idiv`: the double-width dividend in `rdx:rax` over the operand. A 64-bit division
    /// whose high half is not the extension of the low half traps: the oracle has no 128-bit
    /// division runtime on this target.
    fn divide(&mut self, block: BlockId, i: &Instruction, signed: bool) -> BlockId {
        let bits = self.op_bits(i, 0);
        let ty = Type::Int(bits);
        let ip = i.ip();
        let divisor = self.read(block, i, 0);
        let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), divisor, 0i64, "div.zero");
        let trap_block = self.m.b.block("div.trap");
        self.m.trap(trap_block, ip, trap::DIVIDE);
        let go = self.m.b.block("div");
        self.m.b.terminate(block, Terminator::CondBr { cond: zero.into(), then: trap_block, els: go });
        let block = go;

        let (low, high) = if bits == 8 {
            let ax = self.m.read_gpr(block, R::AX);
            let low = self.m.trunc(block, Type::I16, ax, Type::I8, "al");
            let shifted = self.m.b.bin(block, BinOp::LShr, Type::I16, ax, 8i64, "ah.s");
            let high = self.m.trunc(block, Type::I16, shifted, Type::I8, "ah");
            (low, high)
        } else {
            (self.m.read_gpr(block, accumulator(bits)), self.m.read_gpr(block, data_register(bits)))
        };

        let (quotient, remainder, block) = if bits == 64 {
            // The fast and only path: the dividend fits in 64 bits.
            let expected = if signed {
                self.m.b.bin(block, BinOp::AShr, ty.clone(), low, 63i64, "sign")
            } else {
                self.m.value_of(block, ty.clone(), 0i64)
            };
            let fits = self.m.b.icmp(block, ICond::Eq, ty.clone(), high, expected, "fits");
            let wide_trap = self.m.b.block("div.wide");
            self.m.trap(wide_trap, ip, trap::DIVIDE_WIDE);
            let narrow = self.m.b.block("div.64");
            self.m.b.terminate(block, Terminator::CondBr { cond: fits.into(), then: narrow, els: wide_trap });
            let q = self.m.b.bin(narrow, if signed { BinOp::SDiv } else { BinOp::UDiv }, ty.clone(), low, divisor, "quot");
            let r = self.m.b.bin(narrow, if signed { BinOp::SRem } else { BinOp::URem }, ty.clone(), low, divisor, "rem");
            (q, r, narrow)
        } else {
            let wide = Type::Int(bits * 2);
            let low_w = self.m.zext(block, ty.clone(), low, wide.clone(), "low.w");
            let high_w = self.m.zext(block, ty.clone(), high, wide.clone(), "high.w");
            let shifted = self.m.b.bin(block, BinOp::Shl, wide.clone(), high_w, i64::from(bits), "high.s");
            let dividend = self.m.b.bin(block, BinOp::Or, wide.clone(), shifted, low_w, "dividend");
            let divisor_w = if signed {
                self.m.sext(block, ty.clone(), divisor, wide.clone(), "divisor")
            } else {
                self.m.zext(block, ty.clone(), divisor, wide.clone(), "divisor")
            };
            let q_w = self.m.b.bin(block, if signed { BinOp::SDiv } else { BinOp::UDiv }, wide.clone(), dividend, divisor_w, "quot.w");
            let r_w = self.m.b.bin(block, if signed { BinOp::SRem } else { BinOp::URem }, wide.clone(), dividend, divisor_w, "rem.w");
            let q = self.m.trunc(block, wide.clone(), q_w, ty.clone(), "quot");
            let r = self.m.trunc(block, wide, r_w, ty.clone(), "rem");
            (q, r, block)
        };
        if bits == 8 {
            self.write_product(block, 8, quotient, remainder);
        } else {
            self.m.write_gpr(block, accumulator(bits), quotient);
            self.m.write_gpr(block, data_register(bits), remainder);
        }
        block
    }

    fn bit_test(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        use Mnemonic as M;
        let bits = self.op_bits(i, 0);
        let ty = Type::Int(bits);
        let offset = self.read(block, i, 1);
        let offset = self.widen_to(block, i, 1, offset, bits);
        // A memory operand with a register offset addresses the bit anywhere past the operand.
        let (value, address) = if i.op_kind(0) == OpKind::Memory && i.op_kind(1) == OpKind::Register {
            let base = self.mem_address(block, i);
            let off_a = self.m.sext(block, ty.clone(), offset, self.addr_ty(), "bit.off");
            let byte_index = self.m.b.bin(block, BinOp::AShr, self.addr_ty(), off_a, 3i64, "bit.byte");
            let address = self.m.b.bin(block, BinOp::Add, self.addr_ty(), base, byte_index, "bit.addr");
            let value = self.m.load(block, Type::I8, address, "bit.m");
            (value, Some(address))
        } else {
            (self.read(block, i, 0), None)
        };
        let (vty, mask_bits) = if address.is_some() { (Type::I8, 7i64) } else { (ty.clone(), i64::from(bits) - 1) };
        let off_t = if address.is_some() { self.m.trunc(block, ty, offset, Type::I8, "off8") } else { offset };
        let index = self.m.b.bin(block, BinOp::And, vty.clone(), off_t, mask_bits, "bit.i");
        let one = self.m.value_of(block, vty.clone(), 1i64);
        let mask = self.m.b.bin(block, BinOp::Shl, vty.clone(), one, index, "bit.mask");
        let selected = self.m.b.bin(block, BinOp::And, vty.clone(), value, mask, "bit.sel");
        let cf = self.m.is_true(block, vty.clone(), selected, "cf");
        self.m.set_flag(block, Flag::Cf, cf);
        let new = match i.mnemonic() {
            M::Bts => Some(self.m.b.bin(block, BinOp::Or, vty.clone(), value, mask, "bts")),
            M::Btr => {
                let inv = self.m.b.bin(block, BinOp::Xor, vty.clone(), mask, -1i64, "inv");
                Some(self.m.b.bin(block, BinOp::And, vty.clone(), value, inv, "btr"))
            }
            M::Btc => Some(self.m.b.bin(block, BinOp::Xor, vty.clone(), value, mask, "btc")),
            _ => None,
        };
        if let Some(new) = new {
            match address {
                Some(a) => self.m.store(block, vty, new, a),
                None => self.write(block, i, 0, new),
            }
        }
        block
    }

    /// The string instructions, with their `rep` loop when they have one.
    fn string_op(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        use Mnemonic as M;
        let bytes = match i.mnemonic() {
            M::Movsb | M::Stosb | M::Lodsb | M::Scasb | M::Cmpsb => 1,
            M::Movsw | M::Stosw | M::Lodsw | M::Scasw | M::Cmpsw => 2,
            M::Movsd | M::Stosd | M::Lodsd | M::Scasd | M::Cmpsd => 4,
            _ => 8,
        };
        let ty = Type::Int(bytes * 8);
        let aty = self.addr_ty();
        let (si, di, cx) = if self.m.bits == 64 { (R::RSI, R::RDI, R::RCX) } else { (R::ESI, R::EDI, R::ECX) };
        let repeated = i.has_rep_prefix() || i.has_repe_prefix() || i.has_repne_prefix();
        let compares = matches!(i.mnemonic(), M::Scasb | M::Scasw | M::Scasd | M::Scasq | M::Cmpsb | M::Cmpsw | M::Cmpsd | M::Cmpsq);

        let (head, body, exit) = if repeated {
            let head = self.m.b.block("rep.head");
            let body = self.m.b.block("rep.body");
            let exit = self.m.b.block("rep.exit");
            self.m.b.terminate(block, Terminator::Br(head));
            let count = self.m.read_gpr(head, cx);
            let done = self.m.b.icmp(head, ICond::Eq, aty.clone(), count, 0i64, "rep.done");
            self.m.b.terminate(head, Terminator::CondBr { cond: done.into(), then: exit, els: body });
            (Some(head), body, exit)
        } else {
            (None, block, block)
        };

        // The step: +size or -size by DF.
        let df = self.m.get_flag(body, Flag::Df);
        let step = self.m.select(body, df, aty.clone(), -i64::from(bytes), i64::from(bytes), "step");
        let si_v = self.m.read_gpr(body, si);
        let di_v = self.m.read_gpr(body, di);
        match i.mnemonic() {
            M::Movsb | M::Movsw | M::Movsd | M::Movsq => {
                let v = self.m.load(body, ty.clone(), si_v, "s");
                self.m.store(body, ty, v, di_v);
                let nsi = self.m.b.bin(body, BinOp::Add, aty.clone(), si_v, step, "si");
                let ndi = self.m.b.bin(body, BinOp::Add, aty.clone(), di_v, step, "di");
                self.m.write_gpr(body, si, nsi);
                self.m.write_gpr(body, di, ndi);
            }
            M::Stosb | M::Stosw | M::Stosd | M::Stosq => {
                let a = self.m.read_gpr(body, accumulator(bytes * 8));
                self.m.store(body, ty, a, di_v);
                let ndi = self.m.b.bin(body, BinOp::Add, aty.clone(), di_v, step, "di");
                self.m.write_gpr(body, di, ndi);
            }
            M::Lodsb | M::Lodsw | M::Lodsd | M::Lodsq => {
                let v = self.m.load(body, ty, si_v, "s");
                self.m.write_gpr(body, accumulator(bytes * 8), v);
                let nsi = self.m.b.bin(body, BinOp::Add, aty.clone(), si_v, step, "si");
                self.m.write_gpr(body, si, nsi);
            }
            M::Scasb | M::Scasw | M::Scasd | M::Scasq => {
                let a = self.m.read_gpr(body, accumulator(bytes * 8));
                let v = self.m.load(body, ty.clone(), di_v, "d");
                self.sub(body, ty, a, v, None);
                let ndi = self.m.b.bin(body, BinOp::Add, aty.clone(), di_v, step, "di");
                self.m.write_gpr(body, di, ndi);
            }
            _ => {
                let a = self.m.load(body, ty.clone(), si_v, "s");
                let v = self.m.load(body, ty.clone(), di_v, "d");
                self.sub(body, ty, a, v, None);
                let nsi = self.m.b.bin(body, BinOp::Add, aty.clone(), si_v, step, "si");
                let ndi = self.m.b.bin(body, BinOp::Add, aty.clone(), di_v, step, "di");
                self.m.write_gpr(body, si, nsi);
                self.m.write_gpr(body, di, ndi);
            }
        }
        if let Some(head) = head {
            let count = self.m.read_gpr(body, cx);
            let dec = self.m.b.bin(body, BinOp::Sub, aty, count, 1i64, "cx");
            self.m.write_gpr(body, cx, dec);
            if compares && (i.has_repe_prefix() || i.has_repne_prefix()) {
                let zf = self.m.get_flag(body, Flag::Zf);
                let stop = if i.has_repne_prefix() { zf } else { self.m.b.bin(body, BinOp::Xor, Type::I1, zf, 1i64, "nz") };
                self.m.b.terminate(body, Terminator::CondBr { cond: stop.into(), then: exit, els: head });
            } else {
                self.m.b.terminate(body, Terminator::Br(head));
            }
        }
        exit
    }

    // region Vectors

    /// Reads operand `n` as a vector or float type: the low bytes of a register, or memory.
    fn read_vec(&mut self, block: BlockId, i: &Instruction, n: u32, ty: Type) -> ValueId {
        match i.op_kind(n) {
            OpKind::Register => {
                let r = i.op_register(n);
                if r.is_xmm() {
                    self.m.read_xmm(block, r, ty)
                } else {
                    // A general register into a vector type of the same width.
                    let v = self.m.read_gpr(block, r);
                    self.m.b.cast(block, Cast::Bitcast, Type::Int((r.size() * 8) as u16), v, ty, "asvec")
                }
            }
            OpKind::Memory => {
                let addr = self.mem_address(block, i);
                self.m.load(block, ty, addr, "vm")
            }
            _ => {
                let v = Self::immediate(i, n);
                self.m.value_of(block, ty, v)
            }
        }
    }

    fn write_vec(&mut self, block: BlockId, i: &Instruction, n: u32, ty: Type, value: ValueId) {
        match i.op_kind(n) {
            OpKind::Register => {
                let r = i.op_register(n);
                if r.is_xmm() {
                    self.m.write_xmm(block, r, ty, value, true);
                } else {
                    let bits = (r.size() * 8) as u16;
                    let v = self.m.b.cast(block, Cast::Bitcast, ty, value, Type::Int(bits), "asint");
                    self.m.write_gpr(block, r, v);
                }
            }
            OpKind::Memory => {
                let addr = self.mem_address(block, i);
                self.m.store(block, ty, value, addr);
            }
            _ => {}
        }
    }

    fn splat(&mut self, block: BlockId, ty: &Type, lanes: u32, value: i64) -> ValueId {
        let mut v = self.m.value_of(block, ty.clone(), 0i64);
        for k in 0..lanes {
            v = self.m.b.value(
                block,
                Inst::InsertElement {
                    ty: ty.clone(),
                    vector: v.into(),
                    element: Operand::Int(i128::from(value)),
                    index: Operand::Int(i128::from(k)),
                },
                "splat",
            );
        }
        v
    }

    /// The vector and scalar moves: `movd`, `movq`, the aligned and unaligned 128-bit moves,
    /// `movss` and `movsd`, and the half moves.
    fn vector_move(&mut self, block: BlockId, i: &Instruction) -> BlockId {
        use Mnemonic as M;
        let dst = i.op_register(0);
        let src = i.op_register(1);
        match i.mnemonic() {
            M::Movd | M::Movq => {
                let bits = if i.mnemonic() == M::Movd { 32 } else { 64 };
                let ty = Type::Int(bits);
                let v = if i.op_kind(1) == OpKind::Register && src.is_xmm() {
                    self.m.read_xmm(block, src, ty.clone())
                } else {
                    self.read_as(block, i, 1, ty.clone())
                };
                if i.op_kind(0) == OpKind::Register && dst.is_xmm() {
                    self.m.write_xmm(block, dst, ty, v, true);
                } else {
                    self.write_as(block, i, 0, ty, v);
                }
            }
            M::Movdqa | M::Movdqu | M::Movaps | M::Movups | M::Movapd | M::Movupd | M::Lddqu => {
                let v = self.read_vec(block, i, 1, Type::I128);
                self.write_vec(block, i, 0, Type::I128, v);
            }
            M::Movss | M::Movsd => {
                let ty = if i.mnemonic() == M::Movss { Type::Float } else { Type::Double };
                let v = self.read_vec(block, i, 1, ty.clone());
                let both_registers = i.op_kind(0) == OpKind::Register && i.op_kind(1) == OpKind::Register;
                if i.op_kind(0) == OpKind::Register {
                    // From memory the upper lanes clear; between registers they stay.
                    self.m.write_xmm(block, dst, ty, v, !both_registers);
                } else {
                    self.write_vec(block, i, 0, ty, v);
                }
            }
            M::Movlps | M::Movlpd => {
                let v = self.read_vec(block, i, 1, Type::I64);
                if i.op_kind(0) == OpKind::Register {
                    self.m.write_xmm(block, dst, Type::I64, v, false);
                } else {
                    self.write_vec(block, i, 0, Type::I64, v);
                }
            }
            M::Movhps | M::Movhpd => {
                if i.op_kind(0) == OpKind::Register {
                    let low = self.m.read_xmm(block, dst, Type::I64);
                    let high = self.read_vec(block, i, 1, Type::I64);
                    let v2 = Type::vector(2, Type::I64);
                    let z = self.m.value_of(block, v2.clone(), 0i64);
                    let a = self.m.b.value(
                        block,
                        Inst::InsertElement { ty: v2.clone(), vector: z.into(), element: low.into(), index: Operand::Int(0) },
                        "mh",
                    );
                    let b = self.m.b.value(
                        block,
                        Inst::InsertElement { ty: v2.clone(), vector: a.into(), element: high.into(), index: Operand::Int(1) },
                        "mh",
                    );
                    self.m.write_xmm(block, dst, v2, b, true);
                } else {
                    let v2 = Type::vector(2, Type::I64);
                    let whole = self.m.read_xmm(block, src, v2.clone());
                    let high = self.m.b.value(block, Inst::ExtractElement { ty: v2, vector: whole.into(), index: Operand::Int(1) }, "high");
                    self.write_vec(block, i, 0, Type::I64, high);
                }
            }
            _ => {
                // mov between a general register and an xmm never happens; a mov with an xmm
                // operand is one of the forms above.
                self.warn(format!("{:#x}: {:?} with a vector operand is not lifted", i.ip(), i.mnemonic()));
                self.m.unsupported(block, i.ip(), i.code() as u64);
            }
        }
        block
    }

    /// Reads a general register or memory operand as an integer of `ty`'s width.
    fn read_as(&mut self, block: BlockId, i: &Instruction, n: u32, ty: Type) -> ValueId {
        let v = self.read(block, i, n);
        let have = Type::Int(self.op_bits(i, n));
        if have == ty {
            v
        } else if have.bits() < ty.bits() {
            self.m.zext(block, have, v, ty, "w")
        } else {
            self.m.trunc(block, have, v, ty, "n")
        }
    }

    fn write_as(&mut self, block: BlockId, i: &Instruction, n: u32, ty: Type, value: ValueId) {
        let want = Type::Int(self.op_bits(i, n));
        let v = if want == ty {
            value
        } else if want.bits() > ty.bits() {
            self.m.zext(block, ty, value, want, "w")
        } else {
            self.m.trunc(block, ty, value, want, "n")
        };
        self.write(block, i, n, v);
    }

    // endregion

    // region Flags as a word

    fn pack_flags(&mut self, block: BlockId) -> ValueId {
        let ty = self.addr_ty();
        let mut acc = self.m.value_of(block, ty.clone(), 0x202i64);
        for (flag, bit) in [(Flag::Cf, 0), (Flag::Pf, 2), (Flag::Af, 4), (Flag::Zf, 6), (Flag::Sf, 7), (Flag::Df, 10), (Flag::Of, 11)] {
            let f = self.m.get_flag(block, flag);
            let wide = self.m.zext(block, Type::I1, f, ty.clone(), "f.w");
            let placed = self.m.b.bin(block, BinOp::Shl, ty.clone(), wide, bit, "f.p");
            acc = self.m.b.bin(block, BinOp::Or, ty.clone(), acc, placed, "flags");
        }
        acc
    }

    fn unpack_flags(&mut self, block: BlockId, word: ValueId) {
        let ty = self.addr_ty();
        for (flag, bit) in [(Flag::Cf, 0), (Flag::Pf, 2), (Flag::Af, 4), (Flag::Zf, 6), (Flag::Sf, 7), (Flag::Df, 10), (Flag::Of, 11)] {
            let shifted = self.m.b.bin(block, BinOp::LShr, ty.clone(), word, bit, "f.s");
            let one = self.m.b.bin(block, BinOp::And, ty.clone(), shifted, 1i64, "f.b");
            let v = self.m.is_true(block, ty.clone(), one, "f");
            self.m.set_flag(block, flag, v);
        }
    }

    // endregion
    // endregion
}

/// Whether a two-operand instruction names the same register twice.
fn same_register(i: &Instruction) -> bool {
    i.op_count() == 2 && i.op_kind(0) == OpKind::Register && i.op_kind(1) == OpKind::Register && i.op_register(0) == i.op_register(1)
}

/// The accumulator of a width: `al`, `ax`, `eax`, `rax`.
fn accumulator(bits: u16) -> R {
    match bits {
        8 => R::AL,
        16 => R::AX,
        32 => R::EAX,
        _ => R::RAX,
    }
}

/// The data register of a width: `dl`, `dx`, `edx`, `rdx`.
fn data_register(bits: u16) -> R {
    match bits {
        8 => R::DL,
        16 => R::DX,
        32 => R::EDX,
        _ => R::RDX,
    }
}

/// The symbol an import links as. An import by ordinal has no symbol: it gets a name the runtime
/// reports as missing.
pub(crate) fn import_symbol(display_name: &str) -> String {
    if display_name.contains("!#") { format!("ra_ordinal_{}", display_name.replace(['!', '#', '.'], "_")) } else { display_name.to_owned() }
}
