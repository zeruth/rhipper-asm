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
    // The build of the base instructions alone, the one with the compressed ones, and the
    // narrow machine's.
    for name in ["rv64", "rv64c", "rv32c"] {
        let program = Program::parse(&std::fs::read(fixture(&format!("{name}.elf"))).unwrap()).unwrap();
        assert_eq!(program.width, if name.starts_with("rv32") { Width::Rv32 } else { Width::Rv64 });
        let recorded = std::fs::read_to_string(fixture(&format!("{name}.out"))).unwrap().replace("\r\n", "\n");
        let (printed, ended) = recorded.trim_end().rsplit_once('\n').unwrap();
        let code: i32 = ended.strip_prefix("exit ").unwrap().parse().unwrap();
        let outcome = run(&program, 5_000_000).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed, "{name}");
        assert_eq!(outcome.exit, Some(code), "{name}");
        // Lifted as its functions, each a function of the IR, the program does the same.
        let module = program.lift_functions().unwrap();
        let functions = module.functions.iter().filter(|f| f.name.starts_with("f_") || f.name.starts_with("rv_f_")).count();
        assert!(functions >= 6, "{name}: {functions} functions found");
        // The file's symbols name them.
        for named in ["f__start", "f_mix", "f_divide", "f_pick"] {
            assert!(module.function(named).is_some(), "{name}: no function {named}");
        }
        let outcome = ra_asm_riscv::run::run_functions(&program, 5_000_000).unwrap_or_else(|e| panic!("{name}, as functions: {e}"));
        assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed, "{name}, as functions");
        assert_eq!(outcome.exit, Some(code), "{name}, as functions");
        assert!(printed.lines().count() >= 14, "{name}: the fixture prints {} lines", printed.lines().count());
    }
}

/// A file that is almost a program is refused or read as what it now says, and never stops the
/// reader: each fixture with fields of its headers and its tables set to ones, to nothing or to
/// any number, from a fixed seed.
#[test]
fn a_changed_file_never_panics_the_reader() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for name in ["rv64", "rv64c", "rv32c"] {
        let file = std::fs::read(fixture(&format!("{name}.elf"))).unwrap();
        // The file header and the program headers lie at the start, the section headers and
        // the symbols at the end.
        let spans = [(0, 0x200.min(file.len())), (file.len().saturating_sub(0x600), file.len())];
        for round in 0..2000 {
            let mut changed = file.clone();
            for _ in 0..=next() % 3 {
                let (from, to) = spans[(next() % 2) as usize];
                let at = from + (next() as usize) % (to - from);
                let width = [1, 4, 8][(next() % 3) as usize];
                let start = at - at % width;
                let fill = [0xff, 0x00, next() as u8][(next() % 3) as usize];
                for byte in changed.iter_mut().skip(start).take(width) {
                    *byte = fill;
                }
            }
            let read = std::panic::catch_unwind(|| drop(Program::parse(&changed)));
            assert!(read.is_ok(), "{name}, round {round}: the reader panicked");
        }
    }
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
