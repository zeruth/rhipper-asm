//! The 32-bit ARM vectors: the Thumb-2 cases of `testdata/vectors/arm32` replayed through the
//! lift of `thumb.rs` and the IR interpreter, and the ARM cases read and counted, recorded for
//! the A32 lift to come. A vector is a case (an instruction, or an IT block with the instructions
//! it predicates) with the registers r0..r14, the flags and a window of data bytes in, and what
//! the processor left out under `qemu-arm-static`.
//!
//! The replay lifts each case from a synthetic ARMNT image whose page is the harness's (a `bkpt`
//! fill, the prologue, the case, `bkpt` after it), runs the lifted function over the state and
//! compares the registers, the flags, the window and where the case stopped. A register branch
//! and a popped return go to `JUMP_PIN`, a `bkpt` of the fill: the lift's `ret` for `bx lr` and
//! `pop {.., pc}` is checked against the address the machine landed on. The pending-list rule is
//! the `AArch64` one: exactly the forms of `arm32/PENDING.txt` differ.
//!
//! `write_arm32_vectors` (ignored: it needs `wsl`) regenerates the files with the guest built for
//! ARM and run under `qemu-arm-static`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ra_asm_ir::Type;
use ra_asm_ir::interp::{Memory, Region, Value, Vm};

use super::common::arm32::{self, arm32_dir, is_arm_group};
use super::common::{read_templates, wsl_path};
use super::{
    BASE_PIN, CODE_BASE, CODE_LEN, CODE_MAP_LEN, DATA_BASE, DATA_LEN, End, Ends, FRAMES_AT, IMAGE_BASE, INDEX_PIN, IP, LiftedCase, Rng,
    SCRATCH_LEN, STATES, STATICS_AT, WINDOW, end_of, ensure_fixed, group_seed, parse_int, parse_u64, read_pending, replay_turn,
    signal_name, tokens, wsl_args,
};

// region The layout the guest and the replay share

/// Where a register branch and a popped return land: a `bkpt` of the fill, taken with the Thumb
/// bit set by a Thumb case and clear by an ARM one.
const JUMP_PIN: u64 = IP + 0x400;
const BKPT_THUMB: u16 = 0xbe00;
/// `ldr.w r12, [r0, #48]; ldr.w r0, [r0]`: the Thumb prologue the trampoline enters, see arm.S.
const PROLOGUE_THUMB: [u8; 8] = [0xd0, 0xf8, 0x30, 0xc0, 0xd0, 0xf8, 0x00, 0x00];

/// One record of the harness's input and of its output, see `struct case_in` in guest.c.
const RECORD_IN: usize = 160;
/// The longest case: an IT block with four 16-bit instructions is ten bytes.
const CODE_MAX: usize = 16;
const RECORD_OUT: usize = 144;

const SP: u8 = 13;
const LR: u8 = 14;
const PC: u8 = 15;

/// The Thumb case page: a `bkpt` fill, the prologue before the case, the case's bytes at `IP`.
fn code_page(bytes: &[u8]) -> Vec<u8> {
    let mut page: Vec<u8> = BKPT_THUMB.to_le_bytes().repeat(CODE_LEN as usize / 2);
    let at = (IP - CODE_BASE) as usize;
    page[at - 8..at].copy_from_slice(&PROLOGUE_THUMB);
    page[at..at + bytes.len()].copy_from_slice(bytes);
    page
}

// endregion

// region Pictures and vectors

/// A machine state: r0..r15 (r15 unused in, the pc out), the flags as four bits (N the highest)
/// and the window of data bytes with its address. `named` are the registers the case sets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Picture {
    r: [u32; 16],
    nzcv: u8,
    window_at: u64,
    window: [u8; WINDOW],
    named: Vec<u8>,
}

impl Picture {
    fn zero() -> Self {
        Self { r: [0; 16], nzcv: 0, window_at: BASE_PIN, window: [0; WINDOW], named: Vec::new() }
    }

    fn flags(&self) -> String {
        format!("{:04b}", self.nzcv)
    }
}

fn register_name(n: u8) -> String {
    match n {
        SP => "sp".to_owned(),
        LR => "lr".to_owned(),
        PC => "pc".to_owned(),
        n => format!("r{n}"),
    }
}

/// The number of a core register token: `r0`..`r15`, `sp`, `lr` and `pc`.
fn general_register(token: &str) -> Option<u8> {
    match token {
        "sp" => Some(SP),
        "lr" => Some(LR),
        "pc" => Some(PC),
        _ => {
            let digits = token.strip_prefix('r')?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse().ok().filter(|n| *n <= PC)
        }
    }
}

/// One vector: the case, the template it came from, the state in and the state the machine left,
/// with where it stopped and the first address outside the window it stored to, if any.
#[derive(Debug, Clone)]
struct Vector {
    opcode: String,
    bytes: Vec<u8>,
    asm: String,
    form: String,
    input: Picture,
    pc: u64,
    sig: String,
    after: Picture,
    stray: Option<u64>,
}

#[derive(Default)]
struct Ending {
    pc: Option<u64>,
    sig: Option<String>,
    stray: Option<u64>,
}

/// Applies the `key=value` fields of an `in` or `out` line to a picture.
fn apply_fields(picture: &mut Picture, fields: &str, where_: &str) -> Ending {
    let mut ending = Ending::default();
    for field in fields.split_whitespace() {
        let (key, value) = field.split_once('=').unwrap_or_else(|| panic!("{where_}: `{field}` is not key=value"));
        match key {
            "nzcv" => {
                assert!(value.len() == 4 && value.bytes().all(|b| b == b'0' || b == b'1'), "{where_}: nzcv={value}");
                picture.nzcv = u8::from_str_radix(value, 2).unwrap();
            }
            "pc" => ending.pc = Some(parse_u64(value, where_)),
            "sig" => ending.sig = Some(value.to_owned()),
            "stray" => ending.stray = Some(parse_u64(value, where_)),
            "mem" => {
                let (at, bytes) = value.split_once(':').unwrap_or_else(|| panic!("{where_}: mem={value}"));
                picture.window_at = parse_u64(at, where_);
                assert_eq!(bytes.len(), 2 * WINDOW, "{where_}: a window of {WINDOW} bytes");
                picture.window.copy_from_slice(&arm32::parse_hex(bytes, where_));
            }
            _ => {
                let n = general_register(key).unwrap_or_else(|| panic!("{where_}: unknown field {key}"));
                let value = parse_u64(value, where_);
                assert!(value <= u64::from(u32::MAX), "{where_}: {key}={value:#x} is wider than a register");
                picture.r[usize::from(n)] = value as u32;
                picture.named.push(n);
            }
        }
    }
    ending
}

fn read_vectors(path: &Path) -> Vec<Vector> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut lines = text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'));
    let mut out = Vec::new();
    while let Some((number, line)) = lines.next() {
        let where_ = format!("{name}:{}", number + 1);
        let (head, form) = line.split_once(" <- ").unwrap_or_else(|| panic!("{where_}: not `case ... <- template`"));
        let mut words = head.splitn(4, ' ');
        assert_eq!(words.next(), Some("case"), "{where_}: not a case line");
        let (Some(opcode), Some(code), Some(asm)) = (words.next(), words.next(), words.next()) else {
            panic!("{where_}: not `case kind bytes asm`")
        };
        let bytes = arm32::parse_hex(code, &where_);
        let (number, input_line) = lines.next().unwrap_or_else(|| panic!("{where_}: no `in` line"));
        let input_fields = input_line.strip_prefix("in ").unwrap_or_else(|| panic!("{name}:{}: not an `in` line", number + 1));
        let mut input = Picture::zero();
        apply_fields(&mut input, input_fields, &format!("{name}:{}", number + 1));
        let (number, out_line) = lines.next().unwrap_or_else(|| panic!("{where_}: no `out` line"));
        let out_fields = out_line.strip_prefix("out ").unwrap_or_else(|| panic!("{name}:{}: not an `out` line", number + 1));
        let mut after = input.clone();
        let ending = apply_fields(&mut after, out_fields, &format!("{name}:{}", number + 1));
        out.push(Vector {
            opcode: opcode.to_owned(),
            bytes,
            asm: asm.to_owned(),
            form: form.to_owned(),
            input,
            pc: ending.pc.unwrap_or_else(|| panic!("{where_}: the `out` line has no pc")),
            sig: ending.sig.unwrap_or_default(),
            after,
            stray: ending.stray,
        });
    }
    out
}

// endregion

// region The assembly of a case

/// The instruction a case ends with: the one an IT block predicates last, or the case itself.
fn instruction(asm: &str) -> &str {
    asm.rsplit("; ").next().unwrap_or(asm)
}

fn mnemonic(asm: &str) -> &str {
    tokens(instruction(asm)).first().copied().unwrap_or("")
}

/// The registers of the `{...}` list of a load or store multiple.
fn register_list(asm: &str) -> Vec<u8> {
    let Some(open) = asm.find('{') else { return Vec::new() };
    let Some(close) = asm[open..].find('}') else { return Vec::new() };
    tokens(&asm[open + 1..open + close]).iter().filter_map(|t| general_register(t)).collect()
}

/// The bracketed operand of a load or store: its base, its index and the address it names with
/// the base at `BASE_PIN` and the index at `INDEX_PIN`; `None` for a literal (a base of `pc`).
struct MemoryOperand {
    base: u8,
    index: Option<u8>,
    address: u64,
}

fn memory_operand(asm: &str) -> Option<MemoryOperand> {
    let open = asm.find('[')?;
    let close = asm[open..].find(']')? + open;
    let mut parts = asm[open + 1..close].split(", ");
    let base = general_register(parts.next()?)?;
    if base == PC {
        return None;
    }
    let mut address = BASE_PIN;
    let mut index = None;
    if let Some(second) = parts.next() {
        if let Some(imm) = second.strip_prefix('#') {
            address = address.wrapping_add(parse_int(imm) as u64);
        } else {
            let (negative, name) = second.strip_prefix('-').map_or((false, second), |rest| (true, rest));
            index = general_register(name);
            let amount = parts.next().and_then(|ext| ext.split_once('#')).map_or(0, |(_, a)| a.parse::<u32>().expect("a shift amount"));
            let by = INDEX_PIN << amount;
            address = if negative { address - by } else { address + by };
        }
    }
    Some(MemoryOperand { base, index, address })
}

// endregion

// region The lifted run

/// A PE32 for ARMNT with one `.text` section, the case page, at RVA 0x1000; the entry is `IP`.
fn pe_with_page(page: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 0x400 + page.len()];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x01C4);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xE0);
    w16(&mut f, coff + 18, 0x0102);
    let opt = coff + 20;
    w16(&mut f, opt, 0x10B);
    w32(&mut f, opt + 4, CODE_LEN as u32);
    w32(&mut f, opt + 16, (IP - IMAGE_BASE) as u32);
    w32(&mut f, opt + 20, 0x1000);
    w32(&mut f, opt + 28, IMAGE_BASE as u32);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, 0x1000 + CODE_LEN as u32);
    w32(&mut f, opt + 60, 0x400);
    w16(&mut f, opt + 68, 3);
    w32(&mut f, opt + 72, 0x10_0000);
    w32(&mut f, opt + 76, 0x1000);
    w32(&mut f, opt + 80, 0x10_0000);
    w32(&mut f, opt + 84, 0x1000);
    w32(&mut f, opt + 92, 16);
    let sec = opt + 0xE0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, CODE_LEN as u32);
    w32(&mut f, sec + 12, 0x1000);
    w32(&mut f, sec + 16, CODE_LEN as u32);
    w32(&mut f, sec + 20, 0x400);
    w32(&mut f, sec + 36, 0x6000_0020);
    f[0x400..].copy_from_slice(page);
    f
}

fn lift_case(bytes: &[u8]) -> LiftedCase {
    let image = ra_asm_pe::Image::parse(pe_with_page(&code_page(bytes))).expect("the synthetic image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    LiftedCase { lifted, name: ra_asm_lift::function_name(IP) }
}

/// What a lifted run left: its end, the register slots as the state holds them (64 bits each,
/// so a value the lift failed to zero-extend shows), the flags, the window and the stores.
struct Run {
    end: End,
    state: [u64; 16],
    nzcv: u8,
    window: [u8; WINDOW],
    writes: Vec<(u64, Vec<u8>)>,
}

fn run_lifted(case: &LiftedCase, input: &Picture, bytes: &[u8], scratch: &mut [u8]) -> Run {
    ensure_fixed();
    let scratch_at = scratch.as_ptr() as usize as u64;
    let mut memory = Memory::new();
    memory.add_region(Region::new(IMAGE_BASE, CODE_MAP_LEN, "code"));
    memory.add_region(Region::new(DATA_BASE, DATA_LEN, "data"));
    memory.add_region(Region::new(scratch_at, scratch.len() as u64, "scratch"));
    memory.write(CODE_BASE, &code_page(bytes)).expect("the code page");
    let mut vm = Vm::new(&case.lifted.module, memory);
    vm.set_static_region(scratch_at + STATICS_AT, FRAMES_AT - STATICS_AT);
    vm.set_frame_region(scratch_at + FRAMES_AT, SCRATCH_LEN as u64 - FRAMES_AT);
    vm.limit = 100_000;
    vm.place().expect("the module places");

    let state_ty = Type::Named(ra_asm_lift::state::STATE_TYPE.to_owned());
    let (offsets, size) = {
        let layout = vm.layout();
        (layout.offsets(&state_ty), layout.size(&state_ty))
    };
    assert_eq!(offsets.len(), 4, "the state has four fields");
    let (gprs_at, flags_at) = (offsets[0] as usize, offsets[2] as usize);
    let mut state = vec![0u8; size as usize];
    for (i, r) in input.r.iter().enumerate() {
        state[gprs_at + 8 * i..gprs_at + 8 * i + 8].copy_from_slice(&u64::from(*r).to_le_bytes());
    }
    for (i, bit) in [3u8, 2, 1, 0].into_iter().enumerate() {
        state[flags_at + i] = (input.nzcv >> bit) & 1;
    }
    vm.memory.write(scratch_at, &state).expect("the state");
    vm.memory.write(input.window_at, &input.window).expect("the window");
    vm.memory.arm();
    let result = vm.call_named(&mut Ends, &case.name, vec![Value::Int(u128::from(scratch_at))]);
    let site = vm.site;
    let access = vm.memory.disarm();

    let state = vm.memory.read_vec(scratch_at, size).expect("the state");
    let mut slots = [0u64; 16];
    for (i, slot) in slots.iter_mut().enumerate() {
        *slot = u64::from_le_bytes(state[gprs_at + 8 * i..gprs_at + 8 * i + 8].try_into().unwrap());
    }
    let nzcv = (0..4).fold(0, |acc, i| (acc << 1) | (state[flags_at + i] & 1));
    let window: [u8; WINDOW] = vm.memory.read_vec(input.window_at, WINDOW as u64).expect("the window").try_into().unwrap();
    // The data region stays zero for the next run.
    vm.memory.fill(input.window_at, WINDOW as u64, 0).unwrap();
    for (at, bytes) in &access.writes {
        if (DATA_BASE..DATA_BASE + DATA_LEN).contains(at) {
            vm.memory.fill(*at, bytes.len() as u64, 0).unwrap();
        }
    }
    Run { end: end_of(result, site), state: slots, nzcv, window, writes: access.writes }
}

/// Where a case the lift returned from went on the machine: `bx lr` to lr, a `pop` or `ldm` with
/// `pc` to the word it loaded, read from the window the case was given; `None` when the replay
/// cannot tell.
fn return_target(vector: &Vector, run: &Run) -> Option<u64> {
    let instr = instruction(&vector.asm);
    let name = mnemonic(&vector.asm);
    if name.starts_with("bx") {
        return Some((run.state[usize::from(LR)] & 0xffff_ffff) & !1);
    }
    let list = register_list(instr);
    if !list.contains(&PC) {
        return None;
    }
    let base = if name.starts_with("pop") { SP } else { tokens(instr).iter().find_map(|t| general_register(t))? };
    let base = u64::from(vector.input.r[usize::from(base)]);
    let slot = if name.starts_with("ldmdb") { base - 4 } else { base + 4 * (list.len() as u64 - 1) };
    let at = slot.checked_sub(vector.input.window_at)? as usize;
    let word = vector.input.window.get(at..at + 4)?;
    Some(u64::from(u32::from_le_bytes(word.try_into().unwrap())) & !1)
}

/// Every way the lifted run differs from the machine's record, the stop first.
fn differences(vector: &Vector, run: &Run) -> Vec<String> {
    let mut out = Vec::new();
    let stopped = match &run.end {
        End::Pc(pc) => Some(*pc),
        End::Returned => {
            let target = return_target(vector, run);
            if target.is_none() {
                out.push(format!("returned, the machine stopped at {:#x} ({})", vector.pc, vector.sig));
            }
            target
        }
        End::Other(message) => {
            out.push(format!("{message}; the machine stopped at {:#x} ({})", vector.pc, vector.sig));
            None
        }
    };
    if let Some(pc) = stopped
        && pc != vector.pc
    {
        out.push(format!("stopped at {pc:#x}, the machine at {:#x} ({})", vector.pc, vector.sig));
    }
    for (n, (lifted, native)) in run.state.iter().zip(&vector.after.r).enumerate().take(usize::from(PC)) {
        if *lifted != u64::from(*native) {
            out.push(format!("{} is {lifted:#x}, the machine gives {native:#x}", register_name(n as u8)));
        }
    }
    if run.nzcv != vector.after.nzcv {
        out.push(format!("nzcv is {:04b}, the machine gives {}", run.nzcv, vector.after.flags()));
    }
    if run.window != vector.after.window {
        let first = run.window.iter().zip(&vector.after.window).position(|(a, b)| a != b).unwrap();
        out.push(format!(
            "byte {:#x} of the window is {:#04x}, the machine gives {:#04x}",
            vector.input.window_at + first as u64,
            run.window[first],
            vector.after.window[first]
        ));
    }
    let window = vector.input.window_at..vector.input.window_at + WINDOW as u64;
    for (at, bytes) in &run.writes {
        let end = at + bytes.len() as u64;
        if (DATA_BASE..DATA_BASE + DATA_LEN).contains(at) && !(window.contains(at) && end <= window.end) {
            out.push(format!("a store of {} bytes at {at:#x}, outside the window", bytes.len()));
        } else if (IMAGE_BASE..IMAGE_BASE + CODE_MAP_LEN).contains(at) {
            out.push(format!("a store of {} bytes into the code page at {at:#x}", bytes.len()));
        }
    }
    if let Some(stray) = vector.stray {
        out.push(format!("the machine stored outside the window at {stray:#x}"));
    }
    out
}

// endregion

/// Every Thumb-2 vector replays through the lift and the interpreter, exactly the forms of
/// `arm32/PENDING.txt` differ, and the ARM vectors read as the recording they are.
#[test]
fn arm32() {
    let _turn = replay_turn();
    let dir = arm32_dir();
    let pending = read_pending(&dir.join("PENDING.txt"));
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vec"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .vec files under {}", dir.display());

    let mut lifts: HashMap<Vec<u8>, LiftedCase> = HashMap::new();
    let mut scratch = vec![0u8; SCRATCH_LEN];
    let mut forms: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let (mut replayed, mut recorded) = (0, 0);
    for file in &files {
        let group = file.file_stem().unwrap().to_string_lossy().into_owned();
        let vectors = read_vectors(file);
        if is_arm_group(&group) {
            recorded += vectors.len();
            continue;
        }
        for vector in vectors {
            replayed += 1;
            let case = lifts.entry(vector.bytes.clone()).or_insert_with(|| lift_case(&vector.bytes));
            let run = run_lifted(case, &vector.input, &vector.bytes, &mut scratch);
            let found = differences(&vector, &run);
            let key = format!("{} {}", vector.opcode, vector.form);
            let entry = forms.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                (0, Vec::new())
            });
            entry.0 += 1;
            if let Some(first) = found.first() {
                entry.1.push(format!("`{}` ({}): {first}", vector.asm, arm32::hex(&vector.bytes)));
            }
        }
    }

    let mut failures = Vec::new();
    for key in &order {
        let (_, found) = &forms[key];
        match (found.first(), pending.contains_key(key)) {
            (Some(first), false) => failures.push(format!("{key}: {first}")),
            (None, true) => failures.push(format!("remove from PENDING.txt: {key}")),
            _ => {}
        }
    }
    for key in pending.keys() {
        if !forms.contains_key(key) {
            failures.push(format!("PENDING.txt names a form without vectors: {key}"));
        }
    }
    let differing = forms.values().filter(|(_, found)| !found.is_empty()).count();
    println!(
        "arm32 vectors: {replayed} Thumb cases replayed over {} forms, {recorded} ARM cases recorded; {differing} forms differ, {} pending",
        forms.len(),
        pending.len()
    );
    assert!(replayed >= 3000, "{replayed} Thumb vectors replayed, at least 3000 expected");
    assert!(failures.is_empty(), "{} forms are not as PENDING.txt says:\n{}", failures.len(), failures.join("\n"));
}

// region Generating

/// A 32-bit value with interesting shapes as often as a uniform one, as the `AArch64` `value`.
fn value32(rng: &mut Rng) -> u32 {
    match rng.below(8) {
        0 => rng.below(16) as u32,
        1 => u32::MAX - rng.below(16) as u32,
        2 => 1u32 << rng.below(32),
        3 => (1u32 << rng.below(32)).wrapping_sub(1),
        4 => 0x8000_0000 | rng.below(16) as u32,
        5 => 0x7fff_fff0 | rng.below(16) as u32,
        _ => rng.next() as u32,
    }
}

/// A state for a case: the registers it names get values (a memory operand's base, the base of a
/// load or store multiple and `sp` pinned into the data region, an index likewise, a register a
/// branch goes through pinned to `JUMP_PIN`, a compare-and-branch register zero half the time, a
/// divisor zero a third of the time), the flags are random, and a memory form gets a window of
/// random bytes around the address it names, with `JUMP_PIN` in the word a `pop` loads into `pc`.
fn picture(rng: &mut Rng, kind: &str, asm: &str, arm: bool) -> Picture {
    let mut p = Picture::zero();
    let instr = instruction(asm);
    let name = mnemonic(asm);
    let mem = memory_operand(instr);
    let multiple = kind == "Multiple";
    let mut regs: BTreeSet<u8> = tokens(asm).iter().filter_map(|t| general_register(t)).filter(|n| *n != PC).collect();
    let words = tokens(instr);
    // The stack pointer a `push` or `pop` works on is no token of its own.
    let base = match (multiple, name.starts_with("push") || name.starts_with("pop")) {
        (true, true) => Some(SP),
        (true, false) => words.iter().find_map(|t| general_register(t)),
        _ => None,
    };
    regs.extend(base);
    let divisor = if kind == "Div" { words.iter().rev().find_map(|t| general_register(t)) } else { None };
    let jump = (if arm { JUMP_PIN } else { JUMP_PIN | 1 }) as u32;
    for &n in &regs {
        p.r[usize::from(n)] = if n == SP || mem.as_ref().is_some_and(|m| m.base == n) || base == Some(n) {
            BASE_PIN as u32
        } else if mem.as_ref().is_some_and(|m| m.index == Some(n)) {
            INDEX_PIN as u32
        } else if kind == "BranchReg" {
            jump
        } else if (kind == "CompareBranch" && rng.chance(2)) || (divisor == Some(n) && rng.chance(3)) {
            0
        } else {
            value32(rng)
        };
    }
    p.nzcv = rng.below(16) as u8;
    if mem.is_some() || multiple {
        let address = mem.as_ref().map_or(BASE_PIN, |m| m.address);
        p.window_at = (address & !15) - 16;
        rng.fill(&mut p.window);
        let list = register_list(instr);
        if multiple && list.contains(&PC) {
            let slot = if name.starts_with("ldmdb") { BASE_PIN - 4 } else { BASE_PIN + 4 * (list.len() as u64 - 1) };
            let at = (slot - p.window_at) as usize;
            p.window[at..at + 4].copy_from_slice(&jump.to_le_bytes());
        }
    }
    p.named = regs.into_iter().collect();
    p
}

/// One `struct case_in` of guest.c appended to `blob`.
fn pack_in(blob: &mut Vec<u8>, p: &Picture, bytes: &[u8], thumb: bool) {
    for r in &p.r {
        blob.extend_from_slice(&r.to_le_bytes());
    }
    blob.extend_from_slice(&u32::from(p.nzcv).to_le_bytes());
    blob.extend_from_slice(&u32::from(thumb).to_le_bytes());
    blob.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    blob.extend_from_slice(&(p.window_at as u32).to_le_bytes());
    let mut code = [0u8; CODE_MAX];
    code[..bytes.len()].copy_from_slice(bytes);
    blob.extend_from_slice(&code);
    blob.extend_from_slice(&p.window);
}

/// One `struct case_out` of guest.c: the signal, the pc, the state and the stray address plus one.
struct Record {
    sig: u64,
    pc: u64,
    state: Picture,
    stray: u64,
}

fn unpack_out(bytes: &[u8]) -> Record {
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut state = Picture::zero();
    for (i, r) in state.r.iter_mut().enumerate() {
        *r = u32_at(8 + 4 * i);
    }
    state.nzcv = u32_at(72) as u8;
    state.window.copy_from_slice(&bytes[80..144]);
    Record { sig: u64::from(u32_at(0)), pc: u64::from(u32_at(4)), state, stray: u64::from(u32_at(76)) }
}

fn format_in(p: &Picture) -> String {
    let mut line = format!("in nzcv={}", p.flags());
    for &n in &p.named {
        let _ = write!(line, " {}=0x{:x}", register_name(n), p.r[usize::from(n)]);
    }
    if p.window.iter().any(|&b| b != 0) {
        let _ = write!(line, " mem=0x{:x}:{}", p.window_at, arm32::hex(&p.window));
    }
    line
}

fn format_out(p: &Picture, r: &Record) -> String {
    let mut line = format!("out pc=0x{:x} sig={}", r.pc, signal_name(r.sig));
    for (n, (after, before)) in r.state.r.iter().zip(&p.r).enumerate().take(usize::from(PC)) {
        if after != before {
            let _ = write!(line, " {}=0x{after:x}", register_name(n as u8));
        }
    }
    if r.state.nzcv != p.nzcv {
        let _ = write!(line, " nzcv={}", r.state.flags());
    }
    if r.state.window != p.window {
        let _ = write!(line, " mem=0x{:x}:{}", p.window_at, arm32::hex(&r.state.window));
    }
    if r.stray != 0 {
        let _ = write!(line, " stray=0x{:x}", r.stray - 1);
    }
    line
}

/// Builds the guest for ARM with clang in WSL into `scratch`; the path WSL sees for it.
fn build_guest(wsl: &Path, scratch: &Path) -> String {
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness");
    let guest = wsl_path(&scratch.join("guest32"));
    let output = wsl_args(wsl)
        .args([
            "clang",
            "--target=armv7-linux-gnueabi",
            "-marm",
            "-mfloat-abi=soft",
            "-ffreestanding",
            "-fno-builtin",
            "-nostdlib",
            "-static",
            "-fno-stack-protector",
            "-fno-asynchronous-unwind-tables",
            "-O1",
            "-fuse-ld=lld",
            "-Wl,--image-base=0x0a000000",
        ])
        .arg(wsl_path(&harness.join("guest.c")))
        .arg(wsl_path(&harness.join("arm.S")))
        .args(["-o", guest.as_str()])
        .output()
        .expect("wsl runs");
    assert!(output.status.success(), "the guest did not build:\n{}", String::from_utf8_lossy(&output.stderr));
    guest
}

/// Runs the cases of `blob` through the guest under `qemu-arm-static`; the records it wrote.
fn run_guest(wsl: &Path, guest: &str, scratch: &Path, group: &str, blob: &[u8]) -> Vec<u8> {
    let input = scratch.join(format!("{group}.in"));
    let output = scratch.join(format!("{group}.out"));
    fs::write(&input, blob).unwrap();
    let run = wsl_args(wsl)
        .args(["qemu-arm-static", "-cpu", "max", guest])
        .args([wsl_path(&input), wsl_path(&output)])
        .output()
        .expect("wsl runs");
    assert!(run.status.success(), "the guest failed on {group}: {}\n{}", run.status, String::from_utf8_lossy(&run.stderr));
    fs::read(&output).unwrap_or_else(|e| panic!("{}: {e}", output.display()))
}

/// Rewrites `testdata/vectors/arm32/<group>.vec` for every group: the cases over `STATES` states
/// each, run by the guest under `qemu-arm-static` in WSL, in Thumb or ARM state by the group.
#[test]
#[ignore = "rewrites testdata/vectors/arm32/*.vec with the guest under qemu-arm-static in WSL"]
fn write_arm32_vectors() {
    let Some(wsl) = ra_asm_testing::tools::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "arm32_vectors");
    let guest = build_guest(&wsl, &scratch);
    let templates = read_templates(&arm32_dir().join("templates.txt"));
    let mut groups: Vec<&str> = Vec::new();
    for template in &templates {
        if !groups.contains(&template.group.as_str()) {
            groups.push(&template.group);
        }
    }
    let mut total = 0;
    for group in groups {
        let arm = is_arm_group(group);
        let cases = arm32::read_cases(group);
        let forms: Vec<&str> = templates
            .iter()
            .filter(|t| t.group == group)
            .flat_map(|t| std::iter::repeat_n(t.asm.as_str(), arm32::expand(&t.asm).len()))
            .collect();
        assert_eq!(forms.len(), cases.len(), "arm32/cases/{group}.cases is not the expansion of its templates");
        let mut rng = Rng::new(group_seed(group));
        let mut pictures = Vec::new();
        let mut blob = Vec::new();
        for (case, form) in cases.iter().zip(&forms) {
            for _ in 0..STATES {
                let p = picture(&mut rng, &case.opcode, &case.asm, arm);
                pack_in(&mut blob, &p, &case.bytes, !arm);
                pictures.push((case, *form, p));
            }
        }
        assert_eq!(blob.len(), RECORD_IN * pictures.len(), "the record the guest reads");
        let results = run_guest(&wsl, &guest, &scratch, group, &blob);
        assert_eq!(results.len(), RECORD_OUT * pictures.len(), "{group}: the guest answered {} bytes", results.len());
        let state = if arm { "ARM" } else { "Thumb-2" };
        let mut text = format!(
            "# arm32 vectors of the {group} group ({state} code): the state after each case of\n\
             # cases/{group}.cases under qemu-arm-static (-cpu max) over {STATES} states a case, written by\n\
             # write_arm32_vectors in crates/ra-asm-lift/tests/vectors/arm32.rs and replayed by its arm32 test.\n\
             # case <kind> <bytes> <assembly> <- <template>; in: the registers the case sets (the rest are\n\
             # zero), the flags as NZCV bits, the window of data bytes; out: where the case stopped and\n\
             # what it changed.\n"
        );
        for (i, (case, form, p)) in pictures.iter().enumerate() {
            let record = unpack_out(&results[RECORD_OUT * i..RECORD_OUT * (i + 1)]);
            let _ = writeln!(text, "case {} {} {} <- {form}", case.opcode, arm32::hex(&case.bytes), case.asm);
            let _ = writeln!(text, "{}", format_in(p));
            let _ = writeln!(text, "{}", format_out(p, &record));
        }
        let path = arm32_dir().join(format!("{group}.vec"));
        fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        total += pictures.len();
        println!("{group}.vec: {} vectors", pictures.len());
    }
    println!("arm32 vectors: {total}");
}

// endregion
