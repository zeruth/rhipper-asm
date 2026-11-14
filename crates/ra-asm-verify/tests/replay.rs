//! The replayer over a whole program: `control64.exe` is recorded once at the machine level, and
//! its vectors replay against the module at the two levels that take a state, as lifted and with
//! one instruction changed.

use std::path::Path;
use std::sync::OnceLock;

use ra_asm_ir::{BinOp, Inst};
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::record::record;
use ra_asm_verify::{Difference, Replayer, Vector, replay};

/// `op_add` of the fixture, `lea eax, [rcx + rdx]` and a return: entered once, through the table
/// of operations.
const OP_ADD: u64 = 0x1_4000_1660;

fn image() -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    ra_asm_pe::load(&path).expect("the fixture loads")
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

/// The vectors of one recorded run of `control64.exe` at the machine level, made once; none on a
/// host without the Windows DLLs the program imports, where the tests are skipped.
fn vectors() -> Option<&'static [Vector]> {
    static RUN: OnceLock<Option<Vec<Vector>>> = OnceLock::new();
    RUN.get_or_init(|| {
        let image = image();
        let lifted = lifted(&image, RaiseLevel::Machine);
        let (recording, outcome) = record(&image, &lifted, &ra_asm_lift::run::Options::default()).expect("the run starts");
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: control64.exe: {stop}; this host has no Windows DLLs: the replays are skipped");
            return None;
        }
        assert_eq!(outcome.exit, 0, "the recorded run ends as the program does (stop: {:?})", outcome.stop);
        Some(recording.vectors)
    })
    .as_deref()
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
