//! The Thumb-2 semantics: one instruction of [`ra_asm_arm64::thumb`] becomes IR over the
//! [`Machine`] state.
//!
//! The state holds `r0..r15` in the first sixteen general slots, each zero-extended to 64 bits,
//! and N, Z, C and V in the first four flag bytes, as ARM64 keeps them. The integer subset the
//! decoder gives is covered: the data processing with its shifter and its flags, the moves of
//! halves, the multiplies and divides, the extends and the bitfields, the single, dual and
//! multiple loads and stores, the branches and calls, the VFP unit, and `svc`, which goes to the
//! runtime. An instruction an IT block predicates runs behind a branch on its condition; one
//! that leaves the function (a return, a branch through a register) leaves inside the branch,
//! and the flow goes on past it. An encoding outside the subset traps.
//!
//! The same lift reads ARM code, an image's or a block's ([`ra_asm_arm64::arm`]): a word an
//! instruction, each under its own condition, the PC its address and eight. In a program of
//! both sets a call or a branch through a register goes to either.
//!
//! A literal load reads the pool where it lies in the mapped image at the machine levels; in a
//! standalone lift the word is the constant it is, or the address of what it names, and a switch
//! through a table in the code is a switch on its index.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, Function, Terminator as T};
use ra_asm_arch::Instr;
use ra_asm_arm64::thumb::{self, Alu, Bits, Decoded, LR, Offset, Op, PC, Shift, Source};
use ra_asm_arm64::vfp::Fp;
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FCond, FnType, FunctionBuilder, ICond, Inst, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;

use crate::state::{Isa, Machine};
use crate::x86::trap;
use crate::{Lifted, function_name};

/// The flags, by their byte index in the state, and their names.
const N: i64 = 0;
const Z: i64 = 1;
const C: i64 = 2;
const V: i64 = 3;
const FLAG_NAMES: [&str; 4] = ["n", "z", "c", "v"];

/// Lifts one function.
pub fn lift_function(image: &Image, _analysis: &Analysis<'_>, lifted: &mut Lifted, f: &Function) -> ra_asm_ir::Function {
    let name = function_name(f.entry);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_address(f.entry);
    if f.noreturn {
        b.add_attr("noreturn");
    }
    let state = b.param(0);
    let mut blocks = HashMap::new();
    blocks.insert(f.entry, b.block(&format!("b_{:x}", f.entry)));
    for blk in &f.blocks {
        if blk.start != f.entry {
            blocks.insert(blk.start, b.block(&format!("b_{:x}", blk.start)));
        }
    }
    let x87 = lifted.x87;
    let mut lifter = Lifter {
        m: Machine { b: &mut b, state, bits: 32, x87, isa: Isa::Arm32 },
        image,
        arm: image.arm_at(f.entry),
        lifted,
        blocks,
        cond: None,
        jump_target: None,
        switch_index: None,
    };
    for blk in &f.blocks {
        lifter.lift_block(blk);
    }
    let mut built = b.finish();
    // A function that never returns has no `ret` to come back through.
    if f.noreturn {
        for block in &mut built.blocks {
            if let Terminator::Ret(_) = block.terminator {
                block.terminator = Terminator::Unreachable;
            }
        }
    }
    built
}

struct Lifter<'a, 'b> {
    m: Machine<'b>,
    image: &'a Image,
    /// Whether the block being lifted is ARM code: a word an instruction, its condition in it,
    /// the PC it reads its address and eight. The operations are Thumb's.
    arm: bool,
    lifted: &'a mut Lifted,
    blocks: HashMap<u64, BlockId>,
    /// The condition of the conditional branch that ends the current block.
    cond: Option<ValueId>,
    /// The target of the indirect branch that ends the current block, as a 64-bit address.
    jump_target: Option<ValueId>,
    /// The index of the switch that ends the current block, where the lift switches on it.
    switch_index: Option<ValueId>,
}

impl Lifter<'_, '_> {
    fn warn(&mut self, text: String) {
        self.lifted.warnings.push(text);
    }

    // region Blocks

    fn lift_block(&mut self, blk: &Block) {
        let mut block = self.blocks[&blk.start];
        self.m.b.set_block_address(block, blk.start);
        self.cond = None;
        self.jump_target = None;
        self.arm = self.image.arm_at(blk.start);
        // A switch through a table that lies in the code: a standalone port has no code to read
        // the table from, so the instructions that read it are left out and the switch is on
        // the index they start from, which the analysis mapped to the cases.
        let table = match &blk.terminator {
            T::Switch { load_at, index, .. } if self.lifted.standalone => Some((*load_at, index.id as u8)),
            _ => None,
        };
        self.switch_index = None;
        for (n, i) in blk.instrs.iter().enumerate() {
            if self.m.b.has_terminator(block) {
                break;
            }
            if let Some((load_at, index)) = table
                && i.address == load_at
            {
                self.switch_index = Some(self.reg(block, index, i.address));
                break;
            }
            if self.lifted.standalone
                && let Some((rd, address)) = self.pc_relative_address(&blk.instrs[..n], i)
            {
                self.set_reg(block, rd, address);
                continue;
            }
            self.m.b.address = Some(i.address);
            let d = if self.arm {
                ra_asm_arm64::arm::decode(i.code, i.address)
            } else {
                let it = thumb::it_condition(&blk.instrs[..n]);
                let Some(d) = thumb::decode_raw(i.code, i.address, it) else { break };
                d
            };
            block = self.lift_predicated(block, &d, i);
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
                    self.warn(format!("{last:#x}: a conditional branch with no condition"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                let then = self.target_block(*taken);
                let els = self.target_block(*fallthrough);
                self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then, els });
            }
            T::Switch { targets, cases, .. } => {
                let default = self.m.b.block("switch.bad");
                self.m.trap(default, last, trap::BAD_SWITCH);
                if let Some(index) = self.switch_index.take() {
                    let cases = cases.iter().filter_map(|(n, t)| self.blocks.get(t).map(|b| (i128::from(*n), *b))).collect();
                    self.m.b.terminate(block, Terminator::Switch { ty: Type::I32, value: index.into(), default, cases });
                    return;
                }
                let Some(target) = self.jump_target else {
                    self.warn(format!("{last:#x}: a switch with no computed target"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                let cases = targets.iter().filter_map(|t| self.blocks.get(t).map(|b| (i128::from(*t), *b))).collect();
                self.m.b.terminate(block, Terminator::Switch { ty: Type::I64, value: target.into(), default, cases });
            }
            T::IndirectJump => {
                let Some(target) = self.jump_target else {
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                // A branch the analysis did not resolve: most often a tail call through a pointer.
                self.runtime_call(block, target);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::TailCall(t) => {
                self.call_lifted(block, *t);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::ImportTailCall(import) => {
                self.warn(format!("{last:#x}: a tail call of the import {import}, which the Thumb-2 lift does not bind"));
                self.m.trap(block, last, trap::INVALID);
            }
            T::NoReturnCall(_) | T::NoReturnImportCall(_) => self.m.b.terminate(block, Terminator::Unreachable),
            T::Return => self.m.b.terminate(block, Terminator::Ret(None)),
            T::Trap => self.m.trap(block, last, trap::UD2),
            T::Halt => self.m.trap(block, last, trap::HLT),
            T::Invalid => self.m.trap(block, last, trap::INVALID),
        }
    }

    /// The address a position-independent program forms of something of its own: a word of the
    /// literal pool, the distance from the code to it, and an `add` of the PC to the register
    /// that holds the word. For such an `add` whose register the block last loaded from the
    /// pool, the register and the address of what the sum names, as a standalone port names it.
    fn pc_relative_address(&self, before: &[Instr], add: &Instr) -> Option<(u8, Operand)> {
        use ra_asm_arch::Access;
        let decode = |i: &Instr| {
            if self.arm { Some(ra_asm_arm64::arm::decode(i.code, i.address)) } else { thumb::decode_raw(i.code, i.address, None) }
        };
        let d = decode(add)?;
        if d.cond.is_some() {
            return None;
        }
        // `add rd, pc` in Thumb; `add rd, pc, rd` or `add rd, rd, pc` in ARM code.
        let Op::Alu { op: Alu::Add, s: false, rd, rn, src: Source::Reg { rm, shift: Shift::Lsl, amount: 0 } } = d.op else { return None };
        if rd == PC || !((rn == PC && rm == rd) || (rm == PC && rn == rd)) {
            return None;
        }
        let writes =
            |i: &Instr| i.registers.iter().any(|(r, access)| r.id == u16::from(rd) && matches!(access, Access::Write | Access::ReadWrite));
        let load = before.iter().rev().find(|i| writes(i))?;
        let Op::Mem { load: true, bits: 32, rt, rn: PC, offset: Offset::Imm(_), index: true, wback: false, .. } = decode(load)?.op else {
            return None;
        };
        let word = self.image.u32(load.absolute_memory_operand()?).ok()?;
        let pc = add.address + if self.arm { 8 } else { 4 };
        let address = pc.wrapping_add(u64::from(word)) & 0xffff_ffff;
        (rt == rd).then(|| self.lifted.address_of(address)).flatten().map(|operand| (rd, operand))
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

    /// An instruction under the condition of its IT block runs behind a branch on it. A
    /// conditional branch's condition is the block's own, which its terminator consumes.
    fn lift_predicated(&mut self, block: BlockId, d: &Decoded, i: &Instr) -> BlockId {
        let Some(cond) = d.cond else { return self.lift_instr(block, d, i) };
        if matches!(d.op, Op::Branch { .. }) {
            return self.lift_instr(block, d, i);
        }
        if !i.flow.falls_through() || matches!(d.op, Op::It { .. }) {
            return self.unsupported(block, i);
        }
        let ip = i.address;
        let then = self.m.b.block(&format!("if_{ip:x}"));
        let next = self.m.b.block(&format!("fi_{ip:x}"));
        let c = self.condition(block, cond);
        self.m.b.terminate(block, Terminator::CondBr { cond: c.into(), then, els: next });
        let end = self.lift_instr(then, d, i);
        if !self.m.b.has_terminator(end) {
            // Code leaves a function under a condition, in ARM code by the instruction's own
            // and in Thumb by an IT block's: a return, or a jump through a register, which is a
            // call of where it goes and a return.
            let returns = match d.op {
                Op::BranchReg { rm: LR, link: false } => true,
                Op::Multiple { load: true, list, .. } => list & (1 << PC) != 0,
                Op::Alu { op: Alu::Mov, rd: PC, src: Source::Reg { rm: LR, .. }, .. } => true,
                _ => false,
            };
            let jumps = self.jump_target.take();
            if returns {
                self.m.b.terminate(end, Terminator::Ret(None));
            } else if let Some(target) = jumps {
                self.runtime_call(end, target);
                self.m.b.terminate(end, Terminator::Ret(None));
            }
        }
        if !self.m.b.has_terminator(end) {
            self.m.b.terminate(end, Terminator::Br(next));
        }
        next
    }

    // endregion

    // region Calls

    /// `call F_target(state)`.
    fn call_lifted(&mut self, block: BlockId, target: u64) {
        let name = function_name(target);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// `call @ra_call(state, target)`: the runtime finds the lifted function at the address.
    fn runtime_call(&mut self, block: BlockId, target: ValueId) {
        self.m.b.call(
            block,
            FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
            Callee::Direct("ra_call".into()),
            vec![(Type::Ptr, self.m.state.into()), (Type::I64, target.into())],
            "",
        );
    }

    /// The code address a register's value names, as a 64-bit value in the block the lift goes
    /// on in: the low bit set says Thumb and is dropped; clear, it says ARM code, which traps.
    fn thumb_target(&mut self, block: BlockId, target: ValueId, ip: u64) -> (BlockId, ValueId) {
        if self.image.arm_and_thumb() {
            // A program of both sets goes to either: the bit says which, and is no part of the address.
            let code = self.m.b.bin(block, BinOp::And, Type::I32, target, -2i64, "code");
            return (block, self.m.b.cast(block, Cast::ZExt, Type::I32, code, Type::I64, "target"));
        }
        let bit = self.m.b.cast(block, Cast::Trunc, Type::I32, target, Type::I1, "thumb");
        let ok = self.m.b.block(&format!("thumb_{ip:x}"));
        let arm = self.m.b.block(&format!("arm_{ip:x}"));
        // In ARM code it is the other way round: the bit set says Thumb code, which traps.
        let (set, clear) = if self.arm { (arm, ok) } else { (ok, arm) };
        self.m.b.terminate(block, Terminator::CondBr { cond: bit.into(), then: set, els: clear });
        self.m.trap(arm, ip, trap::INVALID);
        let code = self.m.b.bin(ok, BinOp::And, Type::I32, target, -2i64, "code");
        let wide = self.m.b.cast(ok, Cast::ZExt, Type::I32, code, Type::I64, "target");
        (ok, wide)
    }

    // endregion

    // region Registers and flags

    /// The value of core register `n`; the PC reads as the instruction's address plus four.
    fn reg(&mut self, block: BlockId, n: u8, ip: u64) -> ValueId {
        if n == PC {
            return self.m.value_of(block, Type::I32, (ip + if self.arm { 8 } else { 4 }) as i64);
        }
        let p = self.m.gpr_ptr(block, u32::from(n));
        let whole = self.m.b.load(block, Type::I64, p, self.m.gpr_name(u32::from(n)));
        self.m.b.cast(block, Cast::Trunc, Type::I64, whole, Type::I32, thumb::register(n).name)
    }

    /// Writes core register `n`, zero-extended into its slot.
    fn set_reg(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        let p = self.m.gpr_ptr(block, u32::from(n));
        let wide = self.m.b.cast(block, Cast::ZExt, Type::I32, value, Type::I64, &format!("{}.x", thumb::register(n).name));
        self.m.b.store(block, Type::I64, wide, p);
    }

    fn flag(&mut self, block: BlockId, index: i64) -> ValueId {
        let name = FLAG_NAMES[index as usize];
        let p = self.m.flag_ptr_at(block, index, name);
        let byte = self.m.b.load(block, Type::I8, p, &format!("{name}.b"));
        self.m.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, name)
    }

    fn set_flag(&mut self, block: BlockId, index: i64, value: impl Into<Operand>) {
        let name = FLAG_NAMES[index as usize];
        let p = self.m.flag_ptr_at(block, index, name);
        let byte = self.m.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{name}.b"));
        self.m.b.store(block, Type::I8, byte, p);
    }

    fn set_nz(&mut self, block: BlockId, result: ValueId) {
        let n = self.m.b.icmp(block, ICond::Slt, Type::I32, result, 0i64, "n");
        self.set_flag(block, N, n);
        let z = self.m.b.icmp(block, ICond::Eq, Type::I32, result, 0i64, "z");
        self.set_flag(block, Z, z);
    }

    /// The truth of condition `cc`, from the flags.
    fn condition(&mut self, block: BlockId, cc: u8) -> ValueId {
        let base = match cc >> 1 {
            0 => self.flag(block, Z),
            1 => self.flag(block, C),
            2 => self.flag(block, N),
            3 => self.flag(block, V),
            4 => {
                let c = self.flag(block, C);
                let z = self.flag(block, Z);
                let nz = self.m.b.bin(block, BinOp::Xor, Type::I1, z, 1i64, "nz");
                self.m.b.bin(block, BinOp::And, Type::I1, c, nz, "hi")
            }
            5 => {
                let n = self.flag(block, N);
                let v = self.flag(block, V);
                self.m.b.icmp(block, ICond::Eq, Type::I1, n, v, "ge")
            }
            6 => {
                let n = self.flag(block, N);
                let v = self.flag(block, V);
                let ge = self.m.b.icmp(block, ICond::Eq, Type::I1, n, v, "ge");
                let z = self.flag(block, Z);
                let nz = self.m.b.bin(block, BinOp::Xor, Type::I1, z, 1i64, "nz");
                self.m.b.bin(block, BinOp::And, Type::I1, ge, nz, "gt")
            }
            _ => return self.m.value_of(block, Type::I1, 1i64),
        };
        if cc & 1 == 1 { self.m.b.bin(block, BinOp::Xor, Type::I1, base, 1i64, "not") } else { base }
    }

    /// Bit `k` of a 32-bit value, as an `i1`.
    fn bit(&mut self, block: BlockId, value: ValueId, k: u8) -> ValueId {
        let shifted = if k == 0 { value } else { self.m.b.bin(block, BinOp::LShr, Type::I32, value, i64::from(k), "bit") };
        self.m.b.cast(block, Cast::Trunc, Type::I32, shifted, Type::I1, "bit.1")
    }

    // endregion

    // region The shifter and the adder

    /// The second source of a data-processing instruction, with the shifter's carry out when
    /// `want_carry`; `None` for the carry when the shifter leaves C as it is.
    fn shifted(&mut self, block: BlockId, src: Source, ip: u64, want_carry: bool) -> (ValueId, Option<ValueId>) {
        let ty = Type::I32;
        match src {
            Source::Imm { value, carry } => {
                let v = self.m.value_of(block, ty, i64::from(value as i32));
                let c = if want_carry { carry.map(|c| self.m.value_of(block, Type::I1, i64::from(c))) } else { None };
                (v, c)
            }
            Source::Reg { rm, shift, amount } => {
                let v = self.reg(block, rm, ip);
                let k = i64::from(amount);
                let (result, carry_bit): (ValueId, Option<(ValueId, u8)>) = match (shift, amount) {
                    (Shift::Lsl, 0) => (v, None),
                    (Shift::Lsl, _) => (self.m.b.bin(block, BinOp::Shl, ty, v, k, "lsl"), Some((v, 32 - amount))),
                    (Shift::Lsr, 32) => (self.m.value_of(block, ty, 0i64), Some((v, 31))),
                    (Shift::Lsr, _) => (self.m.b.bin(block, BinOp::LShr, ty, v, k, "lsr"), Some((v, amount - 1))),
                    (Shift::Asr, 32) => (self.m.b.bin(block, BinOp::AShr, ty, v, 31i64, "asr"), Some((v, 31))),
                    (Shift::Asr, _) => (self.m.b.bin(block, BinOp::AShr, ty, v, k, "asr"), Some((v, amount - 1))),
                    (Shift::Ror, _) => {
                        let low = self.m.b.bin(block, BinOp::LShr, ty.clone(), v, k, "ror.lo");
                        let high = self.m.b.bin(block, BinOp::Shl, ty.clone(), v, 32 - k, "ror.hi");
                        let r = self.m.b.bin(block, BinOp::Or, ty, low, high, "ror");
                        (r, Some((r, 31)))
                    }
                    (Shift::Rrx, _) => {
                        let c = self.flag(block, C);
                        let c32 = self.m.b.cast(block, Cast::ZExt, Type::I1, c, ty.clone(), "c.w");
                        let top = self.m.b.bin(block, BinOp::Shl, ty.clone(), c32, 31i64, "rrx.hi");
                        let low = self.m.b.bin(block, BinOp::LShr, ty.clone(), v, 1i64, "rrx.lo");
                        (self.m.b.bin(block, BinOp::Or, ty, low, top, "rrx"), Some((v, 0)))
                    }
                };
                let carry = match carry_bit {
                    Some((from, k)) if want_carry => Some(self.bit(block, from, k)),
                    _ => None,
                };
                (result, carry)
            }
            Source::RegReg { rm, shift, rs } => self.shifted_by_register(block, rm, shift, rs, ip, want_carry),
        }
    }

    /// A register shifted by the low byte of another: at 64 bits, with the amount held where the
    /// result and the carry no longer change, so no shift of the IR is by its width or more.
    fn shifted_by_register(
        &mut self,
        block: BlockId,
        rm: u8,
        shift: Shift,
        rs: u8,
        ip: u64,
        want_carry: bool,
    ) -> (ValueId, Option<ValueId>) {
        let (i32t, i64t) = (Type::I32, Type::I64);
        let v = self.reg(block, rm, ip);
        let s = self.reg(block, rs, ip);
        let byte = self.m.b.bin(block, BinOp::And, i32t.clone(), s, 0xffi64, "amt");
        let amount = self.m.b.cast(block, Cast::ZExt, i32t.clone(), byte, i64t.clone(), "amt.w");
        let clamp = |this: &mut Self, at: i64| {
            let over = this.m.b.icmp(block, ICond::Ugt, Type::I64, amount, at, "amt.over");
            this.m.select(block, over, Type::I64, at, amount, "amt.c")
        };
        let (wide, carry_source): (ValueId, ValueId) = match shift {
            Shift::Lsl => {
                let k = clamp(self, 33);
                let w = self.m.b.cast(block, Cast::ZExt, i32t.clone(), v, i64t.clone(), "v.w");
                let r = self.m.b.bin(block, BinOp::Shl, i64t.clone(), w, k, "lsl");
                let c = self.m.b.bin(block, BinOp::LShr, i64t.clone(), r, 32i64, "lsl.c");
                (r, c)
            }
            Shift::Lsr | Shift::Asr => {
                let (cast, op, at) = if shift == Shift::Lsr { (Cast::ZExt, BinOp::LShr, 33) } else { (Cast::SExt, BinOp::AShr, 32) };
                let k = clamp(self, at);
                let w = self.m.b.cast(block, cast, i32t.clone(), v, i64t.clone(), "v.w");
                let r = self.m.b.bin(block, op, i64t.clone(), w, k, "shr");
                let twice = self.m.b.bin(block, BinOp::Shl, i64t.clone(), w, 1i64, "v.2");
                let c = self.m.b.bin(block, op, i64t.clone(), twice, k, "shr.c");
                (r, c)
            }
            Shift::Ror | Shift::Rrx => {
                let k = self.m.b.bin(block, BinOp::And, i64t.clone(), amount, 31i64, "amt.r");
                let w = self.m.b.cast(block, Cast::ZExt, i32t.clone(), v, i64t.clone(), "v.w");
                let up = self.m.b.bin(block, BinOp::Shl, i64t.clone(), w, 32i64, "v.hi");
                let doubled = self.m.b.bin(block, BinOp::Or, i64t.clone(), w, up, "v.d");
                let r = self.m.b.bin(block, BinOp::LShr, i64t.clone(), doubled, k, "ror");
                let c = self.m.b.bin(block, BinOp::LShr, i64t.clone(), r, 31i64, "ror.c");
                (r, c)
            }
        };
        let result = self.m.b.cast(block, Cast::Trunc, i64t.clone(), wide, i32t, "shifted");
        if !want_carry {
            return (result, None);
        }
        let out = self.m.b.cast(block, Cast::Trunc, i64t, carry_source, Type::I1, "c.out");
        // A shift by zero leaves the carry as it is.
        let zero = self.m.b.icmp(block, ICond::Eq, Type::I64, amount, 0i64, "amt.z");
        let c = self.flag(block, C);
        (result, Some(self.m.select(block, zero, Type::I1, c, out, "c.sh")))
    }

    /// `a + b + carry`, with the carry out and the signed overflow: the one adder of `add`,
    /// `adc`, `sub` and `rsb` (an operand inverted and a carry of one) and `sbc`.
    fn add_with_carry(&mut self, block: BlockId, a: ValueId, b: ValueId, carry: impl Into<Operand>) -> (ValueId, ValueId, ValueId) {
        // The sum at twice the width, its carry bit 32: a width a C compiler has a type for.
        let (ty, wide) = (Type::I32, Type::I64);
        let wa = self.m.b.cast(block, Cast::ZExt, ty.clone(), a, wide.clone(), "a.w");
        let wb = self.m.b.cast(block, Cast::ZExt, ty.clone(), b, wide.clone(), "b.w");
        let wc = self.m.b.cast(block, Cast::ZExt, Type::I1, carry, wide.clone(), "c.w");
        let ab = self.m.b.bin(block, BinOp::Add, wide.clone(), wa, wb, "ab");
        let sum = self.m.b.bin(block, BinOp::Add, wide.clone(), ab, wc, "sum");
        let result = self.m.b.cast(block, Cast::Trunc, wide.clone(), sum, ty.clone(), "res");
        let high = self.m.b.bin(block, BinOp::LShr, wide.clone(), sum, 32i64, "sum.hi");
        let c = self.m.b.cast(block, Cast::Trunc, wide, high, Type::I1, "c");
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), a, result, "v.x");
        let y = self.m.b.bin(block, BinOp::Xor, ty.clone(), b, result, "v.y");
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "v.and");
        let v = self.m.b.icmp(block, ICond::Slt, ty, both, 0i64, "v");
        (result, c, v)
    }

    fn not(&mut self, block: BlockId, value: ValueId) -> ValueId {
        self.m.b.bin(block, BinOp::Xor, Type::I32, value, -1i64, "not")
    }

    /// The result of a data-processing operation, with C and V when it sets the flags and they
    /// come from the adder.
    fn alu(&mut self, block: BlockId, op: Alu, s: bool, a: ValueId, b: ValueId) -> (ValueId, Option<(ValueId, ValueId)>) {
        let ty = Type::I32;
        let logic = |this: &mut Self, op: BinOp, x: ValueId, y: ValueId| (this.m.b.bin(block, op, ty.clone(), x, y, "r"), None);
        match op {
            Alu::And | Alu::Tst => logic(self, BinOp::And, a, b),
            Alu::Eor | Alu::Teq => logic(self, BinOp::Xor, a, b),
            Alu::Orr => logic(self, BinOp::Or, a, b),
            Alu::Orn => {
                let nb = self.not(block, b);
                logic(self, BinOp::Or, a, nb)
            }
            Alu::Bic => {
                let nb = self.not(block, b);
                logic(self, BinOp::And, a, nb)
            }
            Alu::Mov => (b, None),
            Alu::Mvn => (self.not(block, b), None),
            Alu::Add | Alu::Cmn | Alu::Adc | Alu::Sub | Alu::Cmp | Alu::Sbc | Alu::Rsb | Alu::Rsc => {
                let (x, y) = match op {
                    Alu::Add | Alu::Cmn | Alu::Adc => (a, b),
                    Alu::Rsb | Alu::Rsc => (self.not(block, a), b),
                    _ => (a, self.not(block, b)),
                };
                let carry: Operand = match op {
                    Alu::Add | Alu::Cmn => 0i64.into(),
                    Alu::Adc | Alu::Sbc | Alu::Rsc => self.flag(block, C).into(),
                    _ => 1i64.into(),
                };
                if !s && matches!(op, Alu::Add | Alu::Sub) {
                    let bin = if op == Alu::Add { BinOp::Add } else { BinOp::Sub };
                    return (self.m.b.bin(block, bin, ty, a, b, "r"), None);
                }
                let (result, c, v) = self.add_with_carry(block, x, y, carry);
                (result, s.then_some((c, v)))
            }
        }
    }

    // endregion

    // region Instructions

    fn unsupported(&mut self, block: BlockId, i: &Instr) -> BlockId {
        self.warn(format!("{:#x}: unsupported instruction `{}`", i.address, i.text));
        self.m.unsupported(block, i.address, u64::from(i.code));
        block
    }

    // region The floating-point unit

    /// The pointer to a register of the unit: the doubles are the halves of the state's sixteen
    /// 128-bit slots, the singles the halves of the low sixteen doubles.
    fn fp_ptr(&mut self, block: BlockId, double: bool, n: u8) -> ValueId {
        let (slot, offset) = if double { (n / 2, 8 * (n % 2)) } else { (n / 4, 4 * (n % 4)) };
        let base = self.m.xmm_ptr(block, u32::from(slot));
        self.m.b.gep(block, Type::I8, base, vec![(Type::I64, i64::from(offset).into())], if double { "p.d" } else { "p.s" })
    }

    fn fp_type(double: bool) -> Type {
        if double { Type::Double } else { Type::Float }
    }

    fn fp_bits_type(double: bool) -> Type {
        if double { Type::I64 } else { Type::I32 }
    }

    fn fp_get(&mut self, block: BlockId, double: bool, n: u8) -> ValueId {
        let p = self.fp_ptr(block, double, n);
        self.m.b.load(block, Self::fp_type(double), p, "f")
    }

    fn fp_set(&mut self, block: BlockId, double: bool, n: u8, value: impl Into<Operand>) {
        let p = self.fp_ptr(block, double, n);
        self.m.b.store(block, Self::fp_type(double), value, p);
    }

    fn fp_bits(&mut self, block: BlockId, double: bool, n: u8) -> ValueId {
        let p = self.fp_ptr(block, double, n);
        self.m.b.load(block, Self::fp_bits_type(double), p, "f.bits")
    }

    fn fp_set_bits(&mut self, block: BlockId, double: bool, n: u8, value: impl Into<Operand>) {
        let p = self.fp_ptr(block, double, n);
        self.m.b.store(block, Self::fp_bits_type(double), value, p);
    }

    /// A value with its sign turned.
    fn fp_negated(&mut self, block: BlockId, double: bool, value: ValueId) -> ValueId {
        let (ty, bits) = (Self::fp_type(double), Self::fp_bits_type(double));
        let sign: i64 = if double { i64::MIN } else { i64::from(i32::MIN) };
        let raw = self.m.b.cast(block, Cast::Bitcast, ty.clone(), value, bits.clone(), "f.bits");
        let turned = self.m.b.bin(block, BinOp::Xor, bits.clone(), raw, sign, "f.neg");
        self.m.b.cast(block, Cast::Bitcast, bits, turned, ty, "f")
    }

    /// One of the status register's flags, which the state keeps after the processor's four.
    fn set_fp_flag(&mut self, block: BlockId, index: i64, value: impl Into<Operand>) {
        let p = self.m.flag_ptr_at(block, 4 + index, ["fn", "fz", "fc", "fv"][index as usize]);
        let byte = self.m.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, "f.flag");
        self.m.b.store(block, Type::I8, byte, p);
    }

    fn fp_flag(&mut self, block: BlockId, index: i64) -> ValueId {
        let p = self.m.flag_ptr_at(block, 4 + index, ["fn", "fz", "fc", "fv"][index as usize]);
        let byte = self.m.b.load(block, Type::I8, p, "f.flag");
        self.m.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, "f.flag")
    }

    /// An instruction of the VFP unit; whether the lift has it.
    #[allow(clippy::too_many_lines)]
    fn lift_fp(&mut self, block: BlockId, fp: Fp, ip: u64) -> bool {
        use ra_asm_arm64::vfp::{Arith, Unary};
        let fcmp = |this: &mut Self, cond: FCond, ty: Type, a: Operand, b: Operand| {
            this.m.b.value(block, Inst::FCmp { cond, ty, lhs: a, rhs: b }, "f.is")
        };
        match fp {
            Fp::Arith { op, double, d, n, m } => {
                let ty = Self::fp_type(double);
                let (a, b) = (self.fp_get(block, double, n), self.fp_get(block, double, m));
                let bin = match op {
                    Arith::Add => BinOp::FAdd,
                    Arith::Sub => BinOp::FSub,
                    Arith::Mul | Arith::Nmul => BinOp::FMul,
                    Arith::Div => BinOp::FDiv,
                };
                let mut r = self.m.b.bin(block, bin, ty, a, b, "f");
                if op == Arith::Nmul {
                    r = self.fp_negated(block, double, r);
                }
                self.fp_set(block, double, d, r);
            }
            Fp::Accumulate { double, d, n, m, negate_sum, negate_product } => {
                let ty = Self::fp_type(double);
                let (a, b) = (self.fp_get(block, double, n), self.fp_get(block, double, m));
                let mut product = self.m.b.bin(block, BinOp::FMul, ty.clone(), a, b, "f.mul");
                if negate_product {
                    product = self.fp_negated(block, double, product);
                }
                let mut sum = self.fp_get(block, double, d);
                if negate_sum {
                    sum = self.fp_negated(block, double, sum);
                }
                let r = self.m.b.bin(block, BinOp::FAdd, ty, sum, product, "f");
                self.fp_set(block, double, d, r);
            }
            Fp::Unary { op, double, d, m } => {
                let bits = Self::fp_bits_type(double);
                let sign: i64 = if double { i64::MIN } else { i64::from(i32::MIN) };
                match op {
                    Unary::Mov => {
                        let raw = self.fp_bits(block, double, m);
                        self.fp_set_bits(block, double, d, raw);
                    }
                    Unary::Abs | Unary::Neg => {
                        let raw = self.fp_bits(block, double, m);
                        let r = if op == Unary::Abs {
                            self.m.b.bin(block, BinOp::And, bits, raw, !sign, "f.abs")
                        } else {
                            self.m.b.bin(block, BinOp::Xor, bits, raw, sign, "f.neg")
                        };
                        self.fp_set_bits(block, double, d, r);
                    }
                    Unary::Sqrt => {
                        let ty = Self::fp_type(double);
                        let x = self.fp_get(block, double, m);
                        let name = if double { "llvm.sqrt.f64" } else { "llvm.sqrt.f32" };
                        let r = self.m.intrinsic(block, name, ty.clone(), vec![(ty, x.into())], "f.sqrt");
                        self.fp_set(block, double, d, r);
                    }
                }
            }
            Fp::Constant { double, d, bits } => self.fp_set_bits(block, double, d, Operand::Int(i128::from(bits))),
            Fp::Compare { double, d, m } => {
                // Less sets N; equal Z and C; greater C; what cannot be ordered C and V.
                let ty = Self::fp_type(double);
                let a: Operand = self.fp_get(block, double, d).into();
                let b: Operand = match m {
                    Some(m) => self.fp_get(block, double, m).into(),
                    None => Operand::Float(0.0),
                };
                let less = fcmp(self, FCond::Olt, ty.clone(), a.clone(), b.clone());
                let equal = fcmp(self, FCond::Oeq, ty.clone(), a.clone(), b.clone());
                let unordered = fcmp(self, FCond::Uno, ty, a, b);
                let carry = self.m.b.bin(block, BinOp::Xor, Type::I1, less, 1i64, "f.c");
                self.set_fp_flag(block, N, less);
                self.set_fp_flag(block, Z, equal);
                self.set_fp_flag(block, C, carry);
                self.set_fp_flag(block, V, unordered);
            }
            Fp::Width { to_double, d, m } => {
                let x = self.fp_get(block, !to_double, m);
                let r = if to_double {
                    self.m.b.cast(block, Cast::FPExt, Type::Float, x, Type::Double, "f")
                } else {
                    self.m.b.cast(block, Cast::FPTrunc, Type::Double, x, Type::Float, "f")
                };
                self.fp_set(block, to_double, d, r);
            }
            Fp::FromInt { double, signed, d, m } => {
                let whole = self.fp_bits(block, false, m);
                let r =
                    self.m.b.cast(block, if signed { Cast::SIToFP } else { Cast::UIToFP }, Type::I32, whole, Self::fp_type(double), "f");
                self.fp_set(block, double, d, r);
            }
            Fp::ToInt { toward_zero: false, .. } => return false,
            Fp::ToInt { double, signed, d, m, .. } => {
                // Toward zero; past an end of the word is that end, and what is no number is zero.
                let x = self.fp_get(block, double, m);
                let x: Operand =
                    if double { x.into() } else { self.m.b.cast(block, Cast::FPExt, Type::Float, x, Type::Double, "f.w").into() };
                let (top, bottom, most, least): (f64, f64, i64, i64) = if signed {
                    (2_147_483_648.0, -2_147_483_648.0, i64::from(i32::MAX), i64::from(i32::MIN))
                } else {
                    (4_294_967_296.0, 0.0, 0xffff_ffff, 0)
                };
                let high = fcmp(self, FCond::Oge, Type::Double, x.clone(), Operand::Float(top));
                let low = fcmp(self, FCond::Olt, Type::Double, x.clone(), Operand::Float(bottom));
                let none = fcmp(self, FCond::Uno, Type::Double, x.clone(), x.clone());
                let outside = self.m.b.bin(block, BinOp::Or, Type::I1, high, low, "f.out");
                let outside = self.m.b.bin(block, BinOp::Or, Type::I1, outside, none, "f.out");
                let inside = self.m.b.select(block, outside, Type::Double, Operand::Float(0.0), x, "f.in");
                let whole =
                    self.m.b.cast(block, if signed { Cast::FPToSI } else { Cast::FPToUI }, Type::Double, inside, Type::I32, "whole");
                let whole = self.m.b.select(block, low, Type::I32, least, whole, "whole");
                let whole = self.m.b.select(block, high, Type::I32, most, whole, "whole");
                self.fp_set_bits(block, false, d, whole);
            }
            Fp::Core { to_core, s, rt } => {
                if to_core {
                    let raw = self.fp_bits(block, false, s);
                    self.set_reg(block, rt, raw);
                } else {
                    let v = self.reg(block, rt, ip);
                    self.fp_set_bits(block, false, s, v);
                }
            }
            Fp::CorePair { to_core, d, rt, rt2 } => {
                if to_core {
                    let raw = self.fp_bits(block, true, d);
                    let low = self.m.b.cast(block, Cast::Trunc, Type::I64, raw, Type::I32, "f.lo");
                    let high = self.m.b.bin(block, BinOp::LShr, Type::I64, raw, 32i64, "f.hi");
                    let high = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "f.hi");
                    self.set_reg(block, rt, low);
                    self.set_reg(block, rt2, high);
                } else {
                    let (low, high) = (self.reg(block, rt, ip), self.reg(block, rt2, ip));
                    let low = self.m.b.cast(block, Cast::ZExt, Type::I32, low, Type::I64, "f.lo");
                    let high = self.m.b.cast(block, Cast::ZExt, Type::I32, high, Type::I64, "f.hi");
                    let high = self.m.b.bin(block, BinOp::Shl, Type::I64, high, 32i64, "f.hi");
                    let raw = self.m.b.bin(block, BinOp::Or, Type::I64, high, low, "f.bits");
                    self.fp_set_bits(block, true, d, raw);
                }
            }
            Fp::Status { to_core: true, rt: PC } => {
                // `vmrs APSR_nzcv, fpscr`: the unit's flags to the processor's.
                for flag in [N, Z, C, V] {
                    let v = self.fp_flag(block, flag);
                    self.set_flag(block, flag, v);
                }
            }
            Fp::Status { to_core: true, rt } => {
                // The status register as a word: its flags in the top four bits, the rest as at reset.
                let mut word: Operand = 0i64.into();
                for (flag, shift) in [(N, 31i64), (Z, 30), (C, 29), (V, 28)] {
                    let v = self.fp_flag(block, flag);
                    let wide = self.m.b.cast(block, Cast::ZExt, Type::I1, v, Type::I32, "f.flag");
                    let placed = self.m.b.bin(block, BinOp::Shl, Type::I32, wide, shift, "f.flag");
                    word = self.m.b.bin(block, BinOp::Or, Type::I32, word, placed, "fpscr").into();
                }
                self.set_reg(block, rt, word);
            }
            Fp::Status { to_core: false, rt } => {
                // Only the flags are kept: the rounding mode and the exception bits are not.
                let word = self.reg(block, rt, ip);
                for (flag, shift) in [(N, 31i64), (Z, 30), (C, 29), (V, 28)] {
                    let bit = self.m.b.bin(block, BinOp::LShr, Type::I32, word, shift, "f.flag");
                    let bit = self.m.b.cast(block, Cast::Trunc, Type::I32, bit, Type::I1, "f.flag");
                    self.set_fp_flag(block, flag, bit);
                }
            }
            Fp::Mem { load, double, d, rn, offset } => {
                let address: Operand = if rn == PC {
                    let pc = if self.arm { ip + 8 } else { thumb::aligned_pc(ip) };
                    let at = pc.wrapping_add(offset as i64 as u64) & 0xffff_ffff;
                    // A constant of the literal pool: in a standalone port, the constant itself.
                    let constant = if double { self.image.u64(at).ok() } else { self.image.u32(at).ok().map(u64::from) };
                    if self.lifted.standalone
                        && load
                        && let Some(raw) = constant
                    {
                        self.fp_set_bits(block, double, d, Operand::Int(i128::from(raw)));
                        return true;
                    }
                    Operand::Int(i128::from(at))
                } else {
                    let base = self.reg(block, rn, ip);
                    self.m.b.bin(block, BinOp::Add, Type::I32, base, i64::from(offset), "addr").into()
                };
                let bits = Self::fp_bits_type(double);
                if load {
                    let raw = self.m.load(block, bits, address, "f.ld");
                    self.fp_set_bits(block, double, d, raw);
                } else {
                    let raw = self.fp_bits(block, double, d);
                    self.m.store(block, bits, raw, address);
                }
            }
            Fp::Multiple { load, double, d, count, rn, wback, before } => {
                let size = if double { 8i64 } else { 4 };
                let total = size * i64::from(count);
                let base = self.reg(block, rn, ip);
                let moved = self.m.b.bin(block, if before { BinOp::Sub } else { BinOp::Add }, Type::I32, base, total, "moved");
                let start = if before { moved } else { base };
                let bits = Self::fp_bits_type(double);
                for k in 0..count {
                    let at = self.m.b.bin(block, BinOp::Add, Type::I32, start, size * i64::from(k), "at");
                    if load {
                        let raw = self.m.load(block, bits.clone(), at, "f.ld");
                        self.fp_set_bits(block, double, d + k, raw);
                    } else {
                        let raw = self.fp_bits(block, double, d + k);
                        self.m.store(block, bits.clone(), raw, at);
                    }
                }
                if wback {
                    self.set_reg(block, rn, moved);
                }
            }
        }
        true
    }

    // endregion

    #[allow(clippy::too_many_lines)]
    fn lift_instr(&mut self, block: BlockId, d: &Decoded, i: &Instr) -> BlockId {
        let ip = i.address;
        let ty = Type::I32;
        match d.op {
            Op::Alu { op, s, rd, rn, src } => {
                let (b, shifter_carry) = self.shifted(block, src, ip, s && !op.arithmetic());
                let a = if op.unary() { b } else { self.reg(block, rn, ip) };
                let (result, adder) = self.alu(block, op, s, a, b);
                if !op.compares() && rd == PC {
                    // A write of the PC is a jump to the value, its low bit dropped: the jump of
                    // a switch (`add pc, r0`) and the oldest return (`mov pc, lr`).
                    if i.flow != ra_asm_arch::Flow::Return {
                        let code = self.m.b.bin(block, BinOp::And, Type::I32, result, -2i64, "code");
                        self.jump_target = Some(self.m.b.cast(block, Cast::ZExt, Type::I32, code, Type::I64, "target"));
                    }
                    return block;
                }
                if !op.compares() {
                    self.set_reg(block, rd, result);
                }
                if s {
                    self.set_nz(block, result);
                    if let Some((c, v)) = adder {
                        self.set_flag(block, C, c);
                        self.set_flag(block, V, v);
                    } else if let Some(c) = shifter_carry {
                        self.set_flag(block, C, c);
                    }
                }
            }
            Op::MovWide { rd, imm, top } => {
                if top {
                    let old = self.reg(block, rd, ip);
                    let low = self.m.b.bin(block, BinOp::And, ty.clone(), old, 0xffffi64, "lo");
                    let r = self.m.b.bin(block, BinOp::Or, ty, low, i64::from((u32::from(imm) << 16) as i32), "movt");
                    self.set_reg(block, rd, r);
                } else {
                    self.set_reg(block, rd, i64::from(imm));
                }
            }
            Op::Mul { s, rd, rn, rm, acc, subtract } => {
                let a = self.reg(block, rn, ip);
                let b = self.reg(block, rm, ip);
                let mut r = self.m.b.bin(block, BinOp::Mul, ty.clone(), a, b, "mul");
                if let Some(ra) = acc {
                    let x = self.reg(block, ra, ip);
                    r = if subtract {
                        self.m.b.bin(block, BinOp::Sub, ty, x, r, "mls")
                    } else {
                        self.m.b.bin(block, BinOp::Add, ty, r, x, "mla")
                    };
                }
                self.set_reg(block, rd, r);
                if s {
                    self.set_nz(block, r);
                }
            }
            Op::LongMul { signed, accumulate, lo, hi, rn, rm } => {
                let cast = if signed { Cast::SExt } else { Cast::ZExt };
                let a = self.reg(block, rn, ip);
                let b = self.reg(block, rm, ip);
                let wa = self.m.b.cast(block, cast, ty.clone(), a, Type::I64, "a.w");
                let wb = self.m.b.cast(block, cast, ty.clone(), b, Type::I64, "b.w");
                let mut p = self.m.b.bin(block, BinOp::Mul, Type::I64, wa, wb, "mull");
                if accumulate {
                    let (l, h) = (self.reg(block, lo, ip), self.reg(block, hi, ip));
                    let wl = self.m.b.cast(block, Cast::ZExt, ty.clone(), l, Type::I64, "lo.w");
                    let wh = self.m.b.cast(block, Cast::ZExt, ty.clone(), h, Type::I64, "hi.w");
                    let up = self.m.b.bin(block, BinOp::Shl, Type::I64, wh, 32i64, "hi.s");
                    let acc = self.m.b.bin(block, BinOp::Or, Type::I64, up, wl, "acc");
                    p = self.m.b.bin(block, BinOp::Add, Type::I64, p, acc, "mlal");
                }
                let low = self.m.b.cast(block, Cast::Trunc, Type::I64, p, ty.clone(), "lo");
                let shifted = self.m.b.bin(block, BinOp::LShr, Type::I64, p, 32i64, "hi.s");
                let high = self.m.b.cast(block, Cast::Trunc, Type::I64, shifted, ty, "hi");
                self.set_reg(block, lo, low);
                self.set_reg(block, hi, high);
            }
            Op::Div { signed, rd, rn, rm } => {
                // A division by zero gives zero, and the one signed overflow gives the dividend:
                // the machine does not trap.
                let a = self.reg(block, rn, ip);
                let b = self.reg(block, rm, ip);
                let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, 0i64, "div.z");
                let mut divisor = self.m.select(block, zero, ty.clone(), 1i64, b, "div.d");
                if signed {
                    let min = self.m.b.icmp(block, ICond::Eq, ty.clone(), a, i64::from(i32::MIN), "div.min");
                    let minus = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, -1i64, "div.m1");
                    let overflow = self.m.b.bin(block, BinOp::And, Type::I1, min, minus, "div.o");
                    divisor = self.m.select(block, overflow, ty.clone(), 1i64, divisor, "div.d");
                }
                let q = self.m.b.bin(block, if signed { BinOp::SDiv } else { BinOp::UDiv }, ty.clone(), a, divisor, "div");
                let r = self.m.select(block, zero, ty, 0i64, q, "div.r");
                self.set_reg(block, rd, r);
            }
            Op::Extend { signed, bits, rd, rm, rotate } => {
                let mut v = self.reg(block, rm, ip);
                if rotate != 0 {
                    let low = self.m.b.bin(block, BinOp::LShr, ty.clone(), v, i64::from(rotate), "ror.lo");
                    let high = self.m.b.bin(block, BinOp::Shl, ty.clone(), v, i64::from(32 - rotate), "ror.hi");
                    v = self.m.b.bin(block, BinOp::Or, ty.clone(), low, high, "ror");
                }
                let narrow = Type::Int(u16::from(bits));
                let t = self.m.b.cast(block, Cast::Trunc, ty.clone(), v, narrow.clone(), "ext.t");
                let r = self.m.b.cast(block, if signed { Cast::SExt } else { Cast::ZExt }, narrow, t, ty, "ext");
                self.set_reg(block, rd, r);
            }
            Op::HighMul { rd, rn, rm, round } => {
                let (a, b) = (self.reg(block, rn, ip), self.reg(block, rm, ip));
                let a = self.m.b.cast(block, Cast::SExt, Type::I32, a, Type::I64, "a.w");
                let b = self.m.b.cast(block, Cast::SExt, Type::I32, b, Type::I64, "b.w");
                let mut product = self.m.b.bin(block, BinOp::Mul, Type::I64, a, b, "mul");
                if round {
                    product = self.m.b.bin(block, BinOp::Add, Type::I64, product, 0x8000_0000i64, "mul.r");
                }
                let high = self.m.b.bin(block, BinOp::AShr, Type::I64, product, 32i64, "hi");
                let high = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "hi");
                self.set_reg(block, rd, high);
            }
            Op::Fp(fp) => {
                if !self.lift_fp(block, fp, ip) {
                    return self.unsupported(block, i);
                }
            }
            // The saturating forms hold a result to a range. The flag that says one was held
            // (`Q`) is not among the state's: nothing the lift covers reads it.
            Op::Saturate { signed, rd, rn, bits, shift, amount } => {
                let (v, _) = self.shifted(block, Source::Reg { rm: rn, shift, amount }, ip, false);
                let (lowest, highest) = if signed { (-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1) } else { (0, (1i64 << bits) - 1) };
                let under = self.m.b.icmp(block, ICond::Slt, Type::I32, v, lowest, "under");
                let over = self.m.b.icmp(block, ICond::Sgt, Type::I32, v, highest, "over");
                let held = self.m.b.select(block, under, Type::I32, lowest, v, "sat");
                let held = self.m.b.select(block, over, Type::I32, highest, held, "sat");
                self.set_reg(block, rd, held);
            }
            Op::SatArith { subtract, double, rd, rm, rn } => {
                let (a, b) = (self.reg(block, rm, ip), self.reg(block, rn, ip));
                let (lowest, highest) = (i64::from(i32::MIN), i64::from(i32::MAX));
                let held = |this: &mut Self, v: ValueId| {
                    let under = this.m.b.icmp(block, ICond::Slt, Type::I64, v, lowest, "under");
                    let over = this.m.b.icmp(block, ICond::Sgt, Type::I64, v, highest, "over");
                    let held = this.m.b.select(block, under, Type::I64, lowest, v, "sat");
                    this.m.b.select(block, over, Type::I64, highest, held, "sat")
                };
                let a = self.m.b.cast(block, Cast::SExt, Type::I32, a, Type::I64, "a.w");
                let mut b = self.m.b.cast(block, Cast::SExt, Type::I32, b, Type::I64, "b.w");
                if double {
                    let twice = self.m.b.bin(block, BinOp::Shl, Type::I64, b, 1i64, "twice");
                    b = held(self, twice);
                }
                let sum = self.m.b.bin(block, if subtract { BinOp::Sub } else { BinOp::Add }, Type::I64, a, b, "q");
                let sum = held(self, sum);
                let sum = self.m.b.cast(block, Cast::Trunc, Type::I64, sum, Type::I32, "q");
                self.set_reg(block, rd, sum);
            }
            // One thread of control always wins its exclusive store.
            Op::StoreExclusive { rd, rt, rn, offset } => {
                let (base, v) = (self.reg(block, rn, ip), self.reg(block, rt, ip));
                let address = self.m.b.bin(block, BinOp::Add, ty.clone(), base, i64::from(offset), "addr");
                self.m.store(block, Type::I32, v, address);
                self.set_reg(block, rd, 0i64);
            }
            // The flags in the top four bits, and the mode a program runs in in the low five.
            Op::Status { rd } => {
                let mut status: Operand = 0x10i64.into();
                for (index, place) in [(N, 31), (Z, 30), (C, 29), (V, 28)] {
                    let flag = self.flag(block, index);
                    let wide = self.m.b.cast(block, Cast::ZExt, Type::I1, flag, Type::I32, "flag");
                    let placed = self.m.b.bin(block, BinOp::Shl, Type::I32, wide, i64::from(place), "flag");
                    status = self.m.b.bin(block, BinOp::Or, Type::I32, status, placed, "apsr").into();
                }
                self.set_reg(block, rd, status);
            }
            Op::Bits { kind, rd, rm } => {
                let v = self.reg(block, rm, ip);
                // The two groups of bits a mask and its shift name, each put where the other was.
                let swap = |this: &mut Self, v: ValueId, by: i64, mask: i64| {
                    let high = this.m.b.bin(block, BinOp::LShr, Type::I32, v, by, "hi");
                    let high = this.m.b.bin(block, BinOp::And, Type::I32, high, mask, "hi");
                    let low = this.m.b.bin(block, BinOp::And, Type::I32, v, mask, "lo");
                    let low = this.m.b.bin(block, BinOp::Shl, Type::I32, low, by, "lo");
                    this.m.b.bin(block, BinOp::Or, Type::I32, high, low, "swap")
                };
                let r = match kind {
                    Bits::Rev16 => swap(self, v, 8, 0x00ff_00ff),
                    Bits::Rev | Bits::Rbit => {
                        let mut v = v;
                        if kind == Bits::Rbit {
                            for (by, mask) in [(1, 0x5555_5555), (2, 0x3333_3333), (4, 0x0f0f_0f0f)] {
                                v = swap(self, v, by, mask);
                            }
                        }
                        let v = swap(self, v, 8, 0x00ff_00ff);
                        swap(self, v, 16, 0xffff)
                    }
                    Bits::Revsh => {
                        let v = swap(self, v, 8, 0x00ff_00ff);
                        let t = self.m.b.cast(block, Cast::Trunc, ty.clone(), v, Type::I16, "rev.t");
                        self.m.b.cast(block, Cast::SExt, Type::I16, t, ty, "revsh")
                    }
                    Bits::Clz => {
                        // By halves: where the top bits are all zero they are counted and passed.
                        let zero: Operand = 0i64.into();
                        let (mut count, mut rest): (Operand, Operand) = (zero, v.into());
                        for step in [16i64, 8, 4, 2, 1] {
                            let top = self.m.b.bin(block, BinOp::LShr, Type::I32, rest.clone(), 32 - step, "top");
                            let none = self.m.b.icmp(block, ICond::Eq, Type::I32, top, 0i64, "none");
                            let more = self.m.b.bin(block, BinOp::Add, Type::I32, count.clone(), step, "n");
                            let moved = self.m.b.bin(block, BinOp::Shl, Type::I32, rest.clone(), step, "rest");
                            count = self.m.b.select(block, none, Type::I32, more, count, "n").into();
                            rest = self.m.b.select(block, none, Type::I32, moved, rest, "rest").into();
                        }
                        let empty = self.m.b.icmp(block, ICond::Eq, Type::I32, v, 0i64, "zero");
                        self.m.b.select(block, empty, ty, 32i64, count, "clz")
                    }
                };
                self.set_reg(block, rd, r);
            }
            Op::Field { signed, rd, rn, lsb, width } => {
                let v = self.reg(block, rn, ip);
                let r = if signed {
                    let up = self.m.b.bin(block, BinOp::Shl, ty.clone(), v, i64::from(32 - lsb - width), "bf.up");
                    self.m.b.bin(block, BinOp::AShr, ty, up, i64::from(32 - width), "sbfx")
                } else {
                    let down = self.m.b.bin(block, BinOp::LShr, ty.clone(), v, i64::from(lsb), "bf.down");
                    let mask = ((1u64 << width) - 1) as u32 as i32;
                    self.m.b.bin(block, BinOp::And, ty, down, i64::from(mask), "ubfx")
                };
                self.set_reg(block, rd, r);
            }
            Op::Insert { rd, rn, lsb, width } => {
                let mask = (((1u64 << width) - 1) << lsb) as u32 as i32;
                let old = self.reg(block, rd, ip);
                let cleared = self.m.b.bin(block, BinOp::And, ty.clone(), old, i64::from(!mask), "bf.clear");
                let r = match rn {
                    Some(rn) => {
                        let src = self.reg(block, rn, ip);
                        let placed = self.m.b.bin(block, BinOp::Shl, ty.clone(), src, i64::from(lsb), "bf.place");
                        let kept = self.m.b.bin(block, BinOp::And, ty.clone(), placed, i64::from(mask), "bf.keep");
                        self.m.b.bin(block, BinOp::Or, ty, cleared, kept, "bfi")
                    }
                    None => cleared,
                };
                self.set_reg(block, rd, r);
            }
            Op::Mem { load, bits, signed, rt, rn, offset, index, wback } => {
                let (address, after): (Operand, Option<ValueId>) = match (rn, offset) {
                    (PC, Offset::Imm(o)) => {
                        let pc = if self.arm { ip + 8 } else { thumb::aligned_pc(ip) };
                        let at = pc.wrapping_add(o as i64 as u64) & 0xffff_ffff;
                        // A word of the literal pool, which lies in the code and never changes:
                        // a standalone port has no code to read it from, so the word is the
                        // constant it is, and an address it holds the address of what it names.
                        if self.lifted.standalone
                            && load
                            && bits == 32
                            && let Ok(word) = self.image.u32(at)
                        {
                            if rt != PC {
                                // An address only where the word lies inside something the
                                // image has: a number just below a section is a number (0xffff
                                // under an image at 0x10000 is a mask).
                                let at = u64::from(word);
                                let inside =
                                    self.lifted.functions.contains_key(&at) || self.lifted.data.iter().any(|s| at >= s.start && at < s.end);
                                let value = inside.then(|| self.lifted.address_of(at)).flatten().unwrap_or(Operand::Int(i128::from(word)));
                                self.set_reg(block, rt, value);
                            }
                            return block;
                        }
                        (Operand::Int(i128::from(at)), None)
                    }
                    (PC, Offset::Reg { .. } | Offset::Shifted { .. }) => return self.unsupported(block, i),
                    (_, offset) => {
                        let base = self.reg(block, rn, ip);
                        let by: Operand = match offset {
                            Offset::Shifted { rm, shift, amount, subtract } => {
                                let (by, _) = self.shifted(block, Source::Reg { rm, shift, amount }, ip, false);
                                if subtract { self.m.b.bin(block, BinOp::Sub, ty.clone(), 0i64, by, "idx").into() } else { by.into() }
                            }
                            Offset::Imm(o) => i64::from(o).into(),
                            Offset::Reg { rm, shift: 0 } => self.reg(block, rm, ip).into(),
                            Offset::Reg { rm, shift } => {
                                let r = self.reg(block, rm, ip);
                                self.m.b.bin(block, BinOp::Shl, ty.clone(), r, i64::from(shift), "idx").into()
                            }
                        };
                        let sum = self.m.b.bin(block, BinOp::Add, ty.clone(), base, by, "addr");
                        (if index { sum } else { base }.into(), wback.then_some(sum))
                    }
                };
                let width = Type::Int(u16::from(bits));
                if load {
                    let v = self.m.load(block, width.clone(), address, "ld");
                    let v = if bits == 32 {
                        v
                    } else {
                        self.m.b.cast(block, if signed { Cast::SExt } else { Cast::ZExt }, width, v, ty, "ld.x")
                    };
                    if rt == PC {
                        // A load into the PC is a jump to what was read.
                        if let Some(after) = after {
                            self.set_reg(block, rn, after);
                        }
                        let code = self.m.b.bin(block, BinOp::And, Type::I32, v, -2i64, "code");
                        self.jump_target = Some(self.m.b.cast(block, Cast::ZExt, Type::I32, code, Type::I64, "target"));
                        return block;
                    }
                    self.set_reg(block, rt, v);
                } else {
                    let v = self.reg(block, rt, ip);
                    let v = if bits == 32 { v } else { self.m.b.cast(block, Cast::Trunc, ty, v, width.clone(), "st.t") };
                    self.m.store(block, width, v, address);
                }
                if let Some(after) = after {
                    self.set_reg(block, rn, after);
                }
            }
            Op::Dual { load, rt, rt2, rn, offset, index, wback } => {
                let base = self.reg(block, rn, ip);
                let base = if rn == PC { self.m.b.bin(block, BinOp::And, ty.clone(), base, -4i64, "pc.a") } else { base };
                let sum = self.m.b.bin(block, BinOp::Add, ty.clone(), base, i64::from(offset), "addr");
                let first = if index { sum } else { base };
                let second = self.m.b.bin(block, BinOp::Add, ty.clone(), first, 4i64, "addr.2");
                if load {
                    let a = self.m.load(block, ty.clone(), first, "ld");
                    let b = self.m.load(block, ty, second, "ld.2");
                    self.set_reg(block, rt, a);
                    self.set_reg(block, rt2, b);
                } else {
                    let a = self.reg(block, rt, ip);
                    let b = self.reg(block, rt2, ip);
                    self.m.store(block, ty.clone(), a, first);
                    self.m.store(block, ty, b, second);
                }
                if wback {
                    self.set_reg(block, rn, sum);
                }
            }
            Op::Multiple { load, rn, list, wback, before, swapped } => {
                let registers: Vec<u8> = (0..16u8).filter(|n| list & (1 << n) != 0).collect();
                let size = 4 * registers.len() as i64;
                let base = self.reg(block, rn, ip);
                // The lowest address written or read, and what the base becomes: down for
                // `db` and `da`, up for `ia` and `ib`.
                let down = before != swapped;
                let moved = self.m.b.bin(block, if down { BinOp::Sub } else { BinOp::Add }, ty.clone(), base, size, "moved");
                let (start, end) = match (down, swapped) {
                    (true, false) => (moved, moved),
                    (false, false) => (base, moved),
                    (true, true) => (self.m.b.bin(block, BinOp::Add, ty.clone(), moved, 4i64, "lo"), moved),
                    (false, true) => (self.m.b.bin(block, BinOp::Add, ty.clone(), base, 4i64, "lo"), moved),
                };
                let addresses: Vec<ValueId> =
                    (0..registers.len()).map(|k| self.m.b.bin(block, BinOp::Add, ty.clone(), start, 4 * k as i64, "at")).collect();
                if load {
                    // The PC a `pop` loads is the return, which the block's terminator makes.
                    let values: Vec<(u8, ValueId)> = registers
                        .iter()
                        .zip(&addresses)
                        .filter(|(n, _)| **n != PC)
                        .map(|(&n, &at)| (n, self.m.load(block, ty.clone(), at, "ld")))
                        .collect();
                    for (n, v) in values {
                        self.set_reg(block, n, v);
                    }
                } else {
                    let values: Vec<ValueId> = registers.iter().map(|&n| self.reg(block, n, ip)).collect();
                    for (v, at) in values.into_iter().zip(addresses) {
                        self.m.store(block, ty.clone(), v, at);
                    }
                }
                if wback {
                    self.set_reg(block, rn, end);
                }
            }
            Op::Branch { cond, .. } => {
                if let Some(cc) = cond.or(d.cond) {
                    self.cond = Some(self.condition(block, cc));
                }
            }
            // A call that changes the instruction set, where the image is not one of both.
            Op::Call { arm: true, .. } if !self.image.arm_and_thumb() => return self.unsupported(block, i),
            Op::Call { target, .. } => {
                self.set_reg(block, LR, ((ip + 4) | u64::from(!self.arm)) as i64);
                if self.lifted.functions.contains_key(&target) {
                    self.call_lifted(block, target);
                } else {
                    self.warn(format!("{ip:#x}: a call to {target:#x}, which is no function"));
                    self.m.trap(block, ip, trap::INVALID);
                }
            }
            Op::BranchReg { rm: LR, link: false } => {}
            // `bx pc` goes on to the code after it in the other set: the block's branch.
            Op::BranchReg { rm: PC, link: false } if !self.arm && self.image.arm_and_thumb() => {}
            // The return through a register the address was popped into: the block ends in it.
            Op::BranchReg { link: false, .. } if i.flow == ra_asm_arch::Flow::Return => {}
            Op::BranchReg { rm, link } => {
                let target = self.reg(block, rm, ip);
                let (block, target) = self.thumb_target(block, target, ip);
                if link {
                    self.set_reg(block, LR, ((ip + u64::from(d.len)) | u64::from(!self.arm)) as i64);
                    self.runtime_call(block, target);
                } else {
                    self.jump_target = Some(target);
                }
                return block;
            }
            Op::CompareBranch { nonzero, rn, .. } => {
                let v = self.reg(block, rn, ip);
                self.cond = Some(self.m.b.icmp(block, if nonzero { ICond::Ne } else { ICond::Eq }, ty, v, 0i64, "cb"));
            }
            Op::Adr { rd, target } => self.set_reg(block, rd, (target & 0xffff_ffff) as i64),
            // A Linux program's system call: the number in `r7`, the arguments from `r0`, the
            // result in `r0`. They go to the runtime by value, the number as the kernel's oldest
            // table has it (which MIPS's calls use, from 4000), so a raised function keeps its
            // registers in values across the call. A console's program names the call by the
            // instruction's own number, and its system reads the state.
            Op::Svc(0) if self.image.platform().is_none() => {
                let wide = |this: &mut Self, n: u8| {
                    let v = this.reg(block, n, ip);
                    this.m.b.cast(block, Cast::ZExt, Type::I32, v, Type::I64, "sys.a")
                };
                let number = wide(self, 7);
                let number = self.m.b.bin(block, BinOp::Add, Type::I64, number, 4000i64, "sys.n");
                let mut args = vec![(Type::I64, number.into())];
                for n in 0..4 {
                    args.push((Type::I64, wide(self, n).into()));
                }
                let result = self
                    .m
                    .b
                    .call(block, crate::state::syscall_type(), Callee::Direct("ra_syscall".into()), args, "sys")
                    .expect("the system call returns a value");
                let result = self.m.b.cast(block, Cast::Trunc, Type::I64, result, Type::I32, "sys.r");
                self.set_reg(block, 0, result);
            }
            Op::Svc(imm) => {
                self.m.b.call(
                    block,
                    FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
                    Callee::Direct("ra_svc".into()),
                    vec![(Type::Ptr, self.m.state.into()), (Type::I64, i64::from(imm).into())],
                    "",
                );
            }
            // A breakpoint is the runtime's too: its number goes above the eight bits of an
            // `svc`'s, so the runtime tells the two apart.
            Op::Bkpt(imm) => {
                self.m.b.call(
                    block,
                    FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
                    Callee::Direct("ra_svc".into()),
                    vec![(Type::Ptr, self.m.state.into()), (Type::I64, (crate::state::BKPT | i64::from(imm)).into())],
                    "",
                );
            }
            Op::It { .. } | Op::Nop => {}
            Op::Trap => self.m.trap(block, ip, trap::UD2),
            Op::Unsupported => return self.unsupported(block, i),
        }
        block
    }

    // endregion
}
