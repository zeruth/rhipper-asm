//! The instruction vectors replayed through the lift and the IR interpreter, on any host. A
//! vector is one case of `testdata/vectors/<machine>/cases` with a machine state in and the state
//! the processor left out; the files under `testdata/vectors/arm64/*.vec` hold the `AArch64`
//! ones, recorded by the harness of `testdata/vectors/harness` under `qemu-aarch64-static` in WSL.
//!
//! The replay lifts each case from a synthetic image whose page is the harness's (a `brk` fill,
//! the prologue, the instruction, a `brk` after it), runs the lifted function over the state and
//! compares every register, the flags, the window of data bytes and where the case stopped:
//! the `brk` after the instruction, the `brk` a branch landed on, or the address a jump went to.
//! A form whose vectors differ is a finding, not a failure, once it is in `PENDING.txt` with its
//! first difference; the test fails on an unlisted form that differs and on a listed one that
//! passes, so the pending list is exact.
//!
//! `write_arm64_vectors` (ignored: it needs `wsl`) regenerates the files: it builds the guest
//! with clang in WSL, writes the cases with their states, runs them under qemu and writes what
//! came back.
//!
//! The 32-bit ARM vectors (`testdata/vectors/arm32`, recorded under `qemu-arm-static`) replay the
//! same way through the Thumb-2 lift in `arm32.rs`, which shares the layout, the host, the
//! generator's random states and the pending-list rule of this file.
//!
//! The x86-64 vectors (`testdata/vectors/x86_64`, written from the processor by the instruction
//! oracle's `write_vectors`) replay through the x86 lift in `x86_64.rs`, a group as one image
//! and the files' arena as host memory at its own address, under the same pending-list rule with
//! `testdata/vectors/x86_64/PENDING.txt`. The 32-bit x86 vectors (`testdata/vectors/x86`,
//! written by the WOW64 harness through `common/x86_32.rs`) replay the same way in `x86_32.rs`,
//! and the x87 vectors (`testdata/vectors/x87`, the same harness carrying the unit as an fsave
//! image) through the exact 80-bit unit of the lift in `x87.rs`.

// A test target's root is `tests/vectors.rs`, so its modules are named by path.
#[path = "vectors/arm32.rs"]
mod arm32;
mod common;
#[path = "vectors/x86_32.rs"]
mod x86_32;
#[path = "vectors/x86_64.rs"]
mod x86_64;
#[path = "vectors/x87.rs"]
mod x87;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::c_void;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use common::{Rng, arm64_dir, expand, group_seed, read_cases, read_templates, wsl_path};
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Type};

// region The layout the guest and the replay share

/// The synthetic image's base; its `.text` is the case page, mapped in the guest at the same
/// address so that `adr`, the literal loads and the branch targets agree.
const IMAGE_BASE: u64 = 0x3000_0000;
const CODE_MAP_LEN: u64 = 0x10000;
const CODE_BASE: u64 = IMAGE_BASE + 0x1000;
const CODE_LEN: u64 = 0x2000;
/// The address of the case instruction, in the middle of the page so that every `{label}` and
/// literal offset lands inside it.
const IP: u64 = CODE_BASE + 0x1000;
/// The region the loads and stores touch, zero but for the window of a case; the base registers
/// of a memory operand are pinned to `BASE_PIN`, which every offset of the templates stays near.
const DATA_BASE: u64 = 0x4000_0000;
const DATA_LEN: u64 = 0x30000;
const BASE_PIN: u64 = DATA_BASE + 0x10000;
const INDEX_PIN: u64 = 24;
const WINDOW: usize = 64;
/// The machine states each case is run over.
const STATES: usize = 3;

const BRK: u32 = 0xd420_0000;
/// `ldr x9, [x0, #72]; ldr x0, [x0]`: the prologue the trampoline enters, see aarch64.S.
const PROLOGUE: [u32; 2] = [0xf940_2409, 0xf940_0000];

/// One record of the harness's input and of its output, see `struct case_in` in guest.c.
const RECORD: usize = 864;

/// The case page: a `brk` fill, the prologue before the instruction, the instruction at `IP`.
fn code_page(code: u32) -> Vec<u8> {
    let mut page: Vec<u8> = BRK.to_le_bytes().repeat(CODE_LEN as usize / 4);
    let at = (IP - CODE_BASE) as usize;
    page[at - 8..at - 4].copy_from_slice(&PROLOGUE[0].to_le_bytes());
    page[at - 4..at].copy_from_slice(&PROLOGUE[1].to_le_bytes());
    page[at..at + 4].copy_from_slice(&code.to_le_bytes());
    page
}

/// Where a branch to `target` goes: Linux runs user space with the top byte of an address ignored
/// (TBI0), so a target with bit 55 clear loses its top byte, and one with it set is used whole.
fn branch_target(target: u64) -> u64 {
    if target & (1 << 55) == 0 { target & 0x00ff_ffff_ffff_ffff } else { target }
}

// endregion

// region Pictures and vectors

/// A machine state: the general registers (`sp` in slot 31), the flags as four bits (N the
/// highest), the vector registers and the window of data bytes with its address. `named` and
/// `vnamed` are the registers the case sets; the rest are zero.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Picture {
    x: [u64; 32],
    nzcv: u8,
    v: [u128; 32],
    window_at: u64,
    window: [u8; WINDOW],
    named: Vec<u16>,
    vnamed: Vec<u16>,
}

impl Picture {
    fn zero() -> Self {
        Self { x: [0; 32], nzcv: 0, v: [0; 32], window_at: BASE_PIN, window: [0; WINDOW], named: Vec::new(), vnamed: Vec::new() }
    }

    fn flags(&self) -> String {
        format!("{:04b}", self.nzcv)
    }
}

fn register_name(n: u16) -> String {
    if n == 31 { "sp".to_owned() } else { format!("x{n}") }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The name of a signal of the guest, as the vectors write it.
fn signal_name(sig: u64) -> String {
    match sig {
        4 => "ill".to_owned(),
        5 => "trap".to_owned(),
        7 => "bus".to_owned(),
        8 => "fpe".to_owned(),
        11 => "segv".to_owned(),
        other => other.to_string(),
    }
}

/// One vector: the case, the template it came from, the state in and the state the machine left,
/// with where it stopped and the first address outside the window it stored to, if any.
#[derive(Debug, Clone)]
struct Vector {
    opcode: String,
    code: u32,
    asm: String,
    form: String,
    input: Picture,
    pc: u64,
    sig: String,
    after: Picture,
    stray: Option<u64>,
}

fn parse_u64(text: &str, what: &str) -> u64 {
    u64::from_str_radix(text.trim_start_matches("0x"), 16).unwrap_or_else(|e| panic!("{what}: {text}: {e}"))
}

fn parse_u128(text: &str, what: &str) -> u128 {
    u128::from_str_radix(text.trim_start_matches("0x"), 16).unwrap_or_else(|e| panic!("{what}: {text}: {e}"))
}

/// What an `out` line says besides the registers.
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
            "sp" => {
                picture.x[31] = parse_u64(value, where_);
                picture.named.push(31);
            }
            "pc" => ending.pc = Some(parse_u64(value, where_)),
            "sig" => ending.sig = Some(value.to_owned()),
            "stray" => ending.stray = Some(parse_u64(value, where_)),
            "mem" => {
                let (at, bytes) = value.split_once(':').unwrap_or_else(|| panic!("{where_}: mem={value}"));
                picture.window_at = parse_u64(at, where_);
                assert_eq!(bytes.len(), 2 * WINDOW, "{where_}: a window of {WINDOW} bytes");
                for (i, pair) in bytes.as_bytes().chunks(2).enumerate() {
                    picture.window[i] =
                        u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap_or_else(|e| panic!("{where_}: {e}"));
                }
            }
            _ if key.starts_with('x') => {
                let n: u16 = key[1..].parse().unwrap_or_else(|_| panic!("{where_}: register {key}"));
                picture.x[usize::from(n)] = parse_u64(value, where_);
                picture.named.push(n);
            }
            _ if key.starts_with('v') => {
                let n: u16 = key[1..].parse().unwrap_or_else(|_| panic!("{where_}: register {key}"));
                picture.v[usize::from(n)] = parse_u128(value, where_);
                picture.vnamed.push(n);
            }
            _ => panic!("{where_}: unknown field {key}"),
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
            panic!("{where_}: not `case opcode code asm`")
        };
        let code = u32::from_str_radix(code, 16).unwrap_or_else(|e| panic!("{where_}: {code}: {e}"));
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
            code,
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

/// The tokens of an assembly line: runs of letters, digits and dots, so `v0.2d` stays one.
fn tokens(asm: &str) -> Vec<&str> {
    asm.split(|c: char| !(c.is_ascii_alphanumeric() || c == '.')).filter(|t| !t.is_empty()).collect()
}

/// The number of a general register token; `sp` and `wsp` are 31, the zero registers are none.
fn general_register(token: &str) -> Option<u16> {
    if token == "sp" || token == "wsp" {
        return Some(31);
    }
    let digits = token.strip_prefix('x').or_else(|| token.strip_prefix('w'))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| *n <= 30)
}

/// A vector register token: its kind (`d`, `s`, `q` or `v`), its number and its arrangement.
fn vector_register(token: &str) -> Option<(char, u16, &str)> {
    let (name, lanes) = token.split_once('.').unwrap_or((token, ""));
    let kind = name.chars().next()?;
    if !"dsqv".contains(kind) || name.len() < 2 || !name[1..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name[1..].parse().ok().filter(|n| *n <= 31).map(|n| (kind, n, lanes))
}

/// `ret` goes to x30, `ret xN` to xN; `None` for the zero register.
fn ret_register(asm: &str) -> Option<u16> {
    match tokens(asm).get(1) {
        None => Some(30),
        Some(token) => general_register(token),
    }
}

fn parse_int(text: &str) -> i64 {
    let (negative, digits) = text.strip_prefix('-').map_or((false, text), |rest| (true, rest));
    let value = match digits.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16),
        None => digits.parse(),
    }
    .unwrap_or_else(|e| panic!("{text}: {e}"));
    if negative { -value } else { value }
}

/// The bracketed operand of a load or store: its base, its index and the address it names with
/// the base at `BASE_PIN` and the index at `INDEX_PIN`.
struct MemoryOperand {
    base: u16,
    index: Option<u16>,
    address: u64,
}

fn memory_operand(asm: &str) -> Option<MemoryOperand> {
    let open = asm.find('[')?;
    let close = asm[open..].find(']')? + open;
    let mut parts = asm[open + 1..close].split(", ");
    let base = general_register(parts.next()?)?;
    let mut address = BASE_PIN;
    let mut index = None;
    if let Some(second) = parts.next() {
        if let Some(imm) = second.strip_prefix('#') {
            address = address.wrapping_add(parse_int(imm) as u64);
        } else {
            index = general_register(second);
            let amount = parts.next().and_then(|ext| ext.split_once('#')).map_or(0, |(_, a)| a.parse::<u32>().expect("a shift amount"));
            address += INDEX_PIN << amount;
        }
    }
    Some(MemoryOperand { base, index, address })
}

// endregion

// region The lifted run

/// A PE32+ for ARM64 with one `.text` section, the case page, at RVA 0x1000; the entry is `IP`.
fn pe_with_page(page: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 0x400 + page.len()];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let w64 = |f: &mut Vec<u8>, at: usize, v: u64| f[at..at + 8].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0xAA64);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xF0);
    w16(&mut f, coff + 18, 0x22);
    let opt = coff + 20;
    w16(&mut f, opt, 0x20B);
    w32(&mut f, opt + 4, CODE_LEN as u32);
    w32(&mut f, opt + 16, (IP - IMAGE_BASE) as u32);
    w32(&mut f, opt + 20, 0x1000);
    w64(&mut f, opt + 24, IMAGE_BASE);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, 0x1000 + CODE_LEN as u32);
    w32(&mut f, opt + 60, 0x400);
    w16(&mut f, opt + 68, 3);
    w64(&mut f, opt + 72, 0x10_0000);
    w64(&mut f, opt + 80, 0x1000);
    w64(&mut f, opt + 88, 0x10_0000);
    w64(&mut f, opt + 96, 0x1000);
    w32(&mut f, opt + 108, 16);
    let sec = opt + 0xF0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, CODE_LEN as u32);
    w32(&mut f, sec + 12, 0x1000);
    w32(&mut f, sec + 16, CODE_LEN as u32);
    w32(&mut f, sec + 20, 0x400);
    w32(&mut f, sec + 36, 0x6000_0020);
    f[0x400..].copy_from_slice(page);
    f
}

/// The lift of one case: the function at `IP` of the synthetic image.
struct LiftedCase {
    lifted: ra_asm_lift::Lifted,
    name: String,
}

fn lift_case(code: u32) -> LiftedCase {
    let image = ra_asm_pe::Image::parse(pe_with_page(&code_page(code))).expect("the synthetic image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    LiftedCase { lifted, name: ra_asm_lift::function_name(IP) }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
}

#[cfg(not(windows))]
unsafe extern "C" {
    fn mmap(address: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut c_void;
}

/// Commits `len` bytes of host memory at exactly `at`: the interpreter's regions are the host's
/// own memory, so the guest's addresses must be the replay's.
fn map_fixed(at: u64, len: u64) -> bool {
    #[cfg(windows)]
    let got = unsafe { VirtualAlloc(at as usize as *mut c_void, len as usize, 0x3000, 0x04) };
    // Linux: PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE.
    #[cfg(not(windows))]
    let got = unsafe { mmap(at as usize as *mut c_void, len as usize, 3, 0x02 | 0x20 | 0x10_0000, -1, 0) };
    got as usize as u64 == at
}

static FIXED: OnceLock<bool> = OnceLock::new();

fn ensure_fixed() {
    let mapped = *FIXED.get_or_init(|| map_fixed(IMAGE_BASE, CODE_MAP_LEN) && map_fixed(DATA_BASE, DATA_LEN));
    assert!(mapped, "the replay needs host memory at {IMAGE_BASE:#x} and {DATA_BASE:#x}, the guest's addresses");
}

/// The fixed memory is one per process, so the replays of the architectures take turns: the test
/// harness runs them on threads of its own, and a failed one must not keep the others out.
static REPLAY: Mutex<()> = Mutex::new(());

fn replay_turn() -> MutexGuard<'static, ()> {
    REPLAY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The state block, the interpreter's statics and its frames, in one allocation of the test's.
const SCRATCH_LEN: usize = 0x40000;
const STATICS_AT: u64 = 0x1000;
const FRAMES_AT: u64 = 0x20000;

const END_TRAP: &str = "the trap at ";
const END_JUMP: &str = "the jump to ";
const END_UNSUPPORTED: &str = "unsupported by the lift: the instruction at ";

/// The host of a lone instruction: a trap, a jump through the runtime and an unsupported
/// instruction end the run with a message the replay reads; nothing else may be called.
struct Ends;

impl Host for Ends {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let arg = |i: usize| args.get(i).map_or(0, Value::as_u64);
        match name {
            "ra_trap" => Err(Stop::fault(format!("{END_TRAP}{:#x}", arg(0)))),
            "ra_call" => Err(Stop::fault(format!("{END_JUMP}{:#x}", arg(1)))),
            "ra_unsupported" => Err(Stop::fault(format!("{END_UNSUPPORTED}{:#x}", arg(0)))),
            other => Err(Stop::fault(format!("a call to {other}"))),
        }
    }
}

/// Where the lifted run ended: at an address (a trap, a jump, a fault), by returning, or otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
enum End {
    Pc(u64),
    Returned,
    Other(String),
}

/// The hexadecimal number at the start of `text`.
fn leading_hex(text: &str) -> Option<u64> {
    let digits: String = text.trim_start_matches("0x").chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    u64::from_str_radix(&digits, 16).ok()
}

/// `site` is the address of the instruction the interpreter was running when it stopped: the pc
/// of its own faults, whose messages name the address accessed, not the instruction's.
fn end_of(result: Result<Value, Stop>, site: Option<u64>) -> End {
    match result {
        Ok(_) => End::Returned,
        Err(Stop::Fault(message)) => {
            if let Some(rest) = message.strip_prefix(END_TRAP) {
                End::Pc(leading_hex(rest).unwrap_or(0))
            } else if let Some(rest) = message.strip_prefix(END_JUMP) {
                End::Pc(branch_target(leading_hex(rest).unwrap_or(0)))
            } else if message.starts_with(END_UNSUPPORTED) {
                End::Other(message)
            } else {
                // The interpreter's own fault, a memory access most often: the machine faults at
                // the same instruction, with a signal.
                site.map_or(End::Other(message), End::Pc)
            }
        }
        Err(Stop::Limit(n)) => End::Other(format!("the limit of {n} instructions")),
        Err(Stop::Exit(c)) => End::Other(format!("exit {c}")),
        Err(Stop::Unpacked(a)) => End::Other(format!("an entry of {a:#x}, which the run wrote")),
    }
}

/// What a lifted run left: its end, the state, the window and the stores it made.
struct Run {
    end: End,
    state: Picture,
    window: [u8; WINDOW],
    writes: Vec<(u64, Vec<u8>)>,
}

fn run_lifted(case: &LiftedCase, input: &Picture, code: u32, scratch: &mut [u8]) -> Run {
    ensure_fixed();
    let scratch_at = scratch.as_ptr() as usize as u64;
    let mut memory = Memory::new();
    memory.add_region(Region::new(IMAGE_BASE, CODE_MAP_LEN, "code"));
    memory.add_region(Region::new(DATA_BASE, DATA_LEN, "data"));
    memory.add_region(Region::new(scratch_at, scratch.len() as u64, "scratch"));
    memory.write(CODE_BASE, &code_page(code)).expect("the code page");
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
    let (gprs_at, flags_at, vectors_at) = (offsets[0] as usize, offsets[2] as usize, offsets[3] as usize);
    let mut state = vec![0u8; size as usize];
    for (i, x) in input.x.iter().enumerate() {
        state[gprs_at + 8 * i..gprs_at + 8 * i + 8].copy_from_slice(&x.to_le_bytes());
    }
    for (i, bit) in [3u8, 2, 1, 0].into_iter().enumerate() {
        state[flags_at + i] = (input.nzcv >> bit) & 1;
    }
    for (i, v) in input.v.iter().enumerate() {
        state[vectors_at + 16 * i..vectors_at + 16 * i + 16].copy_from_slice(&v.to_le_bytes());
    }
    vm.memory.write(scratch_at, &state).expect("the state");
    vm.memory.write(input.window_at, &input.window).expect("the window");
    vm.memory.arm();
    let result = vm.call_named(&mut Ends, &case.name, vec![Value::Int(u128::from(scratch_at))]);
    let site = vm.site;
    let access = vm.memory.disarm();

    let state = vm.memory.read_vec(scratch_at, size).expect("the state");
    let mut after = Picture::zero();
    for (i, x) in after.x.iter_mut().enumerate() {
        *x = u64::from_le_bytes(state[gprs_at + 8 * i..gprs_at + 8 * i + 8].try_into().unwrap());
    }
    after.nzcv = (0..4).fold(0, |acc, i| (acc << 1) | (state[flags_at + i] & 1));
    for (i, v) in after.v.iter_mut().enumerate() {
        *v = u128::from_le_bytes(state[vectors_at + 16 * i..vectors_at + 16 * i + 16].try_into().unwrap());
    }
    let window: [u8; WINDOW] = vm.memory.read_vec(input.window_at, WINDOW as u64).expect("the window").try_into().unwrap();
    // The data region stays zero for the next run.
    vm.memory.fill(input.window_at, WINDOW as u64, 0).unwrap();
    for (at, bytes) in &access.writes {
        if (DATA_BASE..DATA_BASE + DATA_LEN).contains(at) {
            vm.memory.fill(*at, bytes.len() as u64, 0).unwrap();
        }
    }
    Run { end: end_of(result, site), state: after, window, writes: access.writes }
}

/// Every way the lifted run differs from the machine's record, the stop first.
fn differences(vector: &Vector, run: &Run) -> Vec<String> {
    let mut out = Vec::new();
    let stopped = match &run.end {
        End::Pc(pc) => Some(*pc),
        End::Returned if vector.opcode == "RET" => {
            Some(branch_target(ret_register(&vector.asm).map_or(0, |n| run.state.x[usize::from(n)])))
        }
        End::Returned => {
            out.push(format!("returned, the machine stopped at {:#x} ({})", vector.pc, vector.sig));
            None
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
    for (n, (lifted, native)) in run.state.x.iter().zip(&vector.after.x).enumerate() {
        if lifted != native {
            out.push(format!("{} is {lifted:#x}, the machine gives {native:#x}", register_name(n as u16)));
        }
    }
    if run.state.nzcv != vector.after.nzcv {
        out.push(format!("nzcv is {}, the machine gives {}", run.state.flags(), vector.after.flags()));
    }
    for (n, (lifted, native)) in run.state.v.iter().zip(&vector.after.v).enumerate() {
        if lifted != native {
            out.push(format!("v{n} is {lifted:#034x}, the machine gives {native:#034x}"));
        }
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

// region The pending list

fn pending_file() -> PathBuf {
    arm64_dir().join("PENDING.txt")
}

/// The forms listed as differing today, by `OPCODE template`, with their notes.
fn read_pending(path: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once(": ").map_or_else(|| (line.to_owned(), String::new()), |(k, v)| (k.to_owned(), v.to_owned())))
        .collect()
}

/// The forms a replay saw, by the key `PENDING.txt` names them with: the count of their cases and
/// the first difference of each case that differed.
type Forms = BTreeMap<String, (usize, Vec<String>)>;

/// What is not as `PENDING.txt` says, in `order` (the forms as first seen): an unlisted form that
/// differs, with its first difference as a line to paste; a listed form that passes, so the card
/// that fixed it removes its line; a listed form the files do not hold.
fn pending_failures(order: &[String], forms: &Forms, pending: &BTreeMap<String, String>) -> Vec<String> {
    let mut failures = Vec::new();
    for key in order {
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
    failures
}

mod pending {
    use std::collections::BTreeMap;

    use super::{Forms, pending_failures};

    fn seen(forms: &[(&str, &[&str])]) -> (Vec<String>, Forms) {
        let order: Vec<String> = forms.iter().map(|(key, _)| (*key).to_owned()).collect();
        let forms = forms.iter().map(|(key, found)| ((*key).to_owned(), (11, found.iter().map(|s| (*s).to_owned()).collect()))).collect();
        (order, forms)
    }

    fn listed(keys: &[&str]) -> BTreeMap<String, String> {
        keys.iter().map(|k| ((*k).to_owned(), "why".to_owned())).collect()
    }

    #[test]
    fn pending_accepts_a_listed_form_that_differs() {
        let (order, forms) = seen(&[("arith Adc_r32_imm32", &["eax: expected 0x1, lifted 0x2"]), ("arith Add_r64_rm64", &[])]);
        assert!(pending_failures(&order, &forms, &listed(&["arith Adc_r32_imm32"])).is_empty());
    }

    #[test]
    fn pending_rejects_a_listed_form_that_passes() {
        let (order, forms) = seen(&[("arith Add_r64_rm64", &[])]);
        let failures = pending_failures(&order, &forms, &listed(&["arith Add_r64_rm64"]));
        assert_eq!(failures, ["remove from PENDING.txt: arith Add_r64_rm64"]);
    }

    #[test]
    fn pending_rejects_an_unlisted_form_that_differs() {
        let (order, forms) = seen(&[("arith Adc_r32_imm32", &["eax: expected 0x1, lifted 0x2", "a second case"])]);
        let failures = pending_failures(&order, &forms, &listed(&[]));
        assert_eq!(failures, ["arith Adc_r32_imm32: eax: expected 0x1, lifted 0x2"]);
    }

    #[test]
    fn pending_rejects_a_listed_form_without_vectors() {
        let (order, forms) = seen(&[("arith Add_r64_rm64", &[])]);
        let failures = pending_failures(&order, &forms, &listed(&["misc Cpuid"]));
        assert_eq!(failures, ["PENDING.txt names a form without vectors: misc Cpuid"]);
    }
}

// endregion

// region The forms count

/// The forms the x86-64 vector files hold: a worker reads this constant instead of running the
/// suite, and `forms_count` keeps it true to the committed files.
const FORMS: usize = 733;

/// The states a form has in `testdata/vectors/x86_64/*.vec`, one case line each (the
/// `VECTOR_SAMPLES` of the instruction oracle's `write_vectors`).
const SAMPLES: usize = 11;

/// The committed x86-64 vector files hold `FORMS` forms: their non-`#` lines, `SAMPLES` a form.
#[test]
fn forms_count() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/x86_64");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vec"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .vec files under {}", dir.display());
    let mut lines = 0;
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        lines += text.lines().filter(|line| !line.trim().is_empty() && !line.starts_with('#')).count();
    }
    assert_eq!(lines % SAMPLES, 0, "{lines} case lines are not a whole number of forms at {SAMPLES} states each");
    let forms = lines / SAMPLES;
    println!("{forms} forms");
    assert_eq!(forms, FORMS, "the files hold {forms} forms; set FORMS in tests/vectors.rs to match");
}

// endregion

/// Every `AArch64` vector replays through the lift and the interpreter, and exactly the forms of
/// `PENDING.txt` differ.
#[test]
fn arm64() {
    let _turn = replay_turn();
    let dir = arm64_dir();
    let pending = read_pending(&pending_file());
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "vec"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .vec files under {}", dir.display());

    let mut lifts: HashMap<u32, LiftedCase> = HashMap::new();
    let mut scratch = vec![0u8; SCRATCH_LEN];
    let mut forms = Forms::new();
    let mut order: Vec<String> = Vec::new();
    let mut total = 0;
    for file in &files {
        for vector in read_vectors(file) {
            total += 1;
            if !lifts.contains_key(&vector.code) {
                lifts.insert(vector.code, lift_case(vector.code));
            }
            let run = run_lifted(&lifts[&vector.code], &vector.input, vector.code, &mut scratch);
            let found = differences(&vector, &run);
            let key = format!("{} {}", vector.opcode, vector.form);
            let entry = forms.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                (0, Vec::new())
            });
            entry.0 += 1;
            if let Some(first) = found.first() {
                entry.1.push(format!("`{}` ({:08x}): {first}", vector.asm, vector.code));
            }
        }
    }

    let failures = pending_failures(&order, &forms, &pending);
    let differing = forms.values().filter(|(_, found)| !found.is_empty()).count();
    println!("arm64 vectors: {total} cases replayed over {} forms; {differing} forms differ, {} pending", forms.len(), pending.len());
    assert!(total >= 4000, "{total} vectors replayed, at least 4000 expected");
    assert!(failures.is_empty(), "{} forms are not as PENDING.txt says:\n{}", failures.len(), failures.join("\n"));
}

// region Generating

/// A vector register's 128 bits with finite floats in the lanes the form reads.
fn vector_value(rng: &mut Rng, kind: char, lanes: &str, positive: bool) -> u128 {
    let mut high = rng.next();
    let mut low = rng.next();
    match (kind, lanes) {
        ('d', _) => low = rng.f64_bits(positive),
        ('s', _) => low = (low & !0xffff_ffff) | rng.f32_bits(positive),
        ('v', "2d") => {
            low = rng.f64_bits(positive);
            high = rng.f64_bits(positive);
        }
        ('v', "4s") => {
            low = rng.f32_bits(positive) | (rng.f32_bits(positive) << 32);
            high = rng.f32_bits(positive) | (rng.f32_bits(positive) << 32);
        }
        ('v', "2s") => low = rng.f32_bits(positive) | (rng.f32_bits(positive) << 32),
        _ => {}
    }
    (u128::from(high) << 64) | u128::from(low)
}

/// A state for `asm`: the registers it names get values (a memory operand's base and index pinned
/// into the data region, `sp` likewise, a compare-and-branch register zero half the time, a
/// float register finite), the flags are random, and a memory form gets a window of random bytes
/// around the address it names. Everything else is zero.
fn picture(rng: &mut Rng, opcode: &str, asm: &str) -> Picture {
    let mut p = Picture::zero();
    let mem = memory_operand(asm);
    let words = tokens(asm);
    let mut regs: BTreeSet<u16> = words.iter().filter_map(|t| general_register(t)).collect();
    if opcode == "RET" && regs.is_empty() {
        regs.insert(30);
    }
    for &n in &regs {
        p.x[usize::from(n)] = if n == 31 || mem.as_ref().is_some_and(|m| m.base == n) {
            BASE_PIN
        } else if mem.as_ref().is_some_and(|m| m.index == Some(n)) {
            INDEX_PIN
        } else if matches!(opcode, "CBZ" | "CBNZ") && rng.chance(2) {
            0
        } else {
            rng.value()
        };
    }
    p.nzcv = rng.below(16) as u8;
    let positive = asm.starts_with("fsqrt");
    let mut seen: BTreeMap<u16, (char, &str)> = BTreeMap::new();
    for word in &words {
        if let Some((kind, n, lanes)) = vector_register(word) {
            seen.insert(n, (kind, lanes));
        }
    }
    for (&n, &(kind, lanes)) in &seen {
        p.v[usize::from(n)] = vector_value(rng, kind, lanes, positive);
    }
    if let Some(mem) = &mem {
        p.window_at = (mem.address & !15) - 16;
        rng.fill(&mut p.window);
    }
    p.named = regs.into_iter().collect();
    p.vnamed = seen.keys().copied().collect();
    p
}

/// One `struct case_in` of guest.c appended to `blob`.
fn pack_in(blob: &mut Vec<u8>, p: &Picture, code: u32) {
    for x in &p.x {
        blob.extend_from_slice(&x.to_le_bytes());
    }
    blob.extend_from_slice(&u64::from(p.nzcv).to_le_bytes());
    blob.extend_from_slice(&0u64.to_le_bytes());
    for v in &p.v {
        blob.extend_from_slice(&v.to_le_bytes());
    }
    blob.extend_from_slice(&code.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    blob.extend_from_slice(&p.window_at.to_le_bytes());
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
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let mut state = Picture::zero();
    for (i, x) in state.x.iter_mut().enumerate() {
        *x = u64_at(16 + 8 * i);
    }
    state.nzcv = u64_at(272) as u8;
    for (i, v) in state.v.iter_mut().enumerate() {
        *v = u128::from_le_bytes(bytes[280 + 16 * i..296 + 16 * i].try_into().unwrap());
    }
    state.window.copy_from_slice(&bytes[800..864]);
    Record { sig: u64_at(0), pc: u64_at(8), state, stray: u64_at(792) }
}

fn format_in(p: &Picture) -> String {
    let mut line = format!("in nzcv={}", p.flags());
    for &n in &p.named {
        let _ = write!(line, " {}=0x{:x}", register_name(n), p.x[usize::from(n)]);
    }
    for &n in &p.vnamed {
        let _ = write!(line, " v{n}=0x{:032x}", p.v[usize::from(n)]);
    }
    if p.window.iter().any(|&b| b != 0) {
        let _ = write!(line, " mem=0x{:x}:{}", p.window_at, hex(&p.window));
    }
    line
}

fn format_out(p: &Picture, r: &Record) -> String {
    let mut line = format!("out pc=0x{:x} sig={}", r.pc, signal_name(r.sig));
    for (n, (after, before)) in r.state.x.iter().zip(&p.x).enumerate() {
        if after != before {
            let _ = write!(line, " {}=0x{after:x}", register_name(n as u16));
        }
    }
    if r.state.nzcv != p.nzcv {
        let _ = write!(line, " nzcv={}", r.state.flags());
    }
    for (n, (after, before)) in r.state.v.iter().zip(&p.v).enumerate() {
        if after != before {
            let _ = write!(line, " v{n}=0x{after:032x}");
        }
    }
    if r.state.window != p.window {
        let _ = write!(line, " mem=0x{:x}:{}", p.window_at, hex(&r.state.window));
    }
    if r.stray != 0 {
        let _ = write!(line, " stray=0x{:x}", r.stray - 1);
    }
    line
}

fn wsl_args(wsl: &Path) -> Command {
    let mut command = Command::new(wsl);
    command.args(["-d", "Ubuntu-24.04", "--"]);
    command
}

/// Builds the guest with clang in WSL into `scratch`; the path WSL sees for it.
fn build_guest(wsl: &Path, scratch: &Path) -> String {
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness");
    let guest = wsl_path(&scratch.join("guest"));
    let output = wsl_args(wsl)
        .args([
            "clang",
            "--target=aarch64-linux-gnu",
            "-ffreestanding",
            "-fno-builtin",
            "-nostdlib",
            "-static",
            "-mgeneral-regs-only",
            "-fno-stack-protector",
            "-fno-asynchronous-unwind-tables",
            "-O1",
            "-fuse-ld=lld",
            "-Wl,--image-base=0x0a000000",
        ])
        .arg(wsl_path(&harness.join("guest.c")))
        .arg(wsl_path(&harness.join("aarch64.S")))
        .args(["-o", guest.as_str()])
        .output()
        .expect("wsl runs");
    assert!(output.status.success(), "the guest did not build:\n{}", String::from_utf8_lossy(&output.stderr));
    guest
}

/// Runs the cases of `blob` through the guest under qemu; the records it wrote.
fn run_guest(wsl: &Path, guest: &str, scratch: &Path, group: &str, blob: &[u8]) -> Vec<u8> {
    let input = scratch.join(format!("{group}.in"));
    let output = scratch.join(format!("{group}.out"));
    fs::write(&input, blob).unwrap();
    let run = wsl_args(wsl)
        .args(["qemu-aarch64-static", "-cpu", "max,pauth=off", guest])
        .args([wsl_path(&input), wsl_path(&output)])
        .output()
        .expect("wsl runs");
    assert!(run.status.success(), "the guest failed on {group}: {}\n{}", run.status, String::from_utf8_lossy(&run.stderr));
    fs::read(&output).unwrap_or_else(|e| panic!("{}: {e}", output.display()))
}

/// Rewrites `testdata/vectors/arm64/<group>.vec` for every group: the cases over `STATES` states
/// each, run by the guest under `qemu-aarch64-static` in WSL.
#[test]
#[ignore = "rewrites testdata/vectors/arm64/*.vec with the guest under qemu-aarch64-static in WSL"]
fn write_arm64_vectors() {
    let Some(wsl) = ra_asm_testing::tools::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "arm64_vectors");
    let guest = build_guest(&wsl, &scratch);
    let templates = read_templates(&arm64_dir().join("templates.txt"));
    let mut groups: Vec<&str> = Vec::new();
    for template in &templates {
        if !groups.contains(&template.group.as_str()) {
            groups.push(&template.group);
        }
    }
    let mut total = 0;
    for group in groups {
        let cases = read_cases(group);
        let forms: Vec<&str> =
            templates.iter().filter(|t| t.group == group).flat_map(|t| std::iter::repeat_n(t.asm.as_str(), expand(&t.asm).len())).collect();
        assert_eq!(forms.len(), cases.len(), "cases/{group}.cases is not the expansion of its templates");
        let mut rng = Rng::new(group_seed(group));
        let mut pictures = Vec::new();
        let mut blob = Vec::new();
        for (case, form) in cases.iter().zip(&forms) {
            for _ in 0..STATES {
                let p = picture(&mut rng, &case.opcode, &case.asm);
                pack_in(&mut blob, &p, case.code);
                pictures.push((case, *form, p));
            }
        }
        let results = run_guest(&wsl, &guest, &scratch, group, &blob);
        assert_eq!(results.len(), RECORD * pictures.len(), "{group}: the guest answered {} bytes", results.len());
        let mut text = format!(
            "# arm64 vectors of the {group} group: the state after each case of cases/{group}.cases under\n\
             # qemu-aarch64-static (-cpu max,pauth=off) over {STATES} states a case, written by\n\
             # write_arm64_vectors in crates/ra-asm-lift/tests/vectors.rs and replayed by its arm64 test.\n\
             # case <opcode> <encoding> <assembly> <- <template>; in: the registers the case sets (the\n\
             # rest are zero), the flags as NZCV bits, the window of data bytes; out: where the case\n\
             # stopped and what it changed.\n"
        );
        for (i, (case, form, p)) in pictures.iter().enumerate() {
            let record = unpack_out(&results[RECORD * i..RECORD * (i + 1)]);
            let _ = writeln!(text, "case {} {:08x} {} <- {form}", case.opcode, case.code, case.asm);
            let _ = writeln!(text, "{}", format_in(p));
            let _ = writeln!(text, "{}", format_out(p, &record));
        }
        let path = arm64_dir().join(format!("{group}.vec"));
        fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        total += pictures.len();
        println!("{group}.vec: {} vectors", pictures.len());
    }
    println!("arm64 vectors: {total}");
}

// endregion
