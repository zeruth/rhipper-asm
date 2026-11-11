//! `ras functions --json` over the MSVC-built core program: its functions carry the names of the
//! link map beside it.

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
fn the_core_program_is_named_from_its_map() {
    let exe = root().join("testdata/rhtest/core-O2.exe");
    let text = ras(&["--json", "functions", exe.to_str().unwrap()]);
    assert!(text.contains("_rhtest_run"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let functions = json["functions"].as_array().unwrap();
    let run = functions.iter().find(|f| f["symbol"] == "_rhtest_run").unwrap_or_else(|| panic!("no _rhtest_run in {text}"));
    assert_eq!(run["name"], "rhtest_run");
    assert!(functions.iter().filter(|f| f["symbol"].is_string()).count() > 20, "{text}");
}
