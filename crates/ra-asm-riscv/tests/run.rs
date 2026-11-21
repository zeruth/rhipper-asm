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
        let functions = module.functions.iter().filter(|f| f.name.starts_with("rv_f_")).count();
        assert!(functions >= 6, "{name}: {functions} functions found");
        let outcome = ra_asm_riscv::run::run_functions(&program, 5_000_000).unwrap_or_else(|e| panic!("{name}, as functions: {e}"));
        assert_eq!(String::from_utf8_lossy(&outcome.out).trim_end(), printed, "{name}, as functions");
        assert_eq!(outcome.exit, Some(code), "{name}, as functions");
        assert!(printed.lines().count() >= 14, "{name}: the fixture prints {} lines", printed.lines().count());
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
