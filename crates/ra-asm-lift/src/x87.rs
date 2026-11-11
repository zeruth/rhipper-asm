//! The x87 floating-point unit: the register stack, its control and status words, and the
//! instructions 32-bit code uses for floats, conversions and the occasional `long double` idiom.
//!
//! The stack is eight doubles in a global, with a global top: `st(i)` is the entry `top + i`. The
//! 80-bit precision of the real unit is not kept; a double is what the C the port becomes would
//! use, and what a program that stores its results to memory as doubles observes. The state is in
//! globals rather than in `%State` because it crosses calls: a 32-bit function returns a double in
//! `st(0)`, and the raising passes, which drop `%State`, leave globals alone.
//!
//! The control word matters for the rounding of `fist` and `frndint`: the usual idiom loads a
//! truncating control word, converts, and restores the old one. The status word carries the
//! condition codes of `fcom`, which `fnstsw ax; sahf` or a `test ah` reads.

use ra_asm_ir::{BinOp, BlockId, Cast, FCond, ICond, Inst, Operand, Type, ValueId};
use ra_asm_x86::iced_x86::{Instruction, Mnemonic as M, OpKind, Register as R};

use crate::state::{Flag, X87_CW, X87_STACK, X87_SW, X87_TOP};
use crate::x86::Lifter;

/// Whether the lift handles `m` here.
pub fn is_x87(m: M) -> bool {
    matches!(
        m,
        M::Fld
            | M::Fld1
            | M::Fldz
            | M::Fldpi
            | M::Fldl2e
            | M::Fldln2
            | M::Fldlg2
            | M::Fldl2t
            | M::Fild
            | M::Fst
            | M::Fstp
            | M::Fist
            | M::Fistp
            | M::Fisttp
            | M::Fadd
            | M::Faddp
            | M::Fiadd
            | M::Fsub
            | M::Fsubp
            | M::Fisub
            | M::Fsubr
            | M::Fsubrp
            | M::Fisubr
            | M::Fmul
            | M::Fmulp
            | M::Fimul
            | M::Fdiv
            | M::Fdivp
            | M::Fidiv
            | M::Fdivr
            | M::Fdivrp
            | M::Fidivr
            | M::Fchs
            | M::Fabs
            | M::Fsqrt
            | M::Frndint
            | M::Fxch
            | M::Fcom
            | M::Fcomp
            | M::Fcompp
            | M::Fucom
            | M::Fucomp
            | M::Fucompp
            | M::Ficom
            | M::Ficomp
            | M::Fcomi
            | M::Fcomip
            | M::Fucomi
            | M::Fucomip
            | M::Ftst
            | M::Fxam
            | M::Fnstsw
            | M::Fstsw
            | M::Fnstcw
            | M::Fstcw
            | M::Fldcw
            | M::Fninit
            | M::Finit
            | M::Fnclex
            | M::Fclex
            | M::Ffree
            | M::Ffreep
            | M::Fincstp
            | M::Fdecstp
            | M::Fcmovb
            | M::Fcmove
            | M::Fcmovbe
            | M::Fcmovu
            | M::Fcmovnb
            | M::Fcmovne
            | M::Fcmovnbe
            | M::Fcmovnu
            | M::Fnop
    )
}

/// The index of an `st(i)` register.
fn st_index(r: R) -> i64 {
    i64::from(r as u32 - R::ST0 as u32)
}

impl Lifter<'_, '_> {
    /// A pointer to a global of the x87 state.
    fn x87_global(&mut self, block: BlockId, name: &str) -> ValueId {
        let address = Operand::Address { global: name.to_owned(), offset: 0 };
        self.m.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, &format!("{name}.p"))
    }

    fn x87_top(&mut self, block: BlockId) -> ValueId {
        let p = self.x87_global(block, X87_TOP);
        self.m.b.load(block, Type::I64, p, "top")
    }

    fn set_x87_top(&mut self, block: BlockId, top: ValueId) {
        let p = self.x87_global(block, X87_TOP);
        self.m.b.store(block, Type::I64, top, p);
    }

    /// A pointer to `st(i)`: entry `(top + i) & 7` of the stack.
    fn st_ptr(&mut self, block: BlockId, i: i64) -> ValueId {
        let top = self.x87_top(block);
        let index = if i == 0 { top } else { self.m.b.bin(block, BinOp::Add, Type::I64, top, i, "st.i") };
        let wrapped = self.m.b.bin(block, BinOp::And, Type::I64, index, 7i64, "st.w");
        let offset = self.m.b.bin(block, BinOp::Shl, Type::I64, wrapped, 3i64, "st.off");
        let base = Operand::Address { global: X87_STACK.to_owned(), offset: 0 };
        let address = self.m.b.bin(block, BinOp::Add, Type::I64, offset, base, "st.addr");
        self.m.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, "st.p")
    }

    fn st(&mut self, block: BlockId, i: i64) -> ValueId {
        let p = self.st_ptr(block, i);
        self.m.b.load(block, Type::Double, p, &format!("st{i}"))
    }

    fn set_st(&mut self, block: BlockId, i: i64, value: impl Into<Operand>) {
        let p = self.st_ptr(block, i);
        self.m.b.store(block, Type::Double, value, p);
    }

    /// Pushes `value`: the top moves down and the new `st(0)` holds it.
    fn fpush(&mut self, block: BlockId, value: impl Into<Operand>) {
        let top = self.x87_top(block);
        let down = self.m.b.bin(block, BinOp::Sub, Type::I64, top, 1i64, "top.d");
        let wrapped = self.m.b.bin(block, BinOp::And, Type::I64, down, 7i64, "top");
        self.set_x87_top(block, wrapped);
        self.set_st(block, 0, value);
    }

    /// Pops: the top moves up. The value that was `st(0)` is the caller's to read first.
    fn fpop(&mut self, block: BlockId) {
        let top = self.x87_top(block);
        let up = self.m.b.bin(block, BinOp::Add, Type::I64, top, 1i64, "top.u");
        let wrapped = self.m.b.bin(block, BinOp::And, Type::I64, up, 7i64, "top");
        self.set_x87_top(block, wrapped);
    }

    fn control_word(&mut self, block: BlockId) -> ValueId {
        let p = self.x87_global(block, X87_CW);
        self.m.b.load(block, Type::I16, p, "cw")
    }

    /// The memory operand of an x87 instruction as a double: a float, a double, or an integer of
    /// 16, 32 or 64 bits (`integer`), by the operand's size.
    fn fp_memory(&mut self, block: BlockId, i: &Instruction, n: u32, integer: bool) -> Option<ValueId> {
        let bits = self.op_bits(i, n);
        let address = self.mem_address(block, i);
        Some(match (bits, integer) {
            (32, false) => {
                let v = self.m.load(block, Type::Float, address, "m32");
                self.m.b.cast(block, Cast::FPExt, Type::Float, v, Type::Double, "fx")
            }
            (64, false) => self.m.load(block, Type::Double, address, "m64"),
            (16 | 32 | 64, true) => {
                let v = self.m.load(block, Type::Int(bits), address, "mi");
                self.m.b.cast(block, Cast::SIToFP, Type::Int(bits), v, Type::Double, "fi")
            }
            _ => return None,
        })
    }

    /// Stores a double to the memory operand as a float or a double, by the operand's size.
    fn store_fp_memory(&mut self, block: BlockId, i: &Instruction, n: u32, value: ValueId) -> bool {
        let bits = self.op_bits(i, n);
        let address = self.mem_address(block, i);
        match bits {
            32 => {
                let v = self.m.b.cast(block, Cast::FPTrunc, Type::Double, value, Type::Float, "fn");
                self.m.store(block, Type::Float, v, address);
            }
            64 => self.m.store(block, Type::Double, value, address),
            _ => return false,
        }
        true
    }

    /// `value` rounded by the control word's rounding mode: nearest even, down, up or toward zero.
    fn round_by_control_word(&mut self, block: BlockId, value: ValueId) -> ValueId {
        let cw = self.control_word(block);
        let shifted = self.m.b.bin(block, BinOp::LShr, Type::I16, cw, 10i64, "rc.s");
        let rc = self.m.b.bin(block, BinOp::And, Type::I16, shifted, 3i64, "rc");
        let d = Type::Double;
        let nearest = self.m.intrinsic(block, "llvm.rint.f64", d.clone(), vec![(d.clone(), value.into())], "rn");
        let down = self.m.intrinsic(block, "llvm.floor.f64", d.clone(), vec![(d.clone(), value.into())], "rd");
        let up = self.m.intrinsic(block, "llvm.ceil.f64", d.clone(), vec![(d.clone(), value.into())], "ru");
        let zero = self.m.intrinsic(block, "llvm.trunc.f64", d.clone(), vec![(d.clone(), value.into())], "rz");
        let is_zero = self.m.b.icmp(block, ICond::Eq, Type::I16, rc, 3i64, "rc.z");
        let is_up = self.m.b.icmp(block, ICond::Eq, Type::I16, rc, 2i64, "rc.u");
        let is_down = self.m.b.icmp(block, ICond::Eq, Type::I16, rc, 1i64, "rc.d");
        let a = self.m.b.select(block, is_down, d.clone(), down, nearest, "r.a");
        let b = self.m.b.select(block, is_up, d.clone(), up, a, "r.b");
        self.m.b.select(block, is_zero, d, zero, b, "rounded")
    }

    /// Sets C0, C2 and C3 of the status word from a compare: `a < b`, unordered, `a == b`.
    fn set_condition_codes(&mut self, block: BlockId, a: ValueId, b: ValueId) {
        let d = Type::Double;
        let less = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "c0");
        let unordered = self.m.b.value(block, Inst::FCmp { cond: FCond::Uno, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "c2");
        let equal = self.m.b.value(block, Inst::FCmp { cond: FCond::Oeq, ty: d, lhs: a.into(), rhs: b.into() }, "c3");
        // An unordered compare sets all three.
        let c0 = self.m.b.bin(block, BinOp::Or, Type::I1, less, unordered, "c0.u");
        let c3 = self.m.b.bin(block, BinOp::Or, Type::I1, equal, unordered, "c3.u");
        let p = self.x87_global(block, X87_SW);
        let sw = self.m.b.load(block, Type::I16, p, "sw");
        let cleared = self.m.b.bin(block, BinOp::And, Type::I16, sw, !0x4500i64 & 0xffff, "sw.c");
        let mut acc = cleared;
        for (bit, flag) in [(8i64, c0), (10, unordered), (14, c3)] {
            let wide = self.m.zext(block, Type::I1, flag, Type::I16, "cc.w");
            let placed = self.m.b.bin(block, BinOp::Shl, Type::I16, wide, bit, "cc.p");
            acc = self.m.b.bin(block, BinOp::Or, Type::I16, acc, placed, "sw");
        }
        self.m.b.store(block, Type::I16, acc, p);
    }

    /// Sets ZF, PF and CF from a compare, as `fcomi` does.
    fn set_compare_flags(&mut self, block: BlockId, a: ValueId, b: ValueId) {
        let d = Type::Double;
        let less = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "lt");
        let unordered = self.m.b.value(block, Inst::FCmp { cond: FCond::Uno, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "uno");
        let equal = self.m.b.value(block, Inst::FCmp { cond: FCond::Oeq, ty: d, lhs: a.into(), rhs: b.into() }, "eq");
        let zf = self.m.b.bin(block, BinOp::Or, Type::I1, equal, unordered, "zf");
        let cf = self.m.b.bin(block, BinOp::Or, Type::I1, less, unordered, "cf");
        self.m.set_flag(block, Flag::Zf, zf);
        self.m.set_flag(block, Flag::Pf, unordered);
        self.m.set_flag(block, Flag::Cf, cf);
    }

    /// Operand `n` as a double: memory by its size, or an `st(i)` register. iced lists the implicit
    /// `st(0)` as an operand, so `fld m64` is `fld st, m64` and `fistp m64` is `fistp m64, st`: the
    /// destination is operand 0 and the source operand 1 throughout.
    fn fp_operand(&mut self, block: BlockId, i: &Instruction, n: u32, integer: bool) -> Option<ValueId> {
        if i.op_kind(n) == OpKind::Memory { self.fp_memory(block, i, n, integer) } else { Some(self.st(block, st_index(i.op_register(n)))) }
    }

    /// The source of a one-source instruction: operand 1 when there are two, else operand 0, else
    /// `st(1)`.
    fn source_operand(&mut self, block: BlockId, i: &Instruction, integer: bool) -> Option<ValueId> {
        match i.op_count() {
            2 => self.fp_operand(block, i, 1, integer),
            1 => self.fp_operand(block, i, 0, integer),
            _ => Some(self.st(block, 1)),
        }
    }

    /// The `st(i)` a one-register-source instruction names: operand 1, else operand 0, else 1.
    fn source_index(i: &Instruction) -> i64 {
        match i.op_count() {
            2 => st_index(i.op_register(1)),
            1 => st_index(i.op_register(0)),
            _ => 1,
        }
    }

    /// The two operands of an arithmetic instruction and where the result goes: the index of the
    /// destination register, the left value and the right value, with the `r` forms swapped.
    fn arith_operands(&mut self, block: BlockId, i: &Instruction, reversed: bool, integer: bool) -> Option<(i64, ValueId, ValueId)> {
        let (dst, a, b) = match i.op_count() {
            // `faddp` alone is `faddp st(1), st`.
            0 => {
                let a = self.st(block, 1);
                let b = self.st(block, 0);
                (1, a, b)
            }
            1 => {
                let st0 = self.st(block, 0);
                let other = self.fp_operand(block, i, 0, integer)?;
                (0, st0, other)
            }
            _ => {
                let dst = if i.op_kind(0) == OpKind::Memory { 0 } else { st_index(i.op_register(0)) };
                let a = self.st(block, dst);
                let b = self.fp_operand(block, i, 1, integer)?;
                (dst, a, b)
            }
        };
        Some(if reversed { (dst, b, a) } else { (dst, a, b) })
    }

    /// The index of the memory operand, when the instruction has one.
    fn memory_operand(i: &Instruction) -> Option<u32> {
        (0..i.op_count()).find(|&n| i.op_kind(n) == OpKind::Memory)
    }

    /// Lifts one x87 instruction.
    #[allow(clippy::many_single_char_names)]
    pub(crate) fn x87(&mut self, block: BlockId, i: &Instruction) {
        let ip = i.ip();
        let d = Type::Double;
        macro_rules! unsupported {
            () => {{
                self.warn(format!("{ip:#x}: {:?} with this operand is not lifted", i.mnemonic()));
                self.m.unsupported(block, ip, i.code() as u64);
                return;
            }};
        }
        match i.mnemonic() {
            M::Fld => {
                let Some(v) = self.source_operand(block, i, false) else { unsupported!() };
                self.fpush(block, v);
            }
            M::Fld1 | M::Fldz | M::Fldpi | M::Fldl2e | M::Fldln2 | M::Fldlg2 | M::Fldl2t => {
                let c = match i.mnemonic() {
                    M::Fld1 => 1.0,
                    M::Fldz => 0.0,
                    M::Fldpi => std::f64::consts::PI,
                    M::Fldl2e => std::f64::consts::LOG2_E,
                    M::Fldln2 => std::f64::consts::LN_2,
                    M::Fldlg2 => std::f64::consts::LOG10_2,
                    _ => std::f64::consts::LOG2_10,
                };
                self.fpush(block, Operand::Float(c));
            }
            M::Fild => {
                let Some(n) = Self::memory_operand(i) else { unsupported!() };
                let Some(v) = self.fp_memory(block, i, n, true) else { unsupported!() };
                self.fpush(block, v);
            }
            M::Fst | M::Fstp => {
                let v = self.st(block, 0);
                if i.op_kind(0) == OpKind::Memory {
                    if !self.store_fp_memory(block, i, 0, v) {
                        unsupported!()
                    }
                } else {
                    self.set_st(block, st_index(i.op_register(0)), v);
                }
                if i.mnemonic() == M::Fstp {
                    self.fpop(block);
                }
            }
            M::Fist | M::Fistp | M::Fisttp => {
                let Some(n) = Self::memory_operand(i) else { unsupported!() };
                let bits = self.op_bits(i, n);
                if !matches!(bits, 16 | 32 | 64) {
                    unsupported!()
                }
                let v = self.st(block, 0);
                let rounded = if i.mnemonic() == M::Fisttp {
                    self.m.intrinsic(block, "llvm.trunc.f64", d.clone(), vec![(d.clone(), v.into())], "rz")
                } else {
                    self.round_by_control_word(block, v)
                };
                let n = self.m.b.cast(block, Cast::FPToSI, d, rounded, Type::Int(bits), "fi");
                let address = self.mem_address(block, i);
                self.m.store(block, Type::Int(bits), n, address);
                if i.mnemonic() != M::Fist {
                    self.fpop(block);
                }
            }
            M::Fadd
            | M::Faddp
            | M::Fiadd
            | M::Fsub
            | M::Fsubp
            | M::Fisub
            | M::Fsubr
            | M::Fsubrp
            | M::Fisubr
            | M::Fmul
            | M::Fmulp
            | M::Fimul
            | M::Fdiv
            | M::Fdivp
            | M::Fidiv
            | M::Fdivr
            | M::Fdivrp
            | M::Fidivr => {
                let m = i.mnemonic();
                let reversed = matches!(m, M::Fsubr | M::Fsubrp | M::Fisubr | M::Fdivr | M::Fdivrp | M::Fidivr);
                let integer = matches!(m, M::Fiadd | M::Fisub | M::Fisubr | M::Fimul | M::Fidivr | M::Fidiv);
                let Some((dst, a, b)) = self.arith_operands(block, i, reversed, integer) else { unsupported!() };
                let op = match m {
                    M::Fadd | M::Faddp | M::Fiadd => BinOp::FAdd,
                    M::Fsub | M::Fsubp | M::Fisub | M::Fsubr | M::Fsubrp | M::Fisubr => BinOp::FSub,
                    M::Fmul | M::Fmulp | M::Fimul => BinOp::FMul,
                    _ => BinOp::FDiv,
                };
                let r = self.m.b.bin(block, op, d, a, b, "f");
                self.set_st(block, dst, r);
                if matches!(m, M::Faddp | M::Fsubp | M::Fsubrp | M::Fmulp | M::Fdivp | M::Fdivrp) {
                    self.fpop(block);
                }
            }
            M::Fchs => {
                let v = self.st(block, 0);
                let r = self.m.b.bin(block, BinOp::FSub, d, Operand::Float(-0.0), v, "neg");
                self.set_st(block, 0, r);
            }
            M::Fabs => {
                let v = self.st(block, 0);
                let r = self.m.intrinsic(block, "llvm.fabs.f64", d.clone(), vec![(d, v.into())], "abs");
                self.set_st(block, 0, r);
            }
            M::Fsqrt => {
                let v = self.st(block, 0);
                let r = self.m.intrinsic(block, "llvm.sqrt.f64", d.clone(), vec![(d, v.into())], "sqrt");
                self.set_st(block, 0, r);
            }
            M::Frndint => {
                let v = self.st(block, 0);
                let r = self.round_by_control_word(block, v);
                self.set_st(block, 0, r);
            }
            M::Fxch => {
                let other = Self::source_index(i);
                let a = self.st(block, 0);
                let b = self.st(block, other);
                self.set_st(block, 0, b);
                self.set_st(block, other, a);
            }
            M::Fcom | M::Fcomp | M::Fcompp | M::Fucom | M::Fucomp | M::Fucompp | M::Ficom | M::Ficomp | M::Ftst => {
                let m = i.mnemonic();
                let a = self.st(block, 0);
                let b = if m == M::Ftst {
                    self.m.value_of(block, d, Operand::Float(0.0))
                } else {
                    let Some(v) = self.source_operand(block, i, matches!(m, M::Ficom | M::Ficomp)) else { unsupported!() };
                    v
                };
                self.set_condition_codes(block, a, b);
                match m {
                    M::Fcomp | M::Fucomp | M::Ficomp => self.fpop(block),
                    M::Fcompp | M::Fucompp => {
                        self.fpop(block);
                        self.fpop(block);
                    }
                    _ => {}
                }
            }
            M::Fcomi | M::Fcomip | M::Fucomi | M::Fucomip => {
                let a = self.st(block, 0);
                let b = self.st(block, Self::source_index(i));
                self.set_compare_flags(block, a, b);
                if matches!(i.mnemonic(), M::Fcomip | M::Fucomip) {
                    self.fpop(block);
                }
            }
            M::Fxam => {
                // C3, C2, C0 classify st(0): zero is 100, a normal number is 010, NaN is 001.
                let v = self.st(block, 0);
                let zero =
                    self.m.b.value(block, Inst::FCmp { cond: FCond::Oeq, ty: d.clone(), lhs: v.into(), rhs: Operand::Float(0.0) }, "z");
                let nan = self.m.b.value(block, Inst::FCmp { cond: FCond::Uno, ty: d, lhs: v.into(), rhs: v.into() }, "nan");
                let neither = {
                    let either = self.m.b.bin(block, BinOp::Or, Type::I1, zero, nan, "zn");
                    self.m.b.bin(block, BinOp::Xor, Type::I1, either, 1i64, "normal")
                };
                let p = self.x87_global(block, X87_SW);
                let sw = self.m.b.load(block, Type::I16, p, "sw");
                let mut acc = self.m.b.bin(block, BinOp::And, Type::I16, sw, !0x4500i64 & 0xffff, "sw.c");
                for (bit, flag) in [(8i64, nan), (10, neither), (14, zero)] {
                    let wide = self.m.zext(block, Type::I1, flag, Type::I16, "cc.w");
                    let placed = self.m.b.bin(block, BinOp::Shl, Type::I16, wide, bit, "cc.p");
                    acc = self.m.b.bin(block, BinOp::Or, Type::I16, acc, placed, "sw");
                }
                self.m.b.store(block, Type::I16, acc, p);
            }
            M::Fnstsw | M::Fstsw => {
                let p = self.x87_global(block, X87_SW);
                let sw = self.m.b.load(block, Type::I16, p, "sw");
                // The top of stack field, bits 11 to 13, as the hardware would report it.
                let top = self.x87_top(block);
                let top16 = self.m.trunc(block, Type::I64, top, Type::I16, "top16");
                let placed = self.m.b.bin(block, BinOp::Shl, Type::I16, top16, 11i64, "top.p");
                let cleared = self.m.b.bin(block, BinOp::And, Type::I16, sw, !0x3800i64 & 0xffff, "sw.t");
                let word = self.m.b.bin(block, BinOp::Or, Type::I16, cleared, placed, "sw.w");
                if i.op_kind(0) == OpKind::Memory {
                    let address = self.mem_address(block, i);
                    self.m.store(block, Type::I16, word, address);
                } else {
                    self.m.write_gpr(block, R::AX, word);
                }
            }
            M::Fnstcw | M::Fstcw => {
                let cw = self.control_word(block);
                let address = self.mem_address(block, i);
                self.m.store(block, Type::I16, cw, address);
            }
            M::Fldcw => {
                let address = self.mem_address(block, i);
                let cw = self.m.load(block, Type::I16, address, "cw.m");
                let p = self.x87_global(block, X87_CW);
                self.m.b.store(block, Type::I16, cw, p);
            }
            M::Fninit | M::Finit => {
                let top = self.m.value_of(block, Type::I64, 0i64);
                self.set_x87_top(block, top);
                let cw = self.x87_global(block, X87_CW);
                self.m.b.store(block, Type::I16, 0x037fi64, cw);
                let sw = self.x87_global(block, X87_SW);
                self.m.b.store(block, Type::I16, 0i64, sw);
            }
            M::Fnclex | M::Fclex | M::Ffree | M::Fnop => {}
            M::Ffreep | M::Fincstp => self.fpop(block),
            M::Fdecstp => {
                let top = self.x87_top(block);
                let down = self.m.b.bin(block, BinOp::Sub, Type::I64, top, 1i64, "top.d");
                let wrapped = self.m.b.bin(block, BinOp::And, Type::I64, down, 7i64, "top");
                self.set_x87_top(block, wrapped);
            }
            M::Fcmovb | M::Fcmove | M::Fcmovbe | M::Fcmovu | M::Fcmovnb | M::Fcmovne | M::Fcmovnbe | M::Fcmovnu => {
                let cond = self.condition(block, i.condition_code());
                let a = self.st(block, 0);
                let b = self.st(block, Self::source_index(i));
                let r = self.m.b.select(block, cond, d, b, a, "fcmov");
                self.set_st(block, 0, r);
            }
            _ => unsupported!(),
        }
    }
}
