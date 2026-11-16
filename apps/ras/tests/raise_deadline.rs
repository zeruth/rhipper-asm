//! `--deadline`: `ras lift`, `ras emit` and `ras case` stop a raise past it with a message that
//! names the function and the step it was at, and exit non-zero; a deadline the raise keeps
//! changes nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn control() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

fn ras(args: &[&str]) -> Output {
    let (command, rest) = args.split_first().unwrap();
    Command::new(env!("CARGO_BIN_EXE_ras")).arg(command).arg(control()).args(rest).output().unwrap()
}

/// The function and the step of the message of a passed deadline of `seconds`, when `stderr`
/// holds one.
fn passed(stderr: &str, seconds: u64) -> Option<(String, String)> {
    let prefix = format!("ras: deadline of {seconds} s passed in ");
    let rest = stderr.lines().find_map(|line| line.strip_prefix(prefix.as_str()))?;
    let (function, step) = rest.rsplit_once(" at ")?;
    Some((function.to_string(), step.to_string()))
}

/// A deadline of no seconds has passed at the raise's first look: each command stops there, prints
/// nothing of its output, and names a function and a step.
#[test]
fn a_passed_deadline_stops_the_raise_and_names_where() {
    for args in [
        &["lift", "--deadline", "0"][..],
        &["lift", "--level", "promoted", "--deadline", "0"],
        &["emit", "--quiet", "--deadline", "0"],
        &["case", "fib", "--pass", "conditions", "--deadline", "0"],
    ] {
        let output = ras(args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "`ras {}` ran on past its deadline\n{stderr}", args.join(" "));
        let (function, step) = passed(&stderr, 0).unwrap_or_else(|| panic!("`ras {}` named no deadline\n{stderr}", args.join(" ")));
        assert!(!function.is_empty() && !step.is_empty(), "{stderr}");
        assert!(!step.contains(' '), "the step `{step}` is no step's name\n{stderr}");
        assert_eq!(stderr.matches("ras: deadline of").count(), 1, "the deadline was named more than once\n{stderr}");
    }
}

/// A deadline the raise keeps lets the lift run through and print its IR.
#[test]
fn a_kept_deadline_changes_nothing() {
    let output = ras(&["lift", "--deadline", "3600"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(passed(&stderr, 3600).is_none(), "{stderr}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.lines().any(|line| line.starts_with("define ")), "the lift printed no function\n{stderr}");
}
