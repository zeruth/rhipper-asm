//! Universal entry points for the x86 functions a pointer reaches.
//!
//! A call through a pointer cannot know its callee's convention: a `cdecl` callee reads its
//! arguments from the stack, a `thiscall` one its first from `ecx`, and a `stdcall` one pops them.
//! On Win64 the universal call (four registers, then stack slots) is the convention itself, and a
//! callee reads what it needs. On x86 the raise gives every address-taken function a stub with the
//! universal signature, six stack dwords and then `ecx` and `edx`, that calls the function with
//! the arguments its signature places, records in `ra_pops` what the function pops beyond its
//! return address, and returns the result. The data and the function table point at the stub, so
//! a pointer the program holds reaches it; a direct call still goes to the function. With the
//! stubs holding the addresses, the functions themselves are free for the width pass to narrow.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{BinOp, Callee, Cast, Const, FnType, FunctionBuilder, Inst, Linkage, Module, Operand, Terminator, Type};

use super::Target;
use super::abi::{Kind, Signature};
use crate::state::{POPS, import_adapter_type};

/// The stub of the function `name`: `U_<address>` for `F_<address>`.
pub fn stub_name(name: &str) -> String {
    format!("U_{}", name.strip_prefix("F_").unwrap_or(name))
}

/// Adds a universal stub for every address-taken `F_` function and points the addresses at it.
pub fn add_universal_stubs(module: &mut Module, signatures: &HashMap<String, Signature>, target: Target) {
    let taken = address_taken(module);
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut stubs = Vec::new();
    for f in &module.functions {
        if !f.name.starts_with("F_") || !taken.contains(&f.name) {
            continue;
        }
        let Some(sig) = signatures.get(&f.name) else { continue };
        let name = stub_name(&f.name);
        stubs.push(build_stub(&name, f, sig, target));
        renamed.insert(f.name.clone(), name);
    }
    if renamed.is_empty() {
        return;
    }
    for g in &mut module.globals {
        if let Some(init) = &mut g.init {
            rename_const(init, &renamed);
        }
    }
    for f in &mut module.functions {
        for b in &mut f.blocks {
            for i in &mut b.instrs {
                for o in i.inst.operands_mut() {
                    rename_operand(o, &renamed);
                }
            }
            for o in b.terminator.operands_mut() {
                rename_operand(o, &renamed);
            }
        }
    }
    module.functions.extend(stubs);
}

/// `U_x(u0..u7)`: the universal arguments are six stack dwords, then `ecx`, then `edx`.
fn build_stub(name: &str, f: &ra_asm_ir::Function, sig: &Signature, target: Target) -> ra_asm_ir::Function {
    let mut b = FunctionBuilder::new(name, import_adapter_type(), &["u0", "u1", "u2", "u3", "u4", "u5", "ecx", "edx"]);
    b.set_linkage(Linkage::External);
    let block = b.block("entry");
    let params: Vec<Operand> = (0..8).map(|n| Operand::Local(b.param(n))).collect();
    let dword = |n: usize| -> Operand { params.get(n).filter(|_| n < 6).cloned().unwrap_or(Operand::Int(0)) };
    let mut args: Vec<(Type, Operand)> = Vec::new();
    for (p, kind) in sig.params.iter().enumerate() {
        // An integer in ecx or edx comes as the last two universal arguments; a float in a
        // vector register has no universal slot, and a pointer never names such a function.
        let value = match (kind, target.param_slot(&sig.params, p)) {
            (Kind::Int | Kind::Wide, Some(slot)) if slot == super::liveness::RCX => params[6].clone(),
            (Kind::Int | Kind::Wide, Some(slot)) if slot == super::liveness::RDX => params[7].clone(),
            _ => Operand::Undef,
        };
        args.push((kind.ty(), value));
    }
    for (kind, offset) in sig.stack_params.iter().zip(sig.stack_offsets(target)) {
        let j = (offset / 4) as usize;
        let value = match kind {
            Kind::Wide => {
                let high = b.bin(block, BinOp::Shl, Type::I64, dword(j + 1), 32i64, "hi");
                Operand::Local(b.bin(block, BinOp::Or, Type::I64, dword(j), high, "wide"))
            }
            _ => dword(j),
        };
        args.push((kind.ty(), value));
    }
    // A variadic function reads its arguments through a pointer: the universal dwords go into a
    // block of the stub's own, and the block's address is the argument.
    if sig.variadic {
        let block_p = b.value(block, Inst::Alloca { ty: Type::array(8, Type::I32), align: 16 }, "varargs");
        for n in 0..6 {
            let slot = b.gep(block, Type::I32, block_p, vec![(Type::I64, Operand::Int(n as i128))], "slot");
            let low = b.cast(block, Cast::Trunc, Type::I64, dword(n), Type::I32, "lo");
            b.store(block, Type::I32, low, slot);
        }
        let address = b.cast(block, Cast::PtrToInt, Type::Ptr, block_p, Type::I64, "varargs.i");
        args.push((Type::I64, Operand::Local(address)));
    }
    // The function's own parameter types, as the call must give them.
    for (arg, ty) in args.iter_mut().zip(&f.ty.params) {
        if arg.0 != *ty {
            let v = b.cast(
                block,
                if ty.bits() < arg.0.bits() { Cast::Trunc } else { Cast::ZExt },
                arg.0.clone(),
                arg.1.clone(),
                ty.clone(),
                "a",
            );
            *arg = (ty.clone(), Operand::Local(v));
        }
    }
    let result = b.call(block, f.ty.clone(), Callee::Direct(f.name.clone()), args, "ret");
    let pops_ptr = b.cast(block, Cast::IntToPtr, Type::I64, Operand::Address { global: POPS.to_owned(), offset: 0 }, Type::Ptr, "pops.p");
    b.store(block, Type::I64, sig.pops as i64, pops_ptr);
    if f.attrs.iter().any(|a| a == "noreturn") {
        b.terminate(block, Terminator::Unreachable);
        return b.finish();
    }
    let returned: Operand = match (result, f.ty.ret.clone()) {
        (Some(v), Type::I64) => Operand::Local(v),
        (Some(v), Type::Int(bits)) => Operand::Local(b.cast(block, Cast::ZExt, Type::Int(bits), v, Type::I64, "ret.w")),
        _ => Operand::Int(0),
    };
    b.terminate(block, Terminator::Ret(Some((Type::I64, returned))));
    b.finish()
}

/// The functions whose address the module takes: in the data (the function table included, which
/// the runtime resolves pointers through) or as an operand.
fn address_taken(module: &Module) -> HashSet<String> {
    fn consts(c: &Const, out: &mut HashSet<String>) {
        match c {
            Const::Address { global, .. } | Const::Global(global) => {
                out.insert(global.clone());
            }
            Const::Array(items) | Const::Struct(items) => items.iter().for_each(|i| consts(i, out)),
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for g in &module.globals {
        if let Some(init) = &g.init {
            consts(init, &mut out);
        }
    }
    for f in &module.functions {
        for b in &f.blocks {
            for i in &b.instrs {
                let mut inst = i.inst.clone();
                for o in inst.operands_mut() {
                    match o {
                        Operand::Address { global, .. } | Operand::Global(global) => {
                            out.insert(global.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn rename_const(c: &mut Const, renamed: &HashMap<String, String>) {
    match c {
        Const::Address { global, .. } | Const::Global(global) => {
            if let Some(new) = renamed.get(global) {
                *global = new.clone();
            }
        }
        Const::Array(items) | Const::Struct(items) => items.iter_mut().for_each(|i| rename_const(i, renamed)),
        _ => {}
    }
}

pub(crate) fn rename_operand(o: &mut Operand, renamed: &HashMap<String, String>) {
    if let Operand::Address { global, .. } | Operand::Global(global) = o
        && let Some(new) = renamed.get(global)
    {
        *global = new.clone();
    }
}

#[allow(dead_code)]
fn _uses_fn_type(_: FnType) {}
