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
fn the_tools_a_model_asks_answer_from_the_facts() {
    let exe = root().join("testdata/bin/control64.exe");
    let args = ["--exe", exe.to_str().unwrap()];
    let responses = ask(
        &args,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "explain", "params": {"query": "mainCRTStartup_"}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "name_hints", "params": {"query": "0x140001330"}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "traps"}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "traps", "params": {"query": "mainCRTStartup_"}}),
        ],
    );
    let tools = responses[0]["result"]["tools"].as_array().unwrap();
    for method in ["explain", "name_hints", "writers", "traps", "function", "callers", "xrefs"] {
        assert!(tools.iter().any(|t| t["method"] == method && t["inputSchema"]["type"] == "object"), "{method}: {tools:?}");
    }
    let explained = &responses[1]["result"];
    assert!(explained["summary"].as_str().is_some_and(|s| s.starts_with("mainCRTStartup_ at 0x")), "{explained}");
    assert!(explained["calls"].as_array().is_some_and(|c| !c.is_empty()), "{explained}");
    assert!(explained["imports"].as_array().is_some_and(|i| !i.is_empty()), "{explained}");
    assert!(explained["shape"]["back_edges"].is_u64(), "{explained}");
    // The entry stores the handle of the output in a global, which the writers of that global name.
    let written = explained["data"].as_array().unwrap().iter().find(|d| d["access"] == "write").expect("a global written").clone();
    let hints = &responses[2]["result"];
    assert!(hints["callers"].as_array().is_some_and(|c| c.iter().any(|c| c["name"] == "mainCRTStartup_" && c["named"] == true)), "{hints}");
    assert!(responses[3]["result"]["total"].is_u64(), "{}", responses[3]);
    assert!(responses[3]["result"]["functions"].is_array(), "{}", responses[3]);
    let one = responses[4]["result"]["functions"].as_array().unwrap();
    assert!(one.len() == 1 && one[0]["name"] == "mainCRTStartup_", "{one:?}");
    // Every tool the workbench lists is a method the server answers.
    let mut requests = vec![json!({"jsonrpc": "2.0", "id": 1, "method": "writers", "params": {"address": written["addr"]}})];
    requests.extend(tools.iter().enumerate().map(|(n, t)| json!({"jsonrpc": "2.0", "id": n + 2, "method": t["method"]})));
    let responses = ask(&args, &requests);
    let writers = &responses[0]["result"];
    assert!(writers["writers"].as_array().is_some_and(|w| w.iter().any(|w| w["name"] == "mainCRTStartup_")), "{writers}");
    assert!(writers["accesses"].as_array().is_some_and(|a| a.iter().any(|a| a["access"] == "write")), "{writers}");
    for (tool, response) in tools.iter().zip(&responses[1..]) {
        assert_ne!(response["error"]["code"], -32601, "{}: {response}", tool["name"]);
    }
}

#[test]
fn the_server_reads_the_translation_of_a_project() {
    let dir = ra_asm_testing::Scratch::new("ras-serve-tests", "translation");
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

#[test]
fn a_rename_through_the_server_lands_in_the_sidecar_and_the_next_emit() {
    let dir = ra_asm_testing::Scratch::new("ras-serve-tests", "annotations");
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe" }"#).unwrap();
    let responses = ask(
        &["--project", dir.to_str().unwrap()],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "annotate", "params": {"query": "F_140001330", "name": "fib", "comment": "the n-th Fibonacci number"}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "function", "params": {"query": "fib"}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "annotate", "params": {"query": "0x140001330", "name": "mainCRTStartup"}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "annotations"}),
        ],
    );
    assert_eq!(responses[0]["result"]["annotation"]["name"], "fib", "{}", responses[0]);
    // The server answers with the new name from the next request on.
    let function = &responses[1]["result"];
    assert_eq!(function["addr"], "0x140001330", "{function}");
    assert_eq!(function["annotation"]["comment"], "the n-th Fibonacci number", "{function}");
    // A name the host's runtime owns is refused, and the annotation stays as it was.
    assert_eq!(responses[2]["error"]["code"], -32602, "{}", responses[2]);
    assert_eq!(responses[3]["result"]["annotations"][0]["name"], "fib", "{}", responses[3]);
    // One function, one file, one key a line, and the function's identity last, by which the
    // next build of the reference finds it.
    let file = dir.join("rhipper/annotations/0x140001330.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with(
            "{\n  \"addr\": \"0x140001330\",\n  \"name\": \"fib\",\n  \"comment\": \"the n-th Fibonacci number\",\n  \"identity\": {"
        ),
        "{text}"
    );
    assert!(text.ends_with("}\n}\n") && text.lines().count() == 6 && text.contains("\"import WriteFile\""), "{text}");
    // The next emit names the function as the sidecar does, in the source and in the table.
    let emitted =
        Command::new(env!("CARGO_BIN_EXE_ras")).args(["pipeline", "--project"]).arg(&dir).args(["--only", "emit"]).output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let table: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
    let entry = table.as_array().unwrap().iter().find(|e| e["addr"] == "0x140001330").unwrap();
    assert_eq!(entry["name"], "fib", "{entry}");
    assert_eq!(entry["annotated"], true, "{entry}");
    assert_eq!(entry["comment"], "the n-th Fibonacci number", "{entry}");
    let file = entry["file"].as_str().unwrap();
    let source = std::fs::read_to_string(dir.join("src/gen").join(file)).unwrap();
    assert!(source.contains("fib("), "{source}");
}
