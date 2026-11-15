//! The assembler and the cases on the toy machine of the decoder's tests: a six-bit opcode on top,
//! three five-bit registers below it, and `nop` as the one `add` that names register 0 three times.

use ra_asm_machine::{AsmError, Field, Flow, Kind, Plan, Row, Table, assemble, cases};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const FAR: Field = Field { name: "offset", lo: 0, width: 26, signed: true, kind: Kind::Rel { shift: 2, from: 4 } };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Rel { shift: 2, from: 4 } };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };

const OPCODE: u32 = 0xfc00_0000;

const NOP: Row = Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next, outlined: false, sem: None };
const ADD: Row = Row { name: "add", mask: OPCODE | 0x7ff, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: None };
const ADDI: Row =
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, outlined: false, sem: None };
const LD: Row = Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, outlined: false, sem: None };
const B: Row = Row { name: "b", mask: OPCODE, bits: 0x18 << 26, fields: &[FAR], flow: Flow::Jump, outlined: false, sem: None };
const BEQ: Row =
    Row { name: "beq", mask: OPCODE, bits: 0x19 << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch, outlined: false, sem: None };
const CALL: Row = Row { name: "call", mask: OPCODE, bits: 0x1a << 26, fields: &[FAR], flow: Flow::Call, outlined: false, sem: None };
const RET: Row = Row { name: "ret", mask: u32::MAX, bits: 0x3f << 26, fields: &[], flow: Flow::Return, outlined: false, sem: None };

const TOY: [Row; 8] = [NOP, ADD, ADDI, LD, B, BEQ, CALL, RET];

/// Thirty-two 32-bit registers and a handful of random cases per row.
const PLAN: Plan = Plan { registers: 32, bits: 32, random: 16 };

fn toy() -> Table {
    Table::new(&TOY).expect("the toy rows make a table")
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

#[test]
fn a_row_assembles_to_the_word_it_decodes_from() {
    let words: [(u32, Row, &[i64]); 8] = [
        (0, NOP, &[]),
        (regs(3, 4, 5), ADD, &[3, 4, 5]),
        (0x08 << 26 | regs(1, 2, 0) | 0x0010, ADDI, &[1, 2, 16]),
        (0x10 << 26 | regs(7, 31, 0) | 0xfff8, LD, &[7, 31, -8]),
        (0x18 << 26 | 0x03ff_fffc, B, &[-4]),
        (0x19 << 26 | regs(6, 9, 0) | 0x0004, BEQ, &[6, 9, 4]),
        (0x1a << 26 | 0x0100, CALL, &[0x100]),
        (0x3f << 26, RET, &[]),
    ];
    for (word, row, values) in words {
        assert_eq!(assemble(&row, values), Ok(word), "{}", row.name);
    }
}

#[test]
fn values_that_make_no_word_are_refused() {
    assert_eq!(assemble(&ADD, &[1, 2]), Err(AsmError::Count { row: "add", expected: 3, given: 2 }));
    assert_eq!(assemble(&ADD, &[32, 0, 0]), Err(AsmError::Range { row: "add", field: "rd", value: 32 }));
    assert_eq!(assemble(&ADDI, &[0, 0, 0x8000]), Err(AsmError::Range { row: "addi", field: "imm", value: 0x8000 }));
    assert_eq!(assemble(&ADDI, &[0, 0, -0x8000]), Ok(0x08 << 26 | 0x8000));
    // A field over the bits the row fixes: the opcode of `ret` is no register.
    let over = Row { name: "over", fields: &[Field { name: "top", lo: 26, width: 6, signed: false, kind: Kind::Imm }], ..RET };
    assert_eq!(assemble(&over, &[0x3f]), Ok(0x3f << 26));
    assert_eq!(assemble(&over, &[0x01]), Err(AsmError::Conflict { row: "over", field: "top" }));
}

#[test]
fn every_case_decodes_to_the_row_it_came_from() {
    let table = toy();
    for row in table.rows() {
        let found = cases(&table, row, PLAN, 7);
        assert!(!found.is_empty(), "{} has cases", row.name);
        for case in &found {
            let decoded = table.decode(case.word).unwrap_or_else(|| panic!("{:#010x} of {} decodes", case.word, row.name));
            assert_eq!(decoded.row.name, row.name, "{:#010x}", case.word);
            assert_eq!(decoded.values, case.values, "{:#010x}", case.word);
            assert_eq!(case.registers.len(), PLAN.registers, "{:#010x}", case.word);
            assert!(case.registers.iter().all(|&value| value <= u64::from(u32::MAX)), "{:#010x}", case.word);
        }
    }
    // `add r0, r0, r0` is `nop`'s word, so `add`'s cases leave it out.
    let add = cases(&table, &ADD, PLAN, 7);
    assert!(add.iter().all(|case| case.word != 0));
}

#[test]
fn the_same_seed_gives_the_same_cases() {
    let table = toy();
    for row in table.rows() {
        assert_eq!(cases(&table, row, PLAN, 42), cases(&table, row, PLAN, 42), "{}", row.name);
    }
    assert_ne!(cases(&table, &ADDI, PLAN, 42), cases(&table, &ADDI, PLAN, 43), "another seed, other cases");
}

#[test]
fn each_field_meets_its_boundaries() {
    let table = toy();
    let addi = cases(&table, &ADDI, PLAN, 1);
    for (at, name) in [(0, "rd"), (1, "ra")] {
        for register in [0, 1, 30, 31] {
            assert!(addi.iter().any(|case| case.values[at] == register), "{name} = r{register}");
        }
    }
    for imm in [-0x8000, -0x7fff, -1, 0, 1, 0x7ffe, 0x7fff] {
        assert!(addi.iter().any(|case| case.values[2] == imm), "imm = {imm}");
    }
    assert!(addi.iter().any(|case| case.values[0] == case.values[1]), "rd and ra name one register");
    let b = cases(&table, &B, PLAN, 1);
    for offset in [-(1 << 25), (1 << 25) - 1] {
        assert!(b.iter().any(|case| case.values[0] == offset), "offset = {offset}");
    }
}

#[test]
fn registers_name_only_the_machines_and_start_at_its_boundaries() {
    let table = toy();
    let small = Plan { registers: 8, bits: 16, random: 64 };
    let add = cases(&table, &ADD, small, 3);
    assert!(add.iter().all(|case| case.values.iter().all(|&register| (0..8).contains(&register))));
    assert!(add.iter().any(|case| case.values[0] == 7));
    let values: Vec<u64> = add.iter().flat_map(|case| case.registers.iter().copied()).collect();
    assert!(values.iter().all(|&value| value <= 0xffff));
    for edge in [0, 1, 0xffff, 0x8000, 0x7fff] {
        assert!(values.contains(&edge), "a register starts at {edge:#x}");
    }
}
