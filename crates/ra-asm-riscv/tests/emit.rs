//! The emit: the lifted fixtures through the C backend. The source defines the program's
//! functions, each under its symbol's name, and its entry; compiled with the C runtime
//! (`runtime/riscv.c`), where there is a `clang`, it is a program of this machine that prints
//! what the fixture printed under the emulator and ends with the same code.

use std::path::PathBuf;
use std::process::Command;

use ra_asm_emit::c::{Options, emit_c};
use ra_asm_riscv::run::Program;

/// The fixtures: the wide machine without and with the compressed instructions, and the narrow one.
const FIXTURES: [&str; 3] = ["rv64", "rv64c", "rv32c"];

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(name: &str) -> String {
    let program = Program::parse(&std::fs::read(workspace().join(format!("testdata/riscv/{name}.elf"))).unwrap()).unwrap();
    // The program as its functions: a function of C for each.
    emit_c(&program.lift_functions().unwrap(), &Options::default())
}

#[test]
fn the_source_defines_the_program() {
    for name in FIXTURES {
        let text = source(name);
        assert!(!text.contains("is not written"), "{name}: a global the backend left out");
        for defined in [
            "void f__start(",
            "void f_mix(",
            "void f_divide(",
            "void rv_run(",
            "void rv_call(",
            "void rv_main(",
            "rv_memory",
            "rv_segment_0",
        ] {
            assert!(text.contains(defined), "{name}: the source has no {defined}");
        }
        for called in ["rv_ecall(", "rv_trap(", "rv_segment(", "f_mix(state)"] {
            assert!(text.contains(called), "{name}: the source never calls {called}");
        }
    }
}

#[test]
fn the_compiled_port_does_what_the_machine_did() {
    if !Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("clang is not on the PATH: the port's test is skipped");
        return;
    }
    let scratch = ra_asm_testing::Scratch::new("ra-asm-riscv-tests", "port");
    for name in FIXTURES {
        let path = scratch.join(format!("{name}.c"));
        std::fs::write(&path, source(name)).unwrap();
        let executable = scratch.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let build = Command::new("clang")
            .args(["-O1", "-Werror=implicit-function-declaration", "-I"])
            .arg(workspace().join("runtime"))
            .arg(&path)
            .arg(workspace().join("runtime/riscv.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        if !build.status.success() {
            let text = String::from_utf8_lossy(&build.stderr);
            // Without the C runtime library installed, the link fails and the run is skipped.
            if text.contains("cannot open input file") || text.contains("libcmt") || text.contains("msvcrt") {
                eprintln!("the C runtime is not installed: the run is skipped\n{text}");
                return;
            }
            panic!("{name}: clang:\n{text}");
        }
        let run = Command::new(&executable).output().unwrap();
        let recorded = std::fs::read_to_string(workspace().join(format!("testdata/riscv/{name}.out"))).unwrap().replace("\r\n", "\n");
        let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
        let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
        let wrote = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
        assert_eq!(wrote.trim_end(), printed, "{name}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(run.status.code(), Some(code), "{name}");
    }
}
