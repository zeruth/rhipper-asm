//! `ras pipeline` over a project made of one fixture: every step runs, every output lands where
//! the workbench looks for it, and the port prints what the reference prints.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn available(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
}

/// A project directory with the fixture as its reference binary.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ras-pipeline-tests").join(format!("{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe" }"#).unwrap();
    dir
}

fn ras(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg("pipeline").arg("--project").arg(dir).args(args).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

#[test]
fn emit_writes_the_translation_and_the_table() {
    let dir = project("emit");
    let (ok, text) = ras(&dir, &["--only", "emit"]);
    assert!(ok, "{text}");
    assert!(text.contains("] emit\n") && text.contains("] done\n"), "{text}");
    for file in [
        "src/gen/program.c",
        "src/gen/image.h",
        "src/gen/runtime/rhipper.h",
        "src/gen/runtime/driver.c",
        "build/functions.json",
        "build/pipeline/emit.log",
    ] {
        assert!(dir.join(file).is_file(), "{file} is missing");
    }
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
    assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["name"] == "entry" && e["status"] == "lifted")), "{table}");
}

#[test]
fn the_whole_pipeline_builds_and_runs_the_port() {
    if !(available("cmake") && available("clang")) {
        eprintln!("cmake or clang is not on the PATH: the build and the run are skipped");
        return;
    }
    let dir = project("all");
    let (ok, text) = ras(&dir, &[]);
    assert!(ok, "{text}");
    for step in ["analyze", "lift", "emit", "build", "run"] {
        assert!(text.contains(&format!("] {step}\n")), "no progress line for {step}:\n{text}");
        assert!(dir.join(format!("build/pipeline/{step}.log")).is_file(), "no {step}.log");
    }
    assert!(text.ends_with("] done\n"), "{text}");
    assert!(dir.join("CMakeLists.txt").is_file() && dir.join("build/program.ll").is_file());
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
    assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
    let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
    let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
    assert_eq!(record["exit"], "0", "{record}");
    assert_eq!(record["alive"], false);
    assert!(record["when"].as_str().is_some_and(|w| w.ends_with('Z')));
    // A second run reuses the configured build directory.
    let (ok, text) = ras(&dir, &["--from", "build"]);
    assert!(ok, "{text}");
}
