//! The toy machine the generator and the replay are tried on: 32 registers of 32 bits with `r0`
//! reading zero, and six rows of fixed 32-bit words: `add`, `mix` (an `add` that scribbles on
//! [`SCRATCH`]), `addi`, the loads and stores `ld` and `st`, and `beq`.

use ra_asm_arch::{RegClass, Register};
use ra_asm_machine::{Builder, Field, Fields, Flow, Kind, Machine, Row, Sem, StateSpec, Syntax, Table};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const DISP: Field = Field { name: "disp", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "ra", bits: 32 } };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Rel { shift: 2, from: 4 } };

const OPCODE: u32 = 0xfc00_0000;
const FUNCTION: u32 = OPCODE | 0x7ff;
/// The register `mix` scribbles on, which its cases declare undefined.
pub const SCRATCH: u32 = 1;

fn add(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let sum = b.add(32, x, y);
    b.set(f.reg("rd"), sum);
}

/// `add`, leaving the exclusive or of its operands in [`SCRATCH`].
fn mix(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let sum = b.add(32, x, y);
    let noise = b.xor(32, x, y);
    b.set(f.reg("rd"), sum);
    b.set(SCRATCH, noise);
}

fn addi(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, k) = (b.r(f.reg("ra")), b.imm(32, f.value("imm")));
    let sum = b.add(32, x, k);
    b.set(f.reg("rd"), sum);
}

fn effective(b: &mut dyn Builder, f: &Fields<'_>) -> ra_asm_machine::Val {
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

fn beq(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let same = b.eq(32, x, y);
    let target = b.addr(f.target("offset"));
    b.branch_if(same, target);
}

pub const TOY_ROWS: [Row; 6] = [
    Row { name: "add", mask: FUNCTION, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: Some(Sem(add)) },
    Row { name: "mix", mask: FUNCTION, bits: 0x2b, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: Some(Sem(mix)) },
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, outlined: false, sem: Some(Sem(addi)) },
    Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, DISP], flow: Flow::Next, outlined: false, sem: Some(Sem(ld)) },
    Row { name: "st", mask: OPCODE, bits: 0x14 << 26, fields: &[RD, RA, DISP], flow: Flow::Next, outlined: false, sem: Some(Sem(st)) },
    Row {
        name: "beq",
        mask: OPCODE,
        bits: 0x19 << 26,
        fields: &[BEQ_RA, BEQ_RB, NEAR],
        flow: Flow::Branch,
        outlined: false,
        sem: Some(Sem(beq)),
    },
];

const TOY_NAMES: [&str; 32] = [
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "r16", "r17", "r18", "r19",
    "r20", "r21", "r22", "r23", "r24", "r25", "r26", "r27", "r28", "r29", "r30", "r31",
];

const fn toy_file() -> [Register; 32] {
    let mut registers = [Register { id: 0, name: "", class: RegClass::General, full: 0, bits: 32, shift: 0 }; 32];
    let mut n: u16 = 0;
    while n < 32 {
        registers[n as usize] = Register { id: n, name: TOY_NAMES[n as usize], class: RegClass::General, full: n, bits: 32, shift: 0 };
        n += 1;
    }
    registers
}

static TOY_REGISTERS: [Register; 32] = toy_file();

pub const TOY_SPEC: StateSpec = StateSpec { registers: 32, bits: 32, pc_bits: 32, zero: Some(0) };

pub fn toy_machine() -> Machine {
    let table = Table::new(&TOY_ROWS).expect("the rows make a table");
    Machine { name: "toy", table, registers: &TOY_REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}
