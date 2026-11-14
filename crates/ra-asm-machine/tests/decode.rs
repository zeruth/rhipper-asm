//! The table on a toy machine of eight instructions: a six-bit opcode on top, three five-bit
//! registers below it, and `nop` as the one `add` that names register 0 three times.

use ra_asm_machine::{Error, Field, Flow, Row, Table};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true };
const FAR: Field = Field { name: "offset", lo: 0, width: 26, signed: true };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false };
/// A field that runs past bit 31.
const WIDE: Field = Field { name: "wide", lo: 20, width: 13, signed: false };

const OPCODE: u32 = 0xfc00_0000;

const NOP: Row = Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next };
const ADD: Row = Row { name: "add", mask: OPCODE | 0x7ff, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next };
const ADDI: Row = Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next };
const LD: Row = Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, IMM], flow: Flow::Next };
const B: Row = Row { name: "b", mask: OPCODE, bits: 0x18 << 26, fields: &[FAR], flow: Flow::Jump };
const BEQ: Row = Row { name: "beq", mask: OPCODE, bits: 0x19 << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch };
const CALL: Row = Row { name: "call", mask: OPCODE, bits: 0x1a << 26, fields: &[FAR], flow: Flow::Call };
const RET: Row = Row { name: "ret", mask: u32::MAX, bits: 0x3f << 26, fields: &[], flow: Flow::Return };

const TOY: [Row; 8] = [NOP, ADD, ADDI, LD, B, BEQ, CALL, RET];

fn toy() -> Table {
    Table::new(&TOY).expect("the toy rows make a table")
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

#[test]
fn each_toy_row_decodes() {
    let table = toy();
    let words: [(u32, &str, Flow, &[i64]); 8] = [
        (0, "nop", Flow::Next, &[]),
        (regs(3, 4, 5), "add", Flow::Next, &[3, 4, 5]),
        (0x08 << 26 | regs(1, 2, 0) | 0x0010, "addi", Flow::Next, &[1, 2, 16]),
        (0x10 << 26 | regs(7, 31, 0) | 0x0008, "ld", Flow::Next, &[7, 31, 8]),
        (0x18 << 26 | 0x40, "b", Flow::Jump, &[0x40]),
        (0x19 << 26 | regs(6, 9, 0) | 0x0004, "beq", Flow::Branch, &[6, 9, 4]),
        (0x1a << 26 | 0x0100, "call", Flow::Call, &[0x100]),
        (0x3f << 26, "ret", Flow::Return, &[]),
    ];
    for (word, name, flow, values) in words {
        let decoded = table.decode(word).unwrap_or_else(|| panic!("{word:#010x} decodes"));
        assert_eq!(decoded.row.name, name, "{word:#010x}");
        assert_eq!(decoded.row.flow, flow, "{word:#010x}");
        assert_eq!(decoded.values, values, "{word:#010x}");
    }
}

#[test]
fn a_signed_field_sign_extends() {
    let table = toy();
    let back = table.decode(0x18 << 26 | 0x03ff_fffc).expect("b decodes");
    assert_eq!(back.field("offset"), Some(-4));
    let addi = table.decode(0x08 << 26 | regs(1, 1, 0) | 0xffff).expect("addi decodes");
    assert_eq!(addi.field("imm"), Some(-1));
    assert_eq!(addi.field("rd"), Some(1));
    assert_eq!(addi.field("rb"), None);
}

#[test]
fn a_specific_row_wins_over_a_general_one() {
    let table = toy();
    assert_eq!(table.decode(0).expect("word 0 decodes").row.name, "nop");
    assert_eq!(table.decode(regs(0, 0, 1)).expect("add r0, r0, r1 decodes").row.name, "add");
}

#[test]
fn the_order_of_the_rows_does_not_choose_the_winner() {
    let mut rows = TOY;
    rows.reverse();
    let table = Table::new(&rows).expect("the reversed rows make a table");
    assert_eq!(table.decode(0).expect("word 0 decodes").row.name, "nop");
    assert_eq!(table.decode(regs(2, 0, 0)).expect("add r2, r0, r0 decodes").row.name, "add");
}

#[test]
fn an_overlap_is_refused_with_both_names() {
    // `mov` fixes `rb`, which `add` leaves open, and leaves the opcode open, which `add` fixes:
    // neither holds the other. `nop` holds both, so the overlap it meets first is theirs.
    let mov = Row { name: "mov", mask: 0x7ff | 0x1f << 11, bits: 0, fields: &[RD, RA], flow: Flow::Next };
    let rows = [NOP, ADD, mov];
    let error = Table::new(&rows).expect_err("add and mov overlap");
    let Error::Overlap { first, second, word } = error.clone() else { panic!("an overlap, not {error:?}") };
    assert_eq!((first, second), ("add", "mov"));
    assert_eq!(word & ADD.mask, ADD.bits);
    assert_eq!(word & mov.mask, mov.bits);
    let message = error.to_string();
    assert!(message.contains("`add`") && message.contains("`mov`"), "{message}");
}

#[test]
fn two_rows_of_one_encoding_are_refused() {
    let jmp = Row { name: "jmp", ..B };
    let error = Table::new(&[B, jmp]).expect_err("b and jmp are one encoding");
    assert!(matches!(error, Error::Overlap { first: "b", second: "jmp", .. }), "{error:?}");
}

#[test]
fn an_unknown_word_is_none() {
    let table = toy();
    assert!(table.decode(0x3e << 26).is_none(), "an opcode no row has");
    assert!(table.decode(regs(1, 2, 3) | 0x7).is_none(), "opcode 0 with a function no row has");
    assert!(table.decode(0x3f << 26 | 1).is_none(), "ret with a stray bit");
}

#[test]
fn a_malformed_row_is_refused() {
    let loose = Row { name: "loose", mask: OPCODE, bits: 0x01, fields: &[], flow: Flow::Trap };
    assert_eq!(Table::new(&[loose]).expect_err("a bit outside the mask"), Error::Bits { row: "loose" });
    let wide = Row { name: "far", mask: OPCODE, bits: 0, fields: &[WIDE], flow: Flow::Next };
    assert_eq!(Table::new(&[wide]).expect_err("a field past bit 31"), Error::Field { row: "far", field: "wide" });
}
