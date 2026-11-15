//! The rhtest ports the oracle still ignores are a tracked list: `testdata/rhtest/pending.txt`
//! holds one row per ignored port of `tests/oracle.rs`, and this fast test fails when the two
//! differ, so a change that lands a port removes its row and a new ignore adds one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A row as the list writes it: the program, its variant, the language of the port and the reason.
fn row(program: &str, variant: &str, language: &str, reason: &str) -> String {
    format!("{program} | {variant} | {language} | {reason}")
}

/// The rows of the oracle's ignored rhtest ports, each with the test it comes from: a test named
/// `rhtest_…_runs_as_<language>` under an `#[ignore = "<reason>"]`, whose body names the program
/// and the variant as its first two string literals. A debugging aid (`_traced`) is not a port.
fn oracle_rows(source: &str) -> BTreeMap<String, String> {
    let lines: Vec<&str> = source.lines().map(str::trim).collect();
    let mut rows = BTreeMap::new();
    let mut reason: Option<&str> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(r) = line.strip_prefix("#[ignore = \"").and_then(|r| r.strip_suffix("\"]")) {
            reason = Some(r);
            continue;
        }
        let Some(signature) = line.strip_prefix("fn ") else { continue };
        let Some(reason) = reason.take() else { continue };
        let name = signature.split('(').next().unwrap_or(signature);
        let Some(suffix) = name.strip_prefix("rhtest_").and_then(|n| n.rsplit_once("_runs_as_")).map(|(_, l)| l) else { continue };
        let language = match suffix {
            "c" => "C",
            "cpp" => "C++",
            "rust" => "Rust",
            other => panic!("oracle.rs: {name}: the language `{other}` is not C, C++ or Rust"),
        };
        let body = lines[i + 1..].iter().copied().take_while(|l| *l != "}");
        let literals: Vec<&str> = body.flat_map(|l| l.split('"').skip(1).step_by(2)).collect();
        let [program, variant, ..] = literals[..] else {
            panic!("oracle.rs: {name}: the body does not name the program and the variant as string literals");
        };
        rows.insert(row(program, variant, language, reason), name.to_owned());
    }
    rows
}

/// The rows of the list, without its comments and blank lines, each with its line number.
fn list_rows(text: &str) -> BTreeMap<String, usize> {
    let mut rows = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.splitn(4, '|').map(str::trim).collect();
        let [program, variant, language, reason] = fields[..] else {
            panic!("pending.txt:{}: not `program | variant | language | reason`: {line}", i + 1);
        };
        let previous = rows.insert(row(program, variant, language, reason), i + 1);
        assert!(previous.is_none(), "pending.txt:{}: the row is listed twice: {line}", i + 1);
    }
    rows
}

#[test]
fn the_pending_list_is_the_oracles_ignored_ports() {
    let source = std::fs::read_to_string(root().join("crates/ra-asm-emit/tests/oracle.rs")).unwrap();
    let text = std::fs::read_to_string(root().join("testdata/rhtest/pending.txt")).unwrap();
    let oracle = oracle_rows(&source);
    let list = list_rows(&text);
    assert!(!oracle.is_empty(), "oracle.rs: no ignored rhtest port found: the test no longer reads its ignores");
    let mut differences = Vec::new();
    for (row, test) in &oracle {
        if !list.contains_key(row) {
            differences.push(format!("oracle.rs ignores {test}, pending.txt lacks its row: {row}"));
        }
    }
    for (row, line) in &list {
        if !oracle.contains_key(row) {
            differences.push(format!("pending.txt:{line} has a row no ignored test of oracle.rs matches: {row}"));
        }
    }
    assert!(differences.is_empty(), "the pending list and the oracle's ignores differ:\n{}", differences.join("\n"));
}

#[test]
fn an_ignored_port_reads_as_its_row() {
    let source = r#"
#[test]
#[ignore = "needs a thing"]
fn rhtest_misc_clang_runs_as_rust() {
    run_rhtest_as_rust("misc", "clang");
}

#[test]
#[ignore = "a debugging aid"]
fn rhtest_crt_clang_traced() {
    run_fixture_as_c_with(&rhtest("crt", "clang"), "traced");
}

#[test]
fn rhtest_sys_clang_runs_as_c() {
    run_rhtest_as_c("sys", "clang");
}
"#;
    let rows = oracle_rows(source);
    assert_eq!(
        rows.into_iter().collect::<Vec<_>>(),
        [("misc | clang | Rust | needs a thing".to_owned(), "rhtest_misc_clang_runs_as_rust".to_owned())]
    );
}
