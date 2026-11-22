//! `ras riscv`, `ras ppc`, `ras sh4`: the command over each machine's fixture. It lists the
//! program's instructions, and with `--run` the lifted program prints what the fixture printed
//! under an emulator of its machine and ends with the same code.

use std::path::PathBuf;
use std::process::Command;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata").join(path)
}

fn ras(arguments: &[&str], program: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).args(arguments).arg(fixture(program)).output().unwrap()
}

/// The command's name, the fixture's program, and an instruction its listing has.
const MACHINES: [(&str, &str, &str); 9] = [
    ("ppc", "ppc/gc.dol", "mullw 5, 3, 5"),
    ("ppc", "ppc/ppc64.elf", "extsw 3, 3"),
    ("sh4", "sh4/sh4eb.elf", "dmulu.l"),
    ("sh4", "sh4/sh4fp.elf", "fmac"),
    ("ppc", "ppc/ppcfp.elf", "fmadd 0, 1, 2, 0"),
    ("riscv", "riscv/rv64.elf", "ecall"),
    ("riscv", "riscv/rv32c.elf", "ecall"),
    ("ppc", "ppc/ppc.elf", "mullw 5, 3, 5"),
    ("sh4", "sh4/sh4.elf", "trapa"),
];

#[test]
fn a_program_is_listed() {
    for (command, program, instruction) in MACHINES {
        let output = ras(&[command], program);
        assert!(output.status.success(), "{program}: {}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains(instruction), "{program}: the listing has no {instruction}");
        assert!(text.lines().count() > 100, "{program}: {} lines", text.lines().count());
    }
}

#[test]
fn a_lifted_program_runs_as_the_machine_ran_it() {
    for (command, program, _) in MACHINES {
        let recorded = std::fs::read_to_string(fixture(program).with_extension("out")).unwrap().replace("\r\n", "\n");
        let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
        let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
        let output = ras(&[command, "--run"], program);
        assert_eq!(String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n").trim_end(), printed, "{program}");
        assert_eq!(output.status.code(), Some(code), "{program}");
    }
}

#[test]
fn the_machine_is_the_file_s_whatever_the_command_s_name() {
    let output = ras(&["riscv", "--run"], "ppc/ppc.elf");
    assert_eq!(output.status.code(), Some(98));
    // A program of a machine the command has no lift of is refused by name.
    let output = ras(&["ppc"], "bin/cortexm.elf");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("none of RISC-V, PowerPC and SH-4"));
}
