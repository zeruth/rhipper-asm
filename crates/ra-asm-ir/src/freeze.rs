//! A function frozen into bytes and thawed back: what a pass over a large program keeps of the
//! functions it is not working on. The machine IR of a large image is hundreds of bytes an
//! instruction in memory (an operand is 32 bytes, a type 32, a `getelementptr`'s indices a vector
//! of their own) and a few bytes frozen, so the lift freezes each function as it makes it and a
//! pass thaws one function at a time.
//!
//! The format is the function's fields in order: integers as LEB128 (signed ones zigzagged),
//! strings as their length and bytes, each enum as a tag byte and its fields. It is a format of
//! this build alone, never written to disk: [`thaw`] reads exactly what [`freeze`] wrote.

use crate::inst::{BinOp, Callee, Cast, FCond, ICond, Inst, Instruction, Operand, Terminator};
use crate::module::{Block, BlockId, CallConv, Function, Linkage, ValueId};
use crate::types::{FnType, Type};

/// A frozen function: its name, readable without thawing, and its bytes.
#[derive(Debug, Clone)]
pub struct Frozen {
    pub name: String,
    bytes: Box<[u8]>,
}

impl Frozen {
    /// The size of the frozen bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Freezes `f`.
pub fn freeze(f: &Function) -> Frozen {
    let mut w = Writer(Vec::with_capacity(f.instr_count() * 12 + 64));
    w.function(f);
    Frozen { name: f.name.clone(), bytes: w.0.into_boxed_slice() }
}

/// Thaws a function [`freeze`] froze.
pub fn thaw(frozen: &Frozen) -> Function {
    let mut r = Reader { bytes: &frozen.bytes, at: 0 };
    let f = r.function();
    assert_eq!(r.at, r.bytes.len(), "the frozen function {} has bytes past its end", frozen.name);
    f
}

const BIN_OPS: [BinOp; 18] = [
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::UDiv,
    BinOp::SDiv,
    BinOp::URem,
    BinOp::SRem,
    BinOp::Shl,
    BinOp::LShr,
    BinOp::AShr,
    BinOp::And,
    BinOp::Or,
    BinOp::Xor,
    BinOp::FAdd,
    BinOp::FSub,
    BinOp::FMul,
    BinOp::FDiv,
    BinOp::FRem,
];
const ICONDS: [ICond; 10] =
    [ICond::Eq, ICond::Ne, ICond::Ugt, ICond::Uge, ICond::Ult, ICond::Ule, ICond::Sgt, ICond::Sge, ICond::Slt, ICond::Sle];
const FCONDS: [FCond; 14] = [
    FCond::Oeq,
    FCond::Ogt,
    FCond::Oge,
    FCond::Olt,
    FCond::Ole,
    FCond::One,
    FCond::Ord,
    FCond::Ueq,
    FCond::Ugt,
    FCond::Uge,
    FCond::Ult,
    FCond::Ule,
    FCond::Une,
    FCond::Uno,
];
const CASTS: [Cast; 12] = [
    Cast::Trunc,
    Cast::ZExt,
    Cast::SExt,
    Cast::FPTrunc,
    Cast::FPExt,
    Cast::FPToUI,
    Cast::FPToSI,
    Cast::UIToFP,
    Cast::SIToFP,
    Cast::PtrToInt,
    Cast::IntToPtr,
    Cast::Bitcast,
];
const CONVS: [CallConv; 5] = [CallConv::C, CallConv::Win64, CallConv::X86Stdcall, CallConv::X86Fastcall, CallConv::X86Thiscall];

/// The index of `x` in `table`, as a tag byte.
fn index_of<T: PartialEq>(table: &[T], x: &T) -> u8 {
    table.iter().position(|t| t == x).expect("every variant is in its table") as u8
}

struct Writer(Vec<u8>);

impl Writer {
    fn byte(&mut self, b: u8) {
        self.0.push(b);
    }

    fn u(&mut self, mut v: u128) {
        loop {
            let low = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(low);
                return;
            }
            self.0.push(low | 0x80);
        }
    }

    fn i(&mut self, v: i128) {
        self.u(((v << 1) ^ (v >> 127)) as u128);
    }

    fn bool(&mut self, b: bool) {
        self.byte(u8::from(b));
    }

    fn str(&mut self, s: &str) {
        self.u(s.len() as u128);
        self.0.extend_from_slice(s.as_bytes());
    }

    fn opt_u64(&mut self, v: Option<u64>) {
        match v {
            None => self.byte(0),
            Some(v) => {
                self.byte(1);
                self.u(u128::from(v));
            }
        }
    }

    fn ty(&mut self, t: &Type) {
        match t {
            Type::Void => self.byte(0),
            Type::Int(n) => {
                self.byte(1);
                self.u(u128::from(*n));
            }
            Type::Half => self.byte(2),
            Type::Float => self.byte(3),
            Type::Double => self.byte(4),
            Type::Fp80 => self.byte(5),
            Type::Ptr => self.byte(6),
            Type::Vector(n, e) => {
                self.byte(7);
                self.u(u128::from(*n));
                self.ty(e);
            }
            Type::Array(n, e) => {
                self.byte(8);
                self.u(u128::from(*n));
                self.ty(e);
            }
            Type::Struct(fields) | Type::Packed(fields) => {
                self.byte(if matches!(t, Type::Struct(_)) { 9 } else { 10 });
                self.u(fields.len() as u128);
                for f in fields {
                    self.ty(f);
                }
            }
            Type::Named(name) => {
                self.byte(11);
                self.str(name);
            }
            Type::Func(f) => {
                self.byte(12);
                self.fn_ty(f);
            }
        }
    }

    fn fn_ty(&mut self, f: &FnType) {
        self.ty(&f.ret);
        self.u(f.params.len() as u128);
        for p in &f.params {
            self.ty(p);
        }
        self.bool(f.varargs);
    }

    fn operand(&mut self, o: &Operand) {
        match o {
            Operand::Local(v) => {
                self.byte(0);
                self.u(u128::from(v.0));
            }
            Operand::Int(v) => {
                self.byte(1);
                self.i(*v);
            }
            Operand::Float(f) => {
                self.byte(2);
                self.0.extend_from_slice(&f.to_bits().to_le_bytes());
            }
            Operand::Global(name) => {
                self.byte(3);
                self.str(name);
            }
            Operand::Address { global, offset } => {
                self.byte(4);
                self.str(global);
                self.i(i128::from(*offset));
            }
            Operand::Null => self.byte(5),
            Operand::Undef => self.byte(6),
            Operand::Zero => self.byte(7),
        }
    }

    fn inst(&mut self, inst: &Inst) {
        match inst {
            Inst::Alloca { ty, align } => {
                self.byte(0);
                self.ty(ty);
                self.u(u128::from(*align));
            }
            Inst::Load { ty, ptr, align, volatile } => {
                self.byte(1);
                self.ty(ty);
                self.operand(ptr);
                self.u(u128::from(*align));
                self.bool(*volatile);
            }
            Inst::Store { ty, value, ptr, align, volatile } => {
                self.byte(2);
                self.ty(ty);
                self.operand(value);
                self.operand(ptr);
                self.u(u128::from(*align));
                self.bool(*volatile);
            }
            Inst::Bin { op, ty, lhs, rhs } => {
                self.byte(3);
                self.byte(index_of(&BIN_OPS, op));
                self.ty(ty);
                self.operand(lhs);
                self.operand(rhs);
            }
            Inst::ICmp { cond, ty, lhs, rhs } => {
                self.byte(4);
                self.byte(index_of(&ICONDS, cond));
                self.ty(ty);
                self.operand(lhs);
                self.operand(rhs);
            }
            Inst::FCmp { cond, ty, lhs, rhs } => {
                self.byte(5);
                self.byte(index_of(&FCONDS, cond));
                self.ty(ty);
                self.operand(lhs);
                self.operand(rhs);
            }
            Inst::ShuffleVector { ty, a, b, mask } => {
                self.byte(6);
                self.ty(ty);
                self.operand(a);
                self.operand(b);
                self.u(mask.len() as u128);
                for m in mask {
                    self.i(i128::from(*m));
                }
            }
            Inst::Select { cond, ty, then, els } => {
                self.byte(7);
                self.operand(cond);
                self.ty(ty);
                self.operand(then);
                self.operand(els);
            }
            Inst::Cast { cast, from, value, to } => {
                self.byte(8);
                self.byte(index_of(&CASTS, cast));
                self.ty(from);
                self.operand(value);
                self.ty(to);
            }
            Inst::Gep { base_ty, ptr, indices, inbounds } => {
                self.byte(9);
                self.ty(base_ty);
                self.operand(ptr);
                self.u(indices.len() as u128);
                for (t, o) in indices {
                    self.ty(t);
                    self.operand(o);
                }
                self.bool(*inbounds);
            }
            Inst::ExtractElement { ty, vector, index } => {
                self.byte(10);
                self.ty(ty);
                self.operand(vector);
                self.operand(index);
            }
            Inst::InsertElement { ty, vector, element, index } => {
                self.byte(11);
                self.ty(ty);
                self.operand(vector);
                self.operand(element);
                self.operand(index);
            }
            Inst::Call { fn_ty, callee, args, tail } => {
                self.byte(12);
                self.fn_ty(fn_ty);
                match callee {
                    Callee::Direct(name) => {
                        self.byte(0);
                        self.str(name);
                    }
                    Callee::Indirect(o) => {
                        self.byte(1);
                        self.operand(o);
                    }
                }
                self.u(args.len() as u128);
                for (t, o) in args {
                    self.ty(t);
                    self.operand(o);
                }
                self.bool(*tail);
            }
            Inst::Phi { ty, incoming } => {
                self.byte(13);
                self.ty(ty);
                self.u(incoming.len() as u128);
                for (o, b) in incoming {
                    self.operand(o);
                    self.u(u128::from(b.0));
                }
            }
            Inst::Freeze { ty, value } => {
                self.byte(14);
                self.ty(ty);
                self.operand(value);
            }
        }
    }

    fn terminator(&mut self, t: &Terminator) {
        match t {
            Terminator::Ret(None) => self.byte(0),
            Terminator::Ret(Some((ty, o))) => {
                self.byte(1);
                self.ty(ty);
                self.operand(o);
            }
            Terminator::Br(b) => {
                self.byte(2);
                self.u(u128::from(b.0));
            }
            Terminator::CondBr { cond, then, els } => {
                self.byte(3);
                self.operand(cond);
                self.u(u128::from(then.0));
                self.u(u128::from(els.0));
            }
            Terminator::Switch { ty, value, default, cases } => {
                self.byte(4);
                self.ty(ty);
                self.operand(value);
                self.u(u128::from(default.0));
                self.u(cases.len() as u128);
                for (v, b) in cases {
                    self.i(*v);
                    self.u(u128::from(b.0));
                }
            }
            Terminator::Unreachable => self.byte(5),
        }
    }

    fn function(&mut self, f: &Function) {
        self.str(&f.name);
        self.fn_ty(&f.ty);
        self.u(f.params.len() as u128);
        for p in &f.params {
            self.u(u128::from(p.0));
        }
        self.u(f.blocks.len() as u128);
        for b in &f.blocks {
            self.u(u128::from(b.id.0));
            self.str(&b.name);
            self.opt_u64(b.address);
            self.u(b.instrs.len() as u128);
            for i in &b.instrs {
                // The result as 0 for none, else the value's id plus one.
                self.u(i.result.map_or(0, |v| u128::from(v.0) + 1));
                self.inst(&i.inst);
                self.opt_u64(i.address);
            }
            self.terminator(&b.terminator);
        }
        self.bool(f.linkage == Linkage::Internal);
        self.byte(index_of(&CONVS, &f.conv));
        self.u(f.attrs.len() as u128);
        for a in &f.attrs {
            self.str(a);
        }
        self.opt_u64(f.address);
        self.u(f.value_names.len() as u128);
        for (name, ty) in f.value_names.iter().zip(&f.value_types) {
            self.str(name);
            self.ty(ty);
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> u8 {
        let b = self.bytes[self.at];
        self.at += 1;
        b
    }

    fn u(&mut self) -> u128 {
        let (mut v, mut shift) = (0u128, 0);
        loop {
            let b = self.byte();
            v |= u128::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return v;
            }
            shift += 7;
        }
    }

    fn u32(&mut self) -> u32 {
        self.u() as u32
    }

    fn i(&mut self) -> i128 {
        let z = self.u();
        ((z >> 1) as i128) ^ -((z & 1) as i128)
    }

    fn bool(&mut self) -> bool {
        self.byte() != 0
    }

    fn str(&mut self) -> String {
        let n = self.u() as usize;
        let s = std::str::from_utf8(&self.bytes[self.at..self.at + n]).expect("a frozen string is UTF-8").to_owned();
        self.at += n;
        s
    }

    fn opt_u64(&mut self) -> Option<u64> {
        (self.byte() != 0).then(|| self.u() as u64)
    }

    fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> T) -> Vec<T> {
        let n = self.u() as usize;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(item(self));
        }
        out
    }

    fn ty(&mut self) -> Type {
        match self.byte() {
            0 => Type::Void,
            1 => Type::Int(self.u() as u16),
            2 => Type::Half,
            3 => Type::Float,
            4 => Type::Double,
            5 => Type::Fp80,
            6 => Type::Ptr,
            7 => {
                let n = self.u32();
                Type::Vector(n, Box::new(self.ty()))
            }
            8 => {
                let n = self.u() as u64;
                Type::Array(n, Box::new(self.ty()))
            }
            9 => Type::Struct(self.list(Self::ty)),
            10 => Type::Packed(self.list(Self::ty)),
            11 => Type::Named(self.str()),
            12 => Type::Func(Box::new(self.fn_ty())),
            tag => panic!("a frozen type has the tag {tag}"),
        }
    }

    fn fn_ty(&mut self) -> FnType {
        let ret = self.ty();
        let params = self.list(Self::ty);
        FnType { ret, params, varargs: self.bool() }
    }

    fn operand(&mut self) -> Operand {
        match self.byte() {
            0 => Operand::Local(ValueId(self.u32())),
            1 => Operand::Int(self.i()),
            2 => {
                let bits: [u8; 8] = self.bytes[self.at..self.at + 8].try_into().expect("eight bytes");
                self.at += 8;
                Operand::Float(f64::from_bits(u64::from_le_bytes(bits)))
            }
            3 => Operand::Global(self.str()),
            4 => {
                let global = self.str();
                Operand::Address { global, offset: self.i() as i64 }
            }
            5 => Operand::Null,
            6 => Operand::Undef,
            7 => Operand::Zero,
            tag => panic!("a frozen operand has the tag {tag}"),
        }
    }

    fn inst(&mut self) -> Inst {
        match self.byte() {
            0 => {
                let ty = self.ty();
                Inst::Alloca { ty, align: self.u32() }
            }
            1 => {
                let (ty, ptr) = (self.ty(), self.operand());
                let align = self.u32();
                Inst::Load { ty, ptr, align, volatile: self.bool() }
            }
            2 => {
                let (ty, value, ptr) = (self.ty(), self.operand(), self.operand());
                let align = self.u32();
                Inst::Store { ty, value, ptr, align, volatile: self.bool() }
            }
            3 => {
                let op = BIN_OPS[self.byte() as usize];
                let ty = self.ty();
                let (lhs, rhs) = (self.operand(), self.operand());
                Inst::Bin { op, ty, lhs, rhs }
            }
            4 => {
                let cond = ICONDS[self.byte() as usize];
                let ty = self.ty();
                let (lhs, rhs) = (self.operand(), self.operand());
                Inst::ICmp { cond, ty, lhs, rhs }
            }
            5 => {
                let cond = FCONDS[self.byte() as usize];
                let ty = self.ty();
                let (lhs, rhs) = (self.operand(), self.operand());
                Inst::FCmp { cond, ty, lhs, rhs }
            }
            6 => {
                let ty = self.ty();
                let (a, b) = (self.operand(), self.operand());
                let mask = self.list(|r| r.i() as i32);
                Inst::ShuffleVector { ty, a, b, mask }
            }
            7 => {
                let cond = self.operand();
                let ty = self.ty();
                let (then, els) = (self.operand(), self.operand());
                Inst::Select { cond, ty, then, els }
            }
            8 => {
                let cast = CASTS[self.byte() as usize];
                let (from, value, to) = (self.ty(), self.operand(), self.ty());
                Inst::Cast { cast, from, value, to }
            }
            9 => {
                let (base_ty, ptr) = (self.ty(), self.operand());
                let indices = self.list(|r| (r.ty(), r.operand()));
                Inst::Gep { base_ty, ptr, indices, inbounds: self.bool() }
            }
            10 => {
                let ty = self.ty();
                let (vector, index) = (self.operand(), self.operand());
                Inst::ExtractElement { ty, vector, index }
            }
            11 => {
                let ty = self.ty();
                let (vector, element, index) = (self.operand(), self.operand(), self.operand());
                Inst::InsertElement { ty, vector, element, index }
            }
            12 => {
                let fn_ty = self.fn_ty();
                let callee = match self.byte() {
                    0 => Callee::Direct(self.str()),
                    _ => Callee::Indirect(self.operand()),
                };
                let args = self.list(|r| (r.ty(), r.operand()));
                Inst::Call { fn_ty, callee, args, tail: self.bool() }
            }
            13 => {
                let ty = self.ty();
                let incoming = self.list(|r| (r.operand(), BlockId(r.u32())));
                Inst::Phi { ty, incoming }
            }
            14 => {
                let ty = self.ty();
                Inst::Freeze { ty, value: self.operand() }
            }
            tag => panic!("a frozen instruction has the tag {tag}"),
        }
    }

    fn terminator(&mut self) -> Terminator {
        match self.byte() {
            0 => Terminator::Ret(None),
            1 => {
                let ty = self.ty();
                Terminator::Ret(Some((ty, self.operand())))
            }
            2 => Terminator::Br(BlockId(self.u32())),
            3 => {
                let cond = self.operand();
                let then = BlockId(self.u32());
                Terminator::CondBr { cond, then, els: BlockId(self.u32()) }
            }
            4 => {
                let (ty, value) = (self.ty(), self.operand());
                let default = BlockId(self.u32());
                let cases = self.list(|r| (r.i(), BlockId(r.u32())));
                Terminator::Switch { ty, value, default, cases }
            }
            5 => Terminator::Unreachable,
            tag => panic!("a frozen terminator has the tag {tag}"),
        }
    }

    fn function(&mut self) -> Function {
        let name = self.str();
        let ty = self.fn_ty();
        let params = self.list(|r| ValueId(r.u32()));
        let blocks = self.list(|r| {
            let id = BlockId(r.u32());
            let name = r.str();
            let address = r.opt_u64();
            let instrs = r.list(|r| {
                let result = match r.u32() {
                    0 => None,
                    v => Some(ValueId(v - 1)),
                };
                let inst = r.inst();
                Instruction { result, inst, address: r.opt_u64() }
            });
            Block { id, name, instrs, terminator: r.terminator(), address }
        });
        let linkage = if self.bool() { Linkage::Internal } else { Linkage::External };
        let conv = CONVS[self.byte() as usize];
        let attrs = self.list(Self::str);
        let address = self.opt_u64();
        let n = self.u() as usize;
        let (mut value_names, mut value_types) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for _ in 0..n {
            value_names.push(self.str());
            value_types.push(self.ty());
        }
        Function { name, ty, params, blocks, linkage, conv, attrs, address, value_names, value_types }
    }
}

#[cfg(test)]
mod tests {
    use super::{freeze, thaw};
    use crate::inst::{BinOp, Callee, Cast, Inst, Operand};
    use crate::types::{FnType, Type};
    use crate::{FunctionBuilder, Terminator};

    #[test]
    fn a_function_thaws_as_it_froze() {
        let mut b = FunctionBuilder::new("f", FnType::new(Type::I64, vec![Type::Ptr, Type::I32]), &["p", "n"]);
        let entry = b.block("entry");
        b.set_block_address(entry, 0x40_1000);
        b.set_address(0x40_1000);
        let (p, n) = (b.param(0), b.param(1));
        let wide = b.cast(entry, Cast::ZExt, Type::I32, n, Type::I64, "wide");
        let sum = b.bin(entry, BinOp::Add, Type::I64, wide, i128::from(i64::MIN) - 3, "sum");
        let big = b.bin(entry, BinOp::Or, Type::I128, Operand::Int(-(1i128 << 100)), 0i64, "big");
        let field = b.gep(entry, Type::Named("S".into()), p, vec![(Type::I32, 0i64.into()), (Type::I32, 2i64.into())], "field");
        b.store(entry, Type::I64, sum, field);
        let call = b.value(
            entry,
            Inst::Call {
                fn_ty: FnType::new(Type::Double, vec![Type::Double]),
                callee: Callee::Direct("sqrt".into()),
                args: vec![(Type::Double, Operand::Float(-0.5))],
                tail: false,
            },
            "root",
        );
        let _ = (big, call);
        b.terminate(entry, Terminator::Ret(Some((Type::I64, Operand::Address { global: "sec_data".into(), offset: -8 }))));
        let f = b.finish();
        let frozen = freeze(&f);
        assert!(frozen.len() < 200, "{} bytes", frozen.len());
        assert_eq!(thaw(&frozen), f);
    }
}
