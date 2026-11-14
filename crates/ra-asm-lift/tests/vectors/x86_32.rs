//! The 32-bit x86 vectors: the cases of `testdata/vectors/x86/*.vec`, the processor's answer
//! under WOW64 to every integer form of iced's 32-bit table the lift has an arm for (written by
//! the harness through `common/x86_32.rs`), replayed through the x86 lift and the IR interpreter
//! as the x86-64 ones are: a group as one PE32 image of `instruction; ret` slots, the arena as
//! host memory at the files' address, and exactly the forms of `x86/PENDING.txt`, keyed
//! `<group> <iced Code>`, differing. A register is compared at its 32 bits: the port's machine
//! has no upper half.
//!
//! The files are the processor's, and only this machine writes them: when the directory holds no
//! `.vec` file the test writes them first with clang and the harness, then writes `PENDING.txt`
//! from the forms that differ on that first replay. A committed set replays with no tool but
//! cargo.
//!
//! The x87 vectors (`x87.rs`) replay through the same image, run and files, with a [`Unit`] that
//! sets the x87 globals before a case and checks them after; the integer replay's unit is `Ends`
//! itself, which keeps nothing.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ra_asm_analysis::Facts;
use ra_asm_analysis::facts::SeenCall;
use ra_asm_ir::Type;
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_lift::X87Unit;
use ra_asm_testing::vectors::{self, Case, Outcome, State};
use ra_asm_x86::iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};

use super::common::x86_32::{self as x86, FLAG_BITS, FLAG_NAMES, GPR_NAMES32};
use super::x86_64::{ARENA_LEN, DATA, Fields, Replayed, STACK_END, VECTOR_BASE, arena_turn, ensure_arena, number, number_of, overlay};
use super::{Ends, FRAMES_AT, Forms, SCRATCH_LEN, STATICS_AT, leading_hex, pending_failures, read_pending};

/// What a replay keeps beside the integer machine and answers the lifted code's calls with:
/// nothing for the integer vectors, the x87 globals and the runtime's software `f80` for the x87
/// ones.
pub(super) trait Unit: Host {
    /// The x87 unit the lift models.
    fn x87(&self) -> X87Unit;

    /// The module is placed: where the unit's globals lie.
    fn placed(&mut self, _vm: &Vm<'_>) {}

    /// Sets the unit's state from the case's input, before the run.
    fn load(&mut self, _vm: &mut Vm<'_>, _input: &State) {}

    /// The first difference of the unit's state from the case's output, after the run.
    fn check(&mut self, _vm: &mut Vm<'_>, _input: &State, _output: &State) -> Option<String> {
        None
    }
}

impl Unit for Ends {
    fn x87(&self) -> X87Unit {
        X87Unit::Double
    }
}

// region The image

/// The synthetic image: the harness's code map as its base, so the slots lie where the cases ran,
/// and one slot a case.
const IMAGE_BASE: u64 = 0x3000_0000;
const TEXT_RVA: u64 = 0x1000;
const SLOT: usize = 32;

/// A case decoded: its form and text for a message.
struct Form {
    code: String,
    asm: String,
}

fn form_of(case: &Case, at: u64) -> Form {
    let ins = Decoder::with_ip(32, &case.bytes, at, DecoderOptions::NONE).decode();
    let mut asm = String::new();
    IntelFormatter::new().format(&ins, &mut asm);
    Form { code: format!("{:?}", ins.code()), asm }
}

/// A PE32 for i386 with one `.text` section that holds `text` at `TEXT_RVA`, the entry point
/// its start.
fn pe_with_text(text: &[u8]) -> Vec<u8> {
    let len = text.len().next_multiple_of(0x1000);
    let mut f = vec![0u8; 0x400 + len];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x014C);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xE0);
    w16(&mut f, coff + 18, 0x0102);
    let opt = coff + 20;
    w16(&mut f, opt, 0x10B);
    w32(&mut f, opt + 4, len as u32);
    w32(&mut f, opt + 16, TEXT_RVA as u32);
    w32(&mut f, opt + 20, TEXT_RVA as u32);
    w32(&mut f, opt + 28, IMAGE_BASE as u32);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, TEXT_RVA as u32 + len as u32);
    w32(&mut f, opt + 60, 0x400);
    w16(&mut f, opt + 68, 3);
    w32(&mut f, opt + 72, 0x10_0000);
    w32(&mut f, opt + 76, 0x1000);
    w32(&mut f, opt + 80, 0x10_0000);
    w32(&mut f, opt + 84, 0x1000);
    w32(&mut f, opt + 92, 16);
    let sec = opt + 0xE0;
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

/// Runs one case over its state: the first difference from the file, if any.
fn run_case(vm: &mut Vm<'_>, name: &str, case: &Case, fields: &Fields, unit: &mut dyn Unit) -> Option<String> {
    let input = &case.input;
    // What the files do not name is zero, between the cases too.
    vm.memory.fill(VECTOR_BASE, ARENA_LEN, 0).expect("the arena");
    let mut state = vec![0u8; fields.size];
    for (i, reg) in GPR_NAMES32.iter().enumerate() {
        let v = number(input, reg) as u64;
        state[fields.gprs + 8 * i..fields.gprs + 8 * i + 8].copy_from_slice(&v.to_le_bytes());
    }
    for (i, flag) in FLAG_NAMES.iter().enumerate() {
        state[fields.flags + i] = number(input, flag) as u8;
    }
    vm.memory.write(VECTOR_BASE, &state).expect("the state");
    let mut expected = vec![0u8; (STACK_END - DATA) as usize];
    if let Err(e) = overlay(&mut expected, &input.memory) {
        return Some(e);
    }
    for (&at, bytes) in &input.memory {
        vm.memory.write(at, bytes).expect("a cell of the input");
    }
    unit.load(vm, input);

    vm.steps = 0;
    let stopped = match vm.call_named(&mut *unit, name, vec![Value::Int(u128::from(VECTOR_BASE))]) {
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
    for (i, reg) in GPR_NAMES32.iter().enumerate() {
        let mut got = u64::from_le_bytes(state[fields.gprs + 8 * i..fields.gprs + 8 * i + 8].try_into().unwrap()) as u32;
        // The lifted function returned: its `ret` popped four bytes the processor's run did not.
        if i == x86::ESP {
            got = got.wrapping_sub(4);
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
    // The files name no vector register: an integer form leaves them zero.
    for i in 0..16 {
        let got = u128::from_le_bytes(state[fields.xmm + 16 * i..fields.xmm + 16 * i + 16].try_into().unwrap());
        if got != 0 {
            return Some(format!("xmm{i}: expected 0, lifted {got:#034x}"));
        }
    }
    if let Some(found) = unit.check(vm, input, output) {
        return Some(found);
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

/// The slot a lift warning names by the address it starts with, so the warning is a finding of
/// that case's form and not of the whole group.
fn slot_of(warning: &str) -> Option<usize> {
    let at = leading_hex(warning)?;
    let offset = at.checked_sub(IMAGE_BASE + TEXT_RVA)?;
    usize::try_from(offset / SLOT as u64).ok()
}

/// Lifts a group as one image and runs every case of it over `unit`: each case with what
/// differed, and the lift's warnings that name no slot, which are failures of the group.
fn replay(group: &str, cases: &[Case], scratch: u64, unit: &mut dyn Unit) -> (Vec<Replayed>, Vec<String>) {
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
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: unit.x87() };
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
    unit.placed(&vm);
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
        let found = run_case(&mut vm, &name, case, &fields, unit).or_else(|| warned.remove(&i));
        replayed.push(Replayed {
            key: format!("{group} {}", form.code),
            found: found.map(|found| format!("case {i} `{}` ({}): {found}", form.asm, vectors::hex(&case.bytes))),
        });
    }
    (replayed, failures)
}

// endregion

// region The files

fn vec_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vec"))
        .collect();
    files.sort();
    files
}

/// The head of a `PENDING.txt`: what the file lists and how its test reads it, for the `label`
/// test of `file`.
pub(super) fn pending_header(label: &str, file: &str, what: &str) -> String {
    format!(
        "# The {what} whose vectors differ from the lift today, one a line: the group (the .vec\n\
         # file's name) and the form (the iced `Code` of the case's bytes, as the replay's message names\n\
         # it), then after `:` the first difference the replay saw. The {label} test of\n\
         # crates/ra-asm-lift/tests/vectors/{file} passes when exactly these forms differ: an unlisted\n\
         # form that differs fails it with a line to paste here, and a listed form that passes fails it\n\
         # with \"remove from PENDING.txt\", so the card that fixes an arm removes its line.\n"
    )
}

/// Writes `PENDING.txt` from the first replay of freshly written files: every form that differs,
/// with its first difference; the list it wrote, as `read_pending` would read it.
fn write_pending(path: &Path, header: &str, order: &[String], forms: &Forms) -> BTreeMap<String, String> {
    let mut text = header.to_owned();
    let mut pending = BTreeMap::new();
    for key in order {
        if let Some(first) = forms[key].1.first() {
            let _ = writeln!(text, "{key}: {first}");
            pending.insert(key.clone(), first.clone());
        }
    }
    fs::write(path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    pending
}

/// The generator of a directory's files: the cases it wrote, by group, with `clang` for the
/// harness.
type Writer = fn(&Path, &Path) -> BTreeMap<&'static str, usize>;

/// What a replay of a directory found.
pub(super) struct Replay {
    pub(super) total: usize,
    pub(super) forms: Forms,
    /// What is not as `PENDING.txt` says, with the lift's warnings that named no case.
    pub(super) failures: Vec<String>,
}

/// Replays every `.vec` file of `dir` through the lift over `unit` under the pending-list rule,
/// and prints the count for the `label` test. When the directory holds no `.vec` file, `write`
/// writes them first with clang and the harness, and `PENDING.txt` is written from the forms that
/// differ on that first replay (unless most do, which is a broken replay and not a lift to list).
pub(super) fn replay_dir(label: &str, dir: &Path, unit: &mut dyn Unit, write: Writer, header: &str) -> Replay {
    let mut files = vec_files(dir);
    let bootstrap = files.is_empty();
    if bootstrap {
        let clang = ra_asm_testing::tools::tool("clang")
            .unwrap_or_else(|| panic!("no .vec files under {} and no clang on the PATH to write them with the harness", dir.display()));
        for (group, n) in write(&clang, dir) {
            println!("{label}: {group}.vec: {n} cases written by the harness");
        }
        files = vec_files(dir);
    }
    assert!(!files.is_empty(), "no .vec files under {}", dir.display());

    // The arena at the files' address, shared with the x86-64 replay; the interpreter's scratch
    // is a buffer of the test's.
    let _turn = arena_turn();
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
        let (replayed, unplaced) = replay(&group, &cases, scratch, unit);
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

    let differing = forms.values().filter(|(_, found)| !found.is_empty()).count();
    let pending_path = dir.join("PENDING.txt");
    let pending = if bootstrap {
        assert!(differing * 2 < forms.len(), "{differing} of {} forms differ on the first replay; PENDING.txt is not written", forms.len());
        write_pending(&pending_path, header, &order, &forms)
    } else {
        read_pending(&pending_path)
    };
    failures.extend(pending_failures(&order, &forms, &pending));
    println!(
        "{label} vectors: {total} cases replayed over {} forms in {} groups; {differing} forms differ, {} pending{}",
        forms.len(),
        files.len(),
        pending.len(),
        if bootstrap { " (written by this run)" } else { "" }
    );
    Replay { total, forms, failures }
}

/// Whether a form of the name (the iced `Code`) is among the forms replayed.
pub(super) fn has_form(forms: &Forms, form: &str) -> bool {
    forms.keys().any(|key| key.rsplit(' ').next() == Some(form))
}

// endregion

/// Every 32-bit x86 vector replays through the lift and the interpreter, and exactly the forms
/// of `x86/PENDING.txt` differ; the files are written first when there are none.
#[test]
fn x86_32() {
    let header = pending_header("x86_32", "x86_32.rs", "32-bit x86 forms");
    let Replay { total, forms, failures } = replay_dir("x86_32", &x86::x86_dir(), &mut Ends, x86::write_vectors, &header);
    assert!(total >= 6000, "{total} cases replayed, at least 6000 expected");
    // The forms the 64-bit oracle cannot run and the one-byte inc and dec are the point of the
    // 32-bit files.
    for form in ["Pushad", "Popad", "Pushfd", "Popfd", "Inc_r32", "Dec_r32"] {
        assert!(has_form(&forms, form), "no case of {form} among the vectors");
    }
    assert!(failures.is_empty(), "{} forms are not as PENDING.txt says:\n{}", failures.len(), failures.join("\n"));
}
