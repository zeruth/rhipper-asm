//! `ras smoke`: a fixture goes through every stage, a file that is no image is refused rather
//! than a crash, and (ignored, for a run by hand or the nightly) the Windows system directory's
//! executables go through without a panic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn smoke(args: &[&str]) -> (Output, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(["--json", "smoke", "--quiet"]).args(args).output().unwrap();
    let report =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("no report ({e}):\n{}", String::from_utf8_lossy(&output.stderr)));
    (output, report)
}

#[test]
fn a_fixture_goes_through_every_stage() {
    let exe = root().join("testdata/rhtest/core-O2.exe");
    let (output, report) = smoke(&[exe.to_str().unwrap()]);
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["files"], 1, "{report:#}");
    assert_eq!(report["status"]["ok"], 1, "{report:#}");
    assert_eq!(report["coverage"]["traps"], 0, "{report:#}");
    assert!(report["coverage"]["functions"].as_u64().unwrap() > 0, "{report:#}");
    let slowest = &report["slowest"][0];
    assert_eq!(slowest["stage"], "emit", "{report:#}");
    for stage in ["load", "analyze", "lift", "coverage", "raise", "emit"] {
        assert!(slowest["times"][stage].is_number(), "no time for {stage}: {report:#}");
    }
}

#[test]
fn a_file_that_is_no_image_is_refused() {
    let root = root();
    let (output, report) = smoke(&[root.to_str().unwrap(), "--depth", "0", "--ext", "toml"]);
    assert!(output.status.success(), "{report:#}");
    assert!(report["files"].as_u64().unwrap() > 0, "{report:#}");
    assert_eq!(report["status"]["error"], report["files"], "{report:#}");
    assert_eq!(report["errors"][0]["stage"], "load", "{report:#}");
}

/// The executables of the Windows system directory: every one is lifted and emitted, refused,
/// or stopped at the time limit, and none panics or crashes. The run is held to a budget that
/// ends it, smallest files first, well inside a check's half hour on a debug build; the files it
/// leaves are reported as skipped, and the nightly runs them all.
#[test]
#[ignore = "minutes over the system directory; run with `cargo test -p ras -q -- --ignored`"]
fn the_system_directory_does_not_panic() {
    let Some(windows) = std::env::var_os("SystemRoot") else {
        eprintln!("no SystemRoot: not Windows, the system directory smoke is skipped");
        return;
    };
    let system = PathBuf::from(windows).join("System32");
    let mut args = vec![system.to_str().unwrap()];
    args.extend(["--depth", "0", "--ext", "exe", "--timeout", "10", "--jobs", "3", "--budget", "1080"]);
    let (output, report) = smoke(&args);
    assert!(report["files"].as_u64().unwrap() > 0, "no executable in {}", system.display());
    let status = &report["status"];
    let run: u64 = ["ok", "error", "timeout"].into_iter().map(|s| status[s].as_u64().unwrap()).sum();
    eprintln!("{} executables: {status}", report["files"]);
    assert!(run > 0, "no executable ran: {status}");
    assert_eq!(report["panics"], serde_json::json!([]), "{:#}", report["panics"]);
    assert_eq!(report["crashes"], serde_json::json!([]), "{:#}", report["crashes"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
