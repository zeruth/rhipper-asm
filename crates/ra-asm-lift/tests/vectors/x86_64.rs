//! The x86-64 vectors: the cases of `testdata/vectors/x86_64/*.vec`, the processor's answer to
//! every form of the instruction oracle (`instruction_oracle.rs`, whose `write_vectors` wrote
//! them), replayed through the x86 lift and the IR interpreter on any host, with no processor
//! of the form's own and no tool but cargo.
//!
//! A group lifts as one image: each case is `instruction; ret` in a slot of its own among an
//! `int 3` fill, and the facts of a traced run name every slot a function, so a group is one
//! analysis, one lift and one `Vm`. The arena is host memory committed at `VECTOR_BASE`, the
//! address the files and the oracle's own run gave it: a value the processor computed from a
//! pointer (a sum, a flag, a `moffs` encoding) holds the address's own bits, so no rebasing
//! reproduces it. A difference names the form, the bytes and the first register, flag or chunk
//! that differs. The pending-list rule is the `AArch64` one: exactly the forms of
//! `x86_64/PENDING.txt`, keyed `<group> <iced Code>`, differ.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ra_asm_analysis::Facts;
use ra_asm_analysis::facts::SeenCall;
use ra_asm_ir::Type;
use ra_asm_ir::interp::{Memory, Region, Stop, Value, Vm};
use ra_asm_testing::vectors::{self, Case, Outcome, State};
use ra_asm_x86::iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};

use super::{Ends, FRAMES_AT, Forms, SCRATCH_LEN, STATICS_AT, leading_hex, map_fixed, pending_failures, read_pending};

// region The layout the files and the replay share

/// The arena as the files address it and as the oracle laid it out: the state block at its
/// start, the data buffer from `DATA`, the stack above it to `STACK_END`.
const VECTOR_BASE: u64 = 0x2000_0000;
const ARENA_LEN: u64 = 0x10000;
const DATA: u64 = 0x2000;
const STACK_END: u64 = 0x8000;

/// The synthetic image: the oracle's base, so an encoding replays at the address it was made
/// for, and one slot a case.
const IMAGE_BASE: u64 = 0x7_0000_0000;
const TEXT_RVA: u64 = 0x1000;
const SLOT: usize = 32;

const GPR_NAMES: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
const RSP: usize = 4;
const FLAG_NAMES: [&str; 7] = ["cf", "pf", "af", "zf", "sf", "df", "of"];
/// The rflags bit of each flag, as the `undefined` mask of a case names them.
const FLAG_BITS: [u32; 7] = [0, 2, 4, 6, 7, 10, 11];

/// The arena is mapped once a process; the interpreter's regions are the host's own memory.
static ARENA: OnceLock<bool> = OnceLock::new();

fn ensure_arena() {
    let mapped = *ARENA.get_or_init(|| map_fixed(VECTOR_BASE, ARENA_LEN));
    assert!(mapped, "the replay needs host memory at {VECTOR_BASE:#x}, the files' arena");
}

/// Where the state's fields lie, from the interpreter's layout of `%State`.
struct Fields {
    gprs: usize,
    flags: usize,
    xmm: usize,
    size: usize,
}

// endregion

// region The image

/// A case decoded: its form and text for a message.
struct Form {
    code: String,
    asm: String,
}

fn form_of(case: &Case, at: u64) -> Form {
    let ins = Decoder::with_ip(64, &case.bytes, at, DecoderOptions::NONE).decode();
    let mut asm = String::new();
    IntelFormatter::new().format(&ins, &mut asm);
    Form { code: format!("{:?}", ins.code()), asm }
}

/// A PE32+ with one `.text` section that holds `text` at `TEXT_RVA`, the entry point its start.
fn pe_with_text(text: &[u8]) -> Vec<u8> {
    let len = text.len().next_multiple_of(0x1000);
    let mut f = vec![0u8; 0x400 + len];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let w64 = |f: &mut Vec<u8>, at: usize, v: u64| f[at..at + 8].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x8664);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xF0);
    w16(&mut f, coff + 18, 0x22);
    let opt = coff + 20;
    w16(&mut f, opt, 0x20B);
    w32(&mut f, opt + 4, len as u32);
    w32(&mut f, opt + 16, TEXT_RVA as u32);
    w32(&mut f, opt + 20, TEXT_RVA as u32);
    w64(&mut f, opt + 24, IMAGE_BASE);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, TEXT_RVA as u32 + len as u32);
    w32(&mut f, opt + 60, 0x400);
    w16(&mut f, opt + 68, 3);
    w64(&mut f, opt + 72, 0x10_0000);
    w64(&mut f, opt + 80, 0x1000);
    w64(&mut f, opt + 88, 0x10_0000);
    w64(&mut f, opt + 96, 0x1000);
    w32(&mut f, opt + 108, 16);
    let sec = opt + 0xF0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, len as u32);
    w32(&mut f, sec + 12, TEXT_RVA as u32);
    w32(&mut f, sec + 16, len as u32);
    w32(&mut f, sec + 20, 0x400);
    w32(&mut f, sec + 36, 0x6000_0020);
    f[0x400..].fill(0xCC);
    f[0x400..0x400 + text.len()].copy_from_slice(text);
    f
}

// endregion

// region The replay

/// A register's value in a state, zero when absent; the files write a value's bytes most
/// significant first.
fn number_of(value: &[u8]) -> u128 {
    value.iter().fold(0u128, |acc, &b| (acc << 8) | u128::from(b))
}

fn number(state: &State, name: &str) -> u128 {
    state.registers.get(name).map_or(0, |v| number_of(v))
}

/// Lays the cells over the image of the data and the stack; a cell outside them is a difference.
fn overlay(image: &mut [u8], cells: &BTreeMap<u64, Vec<u8>>) -> Result<(), String> {
    for (&at, bytes) in cells {
        match at.checked_sub(VECTOR_BASE + DATA).map(|o| o as usize) {
            Some(start) if start + bytes.len() <= image.len() => image[start..start + bytes.len()].copy_from_slice(bytes),
            _ => return Err(format!("the cell at {at:x} is outside the data and the stack")),
        }
    }
    Ok(())
}

/// Runs one case over its state: the first difference from the file, if any.
fn run_case(vm: &mut Vm<'_>, name: &str, case: &Case, fields: &Fields) -> Option<String> {
    let input = &case.input;
    // What the files do not name is zero, between the cases too.
    vm.memory.fill(VECTOR_BASE, ARENA_LEN, 0).expect("the arena");
    let mut state = vec![0u8; fields.size];
    for (i, reg) in GPR_NAMES.iter().enumerate() {
        let v = number(input, reg) as u64;
        state[fields.gprs + 8 * i..fields.gprs + 8 * i + 8].copy_from_slice(&v.to_le_bytes());
    }
    for (i, flag) in FLAG_NAMES.iter().enumerate() {
        state[fields.flags + i] = number(input, flag) as u8;
    }
    for i in 0..16 {
        let v = number(input, &format!("xmm{i}"));
        state[fields.xmm + 16 * i..fields.xmm + 16 * i + 16].copy_from_slice(&v.to_le_bytes());
    }
    vm.memory.write(VECTOR_BASE, &state).expect("the state");
    let mut expected = vec![0u8; (STACK_END - DATA) as usize];
    if let Err(e) = overlay(&mut expected, &input.memory) {
        return Some(e);
    }
    for (&at, bytes) in &input.memory {
        vm.memory.write(at, bytes).expect("a cell of the input");
    }

    vm.steps = 0;
    let stopped = match vm.call_named(&mut Ends, name, vec![Value::Int(u128::from(VECTOR_BASE))]) {
        Ok(_) => None,
        Err(Stop::Fault(m)) => Some(m),
        Err(Stop::Limit(n)) => Some(format!("the limit of {n} instructions")),
        Err(Stop::Exit(c)) => Some(format!("exit {c}")),
    };
    let output = match (&case.outcome, stopped) {
        (Outcome::Fault(_), Some(m)) if m.contains("unsupported") => return Some(format!("the lift does not cover the form: {m}")),
        (Outcome::Fault(_), Some(_)) => return None,
        (Outcome::Fault(kind), None) => return Some(format!("the processor faulted ({kind}), the lifted code ran")),
        (Outcome::State(_), Some(m)) => return Some(format!("the processor ran, the lifted code stopped: {m}")),
        (Outcome::State(output), None) => output,
    };

    let state = vm.memory.read_vec(VECTOR_BASE, fields.size as u64).expect("the state");
    let want = |reg: &str| output.registers.get(reg).or_else(|| input.registers.get(reg)).map_or(0, |v| number_of(v));
    for (i, reg) in GPR_NAMES.iter().enumerate() {
        let mut got = u64::from_le_bytes(state[fields.gprs + 8 * i..fields.gprs + 8 * i + 8].try_into().unwrap());
        // The lifted function returned: its `ret` popped eight bytes the processor's run did not.
        if i == RSP {
            got = got.wrapping_sub(8);
        }
        if u128::from(got) != want(reg) {
            return Some(format!("{reg}: expected {:#x}, lifted {got:#x}", want(reg)));
        }
    }
    let undefined = output.registers.get("undefined").map_or(0, |v| number_of(v));
    for (i, flag) in FLAG_NAMES.iter().enumerate() {
        if undefined & (1 << FLAG_BITS[i]) != 0 {
            continue;
        }
        let got = state[fields.flags + i];
        if u128::from(got) != want(flag) {
            return Some(format!("{flag}: expected {}, lifted {got}", want(flag)));
        }
    }
    for i in 0..16 {
        let reg = format!("xmm{i}");
        let got = u128::from_le_bytes(state[fields.xmm + 16 * i..fields.xmm + 16 * i + 16].try_into().unwrap());
        if got != want(&reg) {
            return Some(format!("{reg}: expected {:#034x}, lifted {got:#034x}", want(&reg)));
        }
    }

    if let Err(e) = overlay(&mut expected, &output.memory) {
        return Some(e);
    }
    let got = vm.memory.read_vec(VECTOR_BASE + DATA, STACK_END - DATA).expect("the data and the stack");
    if let Some(i) = expected.iter().zip(&got).position(|(a, b)| a != b) {
        let chunk = i & !15;
        return Some(format!(
            "[{:x}]: expected {}, lifted {}",
            VECTOR_BASE + DATA + chunk as u64,
            vectors::hex(&expected[chunk..chunk + 16]),
            vectors::hex(&got[chunk..chunk + 16])
        ));
    }
    None
}

/// A case replayed: its form's key in `PENDING.txt` and its first difference, if any.
struct Replayed {
    key: String,
    found: Option<String>,
}

/// The slot a lift warning names by the address it starts with, so the warning is a finding of
/// that case's form and not of the whole group.
fn slot_of(warning: &str) -> Option<usize> {
    let at = leading_hex(warning)?;
    let offset = at.checked_sub(IMAGE_BASE + TEXT_RVA)?;
    usize::try_from(offset / SLOT as u64).ok()
}

/// Lifts a group as one image and runs every case of it: each case with what differed, and the
/// lift's warnings that name no slot, which are failures of the group.
fn replay(group: &str, cases: &[Case], scratch: u64) -> (Vec<Replayed>, Vec<String>) {
    let mut text = vec![0xCCu8; cases.len() * SLOT];
    let mut forms = Vec::with_capacity(cases.len());
    let mut slots = Vec::with_capacity(cases.len());
    for (i, case) in cases.iter().enumerate() {
        let at = IMAGE_BASE + TEXT_RVA + (i * SLOT) as u64;
        let bytes = &case.bytes;
        assert!(bytes.len() < SLOT, "{group} case {i}: {} bytes do not fit a slot", bytes.len());
        let start = i * SLOT;
        text[start..start + bytes.len()].copy_from_slice(bytes);
        text[start + bytes.len()] = 0xC3;
        forms.push(form_of(case, at));
        slots.push(at);
    }
    let image = ra_asm_pe::Image::parse(pe_with_text(&text)).expect("the synthetic image parses");
    // Nothing in the image names a slot but the first: the facts of a traced run name the rest.
    let facts = Facts { calls: vec![SeenCall { site: IMAGE_BASE, targets: slots.clone() }], ..Facts::default() };
    let analysis = ra_asm_analysis::analyze_with(&image, &facts);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let mut warned: BTreeMap<usize, String> = BTreeMap::new();
    let mut failures = Vec::new();
    for w in &lifted.warnings {
        match slot_of(w) {
            Some(slot) if slot < cases.len() => {
                warned.entry(slot).or_insert_with(|| format!("the lift warned: {w}"));
            }
            _ => failures.push(format!("{group}: the lift warned: {w}")),
        }
    }

    let mut memory = Memory::new();
    memory.add_region(Region::new(VECTOR_BASE, ARENA_LEN, "arena"));
    memory.add_region(Region::new(scratch, SCRATCH_LEN as u64, "scratch"));
    let mut vm = Vm::new(&lifted.module, memory);
    vm.set_static_region(scratch + STATICS_AT, FRAMES_AT - STATICS_AT);
    vm.set_frame_region(scratch + FRAMES_AT, SCRATCH_LEN as u64 - FRAMES_AT);
    vm.limit = 1_000_000;
    vm.place().expect("the module places");
    let state_ty = Type::Named(ra_asm_lift::state::STATE_TYPE.to_owned());
    let (offsets, size) = {
        let layout = vm.layout();
        (layout.offsets(&state_ty), layout.size(&state_ty))
    };
    assert_eq!(offsets.len(), 4, "the state has four fields");
    let fields = Fields { gprs: offsets[0] as usize, flags: offsets[2] as usize, xmm: offsets[3] as usize, size: size as usize };
    assert!(size <= DATA, "the state ({size} bytes) runs into the data buffer");

    let mut replayed = Vec::with_capacity(cases.len());
    for (i, (case, form)) in cases.iter().zip(&forms).enumerate() {
        let name = ra_asm_lift::function_name(slots[i]);
        // A run that differs says more than the warning that foretold it.
        let found = run_case(&mut vm, &name, case, &fields).or_else(|| warned.remove(&i));
        replayed.push(Replayed {
            key: format!("{group} {}", form.code),
            found: found.map(|found| format!("case {i} `{}` ({}): {found}", form.asm, vectors::hex(&case.bytes))),
        });
    }
    (replayed, failures)
}

/// Every x86-64 vector replays through the lift and the interpreter, and exactly the forms of
/// `x86_64/PENDING.txt` differ.
#[test]
fn x86_64() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/x86_64");
    let pending = read_pending(&dir.join("PENDING.txt"));
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vec"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .vec files under {}", dir.display());

    // The arena at the files' address; the interpreter's scratch is a buffer of the test's.
    ensure_arena();
    let scratch_buffer = vec![0u128; SCRATCH_LEN / 16];
    let scratch = scratch_buffer.as_ptr() as usize as u64;

    let mut total = 0;
    let mut forms = Forms::new();
    let mut order: Vec<String> = Vec::new();
    let mut failures = Vec::new();
    for file in &files {
        let group = file.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let text = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let cases = vectors::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        total += cases.len();
        let (replayed, unplaced) = replay(&group, &cases, scratch);
        failures.extend(unplaced);
        for case in replayed {
            let entry = forms.entry(case.key.clone()).or_insert_with(|| {
                order.push(case.key);
                (0, Vec::new())
            });
            entry.0 += 1;
            entry.1.extend(case.found);
        }
    }
    drop(scratch_buffer);

    failures.extend(pending_failures(&order, &forms, &pending));
    let differing = forms.values().filter(|(_, found)| !found.is_empty()).count();
    println!(
        "x86_64 vectors: {total} cases replayed over {} forms in {} groups; {differing} forms differ, {} pending",
        forms.len(),
        files.len(),
        pending.len()
    );
    assert!(total >= 8000, "{total} cases replayed, at least 8000 expected");
    assert!(failures.is_empty(), "{} forms are not as PENDING.txt says:\n{}", failures.len(), failures.join("\n"));
}

// endregion
