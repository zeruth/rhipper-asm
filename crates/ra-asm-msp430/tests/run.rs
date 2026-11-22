//! The lift against the program's source: the fixture (`testdata/msp430/msp430.elf`), lifted
//! and run over the interpreter, prints what its source prints and ends with its code
//! (`msp430.out`: the output, then `exit <code>`).
//!
//! No emulator of the machine is here to say what the program does. Its source is the integer
//! program every other machine's fixture is built from, each of which printed these lines
//! under an emulator of its own; a lift that got an instruction wrong would not print them.

use std::path::PathBuf;

use ra_asm_msp430::run::{Program, run};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/msp430").join(name)
}

#[test]
fn the_lifted_fixture_prints_what_its_source_prints() {
    let program = Program::parse(&std::fs::read(fixture("msp430.elf")).unwrap()).unwrap();
    let recorded = std::fs::read_to_string(fixture("msp430.out")).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
    let outcome = run(&program, 50_000_000).unwrap();
    assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed);
    assert_eq!(outcome.exit, Some(code));
    assert!(printed.lines().count() >= 14, "the fixture prints {} lines", printed.lines().count());
}

#[test]
fn what_is_no_program_is_refused() {
    assert!(Program::parse(b"\x7fELF").is_err());
    let file = std::fs::read(fixture("msp430.elf")).unwrap();
    let mut other = file.clone();
    other[18] = 40;
    assert!(Program::parse(&other).unwrap_err().0.contains("machine"));
    // A table said to lie past the file's end.
    let mut far = file;
    far[28..32].copy_from_slice(&[0xff; 4]);
    assert!(Program::parse(&far).is_err());
}
