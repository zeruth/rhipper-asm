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

/// A project directory with the fixture as its reference binary, whose run step compares the
/// port with it.
fn project(name: &str) -> PathBuf {
    project_with(name, r#"{ "name": "control", "exe": "ref/control64.exe", "run": { "compare": true } }"#)
}

fn project_with(name: &str, rhipper_json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ras-pipeline-tests").join(format!("{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), rhipper_json).unwrap();
    dir
}

fn ras(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg("pipeline").arg("--project").arg(dir).args(args).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

/// `ras doctor --json` lists the tools of this machine; clang is among them, with a version when
/// it is on the PATH.
#[test]
fn doctor_lists_the_tools_as_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(["doctor", "--json"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let report: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
    let tools = report["tools"].as_array().unwrap();
    let clang = tools.iter().find(|t| t["name"] == "clang").unwrap_or_else(|| panic!("no clang in {text}"));
    assert_eq!(clang["for"], "the C compiler of a C or C++ port");
    assert_eq!(clang["required"], true);
    if available("clang") {
        assert!(clang["version"].as_str().is_some_and(|v| v.contains('.')), "{clang}");
    } else {
        assert!(clang["version"].is_null(), "{clang}");
    }
    for name in ["clang++", "lld", "cmake", "ninja", "visual studio", "cargo", "llvm-symbolizer"] {
        assert!(tools.iter().any(|t| t["name"] == name), "no {name} in {text}");
    }
    assert_eq!(report["ok"].as_bool().unwrap(), output.status.success());
}

/// With no tool on the PATH, the build step fails with the doctor's message, not `CMake`'s.
#[test]
fn the_build_step_names_the_missing_tools() {
    let dir = project("doctor");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["pipeline", "--project"])
        .arg(&dir)
        .args(["--only", "build"])
        .env("PATH", "")
        .env_remove("ProgramFiles(x86)")
        .env_remove("ProgramFiles")
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("missing: clang (") && text.contains("cmake (") && text.contains("] build failed: stopping"), "{text}");
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
    // The reference ran beside the port and both printed the same.
    assert_eq!(record["verdict"], "same", "{record}");
    assert_eq!(record["reference_exit"], "0", "{record}");
    assert!(record["differs_at"].is_null(), "{record}");
    assert!(text.contains("verdict: same"), "{text}");
    let reference_log = std::fs::read_to_string(dir.join("build/pipeline/reference.log")).unwrap().replace("\r\n", "\n");
    assert!(reference_log.contains(expected.trim_end()), "the reference log:\n{reference_log}");
    // A second run reuses the configured build directory.
    let (ok, text) = ras(&dir, &["--from", "build"]);
    assert!(ok, "{text}");
}

/// A Rust project: the emit step writes a cargo package, the build step runs cargo, and the port
/// prints what the reference prints.
#[test]
fn the_pipeline_builds_a_rust_port() {
    if !available("cargo") {
        eprintln!("cargo is not on the PATH: the Rust pipeline is skipped");
        return;
    }
    let dir = project_with("rust", r#"{ "name": "control", "exe": "ref/control64.exe", "lang": "rust" }"#);
    let (ok, text) = ras(&dir, &[]);
    assert!(ok, "{text}");
    for file in [
        "src/gen/Cargo.toml",
        "src/gen/src/main.rs",
        "src/gen/src/rhipper.rs",
        "src/gen/src/data.rs",
        "src/gen/src/program.rs",
        "build/functions.json",
    ] {
        assert!(dir.join(file).is_file(), "{file} is missing");
    }
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
    assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["file"] == "src/program.rs")), "{table}");
    assert!(dir.join("build/cargo/release/control.exe").is_file(), "no port under build/cargo");
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
    assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
}
