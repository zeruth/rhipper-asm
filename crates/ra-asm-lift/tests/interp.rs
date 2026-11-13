//! The interpreted oracle: a fixture is lifted, run through the IR interpreter, and its output is
//! compared with the native program's. It needs no clang and starts no process, so it is the fast
//! tier; the IR gate and the clang runs of the same fixtures are in `oracle.rs`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("testdata/bin").join(name)
}

/// The one skip a missing tool does not explain: a program of another system, which this host
/// cannot run. On a host that is not `native` it says on stderr why `what` is skipped, and answers
/// whether it is.
fn foreign_host(native: bool, program: &str, why: &str, what: &str) -> bool {
    if !native {
        eprintln!("SKIPPED: {program}: {why}: {what} is skipped");
    }
    !native
}

/// Fails `test` with the digest's one line when its output differs from the expected one; how
/// the run ended goes to stderr before it, and both whole sides are under `out/failed`. The blank
/// end of either side does not count.
fn assert_output(test: &str, expected: &str, actual: &str, ended: &str) {
    let failed = ra_asm_testing::digest::failed_dir();
    if let Err(line) = ra_asm_testing::digest::compare_in(&failed, test, expected.trim_end(), actual.trim_end()) {
        eprintln!("{test}: {ended}");
        panic!("{line}");
    }
}

/// A fixture lifted and raised to the level the tag names.
struct Raised {
    image: ra_asm_pe::Image,
    lifted: ra_asm_lift::Lifted,
    signatures: std::collections::HashMap<String, ra_asm_lift::raise::abi::Signature>,
}

/// Lifts `binary` to the level and the x87 unit the tag names. The machine IR, the promoted IR
/// and the ABI IR must all run; the tag picks one. The ABI IR stands alone: its data are globals
/// and it never maps the image. The exact x87 unit when the tag asks for it.
fn raise(binary: &str, tag: &str) -> Raised {
    let image = ra_asm_pe::load(&fixture(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let level = if tag.contains("machine") {
        ra_asm_lift::RaiseLevel::Machine
    } else if tag.contains("promoted") {
        ra_asm_lift::RaiseLevel::Promoted
    } else {
        ra_asm_lift::RaiseLevel::Abi
    };
    let x87 = if tag.contains("exact") { ra_asm_lift::X87Unit::Exact } else { ra_asm_lift::X87Unit::Double };
    let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87 };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    Raised { image, lifted, signatures }
}

/// A host without the Windows DLLs answers only the imports the interpreter runs itself: a run
/// stopped at another one says so and is skipped there.
fn stopped_for_dlls(binary: &str, outcome: &ra_asm_lift::run::Outcome) -> bool {
    match &outcome.stop {
        Some(stop) if stop.starts_with("no binding for ") => {
            foreign_host(cfg!(windows), binary, &format!("{stop}; this host has no Windows DLLs"), "the interpreted run")
        }
        _ => false,
    }
}

/// Runs `binary` at the tag's level through the IR interpreter and compares its output with the
/// native program's: the same proof as the clang run, without clang.
fn run_interpreted(binary: &str, expected: &str, tag: &str) {
    let Raised { image, lifted, signatures } = raise(binary, tag);
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let options = ra_asm_lift::run::Options::default();
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &options).unwrap();
    if stopped_for_dlls(binary, &outcome) {
        return;
    }
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output(&format!("{binary} ({tag}): interpreted"), expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!(outcome.exit, 0, "{binary} ({tag}): the interpreted program's exit (stop: {:?})", outcome.stop);
}

/// Runs `binary` through the interpreter at each of `tags`, against `testdata/expected/<expected>`.
fn run_levels(binary: &str, expected: &str, tags: &[&str]) {
    let expected = std::fs::read_to_string(root().join("testdata/expected").join(expected)).unwrap();
    for tag in tags {
        run_interpreted(binary, &expected, tag);
    }
}

/// The machine, promoted and ABI levels, which an x86 fixture runs at unless it says otherwise.
const LEVELS: [&str; 3] = ["run-machine", "run-promoted", "run-abi"];

/// The ARM64 build of hello runs lifted through the IR interpreter, over the mapped image and
/// standalone: the host is x64, so the interpreter is its ARM64 machine. The standalone run is
/// the ABI level, raised by AAPCS64.
#[test]
fn helloa64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("helloa64.exe", &expected, tag);
    }
}

/// The ARM64 build of the float fixture runs through the interpreter with the native program's
/// output.
#[test]
fn floata64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("floata64.exe", &expected, tag);
    }
}

/// The Thumb-2 fixture, a static 32-bit ARM Linux program, runs through the interpreter over the
/// mapped image with the output its source prints.
#[test]
fn thumb32_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/thumb.txt")).unwrap();
    run_interpreted("thumb32", &expected, "run-machine");
}

/// The ARM64 Linux hello runs through the interpreter with the x64 build's native output.
#[test]
fn hello_linux_a64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello_linux.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("hello_linux_a64", &expected, tag);
    }
}

/// The homebrew XBE: a 32-bit x86 program whose imports are the kernel's, by ordinal, named from
/// the loader's table; the interpreter runs it at every level and answers the kernel calls. The
/// clang run waits for the kernel's platform runtime.
#[test]
fn xbe32_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/xbe.txt")).unwrap();
    let raised = raise("xbe32.xbe", "run-abi");
    for name in ["DbgPrint", "ExAllocatePool", "ExFreePool", "HalReturnToFirmware"] {
        assert!(raised.lifted.imports.contains(name), "the lift names the kernel import {name}: {:?}", raised.lifted.imports);
    }
    for tag in ["run-machine", "run-promoted", "run-abi"] {
        run_interpreted("xbe32.xbe", &expected, tag);
    }
}

#[test]
fn hello64_runs_interpreted() {
    run_levels("hello64.exe", "hello.txt", &LEVELS);
}

#[test]
fn hello32_runs_interpreted() {
    run_levels("hello32.exe", "hello.txt", &LEVELS);
}

#[test]
fn float64_runs_interpreted() {
    run_levels("float64.exe", "float.txt", &LEVELS);
}

#[test]
fn float32_runs_interpreted() {
    run_levels("float32.exe", "float.txt", &LEVELS);
}

/// The x87 program under the exact unit.
#[test]
fn extended32_runs_interpreted_exact() {
    run_levels("extended32.exe", "extended.txt", &["run-machine-exact", "run-promoted-exact", "run-abi-exact"]);
}

#[test]
fn control64_runs_interpreted() {
    run_levels("control64.exe", "control.txt", &LEVELS);
}

#[test]
fn control32_runs_interpreted() {
    run_levels("control32.exe", "control.txt", &LEVELS);
}

#[test]
fn obfusc64_runs_interpreted() {
    run_levels("obfusc64.exe", "obfusc.txt", &LEVELS);
}

#[test]
fn obfusc32_runs_interpreted() {
    run_levels("obfusc32.exe", "obfusc.txt", &LEVELS);
}

#[test]
fn classes64_runs_interpreted() {
    run_levels("classes64.exe", "classes.txt", &LEVELS);
}

#[test]
fn classes32_runs_interpreted() {
    run_levels("classes32.exe", "classes.txt", &LEVELS);
}

#[test]
fn throws64_runs_interpreted() {
    run_levels("throws64.exe", "throws.txt", &LEVELS);
}

#[test]
fn inline64_runs_interpreted() {
    run_levels("inline64.exe", "inline.txt", &LEVELS);
}

#[test]
fn inline32_runs_interpreted() {
    run_levels("inline32.exe", "inline.txt", &LEVELS);
}

#[test]
fn files64_runs_interpreted() {
    run_levels("files64.exe", "files.txt", &LEVELS);
}

/// A thread kernel32 starts enters the lifted code through its native entry, which only the ABI
/// level has, so the run is at that level alone.
#[test]
fn tls64_runs_interpreted() {
    run_levels("tls64.exe", "tls.txt", &["run-abi"]);
}

#[test]
fn tls32_runs_interpreted() {
    run_levels("tls32.exe", "tls.txt", &["run-abi"]);
}

/// A program over the static C runtime starts at `main`, which the ABI level alone has.
#[test]
fn crt64_runs_interpreted() {
    run_levels("crt64.exe", "crt.txt", &["run-abi"]);
}

/// The resources are read in the image's own resource directory.
#[test]
fn resources64_runs_interpreted() {
    run_levels("resources64.exe", "resources.txt", &LEVELS);
}

#[test]
fn structs64_runs_interpreted() {
    run_levels("structs64.exe", "structs.txt", &LEVELS);
}

#[test]
fn structs32_runs_interpreted() {
    run_levels("structs32.exe", "structs.txt", &LEVELS);
}

/// The dispatch program without facts: every call through its table stays an indirect call.
#[test]
fn dispatch64_runs_interpreted() {
    run_levels("dispatch64.exe", "dispatch.txt", &LEVELS);
}

#[test]
fn dispatch32_runs_interpreted() {
    run_levels("dispatch32.exe", "dispatch.txt", &LEVELS);
}
