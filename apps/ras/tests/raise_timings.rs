//! `ras lift --timings`: each pass of the raise over each function writes its line on stderr,
//! `raise: <function> <pass> <ms>`, so the last line of a raise that hangs names where it is.

use std::path::Path;
use std::process::Command;

/// Every `raise:` line ends in a count of milliseconds, a function's passes follow one another in
/// the raise's order, and the module's steps are named as the module's.
#[test]
fn the_lift_prints_each_function_and_pass_with_its_milliseconds() {
    let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .arg("lift")
        .arg(&control)
        .args(["--quiet", "--timings", "--limit", "0"])
        .env_remove("RA_STEP_TIMES")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let lines: Vec<&str> = stderr.lines().filter_map(|line| line.strip_prefix("raise: ")).collect();
    assert!(!lines.is_empty(), "{stderr}");
    for line in &lines {
        let (_, ms) = line.rsplit_once(' ').unwrap_or_else(|| panic!("`raise: {line}` has no milliseconds"));
        assert!(!ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit()), "`raise: {line}` does not end in milliseconds");
    }
    let first = lines.iter().find_map(|line| line.split_once(' ').filter(|(_, rest)| rest.starts_with("promote ")).map(|(f, _)| f));
    let function = first.unwrap_or_else(|| panic!("no function is promoted:\n{stderr}"));
    let passes: Vec<&str> = lines.iter().filter_map(|line| line.strip_prefix(function)?.strip_prefix(' ')?.split(' ').next()).collect();
    let promote = passes.iter().position(|pass| *pass == "promote").unwrap_or_default();
    assert!(passes[promote..].starts_with(&["promote", "conditions", "frame"]), "{function} passes {passes:?}:\n{stderr}");
    assert!(lines.iter().any(|line| line.starts_with("the module types ")), "{stderr}");
}

/// Without the flag the raise writes no such line.
#[test]
fn the_lift_without_the_flag_prints_no_timings() {
    let control = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .arg("lift")
        .arg(&control)
        .args(["--quiet", "--limit", "0"])
        .env_remove("RA_STEP_TIMES")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.lines().any(|line| line.starts_with("raise: ")), "{stderr}");
}
