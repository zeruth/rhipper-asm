//! `ras pipeline --stats` over a project made of one fixture: every step that works says what it
//! cost, and the status file keeps the numbers of each step with their totals.

use std::path::Path;
use std::process::Command;

fn ras(dir: &Path, step: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .arg("pipeline")
        .arg("--project")
        .arg(dir)
        .args(["--only", step, "--stats"])
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{step} failed:\n{text}");
    text
}

/// The analyze and the lift of `control64.exe` each print a time and a peak, and the status file
/// holds both above zero, the second run keeping what the first wrote.
#[test]
fn the_steps_report_their_time_and_peak() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "stats");
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root.join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe" }"#).unwrap();
    // The peak is the working set on Windows and the resident set under `/proc`; another host has none.
    let measured = cfg!(windows) || cfg!(target_os = "linux");

    for step in ["analyze", "lift"] {
        let text = ras(&dir, step);
        let line =
            text.lines().find(|l| l.starts_with(&format!("{step} took "))).unwrap_or_else(|| panic!("no measure of {step} in:\n{text}"));
        assert!(line.contains(" s, peak "), "{line}");
    }

    let file = dir.join("build/pipeline/status.json");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let status: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
    for step in ["analyze", "lift"] {
        let measure = &status["steps"][step];
        assert!(measure["seconds"].as_f64().is_some_and(|s| s > 0.0), "{step}: {text}");
        if measured {
            assert!(measure["peak_bytes"].as_u64().is_some_and(|p| p > 0), "{step}: {text}");
        }
    }
    let sum = status["steps"]["analyze"]["seconds"].as_f64().unwrap() + status["steps"]["lift"]["seconds"].as_f64().unwrap();
    assert!((status["seconds"].as_f64().unwrap() - sum).abs() < 1e-9, "{text}");
    if measured {
        let peaks = ["analyze", "lift"].map(|step| status["steps"][step]["peak_bytes"].as_u64().unwrap());
        assert_eq!(status["peak_bytes"].as_u64(), peaks.into_iter().max(), "{text}");
    }
}
