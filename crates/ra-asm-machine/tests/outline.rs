//! An outlined row of the toy machine, `mac` (`rd += ra * rb`), lifted as a call to its one
//! function and run by the interpreter against the same program lifted with every row inline.
//! `ret` goes to the address in `r30`; `bd` is a `beq` with a delay slot.

use ra_asm_arch::{Access, RegClass, Register};
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{Callee, FnType, Function, Inst, Module};
use ra_asm_machine::{
    Builder, Code, Error, Field, Fields, Flow, Kind, Machine, Outlines, Row, Sem, StateSpec, Summary, Syntax, Table, Transfer,
};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Rel { shift: 2, from: 4 } };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };

const OPCODE: u32 = 0xfc00_0000;
/// The register `ret` finds the return address in.
const LINK: u32 = 30;

const ADDI: u32 = 0x08;
const BD: u32 = 0x1d;
const MAC: u32 = 0x20;
const RET: u32 = 0x3f << 26;

fn addi(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, k) = (b.r(f.reg("ra")), b.imm(32, f.value("imm")));
    let sum = b.add(32, x, k);
    b.set(f.reg("rd"), sum);
}

fn mac(b: &mut dyn Builder, f: &Fields<'_>) {
    let (acc, x, y) = (b.r(f.reg("rd")), b.r(f.reg("ra")), b.r(f.reg("rb")));
    let product = b.mul(32, x, y);
    let sum = b.add(32, acc, product);
    b.set(f.reg("rd"), sum);
}

fn beq(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let same = b.eq(32, x, y);
    let target = b.addr(f.target("offset"));
    b.branch_if(same, target);
}

fn ret(b: &mut dyn Builder, _: &Fields<'_>) {
    let back = b.r(LINK);
    b.jump(back);
}

/// The toy's rows, `mac` outlined when `outlined` says so.
fn rows(outlined: bool) -> [Row; 4] {
    [
        Row {
            name: "addi",
            mask: OPCODE,
            bits: ADDI << 26,
            fields: &[RD, RA, IMM],
            flow: Flow::Next,
            outlined: false,
            sem: Some(Sem(addi)),
        },
        Row { name: "mac", mask: OPCODE, bits: MAC << 26, fields: &[RD, RA, RB], flow: Flow::Next, outlined, sem: Some(Sem(mac)) },
        Row {
            name: "bd",
            mask: OPCODE,
            bits: BD << 26,
            fields: &[BEQ_RA, BEQ_RB, NEAR],
            flow: Flow::Delayed { transfer: Transfer::Branch, likely: false },
            outlined: false,
            sem: Some(Sem(beq)),
        },
        Row { name: "ret", mask: u32::MAX, bits: RET, fields: &[], flow: Flow::Return, outlined: false, sem: Some(Sem(ret)) },
    ]
}

const fn file() -> [Register; 32] {
    let mut registers = [Register { id: 0, name: "r", class: RegClass::General, full: 0, bits: 32, shift: 0 }; 32];
    let mut n: u16 = 0;
    while n < 32 {
        registers[n as usize] = Register { id: n, name: "r", class: RegClass::General, full: n, bits: 32, shift: 0 };
        n += 1;
    }
    registers
}

static REGISTERS: [Register; 32] = file();

const SPEC: StateSpec = StateSpec { registers: 32, bits: 32, pc_bits: 32, zero: Some(0) };

/// Where the state lies.
const STATE: u64 = 0x10_0000;
/// Where every function's code starts, and its entry.
const BASE: u32 = 0x1000;
/// Where `ret` goes.
const CALLER: u32 = 0x4444;

fn toy(outlined: bool) -> Machine {
    let table = Table::new(&rows(outlined)).expect("the rows make a table");
    Machine { name: "toy", table, registers: &REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}

fn mac_word(rd: u32, ra: u32, rb: u32) -> u32 {
    MAC << 26 | rd << 21 | ra << 16 | rb << 11
}

fn addi_word(rd: u32, ra: u32, imm: i16) -> u32 {
    ADDI << 26 | rd << 21 | ra << 16 | u32::from(imm as u16)
}

/// A `bd` comparing `ra` and `rb`, at `from` to `to`.
fn bd_word(ra: u32, rb: u32, from: u32, to: u32) -> u32 {
    BD << 26 | ra << 21 | rb << 16 | ((to.wrapping_sub(from + 4) >> 2) & 0xffff)
}

/// The code of `words` at [`BASE`].
fn code(words: &[u32]) -> Code<'_> {
    Code { base: u64::from(BASE), words }
}

/// The function of `words` lifted with `mac` a call into `outlines`.
fn outlined(words: &[u32], blocks: &[u64], outlines: &mut Outlines) -> Function {
    toy(true).lift_function_outlined(&SPEC, &code(words), u64::from(BASE), blocks, outlines)
}

/// The function of `words` lifted with every row inline.
fn inline(words: &[u32], blocks: &[u64]) -> Function {
    toy(false).lift_function(&SPEC, &code(words), u64::from(BASE), blocks)
}

/// The calls of `function` to `name`.
fn calls_to(function: &Function, name: &str) -> usize {
    let calls = function.blocks.iter().flat_map(|block| &block.instrs);
    calls.filter(|instr| matches!(&instr.inst, Inst::Call { callee: Callee::Direct(callee), .. } if callee == name)).count()
}

struct Runtime;

impl Host for Runtime {
    fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("{name} {:#x} at {:#x}", args[1].as_u64(), args[0].as_u64())))
    }
}

/// The registers and the program counter `function` leaves, with `others` in the module, run over
/// `regs` and the link register at [`CALLER`].
fn run(function: Function, others: &[Function], regs: &[(u32, u32)]) -> (Vec<u32>, u32) {
    let name = function.name.clone();
    let mut module = Module::x86_64_windows("toy");
    SPEC.declare(&mut module);
    module.functions.push(function);
    module.functions.extend(others.iter().cloned());
    let mut memory = Memory::new();
    memory.add_region(Region::owned(STATE, SPEC.size(), "state"));
    let mut vm = Vm::new(&module, memory);
    for &(n, value) in regs.iter().chain(&[(LINK, CALLER)]) {
        vm.memory.write_u32(STATE + SPEC.register_offset(n), value).expect("the state is mapped");
    }
    let stop = vm.call_named(&mut Runtime, &name, vec![Value::Int(u128::from(STATE))]).err();
    assert_eq!(stop, None);
    let registers = (0..32).map(|n| vm.memory.read_u32(STATE + SPEC.register_offset(n)).expect("the state is mapped")).collect();
    (registers, vm.memory.read_u32(STATE + SPEC.pc_offset()).expect("the state is mapped"))
}

/// `mac r3, r1, r2`, `r1 += 1`, `mac r3, r1, r2`, `ret`.
fn twice() -> [u32; 4] {
    [mac_word(3, 1, 2), addi_word(1, 1, 1), mac_word(3, 1, 2), RET]
}

#[test]
fn an_outlined_row_used_twice_is_one_function_and_two_calls() {
    let mut outlines = Outlines::new();
    let function = outlined(&twice(), &[], &mut outlines);
    let [only] = outlines.functions() else { panic!("one function for the row, not {}", outlines.functions().len()) };
    assert_eq!(only.name, "mac_3_1_2", "named after the row and its fields");
    assert_eq!(calls_to(&function, &only.name), 2);
    assert_eq!(calls_to(&inline(&twice(), &[]), &only.name), 0, "the inline form has no call");
    // Another function of the program calls the same one.
    let again = outlined(&[mac_word(3, 1, 2), RET], &[], &mut outlines);
    assert_eq!(outlines.functions().len(), 1);
    assert_eq!(calls_to(&again, "mac_3_1_2"), 1);
}

#[test]
fn other_field_values_are_another_function() {
    let mut outlines = Outlines::new();
    outlined(&[mac_word(3, 1, 2), mac_word(4, 1, 2), mac_word(3, 1, 2), RET], &[], &mut outlines);
    let names: Vec<&str> = outlines.functions().iter().map(|function| function.name.as_str()).collect();
    assert_eq!(names, ["mac_3_1_2", "mac_4_1_2"]);
}

#[test]
fn the_outlined_form_leaves_the_state_the_inline_form_does() {
    let regs = [(1, 2), (2, 5), (3, 1)];
    let mut outlines = Outlines::new();
    let called = run(outlined(&twice(), &[], &mut outlines), outlines.functions(), &regs);
    let inlined = run(inline(&twice(), &[]), &[], &regs);
    assert_eq!(called, inlined);
    // r3 = 1 + 2 * 5, then r1 = 3 and r3 = 11 + 3 * 5.
    assert_eq!((called.0[1], called.0[3], called.1), (3, 26, CALLER));
}

#[test]
fn an_outlined_row_in_a_delay_slot_is_a_call_there() {
    // bd r0, r0 is always taken, past the addi, and its slot is the mac.
    let words = [bd_word(0, 0, 0x1000, 0x100c), mac_word(3, 1, 2), addi_word(3, 0, 99), RET];
    let regs = [(1, 4), (2, 6), (3, 1)];
    let mut outlines = Outlines::new();
    let function = outlined(&words, &[0x100c], &mut outlines);
    assert_eq!(calls_to(&function, "mac_3_1_2"), 1);
    let called = run(function, outlines.functions(), &regs);
    assert_eq!(called, run(inline(&words, &[0x100c]), &[], &regs));
    assert_eq!(called.0[3], 25);
}

#[test]
fn the_access_summary_is_the_inline_rows() {
    let word = mac_word(3, 1, 2);
    let summary = toy(true).access(word, u64::from(BASE));
    assert_eq!(summary, toy(false).access(word, u64::from(BASE)));
    let registers = vec![(3, Access::ReadWrite), (1, Access::Read), (2, Access::Read)];
    assert_eq!(summary, Some(Summary { registers, reads_memory: false, writes_memory: false }));
}

#[test]
fn an_outlined_row_goes_on_to_the_next_instruction() {
    let mut rows = rows(true).to_vec();
    rows[2].outlined = true;
    assert_eq!(Table::new(&rows).err(), Some(Error::Outlined { row: "bd" }));
}
