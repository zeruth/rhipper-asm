//! The types, printed in LLVM's spelling.

use std::fmt;

/// A first-class or aggregate type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    /// `iN`.
    Int(u16),
    Half,
    Float,
    Double,
    /// The x87 extended type.
    Fp80,
    /// An opaque pointer.
    Ptr,
    /// `<N x T>`.
    Vector(u32, Box<Type>),
    /// `[N x T]`.
    Array(u64, Box<Type>),
    /// A literal struct, `{ T, U }`.
    Struct(Vec<Type>),
    /// A named struct, `%name`, defined in the module.
    Named(String),
    /// A function type, used through a pointer.
    Func(Box<FnType>),
}

impl Type {
    pub const I1: Type = Type::Int(1);
    pub const I8: Type = Type::Int(8);
    pub const I16: Type = Type::Int(16);
    pub const I32: Type = Type::Int(32);
    pub const I64: Type = Type::Int(64);
    pub const I128: Type = Type::Int(128);

    pub fn int(bits: u16) -> Self {
        Self::Int(bits)
    }

    pub fn vector(count: u32, element: Type) -> Self {
        Self::Vector(count, Box::new(element))
    }

    pub fn array(count: u64, element: Type) -> Self {
        Self::Array(count, Box::new(element))
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Self::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::Half | Self::Float | Self::Double | Self::Fp80)
    }

    /// The width in bits of an integer, float or vector type; `None` for the rest.
    pub fn bits(&self) -> Option<u32> {
        match self {
            Self::Int(n) => Some(u32::from(*n)),
            Self::Half => Some(16),
            Self::Float => Some(32),
            Self::Double => Some(64),
            Self::Fp80 => Some(80),
            Self::Vector(n, e) => e.bits().map(|b| b * n),
            _ => None,
        }
    }

    /// The size in bytes as the x86-64 data layout gives it, for the types the lift stores.
    pub fn size(&self, pointer_bytes: u64) -> Option<u64> {
        match self {
            Self::Void => Some(0),
            Self::Int(n) => Some(u64::from(n.div_ceil(8))),
            Self::Half => Some(2),
            Self::Float => Some(4),
            Self::Double => Some(8),
            Self::Fp80 => Some(16),
            Self::Ptr => Some(pointer_bytes),
            Self::Vector(n, e) => e.size(pointer_bytes).map(|s| s * u64::from(*n)),
            Self::Array(n, e) => e.size(pointer_bytes).map(|s| s * n),
            Self::Struct(fields) => fields.iter().map(|f| f.size(pointer_bytes)).sum(),
            Self::Named(_) | Self::Func(_) => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => f.write_str("void"),
            Self::Int(n) => write!(f, "i{n}"),
            Self::Half => f.write_str("half"),
            Self::Float => f.write_str("float"),
            Self::Double => f.write_str("double"),
            Self::Fp80 => f.write_str("x86_fp80"),
            Self::Ptr => f.write_str("ptr"),
            Self::Vector(n, e) => write!(f, "<{n} x {e}>"),
            Self::Array(n, e) => write!(f, "[{n} x {e}]"),
            Self::Struct(fields) => {
                f.write_str("{ ")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}")?;
                }
                f.write_str(" }")
            }
            Self::Named(name) => write!(f, "%{name}"),
            Self::Func(fn_ty) => write!(f, "{fn_ty}"),
        }
    }
}

/// A function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnType {
    pub ret: Type,
    pub params: Vec<Type>,
    pub varargs: bool,
}

impl FnType {
    pub fn new(ret: Type, params: Vec<Type>) -> Self {
        Self { ret, params, varargs: false }
    }
}

impl fmt::Display for FnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (", self.ret)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{p}")?;
        }
        if self.varargs {
            if !self.params.is_empty() {
                f.write_str(", ")?;
            }
            f.write_str("...")?;
        }
        f.write_str(")")
    }
}
