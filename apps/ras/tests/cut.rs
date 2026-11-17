//! `ras lift` and `ras disasm` cut their own output: `--match` keeps the lines that hold a text,
//! `--limit` stops after a count and says how many it left out, so no reader needs a pipe.

use std::path::{Path, PathBuf};
use std::process::Command;

fn control64() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

/// Runs `ras <subcommand> <control64> <args>`: the image is the first positional argument.
fn ras(args: &[&str]) -> String {
    let (subcommand, rest) = args.split_first().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg(subcommand).arg(control64()).args(rest).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// The entry point of the fixture: a straight run of code that calls through the import slots.
const ENTRY: &str = "0x140001000";

#[test]
fn match_keeps_the_lines_that_hold_any_text_in_any_case() {
    let whole = ras(&["disasm", ENTRY]);
    let calls = ras(&["disasm", "--match", "CALL", ENTRY]);
    let expected: Vec<&str> = whole.lines().filter(|l| l.to_lowercase().contains("call")).collect();
    assert!(expected.len() > 1, "{whole}");
    assert_eq!(calls.lines().collect::<Vec<_>>(), expected);

    let either = ras(&["disasm", "--match", "getstdhandle", "--match", "ExitProcess", ENTRY]);
    let lines: Vec<&str> = either.lines().collect();
    assert_eq!(lines.len(), 2, "{either}");
    assert!(lines[0].contains("GetStdHandle") && lines[1].contains("ExitProcess"), "{either}");
}

#[test]
fn limit_stops_after_n_lines_and_counts_the_rest() {
    let whole = ras(&["disasm", ENTRY]);
    let total = whole.lines().count();
    let cut = ras(&["disasm", "--limit", "3", ENTRY]);
    let lines: Vec<&str> = cut.lines().collect();
    assert_eq!(lines[..3], whole.lines().take(3).collect::<Vec<_>>()[..]);
    assert_eq!(lines.len(), 4, "{cut}");
    assert_eq!(lines[3], format!("... {} more lines left out by --limit", total - 3));

    let all = ras(&["disasm", "--limit", &total.to_string(), ENTRY]);
    assert_eq!(all, whole, "a limit the output fits in cut something");
}

#[test]
fn lift_cuts_the_ir_it_prints() {
    let whole = ras(&["lift", "--level", "machine", "--quiet"]);
    let defines = ras(&["lift", "--level", "machine", "--quiet", "--match", "DEFINE "]);
    let expected: Vec<&str> = whole.lines().filter(|l| l.to_lowercase().contains("define ")).collect();
    assert!(expected.len() > 1, "{}", &whole[..whole.len().min(4000)]);
    assert_eq!(defines.lines().collect::<Vec<_>>(), expected);

    let first = ras(&["lift", "--level", "machine", "--quiet", "--match", "define ", "--limit", "1"]);
    let lines: Vec<&str> = first.lines().collect();
    assert_eq!(lines, [expected[0].to_string(), format!("... {} more lines left out by --limit", expected.len() - 1)]);
}
