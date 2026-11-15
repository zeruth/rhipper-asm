//! Functions of the toy machine lifted from their rows and run by the interpreter: a loop, and the
//! delayed rows the toy gains for it, `bd` (a `beq` with a delay slot), `bdl` (its likely form,
//! whose slot runs only when taken) and `cd` (a call with a delay slot), each against the state
//! the program means. `r0` reads as zero; `call` and `cd` put the return address in `r30`, where
//! `ret` finds it.

use ra_asm_arch::{Flow as ArchFlow, RegClass, Register, Target};
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module, Terminator};
use ra_asm_machine::{Builder, Code, Field, Fields, Flow, Kind, Machine, Row, Sem, StateSpec, Syntax, Table, Transfer};

const RD: Field = Field { name: "rd", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RA: Field = Field { name: "ra", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RB: Field = Field { name: "rb", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
const REL: Kind = Kind::Rel { shift: 2, from: 4 };
const FAR: Field = Field { name: "offset", lo: 0, width: 26, signed: true, kind: REL };
const NEAR: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: REL };
const BEQ_RA: Field = Field { name: "ra", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const BEQ_RB: Field = Field { name: "rb", lo: 16, width: 5, signed: false, kind: Kind::Reg };

const OPCODE: u32 = 0xfc00_0000;
const FUNCTION: u32 = OPCODE | 0x7ff;
/// The register `call` and `cd` leave the return address in.
const LINK: u32 = 30;

const B: u32 = 0x18;
const BEQ: u32 = 0x19;
const BD: u32 = 0x1d;
const BDL: u32 = 0x1e;
const CD: u32 = 0x1f;
const RET: u32 = 0x3f << 26;

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

const fn delayed(transfer: Transfer, likely: bool) -> Flow {
    Flow::Delayed { transfer, likely }
}

const TOY: [Row; 9] = [
    Row { name: "nop", mask: u32::MAX, bits: 0, fields: &[], flow: Flow::Next, sem: Some(Sem(nop)) },
    Row { name: "add", mask: FUNCTION, bits: 0, fields: &[RD, RA, RB], flow: Flow::Next, sem: Some(Sem(add)) },
    Row { name: "addi", mask: OPCODE, bits: 0x08 << 26, fields: &[RD, RA, IMM], flow: Flow::Next, sem: Some(Sem(addi)) },
    Row { name: "b", mask: OPCODE, bits: B << 26, fields: &[FAR], flow: Flow::Jump, sem: Some(Sem(jump)) },
    Row { name: "beq", mask: OPCODE, bits: BEQ << 26, fields: &[BEQ_RA, BEQ_RB, NEAR], flow: Flow::Branch, sem: Some(Sem(beq)) },
    Row {
        name: "bd",
        mask: OPCODE,
        bits: BD << 26,
        fields: &[BEQ_RA, BEQ_RB, NEAR],
        flow: delayed(Transfer::Branch, false),
        sem: Some(Sem(beq)),
    },
    Row {
        name: "bdl",
        mask: OPCODE,
        bits: BDL << 26,
        fields: &[BEQ_RA, BEQ_RB, NEAR],
        flow: delayed(Transfer::Branch, true),
        sem: Some(Sem(beq)),
    },
    Row { name: "cd", mask: OPCODE, bits: CD << 26, fields: &[FAR], flow: delayed(Transfer::Call, false), sem: Some(Sem(call)) },
    Row { name: "ret", mask: u32::MAX, bits: RET, fields: &[], flow: Flow::Return, sem: Some(Sem(ret)) },
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

/// Where the state lies.
const STATE: u64 = 0x10_0000;
/// Where every function's code starts, and its entry.
const BASE: u32 = 0x1000;
/// Where `ret` goes when a test leaves the link register as it set it.
const CALLER: u32 = 0x4444;

fn toy() -> Machine {
    let table = Table::new(&TOY).expect("the rows make a table");
    Machine { name: "toy", table, registers: &REGISTERS, address_bits: 32, syntax: Syntax { immediate_prefix: "", hex: false } }
}

fn regs(rd: u32, ra: u32, rb: u32) -> u32 {
    rd << 21 | ra << 16 | rb << 11
}

fn add_word(rd: u32, ra: u32, rb: u32) -> u32 {
    regs(rd, ra, rb)
}

fn addi_word(rd: u32, ra: u32, imm: i16) -> u32 {
    0x08 << 26 | regs(rd, ra, 0) | u32::from(imm as u16)
}

/// The offset field of `bits` bits from the instruction at `from` to `to`.
fn rel(from: u32, to: u32, bits: u32) -> u32 {
    (to.wrapping_sub(from + 4) >> 2) & ((1 << bits) - 1)
}

/// A branch of opcode `op` comparing `ra` and `rb`, at `from` to `to`.
fn branch_word(op: u32, ra: u32, rb: u32, from: u32, to: u32) -> u32 {
    op << 26 | ra << 21 | rb << 16 | rel(from, to, 16)
}

/// A jump or a call of opcode `op`, at `from` to `to`.
fn far_word(op: u32, from: u32, to: u32) -> u32 {
    op << 26 | rel(from, to, 26)
}

/// The registers and the program counter of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct State {
    regs: [u32; 32],
    pc: u32,
}

/// The state at the entry, with `regs` set, the link register at [`CALLER`] and the rest zero.
fn with(regs: &[(usize, u32)]) -> State {
    let mut state = State { pc: BASE, ..State::default() };
    state.regs[LINK as usize] = CALLER;
    for &(n, value) in regs {
        state.regs[n] = value;
    }
    state
}

/// The runtime of a lifted function: each call with the value of `r2` when it was made, and its
/// stops as the stop of the run.
#[derive(Default)]
struct Runtime {
    calls: Vec<(u64, u32)>,
}

impl Host for Runtime {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, _: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let (first, second) = (args[0].as_u64(), args[1].as_u64());
        match name {
            "ra_call" => {
                let r2 = vm.memory.read_u32(STATE + SPEC.register_offset(2))?;
                self.calls.push((second, r2));
                Ok(Value::Int(0))
            }
            "ra_trap" => Err(Stop::fault(format!("trap {second:#x} at {first:#x}"))),
            "ra_unsupported" => Err(Stop::fault(format!("unsupported {second:#x} at {first:#x}"))),
            _ => Err(Stop::fault(format!("no runtime function {name}"))),
        }
    }
}

/// What a run of a function left.
struct Run {
    state: State,
    calls: Vec<(u64, u32)>,
    stop: Option<Stop>,
}

/// Lifts the function of `words` at [`BASE`] with blocks at `blocks` and runs it over `before`.
fn run(words: &[u32], blocks: &[u32], before: &State) -> Run {
    let code = Code { base: u64::from(BASE), words };
    let blocks: Vec<u64> = blocks.iter().copied().map(u64::from).collect();
    let function = toy().lift_function(&SPEC, &code, u64::from(BASE), &blocks);
    let name = function.name.clone();
    let mut module = Module::x86_64_windows("toy");
    SPEC.declare(&mut module);
    module.functions.push(function);
    let mut memory = Memory::new();
    memory.add_region(Region::owned(STATE, SPEC.size(), "state"));
    let mut vm = Vm::new(&module, memory);
    for (n, &value) in before.regs.iter().enumerate() {
        vm.memory.write_u32(STATE + SPEC.register_offset(n as u32), value).expect("the state is mapped");
    }
    vm.memory.write_u32(STATE + SPEC.pc_offset(), before.pc).expect("the state is mapped");
    let mut runtime = Runtime::default();
    let stop = vm.call_named(&mut runtime, &name, vec![Value::Int(u128::from(STATE))]).err();
    let mut state = State::default();
    for (n, value) in state.regs.iter_mut().enumerate() {
        *value = vm.memory.read_u32(STATE + SPEC.register_offset(n as u32)).expect("the state is mapped");
    }
    state.pc = vm.memory.read_u32(STATE + SPEC.pc_offset()).expect("the state is mapped");
    Run { state, calls: runtime.calls, stop }
}

/// Runs the function and checks it returned to [`CALLER`] without a stop or a call.
fn returns(words: &[u32], blocks: &[u32], before: &State) -> State {
    let run = run(words, blocks, before);
    assert_eq!(run.stop, None);
    assert!(run.calls.is_empty());
    assert_eq!(run.state.pc, CALLER);
    run.state
}

/// `before` returned from, with `regs` changed.
fn after(before: &State, regs: &[(usize, u32)]) -> State {
    let mut state = State { pc: CALLER, ..before.clone() };
    for &(n, value) in regs {
        state.regs[n] = value;
    }
    state
}

#[test]
fn a_loop_runs_inside_the_function() {
    // do { r2 += r1; r1 -= 1 } while (r1 != 0)
    let words = [add_word(2, 2, 1), addi_word(1, 1, -1), branch_word(BEQ, 1, 0, 0x1008, 0x1010), far_word(B, 0x100c, 0x1000), RET];
    let blocks = [0x1000, 0x100c, 0x1010];
    let before = with(&[(1, 3)]);
    assert_eq!(returns(&words, &blocks, &before), after(&before, &[(1, 0), (2, 6)]));
    let code = Code { base: u64::from(BASE), words: &words };
    let function = toy().lift_function(&SPEC, &code, u64::from(BASE), &[0x100c, 0x1010]);
    let entry = &function.blocks[0];
    assert!(
        function.blocks.iter().any(|block| block.id != entry.id && block.terminator == Terminator::Br(function.blocks[1].id)),
        "the jump back to the entry address is a branch inside the function"
    );
}

#[test]
fn a_delayed_branch_compares_before_its_slot_changes_the_register() {
    // bd r1, r0 compares r1 with zero; its slot then adds one to r1.
    let words = [branch_word(BD, 1, 0, 0x1000, 0x100c), addi_word(1, 1, 1), addi_word(3, 0, 7), RET];
    let blocks = [0x100c];
    let taken = with(&[]);
    assert_eq!(returns(&words, &blocks, &taken), after(&taken, &[(1, 1)]), "taken on the r1 before the slot");
    let not_taken = with(&[(1, 5)]);
    assert_eq!(returns(&words, &blocks, &not_taken), after(&not_taken, &[(1, 6), (3, 7)]), "the slot runs when not taken");
}

#[test]
fn a_likely_branch_annuls_its_slot_when_not_taken() {
    let words = [branch_word(BDL, 1, 0, 0x1000, 0x100c), addi_word(2, 2, 1), addi_word(3, 0, 7), RET];
    let blocks = [0x100c];
    let taken = with(&[]);
    assert_eq!(returns(&words, &blocks, &taken), after(&taken, &[(2, 1)]), "the slot runs when taken");
    let not_taken = with(&[(1, 5)]);
    assert_eq!(returns(&words, &blocks, &not_taken), after(&not_taken, &[(3, 7)]), "the slot is annulled");
}

#[test]
fn a_slot_that_is_a_jump_target_runs_alone_and_falls_through() {
    // beq r4, r0 goes into the slot of the bd that follows, which then runs as a plain instruction.
    let words = [
        branch_word(BEQ, 4, 0, 0x1000, 0x100c),
        addi_word(2, 2, 10),
        branch_word(BD, 1, 1, 0x1008, 0x1014),
        addi_word(2, 2, 1),
        addi_word(3, 0, 7),
        RET,
    ];
    let blocks = [0x100c, 0x1014];
    let into_slot = with(&[]);
    assert_eq!(returns(&words, &blocks, &into_slot), after(&into_slot, &[(2, 1), (3, 7)]));
    let through = with(&[(4, 1)]);
    assert_eq!(returns(&words, &blocks, &through), after(&through, &[(2, 11)]), "the bd runs its slot and is taken");
}

#[test]
fn a_transfer_in_a_delay_slot_stops_at_the_slot() {
    let slot = far_word(B, 0x1004, 0x1000);
    let words = [branch_word(BD, 1, 0, 0x1000, 0x100c), slot, 0, RET];
    let before = with(&[(1, 5)]);
    let jumped = run(&words, &[0x100c], &before);
    assert_eq!(jumped.stop, Some(Stop::Fault(format!("unsupported {slot:#x} at 0x1004"))));
    assert_eq!(jumped.state, before);
    let slot = branch_word(BDL, 0, 0, 0x1004, 0x100c);
    let words = [branch_word(BDL, 1, 0, 0x1000, 0x100c), slot, 0, RET];
    let branched = run(&words, &[0x100c], &with(&[]));
    assert_eq!(branched.stop, Some(Stop::Fault(format!("unsupported {slot:#x} at 0x1004"))), "a delayed row in a taken likely slot");
}

#[test]
fn a_delayed_call_runs_its_slot_first_and_returns_past_it() {
    let words = [far_word(CD, 0x1000, 0x2000), addi_word(2, 0, 5), RET];
    let before = with(&[]);
    let run = run(&words, &[], &before);
    assert_eq!(run.stop, None);
    assert_eq!(run.calls, [(0x2000, 5)], "the slot set r2 before the call");
    assert_eq!(run.state, State { pc: 0x1008, ..after(&before, &[(2, 5), (LINK as usize, 0x1008)]) });
}

#[test]
fn a_delayed_row_alone_lifts_to_unsupported_and_reads_as_its_transfer() {
    let machine = toy();
    let word = branch_word(BD, 1, 0, 0x1000, 0x100c);
    let function = machine.lift(&SPEC, word, u64::from(BASE)).expect("bd decodes");
    assert!(function.blocks.iter().all(|block| block.terminator == Terminator::Unreachable), "nothing runs without the slot");
    let instr = machine.instr(word, u64::from(BASE)).expect("bd decodes");
    assert_eq!(instr.flow, ArchFlow::CondJump(Target::Direct(0x100c)));
    let call = machine.instr(far_word(CD, 0x1000, 0x2000), u64::from(BASE)).expect("cd decodes");
    assert_eq!(call.flow, ArchFlow::Call(Target::Direct(0x2000)));
}
