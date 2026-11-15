//! What the instructions of the toy machine read and write, from the semantics its rows carry: the
//! rows of the semantics tests, with two more that touch a register after a conditional branch and
//! read a register they wrote, and the `Instr` each summary fills.

use ra_asm_arch::{Access, RegClass, Register};
use ra_asm_machine::{Builder, Field, Fields, Flow, Kind, Machine, Row, Sem, Summary, Syntax, Table, Val};

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

fn addi(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, k) = (b.r(f.reg("ra")), b.imm(32, f.value("imm")));
    let sum = b.add(32, x, k);
    b.set(f.reg("rd"), sum);
}

fn effective(b: &mut dyn Builder, f: &Fields<'_>) -> Val {
    let (base, disp) = (b.r(f.reg("ra")), b.imm(32, f.value("disp")));
    b.add(32, base, disp)
}

fn ld(b: &mut dyn Builder, f: &Fields<'_>) {
    let address = effective(b, f);
    let value = b.load(32, address);
    b.set(f.reg("rd"), value);
}

fn st(b: &mut dyn Builder, f: &Fields<'_>) {
    let address = effective(b, f);
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

/// Goes to its target when `ra` is zero, and otherwise copies `rb` to the link register.
fn skipl(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, zero) = (b.r(f.reg("ra")), b.imm(32, 0));
    let none = b.eq(32, x, zero);
    let target = b.addr(f.target("offset"));
    b.branch_if(none, target);
    let y = b.r(f.reg("rb"));
    b.set(LINK, y);
}

/// Puts the next address in the link register and copies it on to `rd`.
fn link(b: &mut dyn Builder, f: &Fields<'_>) {
    let back = b.addr(f.next);
    b.set(LINK, back);
    let again = b.r(LINK);
    b.set(f.reg("rd"), again);
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

const TOY: [Row; 13] = [
    Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next, sem: Some(Sem(nop)) },
    Row { name: "add", mask: FUNCTION, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, sem: Some(Sem(add)) },
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, sem: Some(Sem(addi)) },
    Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, WORD], flow: Flow::Next, sem: Some(Sem(ld)) },
    Row { name: "st", mask: OPCODE, bits: 0x14 << 26, fields: &[RD, RA, WORD], flow: Flow::Next, sem: Some(Sem(st)) },
    Row { name: "b", mask: OPCODE, bits: 0x18 << 26, fields: &[FAR], flow: Flow::Jump, sem: Some(Sem(jump)) },
    Row { name: "beq", mask: OPCODE, bits: 0x19 << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch, sem: Some(Sem(beq)) },
    Row { name: "call", mask: OPCODE, bits: 0x1a << 26, fields: &[FAR], flow: Flow::Call, sem: Some(Sem(call)) },
    Row { name: "skipl", mask: OPCODE, bits: 0x1b << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch, sem: Some(Sem(skipl)) },
    Row { name: "link", mask: OPCODE | 0x1f_ffff, bits: 0x1c << 26, fields: &[RD], flow: Flow::Next, sem: Some(Sem(link)) },
    Row { name: "halt", mask: u32::MAX, bits: 0x3d << 26, fields: &[], flow: Flow::Trap, sem: None },
    Row { name: "brk", mask: OPCODE, bits: 0x3e << 26, fields: &[CODE], flow: Flow::Trap, sem: Some(Sem(brk)) },
    Row { name: "ret", mask: u32::MAX, bits: 0x3f << 26, fields: &[], flow: Flow::Return, sem: Some(Sem(ret)) },
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

/// The address every instruction is summed up at.
const AT: u64 = 0x1000;

fn toy() -> Machine {
    let table = Table::new(&TOY).expect("the rows make a table");
    Machine { name: "toy", table, registers: &REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

fn access(word: u32) -> Summary {
    toy().access(word, AT).expect("a row accepts the word")
}

/// A summary of registers only.
fn registers(registers: &[(u32, Access)]) -> Summary {
    Summary { registers: registers.to_vec(), ..Summary::default() }
}

#[test]
fn arithmetic_reads_its_sources_and_writes_its_destination() {
    assert_eq!(access(regs(3, 4, 5)), registers(&[(4, Access::Read), (5, Access::Read), (3, Access::Write)]));
    assert_eq!(access(0x08 << 26 | regs(3, 3, 0) | 1), registers(&[(3, Access::ReadWrite)]), "addi r3, r3, 1");
    assert_eq!(access(0), Summary::default(), "nop");
}

#[test]
fn loads_and_stores_say_which_way_memory_goes() {
    let load = Summary { reads_memory: true, ..registers(&[(31, Access::Read), (7, Access::Write)]) };
    assert_eq!(access(0x10 << 26 | regs(7, 31, 0) | 8), load);
    let store = Summary { writes_memory: true, ..registers(&[(31, Access::Read), (7, Access::Read)]) };
    assert_eq!(access(0x14 << 26 | regs(7, 31, 0) | 4), store);
}

#[test]
fn control_touches_the_registers_it_names_and_its_implicit_ones() {
    assert_eq!(access(0x18 << 26 | 1), Summary::default(), "b");
    assert_eq!(access(0x19 << 26 | regs(6, 9, 0) | 4), registers(&[(6, Access::Read), (9, Access::Read)]), "beq");
    assert_eq!(access(0x1a << 26 | 0x100), registers(&[(LINK, Access::Write)]), "call writes the link register");
    assert_eq!(access(0x3f << 26), registers(&[(LINK, Access::Read)]), "ret reads it");
    assert_eq!(access(0x3e << 26 | 7), Summary::default(), "brk");
}

#[test]
fn what_follows_a_conditional_branch_happens_on_some_paths() {
    let word = 0x1b << 26 | regs(6, 9, 0) | 4;
    assert_eq!(access(word), registers(&[(6, Access::Read), (9, Access::CondRead), (LINK, Access::CondWrite)]));
}

#[test]
fn a_register_read_after_it_was_written_is_not_read() {
    assert_eq!(access(0x1c << 26 | 3 << 21), registers(&[(LINK, Access::Write), (3, Access::Write)]));
}

#[test]
fn a_row_without_semantics_touches_nothing_and_an_unknown_word_is_none() {
    assert_eq!(access(0x3d << 26), Summary::default());
    assert!(toy().access(0x3c << 26, AT).is_none());
}

#[test]
fn the_summary_fills_the_instr() {
    let machine = toy();
    let load = machine.instr(0x10 << 26 | regs(7, 31, 0) | 8, AT).expect("ld decodes");
    assert_eq!(load.registers, [(REGISTERS[31], Access::Read), (REGISTERS[7], Access::Write)]);
    assert!(load.reads_memory && !load.writes_memory);
    let call = machine.instr(0x1a << 26 | 0x100, AT).expect("call decodes");
    assert_eq!(call.registers, [(REGISTERS[LINK as usize], Access::Write)]);
    assert!(!call.reads_memory && !call.writes_memory);
}
