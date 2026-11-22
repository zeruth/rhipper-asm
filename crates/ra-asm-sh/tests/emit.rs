//! The emit: the lifted fixtures through the C backend. The source defines the program's code
//! and its entry; compiled with the C runtime (`runtime/sh4.c`), where there is a `clang`, it is
//! a program of this machine that prints what the fixture printed under the emulator and ends
//! with the same code.

use std::path::PathBuf;
use std::process::Command;

use ra_asm_emit::c::{Options, emit_c};
use ra_asm_sh::run::Program;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(name: &str) -> String {
    let program = Program::parse(&std::fs::read(workspace().join(format!("testdata/sh4/{name}.elf"))).unwrap()).unwrap();
    emit_c(&program.lift().unwrap(), &Options::default())
}

#[test]
fn the_source_defines_the_program() {
    for name in ["sh4", "sh4eb", "sh4fp", "sh4fpd"] {
        let text = source(name);
        assert!(!text.contains("is not written"), "a global the backend left out");
        for defined in ["void sh_run(", "void sh_main(", "sh_memory", "sh_segment_0"] {
            assert!(text.contains(defined), "the source has no {defined}");
        }
        for called in ["sh_trapa(", "sh_trap(", "sh_segment("] {
            assert!(text.contains(called), "the source never calls {called}");
        }
    }
}

fn the_port_does_what_the_machine_did(name: &str) {
    if !Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("clang is not on the PATH: the port's test is skipped");
        return;
    }
    let scratch = ra_asm_testing::Scratch::new("ra-asm-sh-tests", name);
    let path = scratch.join("program.c");
    std::fs::write(&path, source(name)).unwrap();
    let executable = scratch.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    // The lift's products and sums are the program's own: the compiler is not to fuse them.
    let build = Command::new("clang")
        .args(["-O1", "-ffp-contract=off", "-Werror=implicit-function-declaration", "-I"])
        .arg(workspace().join("runtime"))
        .arg(&path)
        .arg(workspace().join("runtime/sh4.c"))
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
    let recorded = std::fs::read_to_string(workspace().join(format!("testdata/sh4/{name}.out"))).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
    let wrote = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(wrote.trim_end(), printed, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(run.status.code(), Some(code));
}

#[test]
fn the_compiled_port_does_what_the_machine_did() {
    the_port_does_what_the_machine_did("sh4");
}

#[test]
fn the_compiled_big_endian_port_does_what_the_machine_did() {
    the_port_does_what_the_machine_did("sh4eb");
}

#[test]
fn the_compiled_floating_point_does_what_the_machine_did() {
    the_port_does_what_the_machine_did("sh4fp");
}

#[test]
fn the_compiled_sums_of_products_are_what_the_machine_summed() {
    the_port_does_what_the_machine_did("sh4mac");
}

#[test]
fn the_compiled_doubles_do_what_the_machine_did() {
    the_port_does_what_the_machine_did("sh4fpd");
}
