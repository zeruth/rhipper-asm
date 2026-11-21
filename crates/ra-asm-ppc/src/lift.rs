//! The lift of a 32-bit PowerPC program at the machine's own level: its code as one function of
//! the IR over the machine's state.
//!
//! [`RUN`] takes the state and the address to start at. The state is slots of eight bytes, a
//! register's value in the low four: `r0` to `r31`, then `lr`, `ctr`, the condition register and
//! `xer`, then the floating-point registers, eight bytes each. Every instruction word of the code is a block that reads the state, computes, and
//! writes it back. A branch goes to the block of its target; a branch to `lr` or to `ctr` goes to
//! the dispatcher, which finds the block of the address. Memory is the program's own addresses,
//! from where [`MEMORY`] says its address zero is, and its numbers are the machine's, the high
//! byte first: a load and a store turn the bytes round, so the lifted code is right on a host of
//! either order. An `sc` is a call of [`SC`] with the state; a word that is no instruction the
//! lift has, where the code runs into it, is a call of [`TRAP`].
//!
//! What is lifted is the integer machine of the consoles' processors: the arithmetic with its
//! carries, the logic, the shifts and the rotates under a mask, the compares into a field of the
//! condition register and the record forms that compare a result with zero, the loads and
//! stores in their plain, indexed and updating forms, the conditional branches with the count
//! register, and the moves to and from the special registers.
//!
//! The floating-point unit is lifted too, its thirty-two registers a double each in the slots
//! from [`F0`]: the arithmetic in doubles and in singles, a single kept as the double it equals
//! and rounded after each operation, the multiply-adds, the compares into a field of the
//! condition register, the conversion to a whole number toward zero, the moves of the sign, and
//! the loads and stores. A multiply-add is a product and then a sum, each rounded, where the
//! processor rounds once; `fpscr`, with its rounding modes and its exceptions, is not kept. The
//! paired singles, the vector instructions and the overflow bit are not lifted.

use std::collections::BTreeMap;

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FCond, FnType, FunctionBuilder, Global, ICond, Inst, Linkage, Module,
    Operand, Terminator, Type, ValueId,
};

/// The function the lift writes: `void (ptr state, i64 pc)`.
pub const RUN: &str = "ppc_run";
/// The runtime's answer to an `sc`: `void (ptr state)`.
pub const SC: &str = "ppc_sc";
/// What a word that is no instruction calls, with its address: `void (i64 pc)`.
pub const TRAP: &str = "ppc_trap";
/// Where the program's memory is: eight bytes that hold the address the program's address zero
/// is at; zero where the program is mapped at its own addresses.
pub const MEMORY: &str = "ppc_memory";
/// The function of a whole program ([`crate::run::Program::lift`]): `void (ptr state)`.
pub const MAIN: &str = "ppc_main";
/// The runtime's taking of a segment: `void (i64 address, ptr bytes, i64 in_file, i64 size)`.
pub const SEGMENT: &str = "ppc_segment";

/// The slots of the state after the thirty-two general registers.
pub const LR: u64 = 32;
pub const CTR: u64 = 33;
pub const CR: u64 = 34;
pub const XER: u64 = 35;
/// The slots of a state.
pub const SLOTS: u64 = 69;
/// The first of the thirty-two floating-point registers, a double each, and the slot of `fpscr`.
pub const F0: u64 = 36;
pub const FPSCR: u64 = 68;

/// The carry bit of `xer`, and its summary-overflow bit.
const CA: u32 = 0x2000_0000;

/// The code of a program: its bytes, big-endian words, and the address of the first.
#[derive(Debug, Clone, Copy)]
pub struct Code<'a> {
    pub address: u64,
    pub bytes: &'a [u8],
}

struct Lift {
    b: FunctionBuilder,
    state: ValueId,
    block: BlockId,
}

const I32: Type = Type::I32;

/// The bits `from..=to` of a word, numbered as the machine's manuals number them: bit 0 is the highest.
fn field(word: u32, from: u32, to: u32) -> u32 {
    (word >> (31 - to)) & ((1u64 << (to - from + 1)) - 1) as u32
}

/// The mask of the rotates: the bits from `begin` to `end`, round the word when `begin` is after `end`.
fn mask(begin: u32, end: u32) -> u32 {
    let (from, to) = (u32::MAX >> begin, u32::MAX << (31 - end));
    if begin <= end { from & to } else { from | to }
}

impl Lift {
    fn slot(&mut self, n: u64) -> ValueId {
        self.b.gep(self.block, Type::I8, self.state, vec![(Type::I64, Operand::Int(8 * i128::from(n)))], "at")
    }

    fn get(&mut self, n: u64) -> Operand {
        let at = self.slot(n);
        self.b.load(self.block, I32, at, "r").into()
    }

    /// A base register, which reads as zero when it is `r0`.
    fn base(&mut self, n: u64) -> Operand {
        if n == 0 { Operand::Int(0) } else { self.get(n) }
    }

    fn set(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(n);
        self.b.store(self.block, I32, value, at);
    }

    fn bin(&mut self, op: BinOp, a: impl Into<Operand>, b: impl Into<Operand>) -> Operand {
        self.b.bin(self.block, op, I32, a, b, "v").into()
    }

    fn cmp(&mut self, cond: ICond, a: impl Into<Operand>, b: impl Into<Operand>) -> ValueId {
        self.b.icmp(self.block, cond, I32, a, b, "is")
    }

    fn select(&mut self, is: ValueId, then: impl Into<Operand>, els: impl Into<Operand>) -> Operand {
        self.b.select(self.block, is, I32, then, els, "v").into()
    }

    fn bit(&mut self, is: ValueId) -> Operand {
        self.b.cast(self.block, Cast::ZExt, Type::I1, is, I32, "bit").into()
    }

    /// The bytes of a word or a halfword turned round: the machine's order to the host's.
    fn swap(&mut self, size: u32, value: Operand) -> Operand {
        if size == 2 {
            let (high, low) = (self.bin(BinOp::Shl, value.clone(), 8), self.bin(BinOp::LShr, value, 8));
            let (high, low) = (self.bin(BinOp::And, high, 0xff00), self.bin(BinOp::And, low, 0xff));
            return self.bin(BinOp::Or, high, low);
        }
        let (a, b) = (self.bin(BinOp::Shl, value.clone(), 24), self.bin(BinOp::LShr, value.clone(), 24));
        let (c, d) = (self.bin(BinOp::Shl, value.clone(), 8), self.bin(BinOp::LShr, value, 8));
        let (c, d) = (self.bin(BinOp::And, c, 0x00ff_0000), self.bin(BinOp::And, d, 0xff00));
        let (outer, inner) = (self.bin(BinOp::Or, a, b), self.bin(BinOp::Or, c, d));
        self.bin(BinOp::Or, outer, inner)
    }

    fn pointer(&mut self, address: Operand) -> ValueId {
        let wide = self.b.cast(self.block, Cast::ZExt, I32, address, Type::I64, "address");
        let memory = self.b.load(self.block, Type::I64, Operand::Global(MEMORY.to_owned()), "memory");
        let wide = self.b.bin(self.block, BinOp::Add, Type::I64, wide, memory, "address");
        self.b.cast(self.block, Cast::IntToPtr, Type::I64, wide, Type::Ptr, "pointer")
    }

    /// The byte, halfword or word at `address`, a narrow one extended without its sign.
    fn load(&mut self, size: u32, address: Operand) -> Operand {
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value = self.b.load(self.block, ty.clone(), at, "loaded");
        let value: Operand = if size == 4 { value.into() } else { self.b.cast(self.block, Cast::ZExt, ty, value, I32, "loaded").into() };
        if size == 1 { value } else { self.swap(size, value) }
    }

    fn store(&mut self, size: u32, value: Operand, address: Operand) {
        let value = if size == 1 { value } else { self.swap(size, value) };
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value: Operand = if size == 4 { value } else { self.b.cast(self.block, Cast::Trunc, I32, value, ty.clone(), "narrow").into() };
        self.b.store(self.block, ty, value, at);
    }

    /// The carry bit of `xer`, as zero or one.
    fn carry(&mut self) -> Operand {
        let xer = self.get(XER);
        let shifted = self.bin(BinOp::LShr, xer, 29);
        self.bin(BinOp::And, shifted, 1)
    }

    fn set_carry(&mut self, carried: ValueId) {
        let xer = self.get(XER);
        let cleared = self.bin(BinOp::And, xer, !CA);
        let bit = self.select(carried, CA, 0);
        let xer = self.bin(BinOp::Or, cleared, bit);
        self.set(XER, xer);
    }

    /// `a + b + carry` at twice the width: the sum, and whether it carried out of the word.
    fn add(&mut self, a: Operand, b: Operand, carry: Operand) -> (Operand, ValueId) {
        let extend = |lift: &mut Self, v: Operand| lift.b.cast(lift.block, Cast::ZExt, I32, v, Type::I64, "wide");
        let (a, b, carry) = (extend(self, a), extend(self, b), extend(self, carry));
        let sum = self.b.bin(self.block, BinOp::Add, Type::I64, a, b, "sum");
        let sum = self.b.bin(self.block, BinOp::Add, Type::I64, sum, carry, "sum");
        let low: Operand = self.b.cast(self.block, Cast::Trunc, Type::I64, sum, I32, "v").into();
        let above = self.b.bin(self.block, BinOp::LShr, Type::I64, sum, 32, "above");
        let carried = self.b.icmp(self.block, ICond::Ne, Type::I64, above, 0, "carried");
        (low, carried)
    }

    /// Writes a field of the condition register: less, greater, equal, and the summary overflow.
    fn set_field(&mut self, which: u32, less: ValueId, greater: ValueId, equal: ValueId) {
        let (less, greater, equal) = (self.bit(less), self.bit(greater), self.bit(equal));
        let (less, greater, equal) = (self.bin(BinOp::Shl, less, 3), self.bin(BinOp::Shl, greater, 2), self.bin(BinOp::Shl, equal, 1));
        let xer = self.get(XER);
        let summary = self.bin(BinOp::LShr, xer, 31);
        self.place_field(which, less, greater, equal, summary);
    }

    /// Writes a field of the condition register from its four bits, each zero or one in place.
    fn place_field(&mut self, which: u32, less: Operand, greater: Operand, equal: Operand, summary: Operand) {
        let bits = self.bin(BinOp::Or, less, greater);
        let bits = self.bin(BinOp::Or, bits, equal);
        let bits = self.bin(BinOp::Or, bits, summary);
        let shift = 28 - 4 * which;
        let placed = self.bin(BinOp::Shl, bits, shift);
        let cr = self.get(CR);
        let cleared = self.bin(BinOp::And, cr, !(0xfu32 << shift));
        let cr = self.bin(BinOp::Or, cleared, placed);
        self.set(CR, cr);
    }

    /// A floating-point register as the double it holds.
    fn fget(&mut self, n: u64) -> Operand {
        let at = self.slot(F0 + n);
        self.b.load(self.block, Type::Double, at, "f").into()
    }

    fn fset(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(F0 + n);
        self.b.store(self.block, Type::Double, value, at);
    }

    /// A floating-point register as its sixty-four bits.
    fn fbits(&mut self, n: u64) -> Operand {
        let at = self.slot(F0 + n);
        self.b.load(self.block, Type::I64, at, "bits").into()
    }

    fn set_fbits(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(F0 + n);
        self.b.store(self.block, Type::I64, value, at);
    }

    fn fbin(&mut self, op: BinOp, a: impl Into<Operand>, b: impl Into<Operand>) -> Operand {
        self.b.bin(self.block, op, Type::Double, a, b, "f").into()
    }

    fn fcmp(&mut self, cond: FCond, a: Operand, b: impl Into<Operand>) -> ValueId {
        self.b.value(self.block, Inst::FCmp { cond, ty: Type::Double, lhs: a, rhs: b.into() }, "is")
    }

    /// A double rounded to the single nearest it, as a double.
    fn single(&mut self, value: Operand) -> Operand {
        let narrow = self.b.cast(self.block, Cast::FPTrunc, Type::Double, value, Type::Float, "single");
        self.b.cast(self.block, Cast::FPExt, Type::Float, narrow, Type::Double, "f").into()
    }

    fn negated(&mut self, value: Operand) -> Operand {
        let bits = self.b.cast(self.block, Cast::Bitcast, Type::Double, value, Type::I64, "bits");
        let bits = self.b.bin(self.block, BinOp::Xor, Type::I64, bits, Operand::Int(1 << 63), "bits");
        self.b.cast(self.block, Cast::Bitcast, Type::I64, bits, Type::Double, "f").into()
    }

    /// A load into the floating-point register `d` or a store of it, a single or a double.
    fn float_access(&mut self, single: bool, is_store: bool, d: u64, address: Operand) {
        let second = self.bin(BinOp::Add, address.clone(), 4);
        match (is_store, single) {
            (false, true) => {
                let bits = self.load(4, address);
                let value = self.b.cast(self.block, Cast::Bitcast, I32, bits, Type::Float, "single");
                let value = self.b.cast(self.block, Cast::FPExt, Type::Float, value, Type::Double, "f");
                self.fset(d, value);
            }
            (false, false) => {
                // The high word is first.
                let (high, low) = (self.load(4, address), self.load(4, second));
                let high = self.b.cast(self.block, Cast::ZExt, I32, high, Type::I64, "high");
                let low = self.b.cast(self.block, Cast::ZExt, I32, low, Type::I64, "low");
                let high = self.b.bin(self.block, BinOp::Shl, Type::I64, high, 32, "high");
                let bits = self.b.bin(self.block, BinOp::Or, Type::I64, high, low, "bits");
                self.set_fbits(d, bits);
            }
            (true, true) => {
                let value = self.fget(d);
                let value = self.b.cast(self.block, Cast::FPTrunc, Type::Double, value, Type::Float, "single");
                let bits: Operand = self.b.cast(self.block, Cast::Bitcast, Type::Float, value, I32, "bits").into();
                self.store(4, bits, address);
            }
            (true, false) => {
                let bits = self.fbits(d);
                let high = self.b.bin(self.block, BinOp::LShr, Type::I64, bits.clone(), 32, "high");
                let high: Operand = self.b.cast(self.block, Cast::Trunc, Type::I64, high, I32, "high").into();
                let low: Operand = self.b.cast(self.block, Cast::Trunc, Type::I64, bits, I32, "low").into();
                self.store(4, high, address);
                self.store(4, low, second);
            }
        }
    }

    /// The instructions of the floating-point unit, of primary opcode 59 (singles) and 63.
    fn float(&mut self, w: u32) -> bool {
        let (d, a, b, c) =
            (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)), u64::from(field(w, 16, 20)), u64::from(field(w, 21, 25)));
        let single = w >> 26 == 59;
        if !single {
            // The forms with one operand and the compares, by the whole extended opcode.
            match (w >> 1) & 0x3ff {
                0 | 32 => {
                    // An ordered compare differs from an unordered one in the exceptions it raises.
                    let (x, y) = (self.fget(a), self.fget(b));
                    let (less, greater) = (self.fcmp(FCond::Olt, x.clone(), y.clone()), self.fcmp(FCond::Ogt, x.clone(), y.clone()));
                    let (equal, neither) = (self.fcmp(FCond::Oeq, x.clone(), y.clone()), self.fcmp(FCond::Uno, x, y));
                    let (less, greater, equal, neither) = (self.bit(less), self.bit(greater), self.bit(equal), self.bit(neither));
                    let (less, greater, equal) =
                        (self.bin(BinOp::Shl, less, 3), self.bin(BinOp::Shl, greater, 2), self.bin(BinOp::Shl, equal, 1));
                    self.place_field(field(w, 6, 8), less, greater, equal, neither);
                    return true;
                }
                12 => {
                    let value = self.fget(b);
                    let value = self.single(value);
                    self.fset(d, value);
                    return true;
                }
                15 => {
                    // To a whole number toward zero, in the low word: what is past an end of the
                    // word is that end, and what is no number is the lowest.
                    let value = self.fget(b);
                    let high = self.fcmp(FCond::Ogt, value.clone(), Operand::Float(2_147_483_647.0));
                    let low = self.fcmp(FCond::Olt, value.clone(), Operand::Float(-2_147_483_648.0));
                    let neither = self.fcmp(FCond::Uno, value.clone(), value.clone());
                    let low = self.b.bin(self.block, BinOp::Or, Type::I1, low, neither, "low");
                    let outside = self.b.bin(self.block, BinOp::Or, Type::I1, low, high, "outside");
                    let inside = self.b.select(self.block, outside, Type::Double, Operand::Float(0.0), value, "inside");
                    let whole = self.b.cast(self.block, Cast::FPToSI, Type::Double, inside, I32, "whole");
                    let whole = self.select(low, 0x8000_0000u32, whole);
                    let whole = self.select(high, 0x7fff_ffff, whole);
                    let wide = self.b.cast(self.block, Cast::ZExt, I32, whole, Type::I64, "wide");
                    let bits = self.b.bin(self.block, BinOp::Or, Type::I64, wide, Operand::Int(0xfff8_0000 << 32), "bits");
                    self.set_fbits(d, bits);
                    return true;
                }
                xo @ (40 | 72 | 136 | 264) => {
                    // The moves, which are of the bits: as they are, the sign turned, set or cleared.
                    let bits = self.fbits(b);
                    let sign = Operand::Int(1 << 63);
                    let bits: Operand = match xo {
                        40 => self.b.bin(self.block, BinOp::Xor, Type::I64, bits, sign, "bits").into(),
                        136 => self.b.bin(self.block, BinOp::Or, Type::I64, bits, sign, "bits").into(),
                        264 => self.b.bin(self.block, BinOp::And, Type::I64, bits, Operand::Int(i128::from(i64::MAX)), "bits").into(),
                        _ => bits,
                    };
                    self.set_fbits(d, bits);
                    return true;
                }
                _ => {}
            }
        }
        let (x, y) = (self.fget(a), self.fget(b));
        let value = match (w >> 1) & 0x1f {
            18 => self.fbin(BinOp::FDiv, x, y),
            20 => self.fbin(BinOp::FSub, x, y),
            21 => self.fbin(BinOp::FAdd, x, y),
            25 => {
                let z = self.fget(c);
                self.fbin(BinOp::FMul, x, z)
            }
            xo @ 28..=31 => {
                // A product and a sum. The processor rounds once, after the sum; here the product
                // is rounded too, which differs where the product is not exact.
                let z = self.fget(c);
                let product = self.fbin(BinOp::FMul, x, z);
                let sum = self.fbin(if xo & 1 == 0 { BinOp::FSub } else { BinOp::FAdd }, product, y);
                if xo >= 30 { self.negated(sum) } else { sum }
            }
            23 if !single => {
                let z = self.fget(c);
                let positive = self.fcmp(FCond::Oge, x, Operand::Float(0.0));
                self.b.select(self.block, positive, Type::Double, z, y, "f").into()
            }
            _ => return false,
        };
        let value = if single { self.single(value) } else { value };
        self.fset(d, value);
        true
    }

    /// The record form's compare of a result with zero, into the first field.
    fn record(&mut self, value: Operand) {
        let (less, greater, equal) =
            (self.cmp(ICond::Slt, value.clone(), 0), self.cmp(ICond::Sgt, value.clone(), 0), self.cmp(ICond::Eq, value, 0));
        self.set_field(0, less, greater, equal);
    }

    fn rotate(&mut self, value: Operand, by: u32) -> Operand {
        if by == 0 {
            return value;
        }
        let (left, right) = (self.bin(BinOp::Shl, value.clone(), by), self.bin(BinOp::LShr, value, 32 - by));
        self.bin(BinOp::Or, left, right)
    }

    /// The count of the zero bits above a word's highest one bit.
    fn leading_zeros(&mut self, value: Operand) -> Operand {
        let (mut count, mut rest): (Operand, Operand) = (Operand::Int(0), value.clone());
        for step in [16u32, 8, 4, 2, 1] {
            let top = self.bin(BinOp::LShr, rest.clone(), 32 - step);
            let none = self.cmp(ICond::Eq, top, 0);
            let more = self.bin(BinOp::Add, count.clone(), step);
            let moved = self.bin(BinOp::Shl, rest.clone(), step);
            count = self.select(none, more, count);
            rest = self.select(none, moved, rest);
        }
        let zero = self.cmp(ICond::Eq, value, 0);
        self.select(zero, 32, count)
    }

    /// An instruction that is no transfer of control: whether the lift has it.
    #[allow(clippy::too_many_lines)]
    fn simple(&mut self, w: u32) -> bool {
        let (d, a, b) = (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)), u64::from(field(w, 16, 20)));
        let simm = i64::from(w as u16 as i16);
        let uimm = i64::from(w & 0xffff);
        let rc = w & 1 != 0;
        match w >> 26 {
            7 => {
                let value = self.get(a);
                let product = self.bin(BinOp::Mul, value, simm);
                self.set(d, product);
            }
            8 | 12 | 13 => {
                // `subfic` takes the register from the immediate; `addic` adds them.
                let value = self.get(a);
                let (sum, carried) = if w >> 26 == 8 {
                    let inverse = self.bin(BinOp::Xor, value, -1);
                    self.add(inverse, Operand::Int(i128::from(simm as u32)), Operand::Int(1))
                } else {
                    self.add(value, Operand::Int(i128::from(simm as u32)), Operand::Int(0))
                };
                self.set(d, sum.clone());
                self.set_carry(carried);
                if w >> 26 == 13 {
                    self.record(sum);
                }
            }
            10 | 11 => {
                let value = self.get(a);
                let (less, greater, other): (ICond, ICond, i64) =
                    if w >> 26 == 10 { (ICond::Ult, ICond::Ugt, uimm) } else { (ICond::Slt, ICond::Sgt, simm) };
                let (less, greater, equal) =
                    (self.cmp(less, value.clone(), other), self.cmp(greater, value.clone(), other), self.cmp(ICond::Eq, value, other));
                self.set_field(field(w, 6, 8), less, greater, equal);
            }
            14 | 15 => {
                let base = self.base(a);
                let sum = self.bin(BinOp::Add, base, if w >> 26 == 14 { simm } else { simm << 16 });
                self.set(d, sum);
            }
            20 | 21 | 23 => {
                // Rotate left and keep the bits under the mask; `rlwimi` keeps the rest of the target.
                let source = self.get(d);
                let rotated = if w >> 26 == 23 {
                    let count = self.get(b);
                    let count = self.bin(BinOp::And, count, 31);
                    let back = self.bin(BinOp::Sub, 32, count.clone());
                    let back = self.bin(BinOp::And, back, 31);
                    let (left, right) = (self.bin(BinOp::Shl, source.clone(), count), self.bin(BinOp::LShr, source, back));
                    self.bin(BinOp::Or, left, right)
                } else {
                    self.rotate(source, field(w, 16, 20))
                };
                let keep = mask(field(w, 21, 25), field(w, 26, 30));
                let mut value = self.bin(BinOp::And, rotated, keep);
                if w >> 26 == 20 {
                    let target = self.get(a);
                    let rest = self.bin(BinOp::And, target, !keep);
                    value = self.bin(BinOp::Or, value, rest);
                }
                self.set(a, value.clone());
                if rc {
                    self.record(value);
                }
            }
            24..=29 => {
                let source = self.get(d);
                let op = [BinOp::Or, BinOp::Or, BinOp::Xor, BinOp::Xor, BinOp::And, BinOp::And][(w >> 26) as usize - 24];
                let value = self.bin(op, source, if (w >> 26) & 1 == 0 { uimm } else { uimm << 16 });
                self.set(a, value.clone());
                if w >> 26 >= 28 {
                    self.record(value);
                }
            }
            32..=45 => {
                // The loads and the stores with a displacement; the odd ones leave the address in the base.
                let op = w >> 26;
                let update = op & 1 != 0;
                let base = if update { self.get(a) } else { self.base(a) };
                let address = self.bin(BinOp::Add, base, simm);
                let (size, is_store, signed) = match op {
                    32 | 33 => (4, false, false),
                    34 | 35 => (1, false, false),
                    36 | 37 => (4, true, false),
                    38 | 39 => (1, true, false),
                    40 | 41 => (2, false, false),
                    42 | 43 => (2, false, true),
                    _ => (2, true, false),
                };
                self.access(size, is_store, signed, d, address.clone());
                if update {
                    self.set(a, address);
                }
            }
            46 | 47 => {
                // The registers from the one named to the last, a word each.
                let base = self.base(a);
                for (n, register) in (d..32).enumerate() {
                    let address = self.bin(BinOp::Add, base.clone(), simm + 4 * n as i64);
                    self.access(4, w >> 26 == 47, false, register, address);
                }
            }
            48..=55 => {
                // The floating-point loads and stores with a displacement: a single is kept as the
                // double it is equal to.
                let op = (w >> 26) - 48;
                let update = op & 1 != 0;
                let base = if update { self.get(a) } else { self.base(a) };
                let address = self.bin(BinOp::Add, base, simm);
                self.float_access(op & 2 == 0, op & 4 != 0, d, address.clone());
                if update {
                    self.set(a, address);
                }
            }
            59 | 63 => return self.float(w),
            31 => return self.extended(w),
            _ => return false,
        }
        true
    }

    /// A load into register `d` or a store of it, at `address`.
    fn access(&mut self, size: u32, is_store: bool, signed: bool, d: u64, address: Operand) {
        if is_store {
            let value = self.get(d);
            self.store(size, value, address);
            return;
        }
        let mut value = self.load(size, address);
        if signed {
            let narrow = self.b.cast(self.block, Cast::Trunc, I32, value, Type::I16, "narrow");
            value = self.b.cast(self.block, Cast::SExt, Type::I16, narrow, I32, "v").into();
        }
        self.set(d, value);
    }

    /// The instructions of primary opcode 31, by their extended opcode.
    #[allow(clippy::too_many_lines)]
    fn extended(&mut self, w: u32) -> bool {
        let (d, a, b) = (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)), u64::from(field(w, 16, 20)));
        let rc = w & 1 != 0;
        let xo = (w >> 1) & 0x3ff;
        // The arithmetic forms keep their overflow enable in the extended opcode's top bit.
        let arithmetic = xo & 0x1ff;
        match xo {
            0 | 32 => {
                let (x, y) = (self.get(a), self.get(b));
                let (less, greater) = if xo == 0 { (ICond::Slt, ICond::Sgt) } else { (ICond::Ult, ICond::Ugt) };
                let (less, greater, equal) =
                    (self.cmp(less, x.clone(), y.clone()), self.cmp(greater, x.clone(), y.clone()), self.cmp(ICond::Eq, x, y));
                self.set_field(field(w, 6, 8), less, greater, equal);
                return true;
            }
            28 | 60 | 124 | 284 | 316 | 412 | 444 | 476 => {
                let (x, y) = (self.get(d), self.get(b));
                let inverse = |lift: &mut Self, v: Operand| lift.bin(BinOp::Xor, v, -1);
                let value = match xo {
                    28 => self.bin(BinOp::And, x, y),
                    60 => {
                        let y = inverse(self, y);
                        self.bin(BinOp::And, x, y)
                    }
                    124 => {
                        let either = self.bin(BinOp::Or, x, y);
                        inverse(self, either)
                    }
                    284 => {
                        let differ = self.bin(BinOp::Xor, x, y);
                        inverse(self, differ)
                    }
                    316 => self.bin(BinOp::Xor, x, y),
                    412 => {
                        let y = inverse(self, y);
                        self.bin(BinOp::Or, x, y)
                    }
                    444 => self.bin(BinOp::Or, x, y),
                    _ => {
                        let both = self.bin(BinOp::And, x, y);
                        inverse(self, both)
                    }
                };
                self.set(a, value.clone());
                if rc {
                    self.record(value);
                }
                return true;
            }
            24 | 536 | 792 => {
                // A shift by a register counts by six bits: thirty-two and more leaves nothing, or all sign.
                let (value, count) = (self.get(d), self.get(b));
                let count = self.bin(BinOp::And, count, 63);
                let whole = self.cmp(ICond::Ugt, count.clone(), 31);
                let low = self.bin(BinOp::And, count, 31);
                let shifted = match xo {
                    24 => self.bin(BinOp::Shl, value.clone(), low.clone()),
                    536 => self.bin(BinOp::LShr, value.clone(), low.clone()),
                    _ => self.bin(BinOp::AShr, value.clone(), low.clone()),
                };
                let all = if xo == 792 { self.bin(BinOp::AShr, value.clone(), 31) } else { Operand::Int(0) };
                let result = self.select(whole, all, shifted.clone());
                if xo == 792 {
                    // The carry: a negative value that lost one bits.
                    let back = self.bin(BinOp::Shl, shifted, low);
                    let lost = self.cmp(ICond::Ne, back, value.clone());
                    let lost = self.bit(lost);
                    let any = self.cmp(ICond::Ne, value.clone(), 0);
                    let any = self.bit(any);
                    let lost = self.select(whole, any, lost);
                    let negative = self.bin(BinOp::LShr, value, 31);
                    let carried = self.bin(BinOp::And, lost, negative);
                    let carried = self.cmp(ICond::Ne, carried, 0);
                    self.set_carry(carried);
                }
                self.set(a, result.clone());
                if rc {
                    self.record(result);
                }
                return true;
            }
            824 => {
                let value = self.get(d);
                let count = field(w, 16, 20);
                let result = self.bin(BinOp::AShr, value.clone(), count);
                let lost = self.bin(BinOp::And, value.clone(), (1u64 << count) as u32 - 1);
                let negative = self.cmp(ICond::Slt, value, 0);
                let lost = self.cmp(ICond::Ne, lost, 0);
                let both = self.b.bin(self.block, BinOp::And, Type::I1, negative, lost, "carried");
                self.set_carry(both);
                self.set(a, result.clone());
                if rc {
                    self.record(result);
                }
                return true;
            }
            26 | 922 | 954 => {
                let value = self.get(d);
                let result = if xo == 26 {
                    self.leading_zeros(value)
                } else {
                    let ty = if xo == 954 { Type::I8 } else { Type::I16 };
                    let narrow = self.b.cast(self.block, Cast::Trunc, I32, value, ty.clone(), "narrow");
                    self.b.cast(self.block, Cast::SExt, ty, narrow, I32, "v").into()
                };
                self.set(a, result.clone());
                if rc {
                    self.record(result);
                }
                return true;
            }
            23 | 55 | 87 | 119 | 279 | 311 | 343 | 375 | 151 | 183 | 215 | 247 | 407 | 439 => {
                // The indexed loads and stores; the updating ones leave the address in the base.
                let (size, is_store, signed, update) = match xo {
                    23 => (4, false, false, false),
                    55 => (4, false, false, true),
                    87 => (1, false, false, false),
                    119 => (1, false, false, true),
                    279 => (2, false, false, false),
                    311 => (2, false, false, true),
                    343 => (2, false, true, false),
                    375 => (2, false, true, true),
                    151 => (4, true, false, false),
                    183 => (4, true, false, true),
                    215 => (1, true, false, false),
                    247 => (1, true, false, true),
                    407 => (2, true, false, false),
                    _ => (2, true, false, true),
                };
                let (base, index) = (if update { self.get(a) } else { self.base(a) }, self.get(b));
                let address = self.bin(BinOp::Add, base, index);
                self.access(size, is_store, signed, d, address.clone());
                if update {
                    self.set(a, address);
                }
                return true;
            }
            339 | 467 => {
                // A special register, its number's two halves the other way round.
                let number = field(w, 16, 20) << 5 | field(w, 11, 15);
                let slot = match number {
                    1 => XER,
                    8 => LR,
                    9 => CTR,
                    _ => return false,
                };
                if xo == 339 {
                    let value = self.get(slot);
                    self.set(d, value);
                } else {
                    let value = self.get(d);
                    self.set(slot, value);
                }
                return true;
            }
            19 => {
                let value = self.get(CR);
                self.set(d, value);
                return true;
            }
            144 => {
                // The fields of the condition register the mask names, from the register.
                let fields = field(w, 12, 19);
                let keep = (0..8).filter(|n| fields & (0x80 >> n) != 0).fold(0u32, |keep, n| keep | 0xf000_0000 >> (4 * n));
                let (value, cr) = (self.get(d), self.get(CR));
                let (taken, kept) = (self.bin(BinOp::And, value, keep), self.bin(BinOp::And, cr, !keep));
                let cr = self.bin(BinOp::Or, taken, kept);
                self.set(CR, cr);
                return true;
            }
            535 | 567 | 599 | 631 | 663 | 695 | 727 | 759 | 983 => {
                // The floating-point loads and stores by an index; `stfiwx` stores the low word as it is.
                let (single, is_store, update) = match xo {
                    535 => (true, false, false),
                    567 => (true, false, true),
                    599 => (false, false, false),
                    631 => (false, false, true),
                    663 => (true, true, false),
                    695 => (true, true, true),
                    727 => (false, true, false),
                    759 => (false, true, true),
                    _ => (false, true, false),
                };
                let (base, index) = (if update { self.get(a) } else { self.base(a) }, self.get(b));
                let address = self.bin(BinOp::Add, base, index);
                if xo == 983 {
                    let bits = self.fbits(d);
                    let low: Operand = self.b.cast(self.block, Cast::Trunc, Type::I64, bits, I32, "low").into();
                    self.store(4, low, address.clone());
                } else {
                    self.float_access(single, is_store, d, address.clone());
                }
                if update {
                    self.set(a, address);
                }
                return true;
            }
            _ => {}
        }
        let (x, y) = (self.get(a), self.get(b));
        let inverse = self.bin(BinOp::Xor, x.clone(), -1);
        let ones = Operand::Int(0xffff_ffff);
        // The sum each arithmetic form is: its two terms and the carry it starts with, and
        // whether it leaves a carry.
        let sum = match arithmetic {
            266 => Some((x.clone(), y.clone(), Operand::Int(0), false)),
            10 => Some((x.clone(), y.clone(), Operand::Int(0), true)),
            138 => Some((x.clone(), y.clone(), self.carry(), true)),
            202 => Some((x.clone(), Operand::Int(0), self.carry(), true)),
            234 => Some((x.clone(), ones.clone(), self.carry(), true)),
            40 => Some((inverse.clone(), y.clone(), Operand::Int(1), false)),
            8 => Some((inverse.clone(), y.clone(), Operand::Int(1), true)),
            136 => Some((inverse.clone(), y.clone(), self.carry(), true)),
            200 => Some((inverse.clone(), Operand::Int(0), self.carry(), true)),
            232 => Some((inverse.clone(), ones, self.carry(), true)),
            104 => Some((inverse, Operand::Int(0), Operand::Int(1), false)),
            _ => None,
        };
        let value = if let Some((first, second, carry, carries)) = sum {
            let (value, carried) = self.add(first, second, carry);
            if carries {
                self.set_carry(carried);
            }
            value
        } else {
            match arithmetic {
                235 => self.bin(BinOp::Mul, x, y),
                75 | 11 => {
                    let cast = if arithmetic == 75 { Cast::SExt } else { Cast::ZExt };
                    let (x, y) = (
                        self.b.cast(self.block, cast, I32, x, Type::I64, "wide"),
                        self.b.cast(self.block, cast, I32, y, Type::I64, "wide"),
                    );
                    let product = self.b.bin(self.block, BinOp::Mul, Type::I64, x, y, "product");
                    let high = self.b.bin(self.block, BinOp::LShr, Type::I64, product, 32, "high");
                    self.b.cast(self.block, Cast::Trunc, Type::I64, high, I32, "v").into()
                }
                459 | 491 => {
                    // A division by zero, and the one that overflows, leave what the processor
                    // pleases: here the dividend, divided by one.
                    let zero = self.cmp(ICond::Eq, y.clone(), 0);
                    let mut bad = zero;
                    if arithmetic == 491 {
                        let (least, minus) = (self.cmp(ICond::Eq, x.clone(), i64::from(i32::MIN)), self.cmp(ICond::Eq, y.clone(), -1));
                        let overflows = self.b.bin(self.block, BinOp::And, Type::I1, least, minus, "overflows");
                        bad = self.b.bin(self.block, BinOp::Or, Type::I1, zero, overflows, "bad");
                    }
                    let by = self.select(bad, 1, y);
                    self.bin(if arithmetic == 491 { BinOp::SDiv } else { BinOp::UDiv }, x, by)
                }
                _ => return false,
            }
        };
        self.set(d, value.clone());
        if rc {
            self.record(value);
        }
        true
    }
}

/// Lifts `code` into a module with the one function [`RUN`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lift(code: Code<'_>) -> Module {
    let mut module = Module::x86_64_windows("ppc");
    module.globals.push(Global {
        name: MEMORY.to_owned(),
        ty: Type::array(8, Type::I8),
        init: Some(Const::Bytes(vec![0; 8])),
        constant: false,
        linkage: Linkage::External,
        align: 8,
        address: None,
    });
    let declare = |module: &mut Module, name: &str, params: Vec<Type>| {
        module.declare(Declaration { name: name.to_owned(), ty: FnType::new(Type::Void, params), conv: CallConv::C, attrs: Vec::new() });
    };
    declare(&mut module, SC, vec![Type::Ptr]);
    declare(&mut module, TRAP, vec![Type::I64]);
    let mut b = FunctionBuilder::new(RUN, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), &["state", "start"]);
    let (entry, dispatch, nowhere) = (b.block("entry"), b.block("dispatch"), b.block("nowhere"));
    let state = b.param(0);
    // Where a branch to `lr` or `ctr` goes, for the dispatcher to find the block of.
    let pc = b.alloca(entry, Type::I64, "pc");
    let start = b.param(1);
    b.store(entry, Type::I64, start, pc);
    b.terminate(entry, Terminator::Br(dispatch));
    let words: Vec<(u64, u32)> = code
        .bytes
        .chunks_exact(4)
        .enumerate()
        .map(|(n, w)| (code.address + 4 * n as u64, u32::from_be_bytes([w[0], w[1], w[2], w[3]])))
        .collect();
    let mut blocks: BTreeMap<u64, BlockId> = BTreeMap::new();
    for &(address, _) in &words {
        b.address = Some(address);
        blocks.insert(address, b.block(&format!("at.{address:x}")));
    }
    let mut lift = Lift { b, state, block: entry };
    let trap = |lift: &mut Lift, address: Operand| {
        lift.b.call(lift.block, FnType::new(Type::Void, vec![Type::I64]), Callee::Direct(TRAP.to_owned()), vec![(Type::I64, address)], "");
        lift.b.terminate(lift.block, Terminator::Ret(None));
    };
    for &(address, w) in &words {
        lift.block = blocks[&address];
        lift.b.address = Some(address);
        let at = |target: u64| blocks.get(&target).copied().unwrap_or(nowhere);
        let next = address + 4;
        let link = w & 1 != 0;
        let absolute = w & 2 != 0;
        let relative =
            |offset: i64| if absolute { offset.cast_unsigned() & 0xffff_ffff } else { address.wrapping_add_signed(offset) & 0xffff_ffff };
        let through = (w >> 26 == 19).then_some((w >> 1) & 0x3ff).filter(|xo| matches!(xo, 16 | 528));
        match (w >> 26, through) {
            (18, _) => {
                let target = relative(i64::from(((w & 0x03ff_fffc) << 6).cast_signed() >> 6));
                if link {
                    lift.set(LR, next as i64);
                }
                lift.b.terminate(lift.block, Terminator::Br(at(target)));
            }
            (16, _) | (19, Some(_)) => {
                // A conditional branch: the count register comes down and is tested unless the
                // branch says not, and a bit of the condition register is tested unless it says not.
                let (options, bit) = (field(w, 6, 10), field(w, 11, 15));
                let mut taken: Option<ValueId> = None;
                if options & 0b00100 == 0 && through != Some(528) {
                    let count = lift.get(CTR);
                    let count = lift.bin(BinOp::Sub, count, 1);
                    lift.set(CTR, count.clone());
                    taken = Some(lift.cmp(if options & 0b00010 == 0 { ICond::Ne } else { ICond::Eq }, count, 0));
                }
                if options & 0b10000 == 0 {
                    let cr = lift.get(CR);
                    let one = lift.bin(BinOp::And, cr, 0x8000_0000u32 >> bit);
                    let holds = lift.cmp(if options & 0b01000 == 0 { ICond::Eq } else { ICond::Ne }, one, 0);
                    taken = Some(match taken {
                        Some(counted) => lift.b.bin(lift.block, BinOp::And, Type::I1, counted, holds, "taken"),
                        None => holds,
                    });
                }
                let goes = match through {
                    None => at(relative(i64::from(w as u16 as i16 & !3))),
                    Some(xo) => {
                        // Where `lr` or `ctr` says, as it was before the branch linked.
                        let target = lift.get(if xo == 16 { LR } else { CTR });
                        let target = lift.bin(BinOp::And, target, -4);
                        let wide = lift.b.cast(lift.block, Cast::ZExt, I32, target, Type::I64, "target");
                        lift.b.store(lift.block, Type::I64, wide, pc);
                        dispatch
                    }
                };
                if link {
                    lift.set(LR, next as i64);
                }
                let terminator = match taken {
                    Some(taken) => Terminator::CondBr { cond: taken.into(), then: goes, els: at(next) },
                    None => Terminator::Br(goes),
                };
                lift.b.terminate(lift.block, terminator);
            }
            (17, _) => {
                lift.b.call(
                    lift.block,
                    FnType::new(Type::Void, vec![Type::Ptr]),
                    Callee::Direct(SC.to_owned()),
                    vec![(Type::Ptr, state.into())],
                    "",
                );
                lift.b.terminate(lift.block, Terminator::Br(at(next)));
            }
            _ => {
                if lift.simple(w) {
                    lift.b.terminate(lift.block, Terminator::Br(at(next)));
                } else {
                    trap(&mut lift, Operand::Int(i128::from(address)));
                }
            }
        }
    }
    let target = lift.b.load(dispatch, Type::I64, pc, "target");
    let cases = blocks.iter().map(|(&address, &block)| (i128::from(address), block)).collect();
    lift.b.terminate(dispatch, Terminator::Switch { ty: Type::I64, value: target.into(), default: nowhere, cases });
    lift.block = nowhere;
    let target = lift.b.load(nowhere, Type::I64, pc, "target");
    trap(&mut lift, target.into());
    module.functions.push(lift.b.finish());
    module
}
