//! The machine state and the helpers that read and write it: registers, flags and memory.
//!
//! `%State = type { [16 x i64], i64, [8 x i8], [16 x i128] }`: the general registers in iced's
//! order, `rip`, the flags as bytes (cf, pf, af, zf, sf, df, of), and the vector registers. The
//! driver's C `struct State` has the same layout.

use ra_asm_ir::{BinOp, BlockId, CallConv, Callee, Cast, Declaration, FnType, FunctionBuilder, ICond, Operand, Type, ValueId};
use ra_asm_x86::iced_x86::Register as R;

pub const STATE_TYPE: &str = "State";

/// The fields of `%State`.
pub fn state_fields() -> Vec<Type> {
    vec![Type::array(16, Type::I64), Type::I64, Type::array(8, Type::I8), Type::array(16, Type::I128)]
}

/// The flags, by their byte index in the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Cf = 0,
    Pf = 1,
    Af = 2,
    Zf = 3,
    Sf = 4,
    Df = 5,
    Of = 6,
}

/// The signature of an import adapter: four register arguments and four stack slots, as integers.
pub fn import_adapter_type() -> FnType {
    FnType::new(Type::I64, vec![Type::I64; 8])
}

/// Declares the runtime functions the lifted code calls.
pub fn declare_runtime(module: &mut ra_asm_ir::Module) {
    let void = |params: Vec<Type>| FnType::new(Type::Void, params);
    module.declare(Declaration {
        name: "ra_trap".into(),
        ty: void(vec![Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec!["noreturn".into()],
    });
    module.declare(Declaration { name: "ra_call".into(), ty: void(vec![Type::Ptr, Type::I64]), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration {
        name: "ra_unsupported".into(),
        ty: void(vec![Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec!["noreturn".into()],
    });
    module.declare(Declaration {
        name: "llvm.ctpop.i8".into(),
        ty: FnType::new(Type::I8, vec![Type::I8]),
        conv: CallConv::C,
        attrs: vec![],
    });
    for bits in [8u16, 16, 32, 64] {
        let t = Type::Int(bits);
        module.declare(Declaration {
            name: format!("llvm.fshl.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.fshr.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.bswap.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ctpop.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.cttz.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), Type::I1]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ctlz.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), Type::I1]),
            conv: CallConv::C,
            attrs: vec![],
        });
    }
    for (name, t) in [("f32", Type::Float), ("f64", Type::Double)] {
        module.declare(Declaration {
            name: format!("llvm.sqrt.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.fabs.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.minnum.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.maxnum.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.trunc.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.floor.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ceil.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.rint.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
    }
}

/// The globals the lifted code keeps beside the state: the x87 stack with its top, control word
/// and status word (the x87 state crosses calls, since a 32-bit function returns a double in
/// `st(0)`, so it lives in globals rather than in `%State`, and survives every raising pass as
/// plain memory), and a thread environment block for `fs:` and `gs:` accesses, zero like a TEB
/// the program never filled: an exception chain push lands in it and nothing reads it back.
pub const X87_STACK: &str = "ra_x87_stack";
pub const X87_TOP: &str = "ra_x87_top";
pub const X87_CW: &str = "ra_x87_cw";
pub const X87_SW: &str = "ra_x87_sw";
pub const TEB: &str = "ra_teb";
pub const TEB_SIZE: u64 = 4096;

pub fn define_runtime_globals(module: &mut ra_asm_ir::Module) {
    use ra_asm_ir::{Const, Global, Linkage};
    let global = |name: &str, ty: Type, init: Const, align: u32| Global {
        name: name.to_owned(),
        ty,
        init: Some(init),
        constant: false,
        linkage: Linkage::External,
        align,
        address: None,
    };
    module.globals.push(global(X87_STACK, Type::array(8, Type::Double), Const::Zero, 16));
    module.globals.push(global(X87_TOP, Type::I64, Const::Int(0), 8));
    module.globals.push(global(X87_CW, Type::I16, Const::Int(0x037f), 2));
    module.globals.push(global(X87_SW, Type::I16, Const::Int(0), 2));
    module.globals.push(global(TEB, Type::array(TEB_SIZE, Type::I8), Const::Zero, 16));
}

/// Whether `name` is one of the globals of [`define_runtime_globals`].
pub fn is_runtime_global(name: &str) -> bool {
    matches!(name, X87_STACK | X87_TOP | X87_CW | X87_SW | TEB)
}

/// The state helpers over one function under construction.
pub struct Machine<'b> {
    pub b: &'b mut FunctionBuilder,
    pub state: ValueId,
    /// 32 or 64: the address width.
    pub bits: u32,
}

impl Machine<'_> {
    pub fn address_type(&self) -> Type {
        Type::Int(self.bits as u16)
    }

    fn state_ty() -> Type {
        Type::Named(STATE_TYPE.to_owned())
    }

    // region Pointers into the state

    /// The pointer to the slot of a general register, by iced's number (rax is 0).
    pub fn gpr_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = format!("p.{}", GPR_NAMES[number as usize]);
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 0i64.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    pub fn flag_ptr(&mut self, block: BlockId, flag: Flag) -> ValueId {
        let hint = format!("p.{}", flag_name(flag));
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 2i64.into()), (Type::I32, (flag as i64).into())],
            &hint,
        )
    }

    pub fn xmm_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = format!("p.xmm{number}");
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 3i64.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    // endregion

    // region Registers

    /// The value of a general register or a part of one, at its own width.
    pub fn read_gpr(&mut self, block: BlockId, reg: R) -> ValueId {
        let full = reg.full_register();
        let p = self.gpr_ptr(block, full.number() as u32);
        let whole = self.b.load(block, Type::I64, p, GPR_NAMES[full.number()]);
        let bits = (reg.size() * 8) as u16;
        if bits == 64 {
            return whole;
        }
        let shifted = if is_high_byte(reg) { self.b.bin(block, BinOp::LShr, Type::I64, whole, 8i64, "hi") } else { whole };
        self.b.cast(block, Cast::Trunc, Type::I64, shifted, Type::Int(bits), reg_name(reg))
    }

    /// Writes a general register or a part of one. A 32-bit write zero-extends: in 64-bit mode
    /// that is the machine's rule, and in 32-bit mode nothing reads the upper half.
    pub fn write_gpr(&mut self, block: BlockId, reg: R, value: impl Into<Operand>) {
        let full = reg.full_register();
        let p = self.gpr_ptr(block, full.number() as u32);
        let bits = (reg.size() * 8) as u16;
        let value = value.into();
        if bits == 64 {
            self.b.store(block, Type::I64, value, p);
            return;
        }
        let wide = self.b.cast(block, Cast::ZExt, Type::Int(bits), value, Type::I64, &format!("{}.z", reg_name(reg)));
        if bits == 32 {
            self.b.store(block, Type::I64, wide, p);
            return;
        }
        let whole = self.b.load(block, Type::I64, p, GPR_NAMES[full.number()]);
        let (mask, shift) = if is_high_byte(reg) { (!0xff00u64, 8) } else { (!((1u64 << bits) - 1), 0) };
        let kept = self.b.bin(block, BinOp::And, Type::I64, whole, mask as i64, "kept");
        let placed = if shift > 0 { self.b.bin(block, BinOp::Shl, Type::I64, wide, shift, "placed") } else { wide };
        let merged = self.b.bin(block, BinOp::Or, Type::I64, kept, placed, GPR_NAMES[full.number()]);
        self.b.store(block, Type::I64, merged, p);
    }

    /// The stack pointer, at the address width.
    pub fn read_sp(&mut self, block: BlockId) -> ValueId {
        self.read_gpr(block, if self.bits == 64 { R::RSP } else { R::ESP })
    }

    pub fn write_sp(&mut self, block: BlockId, value: impl Into<Operand>) {
        self.write_gpr(block, if self.bits == 64 { R::RSP } else { R::ESP }, value);
    }

    /// Reads a vector register as `ty`, from its low bytes.
    pub fn read_xmm(&mut self, block: BlockId, reg: R, ty: Type) -> ValueId {
        let p = self.xmm_ptr(block, reg.number() as u32);
        self.b.load(block, ty, p, reg_name(reg))
    }

    /// Writes the low bytes of a vector register as `ty`; with `zero_upper`, the rest becomes zero.
    pub fn write_xmm(&mut self, block: BlockId, reg: R, ty: Type, value: impl Into<Operand>, zero_upper: bool) {
        let p = self.xmm_ptr(block, reg.number() as u32);
        if zero_upper && ty.bits() != Some(128) {
            self.b.store(block, Type::I128, 0i64, p);
        }
        self.b.store(block, ty, value, p);
    }

    // endregion

    // region Flags

    pub fn get_flag(&mut self, block: BlockId, flag: Flag) -> ValueId {
        let p = self.flag_ptr(block, flag);
        let byte = self.b.load(block, Type::I8, p, &format!("{}.b", flag_name(flag)));
        self.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, flag_name(flag))
    }

    pub fn set_flag(&mut self, block: BlockId, flag: Flag, value: impl Into<Operand>) {
        let p = self.flag_ptr(block, flag);
        let byte = self.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{}.b", flag_name(flag)));
        self.b.store(block, Type::I8, byte, p);
    }

    pub fn set_flag_const(&mut self, block: BlockId, flag: Flag, value: bool) {
        let p = self.flag_ptr(block, flag);
        self.b.store(block, Type::I8, i64::from(value), p);
    }

    /// ZF, SF and PF from a result.
    pub fn set_result_flags(&mut self, block: BlockId, ty: Type, result: ValueId) {
        let zf = self.b.icmp(block, ICond::Eq, ty.clone(), result, 0i64, "zf");
        self.set_flag(block, Flag::Zf, zf);
        let sf = self.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "sf");
        self.set_flag(block, Flag::Sf, sf);
        let low = if ty == Type::I8 { result } else { self.b.cast(block, Cast::Trunc, ty, result, Type::I8, "low") };
        let ones = self
            .b
            .call(
                block,
                FnType::new(Type::I8, vec![Type::I8]),
                Callee::Direct("llvm.ctpop.i8".into()),
                vec![(Type::I8, low.into())],
                "ones",
            )
            .unwrap();
        let odd = self.b.bin(block, BinOp::And, Type::I8, ones, 1i64, "odd");
        let pf = self.b.icmp(block, ICond::Eq, Type::I8, odd, 0i64, "pf");
        self.set_flag(block, Flag::Pf, pf);
    }

    /// AF from the operands and the result of an add or a sub: bit 4 of `lhs ^ rhs ^ result`.
    pub fn set_af(&mut self, block: BlockId, ty: Type, lhs: impl Into<Operand>, rhs: impl Into<Operand>, result: ValueId) {
        let x = self.b.bin(block, BinOp::Xor, ty.clone(), lhs, rhs, "af.x");
        let y = self.b.bin(block, BinOp::Xor, ty.clone(), x, result, "af.y");
        let bit = self.b.bin(block, BinOp::And, ty.clone(), y, 16i64, "af.bit");
        let af = self.b.icmp(block, ICond::Ne, ty, bit, 0i64, "af");
        self.set_flag(block, Flag::Af, af);
    }

    // endregion

    // region Memory

    /// A host pointer for a machine address.
    pub fn ptr(&mut self, block: BlockId, address: impl Into<Operand>, hint: &str) -> ValueId {
        let address = address.into();
        let wide = if self.bits == 64 { address } else { self.b.cast(block, Cast::ZExt, Type::I32, address, Type::I64, "addr.z").into() };
        self.b.cast(block, Cast::IntToPtr, Type::I64, wide, Type::Ptr, hint)
    }

    pub fn load(&mut self, block: BlockId, ty: Type, address: impl Into<Operand>, hint: &str) -> ValueId {
        let p = self.ptr(block, address, &format!("{hint}.p"));
        self.b.load(block, ty, p, hint)
    }

    pub fn store(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>, address: impl Into<Operand>) {
        let p = self.ptr(block, address, "st.p");
        self.b.store(block, ty, value, p);
    }

    /// Pushes a value of the address width.
    pub fn push(&mut self, block: BlockId, value: impl Into<Operand>) {
        let ty = self.address_type();
        let sp = self.read_sp(block);
        let new_sp = self.b.bin(block, BinOp::Sub, ty.clone(), sp, i64::from(self.bits / 8), "sp");
        self.store(block, ty, value, new_sp);
        self.write_sp(block, new_sp);
    }

    /// Pops a value of the address width.
    pub fn pop(&mut self, block: BlockId, hint: &str) -> ValueId {
        let ty = self.address_type();
        let sp = self.read_sp(block);
        let value = self.load(block, ty.clone(), sp, hint);
        let new_sp = self.b.bin(block, BinOp::Add, ty, sp, i64::from(self.bits / 8), "sp");
        self.write_sp(block, new_sp);
        value
    }

    // endregion

    /// `call @ra_trap(address, code)`, then `unreachable`.
    pub fn trap(&mut self, block: BlockId, address: u64, code: u64) {
        self.b.call(
            block,
            FnType::new(Type::Void, vec![Type::I64, Type::I64]),
            Callee::Direct("ra_trap".into()),
            vec![(Type::I64, address.into()), (Type::I64, code.into())],
            "",
        );
        self.b.terminate(block, ra_asm_ir::Terminator::Unreachable);
    }

    /// `call @ra_unsupported(address, code)`, then `unreachable`: an instruction the lift lacks.
    pub fn unsupported(&mut self, block: BlockId, address: u64, code: u64) {
        self.b.call(
            block,
            FnType::new(Type::Void, vec![Type::I64, Type::I64]),
            Callee::Direct("ra_unsupported".into()),
            vec![(Type::I64, address.into()), (Type::I64, code.into())],
            "",
        );
        self.b.terminate(block, ra_asm_ir::Terminator::Unreachable);
    }

    /// `select cond, a, b` on an integer type.
    pub fn select(
        &mut self,
        block: BlockId,
        cond: impl Into<Operand>,
        ty: Type,
        a: impl Into<Operand>,
        b: impl Into<Operand>,
        hint: &str,
    ) -> ValueId {
        self.b.select(block, cond, ty, a, b, hint)
    }

    /// An intrinsic call with one result.
    pub fn intrinsic(&mut self, block: BlockId, name: &str, ret: Type, args: Vec<(Type, Operand)>, hint: &str) -> ValueId {
        let params = args.iter().map(|(t, _)| t.clone()).collect();
        self.b.call(block, FnType::new(ret, params), Callee::Direct(name.to_owned()), args, hint).unwrap()
    }

    pub fn is_true(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>, hint: &str) -> ValueId {
        self.b.icmp(block, ICond::Ne, ty, value, 0i64, hint)
    }

    pub fn zext(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::ZExt, from, value, to, hint)
    }

    pub fn sext(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::SExt, from, value, to, hint)
    }

    pub fn trunc(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::Trunc, from, value, to, hint)
    }

    /// Materializes an operand as a value.
    pub fn value_of(&mut self, block: BlockId, ty: Type, operand: impl Into<Operand>) -> ValueId {
        match operand.into() {
            Operand::Local(v) => v,
            other => self.b.bin(block, BinOp::Or, ty, other, 0i64, "c"),
        }
    }
}

/// iced's general register numbers, in order.
const GPR_NAMES: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];

fn flag_name(flag: Flag) -> &'static str {
    match flag {
        Flag::Cf => "cf",
        Flag::Pf => "pf",
        Flag::Af => "af",
        Flag::Zf => "zf",
        Flag::Sf => "sf",
        Flag::Df => "df",
        Flag::Of => "of",
    }
}

fn is_high_byte(reg: R) -> bool {
    matches!(reg, R::AH | R::BH | R::CH | R::DH)
}

pub fn reg_name(reg: R) -> &'static str {
    ra_asm_x86::registers::name(reg)
}
