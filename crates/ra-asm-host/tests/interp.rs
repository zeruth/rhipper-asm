//! computed through the interpreter with the residual engine behind it: the three functions it
//! reaches only by addresses it computes are no lifted function, and the engine runs each from
//! the run's memory, so the program runs to its end with the native program's output.

use std::path::{Path, PathBuf};

use ra_asm_lift::run::Outcome;

/// `h_blend`, `h_fold` and `h_weigh`, by the map beside the fixture.
const HIDDEN: [u64; 3] = [0x1_4000_108c, 0x1_4000_1092, 0x1_4000_109a];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// computed64 lifted at the machine level and run with the residual engine; none when this host
/// has no Windows DLLs to answer its imports, which says so on stderr.
fn run_computed() -> Option<Outcome> {
    let image = ra_asm_pe::load(&root().join("testdata/bin/computed64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let outcome = ra_asm_host::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    if let Some(stop) = &outcome.stop
        && stop.starts_with("no binding for ")
        && !cfg!(windows)
    {
        eprintln!("SKIPPED: computed64.exe: {stop}; this host has no Windows DLLs: the interpreted run is skipped");
        return None;
    }
    Some(outcome)
}

#[test]
fn computed64_runs_to_its_end_through_the_residual_engine() {
    let Some(outcome) = run_computed() else { return };
    let expected = std::fs::read_to_string(root().join("testdata/expected/computed.txt")).unwrap();
    let actual = String::from_utf8_lossy(&outcome.output);
    let failed = ra_asm_testing::digest::failed_dir();
    if let Err(line) =
        ra_asm_testing::digest::compare_in(&failed, "computed64.exe (run-machine): residual", expected.trim_end(), actual.trim_end())
    {
        eprintln!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
        panic!("{line}");
    }
    assert_eq!(outcome.exit, 0, "the exit of computed64 (stop: {:?})", outcome.stop);
}

#[test]
fn computed64_facts_name_the_three_functions_the_engine_ran() {
    let Some(outcome) = run_computed() else { return };
    assert_eq!(outcome.stop, None, "computed64 runs to its end");
    assert_eq!(outcome.facts.residual, HIDDEN, "the residual entries of the facts");
    for address in HIDDEN {
        assert!(!outcome.facts.entered.contains(&address), "{address:#x} is no lifted function the run entered");
    }
}
