//! What the raise steps say they assumed, against functions whose calls are known: hand-built
//! machine functions, and the control fixture, whose disassembly says where its calls are.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FnType, Function, FunctionBuilder, Inst, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Inferred, Signature, assumptions, infer_assuming};
use ra_asm_lift::raise::assume::{self, Assumption, Kind, Step};
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

/// `classify` at 0x140001430 is the control fixture's dense switch: `cmp ecx, 8; ja` at
/// 0x140001434 bounds the index of the `jmp rax` at 0x14000144d.
const CLASSIFY: u64 = 0x1_4000_1430;
const CLASSIFY_COMPARE: u64 = 0x1_4000_1434;
const CLASSIFY_JUMP: u64 = 0x1_4000_144d;

#[test]
fn tables_with_a_bound_check_are_listed_discharged() {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let list = assume::tables(analysis.functions.values());
    let classify = ra_asm_lift::function_name(CLASSIFY);
    let of_classify: Vec<&Assumption> = list.iter().filter(|a| a.function == classify).collect();
    let [table] = of_classify.as_slice() else { panic!("classify has one table: {list:?}") };
    assert_eq!((table.step, table.kind, table.site), (Step::JumpTable, Kind::TableIndex, Some(CLASSIFY_JUMP)));
    assert_eq!(table.discharged, Some(CLASSIFY_COMPARE));
    assert!(table.is_discharged());
    assert!(list.iter().all(|a| a.step == Step::JumpTable && a.kind == Kind::TableIndex), "{list:?}");
    assert_eq!(list, assume::tables(analysis.functions.values().rev()), "the order is the list's own");
}

/// The same table as the analysis leaves one whose compare it did not find: read while its
/// entries point at code, with the index assumed inside.
#[test]
fn tables_without_a_bound_check_stay_assumed() {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut classify = analysis.functions[&CLASSIFY].clone();
    for block in &mut classify.blocks {
        if let ra_asm_analysis::Terminator::Switch { bound, .. } = &mut block.terminator {
            *bound = None;
        }
    }
    let list = assume::tables([&classify]);
    let [table] = list.as_slice() else { panic!("classify has one table: {list:?}") };
    assert_eq!((table.kind, table.site, table.discharged), (Kind::TableIndex, Some(CLASSIFY_JUMP), None));
    assert!(!table.is_discharged());
}

/// Neither program has a jump table: the `jmp rax` of `dispatch` goes to a handler its table of
/// pointers holds and the `jmp [rax+8]` of `describe_named` through a vtable slot, and neither
/// resolves as one.
#[test]
fn tables_of_dispatch64_and_classes64_are_none() {
    for binary in ["dispatch64.exe", "classes64.exe"] {
        let image = ra_asm_pe::load(&fixture(binary)).unwrap();
        let analysis = ra_asm_analysis::analyze(&image);
        let list = assume::tables(analysis.functions.values());
        assert!(list.is_empty(), "{binary}: {list:?}");
    }
}

/// What devirtualization assumes of the function at `address` of `lifted`, with the calls a run
/// resolved, as the raise sees the function: its constants folded.
fn call_assumptions(lifted: &ra_asm_lift::Lifted, address: u64, traced: &BTreeMap<u64, Vec<u64>>) -> Vec<Assumption> {
    let mut f = lifted.module.function(&ra_asm_lift::function_name(address)).expect("the function is lifted").clone();
    simplify::run(&mut f);
    assume::calls(&f, &lifted.vtables, Target::WIN64, traced, &|name: &str| lifted.module.function(name).is_some())
}

/// `describe` at 0x140001070 calls `name`, `area` and `scale` through the vtable of a `Shape`
/// whose class it does not know, and `describe_named` at 0x1400010c0 calls `label` and jumps to
/// `weight` through that of a `Named`: each is resolved to the functions the vtables fill the
/// slot with, and nothing in the code checks that the receiver is one of those classes.
#[test]
fn calls_of_classes64_list_the_candidate_sets() {
    let image = ra_asm_pe::load(&fixture("classes64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let describe = call_assumptions(&lifted, 0x1_4000_1070, &BTreeMap::new());
    let name = ra_asm_lift::function_name(0x1_4000_1070);
    assert_eq!(
        sites(&describe, &name, Kind::CompleteCandidates),
        [Some(0x1_4000_107b), Some(0x1_4000_1098), Some(0x1_4000_10ac)],
        "{describe:?}"
    );
    assert!(describe.iter().all(|a| a.step == Step::Devirtualize && !a.is_discharged()), "{describe:?}");
    assert_eq!(describe.len(), 3, "{describe:?}");
    let named = call_assumptions(&lifted, 0x1_4000_10c0, &BTreeMap::new());
    let name = ra_asm_lift::function_name(0x1_4000_10c0);
    let named_sites = sites(&named, &name, Kind::CompleteCandidates);
    assert_eq!(named_sites.len(), 2, "{named:?}");
    assert_eq!(named_sites[0], Some(0x1_4000_10cb), "{named:?}");
    // A function with no call through a slot: `Square::area`.
    let leaf = call_assumptions(&lifted, 0x1_4000_11d0, &BTreeMap::new());
    assert!(leaf.is_empty(), "{leaf:?}");
}

/// A call a run resolved keeps the indirect call for a target the run did not see, so its
/// candidates are not assumed complete: the call to `name` in `describe`, once a run saw it reach
/// `Square::name`, and the jump of `dispatch` at 0x1400011e0 to its four handlers.
#[test]
fn calls_a_run_resolved_assume_no_complete_set() {
    let image = ra_asm_pe::load(&fixture("classes64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let traced = BTreeMap::from([(0x1_4000_107b, vec![0x1_4000_11e0])]);
    let describe = call_assumptions(&lifted, 0x1_4000_1070, &traced);
    let name = ra_asm_lift::function_name(0x1_4000_1070);
    assert_eq!(sites(&describe, &name, Kind::CompleteCandidates), [Some(0x1_4000_1098), Some(0x1_4000_10ac)], "{describe:?}");

    let image = ra_asm_pe::load(&fixture("dispatch64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let dispatch = lifted.module.function(&ra_asm_lift::function_name(0x1_4000_11e0)).expect("dispatch is lifted");
    let indirect: Vec<u64> = dispatch
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(&i.inst, Inst::Call { callee: Callee::Direct(name), .. } if name == "ra_call"))
        .filter_map(|i| i.address)
        .collect();
    assert!(!indirect.is_empty(), "dispatch reaches its handlers through the runtime");
    let handlers = vec![0x1_4000_1110, 0x1_4000_1120, 0x1_4000_1130, 0x1_4000_1140];
    let traced: BTreeMap<u64, Vec<u64>> = indirect.iter().map(|&site| (site, handlers.clone())).collect();
    let is_lifted = |name: &str| lifted.module.function(name).is_some();
    let resolved = ra_asm_lift::raise::abi::traced_calls(dispatch, &traced, &is_lifted);
    assert!(!resolved.is_empty(), "the run's targets are lifted functions");
    for traced in [&BTreeMap::new(), &traced] {
        let list = call_assumptions(&lifted, 0x1_4000_11e0, traced);
        assert!(list.is_empty(), "{list:?}");
    }
}

const RBX: i128 = 3;

/// `push reg`.
fn push(b: &mut FunctionBuilder, block: BlockId, n: i128) {
    let sp = register(b, block, RSP);
    let top = b.load(block, Type::I64, sp, "sp");
    let below = b.bin(block, BinOp::Add, Type::I64, top, Operand::Int(-8), "sp");
    let slot = b.cast(block, Cast::IntToPtr, Type::I64, below, Type::Ptr, "slot");
    let source = register(b, block, n);
    let v = b.load(block, Type::I64, source, "v");
    b.store(block, Type::I64, v, slot);
    b.store(block, Type::I64, below, sp);
}

/// `pop reg`.
fn pop(b: &mut FunctionBuilder, block: BlockId, n: i128) {
    let sp = register(b, block, RSP);
    let top = b.load(block, Type::I64, sp, "sp");
    let slot = b.cast(block, Cast::IntToPtr, Type::I64, top, Type::Ptr, "slot");
    let v = b.load(block, Type::I64, slot, "v");
    let destination = register(b, block, n);
    b.store(block, Type::I64, v, destination);
    let above = b.bin(block, BinOp::Add, Type::I64, top, Operand::Int(8), "sp");
    b.store(block, Type::I64, above, sp);
}

/// A machine function that writes `rbx` and returns: `mov rbx, 7; ret`, between a `push rbx` and
/// a `pop rbx` when it `saves` the register.
fn writer(name: &str, address: u64, saves: bool) -> Function {
    let (mut b, entry) = machine_builder(name, address);
    if saves {
        push(&mut b, entry, RBX);
    }
    let rbx = register(&mut b, entry, RBX);
    b.store(entry, Type::I64, Operand::Int(7), rbx);
    if saves {
        pop(&mut b, entry, RBX);
    }
    adjust(&mut b, entry, RSP, 8);
    b.terminate(entry, Terminator::Ret(None));
    b.finish()
}

/// The callee-saved assumptions of `list` as the function, the call and the callee that proves it.
fn proofs(list: &[Assumption]) -> Vec<(&str, Option<u64>, Option<u64>)> {
    list.iter().filter(|a| a.kind == Kind::CalleeSaved).map(|a| (a.function.as_str(), a.site, a.discharged)).collect()
}

/// A callee that writes no callee-saved register keeps them, and so does one that reloads the one
/// it writes from where it pushed it; one that writes `rbx` and returns does not.
#[test]
fn discharge_proves_the_call_to_a_callee_that_keeps_the_registers() {
    let (module, signatures) = module_of(vec![
        machine_function("F_00001000", 0x1000, &[(0x1005, "F_00002000"), (0x100a, "F_00003000"), (0x100f, "F_00004000")]),
        machine_function("F_00002000", 0x2000, &[]),
        writer("F_00003000", 0x3000, true),
        writer("F_00004000", 0x4000, false),
    ]);
    let mut list = assumptions(&module, &signatures, Target::WIN64);
    assert!(list.iter().all(|a| !a.is_discharged()), "{list:?}");
    assume::discharge(&module.functions, &signatures, Target::WIN64, &mut list);
    assert_eq!(
        proofs(&list),
        [("F_00001000", Some(0x1005), Some(0x2000)), ("F_00001000", Some(0x100a), Some(0x3000)), ("F_00001000", Some(0x100f), None)],
        "{list:?}"
    );
    // The return address and the stack pointer are not what the registers' summary proves.
    assert!(list.iter().filter(|a| a.kind != Kind::CalleeSaved).all(|a| !a.is_discharged()), "{list:?}");
    assert_eq!(list.len(), 9, "{list:?}");
}

/// A callee keeps the registers as far as what it calls does: not through a function that writes
/// one, through itself and through an import. A function that calls through the runtime keeps
/// them as far as that call, whose own assumption is the one that stays open.
#[test]
fn discharge_follows_what_the_callee_calls() {
    let (module, signatures) = module_of(vec![
        machine_function("F_00001000", 0x1000, &[(0x1005, "F_00002000"), (0x100a, "F_00003000"), (0x100f, "F_00005000")]),
        machine_function("F_00002000", 0x2000, &[(0x2004, "F_00004000")]),
        machine_function("F_00003000", 0x3000, &[(0x3004, "F_00003000"), (0x3009, "imp_GetTickCount")]),
        writer("F_00004000", 0x4000, false),
        machine_function("F_00005000", 0x5000, &[(0x5004, "ra_call")]),
    ]);
    let mut list = assumptions(&module, &signatures, Target::WIN64);
    assume::discharge(&module.functions, &signatures, Target::WIN64, &mut list);
    assert_eq!(
        proofs(&list),
        [
            ("F_00001000", Some(0x1005), None),
            ("F_00001000", Some(0x100a), Some(0x3000)),
            ("F_00001000", Some(0x100f), Some(0x5000)),
            ("F_00002000", Some(0x2004), None),
            ("F_00003000", Some(0x3004), Some(0x3000)),
            ("F_00005000", Some(0x5004), None),
        ],
        "{list:?}"
    );
}

/// The frame pointer at a call is one callee-saved register: the callee that keeps them all
/// proves it, and an indirect call's callee proves nothing.
#[test]
fn discharge_proves_the_frame_pointer_with_the_registers() {
    for (callee, proof) in [("F_00002000", Some(0x2000)), ("ra_call", None)] {
        let caller = framed_caller(callee);
        let mut list = stack_assumptions(&caller, Target::WIN64);
        assert_eq!(sites(&list, "F_00001000", Kind::FramePointer), [Some(0x1004)], "{list:?}");
        let (module, signatures) = module_of(vec![caller, machine_function("F_00002000", 0x2000, &[])]);
        assume::discharge(&module.functions, &signatures, Target::WIN64, &mut list);
        assert_eq!(list[0].discharged, proof, "{callee}: {list:?}");
    }
}

/// The 32-bit control fixture calls its functions directly and writes through two pointers it
/// got from `GetProcAddress` (`call eax` at 0x4012bf and 0x4012ef). Every function it calls
/// directly keeps `ebx`, `ebp`, `esi` and `edi`: `collatz` at 0x401380 pushes `esi` and pops it,
/// `fib` at 0x4013b0 does so with `edi` and `esi` around a call to itself, `print_int` at
/// 0x401300 with all four. Only the calls through the runtime are left assuming it.
#[test]
fn discharge_leaves_only_the_indirect_calls_of_control32_open() {
    let image = ra_asm_pe::load(&fixture("control32.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let inferred = infer_assuming(&lifted.module, &lifted.roots, Target::X86, &lifted.vtables, &BTreeMap::new());
    let indirect: BTreeSet<u64> = lifted
        .module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(&i.inst, Inst::Call { callee: Callee::Direct(name), .. } if name == "ra_call"))
        .filter_map(|i| i.address)
        .collect();
    let saved: Vec<&Assumption> = inferred.assumptions.iter().filter(|a| a.kind == Kind::CalleeSaved).collect();
    assert!(saved.iter().any(|a| a.is_discharged()), "{saved:?}");
    for a in &saved {
        let through_the_runtime = a.site.is_some_and(|site| indirect.contains(&site));
        assert_eq!(a.is_discharged(), !through_the_runtime, "{a:?}");
    }
    let proof = |site: u64| saved.iter().find(|a| a.site == Some(site)).and_then(|a| a.discharged);
    assert_eq!(proof(0x40_1046), Some(0x40_1380), "collatz reloads esi");
    assert_eq!(proof(0x40_13c3), Some(0x40_13b0), "fib calls itself");
    assert_eq!(proof(0x40_104d), Some(0x40_1300), "print_int reloads all four");
    assert_eq!(proof(0x40_103c), Some(0x40_12a0), "a function that calls through a pointer keeps them as far as that call");
}
