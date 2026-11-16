//! The emit tests' scratch sits under the workspace's `var/ra-asm-emit-tests`, not the system
//! temp, and a failing case names it relative to the workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_testing::{KEEP, Scratch};

const SUITE: &str = "ra-asm-emit-tests";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// The scratch of an emit test, made as `tests/oracle.rs` makes it.
fn scratch(name: &str) -> Scratch {
    Scratch::in_workspace(Path::new(env!("CARGO_MANIFEST_DIR")), SUITE, name)
}

/// The failing case, run by [`a_failing_case_names_its_scratch_in_the_workspace`] in a process of
/// its own so that its stderr can be read.
#[test]
#[ignore = "the failing half of a_failing_case_names_its_scratch_in_the_workspace; runs as its child"]
fn child_fails_with_a_scratch() {
    let dir = scratch("failed");
    std::fs::write(dir.join("functions.json"), "[]").unwrap();
    panic!("the case failed in {}", dir.shown().display());
}

/// A failing case's panic and its kept line name `var/ra-asm-emit-tests/...`, relative to the
/// workspace, and that directory holds what the case wrote.
#[test]
fn a_failing_case_names_its_scratch_in_the_workspace() {
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
    let path = workspace().join(&kept);
    let in_workspace = kept.is_relative() && kept.starts_with(Path::new("var").join(SUITE));
    // The kept directory goes before an assertion can fail, so no run leaves it behind.
    let written = path.join("functions.json").is_file();
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_dir(path.parent().unwrap());
    assert!(in_workspace, "{} is not under the workspace's var/{SUITE}:\n{stderr}", kept.display());
    assert!(written, "{} does not hold what the case wrote:\n{stderr}", path.display());
    assert!(stderr.contains(&format!("the case failed in {}", kept.display())), "the panic does not name {}:\n{stderr}", kept.display());
    assert!(!path.starts_with(std::env::temp_dir()), "{} is under the system temp", path.display());
}

/// A passing case's scratch is under the workspace and goes with its guard.
#[test]
fn a_passing_case_removes_its_scratch() {
    let path = {
        let dir = scratch("passed");
        assert!(dir.starts_with(workspace().join("var").join(SUITE)), "{} is not under the workspace's var/{SUITE}", dir.display());
        assert_eq!(workspace().join(dir.shown()), dir.path());
        std::fs::write(dir.join("functions.json"), "[]").unwrap();
        dir.path().to_path_buf()
    };
    assert!(!path.exists(), "{} outlived its passing case", path.display());
}
