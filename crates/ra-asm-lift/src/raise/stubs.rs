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
//! A function that takes more than six stack dwords (a virtual method of the C++ library with a
//! double among its arguments) reads the rest from `ra_args`, where every call through a pointer
//! of such a program writes the dwords above its six.
//!
//! The same stub is the shape of a callback the system enters: user32 calls a window procedure,
//! kernel32 a thread's start, through the trampoline at the image address with the arguments in
//! the host's four registers, which are the stub's first four universal dwords, where a `stdcall`
//! callback's stack arguments sit (`hwnd, msg, wparam, lparam` at offsets 0 to 12); the pops the
//! stub records in `ra_pops` are the callback's own (16 for a window procedure), and a native
//! caller, which pops nothing, never reads them.

use std::collections::{BTreeSet, HashMap, HashSet};

use ra_asm_ir::{BinOp, Callee, Cast, Const, FnType, FunctionBuilder, Inst, Linkage, Module, Operand, Terminator, Type};

use super::Target;
use super::abi::{Kind, KindType, Signature};
use crate::state::{ARGS, POPS, import_adapter_type};

/// The stub of the function `name`: `U_<address>` for `F_<address>`.
pub fn stub_name(name: &str) -> String {
    format!("U_{}", name.strip_prefix("F_").unwrap_or(name))
}

/// The stack dwords the universal call carries as arguments.
pub const UNIVERSAL_DWORDS: usize = 6;

/// The stack dwords past the universal six that a function a pointer may reach takes, the most
/// over `roots` (which hold every function whose address the program takes): a virtual method of
/// the C++ library with a double among six arguments, say. A call through a pointer writes that
/// many to [`ARGS`], where the stub of such a function reads its own.
pub fn extra_dwords(signatures: &HashMap<String, Signature>, roots: &BTreeSet<String>, target: Target) -> usize {
    if target.is_64() {
        return 0;
    }
    let most = roots.iter().filter_map(|r| signatures.get(r)).map(|s| stack_dwords(s, target)).max().unwrap_or(0);
    most.saturating_sub(UNIVERSAL_DWORDS)
}

/// The stack dwords a caller of `sig` writes for it. A function that reads its arguments through
/// their address has no stack parameters to count: what it pops is their size (a `thiscall` method
/// of the C++ library that takes an iterator by value and a double, say).
fn stack_dwords(sig: &Signature, target: Target) -> usize {
    let popped = if sig.variadic { sig.pops as i64 } else { 0 };
    (sig.stack_bytes(target).max(popped) / 4) as usize
}

/// Adds a universal stub for every address-taken `F_` function and points the addresses at it.
/// `extra` is what [`extra_dwords`] counted, the dwords of [`ARGS`].
pub fn add_universal_stubs(module: &mut Module, signatures: &HashMap<String, Signature>, target: Target, extra: usize) {
    let taken = address_taken(module);
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut stubs = Vec::new();
    for f in &module.functions {
        if !f.name.starts_with("F_") || !taken.contains(&f.name) {
            continue;
        }
        let Some(sig) = signatures.get(&f.name) else { continue };
        let name = stub_name(&f.name);
        stubs.push(build_stub(&name, f, sig, target, extra));
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

/// `U_x(u0..u7)`: the universal arguments are six stack dwords, then `ecx`, then `edx`; the stack
/// dwords past the six are the caller's in [`ARGS`], `extra` of them.
fn build_stub(name: &str, f: &ra_asm_ir::Function, sig: &Signature, target: Target, extra: usize) -> ra_asm_ir::Function {
    let mut b = FunctionBuilder::new(name, import_adapter_type(), &["u0", "u1", "u2", "u3", "u4", "u5", "ecx", "edx"]);
    b.set_linkage(Linkage::External);
    let block = b.block("entry");
    let params: Vec<Operand> = (0..8).map(|n| Operand::Local(b.param(n))).collect();
    // Read before the function runs: a call through a pointer inside it writes its own.
    let mut dwords: Vec<Operand> = params[..UNIVERSAL_DWORDS].to_vec();
    // A function that reads its arguments through their address may read any of them: it takes
    // every dword the caller wrote.
    let taken = if sig.variadic { UNIVERSAL_DWORDS + extra } else { stack_dwords(sig, target).min(UNIVERSAL_DWORDS + extra) };
    for j in UNIVERSAL_DWORDS..taken {
        let offset = 8 * (j - UNIVERSAL_DWORDS) as i64;
        let p = b.cast(block, Cast::IntToPtr, Type::I64, Operand::Address { global: ARGS.to_owned(), offset }, Type::Ptr, "args.p");
        dwords.push(Operand::Local(b.load(block, Type::I64, p, &format!("u{j}"))));
    }
    let dword = |n: usize| -> Operand { dwords.get(n).cloned().unwrap_or(Operand::Int(0)) };
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
        let block_p = b.value(block, Inst::Alloca { ty: Type::array(dwords.len().max(8) as u64, Type::I32), align: 16 }, "varargs");
        for n in 0..dwords.len() {
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
pub(crate) fn address_taken(module: &Module) -> HashSet<String> {
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
        out.extend(named_in(f).cloned());
    }
    out
}

/// The globals and functions the operands of `f` name: what [`address_taken`] finds in a function.
fn named_in(f: &ra_asm_ir::Function) -> impl Iterator<Item = &String> {
    f.blocks.iter().flat_map(|b| &b.instrs).flat_map(|i| i.inst.operands()).filter_map(|o| match o {
        Operand::Address { global, .. } | Operand::Global(global) => Some(global),
        _ => None,
    })
}

/// Adds to `roots` the lifted functions whose address `f` takes: what [`address_taken`] would find
/// in `f`, for a lift that freezes `f` before the module is whole.
pub(crate) fn function_addresses(f: &ra_asm_ir::Function, roots: &mut std::collections::BTreeSet<String>) {
    for name in named_in(f) {
        if name.starts_with("F_") {
            roots.insert(name.clone());
        }
    }
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
