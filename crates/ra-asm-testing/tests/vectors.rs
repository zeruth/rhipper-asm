//! The instruction vector file: a case with memory, a 128-bit register and a fault go through the
//! writer and the reader unchanged, a zero input register is left out, a changed state is only
//! what changed, and a malformed line is refused with its number.

use std::collections::BTreeMap;

use ra_asm_testing::vectors::{Case, Outcome, State, changed, parse, unhex, write};

fn registers(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
    pairs.iter().map(|(name, digits)| ((*name).to_owned(), unhex(digits).expect("hex"))).collect()
}

fn memory(cells: &[(u64, &str)]) -> BTreeMap<u64, Vec<u8>> {
    cells.iter().map(|(address, digits)| (*address, unhex(digits).expect("hex"))).collect()
}

#[test]
fn a_case_with_memory_round_trips() {
    // mov rax, [rcx]: the cell at rcx read, rax written.
    let case = Case {
        bytes: vec![0x48, 0x8b, 0x01],
        input: State { registers: registers(&[("rcx", "0000000000401000")]), memory: memory(&[(0x40_1000, "efcdab8967452301")]) },
        outcome: Outcome::State(State { registers: registers(&[("rax", "0123456789abcdef")]), memory: BTreeMap::new() }),
    };
    let text = write(&[case.clone()]);
    assert_eq!(text, "488b01 rcx=0000000000401000 [401000]=efcdab8967452301 -> rax=0123456789abcdef\n");
    assert_eq!(parse(&text), Ok(vec![case]));
}

#[test]
fn a_128_bit_register_round_trips() {
    let case = Case {
        bytes: vec![0x66, 0x0f, 0xef, 0xc1],
        input: State {
            registers: registers(&[("xmm0", "ffffffffffffffff0000000000000000"), ("xmm1", "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f")]),
            memory: BTreeMap::new(),
        },
        outcome: Outcome::State(State { registers: registers(&[("xmm0", "f0f0f0f0f0f0f0f00f0f0f0f0f0f0f0f")]), memory: BTreeMap::new() }),
    };
    let cases = parse(&write(&[case.clone()])).expect("read back");
    assert_eq!(cases, [case]);
    assert_eq!(cases[0].input.registers["xmm0"].len(), 16);
}

#[test]
fn an_expected_fault_round_trips() {
    let case = Case {
        bytes: vec![0x48, 0xf7, 0xf1],
        input: State { registers: registers(&[("rax", "0000000000000007")]), memory: BTreeMap::new() },
        outcome: Outcome::Fault("de".to_owned()),
    };
    let text = write(&[case.clone()]);
    assert_eq!(text, "48f7f1 rax=0000000000000007 -> fault=de\n");
    assert_eq!(parse(&text), Ok(vec![case]));
}

#[test]
fn a_zero_input_register_is_left_out_and_comments_are_skipped() {
    let case = Case {
        bytes: vec![0x90],
        input: State { registers: registers(&[("rbx", "0000000000000000"), ("rcx", "01")]), memory: BTreeMap::new() },
        outcome: Outcome::State(State::default()),
    };
    assert_eq!(write(&[case]), "90 rcx=01 ->\n");
    let text = "# the nop group\n\n  90 ->   # nothing changes\n90 rcx=1 -> # a one\n";
    let cases = parse(text).expect("comments and blanks are skipped");
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[1].input.registers["rcx"], [1]);
}

#[test]
fn changed_is_only_what_differs() {
    let before = State { registers: registers(&[("rax", "01"), ("rbx", "02")]), memory: memory(&[(0x10, "aa"), (0x20, "bb")]) };
    let after = State {
        registers: registers(&[("rax", "01"), ("rbx", "03"), ("rcx", "00"), ("rdx", "04")]),
        memory: memory(&[(0x10, "aa"), (0x20, "cc"), (0x30, "dd")]),
    };
    let out = changed(&before, &after);
    assert_eq!(out.registers, registers(&[("rbx", "03"), ("rdx", "04")]));
    assert_eq!(out.memory, memory(&[(0x20, "cc"), (0x30, "dd")]));
}

#[test]
fn a_line_without_an_arrow_is_refused_with_its_number() {
    let text = "90 ->\n# a comment\n4889c8 rax=1 rcx=2\n90 ->\n";
    let error = parse(text).unwrap_err();
    assert_eq!(error.line, 3);
    assert!(error.message.contains("->"), "{}", error.message);
    assert_eq!(error.to_string(), format!("line 3: {}", error.message));
}

#[test]
fn bad_values_are_refused_with_their_number() {
    let line = |text: &str| parse(text).unwrap_err().line;
    assert_eq!(line("\n\n90 rax=zz ->"), 3);
    assert_eq!(line("9 ->"), 1);
    assert_eq!(line("90 rax=1 rax=2 ->"), 1);
    assert_eq!(line("90 -> fault=de rax=1"), 1);
    assert_eq!(line("90 [10000000000000000]=1 ->"), 1);
    assert_eq!(line("90 1ax=1 ->"), 1);
    assert_eq!(line("90 -> fault="), 1);
}
