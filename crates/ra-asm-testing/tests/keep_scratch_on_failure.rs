//! A failing check keeps its scratch directory and names it; a passing one removes its own.

use std::path::PathBuf;
use std::process::Command;

use ra_asm_testing::{KEEP, Scratch};

const SUITE: &str = "ra-asm-testing-keep";

/// The check that fails, run by [`a_failing_check_keeps_its_scratch`] in a process of its own so
/// that its stderr can be read: it writes the port's file and panics with the guard alive.
#[test]
#[ignore = "the failing half of a_failing_check_keeps_its_scratch; runs as its child"]
fn child_fails_with_a_scratch() {
    let scratch = Scratch::new(SUITE, "failed");
    std::fs::write(scratch.join("functions.json"), "[]").unwrap();
    panic!("the check failed");
}

/// Without `RA_KEEP_SCRATCH`, a failing check's output ends with `scratch kept: <path>`, and the
/// path still holds what the check wrote.
#[test]
fn a_failing_check_keeps_its_scratch() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "--nocapture", "child_fails_with_a_scratch"])
        .env_remove(KEEP)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the child passed:\n{stderr}");
    let kept: PathBuf = stderr
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("scratch kept: "))
        .unwrap_or_else(|| panic!("no `scratch kept:` line in the child's stderr:\n{stderr}"))
        .trim()
        .into();
    let failed_at = stderr.find("the check failed").expect("the panic message is in the child's stderr");
    let kept_at = stderr.rfind("scratch kept: ").unwrap();
    assert!(kept_at > failed_at, "the kept line comes before the failure:\n{stderr}");
    assert!(kept.join("functions.json").is_file(), "{} does not hold what the check wrote:\n{stderr}", kept.display());
    std::fs::remove_dir_all(&kept).unwrap();
    let _ = std::fs::remove_dir(kept.parent().unwrap());
}

/// A passing check still removes its scratch, with the suite directory once it is the last.
#[test]
fn a_passing_check_removes_its_scratch() {
    let path = {
        let scratch = Scratch::new(SUITE, "passed");
        std::fs::write(scratch.join("functions.json"), "[]").unwrap();
        scratch.path().to_path_buf()
    };
    assert!(!path.exists(), "{} outlived its passing check", path.display());
}
