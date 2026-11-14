//! The recorder over a whole program: `control64.exe` is lifted, raised to the machine level and
//! run through the interpreter once, and the three tests read what the run recorded.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use ra_asm_verify::record::{KEPT, Recording, record};
use ra_asm_verify::{Vector, read, write};

/// What one recorded run of the fixture left.
struct Recorded {
    recording: Recording,
    /// The image's functions the run entered, as the run's own facts name them.
    entered: BTreeSet<u64>,
    /// The name of the function the run started at.
    entry: String,
    exit: i32,
    stop: Option<String>,
}

/// The recorded run of `control64.exe`, made once; none on a host without the Windows DLLs the
/// program imports, where the run stops at the first of them and the tests are skipped.
fn control() -> Option<&'static Recorded> {
    static RUN: OnceLock<Option<Recorded>> = OnceLock::new();
    RUN.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
        let image = ra_asm_pe::load(&path).expect("the fixture loads");
        let analysis = ra_asm_analysis::analyze(&image);
        let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
        ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, None);
        let (recording, outcome) = record(&image, &lifted, &ra_asm_lift::run::Options::default()).expect("the run starts");
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: control64.exe: {stop}; this host has no Windows DLLs: the recorded run is skipped");
            return None;
        }
        let entry = lifted.entry.or_else(|| image.entry_point()).expect("the image has an entry");
        let entry = lifted.functions.get(&entry).expect("the entry is lifted").clone();
        Some(Recorded {
            recording,
            entered: outcome.facts.entered.iter().copied().collect(),
            entry,
            exit: outcome.exit,
            stop: outcome.stop,
        })
    })
    .as_ref()
}

/// The kept vectors of the function `name`.
fn vectors_of<'a>(recorded: &'a Recorded, name: &'a str) -> impl Iterator<Item = &'a Vector> {
    recorded.recording.vectors.iter().filter(move |vector| vector.function == name)
}

#[test]
fn control64_yields_a_vector_for_every_function_its_run_enters() {
    let Some(recorded) = control() else { return };
    assert_eq!(recorded.exit, 0, "the recorded run ends as the program does (stop: {:?})", recorded.stop);
    assert!(!recorded.entered.is_empty(), "the run entered functions of the image");
    let with_vector: BTreeSet<u64> = recorded.recording.vectors.iter().map(|vector| vector.address).collect();
    assert_eq!(with_vector, recorded.entered, "the functions with a vector are the functions the run entered");
    for vector in &recorded.recording.vectors {
        assert!(vector.entry.iter().any(|(name, _)| name == "rsp"), "{}: the entry holds the registers", vector.function);
        assert_eq!(vector.entry.len(), vector.exit.len(), "{}: the exit holds the registers the entry holds", vector.function);
    }
    // The vectors are a file like any other: they write, and read back as themselves.
    let text = write(&recorded.recording.vectors).expect("the recorded vectors write");
    assert_eq!(read(&text).expect("the written vectors read"), recorded.recording.vectors);
}

#[test]
fn a_function_called_a_thousand_times_keeps_at_most_sixteen() {
    let Some(recorded) = control() else { return };
    let (busiest, activations) =
        recorded.recording.activations.iter().max_by_key(|(_, activations)| **activations).expect("the run entered a function");
    assert!(*activations >= 1000, "{busiest} ran {activations} times: the fixture's recursion runs a thousand at least");
    let kept = vectors_of(recorded, busiest).count();
    assert!((1..=KEPT).contains(&kept), "{busiest} ran {activations} times and keeps {kept} vectors");
    for name in recorded.recording.activations.keys() {
        let kept = vectors_of(recorded, name).count();
        assert!((1..=KEPT).contains(&kept), "{name} keeps {kept} vectors");
    }
}

#[test]
fn a_recursive_functions_inner_activations_are_answered_calls() {
    let Some(recorded) = control() else { return };
    let recursive = recorded
        .recording
        .vectors
        .iter()
        .find(|vector| vector.calls.iter().any(|call| call.callee == vector.function))
        .expect("a kept vector of the fixture's recursive function calls the function itself");
    let name = recursive.function.as_str();
    let activations = recorded.recording.activations[name];
    assert!(activations >= 100, "{name} ran {activations} times: it is the fixture's recursion");
    // An inner activation is answered: the call carries what the callee changed.
    for call in recursive.calls.iter().filter(|call| call.callee == name) {
        assert!(!call.results.is_empty() || !call.effects.is_empty(), "the call of {name} inside {name} carries its answer");
    }
    // A vector holds the calls its activation made itself, never the calls made under them: the
    // argument of the recursion is 15 at most, and no activation calls on more often than that.
    for vector in vectors_of(recorded, name) {
        assert!(vector.calls.len() <= 16, "a vector of {name} holds {} calls", vector.calls.len());
    }
    // The outermost activation of all, the entry's, saw the whole recursion run under it and
    // holds only the calls the entry made.
    let entry = vectors_of(recorded, &recorded.entry).next().expect("the entry has a vector");
    let direct = entry.calls.iter().filter(|call| call.callee == name).count();
    assert!((1..10).contains(&direct), "the entry holds {direct} calls of {name}, which ran {activations} times");
}
