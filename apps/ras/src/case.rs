//! `ras case`: a function as a step of the raise is handed it, written as a pass case (the `.ll`
//! file with a header of comments that `crates/ra-asm-lift/tests/cases.rs` runs).
//!
//! The image is raised to the ABI level up to the step, which does not run. The case is the
//! function with what it references: the globals it names and the ones their initializers name,
//! and a declaration of every function it calls or takes the address of, which the runner's host
//! answers. Its `call:` lines come from the function's vectors: the image runs once through the
//! interpreter at the machine level, and each kept activation of the function gives the arguments
//! its signature reads out of the registers and the stack of the entry.
//!
//! What a case of this command does not hold:
//! - a step up to the promotion is handed a function over the machine state, which a call of a
//!   case cannot load: the first step a case is written for is the one after `promote`;
//! - an argument that is an address of the image or of the program's stack is passed as its
//!   number, where the case has nothing: the function stops at it the same way on both sides;
//! - a global of the case has no address of the image: the runner places it in its own memory.

use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_ir::{Callee, Const, Declaration, Function, Inst, Module, Operand, Type};
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::raise::liveness::Slot;
use ra_asm_lift::raise::{Target, raise_before, steps};
use ra_asm_lift::{Isa, LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::Vector;

/// The step whose rewrite takes the functions off the machine state.
const PROMOTE: &str = "promote";

/// Prints the case of `function` (a name or an address) of `image` ahead of the step `pass`.
pub(crate) fn case(image: &Image, function: &str, pass: &str) -> Result<()> {
    print!("{}", text(image, function, pass, None)?);
    Ok(())
}

/// The case of `function` of `image` ahead of the step `pass`, its calls out of the function's
/// vectors among `recorded`, or out of a run of its own without them.
pub(crate) fn text(image: &Image, function: &str, pass: &str, recorded: Option<&[Vector]>) -> Result<String> {
    let order: Vec<&str> = steps::steps(RaiseLevel::Abi).iter().map(|s| s.name).collect();
    let at = order
        .iter()
        .position(|name| *name == pass)
        .with_context(|| format!("no step `{pass}` in the raise to the abi level: its steps are {}", order.join(", ")))?;
    let promote = order.iter().position(|name| *name == PROMOTE).context("the raise to the abi level promotes")?;
    ensure!(
        at > promote,
        "no case is written ahead of `{pass}`: it is handed a function over the machine state, which a call of a case cannot load; \
         the steps a case is written for are {}",
        order[promote + 1..].join(", ")
    );

    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = raise_before(&mut lifted.module, &lifted.roots, options, Some(pass));
    lifted.apply_names(&analysis, Some(&mut signatures));

    let (address, name) = find(&lifted, function)?;
    let f =
        lifted.module.function(&name).with_context(|| format!("{name} at {address:#x} is not lifted: the port takes it from elsewhere"))?;
    let signature =
        signatures.get(&name).with_context(|| format!("{name} has no signature: `{pass}` is handed it as the lift wrote it"))?;
    let target = Target { bits: lifted.bits, isa: lifted.isa };

    let run;
    let kept: Vec<&Vector> = if let Some(vectors) = recorded {
        vectors.iter().filter(|vector| vector.address == address).collect()
    } else {
        run = vectors(image, address).with_context(|| format!("no call for {name}"))?;
        run.iter().collect()
    };
    ensure!(!kept.is_empty(), "no call for {name}: the vectors hold none of it");
    let mut calls: Vec<String> = Vec::new();
    for vector in kept {
        let call = call(f, signature, target, vector)?;
        if !calls.contains(&call) {
            calls.push(call);
        }
    }

    let module = slice(&lifted.module, f);
    let file = image.path.as_ref().and_then(|p| p.file_name()).map_or_else(|| "the image".to_owned(), |n| n.to_string_lossy().into_owned());
    let mut text = format!("; {name} of {file}, at {address:#x}, as the step `{pass}` is handed it\n; pass: {pass}\n");
    for call in &calls {
        writeln!(text, "; call: {call}").unwrap();
    }
    text.push_str(&module.to_ll());
    Ok(text)
}

/// The function `text` means: one of the lift's by its name, or by its address (see
/// [`crate::disasm::parse_address`]). The name goes first: `add` is a name before it is a number.
fn find(lifted: &Lifted, text: &str) -> Result<(u64, String)> {
    if let Some((address, name)) = lifted.functions.iter().find(|(_, name)| name.as_str() == text) {
        return Ok((*address, name.clone()));
    }
    crate::disasm::parse_address(text)
        .ok()
        .and_then(|address| lifted.functions.get(&address).map(|name| (address, name.clone())))
        .with_context(|| format!("{text} is no function of the image, by name or by address"))
}

/// The vectors of the function at `address`: `image` lifted as it is mapped, raised to the machine
/// level and run once through the interpreter under the recorder.
fn vectors(image: &Image, address: u64) -> Result<Vec<Vector>> {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: false, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Machine);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    let (recording, outcome) = ra_asm_verify::record(image, &lifted, &ra_asm_lift::run::Options::default())?;
    let kept: Vec<Vector> = recording.vectors.into_iter().filter(|vector| vector.address == address).collect();
    if kept.is_empty() {
        match outcome.stop {
            Some(stop) => bail!("the run never entered the function and has no vector of it; it stopped: {stop}"),
            None => bail!("the run never entered the function and has no vector of it"),
        }
    }
    Ok(kept)
}

/// The name a vector has for the register of `slot`; none for a machine whose vectors this
/// command does not read.
fn register(slot: Slot, target: Target) -> Option<String> {
    if target.isa != Isa::X86 {
        return None;
    }
    let name = slot.name();
    Some(if slot.is_gpr() && !target.is_64() { name.replacen('r', "e", 1) } else { name })
}

/// The bytes of the entry at `address`, as the activation first read them, as a number; a byte
/// the activation did not read is zero, since no value of it changes what the activation did.
fn read(vector: &Vector, address: u64, width: usize) -> u128 {
    let mut bytes = [0u8; 16];
    for (n, byte) in bytes.iter_mut().enumerate().take(width) {
        let at = address.wrapping_add(n as u64);
        let found = vector.memory_in.iter().find_map(|(start, run)| run.get(usize::try_from(at.checked_sub(*start)?).ok()?));
        if let Some(found) = found {
            *byte = *found;
        }
    }
    u128::from_le_bytes(bytes)
}

/// An argument of the type `ty` with the bits `bits`, as a `call:` line spells one.
fn spell(ty: &Type, bits: u128) -> Option<String> {
    Some(match ty {
        Type::Float => format!("float {:?}", f32::from_bits(bits as u32)),
        Type::Double => format!("double {:?}", f64::from_bits(bits as u64)),
        Type::Ptr => format!("ptr {:#x}", bits as u64),
        Type::Int(width) if (1..=128).contains(width) => {
            let mask = if *width == 128 { u128::MAX } else { (1u128 << *width) - 1 };
            format!("{ty} {:#x}", bits & mask)
        }
        _ => return None,
    })
}

/// The call of `f` that `vector` recorded: each parameter of the signature read where the
/// convention has it at the entry, a register or the stack above the return address, and spelled
/// at the type the parameter has by now.
fn call(f: &Function, signature: &Signature, target: Target, vector: &Vector) -> Result<String> {
    let name = &f.name;
    ensure!(!signature.variadic, "{name} reads its arguments through their address: a call of a case cannot pass the block");
    ensure!(
        f.ty.params.len() == signature.param_count(),
        "{name} takes {} parameters where its signature has {}: its calls are not read out of a vector",
        f.ty.params.len(),
        signature.param_count()
    );
    let entry = |register: &str| {
        let value = vector.entry.iter().find(|(name, _)| name == register).map(|(_, value)| *value);
        value.with_context(|| format!("the vector of {name} has no {register} at the entry"))
    };
    let mut values = Vec::with_capacity(f.ty.params.len());
    for p in 0..signature.params.len() {
        let register = target.param_slot(&signature.params, p).and_then(|slot| register(slot, target));
        let register = register.with_context(|| format!("parameter {p} of {name} is in no register a vector names"))?;
        values.push(entry(&register)?);
    }
    if !signature.stack_params.is_empty() {
        let sp = register(target.sp(), target).with_context(|| format!("the stack of {name} is under no register a vector names"))?;
        let first = (entry(&sp)? as u64).wrapping_add(target.first_stack_arg() as u64);
        for (kind, offset) in signature.stack_params.iter().zip(signature.stack_offsets(target)) {
            values.push(read(vector, first.wrapping_add(offset as u64), kind.stack_width(target.ptr()) as usize));
        }
    }
    let mut args = Vec::with_capacity(values.len());
    for (ty, bits) in f.ty.params.iter().zip(values) {
        args.push(spell(ty, bits).with_context(|| format!("a parameter of {name} is a {ty}, which a call of a case does not spell"))?);
    }
    Ok(format!("@{name}({})", args.join(", ")))
}

/// The names an operand refers to.
fn operand_names<'a>(operand: &'a Operand, names: &mut Vec<&'a str>) {
    if let Operand::Global(name) | Operand::Address { global: name, .. } = operand {
        names.push(name);
    }
}

/// The names an initializer refers to.
fn const_names<'a>(init: &'a Const, names: &mut Vec<&'a str>) {
    match init {
        Const::Global(name) | Const::Address { global: name, .. } => names.push(name),
        Const::Array(items) | Const::Struct(items) => items.iter().for_each(|item| const_names(item, names)),
        _ => {}
    }
}

/// The module of `f` alone, out of `module`: the function, the globals it reaches through its
/// operands and their initializers, and a declaration of every function it names, its own callees
/// included, so the runner's host answers them. A name `module` does not hold is left out, as it
/// is there. The globals lose their addresses of the image: the case's memory is the runner's.
fn slice(module: &Module, f: &Function) -> Module {
    let mut pending: Vec<&str> = Vec::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Inst::Call { callee: Callee::Direct(name), .. } = &i.inst {
                pending.push(name);
            }
            i.inst.operands().into_iter().for_each(|o| operand_names(o, &mut pending));
        }
        b.terminator.operands().into_iter().for_each(|o| operand_names(o, &mut pending));
    }
    let mut named: BTreeSet<&str> = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if name == f.name || !named.insert(name) {
            continue;
        }
        if let Some(init) = module.global(name).and_then(|g| g.init.as_ref()) {
            const_names(init, &mut pending);
        }
    }
    let mut out = Module {
        name: module.name.clone(),
        triple: module.triple.clone(),
        data_layout: module.data_layout.clone(),
        types: module.types.clone(),
        ..Module::default()
    };
    out.globals = module.globals.iter().filter(|g| named.contains(g.name.as_str())).cloned().collect();
    for g in &mut out.globals {
        g.address = None;
    }
    out.declarations = module.declarations.iter().filter(|d| named.contains(d.name.as_str())).cloned().collect();
    for callee in module.functions.iter().filter(|g| named.contains(g.name.as_str())) {
        out.declare(Declaration { name: callee.name.clone(), ty: callee.ty.clone(), conv: callee.conv, attrs: Vec::new() });
    }
    out.functions.push(f.clone());
    out
}
