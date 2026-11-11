//! The x87 floating-point unit: the register stack, its control and status words, and the
//! instructions 32-bit code uses for floats, conversions and the occasional `long double` idiom.
//!
//! Two units. The double unit, the default, keeps the stack as eight doubles in a global with a
//! global top: `st(i)` is the entry `top + i`. The 80-bit precision of the real unit is not kept;
//! a double is what the C the port becomes would use, and what a program that stores its results
//! to memory as doubles observes. The exact unit keeps eight 80-bit values in 16-byte slots and
//! does every operation through the runtime's software `f80` (`ra_f80_*` over pointers to the
//! slots), with the rounding and the precision control of the control word, so a program that
//! leans on the extended precision prints what the reference prints. The state is in globals
//! rather than in `%State` because it crosses calls: a 32-bit function returns a double in
//! `st(0)`, and the raising passes, which drop `%State`, leave globals alone.
//!
//! The control word matters for the rounding of `fist` and `frndint`: the usual idiom loads a
//! truncating control word, converts, and restores the old one. The status word carries the
//! condition codes of `fcom`, which `fnstsw ax; sahf` or a `test ah` reads.

use ra_asm_ir::{BinOp, BlockId, Cast, FCond, FnType, ICond, Inst, Operand, Type, ValueId};
use ra_asm_x86::iced_x86::{Instruction, Mnemonic as M, OpKind, Register as R};

use crate::state::{Flag, Machine, X87_CW, X87_STACK, X87_SW, X87_TMP, X87_TOP};
use crate::x86::Lifter;

/// Which x87 unit the lift models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum X87Unit {
    /// Eight doubles: what a program that keeps its results as doubles observes.
    #[default]
    Double,
    /// Eight 80-bit values through the runtime's software `f80`: the extended precision, the
    /// `tbyte` loads and stores, the precision control.
    Exact,
}

impl X87Unit {
    /// The name the command line, `rhipper.json` and the workbench use for the unit.
    pub fn name(self) -> &'static str {
        match self {
            Self::Double => "double",
            Self::Exact => "exact",
        }
    }
}

impl std::str::FromStr for X87Unit {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "double" => Ok(Self::Double),
            "exact" => Ok(Self::Exact),
            other => Err(format!("no x87 unit `{other}`: the units are double, exact")),
        }
    }
}

/// The bytes of a slot of the exact unit: the ten of the value and six of padding.
pub const F80_SLOT: u64 = 16;

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

/// The 80-bit constants the unit loads, as the significand and the sign-and-exponent word of
/// their `tbyte` form, rounded to the nearest as the hardware rounds them.
fn f80_constant(m: M) -> (u64, u64) {
    match m {
        M::Fld1 => (0x8000_0000_0000_0000, 0x3fff),
        M::Fldz => (0, 0),
        M::Fldpi => (0xc90f_daa2_2168_c235, 0x4000),
        M::Fldl2e => (0xb8aa_3b29_5c17_f0bc, 0x3fff),
        M::Fldln2 => (0xb172_17f7_d1cf_79ac, 0x3ffe),
        M::Fldlg2 => (0x9a20_9a84_fbcf_f799, 0x3ffd),
        _ => (0xd49a_784b_cd1b_8afe, 0x4000),
    }
}

/// The x87 stack and its words, over the globals: the lift's instructions and an import wrapper
/// that leaves a double result in `st(0)` both move it. A value of the unit is a double under the
/// double unit and a pointer to a 16-byte slot under the exact one.
impl Machine<'_> {
    /// A pointer to a global of the x87 state.
    fn x87_global(&mut self, block: BlockId, name: &str) -> ValueId {
        let address = Operand::Address { global: name.to_owned(), offset: 0 };
        self.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, &format!("{name}.p"))
    }

    pub(crate) fn exact(&self) -> bool {
        self.x87 == X87Unit::Exact
    }

    fn x87_top(&mut self, block: BlockId) -> ValueId {
        let p = self.x87_global(block, X87_TOP);
        self.b.load(block, Type::I64, p, "top")
    }

    fn set_x87_top(&mut self, block: BlockId, top: ValueId) {
        let p = self.x87_global(block, X87_TOP);
        self.b.store(block, Type::I64, top, p);
    }

    /// A pointer to `st(i)`: entry `(top + i) & 7` of the stack, of 8 or 16 bytes by the unit.
    fn st_ptr(&mut self, block: BlockId, i: i64) -> ValueId {
        let top = self.x87_top(block);
        let index = if i == 0 { top } else { self.b.bin(block, BinOp::Add, Type::I64, top, i, "st.i") };
        let wrapped = self.b.bin(block, BinOp::And, Type::I64, index, 7i64, "st.w");
        let shift = if self.exact() { 4i64 } else { 3 };
        let offset = self.b.bin(block, BinOp::Shl, Type::I64, wrapped, shift, "st.off");
        let base = Operand::Address { global: X87_STACK.to_owned(), offset: 0 };
        let address = self.b.bin(block, BinOp::Add, Type::I64, offset, base, "st.addr");
        self.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, "st.p")
    }

    /// `st(i)` as a value of the unit.
    fn st(&mut self, block: BlockId, i: i64) -> ValueId {
        let p = self.st_ptr(block, i);
        if self.exact() { p } else { self.b.load(block, Type::Double, p, &format!("st{i}")) }
    }

    /// Stores a value of the unit to `st(i)`.
    fn set_st(&mut self, block: BlockId, i: i64, value: ValueId) {
        let p = self.st_ptr(block, i);
        if self.exact() {
            self.f80_copy(block, p, value);
        } else {
            self.b.store(block, Type::Double, value, p);
        }
    }

    /// Moves the top down and returns the pointer to the new `st(0)`.
    fn fpush_ptr(&mut self, block: BlockId) -> ValueId {
        let top = self.x87_top(block);
        let down = self.b.bin(block, BinOp::Sub, Type::I64, top, 1i64, "top.d");
        let wrapped = self.b.bin(block, BinOp::And, Type::I64, down, 7i64, "top");
        self.set_x87_top(block, wrapped);
        self.st_ptr(block, 0)
    }

    /// Pushes a double: the top moves down and the new `st(0)` holds it, converted under the
    /// exact unit.
    pub(crate) fn fpush(&mut self, block: BlockId, value: impl Into<Operand>) {
        let p = self.fpush_ptr(block);
        if self.exact() {
            self.f80_call(block, "ra_f80_from_f64", vec![(Type::Ptr, p.into()), (Type::Double, value.into())]);
        } else {
            self.b.store(block, Type::Double, value, p);
        }
    }

    /// Pushes a value of the unit.
    fn fpush_value(&mut self, block: BlockId, value: ValueId) {
        let p = self.fpush_ptr(block);
        if self.exact() {
            self.f80_copy(block, p, value);
        } else {
            self.b.store(block, Type::Double, value, p);
        }
    }

    /// Pops: the top moves up. The value that was `st(0)` is the caller's to read first.
    fn fpop(&mut self, block: BlockId) {
        let top = self.x87_top(block);
        let up = self.b.bin(block, BinOp::Add, Type::I64, top, 1i64, "top.u");
        let wrapped = self.b.bin(block, BinOp::And, Type::I64, up, 7i64, "top");
        self.set_x87_top(block, wrapped);
    }

    fn control_word(&mut self, block: BlockId) -> ValueId {
        let p = self.x87_global(block, X87_CW);
        self.b.load(block, Type::I16, p, "cw")
    }

    /// The control word as the runtime takes it.
    fn control_word_wide(&mut self, block: BlockId) -> ValueId {
        let cw = self.control_word(block);
        self.zext(block, Type::I16, cw, Type::I64, "cw.w")
    }

    /// A call into the software `f80` without a result.
    fn f80_call(&mut self, block: BlockId, name: &str, args: Vec<(Type, Operand)>) {
        let params = args.iter().map(|(t, _)| t.clone()).collect();
        self.b.call(block, FnType::new(Type::Void, params), ra_asm_ir::Callee::Direct(name.to_owned()), args, "");
    }

    /// The scratch slot `n` of the exact unit: a memory operand or a constant converted for one
    /// instruction lives there.
    fn f80_tmp(&mut self, block: BlockId, n: i64) -> ValueId {
        let address = Operand::Address { global: X87_TMP.to_owned(), offset: n * F80_SLOT as i64 };
        self.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, "tmp.p")
    }

    /// Copies a slot: the 16 bytes as one word.
    fn f80_copy(&mut self, block: BlockId, dst: ValueId, src: ValueId) {
        let v = self.b.load(block, Type::I128, src, "f80");
        self.b.store(block, Type::I128, v, dst);
    }

    /// The exact value of a constant, from its `tbyte` words.
    fn f80_pack(&mut self, block: BlockId, p: ValueId, (significand, sign_exponent): (u64, u64)) {
        self.f80_call(
            block,
            "ra_f80_pack",
            vec![(Type::Ptr, p.into()), (Type::I64, (significand as i64).into()), (Type::I64, (sign_exponent as i64).into())],
        );
    }
}

impl Lifter<'_, '_> {
    /// The memory operand of an x87 instruction as a value of the unit: a float, a double, a
    /// `tbyte` (exact unit), or an integer of 16, 32 or 64 bits (`integer`), by the operand's size.
    fn fp_memory(&mut self, block: BlockId, i: &Instruction, n: u32, integer: bool) -> Option<ValueId> {
        let bits = self.op_bits(i, n);
        let address = self.mem_address(block, i);
        if self.m.exact() {
            let tmp = self.m.f80_tmp(block, 0);
            match (bits, integer) {
                (32, false) => {
                    let v = self.m.load(block, Type::Float, address, "m32");
                    let wide = self.m.b.cast(block, Cast::FPExt, Type::Float, v, Type::Double, "fx");
                    self.m.f80_call(block, "ra_f80_from_f64", vec![(Type::Ptr, tmp.into()), (Type::Double, wide.into())]);
                }
                (64, false) => {
                    let v = self.m.load(block, Type::Double, address, "m64");
                    self.m.f80_call(block, "ra_f80_from_f64", vec![(Type::Ptr, tmp.into()), (Type::Double, v.into())]);
                }
                (80, false) => {
                    let p = self.m.ptr(block, address, "m80.p");
                    self.m.f80_call(block, "ra_f80_load", vec![(Type::Ptr, tmp.into()), (Type::Ptr, p.into())]);
                }
                (16 | 32 | 64, true) => {
                    let v = self.m.load(block, Type::Int(bits), address, "mi");
                    let wide = self.m.sext(block, Type::Int(bits), v, Type::I64, "mi.w");
                    self.m.f80_call(block, "ra_f80_from_i64", vec![(Type::Ptr, tmp.into()), (Type::I64, wide.into())]);
                }
                _ => return None,
            }
            return Some(tmp);
        }
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

    /// Stores a value of the unit to the memory operand as a float, a double or (exact unit) a
    /// `tbyte`, by the operand's size; the narrowing rounds by the control word.
    fn store_fp_memory(&mut self, block: BlockId, i: &Instruction, n: u32, value: ValueId) -> bool {
        let bits = self.op_bits(i, n);
        let address = self.mem_address(block, i);
        if self.m.exact() {
            let cw = self.m.control_word_wide(block);
            let args = vec![(Type::Ptr, value.into()), (Type::I64, cw.into())];
            match bits {
                32 => {
                    let v = self.m.intrinsic(block, "ra_f80_to_f32", Type::Float, args, "fn");
                    self.m.store(block, Type::Float, v, address);
                }
                64 => {
                    let v = self.m.intrinsic(block, "ra_f80_to_f64", Type::Double, args, "fd");
                    self.m.store(block, Type::Double, v, address);
                }
                80 => {
                    let p = self.m.ptr(block, address, "m80.p");
                    self.m.f80_call(block, "ra_f80_store", vec![(Type::Ptr, p.into()), (Type::Ptr, value.into())]);
                }
                _ => return false,
            }
            return true;
        }
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

    /// The rounding mode of the control word: nearest even 0, down 1, up 2, toward zero 3.
    fn rounding_mode(&mut self, block: BlockId) -> ValueId {
        let cw = self.m.control_word_wide(block);
        let shifted = self.m.b.bin(block, BinOp::LShr, Type::I64, cw, 10i64, "rc.s");
        self.m.b.bin(block, BinOp::And, Type::I64, shifted, 3i64, "rc")
    }

    /// `value` rounded by the control word's rounding mode (double unit).
    fn round_by_control_word(&mut self, block: BlockId, value: ValueId) -> ValueId {
        let rc = self.rounding_mode(block);
        let d = Type::Double;
        let nearest = self.m.intrinsic(block, "llvm.rint.f64", d.clone(), vec![(d.clone(), value.into())], "rn");
        let down = self.m.intrinsic(block, "llvm.floor.f64", d.clone(), vec![(d.clone(), value.into())], "rd");
        let up = self.m.intrinsic(block, "llvm.ceil.f64", d.clone(), vec![(d.clone(), value.into())], "ru");
        let zero = self.m.intrinsic(block, "llvm.trunc.f64", d.clone(), vec![(d.clone(), value.into())], "rz");
        let is_zero = self.m.b.icmp(block, ICond::Eq, Type::I64, rc, 3i64, "rc.z");
        let is_up = self.m.b.icmp(block, ICond::Eq, Type::I64, rc, 2i64, "rc.u");
        let is_down = self.m.b.icmp(block, ICond::Eq, Type::I64, rc, 1i64, "rc.d");
        let a = self.m.b.select(block, is_down, d.clone(), down, nearest, "r.a");
        let b = self.m.b.select(block, is_up, d.clone(), up, a, "r.b");
        self.m.b.select(block, is_zero, d, zero, b, "rounded")
    }

    /// A compare of two values of the unit: `a < b`, unordered, `a == b`, as `i1`s.
    fn compare(&mut self, block: BlockId, a: ValueId, b: ValueId) -> (ValueId, ValueId, ValueId) {
        if self.m.exact() {
            // The runtime answers with the condition codes: less in bit 0, equal in bit 1,
            // unordered in bit 2 (and then all three, as the hardware sets them).
            let args = vec![(Type::Ptr, a.into()), (Type::Ptr, b.into())];
            let flags = self.m.intrinsic(block, "ra_f80_cmp", Type::I64, args, "fcmp");
            let mut bit = |mask: i64, hint: &str| {
                let masked = self.m.b.bin(block, BinOp::And, Type::I64, flags, mask, &format!("{hint}.m"));
                self.m.b.icmp(block, ICond::Ne, Type::I64, masked, 0i64, hint)
            };
            let less = bit(1, "lt");
            let equal = bit(2, "eq");
            let unordered = bit(4, "uno");
            return (less, unordered, equal);
        }
        let d = Type::Double;
        let less = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "lt");
        let unordered = self.m.b.value(block, Inst::FCmp { cond: FCond::Uno, ty: d.clone(), lhs: a.into(), rhs: b.into() }, "uno");
        let equal = self.m.b.value(block, Inst::FCmp { cond: FCond::Oeq, ty: d, lhs: a.into(), rhs: b.into() }, "eq");
        // An unordered compare sets all three.
        let less = self.m.b.bin(block, BinOp::Or, Type::I1, less, unordered, "lt.u");
        let equal = self.m.b.bin(block, BinOp::Or, Type::I1, equal, unordered, "eq.u");
        (less, unordered, equal)
    }

    /// Sets C0, C2 and C3 of the status word.
    fn set_condition_codes(&mut self, block: BlockId, c0: ValueId, c2: ValueId, c3: ValueId) {
        let p = self.m.x87_global(block, X87_SW);
        let sw = self.m.b.load(block, Type::I16, p, "sw");
        let cleared = self.m.b.bin(block, BinOp::And, Type::I16, sw, !0x4500i64 & 0xffff, "sw.c");
        let mut acc = cleared;
        for (bit, flag) in [(8i64, c0), (10, c2), (14, c3)] {
            let wide = self.m.zext(block, Type::I1, flag, Type::I16, "cc.w");
            let placed = self.m.b.bin(block, BinOp::Shl, Type::I16, wide, bit, "cc.p");
            acc = self.m.b.bin(block, BinOp::Or, Type::I16, acc, placed, "sw");
        }
        self.m.b.store(block, Type::I16, acc, p);
    }

    /// The zero of the unit, for `ftst` and `fxam`.
    fn fp_zero(&mut self, block: BlockId) -> ValueId {
        if self.m.exact() {
            let p = self.m.f80_tmp(block, 1);
            self.m.f80_pack(block, p, f80_constant(M::Fldz));
            p
        } else {
            self.m.value_of(block, Type::Double, Operand::Float(0.0))
        }
    }

    /// Operand `n` as a value of the unit: memory by its size, or an `st(i)` register. iced lists
    /// the implicit `st(0)` as an operand, so `fld m64` is `fld st, m64` and `fistp m64` is
    /// `fistp m64, st`: the destination is operand 0 and the source operand 1 throughout.
    fn fp_operand(&mut self, block: BlockId, i: &Instruction, n: u32, integer: bool) -> Option<ValueId> {
        if i.op_kind(n) == OpKind::Memory {
            self.fp_memory(block, i, n, integer)
        } else {
            Some(self.m.st(block, st_index(i.op_register(n))))
        }
    }

    /// The source of a one-source instruction: operand 1 when there are two, else operand 0, else
    /// `st(1)`.
    fn source_operand(&mut self, block: BlockId, i: &Instruction, integer: bool) -> Option<ValueId> {
        match i.op_count() {
            2 => self.fp_operand(block, i, 1, integer),
            1 => self.fp_operand(block, i, 0, integer),
            _ => Some(self.m.st(block, 1)),
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
                let a = self.m.st(block, 1);
                let b = self.m.st(block, 0);
                (1, a, b)
            }
            1 => {
                let st0 = self.m.st(block, 0);
                let other = self.fp_operand(block, i, 0, integer)?;
                (0, st0, other)
            }
            _ => {
                let dst = if i.op_kind(0) == OpKind::Memory { 0 } else { st_index(i.op_register(0)) };
                let a = self.m.st(block, dst);
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

    /// A one-operand operation of the exact unit on `st(0)`, in place: `ra_f80_<name>(st0, st0[, cw])`.
    fn f80_unary(&mut self, block: BlockId, name: &str, with_control_word: bool) {
        let p = self.m.st_ptr(block, 0);
        let mut args = vec![(Type::Ptr, p.into()), (Type::Ptr, p.into())];
        if with_control_word {
            let cw = self.m.control_word_wide(block);
            args.push((Type::I64, cw.into()));
        }
        self.m.f80_call(block, name, args);
    }

    /// Lifts one x87 instruction.
    #[allow(clippy::many_single_char_names)]
    pub(crate) fn x87(&mut self, block: BlockId, i: &Instruction) {
        let ip = i.ip();
        let d = Type::Double;
        let exact = self.m.exact();
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
                self.m.fpush_value(block, v);
            }
            M::Fld1 | M::Fldz | M::Fldpi | M::Fldl2e | M::Fldln2 | M::Fldlg2 | M::Fldl2t => {
                if exact {
                    let p = self.m.fpush_ptr(block);
                    self.m.f80_pack(block, p, f80_constant(i.mnemonic()));
                } else {
                    let c = match i.mnemonic() {
                        M::Fld1 => 1.0,
                        M::Fldz => 0.0,
                        M::Fldpi => std::f64::consts::PI,
                        M::Fldl2e => std::f64::consts::LOG2_E,
                        M::Fldln2 => std::f64::consts::LN_2,
                        M::Fldlg2 => std::f64::consts::LOG10_2,
                        _ => std::f64::consts::LOG2_10,
                    };
                    self.m.fpush(block, Operand::Float(c));
                }
            }
            M::Fild => {
                let Some(n) = Self::memory_operand(i) else { unsupported!() };
                let Some(v) = self.fp_memory(block, i, n, true) else { unsupported!() };
                self.m.fpush_value(block, v);
            }
            M::Fst | M::Fstp => {
                let v = self.m.st(block, 0);
                if i.op_kind(0) == OpKind::Memory {
                    if !self.store_fp_memory(block, i, 0, v) {
                        unsupported!()
                    }
                } else {
                    self.m.set_st(block, st_index(i.op_register(0)), v);
                }
                if i.mnemonic() == M::Fstp {
                    self.m.fpop(block);
                }
            }
            M::Fist | M::Fistp | M::Fisttp => {
                let Some(n) = Self::memory_operand(i) else { unsupported!() };
                let bits = self.op_bits(i, n);
                if !matches!(bits, 16 | 32 | 64) {
                    unsupported!()
                }
                let v = self.m.st(block, 0);
                let truncating = i.mnemonic() == M::Fisttp;
                let n = if exact {
                    let rc = if truncating { self.m.value_of(block, Type::I64, 3i64) } else { self.rounding_mode(block) };
                    let args = vec![(Type::Ptr, v.into()), (Type::I64, rc.into()), (Type::I64, i64::from(bits).into())];
                    let wide = self.m.intrinsic(block, "ra_f80_to_int", Type::I64, args, "fi.w");
                    self.m.trunc(block, Type::I64, wide, Type::Int(bits), "fi")
                } else {
                    let rounded = if truncating {
                        self.m.intrinsic(block, "llvm.trunc.f64", d.clone(), vec![(d.clone(), v.into())], "rz")
                    } else {
                        self.round_by_control_word(block, v)
                    };
                    self.m.b.cast(block, Cast::FPToSI, d, rounded, Type::Int(bits), "fi")
                };
                let address = self.mem_address(block, i);
                self.m.store(block, Type::Int(bits), n, address);
                if i.mnemonic() != M::Fist {
                    self.m.fpop(block);
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
                let (op, name) = match m {
                    M::Fadd | M::Faddp | M::Fiadd => (BinOp::FAdd, "ra_f80_add"),
                    M::Fsub | M::Fsubp | M::Fisub | M::Fsubr | M::Fsubrp | M::Fisubr => (BinOp::FSub, "ra_f80_sub"),
                    M::Fmul | M::Fmulp | M::Fimul => (BinOp::FMul, "ra_f80_mul"),
                    _ => (BinOp::FDiv, "ra_f80_div"),
                };
                if exact {
                    let p = self.m.st_ptr(block, dst);
                    let cw = self.m.control_word_wide(block);
                    let args = vec![(Type::Ptr, p.into()), (Type::Ptr, a.into()), (Type::Ptr, b.into()), (Type::I64, cw.into())];
                    self.m.f80_call(block, name, args);
                } else {
                    let r = self.m.b.bin(block, op, d, a, b, "f");
                    self.m.set_st(block, dst, r);
                }
                if matches!(m, M::Faddp | M::Fsubp | M::Fsubrp | M::Fmulp | M::Fdivp | M::Fdivrp) {
                    self.m.fpop(block);
                }
            }
            M::Fchs if exact => self.f80_unary(block, "ra_f80_chs", false),
            M::Fabs if exact => self.f80_unary(block, "ra_f80_abs", false),
            M::Fsqrt if exact => self.f80_unary(block, "ra_f80_sqrt", true),
            M::Frndint if exact => self.f80_unary(block, "ra_f80_rndint", true),
            M::Fchs => {
                let v = self.m.st(block, 0);
                let r = self.m.b.bin(block, BinOp::FSub, d, Operand::Float(-0.0), v, "neg");
                self.m.set_st(block, 0, r);
            }
            M::Fabs => {
                let v = self.m.st(block, 0);
                let r = self.m.intrinsic(block, "llvm.fabs.f64", d.clone(), vec![(d, v.into())], "abs");
                self.m.set_st(block, 0, r);
            }
            M::Fsqrt => {
                let v = self.m.st(block, 0);
                let r = self.m.intrinsic(block, "llvm.sqrt.f64", d.clone(), vec![(d, v.into())], "sqrt");
                self.m.set_st(block, 0, r);
            }
            M::Frndint => {
                let v = self.m.st(block, 0);
                let r = self.round_by_control_word(block, v);
                self.m.set_st(block, 0, r);
            }
            M::Fxch => {
                let other = Self::source_index(i);
                if exact {
                    let pa = self.m.st_ptr(block, 0);
                    let pb = self.m.st_ptr(block, other);
                    let a = self.m.b.load(block, Type::I128, pa, "st0");
                    let b = self.m.b.load(block, Type::I128, pb, "sti");
                    self.m.b.store(block, Type::I128, b, pa);
                    self.m.b.store(block, Type::I128, a, pb);
                } else {
                    let a = self.m.st(block, 0);
                    let b = self.m.st(block, other);
                    self.m.set_st(block, 0, b);
                    self.m.set_st(block, other, a);
                }
            }
            M::Fcom | M::Fcomp | M::Fcompp | M::Fucom | M::Fucomp | M::Fucompp | M::Ficom | M::Ficomp | M::Ftst => {
                let m = i.mnemonic();
                let a = self.m.st(block, 0);
                let b = if m == M::Ftst {
                    self.fp_zero(block)
                } else {
                    let Some(v) = self.source_operand(block, i, matches!(m, M::Ficom | M::Ficomp)) else { unsupported!() };
                    v
                };
                let (less, unordered, equal) = self.compare(block, a, b);
                self.set_condition_codes(block, less, unordered, equal);
                match m {
                    M::Fcomp | M::Fucomp | M::Ficomp => self.m.fpop(block),
                    M::Fcompp | M::Fucompp => {
                        self.m.fpop(block);
                        self.m.fpop(block);
                    }
                    _ => {}
                }
            }
            M::Fcomi | M::Fcomip | M::Fucomi | M::Fucomip => {
                // ZF, PF and CF from the compare, as `fcomi` sets them.
                let a = self.m.st(block, 0);
                let b = self.m.st(block, Self::source_index(i));
                let (less, unordered, equal) = self.compare(block, a, b);
                self.m.set_flag(block, Flag::Zf, equal);
                self.m.set_flag(block, Flag::Pf, unordered);
                self.m.set_flag(block, Flag::Cf, less);
                if matches!(i.mnemonic(), M::Fcomip | M::Fucomip) {
                    self.m.fpop(block);
                }
            }
            M::Fxam => {
                // C3, C2, C0 classify st(0): zero is 100, a normal number is 010, NaN is 001.
                let v = self.m.st(block, 0);
                let zero = self.fp_zero(block);
                let (_, nan, equal) = self.compare(block, v, zero);
                let not_nan = self.m.b.bin(block, BinOp::Xor, Type::I1, nan, 1i64, "ord");
                let zero = self.m.b.bin(block, BinOp::And, Type::I1, equal, not_nan, "z");
                let neither = {
                    let either = self.m.b.bin(block, BinOp::Or, Type::I1, zero, nan, "zn");
                    self.m.b.bin(block, BinOp::Xor, Type::I1, either, 1i64, "normal")
                };
                self.set_condition_codes(block, nan, neither, zero);
            }
            M::Fnstsw | M::Fstsw => {
                let p = self.m.x87_global(block, X87_SW);
                let sw = self.m.b.load(block, Type::I16, p, "sw");
                // The top of stack field, bits 11 to 13, as the hardware would report it.
                let top = self.m.x87_top(block);
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
                let cw = self.m.control_word(block);
                let address = self.mem_address(block, i);
                self.m.store(block, Type::I16, cw, address);
            }
            M::Fldcw => {
                let address = self.mem_address(block, i);
                let cw = self.m.load(block, Type::I16, address, "cw.m");
                let p = self.m.x87_global(block, X87_CW);
                self.m.b.store(block, Type::I16, cw, p);
            }
            M::Fninit | M::Finit => {
                let top = self.m.value_of(block, Type::I64, 0i64);
                self.m.set_x87_top(block, top);
                let cw = self.m.x87_global(block, X87_CW);
                self.m.b.store(block, Type::I16, 0x037fi64, cw);
                let sw = self.m.x87_global(block, X87_SW);
                self.m.b.store(block, Type::I16, 0i64, sw);
            }
            M::Fnclex | M::Fclex | M::Ffree | M::Fnop => {}
            M::Ffreep | M::Fincstp => self.m.fpop(block),
            M::Fdecstp => {
                let top = self.m.x87_top(block);
                let down = self.m.b.bin(block, BinOp::Sub, Type::I64, top, 1i64, "top.d");
                let wrapped = self.m.b.bin(block, BinOp::And, Type::I64, down, 7i64, "top");
                self.m.set_x87_top(block, wrapped);
            }
            M::Fcmovb | M::Fcmove | M::Fcmovbe | M::Fcmovu | M::Fcmovnb | M::Fcmovne | M::Fcmovnbe | M::Fcmovnu => {
                let cond = self.condition(block, i.condition_code());
                if exact {
                    let pa = self.m.st_ptr(block, 0);
                    let pb = self.m.st_ptr(block, Self::source_index(i));
                    let a = self.m.b.load(block, Type::I128, pa, "st0");
                    let b = self.m.b.load(block, Type::I128, pb, "sti");
                    let r = self.m.b.select(block, cond, Type::I128, b, a, "fcmov");
                    self.m.b.store(block, Type::I128, r, pa);
                } else {
                    let a = self.m.st(block, 0);
                    let b = self.m.st(block, Self::source_index(i));
                    let r = self.m.b.select(block, cond, d, b, a, "fcmov");
                    self.m.set_st(block, 0, r);
                }
            }
            _ => unsupported!(),
        }
    }
}
