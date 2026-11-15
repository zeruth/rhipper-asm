//! `ras bisect` over `fib` of `control64.exe`: with no step broken its vectors replay after every
//! step; with the environment breaking `conditions`, the bisection names that step, `fib` and
//! the vector, says the step changed the function, and writes the case of `fib` ahead of the
//! step. That case passes the runner of `crates/ra-asm-lift/tests/cases.rs` (this test's module,
//! as in `case.rs`) and fails it under the same fault, in a process of its own.

#[path = "../../../crates/ra-asm-lift/tests/cases.rs"]
mod cases;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ra_asm_lift::RaiseLevel;
use ra_asm_lift::raise::steps::{self, FAULT};

/// The case the runner of [`the_case_of_the_environment_runs`] reads, in the process the fault's
/// test starts.
const CASE: &str = "RHIPPER_BISECT_CASE";

fn control() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

/// `ras bisect` of `fib`, with the step `fault` broken.
fn ras(args: &[&str], fault: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ras"));
    command.arg("bisect").arg(control()).args(["--function", "fib"]).args(args).env_remove(FAULT).env_remove(CASE);
    if let Some(step) = fault {
        command.env(FAULT, step);
    }
    command.output().unwrap()
}

/// Whether the run could not start on this host, which has no Windows DLLs: the test is skipped.
fn skipped(output: &Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let skip = !output.status.success() && !cfg!(windows) && stderr.contains("no binding for ");
    if skip {
        eprintln!("SKIPPED: control64.exe does not run on a host without the Windows DLLs it imports: no vector to bisect");
    }
    skip
}

/// The steps the bisection replays after, in order: the ABI level's from `promote` on.
fn replayed() -> Vec<&'static str> {
    let order: Vec<&'static str> = steps::steps(RaiseLevel::Abi).iter().map(|s| s.name).collect();
    let from = order.iter().position(|name| *name == "promote").expect("the abi level promotes");
    order[from..].to_vec()
}

#[test]
fn with_no_step_broken_no_step_differs() {
    let output = ras(&[], None);
    if skipped(&output) {
        return;
    }
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}\n{}", String::from_utf8_lossy(&output.stderr));
    let mut expected = vec!["machine ok".to_owned()];
    expected.extend(replayed().iter().map(|step| format!("{step} ok")));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[..lines.len() - 1], expected, "a line per step, in order:\n{text}");
    let last = lines.last().unwrap();
    assert!(last.starts_with("no step differs: 1 functions, ") && last.ends_with(" vectors replay after every step"), "{text}");
}

#[test]
fn a_broken_step_is_named_with_the_function_and_its_case_fails_the_runner() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bisect");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fib.ll");
    let _ = std::fs::remove_file(&file);
    let output = ras(&["--write", file.to_str().unwrap()], Some("conditions"));
    if skipped(&output) {
        return;
    }
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "a step that differs fails the command:\n{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[..2], ["machine ok", "promote ok"], "the steps ahead of the broken one replay:\n{text}");
    let line = lines.get(2).copied().unwrap_or_default();
    assert!(
        line.starts_with("conditions differs: fib at 0x") && line.contains(", vector "),
        "the line names the step, the function and the vector: {line}"
    );
    assert!(line.contains(&format!("{:#x}", steps::FAULTY)), "the line holds the value the broken step returns: {line}");
    assert_eq!(lines.get(3).copied(), Some("conditions changes fib"), "{text}");
    assert_eq!(lines.get(4).copied(), Some(format!("the case: {}", file.display()).as_str()), "{text}");
    assert_eq!(lines.len(), 5, "the bisection stops at the first step that differs:\n{text}");

    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.lines().any(|line| line == "; pass: conditions"), "{written}");
    let case = cases::Case::parse("control64/fib", &written).unwrap_or_else(|refusal| panic!("{refusal}\n{written}"));
    assert_eq!(cases::run(&case), Ok(()), "the step as it is keeps what fib does:\n{written}");

    let runner = Command::new(std::env::current_exe().unwrap())
        .args(["the_case_of_the_environment_runs", "--exact", "--nocapture", "--test-threads=1"])
        .env(FAULT, "conditions")
        .env(CASE, &file)
        .output()
        .unwrap();
    let said = format!("{}{}", String::from_utf8_lossy(&runner.stdout), String::from_utf8_lossy(&runner.stderr));
    assert!(!runner.status.success(), "the case passes the runner with `conditions` broken:\n{said}");
    assert!(
        said.contains("control64/fib: `@fib(") && said.contains("through `conditions`: "),
        "the runner names the case and the call:\n{said}"
    );
}

/// The runner over the case the environment names, with the step the environment breaks: the
/// process the test above starts. Nothing without a case.
#[test]
fn the_case_of_the_environment_runs() {
    let Some(path) = std::env::var_os(CASE) else { return };
    let text = std::fs::read_to_string(&path).unwrap();
    let case = cases::Case::parse("control64/fib", &text).unwrap_or_else(|refusal| panic!("{refusal}"));
    if let Err(failure) = cases::run(&case) {
        panic!("{failure}");
    }
}
