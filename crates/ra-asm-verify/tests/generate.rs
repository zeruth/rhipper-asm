//! Generated inputs over `structs64.exe`: its committed vectors stand for the recorded run, and a
//! function is one the run did not enter when its vectors are left out. `weigh` reads and writes
//! the record its parameter points to and nothing else, so it gets vectors of generated inputs,
//! which the machine level makes and the promoted and the ABI level replay; `print_int` reads the
//! global output handle and calls `WriteFile`, and gets none.
//!
//! Mutated vectors over `dispatch64.exe`: every number its run prints is positive, so no recorded
//! vector of `print_int` runs the block that writes the minus sign; a vector mutated from them
//! does, its call of `WriteFile` answered as the record answers it.

use std::collections::HashMap;
use std::path::Path;

use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::generate::{COUNT, MUTATIONS, SEED};
use ra_asm_verify::{Replayer, Report, Vector, generate, mutate, summary};

/// `weigh`: `r->total += r->id * r->delta; r->flags |= 1 << r->kind; return r->total`.
const WEIGH: u64 = 0x1_4000_1180;

/// `print_int`: the digits of its argument, written to the global handle `out`.
const PRINT_INT: u64 = 0x1_4000_10e0;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn image() -> Image {
    image_of("structs64")
}

/// The vectors the recorded run of the fixture kept, as committed.
fn recorded() -> Vec<Vector> {
    recorded_of("structs64")
}

fn image_of(name: &str) -> Image {
    ra_asm_pe::load(&root().join(format!("testdata/bin/{name}.exe"))).expect("the fixture loads")
}

fn recorded_of(name: &str) -> Vec<Vector> {
    let path = root().join(format!("testdata/vectors/fn/{name}.fv"));
    let text = std::fs::read_to_string(&path).expect("the fixture's vectors are committed");
    ra_asm_verify::read(&text).expect("the fixture's vectors read")
}

/// `print_int` of `dispatch64.exe`: its sign, its digits and a call of `WriteFile`.
const DISPATCH_PRINT_INT: u64 = 0x1_4000_1150;

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

/// The fixture lifted standing alone and raised to the ABI level, with its signatures.
fn raised(image: &Image) -> (Lifted, HashMap<String, Signature>) {
    let analysis = ra_asm_analysis::analyze(image);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    (lifted, signatures)
}

/// The vectors generated for the fixture when the run did not enter the function at `left_out`.
fn generated(image: &Image, machine: &Lifted, left_out: u64) -> Vec<Vector> {
    let (abi, signatures) = raised(image);
    let recorded: Vec<Vector> = recorded().into_iter().filter(|vector| vector.address != left_out).collect();
    generate(image, machine, &abi, &signatures, &recorded, SEED).expect("the inputs run")
}

#[test]
fn a_pure_function_the_run_did_not_enter_gets_its_count_of_vectors() {
    let image = image();
    let machine = lifted(&image, RaiseLevel::Machine);
    let vectors = generated(&image, &machine, WEIGH);
    let weigh: Vec<&Vector> = vectors.iter().filter(|vector| vector.address == WEIGH).collect();
    assert_eq!(weigh.len(), COUNT, "weigh gets {COUNT} vectors");
    assert_eq!(COUNT, 16);
    assert!(weigh.iter().all(|vector| vector.function == "weigh" && vector.left == ra_asm_verify::Left::Return));
    // The record it was handed is read, and differs from input to input.
    assert!(weigh.iter().all(|vector| !vector.memory_in.is_empty()), "every input reads the record");
    let inputs: std::collections::BTreeSet<_> = weigh.iter().map(|vector| &vector.memory_in).collect();
    assert!(inputs.len() > COUNT / 2, "the inputs differ: {} of {COUNT} are distinct", inputs.len());
    // The same seed makes the same vectors.
    assert_eq!(vectors, generated(&image, &machine, WEIGH));
}

#[test]
fn the_generated_vectors_replay_at_the_machine_the_promoted_and_the_abi_level() {
    let image = image();
    let machine = lifted(&image, RaiseLevel::Machine);
    let vectors: Vec<Vector> = generated(&image, &machine, WEIGH).into_iter().filter(|vector| vector.address == WEIGH).collect();
    assert_eq!(vectors.len(), COUNT);
    let promoted = lifted(&image, RaiseLevel::Promoted);
    let (abi, signatures) = raised(&image);
    let replayers = [
        ("machine", Replayer::new(&image, &machine, RaiseLevel::Machine).expect("the machine level replays")),
        ("promoted", Replayer::new(&image, &promoted, RaiseLevel::Promoted).expect("the promoted level replays")),
        ("abi", Replayer::with_signatures(&image, &abi, &signatures).expect("the ABI level replays")),
    ];
    for (level, replayer) in &replayers {
        for (n, vector) in vectors.iter().enumerate() {
            let differences = replayer.replay(vector).expect("the vector loads");
            let lines: Vec<String> = differences.iter().map(ToString::to_string).collect();
            assert!(differences.is_empty(), "{level}: weigh vector {} differs:\n{}", n + 1, lines.join("\n"));
        }
    }
}

#[test]
fn a_function_that_reads_a_global_gets_none() {
    let image = image();
    let (abi, _) = raised(&image);
    let machine = lifted(&image, RaiseLevel::Machine);
    let name = machine.functions.get(&PRINT_INT).expect("print_int is a function of the fixture");
    let f = abi.module.function(name).expect("print_int is lifted at the ABI level");
    assert_eq!(summary(f), None, "print_int reads the global handle and calls WriteFile");
    let vectors = generated(&image, &machine, PRINT_INT);
    assert!(vectors.iter().all(|vector| vector.address != PRINT_INT), "print_int gets no vector");
}

#[test]
fn a_function_the_run_entered_gets_none() {
    let image = image();
    let machine = lifted(&image, RaiseLevel::Machine);
    let (abi, signatures) = raised(&image);
    let recorded = recorded();
    let vectors = generate(&image, &machine, &abi, &signatures, &recorded, SEED).expect("the inputs run");
    let entered: Vec<&str> =
        vectors.iter().filter(|v| recorded.iter().any(|r| r.address == v.address)).map(|v| v.function.as_str()).collect();
    assert!(entered.is_empty(), "the run entered {entered:?}, which get no generated vector");
}

/// The blocks of `print_int` no vector of `report` that replayed equal ran.
fn never_of_print_int(report: &Report) -> std::collections::BTreeSet<String> {
    report.covered.get(&DISPATCH_PRINT_INT).expect("the vectors of print_int replay equal").tested.never.clone()
}

#[test]
fn a_mutated_vector_runs_a_block_the_recorded_ones_did_not() {
    let image = image_of("dispatch64");
    let machine = lifted(&image, RaiseLevel::Machine);
    let recorded = recorded_of("dispatch64");
    let mutated = mutate(&image, &machine, &recorded, SEED).expect("the mutations run");
    let of_print_int: Vec<&Vector> = mutated.iter().filter(|vector| vector.address == DISPATCH_PRINT_INT).collect();
    assert!(!of_print_int.is_empty(), "print_int gets a mutated vector");
    assert!(mutated.len() <= MUTATIONS * 8, "{} mutated vectors", mutated.len());
    // A kept vector returned and made the calls of a recorded vector of its function, in order.
    for vector in &mutated {
        assert_eq!(vector.left, ra_asm_verify::Left::Return, "{} returns", vector.function);
        let callees: Vec<&str> = vector.calls.iter().map(|call| call.callee.as_str()).collect();
        let seeded = recorded
            .iter()
            .filter(|seed| seed.address == vector.address)
            .any(|seed| seed.calls.iter().map(|call| call.callee.as_str()).eq(callees.iter().copied()));
        assert!(seeded, "{} calls {callees:?}, as no vector of it does", vector.function);
    }
    // The blocks none of the recorded vectors ran shrink by the mutated ones.
    let replayer = Replayer::new(&image, &machine, RaiseLevel::Machine).expect("the machine level replays");
    let before = never_of_print_int(&replayer.report(&recorded).expect("the recorded vectors load"));
    let all: Vec<Vector> = recorded.iter().chain(&mutated).cloned().collect();
    let after = never_of_print_int(&replayer.report(&all).expect("the mutated vectors load"));
    assert!(!before.is_empty(), "the recorded vectors of print_int leave a block unrun");
    assert!(after.len() < before.len(), "print_int leaves {before:?} unrun before the mutations and {after:?} after");
    assert!(after.is_subset(&before));
    // The same seed makes the same vectors.
    assert_eq!(mutated, mutate(&image, &machine, &recorded, SEED).expect("the mutations run"));
}

#[test]
fn the_mutated_vectors_replay_at_the_machine_and_the_promoted_level() {
    let image = image_of("dispatch64");
    let machine = lifted(&image, RaiseLevel::Machine);
    let mutated = mutate(&image, &machine, &recorded_of("dispatch64"), SEED).expect("the mutations run");
    assert!(!mutated.is_empty(), "the mutations keep a vector");
    let promoted = lifted(&image, RaiseLevel::Promoted);
    let replayers = [
        ("machine", Replayer::new(&image, &machine, RaiseLevel::Machine).expect("the machine level replays")),
        ("promoted", Replayer::new(&image, &promoted, RaiseLevel::Promoted).expect("the promoted level replays")),
    ];
    for (level, replayer) in &replayers {
        for (n, vector) in mutated.iter().enumerate() {
            let differences = replayer.replay(vector).expect("the vector loads");
            let lines: Vec<String> = differences.iter().map(ToString::to_string).collect();
            assert!(differences.is_empty(), "{level}: mutated vector {} of {} differs:\n{}", n + 1, vector.function, lines.join("\n"));
        }
    }
}
