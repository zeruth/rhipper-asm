//! The lift against the machine: each fixture program (`testdata/ppc/ppc.elf`, the integer
//! machine, and `ppcfp.elf`, the floating-point unit), lifted and run over the interpreter,
//! prints what it printed under an emulator of the machine and ends with the same code (its
//! `.out`: the output, then `exit <code>`).

use std::path::PathBuf;

use ra_asm_ppc::run::{Program, run};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ppc").join(name)
}

fn does_what_the_machine_did(name: &str, lines: usize) {
    let program = Program::parse(&std::fs::read(fixture(&format!("{name}.elf"))).unwrap()).unwrap();
    let recorded = std::fs::read_to_string(fixture(&format!("{name}.out"))).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
    let outcome = run(&program, 5_000_000).unwrap();
    assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed);
    assert_eq!(outcome.exit, Some(code));
    assert!(printed.lines().count() >= lines, "the fixture prints {} lines", printed.lines().count());
}

#[test]
fn the_lifted_fixture_does_what_the_machine_did() {
    does_what_the_machine_did("ppc", 14);
}

/// The same program built for the 64-bit machine of the later consoles: a file of the wider
/// class, lifted by the 64-bit lift.
#[test]
fn the_lifted_64_bit_program_does_what_the_machine_did() {
    let program = Program::parse(&std::fs::read(fixture("ppc64.elf")).unwrap()).unwrap();
    assert!(program.wide);
    does_what_the_machine_did("ppc64", 14);
}

#[test]
fn the_lifted_floating_point_does_what_the_machine_did() {
    does_what_the_machine_did("ppcfp", 28);
}

/// The integer program as a DOL file, the GameCube's executable, linked at the console's
/// addresses: the reader knows the file with no mark by its header, the sections are its code
/// and its data, and the lift keeps to the bits of an address that name memory, so the program
/// at 0x80010000 runs in the memory at 0x10000.
#[test]
fn the_lifted_dol_does_what_the_machine_did() {
    let file = std::fs::read(fixture("gc.dol")).unwrap();
    let program = Program::parse(&file).unwrap();
    assert_eq!(program.mask, Some(0x3fff_ffff));
    assert!(program.entry >= 0x8001_0000 && program.segments.iter().any(|s| s.executable));
    assert!(program.segments.iter().all(|s| s.address >= 0x8001_0000));
    let recorded = std::fs::read_to_string(fixture("gc.out")).unwrap().replace("\r\n", "\n");
    let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
    let outcome = run(&program, 5_000_000).unwrap();
    assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed);
    assert_eq!(outcome.exit, ended.strip_prefix("exit ").unwrap().parse().ok());
    // A section said to end past the file, and a file cut inside its header, are refused.
    let mut far = file.clone();
    far[0x90..0x94].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff]);
    assert!(Program::parse(&far).is_err());
    assert!(Program::parse(&file[..0x80]).is_err());
}

/// The floating-point program for the 64-bit machine: a double moved as a doubleword, and a
/// whole number to and from the unit as one.
#[test]
fn the_lifted_64_bit_floating_point_does_what_the_machine_did() {
    does_what_the_machine_did("ppc64fp", 28);
}

#[test]
fn what_is_no_program_is_refused() {
    assert!(Program::parse(b"\x7fELF").is_err());
    let mut other = std::fs::read(fixture("ppc.elf")).unwrap();
    other[19] = 62;
    assert!(Program::parse(&other).unwrap_err().0.contains("machine"));
    // A table said to lie past the file's end.
    let mut far = std::fs::read(fixture("ppc.elf")).unwrap();
    far[28..32].copy_from_slice(&[0xff; 4]);
    assert!(Program::parse(&far).is_err());
}
