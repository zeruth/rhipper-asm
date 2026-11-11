//! The `.ll` printer.

use std::fmt::Write;

use crate::inst::{Callee, Inst, Operand, Terminator};
use crate::module::{Const, Function, Linkage, Module};
use crate::types::Type;

pub fn print(module: &Module) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "; ModuleID = '{}'", module.name);
    let _ = writeln!(out, "source_filename = \"{}\"", module.name);
    if !module.data_layout.is_empty() {
        let _ = writeln!(out, "target datalayout = \"{}\"", module.data_layout);
    }
    if !module.triple.is_empty() {
        let _ = writeln!(out, "target triple = \"{}\"", module.triple);
    }
    if !module.types.is_empty() {
        out.push('\n');
        for (name, fields) in &module.types {
            let _ = write!(out, "%{name} = type {{ ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{f}");
            }
            out.push_str(" }\n");
        }
    }
    if !module.globals.is_empty() {
        out.push('\n');
        for g in &module.globals {
            let linkage = match g.linkage {
                Linkage::External => "",
                Linkage::Internal => "internal ",
            };
            let kind = if g.constant { "constant" } else { "global" };
            let _ = write!(out, "@{} = {linkage}{kind} {}", quote(&g.name), g.ty);
            if let Some(init) = &g.init {
                out.push(' ');
                print_const(&mut out, &g.ty, init);
            } else {
                // A declaration of a global defined elsewhere.
                let _ = write!(out, "{}", if g.linkage == Linkage::External { "" } else { " zeroinitializer" });
            }
            if g.align > 0 {
                let _ = write!(out, ", align {}", g.align);
            }
            if let Some(a) = g.address {
                let _ = write!(out, "  ; {a:#x}");
            }
            out.push('\n');
        }
    }
    if !module.declarations.is_empty() {
        out.push('\n');
        for d in &module.declarations {
            let conv = d.conv.keyword();
            let _ = write!(out, "declare {}{}{} @{}(", conv, if conv.is_empty() { "" } else { " " }, d.ty.ret, quote(&d.name));
            for (i, p) in d.ty.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{p}");
            }
            if d.ty.varargs {
                if !d.ty.params.is_empty() {
                    out.push_str(", ");
                }
                out.push_str("...");
            }
            out.push(')');
            for a in &d.attrs {
                let _ = write!(out, " {a}");
            }
            out.push('\n');
        }
    }
    for f in &module.functions {
        out.push('\n');
        print_function(&mut out, f);
    }
    out
}

fn print_function(out: &mut String, f: &Function) {
    let linkage = match f.linkage {
        Linkage::External => "",
        Linkage::Internal => "internal ",
    };
    let conv = f.conv.keyword();
    let _ = write!(out, "define {linkage}{conv}{}{} @{}(", if conv.is_empty() { "" } else { " " }, f.ty.ret, quote(&f.name));
    for (i, (ty, v)) in f.ty.params.iter().zip(&f.params).enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{ty} %{}", quote(f.value_name(*v)));
    }
    if f.ty.varargs {
        if !f.ty.params.is_empty() {
            out.push_str(", ");
        }
        out.push_str("...");
    }
    out.push(')');
    for a in &f.attrs {
        let _ = write!(out, " {a}");
    }
    if let Some(a) = f.address {
        let _ = writeln!(out, " {{  ; {a:#x}");
    } else {
        out.push_str(" {\n");
    }
    for b in &f.blocks {
        let _ = write!(out, "{}:", quote(&b.name));
        if let Some(a) = b.address {
            let _ = write!(out, "  ; {a:#x}");
        }
        out.push('\n');
        for i in &b.instrs {
            out.push_str("  ");
            if let Some(r) = i.result {
                let _ = write!(out, "%{} = ", quote(f.value_name(r)));
            }
            print_inst(out, f, &i.inst);
            if let Some(a) = i.address {
                let _ = write!(out, "  ; {a:#x}");
            }
            out.push('\n');
        }
        out.push_str("  ");
        print_terminator(out, f, &b.terminator);
        out.push('\n');
    }
    out.push_str("}\n");
}

fn operand(out: &mut String, f: &Function, ty: &Type, o: &Operand) {
    // A scalar constant on a vector type is that constant in every lane.
    if let (Type::Vector(n, element), Operand::Int(_) | Operand::Float(_)) = (ty, o) {
        if matches!(o, Operand::Int(0)) {
            out.push_str("zeroinitializer");
            return;
        }
        out.push('<');
        for i in 0..*n {
            if i > 0 {
                out.push_str(", ");
            }
            let _ = write!(out, "{element} ");
            operand(out, f, element, o);
        }
        out.push('>');
        return;
    }
    match o {
        Operand::Local(v) => {
            let _ = write!(out, "%{}", quote(f.value_name(*v)));
        }
        Operand::Int(v) => {
            if *ty == Type::I1 {
                out.push_str(if *v & 1 == 1 { "true" } else { "false" });
            } else {
                let _ = write!(out, "{}", signed_in(*v, ty));
            }
        }
        Operand::Float(v) => print_float(out, ty, *v),
        Operand::Global(name) => {
            let _ = write!(out, "@{}", quote(name));
        }
        Operand::Address { global, offset } => print_address(out, ty, global, *offset),
        Operand::Null => out.push_str("null"),
        Operand::Undef => out.push_str("undef"),
        Operand::Zero => out.push_str("zeroinitializer"),
    }
}

/// The address of a byte of a global, as a pointer or as an integer of the type asked for.
fn print_address(out: &mut String, ty: &Type, global: &str, offset: i64) {
    let pointer =
        if offset == 0 { format!("@{}", quote(global)) } else { format!("getelementptr (i8, ptr @{}, i64 {offset})", quote(global)) };
    match ty {
        Type::Ptr => out.push_str(&pointer),
        other => {
            let _ = write!(out, "ptrtoint (ptr {pointer} to {other})");
        }
    }
}

/// An integer constant as the signed value of its low `N` bits, which is the form LLVM reads for
/// every `iN`: `0xffffffff` as an `i32` is `-1`.
fn signed_in(v: i128, ty: &Type) -> i128 {
    match ty {
        Type::Int(n) if *n < 128 => {
            let shift = 128 - u32::from(*n);
            (v << shift) >> shift
        }
        _ => v,
    }
}

/// A float constant in the form LLVM reads back exactly: `double` as the 64-bit pattern, `float`
/// as the 64-bit pattern of the value widened, `half` and `x86_fp80` with their own prefixes.
fn print_float(out: &mut String, ty: &Type, v: f64) {
    match ty {
        Type::Float => {
            // A float printed as a double bit pattern must be exactly representable as a float.
            let as_float = v as f32;
            let _ = write!(out, "0x{:016X}", f64::from(as_float).to_bits());
        }
        Type::Half => {
            let _ = write!(out, "0xH{:04X}", half_bits(v as f32));
        }
        _ => {
            let _ = write!(out, "0x{:016X}", v.to_bits());
        }
    }
}

/// The IEEE half bit pattern of a float, by rounding to nearest even.
fn half_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    let new_exp = exp - 127 + 15;
    if new_exp >= 0x1f {
        return sign | 0x7c00;
    }
    if new_exp <= 0 {
        if new_exp < -10 {
            return sign;
        }
        let m = (mantissa | 0x80_0000) >> (1 - new_exp);
        let rounded = (m + 0x1000) >> 13;
        return sign | rounded as u16;
    }
    let mut half = ((new_exp as u32) << 10) | (mantissa >> 13);
    let round_bits = mantissa & 0x1fff;
    if round_bits > 0x1000 || (round_bits == 0x1000 && half & 1 == 1) {
        half += 1;
    }
    sign | half as u16
}

fn print_inst(out: &mut String, f: &Function, inst: &Inst) {
    match inst {
        Inst::Alloca { ty, align } => {
            let _ = write!(out, "alloca {ty}, align {align}");
        }
        Inst::Load { ty, ptr, align, volatile } => {
            let _ = write!(out, "load {}{ty}, ptr ", if *volatile { "volatile " } else { "" });
            operand(out, f, &Type::Ptr, ptr);
            let _ = write!(out, ", align {align}");
        }
        Inst::Store { ty, value, ptr, align, volatile } => {
            let _ = write!(out, "store {}{ty} ", if *volatile { "volatile " } else { "" });
            operand(out, f, ty, value);
            out.push_str(", ptr ");
            operand(out, f, &Type::Ptr, ptr);
            let _ = write!(out, ", align {align}");
        }
        Inst::Bin { op, ty, lhs, rhs } => {
            let _ = write!(out, "{} {ty} ", op.name());
            operand(out, f, ty, lhs);
            out.push_str(", ");
            operand(out, f, ty, rhs);
        }
        Inst::ICmp { cond, ty, lhs, rhs } => {
            let _ = write!(out, "icmp {} {ty} ", cond.name());
            operand(out, f, ty, lhs);
            out.push_str(", ");
            operand(out, f, ty, rhs);
        }
        Inst::FCmp { cond, ty, lhs, rhs } => {
            let _ = write!(out, "fcmp {} {ty} ", cond.name());
            operand(out, f, ty, lhs);
            out.push_str(", ");
            operand(out, f, ty, rhs);
        }
        Inst::Select { cond, ty, then, els } => {
            out.push_str("select i1 ");
            operand(out, f, &Type::I1, cond);
            let _ = write!(out, ", {ty} ");
            operand(out, f, ty, then);
            let _ = write!(out, ", {ty} ");
            operand(out, f, ty, els);
        }
        Inst::Cast { cast, from, value, to } => {
            let _ = write!(out, "{} {from} ", cast.name());
            operand(out, f, from, value);
            let _ = write!(out, " to {to}");
        }
        Inst::Gep { base_ty, ptr, indices, inbounds } => {
            let _ = write!(out, "getelementptr {}{base_ty}, ptr ", if *inbounds { "inbounds " } else { "" });
            operand(out, f, &Type::Ptr, ptr);
            for (ty, i) in indices {
                let _ = write!(out, ", {ty} ");
                operand(out, f, ty, i);
            }
        }
        Inst::ExtractElement { ty, vector, index } => {
            let _ = write!(out, "extractelement {ty} ");
            operand(out, f, ty, vector);
            out.push_str(", i32 ");
            operand(out, f, &Type::I32, index);
        }
        Inst::InsertElement { ty, vector, element, index } => {
            let element_ty = match ty {
                Type::Vector(_, e) => (**e).clone(),
                other => other.clone(),
            };
            let _ = write!(out, "insertelement {ty} ");
            operand(out, f, ty, vector);
            let _ = write!(out, ", {element_ty} ");
            operand(out, f, &element_ty, element);
            out.push_str(", i32 ");
            operand(out, f, &Type::I32, index);
        }
        Inst::ShuffleVector { ty, a, b, mask } => {
            let _ = write!(out, "shufflevector {ty} ");
            operand(out, f, ty, a);
            let _ = write!(out, ", {ty} ");
            operand(out, f, ty, b);
            let _ = write!(out, ", <{} x i32> <", mask.len());
            for (i, m) in mask.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if *m < 0 {
                    out.push_str("i32 poison");
                } else {
                    let _ = write!(out, "i32 {m}");
                }
            }
            out.push('>');
        }
        Inst::Call { fn_ty, callee, args, tail } => {
            if *tail {
                out.push_str("tail ");
            }
            // A varargs call must spell the whole function type.
            if fn_ty.varargs {
                let _ = write!(out, "call {fn_ty} ");
            } else {
                let _ = write!(out, "call {} ", fn_ty.ret);
            }
            match callee {
                Callee::Direct(name) => {
                    let _ = write!(out, "@{}", quote(name));
                }
                Callee::Indirect(p) => operand(out, f, &Type::Ptr, p),
            }
            out.push('(');
            for (i, (ty, a)) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{ty} ");
                operand(out, f, ty, a);
            }
            out.push(')');
        }
        Inst::Phi { ty, incoming } => {
            let _ = write!(out, "phi {ty} ");
            for (i, (o, b)) in incoming.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str("[ ");
                operand(out, f, ty, o);
                let _ = write!(out, ", %{} ]", quote(f.block(*b).map_or("?", |b| b.name.as_str())));
            }
        }
        Inst::Freeze { ty, value } => {
            let _ = write!(out, "freeze {ty} ");
            operand(out, f, ty, value);
        }
    }
}

fn print_terminator(out: &mut String, f: &Function, t: &Terminator) {
    let block_name = |b| quote(f.block(b).map_or("?", |b| b.name.as_str()));
    match t {
        Terminator::Ret(None) => out.push_str("ret void"),
        Terminator::Ret(Some((ty, o))) => {
            let _ = write!(out, "ret {ty} ");
            operand(out, f, ty, o);
        }
        Terminator::Br(b) => {
            let _ = write!(out, "br label %{}", block_name(*b));
        }
        Terminator::CondBr { cond, then, els } => {
            out.push_str("br i1 ");
            operand(out, f, &Type::I1, cond);
            let _ = write!(out, ", label %{}, label %{}", block_name(*then), block_name(*els));
        }
        Terminator::Switch { ty, value, default, cases } => {
            let _ = write!(out, "switch {ty} ");
            operand(out, f, ty, value);
            let _ = write!(out, ", label %{} [", block_name(*default));
            for (v, b) in cases {
                let _ = write!(out, " {ty} {v}, label %{}", block_name(*b));
            }
            out.push_str(" ]");
        }
        Terminator::Unreachable => out.push_str("unreachable"),
    }
}

fn print_const(out: &mut String, ty: &Type, c: &Const) {
    match c {
        Const::Int(v) => {
            let _ = write!(out, "{v}");
        }
        Const::Float(v) => print_float(out, ty, *v),
        Const::Bytes(bytes) => {
            out.push_str("c\"");
            for b in bytes {
                if *b >= 0x20 && *b < 0x7f && *b != b'"' && *b != b'\\' {
                    out.push(*b as char);
                } else {
                    let _ = write!(out, "\\{b:02X}");
                }
            }
            out.push('"');
        }
        Const::Zero => out.push_str("zeroinitializer"),
        Const::Null => out.push_str("null"),
        Const::Global(name) => {
            let _ = write!(out, "@{}", quote(name));
        }
        Const::Address { global, offset } => print_address(out, ty, global, *offset),
        Const::Array(items) => {
            let element = match ty {
                Type::Array(_, e) => (**e).clone(),
                other => other.clone(),
            };
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{element} ");
                print_const(out, &element, item);
            }
            out.push(']');
        }
        Const::Struct(items) => {
            let (fields, packed): (Vec<Type>, bool) = match ty {
                Type::Struct(fields) => (fields.clone(), false),
                Type::Packed(fields) => (fields.clone(), true),
                _ => (vec![Type::I8; items.len()], false),
            };
            out.push_str(if packed { "<{ " } else { "{ " });
            for (i, (item, field)) in items.iter().zip(&fields).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{field} ");
                print_const(out, field, item);
            }
            out.push_str(if packed { " }>" } else { " }" });
        }
    }
}

/// A name, quoted when it holds a character LLVM's bare names do not allow.
fn quote(name: &str) -> String {
    let bare = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-'))
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if bare { name.to_owned() } else { format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\22")) }
}
