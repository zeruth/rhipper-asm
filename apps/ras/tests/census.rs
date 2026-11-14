//! `ras smoke --census`: over the fixtures, the mnemonics the lift traps on are a pending list
//! sorted by the functions they affect, its traps are the traps of the functions, and each line
//! is its mnemonic of the JSON report. After it, the boundary: the imports without a prototype,
//! the structure parameters without an adapter row and the COM interfaces without a row, each a
//! pending list sorted by call sites.

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

/// One entry of a boundary list: `<name>: <n> call sites, <n> functions, <n> files[, through
/// <imports>], first <addr> in <file>`.
struct Boundary {
    name: String,
    sites: u64,
    functions: u64,
    files: u64,
    through: Vec<String>,
}

/// The number a field such as `12 functions` starts with.
fn number(field: &str, line: &str) -> u64 {
    field.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no count in `{field}` of `{line}`"))
}

/// The lists of a census, each its opening `# The ...` comment with the entries under it: the
/// lines without the blank ones and the other `#` comments, as `ras cards` takes a pending list.
fn lists(census: &str) -> Vec<(String, Vec<&str>)> {
    let mut lists: Vec<(String, Vec<&str>)> = Vec::new();
    for line in census.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with("# The ") {
            lists.push((line.to_owned(), Vec::new()));
        } else if !line.starts_with('#') {
            lists.last_mut().unwrap_or_else(|| panic!("an entry before any list's heading: `{line}`")).1.push(line);
        }
    }
    lists
}

/// The list of a census whose heading holds `what`.
fn list<'a>(census: &'a str, what: &str) -> Vec<&'a str> {
    let lists = lists(census);
    let (_, entries) =
        lists.into_iter().find(|(heading, _)| heading.contains(what)).unwrap_or_else(|| panic!("no list of {what} in:\n{census}"));
    entries
}

/// The entries of the mnemonics list of a census.
fn entries(census: &str) -> Vec<Entry> {
    list(census, "mnemonics")
        .into_iter()
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

/// The entries of the boundary list of a census whose heading holds `what`.
fn boundary(census: &str, what: &str) -> Vec<Boundary> {
    list(census, what)
        .into_iter()
        .map(|line| {
            let (name, rest) = line.split_once(": ").unwrap_or_else(|| panic!("no name in `{line}`"));
            let fields: Vec<&str> = rest.split(", ").collect();
            assert!(fields.len() >= 4, "a line is its call sites, functions, files and first instance: `{line}`");
            assert!(fields[0].contains("call site") && fields[1].contains("function") && fields[2].contains("file"), "`{line}`");
            let through = fields[3].strip_prefix("through ").map(|t| t.split(' ').map(str::to_owned).collect()).unwrap_or_default();
            let instance = fields[if fields[3].starts_with("through ") { 4 } else { 3 }];
            assert!(instance.starts_with("first 0x") || instance.starts_with("no call site"), "no first instance in `{line}`");
            Boundary {
                name: name.to_owned(),
                sites: number(fields[0], line),
                functions: number(fields[1], line),
                files: number(fields[2], line),
                through,
            }
        })
        .collect()
}

/// `ras smoke --census` over `path`, and the JSON report of the same run.
fn smoke(path: &Path, report: &str) -> (String, Value) {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(report);
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["smoke", "--quiet", "--census", "--jobs", "4", "--out"])
        .arg(&out)
        .arg(path)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("no report at {} ({e}):\n{stderr}", out.display()));
    let report: Value = serde_json::from_str(&report).unwrap_or_else(|e| panic!("the report is not JSON ({e}):\n{stderr}"));
    assert!(report["status"]["ok"].as_u64().unwrap() > 0, "no fixture went through: {:#}", report["status"]);
    (String::from_utf8(output.stdout).unwrap(), report)
}

/// The census over `testdata/bin` and the JSON report of the same run, made once for the tests.
fn run() -> &'static (String, Value) {
    static RUN: OnceLock<(String, Value)> = OnceLock::new();
    RUN.get_or_init(|| smoke(&root().join("testdata/bin"), "census.json"))
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

/// The three boundary lists follow the mnemonics, each sorted by call sites and each line its
/// name of the report.
#[test]
fn the_boundary_lists_are_sorted_by_call_sites_and_match_the_report() {
    let (census, report) = run();
    let headings: Vec<String> = lists(census).into_iter().map(|(heading, _)| heading).collect();
    assert_eq!(headings.len(), 4, "the mnemonics and the three boundary lists:\n{census}");
    for (key, what) in
        [("imports", "imports without a prototype"), ("structures", "structure parameters"), ("interfaces", "COM interfaces")]
    {
        let entries = boundary(census, what);
        let rows =
            report["boundary"][key].as_array().unwrap_or_else(|| panic!("no boundary.{key} in the report: {:#}", report["boundary"]));
        assert_eq!(entries.len(), rows.len(), "a line per {what} of the report:\n{census}");
        for (e, r) in entries.iter().zip(rows) {
            assert_eq!(r["name"], e.name.as_str(), "{r:#}\n{census}");
            assert_eq!(
                (r["sites"].as_u64(), r["functions"].as_u64(), r["files"].as_u64()),
                (Some(e.sites), Some(e.functions), Some(e.files)),
                "{r:#}"
            );
            assert!(e.files > 0 && e.functions <= e.sites, "{r:#}");
        }
        for pair in entries.windows(2) {
            assert!(pair[0].sites >= pair[1].sites, "{} is listed before {} with fewer call sites:\n{census}", pair[0].name, pair[1].name);
        }
    }
}

/// The window program imports nothing the prototype tables lack and creates no COM object; what
/// it still asks at the boundary are structures of the emit oracle's reason for holding it back
/// (`rhtest_win_clang_runs_as_c`), and the list is empty once their adapter rows pass.
#[test]
fn the_boundary_of_the_window_program_is_what_its_oracle_waits_for() {
    let (census, report) = smoke(&root().join("testdata/rhtest/win-clang.exe"), "census-win.json");
    assert_eq!(report["status"]["ok"], 1, "{:#}", report["status"]);
    assert!(boundary(&census, "imports without a prototype").is_empty(), "an import of win without a prototype:\n{census}");
    assert!(boundary(&census, "COM interfaces").is_empty(), "a COM interface in win:\n{census}");
    let waits_for = ["WNDCLASSA", "CREATESTRUCTA", "MSG", "WSADATA", "CRITICAL_SECTION"];
    for s in boundary(&census, "structure parameters") {
        assert!(waits_for.contains(&s.name.as_str()), "{} is not a structure the win oracle waits for:\n{census}", s.name);
        assert!(
            s.sites > 0 && s.functions > 0 && s.files == 1,
            "{}: {} sites, {} functions, {} files",
            s.name,
            s.sites,
            s.functions,
            s.files
        );
        assert!(!s.through.is_empty(), "{} names no import it is handed to:\n{census}", s.name);
    }
}

/// `fegetround` is an import no table knows: the floating-point environment program lists it
/// with its call sites, and the list is the lift's own `unknown_imports` with the sites added.
#[test]
fn the_boundary_lists_an_import_the_tables_lack_with_its_call_sites() {
    let (census, report) = smoke(&root().join("testdata/rhtest/fpenv-clang.exe"), "census-fpenv.json");
    let imports = boundary(&census, "imports without a prototype");
    let unknown: Vec<&str> =
        report["coverage"]["unknown_imports"].as_array().into_iter().flatten().filter_map(|u| u["name"].as_str()).collect();
    let listed: Vec<&str> = imports.iter().map(|i| i.name.as_str()).collect();
    let mut sorted = listed.clone();
    sorted.sort_unstable();
    let mut unknown_sorted = unknown.clone();
    unknown_sorted.sort_unstable();
    assert_eq!(sorted, unknown_sorted, "the list is not the report's unknown imports:\n{census}");
    let fegetround = imports.iter().find(|i| i.name == "fegetround").unwrap_or_else(|| panic!("fegetround is not listed:\n{census}"));
    assert!(fegetround.sites >= 1 && fegetround.functions >= 1 && fegetround.files == 1, "{}:\n{census}", fegetround.sites);
    assert!(census.contains("fegetround: ") && census.contains("first 0x"), "{census}");
}
