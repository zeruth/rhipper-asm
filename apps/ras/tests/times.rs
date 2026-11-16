//! `ras emit --times`: a line of seconds on stderr for each stage of the emit, so a slow stretch of
//! a large program shows by its stage.

use std::path::Path;
use std::process::Command;

#[test]
fn an_emit_with_times_prints_a_line_per_stage() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("emit-times");
    std::fs::create_dir_all(&out).unwrap();
    let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["emit", exe.to_str().unwrap(), "--out", out.to_str().unwrap(), "--quiet", "--times"])
        .env_remove("RA_STEP_TIMES")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let stages: Vec<&str> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("ra: time: "))
        .map(|l| {
            let (stage, seconds) = l.rsplit_once(": ").unwrap_or_else(|| panic!("no seconds in `{l}`"));
            let seconds = seconds.strip_suffix('s').and_then(|s| s.parse::<f64>().ok());
            assert!(seconds.is_some_and(|s| s >= 0.0), "no seconds in `{l}`");
            stage
        })
        .collect();
    for stage in [
        "analysis",
        "lift",
        "raise",
        "raise: functions",
        "apply_names",
        "translate",
        "translate: infos",
        "translate: module parts",
        "translate: functions",
        "translate: unit",
        "translate: files",
        "note_evidence",
        "format",
        "write",
        "regenerate",
    ] {
        assert!(stages.contains(&stage), "no time for {stage}: {stderr}");
    }
    // The inference and the module steps of the raise time one each, beside its functions.
    assert!(stages.iter().filter(|s| s.starts_with("raise: ")).count() >= 2, "{stderr}");
}

/// The large stages each give the peak working set as they end, which only grows; a host that
/// tells no peak (neither Windows nor `/proc`) prints none.
#[test]
fn an_emit_with_times_prints_its_peak_after_each_large_stage() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("emit-peaks");
    std::fs::create_dir_all(&out).unwrap();
    let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["emit", exe.to_str().unwrap(), "--out", out.to_str().unwrap(), "--quiet", "--times"])
        .env_remove("RA_STEP_TIMES")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    if !(cfg!(windows) || cfg!(target_os = "linux")) {
        return;
    }
    let peaks: Vec<(&str, u64)> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("ra: peak: "))
        .map(|l| {
            let (stage, megabytes) = l.rsplit_once(": ").unwrap_or_else(|| panic!("no peak in `{l}`"));
            let megabytes = megabytes.strip_suffix(" MB").and_then(|m| m.parse::<u64>().ok());
            (stage, megabytes.unwrap_or_else(|| panic!("no megabytes in `{l}`")))
        })
        .collect();
    let stages: Vec<&str> = peaks.iter().map(|(stage, _)| *stage).collect();
    assert_eq!(stages, ["analysis", "lift", "raise", "translate", "header", "identities", "format", "write", "regenerate"], "{stderr}");
    assert!(peaks[0].1 > 0, "{stderr}");
    assert!(peaks.windows(2).all(|w| w[0].1 <= w[1].1), "a peak fell: {stderr}");
}

#[test]
fn an_emit_without_times_prints_none() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("emit-no-times");
    std::fs::create_dir_all(&out).unwrap();
    let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["emit", exe.to_str().unwrap(), "--out", out.to_str().unwrap(), "--quiet"])
        .env_remove("RA_STEP_TIMES")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("ra: time: "), "{stderr}");
    assert!(!stderr.contains("ra: peak: "), "{stderr}");
}
