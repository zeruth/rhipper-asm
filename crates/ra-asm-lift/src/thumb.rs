//! The Thumb-2 semantics: one instruction of [`ra_asm_arm64::thumb`] becomes IR over the
//! [`Machine`] state.
//!
//! The state holds `r0..r15` in the first sixteen general slots, each zero-extended to 64 bits,
//! and N, Z, C and V in the first four flag bytes, as ARM64 keeps them. The integer subset the
//! decoder gives is covered: the data processing with its shifter and its flags, the moves of
//! halves, the multiplies and divides, the extends and the bitfields, the single, dual and
//! multiple loads and stores, the branches and calls, and `svc`, which goes to the runtime's
//! `ra_svc`. An instruction an IT block predicates runs behind a branch on its condition; a
//! predicated one that ends its block (a return, a branch through a register) cannot, since the
//! analysis takes it as unconditional, and traps, as a `blx` to ARM code and every encoding
//! outside the subset do. A literal load reads the pool where it lies in the mapped image, so the
//! lift is of the machine levels: the standalone level needs the pools as globals.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, Function, Terminator as T};
use ra_asm_arch::Instr;
use ra_asm_arm64::thumb::{self, Alu, Decoded, LR, Offset, Op, PC, Shift, Source};
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FnType, FunctionBuilder, ICond, Operand, Terminator, Type, ValueId};
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
        _image: image,
        lifted,
        blocks,
        cond: None,
        jump_target: None,
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
    _image: &'a Image,
    lifted: &'a mut Lifted,
    blocks: HashMap<u64, BlockId>,
    /// The condition of the conditional branch that ends the current block.
    cond: Option<ValueId>,
    /// The target of the indirect branch that ends the current block, as a 64-bit address.
    jump_target: Option<ValueId>,
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
        for (n, i) in blk.instrs.iter().enumerate() {
            if self.m.b.has_terminator(block) {
                break;
            }
            self.m.b.address = Some(i.address);
            let it = thumb::it_condition(&blk.instrs[..n]);
            let Some(d) = thumb::decode_raw(i.code, i.address, it) else { break };
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
            T::Switch { targets, .. } => {
                let default = self.m.b.block("switch.bad");
                self.m.trap(default, last, trap::BAD_SWITCH);
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
        let bit = self.m.b.cast(block, Cast::Trunc, Type::I32, target, Type::I1, "thumb");
        let ok = self.m.b.block(&format!("thumb_{ip:x}"));
        let arm = self.m.b.block(&format!("arm_{ip:x}"));
        self.m.b.terminate(block, Terminator::CondBr { cond: bit.into(), then: ok, els: arm });
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
            return self.m.value_of(block, Type::I32, (ip + 4) as i64);
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
        let (ty, wide) = (Type::I32, Type::Int(33));
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
            Alu::Add | Alu::Cmn | Alu::Adc | Alu::Sub | Alu::Cmp | Alu::Sbc | Alu::Rsb => {
                let (x, y) = match op {
                    Alu::Add | Alu::Cmn | Alu::Adc => (a, b),
                    Alu::Rsb => (self.not(block, a), b),
                    _ => (a, self.not(block, b)),
                };
                let carry: Operand = match op {
                    Alu::Add | Alu::Cmn => 0i64.into(),
                    Alu::Adc | Alu::Sbc => self.flag(block, C).into(),
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

    #[allow(clippy::too_many_lines)]
    fn lift_instr(&mut self, block: BlockId, d: &Decoded, i: &Instr) -> BlockId {
        let ip = i.address;
        let ty = Type::I32;
        match d.op {
            Op::Alu { op, s, rd, rn, src } => {
                let (b, shifter_carry) = self.shifted(block, src, ip, s && !op.arithmetic());
                let a = if op.unary() { b } else { self.reg(block, rn, ip) };
                let (result, adder) = self.alu(block, op, s, a, b);
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
                        (Operand::Int(i128::from(thumb::aligned_pc(ip).wrapping_add(o as i64 as u64) & 0xffff_ffff)), None)
                    }
                    (PC, Offset::Reg { .. }) => return self.unsupported(block, i),
                    (_, offset) => {
                        let base = self.reg(block, rn, ip);
                        let by: Operand = match offset {
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
            Op::Multiple { load, rn, list, wback, before } => {
                let registers: Vec<u8> = (0..16u8).filter(|n| list & (1 << n) != 0).collect();
                let size = 4 * registers.len() as i64;
                let base = self.reg(block, rn, ip);
                let start = if before { self.m.b.bin(block, BinOp::Sub, ty.clone(), base, size, "lo") } else { base };
                let end = if before { start } else { self.m.b.bin(block, BinOp::Add, ty.clone(), base, size, "hi") };
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
            Op::Call { arm: true, .. } => return self.unsupported(block, i),
            Op::Call { target, .. } => {
                self.set_reg(block, LR, ((ip + 4) | 1) as i64);
                if self.lifted.functions.contains_key(&target) {
                    self.call_lifted(block, target);
                } else {
                    self.warn(format!("{ip:#x}: a call to {target:#x}, which is no function"));
                    self.m.trap(block, ip, trap::INVALID);
                }
            }
            Op::BranchReg { rm: LR, link: false } => {}
            Op::BranchReg { rm, link } => {
                let target = self.reg(block, rm, ip);
                let (block, target) = self.thumb_target(block, target, ip);
                if link {
                    self.set_reg(block, LR, ((ip + u64::from(d.len)) | 1) as i64);
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
            Op::Svc(imm) => {
                self.m.b.call(
                    block,
                    FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
                    Callee::Direct("ra_svc".into()),
                    vec![(Type::Ptr, self.m.state.into()), (Type::I64, i64::from(imm).into())],
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
