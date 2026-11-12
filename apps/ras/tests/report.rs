//! `ras lift --report` over a fixture: the four tables of the coverage report, as text and as the
//! `coverage` of `ras functions --json`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ras(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn the_report_has_its_four_tables() {
    let exe = root().join("testdata/bin/control64.exe");
    let text = ras(&["lift", "--report", "--quiet", exe.to_str().unwrap()]);
    let headings =
        ["uncovered instructions (", "functions by trap count (", "imports without a prototype (", "unresolved indirect jumps ("];
    for heading in headings {
        assert!(text.contains(heading), "no `{heading}` in:\n{text}");
    }
    assert!(!text.contains("define "), "the report replaced the IR:\n{text}");

    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "lift", "--report", "--quiet", exe.to_str().unwrap()])).unwrap();
    for table in ["uncovered", "functions", "unknown_imports", "unresolved_jumps"] {
        assert!(json[table].is_array(), "no `{table}` in {json}");
    }
    let functions: serde_json::Value = serde_json::from_str(&ras(&["--json", "functions", exe.to_str().unwrap()])).unwrap();
    assert_eq!(functions["coverage"], json);
}

/// `ras info` names the toolset of each fixture from its binary: lld's images by their linker
/// version and the absent rich header, the MSVC ones by the rich header's linker and compiler.
#[test]
fn info_names_the_toolset() {
    let toolsets = [
        ("testdata/bin/hello64.exe", "clang/lld"),
        ("testdata/rhtest/core-O2.exe", "MSVC 14.51"),
        ("testdata/rhtest/core-vs2010.exe", "MSVC 16.00"),
    ];
    for (path, name) in toolsets {
        let exe = root().join(path);
        if !exe.exists() {
            eprintln!("{} is not here: its toolset is not checked", exe.display());
            continue;
        }
        let text = ras(&["info", exe.to_str().unwrap()]);
        assert!(text.lines().any(|l| l.starts_with("toolset") && l.contains(name)), "no toolset {name} for {path} in:\n{text}");
        let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info", exe.to_str().unwrap()])).unwrap();
        assert_eq!(json["toolset"]["name"], name, "{path}: {}", json["toolset"]);
    }
}

/// The MSVC-built core program is fully covered: no instruction of it traps.
#[test]
fn the_core_program_has_no_uncovered_instruction() {
    let exe = root().join("testdata/rhtest/core-O2.exe");
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "lift", "--report", "--quiet", exe.to_str().unwrap()])).unwrap();
    assert_eq!(json["uncovered"], serde_json::json!([]), "{json}");
    assert_eq!(json["traps"], 0);
}
