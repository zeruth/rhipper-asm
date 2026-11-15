//! The semantics of rows on the toy machine of the decode tests, grown by a comparison, a byte
//! load, a store and a breakpoint: each word is lifted alone, run by the interpreter over a state
//! and a little memory, and the state it leaves compared with the one the instruction means. `r0`
//! reads as zero and drops its writes; `call` puts the return address in `r30`, where `ret` finds
//! it.

use ra_asm_arch::{RegClass, Register};
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};
use ra_asm_machine::{Builder, Field, Fields, Flow, Kind, Machine, Row, Sem, StateSpec, Syntax, Table, Val};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const WORD: Field = Field { name: "disp", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "ra", bits: 32 } };
const BYTE: Field = Field { name: "disp", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "ra", bits: 8 } };
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

fn slt(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, y) = (b.r(f.reg("ra")), b.r(f.reg("rb")));
    let less = b.slt(32, x, y);
    let one = b.zext(32, less);
    b.set(f.reg("rd"), one);
}

fn addi(b: &mut dyn Builder, f: &Fields<'_>) {
    let (x, k) = (b.r(f.reg("ra")), b.imm(32, f.value("imm")));
    let sum = b.add(32, x, k);
    b.set(f.reg("rd"), sum);
}

/// The address of a memory operand: the base register plus the displacement.
fn effective(b: &mut dyn Builder, f: &Fields<'_>) -> Val {
    let (base, disp) = (b.r(f.reg("ra")), b.imm(32, f.value("disp")));
    b.add(32, base, disp)
}

fn ld(b: &mut dyn Builder, f: &Fields<'_>) {
    let address = effective(b, f);
    let value = b.load(32, address);
    b.set(f.reg("rd"), value);
}

fn lb(b: &mut dyn Builder, f: &Fields<'_>) {
    let address = effective(b, f);
    let byte = b.load(8, address);
    let value = b.sext(32, byte);
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

const TOY: [Row; 12] = [
    Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next, outlined: false, sem: Some(Sem(nop)) },
    Row { name: "add", mask: FUNCTION, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: Some(Sem(add)) },
    Row { name: "slt", mask: FUNCTION, bits: 0x2a, fields: &[RD, RA, RB], flow: Flow::Next, outlined: false, sem: Some(Sem(slt)) },
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, outlined: false, sem: Some(Sem(addi)) },
    Row { name: "ld", mask: OPCODE, bits: 0x10 << 26, fields: &[RD, RA, WORD], flow: Flow::Next, outlined: false, sem: Some(Sem(ld)) },
    Row { name: "lb", mask: OPCODE, bits: 0x11 << 26, fields: &[RD, RA, BYTE], flow: Flow::Next, outlined: false, sem: Some(Sem(lb)) },
    Row { name: "st", mask: OPCODE, bits: 0x14 << 26, fields: &[RD, RA, WORD], flow: Flow::Next, outlined: false, sem: Some(Sem(st)) },
    Row { name: "b", mask: OPCODE, bits: 0x18 << 26, fields: &[FAR], flow: Flow::Jump, outlined: false, sem: Some(Sem(jump)) },
    Row {
        name: "beq",
        mask: OPCODE,
        bits: 0x19 << 26,
        fields: &[BEQ_RA, BEQ_RB, NEAR],
        flow: Flow::Branch,
        outlined: false,
        sem: Some(Sem(beq)),
    },
    Row { name: "call", mask: OPCODE, bits: 0x1a << 26, fields: &[FAR], flow: Flow::Call, outlined: false, sem: Some(Sem(call)) },
    Row { name: "brk", mask: OPCODE, bits: 0x3e << 26, fields: &[CODE], flow: Flow::Trap, outlined: false, sem: Some(Sem(brk)) },
    Row { name: "ret", mask: u32::MAX, bits: 0x3f << 26, fields: &[], flow: Flow::Return, outlined: false, sem: Some(Sem(ret)) },
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

/// Where the state and the data the loads and stores touch lie.
const STATE: u64 = 0x10_0000;
const DATA: u64 = 0x2000;
/// The address every instruction is lifted at.
const AT: u32 = 0x1000;

fn machine(rows: &[Row]) -> Machine {
    let table = Table::new(rows).expect("the rows make a table");
    Machine { name: "toy", table, registers: &REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}

fn toy() -> Machine {
    machine(&TOY)
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

/// The registers, the program counter and the data of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct State {
    regs: [u32; 32],
    pc: u32,
    data: [u8; 16],
}

/// The state at the instruction, with `regs` set and the rest zero.
fn with(regs: &[(usize, u32)]) -> State {
    let mut state = State { pc: AT, ..State::default() };
    for &(n, value) in regs {
        state.regs[n] = value;
    }
    state
}

/// The runtime of a lifted row: the calls it makes, and its traps as the stop of the run.
#[derive(Default)]
struct Runtime {
    calls: Vec<u64>,
}

impl Host for Runtime {
    fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let (first, second) = (args[0].as_u64(), args[1].as_u64());
        match name {
            "ra_call" => {
                self.calls.push(second);
                Ok(Value::Int(0))
            }
            "ra_trap" => Err(Stop::fault(format!("trap {second:#x} at {first:#x}"))),
            "ra_unsupported" => Err(Stop::fault(format!("unsupported {second:#x} at {first:#x}"))),
            _ => Err(Stop::fault(format!("no runtime function {name}"))),
        }
    }
}

/// What a run of one instruction left.
struct Run {
    state: State,
    calls: Vec<u64>,
    stop: Option<Stop>,
}

/// Lifts `word` at [`AT`] on `machine` and runs it over `before`.
fn run(machine: &Machine, word: u32, before: &State) -> Run {
    let function = machine.lift(&SPEC, word, u64::from(AT)).expect("a row accepts the word");
    let name = function.name.clone();
    let mut module = Module::x86_64_windows("toy");
    SPEC.declare(&mut module);
    module.functions.push(function);
    let mut memory = Memory::new();
    memory.add_region(Region::owned(STATE, SPEC.size(), "state"));
    memory.add_region(Region::owned(DATA, 16, "data"));
    let mut vm = Vm::new(&module, memory);
    for (n, &value) in before.regs.iter().enumerate() {
        vm.memory.write_u32(STATE + SPEC.register_offset(n as u32), value).expect("the state is mapped");
    }
    vm.memory.write_u32(STATE + SPEC.pc_offset(), before.pc).expect("the state is mapped");
    vm.memory.write(DATA, &before.data).expect("the data is mapped");
    let mut runtime = Runtime::default();
    let stop = vm.call_named(&mut runtime, &name, vec![Value::Int(u128::from(STATE))]).err();
    let mut state = State::default();
    for (n, value) in state.regs.iter_mut().enumerate() {
        *value = vm.memory.read_u32(STATE + SPEC.register_offset(n as u32)).expect("the state is mapped");
    }
    state.pc = vm.memory.read_u32(STATE + SPEC.pc_offset()).expect("the state is mapped");
    vm.memory.read(DATA, &mut state.data).expect("the data is mapped");
    Run { state, calls: runtime.calls, stop }
}

/// Runs `word` over `before` and checks it went on to the next instruction without a call.
fn next(word: u32, before: &State) -> State {
    let run = run(&toy(), word, before);
    assert_eq!(run.stop, None);
    assert!(run.calls.is_empty());
    assert_eq!(run.state.pc, AT + 4);
    run.state
}

#[test]
fn nop_changes_nothing_but_the_program_counter() {
    let before = with(&[(1, 1), (31, 0x8000)]);
    assert_eq!(next(0, &before), State { pc: AT + 4, ..before });
}

#[test]
fn add_wraps_at_the_register_width() {
    let before = with(&[(4, 7), (5, 0xffff_fffe)]);
    assert_eq!(next(regs(3, 4, 5), &before), State { pc: AT + 4, ..with(&[(3, 5), (4, 7), (5, 0xffff_fffe)]) });
}

#[test]
fn slt_compares_signed() {
    let before = with(&[(4, 0xffff_ffff), (5, 1)]);
    assert_eq!(next(regs(3, 4, 5) | 0x2a, &before), State { pc: AT + 4, ..with(&[(3, 1), (4, 0xffff_ffff), (5, 1)]) });
    let before = with(&[(3, 9), (4, 1), (5, 0xffff_ffff)]);
    assert_eq!(next(regs(3, 4, 5) | 0x2a, &before), State { pc: AT + 4, ..with(&[(3, 0), (4, 1), (5, 0xffff_ffff)]) });
}

#[test]
fn addi_adds_its_immediate_sign_extended() {
    let before = with(&[(2, 0x100)]);
    assert_eq!(next(0x08 << 26 | regs(1, 2, 0) | 0xfff0, &before), State { pc: AT + 4, ..with(&[(1, 0xf0), (2, 0x100)]) });
}

#[test]
fn ld_loads_a_word_from_base_plus_displacement() {
    let mut before = with(&[(31, DATA as u32)]);
    before.data[8..12].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
    let mut after = State { pc: AT + 4, ..before.clone() };
    after.regs[7] = 0x1234_5678;
    assert_eq!(next(0x10 << 26 | regs(7, 31, 0) | 8, &before), after);
}

#[test]
fn lb_sign_extends_its_byte() {
    let mut before = with(&[(7, 0x55), (31, DATA as u32)]);
    before.data[3] = 0x80;
    let mut after = State { pc: AT + 4, ..before.clone() };
    after.regs[7] = 0xffff_ff80;
    assert_eq!(next(0x11 << 26 | regs(7, 31, 0) | 3, &before), after);
}

#[test]
fn st_stores_a_word_at_base_plus_displacement() {
    let before = with(&[(7, 0xdead_beef), (31, DATA as u32)]);
    let mut after = State { pc: AT + 4, ..before.clone() };
    after.data[4..8].copy_from_slice(&[0xef, 0xbe, 0xad, 0xde]);
    assert_eq!(next(0x14 << 26 | regs(7, 31, 0) | 4, &before), after);
}

#[test]
fn b_sets_the_program_counter_to_its_target() {
    let before = with(&[(1, 1)]);
    let run = run(&toy(), 0x18 << 26 | 0x03ff_fffc, &before);
    assert_eq!(run.stop, None);
    assert_eq!(run.state, State { pc: 0xff4, ..before });
}

#[test]
fn beq_goes_to_its_target_only_when_equal() {
    let word = 0x19 << 26 | regs(6, 9, 0) | 0x0004;
    let equal = with(&[(6, 3), (9, 3)]);
    assert_eq!(run(&toy(), word, &equal).state, State { pc: 0x1014, ..equal });
    let unequal = with(&[(6, 3), (9, 4)]);
    assert_eq!(next(word, &unequal), State { pc: AT + 4, ..unequal });
}

#[test]
fn call_links_and_calls_its_target() {
    let before = with(&[(1, 1)]);
    let run = run(&toy(), 0x1a << 26 | 0x0100, &before);
    assert_eq!(run.stop, None);
    assert_eq!(run.calls, [0x1404]);
    assert_eq!(run.state, State { pc: AT + 4, ..with(&[(1, 1), (LINK as usize, AT + 4)]) });
}

#[test]
fn ret_goes_to_the_link_register() {
    let before = with(&[(LINK as usize, 0x1234)]);
    let run = run(&toy(), 0x3f << 26, &before);
    assert_eq!(run.stop, None);
    assert_eq!(run.state, State { pc: 0x1234, ..before });
}

#[test]
fn brk_traps_with_its_code_and_leaves_the_state() {
    let before = with(&[(1, 1)]);
    let run = run(&toy(), 0x3e << 26 | 7, &before);
    assert_eq!(run.stop, Some(Stop::Fault(format!("trap 0x7 at {AT:#x}"))));
    assert_eq!(run.state, before);
}

#[test]
fn the_zero_register_reads_zero_and_drops_its_writes() {
    let before = with(&[(4, 7), (5, 8)]);
    assert_eq!(next(regs(0, 4, 5), &before), State { pc: AT + 4, ..before }, "add r0, r4, r5 writes nothing");
    // A stale value in the slot of r0 is not what r0 reads.
    let before = with(&[(0, 0x55)]);
    assert_eq!(next(0x08 << 26 | regs(3, 0, 0) | 5, &before), State { pc: AT + 4, ..with(&[(0, 0x55), (3, 5)]) });
}

#[test]
fn a_row_without_semantics_lifts_to_unsupported() {
    let halt = Row { name: "halt", mask: u32::MAX, bits: 0x3d << 26, fields: &[], flow: Flow::Trap, outlined: false, sem: None };
    let before = with(&[(1, 1)]);
    let run = run(&machine(&[halt]), 0x3d << 26, &before);
    assert_eq!(run.stop, Some(Stop::Fault(format!("unsupported 0xf4000000 at {AT:#x}"))));
    assert_eq!(run.state, before);
}

#[test]
fn an_unknown_word_lifts_to_nothing() {
    assert!(toy().lift(&SPEC, 0x3c << 26, u64::from(AT)).is_none());
}
