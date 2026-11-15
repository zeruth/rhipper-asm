//! The rhtest programs through the interpreter: a program lifted, raised to the ABI level and run,
//! its output and its exit compared with the native run's reference, in the shape the reference
//! takes (`exit = <code>` after what the program printed).
//!
//! And computed through the interpreter with the residual engine behind it: the three functions it
//! reaches only by addresses it computes are no lifted function, and the engine runs each from
//! the run's memory, so the program runs to its end with the native program's output.
//!
//! And hello32 packed by upx through the interpreter, which runs the unpacking stub and stops
//! where it jumps into the code it wrote.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ra_asm_lift::run::Outcome;

/// `h_blend`, `h_fold` and `h_weigh`, by the map beside the fixture.
const HIDDEN: [u64; 3] = [0x1_4000_108c, 0x1_4000_1092, 0x1_4000_109a];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rhtest() -> PathBuf {
    root().join("testdata/rhtest")
}

/// Runs `<name>-<variant>.exe` at the ABI level and compares what it printed and how it ended with
/// its reference: the variant's own when it has one, else the program's.
fn run_abi(name: &str, variant: &str) {
    let dir = rhtest();
    let binary = format!("{name}-{variant}.exe");
    let own = dir.join(format!("{name}-{variant}.txt"));
    let reference = if own.is_file() { own } else { dir.join(format!("{name}.txt")) };
    let expected = std::fs::read_to_string(&reference).unwrap_or_else(|e| panic!("{}: {e}", reference.display())).replace("\r\n", "\n");

    let image = ra_asm_pe::load(&dir.join(&binary)).unwrap_or_else(|e| panic!("{binary}: {e:#}"));
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    // The traps of code the run does not reach are the lift's to report, not this run's to fail.
    for warning in &lifted.warnings {
        eprintln!("{binary}: {warning}");
    }

    let outcome = ra_asm_host::run(&image, &lifted, &signatures, &ra_asm_host::Options::default()).unwrap();
    let mut actual = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    let _ = writeln!(actual, "exit = {}", outcome.exit);
    let test = format!("{binary} (abi): interpreted");
    let failed = ra_asm_testing::digest::failed_dir();
    if let Err(line) = ra_asm_testing::digest::compare_in(&failed, &test, expected.trim_end(), actual.trim_end()) {
        eprintln!("{test}: stop: {:?}, {} steps", outcome.stop, outcome.steps);
        panic!("{line}");
    }
}

/// The access violations of seh.c caught by their `__try`: the filter reads the record the fault
/// left and the `__except` block runs. The second `__try` holds no call, so clang protects no
/// range of it, and its store ends the program as it ends the native run.
#[test]
fn seh_clang_runs_interpreted_at_the_abi_level() {
    run_abi("seh", "clang");
}

/// eh.cpp's throws caught by type at the machine level, the first five of its groups: a throw
/// lands in the catch its type picks, the unwind destroys the guards it passes, and the end of a
/// catch destroys the thrown object, so each group's result and its events checksum are the native
/// run's. The groups past them run in [`eh_clang`], at the ABI level.
#[test]
fn eh_clang_catches_by_type() {
    let dir = rhtest();
    let expected = std::fs::read_to_string(dir.join("eh.txt")).unwrap().replace("\r\n", "\n");
    let first: Vec<&str> = expected.lines().take(10).collect();
    let image = ra_asm_pe::load(&dir.join("eh-clang.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions::default());
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let outcome = ra_asm_host::run(&image, &lifted, &signatures, &ra_asm_host::Options::default()).unwrap();
    let actual = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    let got: Vec<&str> = actual.lines().take(10).collect();
    assert_eq!(got, first, "eh-clang.exe at the machine level (stop: {:?}, {} steps)", outcome.stop, outcome.steps);
}

/// eh.cpp's throws through the x86 frame handler's protocol: the try level the code stores in its
/// registration picks the landing, the unwind map's actions destroy the guards a throw passes, and
/// the end of a catch destroys the thrown object. A rethrow passes the caught object on, a throw
/// from a catch unwinds it with the `esp` it began with, and a `RaiseException` under a
/// `__try`/`__finally` runs the `__finally` on its way to the `__except` further out.
#[test]
fn eh_clang() {
    run_abi("eh", "clang");
}

/// `testdata/bin/<binary>` lifted at the machine level and run with the residual engine under
/// `options`; none when this host has no Windows DLLs to answer its imports, which says so on
/// stderr.
fn run_machine(binary: &str, options: &ra_asm_host::Options) -> Option<Outcome> {
    let image = ra_asm_pe::load(&root().join("testdata/bin").join(binary)).unwrap_or_else(|e| panic!("{binary}: {e:#}"));
    let analysis = ra_asm_analysis::analyze(&image);
    let lift = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, lift);
    let raise = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, raise);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let outcome = ra_asm_host::run(&image, &lifted, &signatures, options).unwrap();
    if let Some(stop) = &outcome.stop
        && stop.starts_with("no binding for ")
        && !cfg!(windows)
    {
        eprintln!("SKIPPED: {binary}: {stop}; this host has no Windows DLLs: the interpreted run is skipped");
        return None;
    }
    Some(outcome)
}

/// computed64 lifted at the machine level and run with the residual engine.
fn run_computed() -> Option<Outcome> {
    run_machine("computed64.exe", &ra_asm_lift::run::Options::default())
}

/// hello32 packed by upx, run at the machine level until its stub, having unpacked hello32 into
/// the image, jumps into it: the run stops there, at hello32's own entry point, which the packed
/// image's lift holds no code for.
#[test]
fn upx_stops_at_the_entry() {
    let twin = ra_asm_pe::load(&root().join("testdata/bin/hello32.exe")).unwrap();
    let entry = twin.entry_point().expect("hello32 has an entry point");
    let options = ra_asm_host::Options { unpack: true, limit: 500_000_000, ..ra_asm_host::Options::default() };
    let Some(outcome) = run_machine("hello32-upx.exe", &options) else { return };
    assert_eq!(
        outcome.unpacked,
        Some(entry),
        "where hello32-upx stopped (exit {:#x}, stop: {:?}, {} steps)",
        outcome.exit,
        outcome.stop,
        outcome.steps
    );
    assert!(outcome.output.is_empty(), "the stub prints nothing before hello32 runs");
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
