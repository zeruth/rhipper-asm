//! `ras smoke --census`: over the fixtures, the mnemonics the lift traps on are a pending list
//! sorted by the functions they affect, its traps are the traps of the functions, and each line
//! is its mnemonic of the JSON report.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One entry of the census: `<mnemonic>: <n> functions, <n> traps, <n> files, first <addr> in <file>`.
struct Entry {
    mnemonic: String,
    functions: u64,
    traps: u64,
    files: u64,
}

/// The number a field such as `12 functions` starts with.
fn number(field: &str, line: &str) -> u64 {
    field.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no count in `{field}` of `{line}`"))
}

/// The entries of a census: its lines without the blank ones and the `#` comments, as `ras
/// cards` takes a pending list.
fn entries(census: &str) -> Vec<Entry> {
    census
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let (mnemonic, rest) = line.split_once(": ").unwrap_or_else(|| panic!("no mnemonic in `{line}`"));
            let fields: Vec<&str> = rest.split(", ").collect();
            assert!(fields.len() >= 4, "a line is its functions, traps, files and first instance: `{line}`");
            assert!(fields[0].contains("function") && fields[1].contains("trap") && fields[2].contains("file"), "`{line}`");
            assert!(fields[3].starts_with("first 0x"), "no first instance in `{line}`");
            Entry {
                mnemonic: mnemonic.to_owned(),
                functions: number(fields[0], line),
                traps: number(fields[1], line),
                files: number(fields[2], line),
            }
        })
        .collect()
}

/// The census over `testdata/bin` and the JSON report of the same run, made once for the tests.
fn run() -> &'static (String, Value) {
    static RUN: OnceLock<(String, Value)> = OnceLock::new();
    RUN.get_or_init(|| {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("census.json");
        let dir = root().join("testdata/bin");
        let output = Command::new(env!("CARGO_BIN_EXE_ras"))
            .args(["smoke", "--quiet", "--census", "--jobs", "4", "--out"])
            .arg(&out)
            .arg(&dir)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report = std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("no report at {} ({e}):\n{stderr}", out.display()));
        let report: Value = serde_json::from_str(&report).unwrap_or_else(|e| panic!("the report is not JSON ({e}):\n{stderr}"));
        assert!(report["status"]["ok"].as_u64().unwrap() > 0, "no fixture went through: {:#}", report["status"]);
        (String::from_utf8(output.stdout).unwrap(), report)
    })
}

#[test]
fn the_census_is_sorted_and_totals_the_functions_traps() {
    let (census, report) = run();
    assert!(census.starts_with("# "), "the census opens with its comment:\n{census}");
    let entries = entries(census);
    for pair in entries.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let ordered = a.functions > b.functions || (a.functions == b.functions && a.traps >= b.traps);
        assert!(
            ordered,
            "{} ({} functions) is listed before {} ({} functions):\n{census}",
            a.mnemonic, a.functions, b.mnemonic, b.functions
        );
    }
    let total: u64 = entries.iter().map(|e| e.traps).sum();
    let coverage = &report["coverage"];
    assert_eq!(total, coverage["function_traps"].as_u64().unwrap(), "the census's traps are not the functions':\n{census}");
    assert_eq!(coverage["function_traps"], coverage["traps"], "{coverage:#}");
}

#[test]
fn a_census_line_is_its_mnemonic_of_the_report() {
    let (census, report) = run();
    let entries = entries(census);
    let uncovered = report["coverage"]["uncovered"].as_array().unwrap();
    assert_eq!(entries.len(), uncovered.len(), "a line per mnemonic of the report:\n{census}");
    let trapping = report["coverage"]["trapping_functions"].as_u64().unwrap();
    for e in &entries {
        let u = uncovered
            .iter()
            .find(|u| u["mnemonic"] == e.mnemonic.as_str())
            .unwrap_or_else(|| panic!("{} is not in the report", e.mnemonic));
        assert_eq!(
            (u["functions"].as_u64(), u["count"].as_u64(), u["files"].as_u64()),
            (Some(e.functions), Some(e.traps), Some(e.files)),
            "{u:#}"
        );
        // A file that traps on a mnemonic has a function that does, and a function a trap.
        assert!(e.files <= e.functions && e.functions <= e.traps && e.functions <= trapping, "{u:#}");
        assert!(e.files > 0, "{u:#}");
    }
}
