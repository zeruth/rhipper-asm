//! `ras functions --json` over the MSVC-built core program: its functions carry the names of the
//! link map beside it. `ras emit --out`: the function table carries the evidence of the ledger in
//! the output directory.

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

/// `op_div` of `control64.exe`, whose call of `fail` at `0x1400016a6` its one vector never ran.
const OP_DIV: &str = "0x140001690";

#[test]
fn the_table_carries_the_evidence_of_the_ledger_in_the_output_directory() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("functions-evidence");
    std::fs::create_dir_all(&out).unwrap();
    let ledger = "ra-asm-verify ledger 1\n\n\
                  function op_div at 0x140001690 bytes 0x0\n  \
                  machine tested 1 vector, 3 of 4 blocks, never b_1400016a6\n  \
                  promoted tested 1 vector, 2 of 2 blocks\n  \
                  c traced\n";
    std::fs::write(out.join("ledger.txt"), ledger).unwrap();
    let exe = root().join("testdata/bin/control64.exe");
    ras(&["emit", exe.to_str().unwrap(), "--out", out.to_str().unwrap(), "--quiet"]);
    let text = std::fs::read_to_string(out.join("functions.json")).unwrap();
    let table: serde_json::Value = serde_json::from_str(&text).unwrap();
    let functions = table.as_array().unwrap();
    let op_div = functions.iter().find(|f| f["addr"] == OP_DIV).unwrap_or_else(|| panic!("no {OP_DIV} in {text}"));
    let evidence = &op_div["evidence"];
    assert_eq!(
        evidence["machine"],
        serde_json::json!({"kind": "tested", "vectors": 1, "blocks": 4, "covered": 3, "never": ["b_1400016a6"]}),
        "{op_div}"
    );
    assert_eq!(
        evidence["promoted"],
        serde_json::json!({"kind": "tested", "vectors": 1, "blocks": 2, "covered": 2, "never": []}),
        "{op_div}"
    );
    assert_eq!(evidence["c"], serde_json::json!({"kind": "traced"}), "{op_div}");
    // The machine level ran three blocks, the promoted two: the machine's test is the stronger.
    assert_eq!(
        op_div["coverage"],
        serde_json::json!({"step": "machine", "vectors": 1, "blocks": 4, "covered": 3, "never": ["b_1400016a6"]}),
        "{op_div}"
    );
    let others = functions.iter().filter(|f| f["addr"] != OP_DIV);
    assert!(others.clone().count() > 5, "{text}");
    for f in others {
        assert!(f.get("evidence").is_none() && f.get("coverage").is_none(), "only op_div is in the ledger: {f}");
    }
}
