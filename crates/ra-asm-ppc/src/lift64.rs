//! The lift of a 64-bit PowerPC program: the code of the later consoles' processors, as
//! [`crate::lift`] writes a 32-bit program's.
//!
//! The function, the state and the runtime's calls are the 32-bit lift's ([`RUN`], [`SC`],
//! [`TRAP`], [`MEMORY`]); a general register, `lr` and `ctr` fill their slot's eight bytes, and
//! the condition register and `xer` stay the low four of theirs. What differs is the machine: an
//! address and a sum are sixty-four bits, with the carry out of the sixty-fourth; a compare says
//! which width it is of; the word instructions (`rlwinm`, `slw`, `srawi`, `mullw`, `cntlzw`) work
//! on the low half and say what the high half becomes; and the doubleword ones are here (`ld`,
//! `std`, the `rld` rotates under a mask, `sld`, `srd`, `srad`, `mulld`, `divd`, `extsw`).
//!
//! The integer machine and the floating-point unit are lifted, the unit as the 32-bit lift has
//! it, with a double moved as one doubleword and the conversions to and from a doubleword's whole
//! number (`fcfid`, `fctidz`). The vector instructions, the status register's rounding modes and
//! the overflow bit are not.

use std::collections::BTreeMap;

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FCond, FnType, FunctionBuilder, Global, ICond, Inst, Linkage, Module,
    Operand, Terminator, Type, ValueId,
};

use crate::lift::{CR, CTR, Code, F0, LR, MEMORY, RUN, SC, TRAP, XER};

const I32: Type = Type::I32;
const I64: Type = Type::I64;

/// The carry bit of `xer`.
const CA: u32 = 0x2000_0000;

struct Lift {
    b: FunctionBuilder,
    state: ValueId,
    block: BlockId,
}

fn field(word: u32, from: u32, to: u32) -> u32 {
    (word >> (31 - to)) & ((1u64 << (to - from + 1)) - 1) as u32
}

/// The mask of a doubleword rotate: the bits from `begin` to `end`, bit 0 the highest, round the
/// doubleword when `begin` is after `end`.
fn mask(begin: u32, end: u32) -> u64 {
    let (from, to) = (u64::MAX >> begin, u64::MAX << (63 - end));
    if begin <= end { from & to } else { from | to }
}

fn int(value: u64) -> Operand {
    Operand::Int(i128::from(value))
}

impl Lift {
    fn slot(&mut self, n: u64) -> ValueId {
        self.b.gep(self.block, Type::I8, self.state, vec![(I64, Operand::Int(8 * i128::from(n)))], "at")
    }

    fn get(&mut self, n: u64) -> Operand {
        let at = self.slot(n);
        self.b.load(self.block, I64, at, "r").into()
    }

    /// A base register, which reads as zero when it is `r0`.
    fn base(&mut self, n: u64) -> Operand {
        if n == 0 { Operand::Int(0) } else { self.get(n) }
    }

    fn set(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(n);
        self.b.store(self.block, I64, value, at);
    }

    /// The condition register or `xer`: thirty-two bits.
    fn get32(&mut self, n: u64) -> Operand {
        let at = self.slot(n);
        self.b.load(self.block, I32, at, "r").into()
    }

    fn set32(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(n);
        self.b.store(self.block, I32, value, at);
    }

    fn bin(&mut self, op: BinOp, a: impl Into<Operand>, b: impl Into<Operand>) -> Operand {
        self.b.bin(self.block, op, I64, a, b, "v").into()
    }

    fn bin32(&mut self, op: BinOp, a: impl Into<Operand>, b: impl Into<Operand>) -> Operand {
        self.b.bin(self.block, op, I32, a, b, "w").into()
    }

    fn cmp(&mut self, cond: ICond, a: impl Into<Operand>, b: impl Into<Operand>) -> ValueId {
        self.b.icmp(self.block, cond, I64, a, b, "is")
    }

    fn select(&mut self, is: ValueId, then: impl Into<Operand>, els: impl Into<Operand>) -> Operand {
        self.b.select(self.block, is, I64, then, els, "v").into()
    }

    fn any(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.b.bin(self.block, BinOp::Or, Type::I1, a, b, "either")
    }

    fn both(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.b.bin(self.block, BinOp::And, Type::I1, a, b, "both")
    }

    /// The low half of a doubleword.
    fn low(&mut self, value: Operand) -> Operand {
        self.b.cast(self.block, Cast::Trunc, I64, value, I32, "low").into()
    }

    /// A word as a doubleword, its high half zero or its sign.
    fn widen(&mut self, value: Operand, signed: bool) -> Operand {
        self.b.cast(self.block, if signed { Cast::SExt } else { Cast::ZExt }, I32, value, I64, "wide").into()
    }

    /// The low `bits` of a doubleword with their sign through the rest.
    fn sign(&mut self, value: Operand, bits: u16) -> Operand {
        let narrow = self.b.cast(self.block, Cast::Trunc, I64, value, Type::Int(bits), "narrow");
        self.b.cast(self.block, Cast::SExt, Type::Int(bits), narrow, I64, "v").into()
    }

    /// The bytes of the low `size` bytes of a doubleword turned round, the rest zero.
    fn swap(&mut self, size: u32, value: Operand) -> Operand {
        let mut value = value;
        // Neighbours change places: bytes, then pairs, then quads, as far as the size goes.
        for (by, mask) in [(8u32, 0x00ff_00ff_00ff_00ffu64), (16, 0x0000_ffff_0000_ffff), (32, 0x0000_0000_ffff_ffff)] {
            if by >= 8 * size {
                break;
            }
            let high = self.bin(BinOp::LShr, value.clone(), by);
            let high = self.bin(BinOp::And, high, int(mask));
            let low = self.bin(BinOp::And, value, int(mask));
            let low = self.bin(BinOp::Shl, low, by);
            value = self.bin(BinOp::Or, high, low);
        }
        if size == 8 { value } else { self.bin(BinOp::And, value, int((1u64 << (8 * size)) - 1)) }
    }

    fn pointer(&mut self, address: Operand) -> ValueId {
        let memory = self.b.load(self.block, I64, Operand::Global(MEMORY.to_owned()), "memory");
        let at = self.b.bin(self.block, BinOp::Add, I64, address, memory, "address");
        self.b.cast(self.block, Cast::IntToPtr, I64, at, Type::Ptr, "pointer")
    }

    /// What is at `address`, `size` bytes, extended without its sign.
    fn load(&mut self, size: u32, address: Operand) -> Operand {
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value = self.b.load(self.block, ty.clone(), at, "loaded");
        let value: Operand = if size == 8 { value.into() } else { self.b.cast(self.block, Cast::ZExt, ty, value, I64, "loaded").into() };
        if size == 1 { value } else { self.swap(size, value) }
    }

    fn store(&mut self, size: u32, value: Operand, address: Operand) {
        let value = if size == 1 { value } else { self.swap(size, value) };
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value: Operand = if size == 8 { value } else { self.b.cast(self.block, Cast::Trunc, I64, value, ty.clone(), "narrow").into() };
        self.b.store(self.block, ty, value, at);
    }

    /// The carry bit of `xer`, as zero or one.
    fn carry(&mut self) -> Operand {
        let xer = self.get32(XER);
        let shifted = self.bin32(BinOp::LShr, xer, 29);
        let bit = self.bin32(BinOp::And, shifted, 1);
        self.widen(bit, false)
    }

    fn set_carry(&mut self, carried: ValueId) {
        let xer = self.get32(XER);
        let cleared = self.bin32(BinOp::And, xer, !CA);
        let bit: Operand = self.b.select(self.block, carried, I32, CA, 0, "ca").into();
        let xer = self.bin32(BinOp::Or, cleared, bit);
        self.set32(XER, xer);
    }

    /// `a + b + carry`: the sum, and whether it carried out of the doubleword.
    fn add(&mut self, a: Operand, b: Operand, carry: Operand) -> (Operand, ValueId) {
        let first = self.bin(BinOp::Add, a.clone(), b);
        let over = self.cmp(ICond::Ult, first.clone(), a);
        let sum = self.bin(BinOp::Add, first.clone(), carry);
        let again = self.cmp(ICond::Ult, sum.clone(), first);
        let carried = self.any(over, again);
        (sum, carried)
    }

    /// Writes a field of the condition register: less, greater, equal, and the summary overflow.
    fn set_field(&mut self, which: u32, less: ValueId, greater: ValueId, equal: ValueId) {
        let bit = |lift: &mut Self, is: ValueId, shift: u32| -> Operand {
            let bit: Operand = lift.b.cast(lift.block, Cast::ZExt, Type::I1, is, I32, "bit").into();
            lift.bin32(BinOp::Shl, bit, shift)
        };
        let (less, greater, equal) = (bit(self, less, 3), bit(self, greater, 2), bit(self, equal, 1));
        let xer = self.get32(XER);
        let summary = self.bin32(BinOp::LShr, xer, 31);
        let bits = self.bin32(BinOp::Or, less, greater);
        let bits = self.bin32(BinOp::Or, bits, equal);
        let bits = self.bin32(BinOp::Or, bits, summary);
        let shift = 28 - 4 * which;
        let placed = self.bin32(BinOp::Shl, bits, shift);
        let cr = self.get32(CR);
        let cleared = self.bin32(BinOp::And, cr, !(0xfu32 << shift));
        let cr = self.bin32(BinOp::Or, cleared, placed);
        self.set32(CR, cr);
    }

    /// A compare of two doublewords into a field, signed or not.
    fn compare(&mut self, which: u32, signed: bool, a: Operand, b: Operand) {
        let (less, greater) = if signed { (ICond::Slt, ICond::Sgt) } else { (ICond::Ult, ICond::Ugt) };
        let (less, greater, equal) =
            (self.cmp(less, a.clone(), b.clone()), self.cmp(greater, a.clone(), b.clone()), self.cmp(ICond::Eq, a, b));
        self.set_field(which, less, greater, equal);
    }

    /// The record form's compare of a result with zero, into the first field.
    fn record(&mut self, value: Operand) {
        self.compare(0, true, value, Operand::Int(0));
    }

    /// A doubleword rotated left by a count the instruction gives or a register holds.
    fn rotate(&mut self, value: Operand, by: Operand) -> Operand {
        let by = self.bin(BinOp::And, by, 63);
        let back = self.bin(BinOp::Sub, 64, by.clone());
        let back = self.bin(BinOp::And, back, 63);
        let (left, right) = (self.bin(BinOp::Shl, value.clone(), by), self.bin(BinOp::LShr, value, back));
        self.bin(BinOp::Or, left, right)
    }

    /// The count of the zero bits above the highest one bit of a doubleword, or of its low half.
    fn leading_zeros(&mut self, value: Operand, bits: u32) -> Operand {
        // The low half is counted at the top of the doubleword.
        let value = if bits == 32 { self.bin(BinOp::Shl, value, 32) } else { value };
        let (mut count, mut rest): (Operand, Operand) = (Operand::Int(0), value.clone());
        for step in [32u32, 16, 8, 4, 2, 1] {
            let top = self.bin(BinOp::LShr, rest.clone(), 64 - step);
            let none = self.cmp(ICond::Eq, top, 0);
            let more = self.bin(BinOp::Add, count.clone(), step);
            let moved = self.bin(BinOp::Shl, rest.clone(), step);
            count = self.select(none, more, count);
            rest = self.select(none, moved, rest);
        }
        let zero = self.cmp(ICond::Eq, value, 0);
        self.select(zero, bits, count)
    }

    /// A shift by a register's count, of a doubleword or of the low half: left, right, or right
    /// with the sign, which sets the carry when a negative value loses one bits.
    fn shift(&mut self, w: u32, bits: u32, kind: BinOp, count: Operand) -> Operand {
        let (source, a) = (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)));
        let value = self.get(source);
        let value = match (bits, kind) {
            (32, BinOp::AShr) => self.sign(value, 32),
            (32, _) => self.bin(BinOp::And, value, 0xffff_ffffu32),
            _ => value,
        };
        // A count of the width or more leaves nothing, or all sign.
        let whole = self.cmp(ICond::Uge, count.clone(), bits);
        let by = self.bin(BinOp::And, count, 63);
        let shifted = self.bin(kind, value.clone(), by.clone());
        let all = if kind == BinOp::AShr { self.bin(BinOp::AShr, value.clone(), 63) } else { Operand::Int(0) };
        let mut result = self.select(whole, all, shifted.clone());
        if kind == BinOp::Shl && bits == 32 {
            result = self.bin(BinOp::And, result, 0xffff_ffffu32);
        }
        if kind == BinOp::AShr {
            let back = self.bin(BinOp::Shl, shifted, by);
            let lost = self.cmp(ICond::Ne, back, value.clone());
            let any = self.cmp(ICond::Ne, value.clone(), 0);
            let lost = self.b.select(self.block, whole, Type::I1, any, lost, "lost");
            let negative = self.cmp(ICond::Slt, value, 0);
            let carried = self.both(negative, lost);
            self.set_carry(carried);
        }
        self.set(a, result.clone());
        if w & 1 != 0 {
            self.record(result.clone());
        }
        result
    }

    /// A load into register `d` or a store of it, at `address`: `signed` extends a load's sign.
    fn access(&mut self, size: u32, is_store: bool, signed: bool, d: u64, address: Operand) {
        if is_store {
            let value = self.get(d);
            self.store(size, value, address);
            return;
        }
        let mut value = self.load(size, address);
        if signed {
            value = self.sign(value, 8 * size as u16);
        }
        self.set(d, value);
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

    /// A load into the floating-point register `d` or a store of it, a single or a double. A
    /// register's slot holds the double's bits, which a general register's access reads as they are.
    fn float_access(&mut self, single: bool, is_store: bool, d: u64, address: Operand) {
        match (is_store, single) {
            (false, true) => {
                let bits = self.load(4, address);
                let bits = self.low(bits);
                let value = self.b.cast(self.block, Cast::Bitcast, I32, bits, Type::Float, "single");
                let value = self.b.cast(self.block, Cast::FPExt, Type::Float, value, Type::Double, "f");
                self.fset(d, value);
            }
            (true, true) => {
                let value = self.fget(d);
                let value = self.b.cast(self.block, Cast::FPTrunc, Type::Double, value, Type::Float, "single");
                let bits: Operand = self.b.cast(self.block, Cast::Bitcast, Type::Float, value, I32, "bits").into();
                let bits = self.widen(bits, false);
                self.store(4, bits, address);
            }
            (is_store, false) => self.access(8, is_store, false, F0 + d, address),
        }
    }

    /// A double to a whole number toward zero, of `bits` bits: what is past an end of the range
    /// is that end, and what is no number is the lowest.
    fn to_whole(&mut self, value: Operand, bits: u32) -> Operand {
        let (top, bottom, most, least) = if bits == 32 {
            (2_147_483_647.0, -2_147_483_648.0, 0x7fff_ffffu64, 0xffff_ffff_8000_0000u64)
        } else {
            // The highest double below two to the sixty-third converts; that power and above do not.
            (9_223_372_036_854_774_784.0, -9_223_372_036_854_775_808.0, i64::MAX.cast_unsigned(), 1u64 << 63)
        };
        let high = self.fcmp(FCond::Ogt, value.clone(), Operand::Float(top));
        let low = self.fcmp(FCond::Olt, value.clone(), Operand::Float(bottom));
        let neither = self.fcmp(FCond::Uno, value.clone(), value.clone());
        let low = self.any(low, neither);
        let outside = self.any(low, high);
        let inside = self.b.select(self.block, outside, Type::Double, Operand::Float(0.0), value, "inside");
        let whole = self.b.cast(self.block, Cast::FPToSI, Type::Double, inside, I64, "whole");
        let whole = self.select(low, int(least), whole);
        self.select(high, int(most), whole)
    }

    /// The instructions of the floating-point unit, of primary opcode 59 (singles) and 63.
    fn float(&mut self, w: u32) -> bool {
        let (d, a, b, c) =
            (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)), u64::from(field(w, 16, 20)), u64::from(field(w, 21, 25)));
        let single = w >> 26 == 59;
        // The forms with one operand and the compares, by the whole extended opcode.
        match ((w >> 1) & 0x3ff, single) {
            (0 | 32, false) => {
                // An ordered compare differs from an unordered one in the exceptions it raises.
                let (x, y) = (self.fget(a), self.fget(b));
                let (less, greater) = (self.fcmp(FCond::Olt, x.clone(), y.clone()), self.fcmp(FCond::Ogt, x.clone(), y.clone()));
                let (equal, neither) = (self.fcmp(FCond::Oeq, x.clone(), y.clone()), self.fcmp(FCond::Uno, x, y));
                let mut bits = Operand::Int(0);
                for (is, shift) in [(less, 3), (greater, 2), (equal, 1), (neither, 0)] {
                    let bit: Operand = self.b.cast(self.block, Cast::ZExt, Type::I1, is, I32, "bit").into();
                    let bit = self.bin32(BinOp::Shl, bit, shift);
                    bits = self.bin32(BinOp::Or, bits, bit);
                }
                let shift = 28 - 4 * field(w, 6, 8);
                let placed = self.bin32(BinOp::Shl, bits, shift);
                let cr = self.get32(CR);
                let cleared = self.bin32(BinOp::And, cr, !(0xfu32 << shift));
                let cr = self.bin32(BinOp::Or, cleared, placed);
                self.set32(CR, cr);
                return true;
            }
            (12, false) => {
                let value = self.fget(b);
                let value = self.single(value);
                self.fset(d, value);
                return true;
            }
            (15, false) => {
                // The whole number in the low word; the high word is not defined, and is this.
                let value = self.fget(b);
                let whole = self.to_whole(value, 32);
                let whole = self.bin(BinOp::And, whole, 0xffff_ffffu32);
                let bits = self.bin(BinOp::Or, whole, int(0xfff8_0000 << 32));
                self.set(F0 + d, bits);
                return true;
            }
            (815, false) => {
                let value = self.fget(b);
                let whole = self.to_whole(value, 64);
                self.set(F0 + d, whole);
                return true;
            }
            (846, _) => {
                // A doubleword's whole number as a double, or as the single nearest it.
                let whole = self.get(F0 + b);
                let value: Operand = self.b.cast(self.block, Cast::SIToFP, I64, whole, Type::Double, "f").into();
                let value = if single { self.single(value) } else { value };
                self.fset(d, value);
                return true;
            }
            (xo @ (40 | 72 | 136 | 264), false) => {
                // The moves, which are of the bits: as they are, the sign turned, set or cleared.
                let bits = self.get(F0 + b);
                let sign = int(1 << 63);
                let bits = match xo {
                    40 => self.bin(BinOp::Xor, bits, sign),
                    136 => self.bin(BinOp::Or, bits, sign),
                    264 => self.bin(BinOp::And, bits, int(i64::MAX.cast_unsigned())),
                    _ => bits,
                };
                self.set(F0 + d, bits);
                return true;
            }
            _ => {}
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
                if xo >= 30 {
                    let bits = self.b.cast(self.block, Cast::Bitcast, Type::Double, sum, I64, "bits");
                    let bits = self.bin(BinOp::Xor, bits, int(1 << 63));
                    self.b.cast(self.block, Cast::Bitcast, I64, bits, Type::Double, "f").into()
                } else {
                    sum
                }
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

    /// The high half of the product of two doublewords, by their halves: of the values as they
    /// are, and of the signed ones by what a negative operand takes from it.
    fn high_product(&mut self, x: Operand, y: Operand, signed: bool) -> Operand {
        let half = 0xffff_ffffu32;
        let (xl, xh) = (self.bin(BinOp::And, x.clone(), half), self.bin(BinOp::LShr, x.clone(), 32));
        let (yl, yh) = (self.bin(BinOp::And, y.clone(), half), self.bin(BinOp::LShr, y.clone(), 32));
        let (ll, lh) = (self.bin(BinOp::Mul, xl.clone(), yl.clone()), self.bin(BinOp::Mul, xl, yh.clone()));
        let (hl, hh) = (self.bin(BinOp::Mul, xh.clone(), yl), self.bin(BinOp::Mul, xh, yh));
        let carried = self.bin(BinOp::LShr, ll, 32);
        let (lh_low, hl_low) = (self.bin(BinOp::And, lh.clone(), half), self.bin(BinOp::And, hl.clone(), half));
        let middle = self.bin(BinOp::Add, carried, lh_low);
        let middle = self.bin(BinOp::Add, middle, hl_low);
        let (lh_high, hl_high, middle_high) =
            (self.bin(BinOp::LShr, lh, 32), self.bin(BinOp::LShr, hl, 32), self.bin(BinOp::LShr, middle, 32));
        let high = self.bin(BinOp::Add, hh, lh_high);
        let high = self.bin(BinOp::Add, high, hl_high);
        let mut high = self.bin(BinOp::Add, high, middle_high);
        if signed {
            for (negative, other) in [(x.clone(), y.clone()), (y, x)] {
                let is = self.cmp(ICond::Slt, negative, 0);
                let taken = self.select(is, other, 0);
                high = self.bin(BinOp::Sub, high, taken);
            }
        }
        high
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
                    self.add(inverse, Operand::Int(i128::from(simm)), Operand::Int(1))
                } else {
                    self.add(value, Operand::Int(i128::from(simm)), Operand::Int(0))
                };
                self.set(d, sum.clone());
                self.set_carry(carried);
                if w >> 26 == 13 {
                    self.record(sum);
                }
            }
            10 | 11 => {
                // A compare with an immediate, of the doubleword or of the low half.
                let signed = w >> 26 == 11;
                let mut value = self.get(a);
                if field(w, 10, 10) == 0 {
                    value = if signed { self.sign(value, 32) } else { self.bin(BinOp::And, value, 0xffff_ffffu32) };
                }
                self.compare(field(w, 6, 8), signed, value, Operand::Int(i128::from(if signed { simm } else { uimm })));
            }
            14 | 15 => {
                let base = self.base(a);
                let sum = self.bin(BinOp::Add, base, if w >> 26 == 14 { simm } else { simm << 16 });
                self.set(d, sum);
            }
            20 | 21 | 23 => {
                // The low half rotated left, the rotation in both halves, under a mask of the
                // low half; a mask that goes round takes the high half too.
                let source = self.get(d);
                let word = self.low(source);
                let rotated = if w >> 26 == 23 {
                    let count = self.get(b);
                    let count = self.low(count);
                    let count = self.bin32(BinOp::And, count, 31);
                    let back = self.bin32(BinOp::Sub, 32, count.clone());
                    let back = self.bin32(BinOp::And, back, 31);
                    let (left, right) = (self.bin32(BinOp::Shl, word.clone(), count), self.bin32(BinOp::LShr, word, back));
                    self.bin32(BinOp::Or, left, right)
                } else {
                    match field(w, 16, 20) {
                        0 => word,
                        by => {
                            let (left, right) = (self.bin32(BinOp::Shl, word.clone(), by), self.bin32(BinOp::LShr, word, 32 - by));
                            self.bin32(BinOp::Or, left, right)
                        }
                    }
                };
                let rotated = self.widen(rotated, false);
                let high = self.bin(BinOp::Shl, rotated.clone(), 32);
                let rotated = self.bin(BinOp::Or, rotated, high);
                let keep = mask(field(w, 21, 25) + 32, field(w, 26, 30) + 32);
                let mut value = self.bin(BinOp::And, rotated, int(keep));
                if w >> 26 == 20 {
                    let target = self.get(a);
                    let rest = self.bin(BinOp::And, target, int(!keep));
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
            30 => {
                // The doubleword rotates under a mask: the count and the mask's end are six bits,
                // their high bit apart from the rest.
                let source = self.get(d);
                let count = field(w, 16, 20) | ((w >> 1) & 1) << 5;
                let end = field(w, 21, 25) | field(w, 26, 26) << 5;
                let by_register = matches!((w >> 1) & 0xf, 8 | 9);
                let rotated = if by_register {
                    let by = self.get(b);
                    self.rotate(source, by)
                } else {
                    self.rotate(source, Operand::Int(i128::from(count)))
                };
                let (keep, insert) = match (by_register, (w >> 1) & 0xf, (w >> 2) & 7) {
                    (true, 8, _) | (false, _, 0) => (mask(end, 63), false),
                    (true, _, _) | (false, _, 1) => (mask(0, end), false),
                    (false, _, 2) => (mask(end, 63 - count), false),
                    (false, _, 3) => (mask(end, 63 - count), true),
                    _ => return false,
                };
                let mut value = self.bin(BinOp::And, rotated, int(keep));
                if insert {
                    let target = self.get(a);
                    let rest = self.bin(BinOp::And, target, int(!keep));
                    value = self.bin(BinOp::Or, value, rest);
                }
                self.set(a, value.clone());
                if rc {
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
            58 | 62 => {
                // The doubleword loads and stores: the displacement's low two bits say which.
                let kind = w & 3;
                let update = kind == 1;
                let base = if update { self.get(a) } else { self.base(a) };
                let address = self.bin(BinOp::Add, base, simm & !3);
                match (w >> 26, kind) {
                    (58, 0 | 1) => self.access(8, false, false, d, address.clone()),
                    (58, 2) => self.access(4, false, true, d, address.clone()),
                    (62, 0 | 1) => self.access(8, true, false, d, address.clone()),
                    _ => return false,
                }
                if update {
                    self.set(a, address);
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

    /// The instructions of primary opcode 31, by their extended opcode.
    #[allow(clippy::too_many_lines)]
    fn extended(&mut self, w: u32) -> bool {
        let (d, a, b) = (u64::from(field(w, 6, 10)), u64::from(field(w, 11, 15)), u64::from(field(w, 16, 20)));
        let rc = w & 1 != 0;
        let xo = (w >> 1) & 0x3ff;
        match xo {
            9 | 73 => {
                let (x, y) = (self.get(a), self.get(b));
                let value = self.high_product(x, y, xo == 73);
                self.set(d, value.clone());
                if rc {
                    self.record(value);
                }
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
                    self.access(4, true, false, F0 + d, address.clone());
                } else {
                    self.float_access(single, is_store, d, address.clone());
                }
                if update {
                    self.set(a, address);
                }
                return true;
            }
            0 | 32 => {
                let signed = xo == 0;
                let (mut x, mut y) = (self.get(a), self.get(b));
                if field(w, 10, 10) == 0 {
                    if signed {
                        (x, y) = (self.sign(x, 32), self.sign(y, 32));
                    } else {
                        (x, y) = (self.bin(BinOp::And, x, 0xffff_ffffu32), self.bin(BinOp::And, y, 0xffff_ffffu32));
                    }
                }
                self.compare(field(w, 6, 8), signed, x, y);
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
            // The shifts by a register: of the low half by six bits of count, of the doubleword by seven.
            24 | 536 | 792 | 27 | 539 | 794 => {
                let (bits, limit) = if matches!(xo, 24 | 536 | 792) { (32, 63) } else { (64, 127) };
                let kind = match xo {
                    24 | 27 => BinOp::Shl,
                    536 | 539 => BinOp::LShr,
                    _ => BinOp::AShr,
                };
                let count = self.get(b);
                let count = self.bin(BinOp::And, count, limit);
                self.shift(w, bits, kind, count);
                return true;
            }
            // The shifts right with the sign by the instruction's own count.
            824 => {
                self.shift(w, 32, BinOp::AShr, Operand::Int(i128::from(field(w, 16, 20))));
                return true;
            }
            826 | 827 => {
                self.shift(w, 64, BinOp::AShr, Operand::Int(i128::from(field(w, 16, 20) | (xo & 1) << 5)));
                return true;
            }
            26 | 58 | 922 | 954 | 986 => {
                let value = self.get(d);
                let result = match xo {
                    26 => self.leading_zeros(value, 32),
                    58 => self.leading_zeros(value, 64),
                    922 => self.sign(value, 16),
                    954 => self.sign(value, 8),
                    _ => self.sign(value, 32),
                };
                self.set(a, result.clone());
                if rc {
                    self.record(result);
                }
                return true;
            }
            21 | 23 | 53 | 55 | 87 | 119 | 279 | 311 | 341 | 343 | 373 | 375 | 149 | 151 | 181 | 183 | 215 | 247 | 407 | 439 => {
                // The indexed loads and stores; the updating ones leave the address in the base.
                let (size, is_store, signed, update) = match xo {
                    21 => (8, false, false, false),
                    53 => (8, false, false, true),
                    23 => (4, false, false, false),
                    55 => (4, false, false, true),
                    341 => (4, false, true, false),
                    373 => (4, false, true, true),
                    87 => (1, false, false, false),
                    119 => (1, false, false, true),
                    279 => (2, false, false, false),
                    311 => (2, false, false, true),
                    343 => (2, false, true, false),
                    375 => (2, false, true, true),
                    149 => (8, true, false, false),
                    181 => (8, true, false, true),
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
                match (number, xo) {
                    (1, 339) => {
                        let xer = self.get32(XER);
                        let xer = self.widen(xer, false);
                        self.set(d, xer);
                    }
                    (1, _) => {
                        let value = self.get(d);
                        let value = self.low(value);
                        self.set32(XER, value);
                    }
                    (8 | 9, 339) => {
                        let value = self.get(if number == 8 { LR } else { CTR });
                        self.set(d, value);
                    }
                    (8 | 9, _) => {
                        let value = self.get(d);
                        self.set(if number == 8 { LR } else { CTR }, value);
                    }
                    _ => return false,
                }
                return true;
            }
            19 => {
                let cr = self.get32(CR);
                let cr = self.widen(cr, false);
                self.set(d, cr);
                return true;
            }
            144 => {
                // The fields of the condition register the mask names, from the register.
                let fields = field(w, 12, 19);
                let keep = (0..8).filter(|n| fields & (0x80 >> n) != 0).fold(0u32, |keep, n| keep | 0xf000_0000 >> (4 * n));
                let value = self.get(d);
                let (value, cr) = (self.low(value), self.get32(CR));
                let (taken, kept) = (self.bin32(BinOp::And, value, keep), self.bin32(BinOp::And, cr, !keep));
                let cr = self.bin32(BinOp::Or, taken, kept);
                self.set32(CR, cr);
                return true;
            }
            _ => {}
        }
        let (x, y) = (self.get(a), self.get(b));
        let inverse = self.bin(BinOp::Xor, x.clone(), -1);
        let ones = Operand::Int(-1);
        // The sum each arithmetic form is: its two terms and the carry it starts with, and
        // whether it leaves a carry. The overflow enable is the extended opcode's top bit.
        let arithmetic = xo & 0x1ff;
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
                233 => self.bin(BinOp::Mul, x, y),
                // The products and the quotients of the low halves.
                235 | 75 | 11 => {
                    let signed = arithmetic != 11;
                    let (x, y) = (self.low(x), self.low(y));
                    let (x, y) = (self.widen(x, signed), self.widen(y, signed));
                    let product = self.bin(BinOp::Mul, x, y);
                    match arithmetic {
                        235 => product,
                        75 => self.bin(BinOp::AShr, product, 32),
                        _ => self.bin(BinOp::LShr, product, 32),
                    }
                }
                459 | 491 | 457 | 489 => {
                    // A division by zero, and the one that overflows, leave what the processor
                    // pleases: here the dividend, divided by one.
                    let signed = matches!(arithmetic, 491 | 489);
                    let word = matches!(arithmetic, 459 | 491);
                    let (x, y) = if word {
                        let (x, y) = (self.low(x), self.low(y));
                        (self.widen(x, signed), self.widen(y, signed))
                    } else {
                        (x, y)
                    };
                    let zero = self.cmp(ICond::Eq, y.clone(), 0);
                    let mut bad = zero;
                    if signed {
                        let least = if word { i64::from(i32::MIN) } else { i64::MIN };
                        let (least, minus) = (self.cmp(ICond::Eq, x.clone(), least), self.cmp(ICond::Eq, y.clone(), -1));
                        let overflows = self.both(least, minus);
                        bad = self.any(zero, overflows);
                    }
                    let by = self.select(bad, 1, y);
                    let quotient = self.bin(if signed { BinOp::SDiv } else { BinOp::UDiv }, x, by);
                    if word { self.bin(BinOp::And, quotient, 0xffff_ffffu32) } else { quotient }
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

/// Lifts the 64-bit `code` into a module with the one function [`RUN`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lift(code: Code<'_>) -> Module {
    let mut module = Module::x86_64_windows("ppc64");
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
    declare(&mut module, TRAP, vec![I64]);
    let mut b = FunctionBuilder::new(RUN, FnType::new(Type::Void, vec![Type::Ptr, I64]), &["state", "start"]);
    let (entry, dispatch, nowhere) = (b.block("entry"), b.block("dispatch"), b.block("nowhere"));
    let state = b.param(0);
    // Where a branch to `lr` or `ctr` goes, for the dispatcher to find the block of.
    let pc = b.alloca(entry, I64, "pc");
    let start = b.param(1);
    b.store(entry, I64, start, pc);
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
        lift.b.call(lift.block, FnType::new(Type::Void, vec![I64]), Callee::Direct(TRAP.to_owned()), vec![(I64, address)], "");
        lift.b.terminate(lift.block, Terminator::Ret(None));
    };
    for &(address, w) in &words {
        lift.block = blocks[&address];
        lift.b.address = Some(address);
        let at = |target: u64| blocks.get(&target).copied().unwrap_or(nowhere);
        let next = address + 4;
        let link = w & 1 != 0;
        let absolute = w & 2 != 0;
        let relative = |offset: i64| if absolute { offset.cast_unsigned() } else { address.wrapping_add_signed(offset) };
        let through = (w >> 26 == 19).then_some((w >> 1) & 0x3ff).filter(|xo| matches!(xo, 16 | 528));
        match (w >> 26, through) {
            (18, _) => {
                let target = relative(i64::from(((w & 0x03ff_fffc) << 6).cast_signed() >> 6));
                if link {
                    lift.set(LR, int(next));
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
                    let cr = lift.get32(CR);
                    let one = lift.bin32(BinOp::And, cr, 0x8000_0000u32 >> bit);
                    let holds = lift.b.icmp(lift.block, if options & 0b01000 == 0 { ICond::Eq } else { ICond::Ne }, I32, one, 0, "holds");
                    taken = Some(match taken {
                        Some(counted) => lift.both(counted, holds),
                        None => holds,
                    });
                }
                let goes = match through {
                    None => at(relative(i64::from(w as u16 as i16 & !3))),
                    Some(xo) => {
                        // Where `lr` or `ctr` says, as it was before the branch linked.
                        let target = lift.get(if xo == 16 { LR } else { CTR });
                        let target = lift.bin(BinOp::And, target, -4);
                        lift.b.store(lift.block, I64, target, pc);
                        dispatch
                    }
                };
                if link {
                    lift.set(LR, int(next));
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
                    trap(&mut lift, int(address));
                }
            }
        }
    }
    let target = lift.b.load(dispatch, I64, pc, "target");
    let cases = blocks.iter().map(|(&address, &block)| (i128::from(address), block)).collect();
    lift.b.terminate(dispatch, Terminator::Switch { ty: I64, value: target.into(), default: nowhere, cases });
    lift.block = nowhere;
    let target = lift.b.load(nowhere, I64, pc, "target");
    trap(&mut lift, target.into());
    module.functions.push(lift.b.finish());
    module
}
