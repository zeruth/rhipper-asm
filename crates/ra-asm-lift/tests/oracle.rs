//! The execution oracle: a fixture is lifted, compiled with clang against the driver, run, and its
//! output compared with the native program's. The IR gate (`clang -c -x ir`) runs on every fixture;
//! the run needs the C runtime and skips without it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("testdata/bin").join(name)
}

fn clang_available() -> bool {
    Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ra-asm-lift-tests").join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Lifts `binary` into a directory with `program.ll`, `image.bin` and `image.h`.
fn lift_to(binary: &str, tag: &str) -> (PathBuf, ra_asm_lift::Lifted) {
    let image = ra_asm_pe::load(&fixture(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    // The tests run in parallel: each one writes its own directory.
    let dir = scratch(&format!("{tag}-{binary}"));
    std::fs::write(dir.join("program.ll"), lifted.module.to_ll()).unwrap();
    std::fs::write(dir.join("image.bin"), image.mapped()).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted)).unwrap();
    (dir, lifted)
}

#[test]
fn the_lifted_fixtures_pass_the_ir_gate() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the IR gate is skipped");
        return;
    }
    for binary in ["hello64.exe", "control64.exe", "float64.exe", "exports64.dll"] {
        let (dir, lifted) = lift_to(binary, "gate");
        assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
        let check =
            Command::new("clang").args(["-c", "-x", "ir", "-o"]).arg(dir.join("program.o")).arg(dir.join("program.ll")).output().unwrap();
        assert!(
            check.status.success(),
            "{binary}: clang -c on {}:\n{}",
            dir.join("program.ll").display(),
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

fn run_oracle(binary: &str, expected: &str) {
    let (dir, lifted) = lift_to(binary, "run");
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let exe = dir.join("lifted.exe");
    let build = Command::new("clang")
        .args(["-O1", "-g", "-gcodeview", "-fuse-ld=lld", "-Wno-override-module"])
        .arg(dir.join("program.ll"))
        .arg(root().join("runtime/driver.c"))
        .arg("-I")
        .arg(&dir)
        .arg("-o")
        .arg(&exe)
        .arg("-lkernel32")
        .output()
        .unwrap();
    if !build.status.success() {
        let text = String::from_utf8_lossy(&build.stderr);
        if text.contains("cannot open input file") || text.contains("libcmt") || text.contains("msvcrt") {
            eprintln!("the C runtime is not installed: the run is skipped\n{text}");
            return;
        }
        panic!("{binary}: clang link:\n{text}");
    }
    let run = Command::new(&exe).arg(dir.join("image.bin")).current_dir(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(
        stdout.trim_end(),
        expected.trim_end(),
        "{binary}: the lifted program's output (exit {:?}, stderr: {stderr})",
        run.status.code()
    );
    assert!(run.status.success(), "{binary}: exit {:?}, stderr: {stderr}", run.status.code());
}

#[test]
fn hello64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    run_oracle("hello64.exe", &expected);
}

#[test]
fn float64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    run_oracle("float64.exe", &expected);
}

#[test]
fn control64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    run_oracle("control64.exe", &expected);
}
