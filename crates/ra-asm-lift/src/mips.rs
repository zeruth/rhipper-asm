//! The MIPS semantics: one instruction word of MIPS I to IV becomes IR over the [`Machine`] state.
//!
//! The state holds `$0..$31` at 64 bits, MIPS III's width: a 32-bit operation reads the low
//! half of its operands and writes its result sign-extended, as the machine does, so a 64-bit
//! operation of MIPS III sees the same registers. `$0` reads as the constant zero and a write to
//! it is dropped. `hi` and `lo` are scalar fields of the state, the FPU's eight condition codes
//! are its flag bytes and its registers the vector slots: under o32 (`Status.FR = 0`) a single is
//! the low word of its slot and a double the even register and the odd one after it, under n32
//! (`Status.FR = 1`) a double is the low half of its own slot.
//!
//! A branch's delay slot is the instruction after it in the block, as the builder places it: the
//! branch's condition, its target and its link are taken from the registers before the slot runs,
//! then the slot runs, then control leaves; a likely branch's slot that shares the block runs on
//! the taken path only. The words are read little-endian (mipsel). The arithmetic and the
//! conversions to an integer that round by the FCSR take its default mode, to nearest. Every
//! encoding outside the subset (the coprocessor 0, `COP1X`, the doubleword unaligned accesses,
//! the conditional traps on an immediate, a branch in a delay slot) traps with its address.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Block, Function, Terminator as T};
use ra_asm_arch::Instr;
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FCond, FnType, FunctionBuilder, ICond, Inst, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;

use crate::state::{Isa, MIPS_FCSR, MIPS_HI, MIPS_LO, Machine};
use crate::x86::trap;
use crate::{Lifted, function_name};

/// `ra`, the register a call links through.
const RA: u8 = 31;

/// Lifts one function.
pub fn lift_function(image: &Image, lifted: &mut Lifted, f: &Function) -> ra_asm_ir::Function {
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
    let (x87, isa) = (lifted.x87, lifted.isa);
    let mut lifter = Lifter { m: Machine { b: &mut b, state, bits: 32, x87, isa }, image, lifted, blocks, cond: None, jump_target: None };
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

/// The fields of an instruction word.
#[derive(Debug, Clone, Copy)]
struct Word(u32);

impl Word {
    fn op(self) -> u32 {
        self.0 >> 26
    }

    fn rs(self) -> u8 {
        ((self.0 >> 21) & 31) as u8
    }

    fn rt(self) -> u8 {
        ((self.0 >> 16) & 31) as u8
    }

    fn rd(self) -> u8 {
        ((self.0 >> 11) & 31) as u8
    }

    fn sa(self) -> u8 {
        ((self.0 >> 6) & 31) as u8
    }

    fn funct(self) -> u32 {
        self.0 & 63
    }

    /// The 16-bit immediate, sign-extended.
    fn simm(self) -> i64 {
        i64::from(self.0 as u16 as i16)
    }

    /// The 16-bit immediate, zero-extended.
    fn uimm(self) -> i64 {
        i64::from(self.0 & 0xffff)
    }

    /// The condition code a `bc1`, a `movf` or a `movt` reads.
    fn branch_cc(self) -> i64 {
        i64::from((self.0 >> 18) & 7)
    }

    /// Whether a `bc1`, a `movt` or a `movt.fmt` tests the condition code for true.
    fn tests_true(self) -> bool {
        (self.0 >> 16) & 1 == 1
    }

    /// Whether the word is a branch or a jump: an instruction with a delay slot.
    fn is_branch(self) -> bool {
        match self.op() {
            0 => matches!(self.funct(), 8 | 9),
            1 => matches!(self.rt(), 0..=3 | 16..=19),
            2..=7 | 20..=23 => true,
            17 => self.rs() == 8,
            _ => false,
        }
    }
}

/// Where a branch calls, once its slot has run.
#[derive(Clone, Copy)]
enum Call {
    Direct(u64),
    Register(ValueId),
}

struct Lifter<'a, 'b> {
    m: Machine<'b>,
    image: &'a Image,
    lifted: &'a mut Lifted,
    blocks: HashMap<u64, BlockId>,
    /// The condition of the conditional branch that ends the current block.
    cond: Option<ValueId>,
    /// The target of the jump through a register that ends the current block, as a 64-bit address.
    jump_target: Option<ValueId>,
}

impl Lifter<'_, '_> {
    fn warn(&mut self, text: String) {
        self.lifted.warnings.push(text);
    }

    fn word(&self, address: u64) -> Option<Word> {
        let bytes = self.image.read(address, 4).ok()?;
        Some(Word(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])))
    }

    fn fr64(&self) -> bool {
        self.m.isa == Isa::MipsN32
    }

    // region Blocks

    fn lift_block(&mut self, blk: &Block) {
        let mut block = self.blocks[&blk.start];
        self.m.b.set_block_address(block, blk.start);
        self.cond = None;
        self.jump_target = None;
        let mut n = 0;
        while let Some(i) = blk.instrs.get(n) {
            if self.m.b.has_terminator(block) {
                break;
            }
            self.m.b.address = Some(i.address);
            let Some(w) = self.word(i.address) else {
                self.m.trap(block, i.address, trap::INVALID);
                break;
            };
            n += 1;
            if w.is_branch() {
                let slot = blk.instrs.get(n).filter(|s| s.address == i.address.wrapping_add(4));
                if slot.is_some() {
                    n += 1;
                }
                block = self.lift_branch(block, w, i.address, slot);
            } else {
                block = self.lift_instr(block, w, i.address);
            }
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
                // A jump the analysis did not resolve: most often a tail call through a pointer.
                self.runtime_call(block, target);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::TailCall(t) => {
                self.call_lifted(block, *t);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::ImportTailCall(import) => {
                self.warn(format!("{last:#x}: a tail call of the import {import}, which the MIPS lift does not bind"));
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

    /// A branch or a jump and its delay slot: the condition, the target and the link from the
    /// registers before the slot, the slot, then the call a linking one makes. Where control
    /// goes after is the block's terminator.
    fn lift_branch(&mut self, block: BlockId, w: Word, ip: u64, slot: Option<&Instr>) -> BlockId {
        let (rs, rt) = (w.rs(), w.rt());
        let relative = ip.wrapping_add(4).wrapping_add((w.simm() << 2) as u64) & 0xffff_ffff;
        let (mut cond, mut likely, mut link, mut call, mut jump) = (None, false, None, None, None);
        match w.op() {
            0 => {
                let target = self.reg32(block, rs);
                let wide = self.m.b.cast(block, Cast::ZExt, Type::I32, target, Type::I64, "target");
                if w.funct() == 9 {
                    link = Some(w.rd());
                    call = Some(Call::Register(wide));
                } else {
                    jump = Some(wide);
                }
            }
            1 if rt & 16 == 0 => {
                let a = self.reg64(block, rs);
                let cc = if rt & 1 == 0 { ICond::Slt } else { ICond::Sge };
                cond = Some(self.m.b.icmp(block, cc, Type::I64, a, 0i64, "cond"));
                likely = rt & 2 != 0;
            }
            // `bal`: the unconditional `bgezal $0`.
            1 if rt == 17 && rs == 0 => {
                link = Some(RA);
                call = Some(Call::Direct(relative));
            }
            2 => {}
            3 => {
                link = Some(RA);
                call = Some(Call::Direct((ip.wrapping_add(4) & 0xf000_0000) | (u64::from(w.0 & 0x03ff_ffff) << 2)));
            }
            4..=7 | 20..=23 => {
                let a = self.reg64(block, rs);
                let c = match w.op() & 0xf {
                    4 | 5 => {
                        let b = self.reg64(block, rt);
                        let cc = if w.op() & 1 == 0 { ICond::Eq } else { ICond::Ne };
                        self.m.b.icmp(block, cc, Type::I64, a, b, "cond")
                    }
                    6 => self.m.b.icmp(block, ICond::Sle, Type::I64, a, 0i64, "cond"),
                    _ => self.m.b.icmp(block, ICond::Sgt, Type::I64, a, 0i64, "cond"),
                };
                cond = Some(c);
                likely = w.op() >= 20;
            }
            17 => {
                let f = self.fcc(block, w.branch_cc());
                cond = Some(if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "cond") });
                likely = (w.0 >> 17) & 1 == 1;
            }
            _ => return self.unsupported(block, w, ip),
        }
        if let Some(r) = link {
            self.set64(block, r, i64::from(ip.wrapping_add(8) as u32 as i32));
        }
        match slot {
            Some(s) => {
                let Some(sw) = self.word(s.address).filter(|sw| !sw.is_branch()) else {
                    self.warn(format!("{:#x}: a branch in the delay slot of the branch at {ip:#x}", s.address));
                    self.m.trap(block, s.address, trap::INVALID);
                    return block;
                };
                self.m.b.address = Some(s.address);
                if let (true, Some(c)) = (likely, cond) {
                    let then = self.m.b.block(&format!("slot_{:x}", s.address));
                    let next = self.m.b.block(&format!("taken_{ip:x}"));
                    self.m.b.terminate(block, Terminator::CondBr { cond: c.into(), then, els: next });
                    let end = self.lift_instr(then, sw, s.address);
                    if !self.m.b.has_terminator(end) {
                        self.m.b.terminate(end, Terminator::Br(next));
                    }
                    return self.after_slot(next, ip, cond, jump, call);
                }
                let end = self.lift_instr(block, sw, s.address);
                self.after_slot(end, ip, cond, jump, call)
            }
            // A likely branch's slot lies on its taken edge.
            None if likely => self.after_slot(block, ip, cond, jump, call),
            None => {
                self.warn(format!("{ip:#x}: a branch whose delay slot is not in its block"));
                self.m.trap(block, ip, trap::INVALID);
                block
            }
        }
    }

    fn after_slot(&mut self, block: BlockId, ip: u64, cond: Option<ValueId>, jump: Option<ValueId>, call: Option<Call>) -> BlockId {
        if self.m.b.has_terminator(block) {
            return block;
        }
        self.m.b.address = Some(ip);
        self.cond = cond;
        self.jump_target = jump;
        match call {
            Some(Call::Direct(t)) if self.lifted.functions.contains_key(&t) => self.call_lifted(block, t),
            Some(Call::Direct(t)) => self.runtime_call(block, t as i64),
            Some(Call::Register(target)) => self.runtime_call(block, target),
            None => {}
        }
        block
    }

    // endregion

    // region Calls

    /// `call F_target(state)`.
    fn call_lifted(&mut self, block: BlockId, target: u64) {
        let name = function_name(target);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// `call @ra_call(state, target)`: the runtime finds the lifted function at the address.
    fn runtime_call(&mut self, block: BlockId, target: impl Into<Operand>) {
        self.m.b.call(
            block,
            FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
            Callee::Direct("ra_call".into()),
            vec![(Type::Ptr, self.m.state.into()), (Type::I64, target.into())],
            "",
        );
    }

    // endregion

    // region Registers

    /// General register `n` at 64 bits; `$0` is zero.
    fn reg64(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I64, 0i64);
        }
        let p = self.m.gpr_ptr(block, u32::from(n));
        self.m.b.load(block, Type::I64, p, self.m.gpr_name(u32::from(n)))
    }

    /// The low word of general register `n`, which a 32-bit operation reads.
    fn reg32(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I32, 0i64);
        }
        let whole = self.reg64(block, n);
        let name = format!("{}.w", self.m.gpr_name(u32::from(n)));
        self.m.b.cast(block, Cast::Trunc, Type::I64, whole, Type::I32, &name)
    }

    fn reg(&mut self, block: BlockId, n: u8, ty: &Type) -> ValueId {
        if *ty == Type::I32 { self.reg32(block, n) } else { self.reg64(block, n) }
    }

    /// Writes general register `n`; a write to `$0` is dropped.
    fn set64(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        if n == 0 {
            return;
        }
        let p = self.m.gpr_ptr(block, u32::from(n));
        self.m.b.store(block, Type::I64, value, p);
    }

    /// Writes a 32-bit result, sign-extended into the register.
    fn set32(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        if n == 0 {
            return;
        }
        let name = format!("{}.x", self.m.gpr_name(u32::from(n)));
        let wide = self.m.b.cast(block, Cast::SExt, Type::I32, value, Type::I64, &name);
        self.set64(block, n, wide);
    }

    fn set(&mut self, block: BlockId, n: u8, ty: &Type, value: impl Into<Operand>) {
        if *ty == Type::I32 { self.set32(block, n, value) } else { self.set64(block, n, value) }
    }

    /// Writes `hi` or `lo`; a 32-bit value is sign-extended, as the 32-bit multiplies and divides
    /// leave it.
    fn set_field(&mut self, block: BlockId, index: u8, ty: &Type, value: impl Into<Operand>) {
        self.set(block, index, ty, value);
    }

    fn field(&mut self, block: BlockId, index: u8) -> ValueId {
        self.reg64(block, index)
    }

    /// The FPU's condition code `cc`.
    fn fcc(&mut self, block: BlockId, cc: i64) -> ValueId {
        let name = format!("fcc{cc}");
        let p = self.m.flag_ptr_at(block, cc, &name);
        let byte = self.m.b.load(block, Type::I8, p, &format!("{name}.b"));
        self.m.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, &name)
    }

    fn set_fcc(&mut self, block: BlockId, cc: i64, value: impl Into<Operand>) {
        let name = format!("fcc{cc}");
        let p = self.m.flag_ptr_at(block, cc, &name);
        let byte = self.m.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{name}.b"));
        self.m.b.store(block, Type::I8, byte, p);
    }

    /// The bits of FPU register `n` as a single (32) or a double (64).
    fn fpr(&mut self, block: BlockId, n: u8, double: bool) -> ValueId {
        if !double {
            return self.m.read_vreg(block, u32::from(n), Type::I32);
        }
        if self.fr64() {
            return self.m.read_vreg(block, u32::from(n), Type::I64);
        }
        let lo = self.m.read_vreg(block, u32::from(n & !1), Type::I32);
        let hi = self.m.read_vreg(block, u32::from(n | 1), Type::I32);
        let lo = self.m.b.cast(block, Cast::ZExt, Type::I32, lo, Type::I64, "lo");
        let hi = self.m.b.cast(block, Cast::ZExt, Type::I32, hi, Type::I64, "hi");
        let high = self.m.b.bin(block, BinOp::Shl, Type::I64, hi, 32i64, "high");
        self.m.b.bin(block, BinOp::Or, Type::I64, high, lo, &format!("f{n}.d"))
    }

    fn set_fpr(&mut self, block: BlockId, n: u8, double: bool, value: impl Into<Operand>) {
        if !double {
            self.m.write_vreg(block, u32::from(n), Type::I32, value, false);
        } else if self.fr64() {
            self.m.write_vreg(block, u32::from(n), Type::I64, value, false);
        } else {
            let value = self.m.value_of(block, Type::I64, value);
            let lo = self.m.b.cast(block, Cast::Trunc, Type::I64, value, Type::I32, "lo");
            let high = self.m.b.bin(block, BinOp::LShr, Type::I64, value, 32i64, "high");
            let hi = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "hi");
            self.m.write_vreg(block, u32::from(n & !1), Type::I32, lo, false);
            self.m.write_vreg(block, u32::from(n | 1), Type::I32, hi, false);
        }
    }

    fn freg(&mut self, block: BlockId, n: u8, double: bool) -> ValueId {
        let (ity, fty) = float_types(double);
        let bits = self.fpr(block, n, double);
        self.m.b.cast(block, Cast::Bitcast, ity, bits, fty, &format!("f{n}"))
    }

    fn set_freg(&mut self, block: BlockId, n: u8, double: bool, value: impl Into<Operand>) {
        let (ity, fty) = float_types(double);
        let bits = self.m.b.cast(block, Cast::Bitcast, fty, value, ity, "bits");
        self.set_fpr(block, n, double, bits);
    }

    // endregion

    // region Instructions

    fn unsupported(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        self.warn(format!("{ip:#x}: the MIPS lift does not cover the word {:#010x}", w.0));
        self.m.unsupported(block, ip, u64::from(w.0));
        block
    }

    /// A trap where `cond` holds; the lift goes on in the block where it does not.
    fn trap_if(&mut self, block: BlockId, cond: ValueId, ip: u64, code: u64) -> BlockId {
        let bad = self.m.b.block(&format!("trap_{ip:x}"));
        let ok = self.m.b.block(&format!("ok_{ip:x}"));
        self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then: bad, els: ok });
        self.m.trap(bad, ip, code);
        ok
    }

    /// `a + b` (or `a - b`), whose signed overflow traps: the block after the check and the result.
    fn checked_sum(&mut self, block: BlockId, ty: &Type, a: ValueId, b: impl Into<Operand>, sub: bool, ip: u64) -> (BlockId, ValueId) {
        let ty = ty.clone();
        let b: Operand = b.into();
        let result = self.m.b.bin(block, if sub { BinOp::Sub } else { BinOp::Add }, ty.clone(), a, b.clone(), "sum");
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), a, result, "ov.a");
        let y = if sub {
            self.m.b.bin(block, BinOp::Xor, ty.clone(), a, b, "ov.b")
        } else {
            self.m.b.bin(block, BinOp::Xor, ty.clone(), b, result, "ov.b")
        };
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "ov");
        let over = self.m.b.icmp(block, ICond::Slt, ty, both, 0i64, "overflow");
        (self.trap_if(block, over, ip, trap::INT), result)
    }

    /// The address a load or a store names: base plus the signed offset, at 32 bits.
    fn address(&mut self, block: BlockId, w: Word) -> ValueId {
        let base = self.reg32(block, w.rs());
        self.m.b.bin(block, BinOp::Add, Type::I32, base, w.simm(), "addr")
    }

    fn lift_instr(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rs, rt) = (w.rs(), w.rt());
        match w.op() {
            0 => return self.special(block, w, ip),
            // addi, daddi: the signed add that traps on overflow.
            8 | 24 => {
                let ty = if w.op() == 8 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let b = self.m.value_of(block, ty.clone(), w.simm());
                let (block, r) = self.checked_sum(block, &ty, a, b, false, ip);
                self.set(block, rt, &ty, r);
                return block;
            }
            9 | 25 => {
                let ty = if w.op() == 9 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let r = self.m.b.bin(block, BinOp::Add, ty.clone(), a, w.simm(), "sum");
                self.set(block, rt, &ty, r);
            }
            10 | 11 => {
                let a = self.reg64(block, rs);
                let cc = if w.op() == 10 { ICond::Slt } else { ICond::Ult };
                let c = self.m.b.icmp(block, cc, Type::I64, a, w.simm(), "lt");
                let r = self.m.b.cast(block, Cast::ZExt, Type::I1, c, Type::I64, "set");
                self.set64(block, rt, r);
            }
            12..=14 => {
                let op = [BinOp::And, BinOp::Or, BinOp::Xor][(w.op() - 12) as usize];
                let a = self.reg64(block, rs);
                let r = self.m.b.bin(block, op, Type::I64, a, w.uimm(), "logic");
                self.set64(block, rt, r);
            }
            15 => self.set64(block, rt, i64::from(((w.0 & 0xffff) << 16) as i32)),
            17 => return self.cop1(block, w, ip),
            32..=39 | 48 | 52 | 55 => self.load(block, w),
            40 | 41 | 43 | 56 | 60 | 63 => {
                let ty = match w.op() {
                    40 => Type::I8,
                    41 => Type::I16,
                    43 | 56 => Type::I32,
                    _ => Type::I64,
                };
                let addr = self.address(block, w);
                let v = self.reg64(block, rt);
                let v = self.m.trunc(block, Type::I64, v, ty.clone(), "st");
                self.m.store(block, ty, v, addr);
                // sc, scd: one thread, so the conditional store always succeeds.
                if matches!(w.op(), 56 | 60) {
                    self.set64(block, rt, 1i64);
                }
            }
            42 | 46 => {
                let addr = self.address(block, w);
                self.unaligned_store(block, addr, rt, w.op() == 42);
            }
            49 | 53 => {
                let addr = self.address(block, w);
                let double = w.op() == 53;
                let v = self.m.load(block, float_types(double).0, addr, "ld");
                self.set_fpr(block, rt, double, v);
            }
            57 | 61 => {
                let addr = self.address(block, w);
                let double = w.op() == 61;
                let v = self.fpr(block, rt, double);
                self.m.store(block, float_types(double).0, v, addr);
            }
            // pref: a hint.
            51 => {}
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    fn load(&mut self, block: BlockId, w: Word) {
        let rt = w.rt();
        let addr = self.address(block, w);
        let (ty, signed) = match w.op() {
            32 => (Type::I8, true),
            36 => (Type::I8, false),
            33 => (Type::I16, true),
            37 => (Type::I16, false),
            35 | 48 => (Type::I32, true),
            39 => (Type::I32, false),
            52 | 55 => (Type::I64, true),
            op => return self.unaligned_load(block, addr, rt, op == 34),
        };
        let v = self.m.load(block, ty.clone(), addr, "ld");
        let r = if signed { self.m.sext(block, ty, v, Type::I64, "ld.x") } else { self.m.zext(block, ty, v, Type::I64, "ld.x") };
        self.set64(block, rt, r);
    }

    /// `lwl` (`left`) and `lwr` on a little-endian machine: the bytes of the aligned word from the
    /// address to its end become the register's high bytes, or from its start to the address its
    /// low bytes; the rest of the register stays.
    fn unaligned_load(&mut self, block: BlockId, addr: ValueId, rt: u8, left: bool) {
        let k = self.m.b.bin(block, BinOp::And, Type::I32, addr, 3i64, "k");
        let aligned = self.m.b.bin(block, BinOp::And, Type::I32, addr, -4i64, "aligned");
        let word = self.m.load(block, Type::I32, aligned, "word");
        let old = self.reg32(block, rt);
        let bits = self.m.b.bin(block, BinOp::Shl, Type::I32, k, 3i64, "bits");
        let (part, keep) = if left {
            let s = self.m.b.bin(block, BinOp::Sub, Type::I32, 24i64, bits, "s");
            let part = self.m.b.bin(block, BinOp::Shl, Type::I32, word, s, "part");
            let one = self.m.b.bin(block, BinOp::Shl, Type::I32, 1i64, s, "one");
            (part, self.m.b.bin(block, BinOp::Sub, Type::I32, one, 1i64, "keep"))
        } else {
            let part = self.m.b.bin(block, BinOp::LShr, Type::I32, word, bits, "part");
            let mask = self.m.b.bin(block, BinOp::LShr, Type::I32, -1i64, bits, "mask");
            (part, self.m.b.bin(block, BinOp::Xor, Type::I32, mask, -1i64, "keep"))
        };
        let kept = self.m.b.bin(block, BinOp::And, Type::I32, old, keep, "kept");
        let merged = self.m.b.bin(block, BinOp::Or, Type::I32, part, kept, "merged");
        self.set32(block, rt, merged);
    }

    /// `swl` (`left`) and `swr` on a little-endian machine: the register's high bytes go to the
    /// aligned word from its start to the address, or its low bytes from the address to its end.
    fn unaligned_store(&mut self, block: BlockId, addr: ValueId, rt: u8, left: bool) {
        let k = self.m.b.bin(block, BinOp::And, Type::I32, addr, 3i64, "k");
        let aligned = self.m.b.bin(block, BinOp::And, Type::I32, addr, -4i64, "aligned");
        let word = self.m.load(block, Type::I32, aligned, "word");
        let value = self.reg32(block, rt);
        let bits = self.m.b.bin(block, BinOp::Shl, Type::I32, k, 3i64, "bits");
        let (part, keep) = if left {
            let s = self.m.b.bin(block, BinOp::Sub, Type::I32, 24i64, bits, "s");
            let part = self.m.b.bin(block, BinOp::LShr, Type::I32, value, s, "part");
            let mask = self.m.b.bin(block, BinOp::LShr, Type::I32, -1i64, s, "mask");
            (part, self.m.b.bin(block, BinOp::Xor, Type::I32, mask, -1i64, "keep"))
        } else {
            let part = self.m.b.bin(block, BinOp::Shl, Type::I32, value, bits, "part");
            let one = self.m.b.bin(block, BinOp::Shl, Type::I32, 1i64, bits, "one");
            (part, self.m.b.bin(block, BinOp::Sub, Type::I32, one, 1i64, "keep"))
        };
        let kept = self.m.b.bin(block, BinOp::And, Type::I32, word, keep, "kept");
        let merged = self.m.b.bin(block, BinOp::Or, Type::I32, part, kept, "merged");
        self.m.store(block, Type::I32, merged, aligned);
    }

    /// The `SPECIAL` opcode: the register-to-register operations.
    fn special(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rs, rt, rd, sa) = (w.rs(), w.rt(), w.rd(), w.sa());
        let funct = w.funct();
        let shift_op = || [BinOp::Shl, BinOp::Shl, BinOp::LShr, BinOp::AShr][(funct & 3) as usize];
        match funct {
            0 | 2 | 3 | 4 | 6 | 7 => {
                let v = self.reg32(block, rt);
                let amount: Operand = if funct < 4 { i64::from(sa).into() } else { self.reg_masked(block, rs, Type::I32, 31).into() };
                let r = self.m.b.bin(block, shift_op(), Type::I32, v, amount, "shift");
                self.set32(block, rd, r);
            }
            20 | 22 | 23 | 56 | 58 | 59 | 60 | 62 | 63 => {
                let v = self.reg64(block, rt);
                let amount: Operand = match funct {
                    20..=23 => self.reg_masked(block, rs, Type::I64, 63).into(),
                    56..=59 => i64::from(sa).into(),
                    _ => (i64::from(sa) + 32).into(),
                };
                let r = self.m.b.bin(block, shift_op(), Type::I64, v, amount, "shift");
                self.set64(block, rd, r);
            }
            // movf, movt: on an FPU condition code.
            1 => {
                let f = self.fcc(block, w.branch_cc());
                let c = if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "not") };
                self.move_if(block, c, rs, rd);
            }
            10 | 11 => {
                let t = self.reg64(block, rt);
                let cc = if funct == 10 { ICond::Eq } else { ICond::Ne };
                let c = self.m.b.icmp(block, cc, Type::I64, t, 0i64, "c");
                self.move_if(block, c, rs, rd);
            }
            // syscall, by o32 Linux: the number in `v0`, the arguments from `a0`; the result in
            // `v0` and the error flag in `a3`, which a failure sets with the error number in `v0`.
            12 => {
                let args: Vec<(Type, Operand)> = [2, 4, 5, 6, 7].iter().map(|&n| (Type::I64, self.reg64(block, n).into())).collect();
                let raw = self
                    .m
                    .b
                    .call(block, crate::state::syscall_type(), Callee::Direct("ra_syscall".into()), args, "sys")
                    .expect("an i64 result");
                let failed = self.m.b.icmp(block, ICond::Uge, Type::I64, raw, -4095i64, "sys.failed");
                let magnitude = self.m.b.bin(block, BinOp::Sub, Type::I64, 0i64, raw, "sys.errno");
                let v0 = self.m.b.select(block, failed, Type::I64, magnitude, raw, "sys.v0");
                let a3 = self.m.b.cast(block, Cast::ZExt, Type::I1, failed, Type::I64, "sys.a3");
                self.set64(block, 2, v0);
                self.set64(block, 7, a3);
            }
            13 => {
                self.m.trap(block, ip, trap::INT3);
                return block;
            }
            // sync: one thread sees its memory in order.
            15 => {}
            16 | 18 => {
                let v = self.field(block, if funct == 16 { MIPS_HI } else { MIPS_LO });
                self.set64(block, rd, v);
            }
            17 | 19 => {
                let v = self.reg64(block, rs);
                self.set_field(block, if funct == 17 { MIPS_HI } else { MIPS_LO }, &Type::I64, v);
            }
            24 | 25 | 28 | 29 => {
                let (ty, wide, bits) = if funct < 28 { (Type::I32, Type::I64, 32) } else { (Type::I64, Type::I128, 64) };
                let cast = if funct & 1 == 0 { Cast::SExt } else { Cast::ZExt };
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let a = self.m.b.cast(block, cast, ty.clone(), a, wide.clone(), "a");
                let b = self.m.b.cast(block, cast, ty.clone(), b, wide.clone(), "b");
                let p = self.m.b.bin(block, BinOp::Mul, wide.clone(), a, b, "product");
                let lo = self.m.b.cast(block, Cast::Trunc, wide.clone(), p, ty.clone(), "lo");
                let high = self.m.b.bin(block, BinOp::LShr, wide.clone(), p, bits, "high");
                let hi = self.m.b.cast(block, Cast::Trunc, wide, high, ty.clone(), "hi");
                self.set_field(block, MIPS_LO, &ty, lo);
                self.set_field(block, MIPS_HI, &ty, hi);
            }
            26 | 27 | 30 | 31 => {
                let ty = if funct < 30 { Type::I32 } else { Type::I64 };
                let signed = funct & 1 == 0;
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                // The quotient of a division by zero, and of the most negative number by -1, is
                // unpredictable: the lift stops rather than pick one.
                let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, 0i64, "zero");
                let mut block = self.trap_if(block, zero, ip, trap::DIVIDE);
                if signed {
                    let min = if ty == Type::I32 { i64::from(i32::MIN) } else { i64::MIN };
                    let is_min = self.m.b.icmp(block, ICond::Eq, ty.clone(), a, min, "min");
                    let minus = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, -1i64, "minus");
                    let both = self.m.b.bin(block, BinOp::And, Type::I1, is_min, minus, "overflow");
                    block = self.trap_if(block, both, ip, trap::DIVIDE_WIDE);
                }
                let (div, rem) = if signed { (BinOp::SDiv, BinOp::SRem) } else { (BinOp::UDiv, BinOp::URem) };
                let q = self.m.b.bin(block, div, ty.clone(), a, b, "quotient");
                let r = self.m.b.bin(block, rem, ty.clone(), a, b, "remainder");
                self.set_field(block, MIPS_LO, &ty, q);
                self.set_field(block, MIPS_HI, &ty, r);
                return block;
            }
            32 | 34 | 44 | 46 => {
                let ty = if funct < 44 { Type::I32 } else { Type::I64 };
                let sub = funct & 2 != 0;
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let (block, r) = self.checked_sum(block, &ty, a, b, sub, ip);
                self.set(block, rd, &ty, r);
                return block;
            }
            33 | 35 | 45 | 47 => {
                let ty = if funct < 45 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let r = self.m.b.bin(block, if funct & 2 != 0 { BinOp::Sub } else { BinOp::Add }, ty.clone(), a, b, "sum");
                self.set(block, rd, &ty, r);
            }
            36..=39 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let op = [BinOp::And, BinOp::Or, BinOp::Xor, BinOp::Or][(funct - 36) as usize];
                let mut r = self.m.b.bin(block, op, Type::I64, a, b, "logic");
                if funct == 39 {
                    r = self.m.b.bin(block, BinOp::Xor, Type::I64, r, -1i64, "nor");
                }
                self.set64(block, rd, r);
            }
            42 | 43 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let cc = if funct == 42 { ICond::Slt } else { ICond::Ult };
                let c = self.m.b.icmp(block, cc, Type::I64, a, b, "lt");
                let r = self.m.b.cast(block, Cast::ZExt, Type::I1, c, Type::I64, "set");
                self.set64(block, rd, r);
            }
            48..=52 | 54 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let cc = [ICond::Sge, ICond::Uge, ICond::Slt, ICond::Ult, ICond::Eq, ICond::Ne, ICond::Ne][(funct - 48) as usize];
                let c = self.m.b.icmp(block, cc, Type::I64, a, b, "trap");
                return self.trap_if(block, c, ip, trap::INT);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// The low bits of a register a variable shift takes its amount from.
    fn reg_masked(&mut self, block: BlockId, n: u8, ty: Type, mask: i64) -> ValueId {
        let v = self.reg(block, n, &ty);
        self.m.b.bin(block, BinOp::And, ty, v, mask, "amount")
    }

    /// `rd = c ? rs : rd`.
    fn move_if(&mut self, block: BlockId, c: ValueId, rs: u8, rd: u8) {
        let from = self.reg64(block, rs);
        let old = self.reg64(block, rd);
        let r = self.m.select(block, c, Type::I64, from, old, "moved");
        self.set64(block, rd, r);
    }

    // endregion

    // region The FPU

    fn cop1(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (fmt, rt, fs) = (w.rs(), w.rt(), w.rd());
        match fmt {
            0 | 1 => {
                let v = self.fpr(block, fs, fmt == 1);
                if fmt == 0 { self.set32(block, rt, v) } else { self.set64(block, rt, v) }
            }
            4 => {
                let v = self.reg32(block, rt);
                self.set_fpr(block, fs, false, v);
            }
            5 => {
                let v = self.reg64(block, rt);
                self.set_fpr(block, fs, true, v);
            }
            // cfc1 of the FCSR: the field, with the condition codes in their bits.
            2 if fs == 31 => {
                let p = self.m.field_ptr(block, MIPS_FCSR, "p.fcsr");
                let v = self.m.b.load(block, Type::I32, p, "fcsr");
                let mut v = self.m.b.bin(block, BinOp::And, Type::I32, v, !FCC_BITS, "fcsr.rest");
                for cc in 0..8 {
                    let f = self.fcc(block, cc);
                    let f = self.m.b.cast(block, Cast::ZExt, Type::I1, f, Type::I32, "fcc.w");
                    let at = self.m.b.bin(block, BinOp::Shl, Type::I32, f, fcc_bit(cc), "fcc.at");
                    v = self.m.b.bin(block, BinOp::Or, Type::I32, v, at, "fcsr");
                }
                self.set32(block, rt, v);
            }
            6 if fs == 31 => {
                let v = self.reg32(block, rt);
                let p = self.m.field_ptr(block, MIPS_FCSR, "p.fcsr");
                self.m.b.store(block, Type::I32, v, p);
                for cc in 0..8 {
                    let at = self.m.b.bin(block, BinOp::LShr, Type::I32, v, fcc_bit(cc), "fcc.at");
                    let f = self.m.b.cast(block, Cast::Trunc, Type::I32, at, Type::I1, "fcc");
                    self.set_fcc(block, cc, f);
                }
            }
            16 | 17 => return self.fpu_arith(block, w, ip, fmt == 17),
            // cvt.s and cvt.d of a word (W) or a long (L).
            20 | 21 if matches!(w.funct(), 32 | 33) => {
                let long = fmt == 21;
                let bits = self.fpr(block, fs, long);
                let double = w.funct() == 33;
                let to = float_types(double).1;
                let v = self.m.b.cast(block, Cast::SIToFP, float_types(long).0, bits, to, "cvt");
                self.set_freg(block, w.sa(), double, v);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// The `S` and `D` formats: `fd = fs op ft`, the conversions, the moves and the compares.
    fn fpu_arith(&mut self, block: BlockId, w: Word, ip: u64, double: bool) -> BlockId {
        let (ft, fs, fd) = (w.rt(), w.rd(), w.sa());
        let (ity, fty) = float_types(double);
        let sign = if double { i64::MIN } else { i64::from(i32::MIN) };
        let suffix = if double { "f64" } else { "f32" };
        let funct = w.funct();
        match funct {
            0..=3 => {
                let op = [BinOp::FAdd, BinOp::FSub, BinOp::FMul, BinOp::FDiv][funct as usize];
                let a = self.freg(block, fs, double);
                let b = self.freg(block, ft, double);
                let r = self.m.b.bin(block, op, fty, a, b, "f");
                self.set_freg(block, fd, double, r);
            }
            4 => {
                let a = self.freg(block, fs, double);
                let r = self.m.intrinsic(block, &format!("llvm.sqrt.{suffix}"), fty.clone(), vec![(fty, a.into())], "sqrt");
                self.set_freg(block, fd, double, r);
            }
            // abs, mov, neg: on the bits, which keeps a NaN's payload as the FPU does.
            5..=7 => {
                let a = self.fpr(block, fs, double);
                let r = match funct {
                    5 => self.m.b.bin(block, BinOp::And, ity, a, !sign, "abs"),
                    6 => a,
                    _ => self.m.b.bin(block, BinOp::Xor, ity, a, sign, "neg"),
                };
                self.set_fpr(block, fd, double, r);
            }
            // round, trunc, ceil, floor to a long (8..=11) or a word; cvt.w and cvt.l round to nearest.
            8..=15 | 36 | 37 => {
                let a = self.freg(block, fs, double);
                let (mode, long) = if funct >= 36 { (0, funct == 37) } else { (funct & 3, funct < 12) };
                let r = self.float_to_int(block, a, double, mode, long);
                self.set_fpr(block, fd, long, r);
            }
            32 if double => {
                let a = self.freg(block, fs, true);
                let r = self.m.b.cast(block, Cast::FPTrunc, Type::Double, a, Type::Float, "cvt");
                self.set_freg(block, fd, false, r);
            }
            33 if !double => {
                let a = self.freg(block, fs, false);
                let r = self.m.b.cast(block, Cast::FPExt, Type::Float, a, Type::Double, "cvt");
                self.set_freg(block, fd, true, r);
            }
            17..=19 => {
                let c = if funct == 17 {
                    let f = self.fcc(block, w.branch_cc());
                    if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "not") }
                } else {
                    let t = self.reg64(block, ft);
                    let cc = if funct == 18 { ICond::Eq } else { ICond::Ne };
                    self.m.b.icmp(block, cc, Type::I64, t, 0i64, "c")
                };
                let from = self.fpr(block, fs, double);
                let old = self.fpr(block, fd, double);
                let r = self.m.select(block, c, ity, from, old, "moved");
                self.set_fpr(block, fd, double, r);
            }
            // c.cond.fmt: bit 0 of the condition takes unordered, bit 1 equal, bit 2 less; bit 3,
            // the signalling form, changes no result.
            48..=63 => {
                let a = self.freg(block, fs, double);
                let b = self.freg(block, ft, double);
                let mut r = self.m.value_of(block, Type::I1, 0i64);
                for (bit, cond) in [(1, FCond::Uno), (2, FCond::Oeq), (4, FCond::Olt)] {
                    if funct & bit != 0 {
                        let c = self.m.b.value(block, Inst::FCmp { cond, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, cond.name());
                        r = self.m.b.bin(block, BinOp::Or, Type::I1, r, c, "fcc");
                    }
                }
                self.set_fcc(block, i64::from((w.0 >> 8) & 7), r);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// A float rounded by `mode` (to nearest, toward zero, up, down) to a word or a long; a value
    /// out of range or a NaN gives the FPU's invalid result, the largest positive integer.
    fn float_to_int(&mut self, block: BlockId, a: ValueId, double: bool, mode: u32, long: bool) -> ValueId {
        let fty = float_types(double).1;
        let suffix = if double { "f64" } else { "f32" };
        let rounding = ["roundeven", "trunc", "ceil", "floor"][mode as usize];
        let r = self.m.intrinsic(block, &format!("llvm.{rounding}.{suffix}"), fty.clone(), vec![(fty.clone(), a.into())], "rounded");
        let (ity, bits) = if long { (Type::I64, 64) } else { (Type::I32, 32) };
        let limit = 2f64.powi(bits - 1);
        let low = self.m.b.value(block, Inst::FCmp { cond: FCond::Oge, ty: fty.clone(), lhs: r.into(), rhs: Operand::Float(-limit) }, "ge");
        let high = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: fty.clone(), lhs: r.into(), rhs: Operand::Float(limit) }, "lt");
        let fits = self.m.b.bin(block, BinOp::And, Type::I1, low, high, "fits");
        let v = self.m.intrinsic(block, &format!("llvm.fptosi.sat.i{bits}.{suffix}"), ity.clone(), vec![(fty, r.into())], "int");
        let invalid = if long { i64::MAX } else { i64::from(i32::MAX) };
        self.m.select(block, fits, ity, v, invalid, "cvt")
    }

    // endregion
}

/// The condition codes' bits in the FCSR: `cc0` at 23, `cc1` to `cc7` at 25 to 31.
const FCC_BITS: i64 = 0xfe80_0000_u32 as i32 as i64;

fn fcc_bit(cc: i64) -> i64 {
    if cc == 0 { 23 } else { 24 + cc }
}

/// The integer and the float type of a single or a double.
fn float_types(double: bool) -> (Type, Type) {
    if double { (Type::I64, Type::Double) } else { (Type::I32, Type::Float) }
}
