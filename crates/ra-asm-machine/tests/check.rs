//! The check of a table on the toy machine of the semantics tests, grown by a delayed jump and an
//! outlined row so that every lift path runs: the toy passes, and a row that no word reaches or
//! whose semantics panics fails by its name and by nothing else.

use ra_asm_arch::{RegClass, Register};
use ra_asm_machine::{Builder, Failure, Field, Fields, Flow, Kind, Machine, Problem, Row, Sem, StateSpec, Syntax, Table, Transfer, check};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const WORD: Field = Field { name: "disp", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "ra", bits: 32 } };
const REL: Kind = Kind::Rel { shift: 2, from: 4 };
const FAR: Field = Field { name: "offset", lo: 0, width: 26, signed: true, kind: REL };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: REL };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const CODE: Field = Field { name: "code", lo: 0, width: 16, signed: false, kind: Kind::Imm };

const OPCODE: u32 = 0xfc00_0000;
const FUNCTION: u32 = OPCODE | 0x7ff;
/// The register `call` leaves the return address in.
const LINK: u32 = 30;

fn nop(_: &mut dyn Builder, _: &Fields<'_>) {}

fn add(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let sum = b.add(32, x, y);
    b.set(f.reg("rd"), sum);
}

fn mul(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let product = b.mul(32, x, y);
    b.set(f.reg("rd"), product);
}

fn addi(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, k) = (b.r(f.reg("ra")), b.imm(32, f.value("imm")));
    let sum = b.add(32, x, k);
    b.set(f.reg("rd"), sum);
}

fn ld(b: &mut dyn Builder, f: &Fields<'_>) {
    let (base, disp) = (b.r(f.reg("ra")), b.imm(32, f.value("disp")));
    let address = b.add(32, base, disp);
    let value = b.load(32, address);
    b.set(f.reg("rd"), value);
}

fn st(b: &mut dyn Builder, f: &Fields<'_>) {
    let (base, disp) = (b.r(f.reg("ra")), b.imm(32, f.value("disp")));
    let address = b.add(32, base, disp);
    let value = b.r(f.reg("rd"));
    b.store(32, address, value);
}

fn jump(b: &mut dyn Builder, f: &Fields<'_>) {
    let target = b.addr(f.target("offset"));
    b.jump(target);
}

fn beq(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let same = b.eq(32, x, y);
    let target = b.addr(f.target("offset"));
    b.branch_if(same, target);
}

fn call(b: &mut dyn Builder, f: &Fields<'_>) {
    let back = b.addr(f.next);
    b.set(LINK, back);
    let target = b.addr(f.target("offset"));
    b.call(target);
}

fn ret(b: &mut dyn Builder, _: &Fields<'_>) {
    let back = b.r(LINK);
    b.jump(back);
}

fn brk(b: &mut dyn Builder, f: &Fields<'_>) {
    b.trap(f.value("code") as u64);
}

/// Reads a field its row does not have, which [`Fields`] refuses with a panic.
fn bad(b: &mut dyn Builder, f: &Fields<'_>) {
    let x = b.r(f.reg("rs"));
    b.set(f.reg("rd"), x);
}

const fn row(
    name: &'static str,
    mask: u32,
    bits: u32,
    fields: &'static [Field],
    flow: Flow,
    lift: fn(&mut dyn Builder, &Fields<'_>),
) -> Row {
    Row { name, mask, bits, fields, flow, outlined: false, sem: Some(Sem(lift)) }
}

const TOY: [Row; 12] = [
    row("nop", u32::MAX, 0, &[], Flow::Next, nop),
    row("add", FUNCTION, 0, &[RD, RA, RB], Flow::Next, add),
    Row { outlined: true, ..row("mul", FUNCTION, 0x18, &[RD, RA, RB], Flow::Next, mul) },
    row("addi", OPCODE, 0x08 << 26, &[RD, RA, IMM], Flow::Next, addi),
    row("ld", OPCODE, 0x10 << 26, &[RD, RA, WORD], Flow::Next, ld),
    row("st", OPCODE, 0x14 << 26, &[RD, RA, WORD], Flow::Next, st),
    row("b", OPCODE, 0x18 << 26, &[FAR], Flow::Jump, jump),
    row("beq", OPCODE, 0x19 << 26, &[BEQ_RA, BEQ_RB, NEAR], Flow::Branch, beq),
    row("call", OPCODE, 0x1a << 26, &[FAR], Flow::Call, call),
    row("bd", OPCODE, 0x1b << 26, &[FAR], Flow::Delayed { transfer: Transfer::Jump, likely: false }, jump),
    row("brk", OPCODE, 0x3e << 26, &[CODE], Flow::Trap, brk),
    row("ret", u32::MAX, 0x3f << 26, &[], Flow::Return, ret),
];

const NAMES: [&str; 32] = [
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "r16", "r17", "r18", "r19",
    "r20", "r21", "r22", "r23", "r24", "r25", "r26", "r27", "r28", "r29", "r30", "sp",
];

const fn file() -> [Register; 32] {
    let mut registers = [Register { id: 0, name: "", class: RegClass::General, full: 0, bits: 32, shift: 0 }; 32];
    let mut n: u16 = 0;
    while n < 32 {
        let class = if n == 31 { RegClass::StackPointer } else { RegClass::General };
        registers[n as usize] = Register { id: n, name: NAMES[n as usize], class, full: n, bits: 32, shift: 0 };
        n += 1;
    }
    registers
}

static REGISTERS: [Register; 32] = file();

const SPEC: StateSpec = StateSpec { registers: 32, bits: 32, pc_bits: 32, zero: Some(0) };

fn machine(rows: &[Row]) -> Machine {
    let table = Table::new(rows).expect("the rows make a table");
    Machine { name: "toy", table, registers: &REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}

/// The toy's rows and `extra`.
fn grown(extra: &[Row]) -> Machine {
    machine(&[TOY.as_slice(), extra].concat())
}

/// The rows the check names, each once, in the table's order.
fn named(failures: &[Failure]) -> Vec<&'static str> {
    let mut rows: Vec<&'static str> = failures.iter().map(|failure| failure.row).collect();
    rows.dedup();
    rows
}

#[test]
fn the_toy_table_passes() {
    let toy = machine(&TOY);
    for seed in [1, 7, 42] {
        if let Err(failures) = check(&toy, &SPEC, seed) {
            let text: Vec<String> = failures.iter().map(ToString::to_string).collect();
            panic!("seed {seed}: {}", text.join("; "));
        }
    }
}

#[test]
fn a_row_no_word_reaches_fails_by_its_name() {
    // `sys` leaves bit 0 open, and the two rows that fix it take both its words.
    let sys = row("sys", OPCODE | 0x03ff_fffe, 0x3c << 26, &[], Flow::Next, nop);
    let sys0 = row("sys0", u32::MAX, 0x3c << 26, &[], Flow::Next, nop);
    let sys1 = row("sys1", u32::MAX, 0x3c << 26 | 1, &[], Flow::Next, nop);
    let failures = check(&grown(&[sys, sys0, sys1]), &SPEC, 7).expect_err("an unreachable row fails");
    assert_eq!(failures, vec![Failure { row: "sys", problem: Problem::Unreachable }]);
}

#[test]
fn a_row_whose_semantics_panics_fails_by_its_name() {
    let broken = row("bad", OPCODE, 0x3c << 26, &[RD], Flow::Next, bad);
    let failures = check(&grown(&[broken]), &SPEC, 7).expect_err("a panicking row fails");
    assert_eq!(named(&failures), ["bad"]);
    let Problem::Panic { pass, message, .. } = &failures[0].problem else { panic!("{} is a panic", failures[0]) };
    assert_eq!(*pass, "access recorder");
    assert!(message.contains("no field `rs`"), "{message}");
}
