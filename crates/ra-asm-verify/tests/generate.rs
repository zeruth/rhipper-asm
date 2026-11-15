//! Generated inputs over `structs64.exe`: its committed vectors stand for the recorded run, and a
//! function is one the run did not enter when its vectors are left out. `weigh` reads and writes
//! the record its parameter points to and nothing else, so it gets vectors of generated inputs,
//! which the machine level makes and the promoted and the ABI level replay; `print_int` reads the
//! global output handle and calls `WriteFile`, and gets none.

use std::collections::HashMap;
use std::path::Path;

use ra_asm_analysis;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::generate::{COUNT, SEED};
use ra_asm_verify::{Replayer, Vector, generate, summary};

/// `weigh`: `r->total += r->id * r->delta; r->flags |= 1 << r->kind; return r->total`.
const WEIGH: u64 = 0x1_4000_1180;

/// `print_int`: the digits of its argument, written to the global handle `out`.
const PRINT_INT: u64 = 0x1_4000_10e0;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn image() -> Image {
    ra_asm_pe::load(&root().join("testdata/bin/structs64.exe")).expect("the fixture loads")
}

/// The vectors the recorded run of the fixture kept, as committed.
fn recorded() -> Vec<Vector> {
    let path = root().join("testdata/vectors/fn/structs64.fv");
    let text = std::fs::read_to_string(&path).expect("the fixture's vectors are committed");
    ra_asm_verify::read(&text).expect("the fixture's vectors read")
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
