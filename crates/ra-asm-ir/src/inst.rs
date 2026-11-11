//! The instructions and their operands.

use crate::module::{BlockId, ValueId};
use crate::types::{FnType, Type};

/// An operand: a local value, a constant, or a global.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Local(ValueId),
    /// An integer constant of the instruction's type.
    Int(i128),
    /// A floating-point constant, printed in LLVM's hexadecimal form from its bits.
    Float(f64),
    /// `@name`.
    Global(String),
    Null,
    Undef,
    Zero,
}

impl Operand {
    pub const TRUE: Operand = Operand::Int(1);
    pub const FALSE: Operand = Operand::Int(0);

    pub fn local(&self) -> Option<ValueId> {
        match self {
            Self::Local(v) => Some(*v),
            _ => None,
        }
    }
}

impl From<ValueId> for Operand {
    fn from(v: ValueId) -> Self {
        Self::Local(v)
    }
}

impl From<i128> for Operand {
    fn from(v: i128) -> Self {
        Self::Int(v)
    }
}

impl From<i64> for Operand {
    fn from(v: i64) -> Self {
        Self::Int(i128::from(v))
    }
}

impl From<u64> for Operand {
    fn from(v: u64) -> Self {
        Self::Int(i128::from(v))
    }
}

impl From<i32> for Operand {
    fn from(v: i32) -> Self {
        Self::Int(i128::from(v))
    }
}

impl From<u32> for Operand {
    fn from(v: u32) -> Self {
        Self::Int(i128::from(v))
    }
}

/// The binary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    UDiv,
    SDiv,
    URem,
    SRem,
    Shl,
    LShr,
    AShr,
    And,
    Or,
    Xor,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
}

impl BinOp {
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::UDiv => "udiv",
            Self::SDiv => "sdiv",
            Self::URem => "urem",
            Self::SRem => "srem",
            Self::Shl => "shl",
            Self::LShr => "lshr",
            Self::AShr => "ashr",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::FAdd => "fadd",
            Self::FSub => "fsub",
            Self::FMul => "fmul",
            Self::FDiv => "fdiv",
            Self::FRem => "frem",
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::FAdd | Self::FSub | Self::FMul | Self::FDiv | Self::FRem)
    }
}

/// The integer comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ICond {
    Eq,
    Ne,
    Ugt,
    Uge,
    Ult,
    Ule,
    Sgt,
    Sge,
    Slt,
    Sle,
}

impl ICond {
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Sgt => "sgt",
            Self::Sge => "sge",
            Self::Slt => "slt",
            Self::Sle => "sle",
        }
    }

    #[must_use]
    pub fn inverse(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::Ugt => Self::Ule,
            Self::Uge => Self::Ult,
            Self::Ult => Self::Uge,
            Self::Ule => Self::Ugt,
            Self::Sgt => Self::Sle,
            Self::Sge => Self::Slt,
            Self::Slt => Self::Sge,
            Self::Sle => Self::Sgt,
        }
    }
}

/// The floating-point comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FCond {
    Oeq,
    Ogt,
    Oge,
    Olt,
    Ole,
    One,
    Ord,
    Ueq,
    Ugt,
    Uge,
    Ult,
    Ule,
    Une,
    Uno,
}

impl FCond {
    pub fn name(self) -> &'static str {
        match self {
            Self::Oeq => "oeq",
            Self::Ogt => "ogt",
            Self::Oge => "oge",
            Self::Olt => "olt",
            Self::Ole => "ole",
            Self::One => "one",
            Self::Ord => "ord",
            Self::Ueq => "ueq",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Une => "une",
            Self::Uno => "uno",
        }
    }
}

/// The casts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cast {
    Trunc,
    ZExt,
    SExt,
    FPTrunc,
    FPExt,
    FPToUI,
    FPToSI,
    UIToFP,
    SIToFP,
    PtrToInt,
    IntToPtr,
    Bitcast,
}

impl Cast {
    pub fn name(self) -> &'static str {
        match self {
            Self::Trunc => "trunc",
            Self::ZExt => "zext",
            Self::SExt => "sext",
            Self::FPTrunc => "fptrunc",
            Self::FPExt => "fpext",
            Self::FPToUI => "fptoui",
            Self::FPToSI => "fptosi",
            Self::UIToFP => "uitofp",
            Self::SIToFP => "sitofp",
            Self::PtrToInt => "ptrtoint",
            Self::IntToPtr => "inttoptr",
            Self::Bitcast => "bitcast",
        }
    }
}

/// What a call calls.
#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    /// A function or declaration of the module, by name.
    Direct(String),
    /// A pointer value.
    Indirect(Operand),
}

/// One non-terminating instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Inst {
    Alloca {
        ty: Type,
        align: u32,
    },
    Load {
        ty: Type,
        ptr: Operand,
        align: u32,
        volatile: bool,
    },
    Store {
        ty: Type,
        value: Operand,
        ptr: Operand,
        align: u32,
        volatile: bool,
    },
    Bin {
        op: BinOp,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
    },
    ICmp {
        cond: ICond,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
    },
    FCmp {
        cond: FCond,
        ty: Type,
        lhs: Operand,
        rhs: Operand,
    },
    /// `shufflevector a, b, mask`: `ty` is the type of `a` and `b`; the result has `mask.len()` lanes.
    /// A negative mask entry is `poison`.
    ShuffleVector {
        ty: Type,
        a: Operand,
        b: Operand,
        mask: Vec<i32>,
    },
    Select {
        cond: Operand,
        ty: Type,
        then: Operand,
        els: Operand,
    },
    Cast {
        cast: Cast,
        from: Type,
        value: Operand,
        to: Type,
    },
    /// `getelementptr inbounds base_ty, ptr, indices`.
    Gep {
        base_ty: Type,
        ptr: Operand,
        indices: Vec<(Type, Operand)>,
        inbounds: bool,
    },
    ExtractElement {
        ty: Type,
        vector: Operand,
        index: Operand,
    },
    InsertElement {
        ty: Type,
        vector: Operand,
        element: Operand,
        index: Operand,
    },
    Call {
        fn_ty: FnType,
        callee: Callee,
        args: Vec<(Type, Operand)>,
        tail: bool,
    },
    Phi {
        ty: Type,
        incoming: Vec<(Operand, BlockId)>,
    },
    /// An LLVM intrinsic or any declared function, by name, when the call is easier to spell.
    Freeze {
        ty: Type,
        value: Operand,
    },
}

impl Inst {
    /// The type of the value the instruction defines, or `Void`.
    pub fn result_type(&self) -> Type {
        match self {
            Self::Alloca { .. } | Self::Gep { .. } => Type::Ptr,
            Self::Load { ty, .. } | Self::Bin { ty, .. } | Self::Select { ty, .. } | Self::Phi { ty, .. } | Self::Freeze { ty, .. } => {
                ty.clone()
            }
            Self::Store { .. } => Type::Void,
            Self::ICmp { ty, .. } | Self::FCmp { ty, .. } => match ty {
                Type::Vector(n, _) => Type::vector(*n, Type::I1),
                _ => Type::I1,
            },
            Self::Cast { to, .. } => to.clone(),
            Self::ExtractElement { ty, .. } => match ty {
                Type::Vector(_, e) => (**e).clone(),
                other => other.clone(),
            },
            Self::InsertElement { ty, .. } => ty.clone(),
            Self::ShuffleVector { ty, mask, .. } => match ty {
                Type::Vector(_, e) => Type::Vector(mask.len() as u32, e.clone()),
                other => other.clone(),
            },
            Self::Call { fn_ty, .. } => fn_ty.ret.clone(),
        }
    }

    /// The local values the instruction reads.
    pub fn uses(&self) -> Vec<ValueId> {
        let mut out = Vec::new();
        let mut push = |o: &Operand| {
            if let Operand::Local(v) = o {
                out.push(*v);
            }
        };
        match self {
            Self::Alloca { .. } => {}
            Self::Load { ptr, .. } => push(ptr),
            Self::Store { value, ptr, .. } => {
                push(value);
                push(ptr);
            }
            Self::Bin { lhs, rhs, .. } | Self::ICmp { lhs, rhs, .. } | Self::FCmp { lhs, rhs, .. } => {
                push(lhs);
                push(rhs);
            }
            Self::Select { cond, then, els, .. } => {
                push(cond);
                push(then);
                push(els);
            }
            Self::Cast { value, .. } | Self::Freeze { value, .. } => push(value),
            Self::Gep { ptr, indices, .. } => {
                push(ptr);
                for (_, i) in indices {
                    push(i);
                }
            }
            Self::ExtractElement { vector, index, .. } => {
                push(vector);
                push(index);
            }
            Self::InsertElement { vector, element, index, .. } => {
                push(vector);
                push(element);
                push(index);
            }
            Self::ShuffleVector { a, b, .. } => {
                push(a);
                push(b);
            }
            Self::Call { callee, args, .. } => {
                if let Callee::Indirect(p) = callee {
                    push(p);
                }
                for (_, a) in args {
                    push(a);
                }
            }
            Self::Phi { incoming, .. } => {
                for (o, _) in incoming {
                    push(o);
                }
            }
        }
        out
    }

    /// Whether removing the instruction, when its value is unused, changes the program. An LLVM
    /// intrinsic the lift calls (`ctpop`, `fshl`, `sqrt` and their kin) is pure.
    pub fn has_side_effects(&self) -> bool {
        match self {
            Self::Call { callee: Callee::Direct(name), .. } => !name.starts_with("llvm."),
            Self::Store { .. } | Self::Load { volatile: true, .. } | Self::Call { .. } => true,
            _ => false,
        }
    }

    /// Every operand, for a pass that rewrites values.
    pub fn operands_mut(&mut self) -> Vec<&mut Operand> {
        match self {
            Self::Alloca { .. } => Vec::new(),
            Self::Load { ptr, .. } => vec![ptr],
            Self::Store { value, ptr, .. } => vec![value, ptr],
            Self::Bin { lhs, rhs, .. } | Self::ICmp { lhs, rhs, .. } | Self::FCmp { lhs, rhs, .. } => vec![lhs, rhs],
            Self::Select { cond, then, els, .. } => vec![cond, then, els],
            Self::Cast { value, .. } | Self::Freeze { value, .. } => vec![value],
            Self::Gep { ptr, indices, .. } => {
                let mut v = vec![ptr];
                v.extend(indices.iter_mut().map(|(_, i)| i));
                v
            }
            Self::ExtractElement { vector, index, .. } => vec![vector, index],
            Self::InsertElement { vector, element, index, .. } => vec![vector, element, index],
            Self::ShuffleVector { a, b, .. } => vec![a, b],
            Self::Call { callee, args, .. } => {
                let mut v = Vec::new();
                if let Callee::Indirect(p) = callee {
                    v.push(p);
                }
                v.extend(args.iter_mut().map(|(_, a)| a));
                v
            }
            Self::Phi { incoming, .. } => incoming.iter_mut().map(|(o, _)| o).collect(),
        }
    }
}

/// One instruction with the value it defines and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    /// The value the instruction defines; `None` for a `store` or a `void` call.
    pub result: Option<ValueId>,
    pub inst: Inst,
    /// The machine instruction it was lifted from.
    pub address: Option<u64>,
}

/// How a block ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Ret(Option<(Type, Operand)>),
    Br(BlockId),
    CondBr { cond: Operand, then: BlockId, els: BlockId },
    Switch { ty: Type, value: Operand, default: BlockId, cases: Vec<(i128, BlockId)> },
    Unreachable,
}

impl Terminator {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Self::Ret(_) | Self::Unreachable => Vec::new(),
            Self::Br(b) => vec![*b],
            Self::CondBr { then, els, .. } => vec![*then, *els],
            Self::Switch { default, cases, .. } => {
                let mut v = vec![*default];
                v.extend(cases.iter().map(|(_, b)| *b));
                v
            }
        }
    }

    pub fn uses(&self) -> Vec<ValueId> {
        match self {
            Self::Ret(Some((_, Operand::Local(v))))
            | Self::CondBr { cond: Operand::Local(v), .. }
            | Self::Switch { value: Operand::Local(v), .. } => vec![*v],
            _ => Vec::new(),
        }
    }

    pub fn operands_mut(&mut self) -> Vec<&mut Operand> {
        match self {
            Self::Ret(Some((_, o))) | Self::CondBr { cond: o, .. } | Self::Switch { value: o, .. } => vec![o],
            _ => Vec::new(),
        }
    }
}
