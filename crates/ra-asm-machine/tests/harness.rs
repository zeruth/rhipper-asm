//! The guest harness of `testdata/vectors/harness` on its first described machine: guest.c with
//! the trampoline of mipsel.S, built by run.sh with clang in WSL and run under
//! `qemu-mipsel-static`. A case is its code at the case page's `CASE_IP` with every register,
//! hi, lo and a window of the data region; its result is the signal that stopped it, the pc it
//! stopped at, the registers, hi, lo and the window after it.
//!
//! The vectors a guest answers are written by `ra_asm_machine::generate`, which `toy_generates`
//! runs over the toy machine of `toy/mod.rs` with a guest that answers from the interpreter.

mod toy;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};
use ra_asm_machine::{Cases, Decoded, Guest, Input, Machine, Output, Vectors, generate};
use ra_asm_testing::vectors::{self, Case, Outcome, State};
use toy::{SCRATCH, TOY_ROWS, TOY_SPEC, toy_machine};

const CASE_IP: u32 = 0x3000_2000;
const DATA_BASE: u32 = 0x4000_0000;
const WINDOW: usize = 64;
const CODE: usize = 16;
/// `sizeof(struct case_in)` and `sizeof(struct case_out)` of guest.c's MIPS build.
const IN_LEN: usize = 224;
const OUT_LEN: usize = 212;
const SIGTRAP: u32 = 5;

const T0: usize = 8;
const T1: usize = 9;
const T2: usize = 10;

/// One case in, as `struct case_in` lays it out.
struct CaseIn {
    r: [u32; 32],
    hi: u32,
    lo: u32,
    code: Vec<u32>,
    window_at: u32,
    window: [u8; WINDOW],
}

/// One case out, as `struct case_out` lays it out; `stray` is the first address outside the
/// window a store reached, plus one.
#[derive(Debug, PartialEq, Eq)]
struct CaseOut {
    sig: u32,
    pc: u32,
    r: [u32; 32],
    hi: u32,
    lo: u32,
    stray: u32,
    window: [u8; WINDOW],
}

fn pack(blob: &mut Vec<u8>, case: &CaseIn) {
    let start = blob.len();
    for word in case.r.iter().chain([&case.hi, &case.lo]) {
        blob.extend_from_slice(&word.to_le_bytes());
    }
    let mut code = [0u8; CODE];
    for (slot, word) in code.chunks_exact_mut(4).zip(&case.code) {
        slot.copy_from_slice(&word.to_le_bytes());
    }
    blob.extend_from_slice(&(4 * case.code.len() as u32).to_le_bytes());
    blob.extend_from_slice(&case.window_at.to_le_bytes());
    blob.extend_from_slice(&code);
    blob.extend_from_slice(&case.window);
    assert_eq!(blob.len() - start, IN_LEN);
}

fn unpack(bytes: &[u8]) -> CaseOut {
    let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
    CaseOut {
        sig: word(0),
        pc: word(1),
        r: std::array::from_fn(|n| word(2 + n)),
        hi: word(34),
        lo: word(35),
        stray: word(36),
        window: bytes[4 * 37..4 * 37 + WINDOW].try_into().unwrap(),
    }
}

/// The path WSL sees for a Windows path: `C:\a\b` is `/mnt/c/a/b`.
fn wsl_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.as_bytes() {
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic() => format!("/mnt/{}{}", drive.to_ascii_lowercase() as char, &text[2..]),
        _ => text,
    }
}

/// Builds the guest for `machine` with run.sh in `scratch` and runs `blob` through it; the
/// results it wrote.
fn run_guest(wsl: &Path, scratch: &Path, machine: &str, blob: &[u8]) -> Vec<u8> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness/run.sh");
    let input = scratch.join("cases.bin");
    let output = scratch.join("results.bin");
    fs::write(&input, blob).unwrap();
    let run = Command::new(wsl)
        .args(["-d", "Ubuntu-24.04", "--", "bash"])
        .arg(wsl_path(&script))
        .arg(machine)
        .args([wsl_path(scratch), wsl_path(&input), wsl_path(&output)])
        .output()
        .expect("wsl runs");
    assert!(
        run.status.success(),
        "run.sh {machine} failed: {}\n{}{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    fs::read(&output).unwrap_or_else(|e| panic!("{}: {e}", output.display()))
}

/// The harness gives the states of `addu`, `lw` and a taken `beq` with its delay slot: each
/// stopped by the `break` after it (the branch's at its target), every register but the ones it
/// writes as it went in, $1 and $4 (which the page's prologue loads), $ra (which the jump into
/// the page loads in its slot) and hi and lo included.
#[test]
#[ignore = "builds testdata/vectors/harness/guest.c for mipsel in WSL and runs it under qemu-mipsel-static"]
fn mipsel_runs_three_cases() {
    let Some(wsl) = ra_asm_testing::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-machine-tests", "harness-mipsel");
    // A value of its own in every register, so a register the trampoline loads or saves wrong
    // shows; $0 reads zero whatever goes in.
    let regs: [u32; 32] = std::array::from_fn(|n| if n == 0 { 0 } else { 0x1000_0000 | ((n as u32) << 16) | n as u32 });
    let (hi, lo) = (0x1357_9bdf, 0x2468_ace0);
    let zeros = [0u8; WINDOW];

    // addu $t0, $t1, $t2
    let mut addu = CaseIn { r: regs, hi, lo, code: vec![0x012a_4021], window_at: DATA_BASE, window: zeros };
    addu.r[T1] = 0x7fff_ffff;
    addu.r[T2] = 1;
    // lw $t0, 4($t1)
    let mut window = zeros;
    window[4..8].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    let mut lw = CaseIn { r: regs, hi, lo, code: vec![0x8d28_0004], window_at: DATA_BASE, window };
    lw.r[T1] = DATA_BASE;
    // beq $t0, $t1, 3 with addiu $t2, $t2, 1 in its slot
    let mut beq = CaseIn { r: regs, hi, lo, code: vec![0x1109_0003, 0x254a_0001], window_at: DATA_BASE, window: zeros };
    beq.r[T0] = 5;
    beq.r[T1] = 5;
    beq.r[T2] = 7;

    let mut blob = Vec::new();
    for case in [&addu, &lw, &beq] {
        pack(&mut blob, case);
    }
    let results = run_guest(&wsl, &scratch, "mipsel", &blob);
    assert_eq!(results.len(), 3 * OUT_LEN, "the harness answered {} bytes", results.len());
    let outs: Vec<CaseOut> = results.chunks_exact(OUT_LEN).map(unpack).collect();

    // addu wraps without the overflow trap of add.
    let mut after = addu.r;
    after[T0] = 0x8000_0000;
    assert_eq!(outs[0], CaseOut { sig: SIGTRAP, pc: CASE_IP + 4, r: after, hi, lo, stray: 0, window: zeros }, "addu $t0, $t1, $t2");

    let mut after = lw.r;
    after[T0] = 0x1122_3344;
    assert_eq!(outs[1], CaseOut { sig: SIGTRAP, pc: CASE_IP + 4, r: after, hi, lo, stray: 0, window }, "lw $t0, 4($t1)");

    // The slot runs before the transfer, and the target is the slot's address plus 12.
    let mut after = beq.r;
    after[T2] = 8;
    assert_eq!(
        outs[2],
        CaseOut { sig: SIGTRAP, pc: CASE_IP + 16, r: after, hi, lo, stray: 0, window: zeros },
        "beq $t0, $t1, 3; addiu $t2, $t2, 1"
    );
    println!("mipsel harness: 3 cases as expected");
}

const TOY_CASES: [Cases; 6] = [
    Cases { row: "add", group: "alu", undefined: &[] },
    Cases { row: "mix", group: "alu", undefined: &["r1"] },
    Cases { row: "addi", group: "alu", undefined: &[] },
    Cases { row: "ld", group: "memory", undefined: &[] },
    Cases { row: "st", group: "memory", undefined: &[] },
    Cases { row: "beq", group: "branch", undefined: &[] },
];

const TOY_AT: u64 = 0x1000;
const TOY_STATE: u64 = 0x10_0000;
const TOY_WINDOW: u64 = 0x2000;
const TOY_WINDOW_LEN: usize = 64;
const TOY_SAMPLES: usize = 8;

/// The runtime of a lifted toy row: every call stops the run, which no toy row makes.
struct Runtime;

impl Host for Runtime {
    fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, _: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("{name} called")))
    }
}

/// The guest of the toy machine: each case's word lifted alone and run by the interpreter over
/// the state and the window, and the groups it was asked for, in order, with their sizes.
struct Interpreter<'m> {
    machine: &'m Machine,
    batches: Vec<(String, usize)>,
}

impl Guest for Interpreter<'_> {
    fn run(&mut self, group: &str, inputs: &[Input]) -> Result<Vec<Output>, String> {
        self.batches.push((group.to_owned(), inputs.len()));
        inputs.iter().map(|input| self.case(input)).collect()
    }
}

impl Interpreter<'_> {
    fn case(&self, input: &Input) -> Result<Output, String> {
        let word = input.code[0];
        let function = self.machine.lift(&TOY_SPEC, word, TOY_AT).ok_or_else(|| format!("no row wins {word:#010x}"))?;
        let name = function.name.clone();
        let mut module = Module::x86_64_windows("toy");
        TOY_SPEC.declare(&mut module);
        module.functions.push(function);
        let mut memory = Memory::new();
        memory.add_region(Region::owned(TOY_STATE, TOY_SPEC.size(), "state"));
        memory.add_region(Region::owned(TOY_WINDOW, TOY_WINDOW_LEN as u64, "window"));
        let mut vm = Vm::new(&module, memory);
        let unmapped = |stop: Stop| format!("{stop:?}");
        for (n, &value) in input.registers.iter().enumerate() {
            vm.memory.write_u32(TOY_STATE + TOY_SPEC.register_offset(n as u32), value as u32).map_err(unmapped)?;
        }
        vm.memory.write_u32(TOY_STATE + TOY_SPEC.pc_offset(), TOY_AT as u32).map_err(unmapped)?;
        vm.memory.write(TOY_WINDOW, &input.window).map_err(unmapped)?;
        let fault = vm.call_named(&mut Runtime, &name, vec![Value::Int(u128::from(TOY_STATE))]).err().map(|_| "trap".to_owned());
        let mut registers = vec![0u64; input.registers.len()];
        for (n, value) in registers.iter_mut().enumerate() {
            *value = u64::from(vm.memory.read_u32(TOY_STATE + TOY_SPEC.register_offset(n as u32)).map_err(unmapped)?);
        }
        let pc = u64::from(vm.memory.read_u32(TOY_STATE + TOY_SPEC.pc_offset()).map_err(unmapped)?);
        let mut window = vec![0u8; input.window.len()];
        vm.memory.read(TOY_WINDOW, &mut window).map_err(unmapped)?;
        Ok(Output { fault, pc, registers, window })
    }
}

/// The register `name` of a side of a case, as a number; `None` when the side does not name it.
fn register(state: &State, name: &str) -> Option<u32> {
    state.registers.get(name).map(|value| value.iter().fold(0, |n, &b| n << 8 | u32::from(b)))
}

/// The byte at `address` of the last of `sides` that has a cell holding it.
fn byte(sides: &[&State], address: u64) -> u8 {
    sides
        .iter()
        .rev()
        .find_map(|side| {
            side.memory
                .range(..=address)
                .next_back()
                .filter(|&(&start, cell)| address < start + cell.len() as u64)
                .map(|(&start, cell)| cell[(address - start) as usize])
        })
        .unwrap_or_else(|| panic!("no cell holds {address:#x}"))
}

/// The little-endian word at `address` of `sides`.
fn word_at(sides: &[&State], address: u64) -> u32 {
    (0..4).rev().fold(0, |n, i| n << 8 | u32::from(byte(sides, address + i)))
}

/// Checks a case read back against what its row means.
fn check(case: &Case, decoded: &Decoded<'_>) {
    let row = decoded.row.name;
    let Outcome::State(out) = &case.outcome else { panic!("a {row} case faulted: {case:?}") };
    let field = |name: &str| decoded.field(name).unwrap_or_else(|| panic!("{row} has no field {name}"));
    let reg = |name: &str| format!("r{}", field(name));
    let input = |name: &str| register(&case.input, name).unwrap_or(0);
    let output = |name: &str| register(out, name).unwrap_or_else(|| input(name));
    // r0 drops what is written to it.
    let written = |name: &str, value: u32| if field(name) == 0 { 0 } else { value };
    let (at, next) = (TOY_AT as u32, TOY_AT as u32 + 4);
    assert_eq!(input("pc"), at, "{row}: {case:?}");
    let address = || u64::from(input(&reg("ra")).wrapping_add(field("disp") as u32));
    match row {
        "add" => assert_eq!(output(&reg("rd")), written("rd", input(&reg("ra")).wrapping_add(input(&reg("rb")))), "{case:?}"),
        "mix" => {
            assert!(!out.registers.contains_key("r1"), "the undefined r1 is out: {case:?}");
            assert_eq!(out.registers.get("undefined").map(Vec::as_slice), Some(&[0, 0, 0, 2][..]), "{case:?}");
            if field("rd") != i64::from(SCRATCH) {
                assert_eq!(output(&reg("rd")), written("rd", input(&reg("ra")).wrapping_add(input(&reg("rb")))), "{case:?}");
            }
        }
        "addi" => assert_eq!(output(&reg("rd")), written("rd", input(&reg("ra")).wrapping_add(field("imm") as u32)), "{case:?}"),
        "ld" => assert_eq!(output(&reg("rd")), written("rd", word_at(&[&case.input], address())), "{case:?}"),
        "st" => assert_eq!(word_at(&[&case.input, out], address()), input(&reg("rd")), "{case:?}"),
        "beq" => {
            let taken = input(&reg("ra")) == input(&reg("rb"));
            let target = next.wrapping_add((field("offset") as u32) << 2);
            assert_eq!(output("pc"), if taken { target } else { next }, "{case:?}");
            return;
        }
        _ => panic!("no toy row {row}"),
    }
    assert_eq!(output("pc"), next, "{row}: {case:?}");
}

/// The toy machine's vectors, its guest the interpreter: one batch a group, one file a group under
/// `<out>/toy`, each read back with every case decoding to a row of its group and its outcome what
/// the row means, the register `mix` leaves undefined masked.
#[test]
#[ignore = "a machine's regenerating test; the toy's writes into its scratch, answered by the interpreter"]
fn toy_generates() {
    let machine = toy_machine();
    let table = Vectors {
        machine: &machine,
        spec: TOY_SPEC,
        rows: &TOY_CASES,
        samples: TOY_SAMPLES,
        at: TOY_AT,
        window: TOY_WINDOW,
        window_len: TOY_WINDOW_LEN,
        slot: 0,
    };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-machine-tests", "generate-toy");
    let mut guest = Interpreter { machine: &machine, batches: Vec::new() };
    let written = generate(&table, &mut guest, 0x5eed_0298, scratch.path()).unwrap_or_else(|e| panic!("{e}"));

    let groups = ["alu", "branch", "memory"];
    assert_eq!(written, groups.map(|group| scratch.join("toy").join(format!("{group}.vec"))));
    let sizes: Vec<(String, usize)> = [("alu", 3), ("branch", 1), ("memory", 2)].map(|(g, rows)| (g.to_owned(), rows * TOY_SAMPLES)).into();
    assert_eq!(guest.batches, sizes, "one batch a group");

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (path, group) in written.iter().zip(groups) {
        let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let cases = vectors::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for case in &cases {
            let word = u32::from_be_bytes(case.bytes[..].try_into().expect("a case is one word"));
            let decoded = machine.table.decode(word).unwrap_or_else(|| panic!("{word:#010x} decodes to no row"));
            let declared = TOY_CASES.iter().find(|cases| cases.row == decoded.row.name).expect("a row of the vectors");
            assert_eq!(declared.group, group, "{} is in {}", decoded.row.name, path.display());
            check(case, &decoded);
            *counts.entry(decoded.row.name).or_default() += 1;
        }
    }
    let expected: BTreeMap<&str, usize> = TOY_ROWS.iter().map(|row| (row.name, TOY_SAMPLES)).collect();
    assert_eq!(counts, expected);
    println!("toy vectors: {} cases in {} files", TOY_ROWS.len() * TOY_SAMPLES, written.len());
}
