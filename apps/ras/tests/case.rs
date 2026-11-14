//! `ras case`: the case it writes for a function of `control64.exe` reads as a pass case and
//! passes the runner of `crates/ra-asm-lift/tests/cases.rs`, which is this test's module: the
//! check is the runner's own, and its tests over `testdata/cases` run here beside these.

#[path = "../../../crates/ra-asm-lift/tests/cases.rs"]
mod cases;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn control() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

fn ras(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).arg("case").arg(control()).args(args).output().unwrap()
}

/// `fib` takes an argument, branches on it and calls itself: the case written ahead of
/// `conditions` names the step, takes a call from the run's vectors, holds the one function, and
/// agrees with itself through the step in the runner.
#[test]
fn the_case_of_a_function_before_conditions_passes_the_runner() {
    let output = ras(&["fib", "--pass", "conditions"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && !cfg!(windows) && stderr.contains("no binding for ") {
        eprintln!("SKIPPED: control64.exe does not run on a host without the Windows DLLs it imports: no vector, no case");
        return;
    }
    assert!(output.status.success(), "{stderr}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.lines().any(|line| line == "; pass: conditions"), "{text}");
    let calls: Vec<&str> = text.lines().filter_map(|line| line.strip_prefix("; call: ")).collect();
    assert!(!calls.is_empty(), "{text}");
    assert!(calls.iter().all(|call| call.starts_with("@fib(") && call.ends_with(')')), "{calls:?}");
    assert_eq!(text.lines().filter(|line| line.starts_with("define ")).count(), 1, "{text}");

    let case = cases::Case::parse("control64/fib", &text).unwrap_or_else(|refusal| panic!("{refusal}\n{text}"));
    assert_eq!(cases::run(&case), Ok(()), "{text}");
}

/// What no case is written for is refused by name: a step that is none of the raise's, a step
/// handed a function over the machine state, and a function the image does not have.
#[test]
fn a_step_or_a_function_without_a_case_is_refused() {
    let refusal = |args: &[&str]| {
        let output = ras(args);
        assert!(!output.status.success(), "`ras case {}` wrote a case", args.join(" "));
        assert!(output.stdout.is_empty(), "a refusal prints no case");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let unknown = refusal(&["fib", "--pass", "none"]);
    assert!(unknown.contains("no step `none`") && unknown.contains("conditions"), "{unknown}");
    let machine = refusal(&["fib", "--pass", "promote"]);
    assert!(machine.contains("no case is written ahead of `promote`"), "{machine}");
    let missing = refusal(&["no_such_function", "--pass", "conditions"]);
    assert!(missing.contains("no_such_function is no function of the image"), "{missing}");
}
