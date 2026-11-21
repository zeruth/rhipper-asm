//! The lift against the machine: the fixture program (`testdata/sh4/sh4.elf`), lifted and run
//! over the interpreter, prints what it printed under an emulator of the machine and ends with
//! the same code (`sh4.out`: the output, then `exit <code>`).

use std::path::PathBuf;

use ra_asm_sh::run::{Program, run};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/sh4").join(name)
}

#[test]
fn the_lifted_fixture_does_what_the_machine_did() {
    let program = Program::parse(&std::fs::read(fixture("sh4.elf")).unwrap()).unwrap();
    let recorded = std::fs::read_to_string(fixture("sh4.out")).unwrap().replace("\r\n", "\n");
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
    let mut other = std::fs::read(fixture("sh4.elf")).unwrap();
    other[18] = 62;
    assert!(Program::parse(&other).unwrap_err().0.contains("machine"));
    // A table said to lie past the file's end.
    let mut far = std::fs::read(fixture("sh4.elf")).unwrap();
    far[28..32].copy_from_slice(&[0xff; 4]);
    assert!(Program::parse(&far).is_err());
}
