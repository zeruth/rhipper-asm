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

/// The C of a fixture, with the image header beside it.
fn emit(binary: &str) -> (PathBuf, String) {
    let image = ra_asm_pe::load(&root().join("testdata/bin").join(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, ra_asm_lift::RaiseOptions::default());
    let names = analysis.functions.values().map(|f| (ra_asm_lift::function_name(f.entry), f.name.clone())).collect();
    let c = ra_asm_emit::emit_c(&lifted.module, &ra_asm_emit::COptions { names });
    let dir = scratch(binary);
    std::fs::write(dir.join("program.c"), &c).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
    (dir, c)
}

fn run_oracle(binary: &str, expected_name: &str) {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let (dir, c) = emit(binary);
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
