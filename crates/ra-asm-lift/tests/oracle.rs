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
    // The machine IR, the promoted IR and the ABI IR must all run; the tag picks one. The ABI IR
    // stands alone: its data are globals and it never maps the image.
    let level = if tag.contains("machine") {
        ra_asm_lift::RaiseLevel::Machine
    } else if tag.contains("promoted") {
        ra_asm_lift::RaiseLevel::Promoted
    } else {
        ra_asm_lift::RaiseLevel::Abi
    };
    // The exact x87 unit when the tag asks for it.
    let x87 = if tag.contains("exact") { ra_asm_lift::X87Unit::Exact } else { ra_asm_lift::X87Unit::Double };
    let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87 };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    // The tests run in parallel: each one writes its own directory.
    let dir = scratch(&format!("{tag}-{binary}"));
    std::fs::write(dir.join("program.ll"), lifted.module.to_ll()).unwrap();
    std::fs::write(dir.join("image.bin"), image.mapped()).unwrap();
    let abi = (level == ra_asm_lift::RaiseLevel::Abi).then_some(&signatures);
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, abi)).unwrap();
    (dir, lifted)
}

#[test]
fn the_lifted_fixtures_pass_the_ir_gate() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the IR gate is skipped");
        return;
    }
    for (binary, tag) in [
        ("hello64.exe", "gate-machine"),
        ("hello64.exe", "gate-abi"),
        ("control64.exe", "gate-abi"),
        ("float64.exe", "gate-abi"),
        ("exports64.dll", "gate-machine"),
        ("exports64.dll", "gate-abi"),
        ("tls64.exe", "gate-machine"),
        ("tls32.exe", "gate-machine"),
        ("crt64.exe", "gate-abi"),
    ] {
        let (dir, lifted) = lift_to(binary, tag);
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
    run_oracle_as(binary, expected, "run-machine");
    run_oracle_as(binary, expected, "run-promoted");
    run_oracle_as(binary, expected, "run-abi");
}

fn run_oracle_as(binary: &str, expected: &str, tag: &str) {
    let (dir, lifted) = lift_to(binary, tag);
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let exe = dir.join("lifted.exe");
    let mut build = Command::new("clang");
    build.args(["-O1", "-g", "-gcodeview", "-fuse-ld=lld", "-Wno-override-module"]);
    // A port of a 32-bit program keeps its 32-bit pointers: every address of the process must
    // fit in them, which the linker arranges for an image that is not large-address-aware. The
    // levels that map the image at its base cannot have that: the loader of such a process puts
    // its own heap and environment block at the low addresses, so they run as a plain process
    // with the driver's stack placed low.
    if lifted.bits == 32 && lifted.standalone {
        build.arg("-Wl,/largeaddressaware:no");
    }
    let build = build
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

/// The 32-bit fixtures at every level.
fn run_oracle_32(binary: &str, expected: &str) {
    run_oracle(binary, expected);
}

#[test]
fn hello32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    run_oracle_32("hello32.exe", &expected);
}

#[test]
fn control32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    run_oracle_32("control32.exe", &expected);
}

#[test]
fn float32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    run_oracle_32("float32.exe", &expected);
}

/// The x87 program under the exact unit: the series summed at 64 bits, the square root, the
/// tbyte round trip, all to 17 digits.
#[test]
fn extended32_runs_lifted_exact() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/extended.txt")).unwrap();
    run_oracle_as("extended32.exe", &expected, "run-machine-exact");
    run_oracle_as("extended32.exe", &expected, "run-promoted-exact");
    run_oracle_as("extended32.exe", &expected, "run-abi-exact");
}

#[test]
fn classes32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/classes.txt")).unwrap();
    run_oracle_32("classes32.exe", &expected);
}

#[test]
fn classes64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/classes.txt")).unwrap();
    run_oracle("classes64.exe", &expected);
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

/// The thread-local storage program: a thread-local counter stepped from two threads, each with
/// its own block, and the TLS callbacks counted. A thread kernel32 starts enters the lifted code
/// through its native entry, which only the ABI level has, so the run is at that level alone;
/// the gate takes the machine IR.
#[test]
fn tls64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/tls.txt")).unwrap();
    run_oracle_as("tls64.exe", &expected, "run-abi");
}

/// A program over the static C runtime starts at `main` with the driver's arguments and calls
/// the host's runtime for the functions the image links in: the ABI level is the one that has
/// a `main` to call and a host runtime to call into.
#[test]
fn crt64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/crt.txt")).unwrap();
    run_oracle_as("crt64.exe", &expected, "run-abi");
}

#[test]
fn tls32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/tls.txt")).unwrap();
    run_oracle_as("tls32.exe", &expected, "run-abi");
}

/// The struct program: a record of mixed fields filled and read through a pointer, which the
/// type recovery turns into a named struct at the ABI level.
#[test]
fn structs64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/structs.txt")).unwrap();
    run_oracle("structs64.exe", &expected);
}

#[test]
fn structs32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/structs.txt")).unwrap();
    run_oracle_32("structs32.exe", &expected);
}
