//! The C backend against the native programs: each fixture is lifted, raised, written as C,
//! compiled with clang against the driver and the runtime header, run, and compared. The
//! structuring must leave no `goto` in the fixtures.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn clang_available() -> bool {
    Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ra-asm-emit-tests").join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The C of a fixture, with the image header and the function table beside it.
fn emit(binary: &str) -> (PathBuf, String) {
    let image = ra_asm_pe::load(&root().join("testdata/bin").join(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, ra_asm_lift::RaiseOptions::default());
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names });
    let c = unit.text();
    let dir = scratch(binary);
    std::fs::write(dir.join("program.c"), &c).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
    std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit, "program.c", &infos)).unwrap();
    (dir, c)
}

/// The function table names every function, and each `line` is the function's definition.
fn check_table(binary: &str, dir: &Path, c: &str) {
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("functions.json")).unwrap()).unwrap();
    let entries = table.as_array().unwrap();
    assert!(!entries.is_empty(), "{binary}: an empty function table");
    let lines: Vec<&str> = c.lines().collect();
    for e in entries {
        let addr = e["addr"].as_str().unwrap();
        assert!(addr.starts_with("0x") && addr.len() >= 10, "{binary}: address {addr}");
        assert!(!e["name"].as_str().unwrap().is_empty());
        if let Some(line) = e["line"].as_u64() {
            let text = lines[line as usize - 1];
            let ir_name = format!("F_{}(", addr.trim_start_matches("0x").trim_start_matches('0'));
            assert!(text.contains(&ir_name) && text.ends_with('{'), "{binary}: line {line} of {addr} is `{text}`");
            assert_eq!(e["status"], if e["traps"] == 0 { "lifted" } else { "partial" });
        } else {
            assert!(e["status"] == "thunk" || e["status"] == "not lifted", "{binary}: {e}");
        }
    }
}

fn run_oracle(binary: &str, expected_name: &str) {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let (dir, c) = emit(binary);
    check_table(binary, &dir, &c);
    assert!(!c.contains("goto "), "{binary}: the C has a goto:\n{c}");
    let exe = dir.join("program.exe");
    let build = Command::new("clang")
        .args([
            "-O1",
            "-std=gnu11",
            "-Wall",
            "-Wno-unused-variable",
            "-Wno-unused-but-set-variable",
            "-Wno-parentheses",
            "-Werror",
            "-fuse-ld=lld",
        ])
        .arg(dir.join("program.c"))
        .arg(root().join("runtime/driver.c"))
        .arg("-I")
        .arg(&dir)
        .arg("-I")
        .arg(root().join("runtime"))
        .arg("-o")
        .arg(&exe)
        .arg("-lkernel32")
        .output()
        .unwrap();
    assert!(build.status.success(), "{binary}: clang on {}:\n{}", dir.join("program.c").display(), String::from_utf8_lossy(&build.stderr));
    let run = Command::new(&exe).current_dir(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(root().join("testdata/expected").join(expected_name)).unwrap();
    assert_eq!(
        stdout.trim_end(),
        expected.trim_end(),
        "{binary}: the output (exit {:?}, stderr {})",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn hello64_runs_as_c() {
    run_oracle("hello64.exe", "hello.txt");
}

#[test]
fn control64_runs_as_c() {
    run_oracle("control64.exe", "control.txt");
}

#[test]
fn float64_runs_as_c() {
    run_oracle("float64.exe", "float.txt");
}
