//! What the raise steps say they assumed, against functions whose calls are known: hand-built
//! machine functions, and the control fixture, whose disassembly says where its calls are.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use ra_asm_ir::{Callee, FnType, Function, FunctionBuilder, Module, Terminator, Type};
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Inferred, Signature, assumptions, infer_assuming};
use ra_asm_lift::raise::assume::{Assumption, Kind, Step};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// A machine function `name(ptr %state)` that calls each of `calls`, a call's address and its
/// callee, and returns.
fn machine_function(name: &str, address: u64, calls: &[(u64, &str)]) -> Function {
    let mut b = FunctionBuilder::new(name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_address(address);
    let state = b.param(0);
    let entry = b.block("entry");
    for &(site, callee) in calls {
        b.address = Some(site);
        b.call(entry, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(callee.into()), vec![(Type::Ptr, state.into())], "");
    }
    b.terminate(entry, Terminator::Ret(None));
    b.finish()
}

/// A module of `functions`, each with the signature of a function that takes and returns nothing.
fn module_of(functions: Vec<Function>) -> (Module, HashMap<String, Signature>) {
    let mut module = Module::x86_64_windows("assumptions");
    let signatures = functions.iter().map(|f| (f.name.clone(), Signature::default())).collect();
    module.functions = functions;
    (module, signatures)
}

/// The sites of the assumptions of `kind` that `function` lists, in the list's order.
fn sites(list: &[Assumption], function: &str, kind: Kind) -> Vec<Option<u64>> {
    list.iter().filter(|a| a.function == function && a.kind == kind).map(|a| a.site).collect()
}

/// The control fixture's signatures and assumptions.
fn control64() -> Inferred {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    infer_assuming(&lifted.module, &lifted.roots, Target::WIN64, &lifted.vtables, &BTreeMap::new())
}

#[test]
fn abi_lists_the_callee_saved_assumption_per_call() {
    let (module, signatures) = module_of(vec![
        machine_function("F_00001000", 0x1000, &[(0x1005, "F_00002000"), (0x100a, "F_00002000")]),
        machine_function("F_00002000", 0x2000, &[]),
    ]);
    let list = assumptions(&module, &signatures, Target::WIN64);
    assert_eq!(sites(&list, "F_00001000", Kind::CalleeSaved), [Some(0x1005), Some(0x100a)], "{list:?}");
    assert!(list.iter().all(|a| a.step == Step::Abi && a.function == "F_00001000"), "{list:?}");
}

#[test]
fn abi_lists_every_kind_at_a_call() {
    let (module, signatures) =
        module_of(vec![machine_function("F_00001000", 0x1000, &[(0x1005, "F_00002000")]), machine_function("F_00002000", 0x2000, &[])]);
    let kinds = |target: Target| -> Vec<Kind> { assumptions(&module, &signatures, target).iter().map(|a| a.kind).collect() };
    assert_eq!(kinds(Target::WIN64), [Kind::CalleeSaved, Kind::ReturnAddress, Kind::StackPointer]);
    assert_eq!(kinds(Target::X86), [Kind::CalleeSaved, Kind::ReturnAddress, Kind::StackPointer]);
    // `bl` leaves the return address in the link register: there is none on the stack to write.
    assert_eq!(kinds(Target::ARM64), [Kind::CalleeSaved, Kind::StackPointer]);
}

#[test]
fn abi_lists_nothing_for_a_leaf() {
    let (module, signatures) = module_of(vec![machine_function("F_00002000", 0x2000, &[])]);
    let list = assumptions(&module, &signatures, Target::WIN64);
    assert!(list.is_empty(), "{list:?}");
}

/// An indirect call goes through the runtime to a callee nobody sees, so it assumes what a direct
/// one does; an import is a function of the host and a callee that never returns hands nothing
/// back, so neither is assumed anything of.
#[test]
fn abi_lists_an_indirect_call_and_no_import_or_noreturn_callee() {
    let (module, mut signatures) = module_of(vec![
        machine_function("F_00001000", 0x1000, &[(0x1005, "imp_ExitProcess"), (0x100a, "F_00003000"), (0x100f, "ra_call")]),
        machine_function("F_00003000", 0x3000, &[]),
    ]);
    signatures.get_mut("F_00003000").unwrap().noreturn = true;
    let list = assumptions(&module, &signatures, Target::WIN64);
    assert_eq!(sites(&list, "F_00001000", Kind::CalleeSaved), [Some(0x100f)], "{list:?}");
    assert_eq!(list.len(), 3, "{list:?}");
}

#[test]
fn abi_list_is_in_function_and_site_order_whatever_the_module_order() {
    let caller = machine_function("F_00001000", 0x1000, &[(0x100a, "F_00002000"), (0x1005, "F_00002000")]);
    let other = machine_function("F_00002000", 0x2000, &[(0x2004, "F_00001000")]);
    let (forward, signatures) = module_of(vec![caller.clone(), other.clone()]);
    let (backward, _) = module_of(vec![other, caller]);
    let list = assumptions(&forward, &signatures, Target::WIN64);
    assert_eq!(list, assumptions(&backward, &signatures, Target::WIN64));
    let order: Vec<(&str, Option<u64>)> =
        list.iter().filter(|a| a.kind == Kind::CalleeSaved).map(|a| (a.function.as_str(), a.site)).collect();
    assert_eq!(order, [("F_00001000", Some(0x1005)), ("F_00001000", Some(0x100a)), ("F_00002000", Some(0x2004))]);
}

/// `fib` at 0x1400013f0 calls itself once, at 0x140001413 (the second recursion became its loop);
/// `collatz` at 0x1400013c0 and `op_add` at 0x140001660 call nothing.
#[test]
fn abi_assumptions_of_control64_follow_its_calls() {
    let inferred = control64();
    let fib = ra_asm_lift::function_name(0x1_4000_13f0);
    assert!(inferred.signatures.contains_key(&fib), "fib is lifted");
    for kind in [Kind::CalleeSaved, Kind::ReturnAddress, Kind::StackPointer] {
        assert_eq!(sites(&inferred.assumptions, &fib, kind), [Some(0x1_4000_1413)], "{kind:?}");
    }
    for leaf in [0x1_4000_13c0, 0x1_4000_1660] {
        let name = ra_asm_lift::function_name(leaf);
        assert!(inferred.signatures.contains_key(&name), "{name} is lifted");
        assert!(inferred.assumptions.iter().all(|a| a.function != name), "{name} is a leaf");
    }
}

#[test]
fn abi_list_is_stable_across_runs() {
    let (first, second) = (control64(), control64());
    assert!(!first.assumptions.is_empty());
    assert_eq!(first.assumptions, second.assumptions);
    assert_eq!(first.signatures, second.signatures);
}
