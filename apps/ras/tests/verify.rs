//! `ras verify` over `control64.exe`: every function the run entered replays at the three levels
//! and the last line counts them all; a vector changed by hand makes its function differ, names
//! the level, the vector and the register, and fails the command. With `--built` beside its
//! port, the harness has a loader of every vector and a mock of every callee.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// `op_add` of the fixture: entered once, through the table of operations.
const OP_ADD: &str = "0x140001660";

fn control() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

fn ras(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).arg("verify").arg(control()).args(args).output().unwrap()
}

/// Whether the run could not start on this host, which has no Windows DLLs: the test is skipped.
fn skipped(output: &Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let skip = !output.status.success() && !cfg!(windows) && stderr.contains("no binding for ");
    if skip {
        eprintln!("SKIPPED: control64.exe does not run on a host without the Windows DLLs it imports: no vector to verify");
    }
    skip
}

/// The last line's numbers: the functions verified, the functions, the vectors.
fn summary(text: &str) -> (usize, usize, usize) {
    let last = text.lines().last().unwrap_or_default();
    let words: Vec<&str> = last.split([' ', ',']).filter(|word| !word.is_empty()).collect();
    match words.as_slice() {
        ["verified", verified, "of", functions, "functions", vectors, "vectors"] => {
            (verified.parse().unwrap(), functions.parse().unwrap(), vectors.parse().unwrap())
        }
        _ => panic!("the last line counts the functions verified: {last}"),
    }
}

#[test]
fn every_function_of_control64_replays_at_every_level() {
    let output = ras(&[]);
    if skipped(&output) {
        return;
    }
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}\n{}", String::from_utf8_lossy(&output.stderr));
    let (verified, functions, vectors) = summary(&text);
    assert_eq!(verified, functions, "{text}");
    // off Windows the C runtime's imports are stand-ins and control64 ends in its startup: one
    // function entered, nothing of the program's own to replay (the Linux release job, 2026-11-15)
    if !cfg!(windows) && functions < 10 {
        eprintln!("SKIPPED: control64.exe entered {functions} function(s) on a host without the Windows DLLs");
        return;
    }
    assert!(functions >= 10, "the run entered {functions} functions:\n{text}");
    assert!(vectors >= functions, "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), functions + 1, "a line per function and the count:\n{text}");
    for line in &lines[..functions] {
        let words: Vec<&str> = line.split(' ').collect();
        assert!(
            matches!(words.as_slice(), [_, "machine", "ok", m, "promoted", "ok", p, "abi", "ok", a] if m == p && p == a),
            "each level replays the function's vectors: {line}"
        );
    }
}

#[test]
fn a_changed_vector_differs_and_fails_the_command() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("verify");
    std::fs::create_dir_all(&dir).unwrap();
    let recorded = dir.join("op_add.vectors");
    let output = ras(&["--function", OP_ADD, "--write", recorded.to_str().unwrap()]);
    if skipped(&output) {
        return;
    }
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}\n{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(summary(&text), (1, 1, 1), "the filter takes the one function:\n{text}");

    let mut vectors = ra_asm_verify::read(&std::fs::read_to_string(&recorded).unwrap()).unwrap();
    assert_eq!(vectors.len(), 1, "the file holds the vector of op_add alone");
    let rax = vectors[0].exit.iter_mut().find(|(name, _)| name == "rax").expect("the exit holds rax");
    rax.1 ^= 1;
    let changed = dir.join("op_add-changed.vectors");
    std::fs::write(&changed, ra_asm_verify::write(&vectors).unwrap()).unwrap();

    let output = ras(&["--vectors", changed.to_str().unwrap()]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1), "a function that differs fails the command:\n{text}");
    assert_eq!(summary(&text), (0, 1, 1), "{text}");
    let line = text.lines().next().unwrap_or_default();
    assert!(line.contains(" machine differs at vector 1: register rax: "), "the line names the level, the vector and the register: {line}");
    assert!(!line.contains("promoted"), "the levels above the first difference are not replayed: {line}");
}

#[test]
fn harness_is_written() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("verify-built");
    let port = dir.join("control64");
    std::fs::create_dir_all(&port).unwrap();
    let emitted =
        Command::new(env!("CARGO_BIN_EXE_ras")).arg("emit").arg(control()).arg("--out").arg(&port).arg("--quiet").output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));

    let recorded = dir.join("control64.vectors");
    let output = ras(&["--built", "--out", port.to_str().unwrap(), "--write", recorded.to_str().unwrap()]);
    if skipped(&output) {
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let harness = std::fs::read_to_string(port.join("verify.c")).expect("the harness is written beside the port");
    assert!(harness.contains("#include \"runtime/verify.c\""), "the harness includes its runtime");
    let runtime = std::fs::read_to_string(port.join("runtime/verify.c")).expect("the runtime is written under the port's runtime");
    let embedded = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/verify.c")).unwrap();
    assert_eq!(runtime, embedded, "the runtime written is the repository's");

    let vectors = ra_asm_verify::read(&std::fs::read_to_string(&recorded).unwrap()).unwrap();
    assert!(!vectors.is_empty(), "the run recorded vectors");
    for (n, vector) in vectors.iter().enumerate() {
        let entry = format!("{{\"{}\", ra_verify_load_{}}},", vector.function, n + 1);
        assert!(harness.contains(&entry), "the harness has a loader of vector {} of {}: {entry}", n + 1, vector.function);
    }
    let callees: std::collections::BTreeSet<&str> = vectors.iter().flat_map(|v| &v.calls).map(|call| call.callee.as_str()).collect();
    if callees.is_empty() && !cfg!(windows) {
        eprintln!("SKIPPED: control64.exe called nothing on a host without the Windows DLLs: no mock to look for");
        return;
    }
    assert!(!callees.is_empty(), "the vectors call something");
    for callee in callees {
        let binding = format!("{{\"{callee}\", (void (*)(void))ra_mock_");
        assert!(harness.contains(&binding), "the mock declares the callee {callee}");
    }
}

#[test]
fn a_function_without_a_vector_is_refused() {
    let output = ras(&["--function", "no_such_function"]);
    if skipped(&output) {
        return;
    }
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no_such_function has no vector"), "{stderr}");
}
