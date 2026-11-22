//! The emit: the lifted fixture through the C backend. The source defines the program's code
//! and its entry; compiled with the C runtime (`runtime/msp430.c`), where there is a `clang`,
//! it is a program of this machine that prints what the fixture's source prints and ends with
//! its code.

use std::path::PathBuf;
use std::process::Command;

use ra_asm_emit::c::{Options, emit_c};
use ra_asm_msp430::run::Program;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source() -> String {
    let program = Program::parse(&std::fs::read(workspace().join("testdata/msp430/msp430.elf")).unwrap()).unwrap();
    emit_c(&program.lift().unwrap(), &Options::default())
}

#[test]
fn the_source_defines_the_program() {
    let text = source();
    assert!(!text.contains("is not written"), "a global the backend left out");
    for defined in ["void msp430_run(", "void msp430_main(", "msp430_memory", "msp430_segment_0"] {
        assert!(text.contains(defined), "the source has no {defined}");
    }
    for called in ["msp430_out(", "msp430_status(", "msp430_trap(", "msp430_segment("] {
        assert!(text.contains(called), "the source never calls {called}");
    }
}

#[test]
fn the_compiled_port_prints_what_its_source_prints() {
    if !Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("clang is not on the PATH: the port's test is skipped");
        return;
    }
    let scratch = ra_asm_testing::Scratch::new("ra-asm-msp430-tests", "msp430");
    let path = scratch.join("program.c");
    std::fs::write(&path, source()).unwrap();
    let executable = scratch.join(format!("msp430{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new("clang")
        .args(["-O1", "-Werror=implicit-function-declaration", "-I"])
        .arg(workspace().join("runtime"))
        .arg(&path)
        .arg(workspace().join("runtime/msp430.c"))
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
        panic!("clang:\n{text}");
    }
    let run = Command::new(&executable).output().unwrap();
    let recorded = std::fs::read_to_string(workspace().join("testdata/msp430/msp430.out")).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
    let wrote = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(wrote.trim_end(), printed, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(run.status.code(), Some(code));
}
