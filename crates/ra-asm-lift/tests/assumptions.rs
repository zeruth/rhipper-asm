//! What the raise steps say they assumed, against functions whose calls are known: hand-built
//! machine functions, and the control fixture, whose disassembly says where its calls are.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FnType, Function, FunctionBuilder, Inst, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Inferred, Signature, assumptions, infer_assuming};
use ra_asm_lift::raise::assume::{Assumption, Kind, Step};
use ra_asm_lift::raise::liveness::slot_pointers;
use ra_asm_lift::raise::{frame, promote, simplify, stack};

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

/// The control fixture as lifted, at the machine level.
fn control64_machine() -> Module {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    ra_asm_lift::lift(&image, &analysis).module
}

#[test]
fn promote_lists_the_private_frame_at_each_call_into_lifted_code() {
    let f = machine_function("F_00001000", 0x1000, &[(0x1005, "imp_ExitProcess"), (0x100a, "F_00003000"), (0x100f, "ra_call")]);
    let list = promote::assumptions(&f);
    assert_eq!(sites(&list, "F_00001000", Kind::PrivateLocal), [Some(0x100a), Some(0x100f)], "{list:?}");
    assert!(list.iter().all(|a| a.step == Step::Promote && a.kind == Kind::PrivateLocal), "{list:?}");
    let leaf = promote::assumptions(&machine_function("F_00003000", 0x3000, &[]));
    assert!(leaf.is_empty(), "{leaf:?}");
}

#[test]
fn promote_assumptions_of_control64_follow_its_calls() {
    let module = control64_machine();
    let fib = module.function(&ra_asm_lift::function_name(0x1_4000_13f0)).expect("fib is lifted");
    let list = promote::assumptions(fib);
    assert_eq!(sites(&list, &fib.name, Kind::PrivateLocal), [Some(0x1_4000_1413)], "{list:?}");
    for leaf in [0x1_4000_13c0, 0x1_4000_1660] {
        let f = module.function(&ra_asm_lift::function_name(leaf)).expect("the leaf is lifted");
        let list = promote::assumptions(f);
        assert!(list.is_empty(), "{list:?}");
    }
}

const RSP: i128 = 4;
const RBP: i128 = 5;

/// The address of general register `n` in the state of the machine function `b` builds.
fn register(b: &mut FunctionBuilder, block: BlockId, n: i128) -> ValueId {
    let state = b.param(0);
    let indices = vec![(Type::I64, Operand::Int(0)), (Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(n))];
    b.gep(block, Type::Named("State".into()), state, indices, "reg")
}

/// Moves register `n` by `delta`, as a push, a pop or a frame does the stack pointer.
fn adjust(b: &mut FunctionBuilder, block: BlockId, n: i128, delta: i64) {
    let p = register(b, block, n);
    let v = b.load(block, Type::I64, p, "v");
    let moved = b.bin(block, BinOp::Add, Type::I64, v, Operand::Int(i128::from(delta)), "moved");
    b.store(block, Type::I64, moved, p);
}

/// `mov to, from`.
fn copy(b: &mut FunctionBuilder, block: BlockId, from: i128, to: i128) {
    let source = register(b, block, from);
    let v = b.load(block, Type::I64, source, "v");
    let destination = register(b, block, to);
    b.store(block, Type::I64, v, destination);
}

/// A call of the machine level to `callee`, at `address`.
fn call(b: &mut FunctionBuilder, block: BlockId, callee: &str, address: u64) {
    b.address = Some(address);
    let state = b.param(0);
    b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(callee.into()), vec![(Type::Ptr, state.into())], "");
}

/// A machine function under construction, with its entry block.
fn machine_builder(name: &str, address: u64) -> (FunctionBuilder, BlockId) {
    let mut b = FunctionBuilder::new(name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_address(address);
    b.address = Some(address);
    let entry = b.block("entry");
    (b, entry)
}

/// What the stack step assumed of the machine function `f`, whose callees it knows nothing of.
fn stack_assumptions(f: &Function, target: Target) -> Vec<Assumption> {
    let state = f.params[0];
    stack::analyze(f, state, &slot_pointers(f, state), target, &|_: &Inst| None).assumptions
}

#[test]
fn stack_lists_nothing_for_a_balanced_function() {
    let (mut b, entry) = machine_builder("F_00001000", 0x1000);
    adjust(&mut b, entry, RSP, -0x28);
    adjust(&mut b, entry, RSP, 0x28);
    // The return pops its address.
    adjust(&mut b, entry, RSP, 8);
    b.terminate(entry, Terminator::Ret(None));
    let list = stack_assumptions(&b.finish(), Target::WIN64);
    assert!(list.is_empty(), "{list:?}");
}

#[test]
fn stack_lists_a_return_that_leaves_the_stack_elsewhere() {
    let (mut b, entry) = machine_builder("F_00001000", 0x1000);
    b.address = Some(0x1010);
    let balanced = b.block("balanced");
    b.address = Some(0x1020);
    let unbalanced = b.block("unbalanced");
    b.address = Some(0x1000);
    adjust(&mut b, entry, RSP, -0x28);
    b.terminate(entry, Terminator::CondBr { cond: Operand::Int(1), then: balanced, els: unbalanced });
    b.address = Some(0x1010);
    adjust(&mut b, balanced, RSP, 0x28);
    adjust(&mut b, balanced, RSP, 8);
    b.terminate(balanced, Terminator::Ret(None));
    // The frame is still there when this path returns.
    b.address = Some(0x1020);
    adjust(&mut b, unbalanced, RSP, 8);
    b.terminate(unbalanced, Terminator::Ret(None));
    let list = stack_assumptions(&b.finish(), Target::WIN64);
    assert_eq!(sites(&list, "F_00001000", Kind::BalancedStack), [Some(0x1020)], "{list:?}");
    assert!(list.iter().all(|a| a.step == Step::Stack && a.kind == Kind::BalancedStack), "{list:?}");
}

#[test]
fn stack_lists_a_block_reached_at_two_depths() {
    let (mut b, entry) = machine_builder("F_00001000", 0x1000);
    b.address = Some(0x1010);
    let pushes = b.block("pushes");
    b.address = Some(0x1020);
    let skips = b.block("skips");
    b.address = Some(0x1030);
    let join = b.block("join");
    b.terminate(entry, Terminator::CondBr { cond: Operand::Int(1), then: pushes, els: skips });
    b.address = Some(0x1010);
    adjust(&mut b, pushes, RSP, -8);
    b.terminate(pushes, Terminator::Br(join));
    b.terminate(skips, Terminator::Br(join));
    // The depth is lost at the join, so the return behind it is not seen to balance either.
    b.address = Some(0x1034);
    adjust(&mut b, join, RSP, 8);
    b.terminate(join, Terminator::Ret(None));
    let list = stack_assumptions(&b.finish(), Target::WIN64);
    assert_eq!(sites(&list, "F_00001000", Kind::BalancedStack), [Some(0x1030), Some(0x1034)], "{list:?}");
    assert_eq!(list.len(), 2, "{list:?}");
}

/// `push rbp; mov rbp, rsp; call callee; mov rsp, rbp; pop rbp; ret`, the call at 0x1004. A call
/// to lifted code pushes its return address; an import's wrapper runs in place of the call.
fn framed_caller(callee: &str) -> Function {
    let (mut b, entry) = machine_builder("F_00001000", 0x1000);
    adjust(&mut b, entry, RSP, -8);
    copy(&mut b, entry, RSP, RBP);
    b.address = Some(0x1004);
    if callee.starts_with("F_") {
        adjust(&mut b, entry, RSP, -8);
    }
    call(&mut b, entry, callee, 0x1004);
    b.address = Some(0x1009);
    copy(&mut b, entry, RBP, RSP);
    adjust(&mut b, entry, RSP, 8);
    adjust(&mut b, entry, RSP, 8);
    b.terminate(entry, Terminator::Ret(None));
    b.finish()
}

#[test]
fn stack_lists_the_frame_pointer_at_a_call_into_lifted_code() {
    let list = stack_assumptions(&framed_caller("F_00002000"), Target::WIN64);
    assert_eq!(sites(&list, "F_00001000", Kind::FramePointer), [Some(0x1004)], "{list:?}");
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0].step, Step::Stack);
    // A host function has no lifted register to write.
    let list = stack_assumptions(&framed_caller("imp_ExitProcess"), Target::WIN64);
    assert!(list.is_empty(), "{list:?}");
}

#[test]
fn stack_lists_no_frame_pointer_where_no_register_holds_the_frame() {
    let (mut b, entry) = machine_builder("F_00001000", 0x1000);
    adjust(&mut b, entry, RSP, -8);
    call(&mut b, entry, "F_00002000", 0x1004);
    adjust(&mut b, entry, RSP, 8);
    b.terminate(entry, Terminator::Ret(None));
    let list = stack_assumptions(&b.finish(), Target::WIN64);
    assert!(list.is_empty(), "{list:?}");
}

/// `fib` saves registers, calls itself and loops, and every path of it returns balanced; it keeps
/// no frame pointer. The leaves call nothing.
#[test]
fn stack_assumptions_of_control64_find_its_functions_balanced() {
    let module = control64_machine();
    for (address, leaf) in [(0x1_4000_13f0, false), (0x1_4000_13c0, true), (0x1_4000_1660, true)] {
        // The analysis reads constants, which the machine IR materializes.
        let mut f = module.function(&ra_asm_lift::function_name(address)).expect("the function is lifted").clone();
        simplify::run(&mut f);
        let list = stack_assumptions(&f, Target::WIN64);
        assert!(sites(&list, &f.name, Kind::BalancedStack).is_empty(), "{list:?}");
        assert!(!leaf || list.is_empty(), "{list:?}");
    }
}

/// A function as the ABI rewrite leaves it: a frame, its top, and `rsp0`, the top's address.
fn abi_builder() -> (FunctionBuilder, BlockId, ValueId) {
    let mut b = FunctionBuilder::new("F_00001000", FnType::new(Type::Void, vec![]), &[]);
    b.set_address(0x1000);
    let entry = b.block("entry");
    let frame = b.alloca(entry, Type::array(64, Type::I8), "frame");
    let top = b.gep(entry, Type::I8, frame, vec![(Type::I64, Operand::Int(32))], "frame.top");
    let rsp0 = b.cast(entry, Cast::PtrToInt, Type::Ptr, top, Type::I64, "rsp0");
    (b, entry, rsp0)
}

/// A store to the frame slot `offset` from the entry stack pointer, at `address`. Returns the
/// slot's address, the integer the code computes.
fn store_slot(b: &mut FunctionBuilder, block: BlockId, rsp0: ValueId, offset: i64, address: u64) -> ValueId {
    b.address = Some(address);
    let at = b.bin(block, BinOp::Add, Type::I64, rsp0, Operand::Int(i128::from(offset)), "at");
    let p = b.cast(block, Cast::IntToPtr, Type::I64, at, Type::Ptr, "p");
    b.store(block, Type::I64, Operand::Int(1), p);
    at
}

#[test]
fn frame_lists_each_slot_it_makes_a_local() {
    let (mut b, entry, rsp0) = abi_builder();
    store_slot(&mut b, entry, rsp0, -8, 0x1008);
    store_slot(&mut b, entry, rsp0, -16, 0x1004);
    b.terminate(entry, Terminator::Ret(None));
    let mut f = b.finish();
    let (changed, list) = frame::run_assuming(&mut f);
    assert!(changed);
    assert_eq!(sites(&list, "F_00001000", Kind::PrivateLocal), [Some(0x1004), Some(0x1008)], "{list:?}");
    assert!(list.iter().all(|a| a.step == Step::Frame && a.kind == Kind::PrivateLocal), "{list:?}");
}

/// The address of the slot at -16 goes to a callee: that slot and the one above it stay in the
/// frame, where the callee may reach them, and only the one below becomes a local.
#[test]
fn frame_lists_no_slot_an_escaped_address_reaches() {
    let (mut b, entry, rsp0) = abi_builder();
    let handed = store_slot(&mut b, entry, rsp0, -16, 0x1004);
    store_slot(&mut b, entry, rsp0, -8, 0x1008);
    store_slot(&mut b, entry, rsp0, -24, 0x100c);
    b.address = Some(0x1010);
    b.call(entry, FnType::new(Type::Void, vec![Type::I64]), Callee::Direct("ext".into()), vec![(Type::I64, handed.into())], "");
    b.terminate(entry, Terminator::Ret(None));
    let mut f = b.finish();
    let (_, list) = frame::run_assuming(&mut f);
    assert_eq!(sites(&list, "F_00001000", Kind::PrivateLocal), [Some(0x100c)], "{list:?}");
    assert_eq!(list.len(), 1, "{list:?}");
}

#[test]
fn frame_lists_nothing_for_a_function_without_a_frame() {
    let mut f = machine_function("F_00001000", 0x1000, &[(0x1005, "F_00002000")]);
    let (changed, list) = frame::run_assuming(&mut f);
    assert!(!changed);
    assert!(list.is_empty(), "{list:?}");
}
