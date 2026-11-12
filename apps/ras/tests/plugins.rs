//! The plugins over a project: the name source of `plugins/names` loads from the project's
//! plugin directory, is listed with what it adds, and its names reach the function table.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `ras` over the project at `dir`, with a user plugin directory of the project's own, which
/// holds nothing, so the user's plugins stay out of the test.
fn ras(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).args(args).env("RHIPPER_PLUGINS", dir.join("user-plugins")).output().unwrap()
}

#[test]
fn a_name_source_plugin_names_the_functions_of_the_table() {
    let dir = ra_asm_testing::Scratch::new("ras-plugin-tests", "names");
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
    std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe" }"#).unwrap();
    let plugin = dir.join("rhipper/plugins/names");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::copy(root().join("plugins/names/names.wasm"), plugin.join("names.wasm")).unwrap();
    std::fs::write(plugin.join("names.txt"), "# the control fixture\r\n0x140001330 fib\r\n\r\n0x1 nowhere\r\nnonsense\r\n").unwrap();

    let listed = ras(&dir, &["plugins", "--json", "--project", dir.to_str().unwrap()]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let names = &listed["plugins"][0];
    assert_eq!(names["name"], "names", "{listed}");
    assert_eq!(names["origin"], "project", "{listed}");
    assert_eq!(names["error"], Value::Null, "{listed}");
    assert_eq!(names["kinds"], serde_json::json!(["names"]), "{listed}");

    let emitted = ras(&dir, &["pipeline", "--project", dir.to_str().unwrap(), "--only", "emit"]);
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let table: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
    let entry = table.as_array().unwrap().iter().find(|e| e["addr"] == "0x140001330").unwrap();
    assert_eq!(entry["name"], "fib", "{entry}");
    let file = entry["file"].as_str().unwrap();
    let source = std::fs::read_to_string(dir.join("src/gen").join(file)).unwrap();
    assert!(source.contains("fib("), "{source}");

    // `ras emit --project` takes the same names.
    let out = dir.join("out");
    let emitted = ras(
        &dir,
        &["emit", dir.join("ref/control64.exe").to_str().unwrap(), "--project", dir.to_str().unwrap(), "--out", out.to_str().unwrap()],
    );
    let warnings = String::from_utf8_lossy(&emitted.stderr);
    assert!(emitted.status.success(), "{warnings}");
    let table: Value = serde_json::from_str(&std::fs::read_to_string(out.join("functions.json")).unwrap()).unwrap();
    assert!(table.as_array().unwrap().iter().any(|e| e["addr"] == "0x140001330" && e["name"] == "fib"), "{table}");
    // The lines of the file for no function, and the one that is no line of names, are the plugin's warnings.
    assert!(warnings.contains("the plugin names: names.txt: no function starts at 0x1"), "{warnings}");
    assert!(warnings.contains("names.txt: not an address and a name: nonsense"), "{warnings}");
}
