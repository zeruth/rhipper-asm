//! `ras serve` over a fixture: each method answers over stdio, one JSON line per request.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Sends `requests` to a server over the control fixture and returns the responses, in order.
fn ask(args: &[&str], requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ras"))
        .arg("serve")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": 999, "method": "shutdown"})).unwrap();
    drop(stdin);
    let responses: Vec<Value> = BufReader::new(stdout).lines().map(|l| serde_json::from_str(&l.unwrap()).unwrap()).collect();
    assert!(child.wait().unwrap().success());
    responses
}

#[test]
fn the_server_answers_each_method_over_the_image() {
    let exe = root().join("testdata/bin/control64.exe");
    let responses = ask(
        &["--exe", exe.to_str().unwrap()],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "info"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "functions"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "function", "params": {"query": "mainCRTStartup_"}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "disasm", "params": {"address": "0x140001330"}}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "cfg", "params": {"address": "0x140001330"}}),
            json!({"jsonrpc": "2.0", "id": 6, "method": "callers", "params": {"address": "0x140001330"}}),
            json!({"jsonrpc": "2.0", "id": 7, "method": "xrefs", "params": {"lo": "0x140003000", "hi": "0x140003040"}}),
            json!({"jsonrpc": "2.0", "id": 8, "method": "strings", "params": {"min": 4}}),
            json!({"jsonrpc": "2.0", "id": 9, "method": "nothing"}),
            json!({"jsonrpc": "2.0", "id": 10, "method": "project"}),
        ],
    );
    assert_eq!(responses.len(), 11, "{responses:?}");
    assert_eq!(responses[0]["result"]["machine"], "x86_64");
    assert!(responses[1]["result"]["functions"].as_array().is_some_and(|f| f.len() >= 10));
    // The map beside the fixture names its entry; the host's runtime owns the bare name.
    let entry = &responses[2]["result"];
    assert_eq!(entry["name"], "mainCRTStartup_");
    assert!(entry["disasm"].as_str().is_some_and(|d| d.contains("call")));
    assert!(entry["calls"].as_array().is_some_and(|c| !c.is_empty()));
    assert!(responses[3]["result"]["instrs"].as_array().is_some_and(|i| !i.is_empty()));
    assert!(responses[4]["result"]["blocks"].as_array().is_some_and(|b| b.len() >= 3));
    assert!(responses[5]["result"]["references"].as_array().is_some_and(|r| !r.is_empty()));
    assert!(responses[6]["result"]["references"].as_array().is_some_and(|r| !r.is_empty()));
    assert!(responses[7]["result"]["strings"].as_array().is_some_and(|s| !s.is_empty()));
    assert_eq!(responses[8]["error"]["code"], -32601);
    assert!(responses[9]["result"]["exe"].as_str().is_some());
    assert_eq!(responses[10]["id"], 999);
}

#[test]
fn the_server_reads_the_translation_of_a_project() {
    let dir = std::env::temp_dir().join("ras-serve-tests").join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe" }"#).unwrap();
    let emitted =
        Command::new(env!("CARGO_BIN_EXE_ras")).args(["pipeline", "--project"]).arg(&dir).args(["--only", "emit"]).output().unwrap();
    assert!(emitted.status.success());
    let lifted =
        Command::new(env!("CARGO_BIN_EXE_ras")).args(["pipeline", "--project"]).arg(&dir).args(["--only", "lift"]).output().unwrap();
    assert!(lifted.status.success());
    let responses = ask(
        &["--project", dir.to_str().unwrap()],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "project"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "function", "params": {"query": "F_140001330"}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "hand", "params": {"query": "0x140001330", "owned": true}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "function", "params": {"query": "F_140001330"}}),
        ],
    );
    let project = &responses[0]["result"];
    assert_eq!(project["name"], "control");
    assert!(project["outputs"]["functions"].as_str().is_some());
    assert_eq!(project["hand"], json!([]));
    let function = &responses[1]["result"];
    assert_eq!(function["table"]["status"], "lifted");
    assert_eq!(function["hand"], false);
    assert!(function["source"].as_str().is_some_and(|s| s.contains("F_140001330(")), "{function}");
    assert!(function["ir"].as_str().is_some_and(|s| s.starts_with("define ")), "{function}");
    // The ownership toggles per function, in rhipper.json, and the function answers with it.
    assert_eq!(responses[2]["result"]["hand"], json!(["F_140001330"]), "{}", responses[2]);
    assert_eq!(responses[3]["result"]["hand"], true, "{}", responses[3]);
    let json: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("rhipper.json")).unwrap()).unwrap();
    assert_eq!(json["hand"], json!(["F_140001330"]));
    assert_eq!(json["exe"], "ref/control64.exe");
    // A new server sees the list, and taking the function back empties it.
    let responses = ask(
        &["--project", dir.to_str().unwrap()],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "project"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "hand", "params": {"query": "F_140001330", "owned": false}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "project"}),
        ],
    );
    assert_eq!(responses[0]["result"]["hand"], json!(["F_140001330"]), "{}", responses[0]);
    assert_eq!(responses[1]["result"]["hand"], json!([]), "{}", responses[1]);
    assert_eq!(responses[2]["result"]["hand"], json!([]), "{}", responses[2]);
    let json: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("rhipper.json")).unwrap()).unwrap();
    assert!(json.get("hand").is_none(), "{json}");
}
