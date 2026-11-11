//! The values of the interpreter, one per first-class type, and their bytes in memory.

use std::collections::HashMap;

use crate::types::{Type, layout};

/// A value of the IR. An integer is masked to its type's width (an `i1` is 0 or 1) and a pointer
/// is an integer of the host's address width; a vector is its lanes; an aggregate (an array or a
/// struct, which only a constant builds) is its elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u128),
    F32(f32),
    F64(f64),
    Vector(Vec<Value>),
    Aggregate(Vec<Value>),
}

/// The ones of `bits` bits.
pub fn mask(bits: u32) -> u128 {
    if bits >= 128 { u128::MAX } else { (1u128 << bits) - 1 }
}

/// `v` of `bits` bits, sign-extended.
pub fn sext(v: u128, bits: u32) -> i128 {
    if bits >= 128 {
        return v as i128;
    }
    let shift = 128 - bits;
    ((v << shift) as i128) >> shift
}

impl Value {
    pub const FALSE: Value = Value::Int(0);
    pub const TRUE: Value = Value::Int(1);

    /// An integer of `bits` bits.
    pub fn int(v: u128, bits: u32) -> Self {
        Self::Int(v & mask(bits))
    }

    pub fn bool(b: bool) -> Self {
        Self::Int(u128::from(b))
    }

    /// The zero of a type: `zeroinitializer`, and what an `undef` reads as.
    pub fn zero(ty: &Type, layout: &Layout<'_>) -> Self {
        match ty {
            Type::Float => Self::F32(0.0),
            Type::Double => Self::F64(0.0),
            Type::Vector(n, e) => Self::Vector(vec![Self::zero(e, layout); *n as usize]),
            Type::Array(n, e) => Self::Aggregate(vec![Self::zero(e, layout); *n as usize]),
            Type::Struct(fields) | Type::Packed(fields) => Self::Aggregate(fields.iter().map(|f| Self::zero(f, layout)).collect()),
            Type::Named(name) => match layout.named.get(name) {
                Some(fields) => Self::Aggregate(fields.iter().map(|f| Self::zero(f, layout)).collect()),
                None => Self::Int(0),
            },
            _ => Self::Int(0),
        }
    }

    pub fn as_u128(&self) -> u128 {
        match self {
            Self::Int(v) => *v,
            Self::F32(f) => u128::from(f.to_bits()),
            Self::F64(f) => u128::from(f.to_bits()),
            Self::Vector(lanes) | Self::Aggregate(lanes) => lanes.first().map_or(0, Self::as_u128),
        }
    }

    pub fn as_u64(&self) -> u64 {
        self.as_u128() as u64
    }

    pub fn as_bool(&self) -> bool {
        self.as_u128() & 1 != 0
    }

    pub fn as_f64(&self) -> f64 {
        match self {
            Self::F64(f) => *f,
            Self::F32(f) => f64::from(*f),
            Self::Int(v) => f64::from_bits(*v as u64),
            Self::Vector(lanes) | Self::Aggregate(lanes) => lanes.first().map_or(0.0, Self::as_f64),
        }
    }

    pub fn as_f32(&self) -> f32 {
        match self {
            Self::F32(f) => *f,
            Self::F64(f) => *f as f32,
            Self::Int(v) => f32::from_bits(*v as u32),
            Self::Vector(lanes) | Self::Aggregate(lanes) => lanes.first().map_or(0.0, Self::as_f32),
        }
    }

    /// The lanes of a vector, or the value alone.
    pub fn lanes(&self) -> &[Value] {
        match self {
            Self::Vector(lanes) => lanes,
            _ => std::slice::from_ref(self),
        }
    }

    /// The value as `ty` expects it: an integer masked to the width, a float of the right
    /// precision, the bits reinterpreted when an integer meets a float type (a universal call
    /// hands an integer where the callee takes a double).
    #[must_use]
    pub fn coerce(self, ty: &Type, layout: &Layout<'_>) -> Self {
        match (ty, self) {
            (Type::Int(_) | Type::Ptr, v) => Self::int(v.as_u128(), layout.bits(ty)),
            (Type::Double, v) => Self::F64(v.as_f64()),
            (Type::Float, Self::F64(f)) => Self::F32(f as f32),
            (Type::Float, v) => Self::F32(v.as_f32()),
            (Type::Vector(n, e), Self::Vector(lanes)) if lanes.len() == *n as usize => {
                Self::Vector(lanes.into_iter().map(|l| l.coerce(e, layout)).collect())
            }
            (Type::Void, _) => Self::Int(0),
            (_, v) => v,
        }
    }

    /// The bytes of the value as `ty` lays it out in memory, little-endian.
    pub fn to_bytes(&self, ty: &Type, layout: &Layout<'_>) -> Vec<u8> {
        let size = layout.size(ty) as usize;
        let mut out = vec![0u8; size];
        self.write_bytes(ty, layout, &mut out);
        out
    }

    fn write_bytes(&self, ty: &Type, layout: &Layout<'_>, out: &mut [u8]) {
        match (ty, self) {
            (Type::Vector(n, e), Self::Vector(lanes)) if **e == Type::I1 => {
                // A vector of bits is packed, as LLVM stores it.
                for (i, lane) in lanes.iter().take(*n as usize).enumerate() {
                    if lane.as_bool() {
                        out[i / 8] |= 1 << (i % 8);
                    }
                }
            }
            (Type::Vector(_, e) | Type::Array(_, e), Self::Vector(lanes) | Self::Aggregate(lanes)) => {
                let stride = layout.size(e) as usize;
                for (i, lane) in lanes.iter().enumerate() {
                    let at = i * stride;
                    if at + stride <= out.len() {
                        lane.write_bytes(e, layout, &mut out[at..at + stride]);
                    }
                }
            }
            (Type::Struct(_) | Type::Packed(_) | Type::Named(_), Self::Aggregate(items)) => {
                let fields = layout.fields(ty);
                let offsets = layout.offsets(ty);
                for ((item, field), at) in items.iter().zip(&fields).zip(offsets) {
                    let size = layout.size(field) as usize;
                    let at = at as usize;
                    if at + size <= out.len() {
                        item.write_bytes(field, layout, &mut out[at..at + size]);
                    }
                }
            }
            (Type::Float, v) => out[..4].copy_from_slice(&v.as_f32().to_bits().to_le_bytes()),
            (Type::Double, v) => out[..8].copy_from_slice(&v.as_f64().to_bits().to_le_bytes()),
            (_, v) => {
                let bytes = v.as_u128().to_le_bytes();
                let n = out.len().min(16);
                out[..n].copy_from_slice(&bytes[..n]);
            }
        }
    }

    /// The value of `ty` the bytes hold.
    pub fn from_bytes(ty: &Type, bytes: &[u8], layout: &Layout<'_>) -> Self {
        let int = |bytes: &[u8]| {
            let mut buf = [0u8; 16];
            let n = bytes.len().min(16);
            buf[..n].copy_from_slice(&bytes[..n]);
            u128::from_le_bytes(buf)
        };
        match ty {
            Type::Float => Self::F32(f32::from_bits(int(bytes) as u32)),
            Type::Double => Self::F64(f64::from_bits(int(bytes) as u64)),
            Type::Vector(n, e) if **e == Type::I1 => {
                Self::Vector((0..*n as usize).map(|i| Self::bool(bytes.get(i / 8).is_some_and(|b| b >> (i % 8) & 1 != 0))).collect())
            }
            Type::Vector(n, e) => {
                let stride = layout.size(e) as usize;
                Self::Vector((0..*n as usize).map(|i| Self::from_bytes(e, &bytes[i * stride..(i + 1) * stride], layout)).collect())
            }
            Type::Array(n, e) => {
                let stride = layout.size(e) as usize;
                Self::Aggregate((0..*n as usize).map(|i| Self::from_bytes(e, &bytes[i * stride..(i + 1) * stride], layout)).collect())
            }
            Type::Struct(_) | Type::Packed(_) | Type::Named(_) => {
                let fields = layout.fields(ty);
                let offsets = layout.offsets(ty);
                Self::Aggregate(
                    fields
                        .iter()
                        .zip(offsets)
                        .map(|(f, at)| {
                            let at = at as usize;
                            Self::from_bytes(f, &bytes[at..at + layout.size(f) as usize], layout)
                        })
                        .collect(),
                )
            }
            _ => Self::int(int(bytes), layout.bits(ty)),
        }
    }
}

/// The sizes and offsets of the types, with the module's named structs. Pointers are the host's,
/// eight bytes: a lifted 32-bit program keeps its own pointers as `i32` values.
pub struct Layout<'a> {
    pub named: &'a HashMap<String, Vec<Type>>,
}

impl Layout<'_> {
    pub const POINTER_BYTES: u64 = 8;

    /// The width in bits of an integer, a float or a pointer; a vector's lane count times its
    /// lane's width.
    pub fn bits(&self, ty: &Type) -> u32 {
        match ty {
            Type::Ptr => 64,
            Type::Named(_) | Type::Struct(_) | Type::Packed(_) | Type::Array(..) | Type::Func(_) | Type::Void => (self.size(ty) * 8) as u32,
            _ => ty.bits().unwrap_or(64),
        }
    }

    /// The bytes the type takes in memory, padding included.
    pub fn size(&self, ty: &Type) -> u64 {
        match ty {
            Type::Vector(n, e) if **e == Type::I1 => u64::from(*n).div_ceil(8),
            Type::Vector(n, e) => self.size(e) * u64::from(*n),
            Type::Array(n, e) => self.size(e) * n,
            Type::Struct(fields) => layout(fields, Self::POINTER_BYTES).map_or(0, |(_, size)| size),
            Type::Packed(fields) => fields.iter().map(|f| self.size(f)).sum(),
            Type::Named(name) => self.named.get(name).and_then(|fields| layout(fields, Self::POINTER_BYTES)).map_or(0, |(_, size)| size),
            Type::Func(_) => Self::POINTER_BYTES,
            _ => ty.size(Self::POINTER_BYTES).unwrap_or(0),
        }
    }

    pub fn align(&self, ty: &Type) -> u64 {
        match ty {
            Type::Named(name) => {
                self.named.get(name).map_or(1, |fields| Type::Struct(fields.clone()).align(Self::POINTER_BYTES).unwrap_or(1))
            }
            Type::Func(_) => Self::POINTER_BYTES,
            _ => ty.align(Self::POINTER_BYTES).unwrap_or(1),
        }
    }

    /// The fields of a struct type, named or literal; empty for any other.
    pub fn fields(&self, ty: &Type) -> Vec<Type> {
        match ty {
            Type::Struct(fields) | Type::Packed(fields) => fields.clone(),
            Type::Named(name) => self.named.get(name).cloned().unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The byte offset of each field of a struct type.
    pub fn offsets(&self, ty: &Type) -> Vec<u64> {
        match ty {
            Type::Packed(fields) => {
                let mut at = 0;
                fields
                    .iter()
                    .map(|f| {
                        let here = at;
                        at += self.size(f);
                        here
                    })
                    .collect()
            }
            _ => layout(&self.fields(ty), Self::POINTER_BYTES).map(|(offsets, _)| offsets).unwrap_or_default(),
        }
    }
}
