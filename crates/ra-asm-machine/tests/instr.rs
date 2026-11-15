//! The neutral record on the toy machine of the decode tests: a six-bit opcode on top, three
//! five-bit registers below it, `r31` named `sp`, and branch offsets in words from the next
//! instruction.

use ra_asm_arch::{Flow as ArchFlow, MemOperand, Operand, RegClass, Register, Target};
use ra_asm_machine::{Error, Field, Flow, Kind, Machine, Row, Syntax, Table};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const DISP: Field = Field { name: "disp", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "ra", bits: 32 } };
const REL: Kind = Kind::Rel { shift: 2, from: 4 };
const FAR: Field = Field { name: "offset", lo: 0, width: 26, signed: true, kind: REL };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: REL };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };

const OPCODE: u32 = 0xfc00_0000;

const TOY: [Row; 8] = [
    Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next, outlined: false, sem: None },
    Row { name: "add", mask: OPCODE | 0x7ff, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: None },
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, outlined: false, sem: None },
    Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, DISP], flow: Flow::Next, outlined: false, sem: None },
    Row { name: "b", mask: OPCODE, bits: 0x18 << 26, fields: &[FAR], flow: Flow::Jump, outlined: false, sem: None },
    Row { name: "beq", mask: OPCODE, bits: 0x19 << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch, outlined: false, sem: None },
    Row { name: "call", mask: OPCODE, bits: 0x1a << 26, fields: &[FAR], flow: Flow::Call, outlined: false, sem: None },
    Row { name: "ret", mask: u32::MAX, bits: 0x3f << 26, fields: &[], flow: Flow::Return, outlined: false, sem: None },
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

const PLAIN: Syntax = Syntax { immediate_prefix: "", hex: false };

fn toy(syntax: Syntax) -> Machine {
    Machine { name: "toy", table: Table::new(&TOY).expect("the toy rows make a table"), registers: &REGISTERS, address_bits: 32, syntax }
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

fn reg(n: usize) -> Operand {
    Operand::Reg(REGISTERS[n])
}

#[test]
fn a_register_form_names_the_machines_registers() {
    let instr = toy(PLAIN).instr(regs(3, 4, 31), 0x1000).expect("add decodes");
    assert_eq!(instr.mnemonic, "add");
    assert_eq!(instr.operands, [reg(3), reg(4), reg(31)]);
    assert_eq!(instr.flow, ArchFlow::Next);
    assert_eq!((instr.address, instr.len, instr.end()), (0x1000, 4, 0x1004));
    assert_eq!(instr.code, 1, "the row's place in the table");
    assert_eq!(instr.text, "add\tr3, r4, sp");
}

#[test]
fn an_immediate_form_keeps_its_sign_and_width() {
    let instr = toy(PLAIN).instr(0x08 << 26 | regs(1, 2, 0) | 0xfff0, 0).expect("addi decodes");
    assert_eq!(instr.operands, [reg(1), reg(2), Operand::Imm { value: -16, bits: 16 }]);
    assert_eq!(instr.text, "addi\tr1, r2, -16");
    let hex = toy(Syntax { immediate_prefix: "#", hex: true }).instr(0x08 << 26 | regs(1, 2, 0) | 0xfff0, 0).expect("addi decodes");
    assert_eq!(hex.text, "addi\tr1, r2, #-0x10");
}

#[test]
fn a_memory_form_takes_its_base_in() {
    let instr = toy(PLAIN).instr(0x10 << 26 | regs(7, 31, 0) | 0x0008, 0).expect("ld decodes");
    let memory = MemOperand { segment: None, base: Some(REGISTERS[31]), index: None, scale: 1, disp: 8, bits: 32, absolute: None };
    assert_eq!(instr.operands, [reg(7), Operand::Mem(memory)]);
    assert_eq!(instr.text, "ld\tr7, 8(sp)");
}

#[test]
fn a_branch_resolves_its_target() {
    let machine = toy(PLAIN);
    let beq = machine.instr(0x19 << 26 | regs(6, 9, 0) | 0x0004, 0x1000).expect("beq decodes");
    assert_eq!(beq.operands, [reg(6), reg(9), Operand::Addr(0x1014)]);
    assert_eq!(beq.flow, ArchFlow::CondJump(Target::Direct(0x1014)));
    assert!(beq.flow.falls_through());
    assert_eq!(beq.text, "beq\tr6, r9, 0x1014");
    let back = machine.instr(0x18 << 26 | 0x03ff_fffc, 0x1000).expect("b decodes");
    assert_eq!(back.flow, ArchFlow::Jump(Target::Direct(0xff4)));
    assert!(!back.flow.falls_through());
    assert_eq!(back.text, "b\t0xff4");
}

#[test]
fn a_call_resolves_its_target() {
    let instr = toy(PLAIN).instr(0x1a << 26 | 0x0100, 0x2000).expect("call decodes");
    assert_eq!(instr.operands, [Operand::Addr(0x2404)]);
    assert_eq!(instr.flow, ArchFlow::Call(Target::Direct(0x2404)));
    assert_eq!(instr.flow.direct_target(), Some(0x2404));
    assert_eq!(instr.text, "call\t0x2404");
}

#[test]
fn a_target_wraps_at_the_address_width() {
    let instr = toy(PLAIN).instr(0x18 << 26, 0xffff_fffc).expect("b decodes");
    assert_eq!(instr.flow, ArchFlow::Jump(Target::Direct(0)));
    assert_eq!(instr.text, "b\t0x0");
}

#[test]
fn a_form_without_operands_is_its_mnemonic() {
    let machine = toy(PLAIN);
    let nop = machine.instr(0, 0).expect("nop decodes");
    assert_eq!((nop.text.as_str(), nop.flow), ("nop", ArchFlow::Next));
    let ret = machine.instr(0x3f << 26, 0).expect("ret decodes");
    assert_eq!((ret.text.as_str(), ret.flow), ("ret", ArchFlow::Return));
    assert!(ret.operands.is_empty());
}

#[test]
fn an_unknown_word_is_none() {
    assert!(toy(PLAIN).instr(0x3e << 26, 0).is_none());
}

#[test]
fn a_memory_field_without_its_base_is_refused() {
    let loose = Row { name: "st", mask: OPCODE, bits: 0x11 << 26, fields: &[RD, DISP], flow: Flow::Next, outlined: false, sem: None };
    assert_eq!(Table::new(&[loose]).expect_err("no field `ra`"), Error::Base { row: "st", field: "disp" });
}
