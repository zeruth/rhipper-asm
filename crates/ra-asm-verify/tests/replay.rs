//! The replayer over a whole program: `control64.exe` is recorded once at the machine level, and
//! its vectors replay against the module at the two levels that take a state, as lifted and with
//! one instruction changed; a function of two 32-bit rhtest programs that reads the x87 stack
//! replays at the machine level as recorded. The `signature` tests replay at the ABI level, where a function
//! takes arguments: functions of `control64.exe`, `control32.exe` and `float64.exe` by their
//! signatures, and the reading of a vector through a signature on vectors written by hand, for
//! the conventions no function of the fixtures has (arguments on the x86 stack, a callee that
//! pops them).

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use ra_asm_ir::interp::Value;
use ra_asm_ir::{BinOp, Inst, Type};
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::{Isa, Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::record::record;
use ra_asm_verify::{Convention, Difference, Replayer, Vector, replay, replay_abi};

/// `op_add` of the fixture, `lea eax, [rcx + rdx]` and a return: entered once, through the table
/// of operations.
const OP_ADD: u64 = 0x1_4000_1660;

const CONTROL64: &str = "control64.exe";
const CONTROL32: &str = "control32.exe";
const FLOAT64: &str = "float64.exe";

fn fixture(name: &str) -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name);
    ra_asm_pe::load(&path).expect("the fixture loads")
}

fn image() -> Image {
    fixture(CONTROL64)
}

/// A fixture lifted and raised to the ABI level, standing alone as a port does, with the
/// signatures of its functions under the names the recorded run knew.
fn raised(image: &Image) -> (Lifted, HashMap<String, Signature>) {
    let analysis = ra_asm_analysis::analyze(image);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    (lifted, signatures)
}

/// The fixture lifted and raised to `level`, under the names the recorded run knew.
fn lifted(image: &Image, level: RaiseLevel) -> Lifted {
    let analysis = ra_asm_analysis::analyze(image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
    let options = lifted.raise_options(level);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    lifted
}

/// The vectors of one recorded run of the fixture `name` at the machine level, made once; none on
/// a host without the Windows DLLs the program imports, where the tests are skipped.
fn recorded(name: &'static str) -> Option<&'static [Vector]> {
    static CONTROL64_RUN: OnceLock<Option<Vec<Vector>>> = OnceLock::new();
    static CONTROL32_RUN: OnceLock<Option<Vec<Vector>>> = OnceLock::new();
    static FLOAT64_RUN: OnceLock<Option<Vec<Vector>>> = OnceLock::new();
    let run = match name {
        CONTROL64 => &CONTROL64_RUN,
        CONTROL32 => &CONTROL32_RUN,
        FLOAT64 => &FLOAT64_RUN,
        other => panic!("no recorded run of {other}"),
    };
    run.get_or_init(|| {
        let image = fixture(name);
        let lifted = lifted(&image, RaiseLevel::Machine);
        let (recording, outcome) = record(&image, &lifted, &ra_asm_lift::run::Options::default()).expect("the run starts");
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: {name}: {stop}; this host has no Windows DLLs: the replays are skipped");
            return None;
        }
        assert_eq!(outcome.exit, 0, "{name}: the recorded run ends as the program does (stop: {:?})", outcome.stop);
        Some(recording.vectors)
    })
    .as_deref()
}

fn vectors() -> Option<&'static [Vector]> {
    recorded(CONTROL64)
}

/// Every vector the recorded run of the fixture kept of each of `functions`, which are addresses
/// in its image, replays at the ABI level through the function's signature with no difference.
fn replays_by_signature(name: &'static str, functions: &[(u64, &str)]) {
    let Some(vectors) = recorded(name) else { return };
    let image = fixture(name);
    let (lifted, signatures) = raised(&image);
    let replayer = Replayer::with_signatures(&image, &lifted, &signatures).expect("the ABI level replays");
    for (address, what) in functions {
        let own: Vec<&Vector> = vectors.iter().filter(|vector| vector.address == *address).collect();
        assert!(!own.is_empty(), "{name}: the run kept a vector of {what} at {address:#x}");
        assert!(signatures.contains_key(&own[0].function), "{name}: {what} has a signature under the name {}", own[0].function);
        for (n, vector) in own.iter().enumerate() {
            let differences = replayer.replay(vector).expect("the vector loads");
            let lines: Vec<String> = differences.iter().map(ToString::to_string).collect();
            assert!(differences.is_empty(), "{name}: {what} vector {} of {} differs:\n{}", n + 1, own.len(), lines.join("\n"));
        }
    }
}

/// Every recorded vector replays against the module as lifted.
fn replays(level: RaiseLevel) {
    let Some(vectors) = vectors() else { return };
    assert!(vectors.len() >= 10, "the run kept {} vectors", vectors.len());
    let image = image();
    let lifted = lifted(&image, level);
    let failures = replay(&image, &lifted, level, vectors).expect("the vectors load");
    let lines: Vec<String> = failures.iter().map(ToString::to_string).collect();
    assert!(
        failures.is_empty(),
        "{} of {} vectors differ at the {} level:\n{}",
        failures.len(),
        vectors.len(),
        level.name(),
        lines.join("\n")
    );
}

/// With the first addition of `op_add` made a subtraction, the vector of `op_add` and no other
/// fails, with a line that names the function, the vector and a register.
fn changed(level: RaiseLevel) {
    let Some(vectors) = vectors() else { return };
    let image = image();
    let mut lifted = lifted(&image, level);
    let name = lifted.functions.get(&OP_ADD).expect("op_add is lifted").clone();
    let f = lifted.module.functions.iter_mut().find(|f| f.name == name).expect("op_add is in the module");
    let add = f
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instrs)
        .find_map(|i| match &mut i.inst {
            Inst::Bin { op, .. } if *op == BinOp::Add => Some(op),
            _ => None,
        })
        .expect("op_add adds");
    *add = BinOp::Sub;
    let failures = replay(&image, &lifted, level, vectors).expect("the vectors load");
    let lines: Vec<String> =
        failures.iter().flat_map(|failure| failure.to_string().lines().map(str::to_owned).collect::<Vec<_>>()).collect();
    assert_eq!(failures.len(), 1, "one vector differs at the {} level:\n{}", level.name(), lines.join("\n"));
    assert_eq!((failures[0].function.as_str(), failures[0].vector), (name.as_str(), 1));
    let register = format!("{name} vector 1: register ");
    assert!(
        lines.iter().any(|line| line.starts_with(&register)),
        "a line names the function, the vector and the register:\n{}",
        lines.join("\n")
    );
    assert!(failures[0].differences.iter().any(|difference| matches!(difference, Difference::Register { .. })));
}

#[test]
fn stateful_machine_level_replays_every_vector_of_control64() {
    replays(RaiseLevel::Machine);
}

#[test]
fn stateful_promoted_level_replays_every_vector_of_control64() {
    replays(RaiseLevel::Promoted);
}

#[test]
fn stateful_machine_level_with_one_instruction_changed_names_the_function_the_vector_and_the_register() {
    changed(RaiseLevel::Machine);
}

#[test]
fn stateful_promoted_level_with_one_instruction_changed_names_the_function_the_vector_and_the_register() {
    changed(RaiseLevel::Promoted);
}

#[test]
fn stateful_a_different_callee_or_argument_is_a_difference() {
    let Some(vectors) = vectors() else { return };
    let image = image();
    let lifted = lifted(&image, RaiseLevel::Machine);
    let replayer = Replayer::new(&image, &lifted, RaiseLevel::Machine).expect("the machine level replays");
    // A call through a pointer goes to the runtime with the state and the target.
    let (vector, at) = vectors
        .iter()
        .find_map(|vector| vector.calls.iter().position(|call| call.callee == "ra_call" && call.args.len() == 2).map(|at| (vector, at)))
        .expect("a function of the fixture calls through a pointer");
    let differences = replayer.replay(vector).expect("the vector loads");
    assert!(differences.is_empty(), "{} replays as recorded: {differences:?}", vector.function);

    let mut other = vector.clone();
    other.calls[at].args[1] ^= 0x10;
    let differences = replayer.replay(&other).expect("the vector loads");
    assert!(
        differences.iter().any(|d| matches!(d, Difference::Arguments { index, callee, .. } if *index == at + 1 && callee == "ra_call")),
        "another target is another argument: {differences:?}"
    );

    let mut other = vector.clone();
    other.calls[at].callee = "ra_other".to_owned();
    let differences = replayer.replay(&other).expect("the vector loads");
    assert!(
        differences.iter().any(|d| matches!(d, Difference::Call { index, replayed, .. } if *index == at + 1 && replayed == "ra_call")),
        "another callee is a difference: {differences:?}"
    );
}

/// A 32-bit function that stores the float a lifted callee left in `st(0)` replays at the machine
/// level as its run recorded it: the x87 stack is a global of the lift's own, which the replay
/// lays out where the vector has it. `templates` of `cpp-clang.exe` stores a callee's double, and
/// `float_reader` of `engine-clang.exe` three callees' floats, the third into `xmm1`.
#[test]
fn machine_replay_matches_its_record() {
    for (name, address, what) in [("cpp-clang.exe", 0x40_1460, "templates"), ("engine-clang.exe", 0x40_12d0, "float_reader")] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest").join(name);
        let image = ra_asm_pe::load(&path).expect("the program loads");
        let lifted = lifted(&image, RaiseLevel::Machine);
        let (recording, outcome) = record(&image, &lifted, &ra_asm_lift::run::Options::default()).expect("the run starts");
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: {name}: {stop}; this host has no Windows DLLs: the replay is skipped");
            return;
        }
        let own: Vec<&Vector> = recording.vectors.iter().filter(|vector| vector.address == address).collect();
        assert!(!own.is_empty(), "{name}: the run kept a vector of {what} at {address:#x} (stop: {:?})", outcome.stop);
        let replayer = Replayer::new(&image, &lifted, RaiseLevel::Machine).expect("the machine level replays");
        for (n, vector) in own.iter().enumerate() {
            let differences = replayer.replay(vector).expect("the vector loads");
            let lines: Vec<String> = differences.iter().map(ToString::to_string).collect();
            assert!(differences.is_empty(), "{name}: {what} vector {} of {} differs:\n{}", n + 1, own.len(), lines.join("\n"));
        }
    }
}

/// The Win64 convention: the arguments in `rcx` and `rdx`, the result in `rax` at the width the
/// function returns it at, a recursion answered call by call, an import answered through its
/// wrapper's record, and a function that writes the program's data.
#[test]
fn signature_functions_of_control64_replay_by_their_register_arguments() {
    replays_by_signature(
        CONTROL64,
        &[
            (OP_ADD, "op_add"),
            (0x1_4000_1670, "op_sub"),
            (0x1_4000_1680, "op_mul"),
            (0x1_4000_13c0, "collatz"),
            (0x1_4000_13f0, "fib"),
            (0x1_4000_1560, "print_str"),
            (0x1_4000_15b0, "sum_table"),
            (0x1_4000_15f0, "sort_table"),
        ],
    );
}

/// The x86 conventions of `control32.exe`, which clang gave its local functions: the arguments
/// in `ecx` and `edx`, the result in `eax`, the return address popped and nothing beyond it.
#[test]
fn signature_functions_of_control32_replay_by_their_register_arguments() {
    replays_by_signature(
        CONTROL32,
        &[
            (0x40_19a0, "op_add"),
            (0x40_19b0, "op_sub"),
            (0x40_19c0, "op_mul"),
            (0x40_1380, "collatz"),
            (0x40_13b0, "fib"),
            (0x40_14f0, "sum_table"),
            (0x40_1530, "sort_table"),
        ],
    );
}

/// `print_str` of `control32.exe` pushes five arguments and calls `WriteFile`, which is stdcall
/// and pops them: the call is answered from the record of the import's wrapper, and the function
/// leaves its own stack pointer where its signature says.
#[test]
fn signature_a_function_of_control32_that_calls_a_stdcall_import_replays() {
    replays_by_signature(CONTROL32, &[(0x40_14c0, "print_str")]);
}

/// A double in `xmm0` in, a double in `xmm0` out, and both beside integers: `milli` takes a
/// double and returns an integer, `mean` takes a pointer and returns a double, `newton_sqrt`
/// takes and returns one.
#[test]
fn signature_float_arguments_and_results_of_float64_replay_through_xmm0() {
    replays_by_signature(FLOAT64, &[(0x1_4000_12b0, "milli"), (0x1_4000_12c0, "mean"), (0x1_4000_12f0, "newton_sqrt")]);
}

/// With the addition of `op_add` made a subtraction at the ABI level, its vector fails on the
/// result, which the line names by the register the record holds it in.
#[test]
fn signature_level_with_one_instruction_changed_names_the_result_register() {
    let Some(vectors) = recorded(CONTROL64) else { return };
    let image = image();
    let (mut lifted, signatures) = raised(&image);
    let name = lifted.functions.get(&OP_ADD).expect("op_add is lifted").clone();
    let own: Vec<Vector> = vectors.iter().filter(|vector| vector.address == OP_ADD).cloned().collect();
    assert!(!own.is_empty(), "the run kept a vector of op_add");
    let failures = replay_abi(&image, &lifted, &signatures, &own).expect("the vectors load");
    assert!(failures.is_empty(), "op_add replays as raised: {failures:?}");

    let f = lifted.module.functions.iter_mut().find(|f| f.name == name).expect("op_add is in the module");
    let add = f
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instrs)
        .find_map(|i| match &mut i.inst {
            Inst::Bin { op, .. } if *op == BinOp::Add => Some(op),
            _ => None,
        })
        .expect("op_add adds");
    *add = BinOp::Sub;
    let failures = replay_abi(&image, &lifted, &signatures, &own).expect("the vectors load");
    assert_eq!(failures.len(), own.len(), "every vector of op_add differs: {failures:?}");
    let line = failures[0].to_string();
    assert!(line.starts_with(&format!("{name} vector 1: register rax: recorded 0x30, replayed 0x24")), "{line}");
    assert!(failures[0].differences.iter().all(|difference| matches!(difference, Difference::Register { .. })), "{line}");
}

/// A cdecl function reads its arguments above the return address, a dword each, a 64-bit
/// integer and a double two; it pops the return address alone, and its result is the low
/// dword of `eax`.
#[test]
fn signature_cdecl_arguments_are_the_stack_above_the_return_address() {
    let convention = Convention::new(Isa::X86, 32);
    let signature = Signature { stack_params: vec![Kind::Int, Kind::Wide, Kind::Double], returns: Some(Kind::Int), ..Signature::default() };
    let mut stack = Vec::new();
    stack.extend(7u32.to_le_bytes());
    stack.extend(0x1_0000_0002_u64.to_le_bytes());
    stack.extend(1.5f64.to_le_bytes());
    let vector = Vector {
        function: "f".to_owned(),
        entry: vec![("esp".to_owned(), 0x2000)],
        // The return address at the stack pointer is no argument.
        memory_in: vec![(0x2000, vec![0xaa; 4]), (0x2004, stack)],
        exit: vec![("eax".to_owned(), 0xffff_ffff_0000_002a), ("esp".to_owned(), 0x2004)],
        ..Vector::default()
    };
    let arguments = convention.arguments(&signature, &vector).expect("the vector holds the stack pointer");
    assert_eq!(arguments, [Value::Int(7), Value::Int(0x1_0000_0002), Value::F64(1.5)]);
    assert_eq!(convention.exit_stack_pointer(&signature, 0x2000), 0x2004);
    let result = convention.result(signature.returns, &Type::I64, &vector.exit).expect("the exit holds eax").expect("a result");
    assert_eq!((result.register.as_str(), result.bits), ("eax", 32));
    assert!(result.is(&Value::Int(0x2a)) && !result.is(&Value::Int(0x2b)));
    // An argument the function never read is not in the vector, and is zero.
    let unread = Vector { memory_in: Vec::new(), ..vector };
    let arguments = convention.arguments(&signature, &unread).expect("the vector holds the stack pointer");
    assert_eq!(arguments, [Value::Int(0), Value::Int(0), Value::F64(0.0)]);
}

/// A stdcall function pops its arguments with the return address, and a thiscall one takes
/// `ecx` ahead of the stack; the frame that dies with either runs from its own space below the
/// entry stack pointer to the end of its arguments.
#[test]
fn signature_stdcall_function_leaves_the_stack_pointer_past_its_arguments() {
    let convention = Convention::new(Isa::X86, 32);
    let stdcall = Signature { stack_params: vec![Kind::Int, Kind::Int], pops: 8, ..Signature::default() };
    assert_eq!(convention.exit_stack_pointer(&stdcall, 0x2000), 0x200c);
    assert_eq!(convention.frame(&stdcall, 0x2000), (0x2000 - 32, 0x200c));
    let deep = Signature { frame_below: 0x40, ..stdcall.clone() };
    assert_eq!(convention.frame(&deep, 0x2000), (0x2000 - 0x50, 0x200c));

    let thiscall = Signature { params: vec![Kind::Int], stack_params: vec![Kind::Int], pops: 4, ..Signature::default() };
    let vector = Vector {
        function: "f".to_owned(),
        entry: vec![("ecx".to_owned(), 0x10), ("esp".to_owned(), 0x3000)],
        memory_in: vec![(0x3004, 5u32.to_le_bytes().to_vec())],
        ..Vector::default()
    };
    let arguments = convention.arguments(&thiscall, &vector).expect("the vector holds its registers");
    assert_eq!(arguments, [Value::Int(0x10), Value::Int(5)]);
    assert_eq!(convention.exit_stack_pointer(&thiscall, 0x3000), 0x3008);
    // The registers the callee keeps: `ebx`, `ebp`, `esi` and `edi`, not the stack pointer.
    assert_eq!(convention.kept(&thiscall), ["ebx", "ebp", "esi", "edi"]);
}

/// A float result is the low bits of `xmm0`, four bytes of them or eight by its type, whatever
/// the rest of the register holds; on Win64 a float argument is the vector register of its
/// position.
#[test]
fn signature_float_result_is_the_low_bits_of_xmm0() {
    let convention = Convention::new(Isa::X86, 64);
    let above = 0xdead_beef_u128 << 64;
    let exit = vec![("rax".to_owned(), 1), ("xmm0".to_owned(), above | u128::from(2.5f64.to_bits()))];
    let double = convention.result(Some(Kind::Double), &Type::Double, &exit).expect("the exit holds xmm0").expect("a result");
    assert_eq!((double.register.as_str(), double.bits, &double.value), ("xmm0", 64, &Value::F64(2.5)));
    assert!(double.is(&Value::F64(2.5)) && !double.is(&Value::F64(2.0)));
    let exit = vec![("xmm0".to_owned(), above | (0x7777_u128 << 32) | u128::from(1.5f32.to_bits()))];
    let single = convention.result(Some(Kind::Float), &Type::Float, &exit).expect("the exit holds xmm0").expect("a result");
    assert_eq!((single.bits, &single.value), (32, &Value::F32(1.5)));
    assert!(single.is(&Value::F32(1.5)) && !single.is(&Value::F32(-1.5)));
    // A record that does not hold the register cannot answer.
    assert_eq!(convention.result(Some(Kind::Double), &Type::Double, &[]), Err("xmm0".to_owned()));
    assert_eq!(convention.result(None, &Type::Void, &[]), Ok(None));

    let signature = Signature { params: vec![Kind::Int, Kind::Double], ..Signature::default() };
    let vector = Vector {
        function: "f".to_owned(),
        entry: vec![("rcx".to_owned(), 3), ("xmm1".to_owned(), above | u128::from(0.25f64.to_bits())), ("rsp".to_owned(), 0x1000)],
        ..Vector::default()
    };
    let arguments = convention.arguments(&signature, &vector).expect("the vector holds its registers");
    assert_eq!(arguments, [Value::Int(3), Value::F64(0.25)]);
}
