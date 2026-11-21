//! The lift against the machine: the fixture program, lifted and run over the interpreter,
//! prints what it printed under an emulator of the machine and ends with the same code
//! (`testdata/riscv/rv64.out`: the output, then `exit <code>`).

use std::path::PathBuf;

use ra_asm_riscv::Width;
use ra_asm_riscv::run::{Program, run};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/riscv").join(name)
}

#[test]
fn the_lifted_fixture_does_what_the_machine_did() {
    let program = Program::parse(&std::fs::read(fixture("rv64.elf")).unwrap()).unwrap();
    assert_eq!(program.width, Width::Rv64);
    let recorded = std::fs::read_to_string(fixture("rv64.out")).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
    let outcome = run(&program, 5_000_000).unwrap();
    assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed);
    assert_eq!(outcome.exit, Some(code));
    assert!(printed.lines().count() >= 14, "the fixture prints {} lines", printed.lines().count());
}

#[test]
fn what_is_no_program_is_refused() {
    assert!(Program::parse(b"\x7fELF").is_err());
    assert!(Program::parse(b"MZ\x90\x00 no ELF file at all, only some bytes").is_err());
    // The same file with another machine in its header.
    let mut other = std::fs::read(fixture("rv64.elf")).unwrap();
    other[18] = 62;
    assert!(Program::parse(&other).unwrap_err().0.contains("machine"));
}
