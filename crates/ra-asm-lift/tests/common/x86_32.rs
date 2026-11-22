//! The 32-bit x86 generator: iced's code table at 32 bits, kept to the integer forms whose
//! mnemonic has an arm in `x86.rs` (and the 32-bit-only ones the port needs before their arm
//! lands), each form given random operands over random states and run on this machine's own
//! processor under WOW64 by `testdata/vectors/harness/x86_32.c`. The answers are written as the
//! vector files of `testdata/vectors/x86`, one a mnemonic group, in the format of
//! `ra_asm_testing::vectors`, which `vectors/x86_32.rs` replays through the lift on any host.
//!
//! The record the harness reads and writes is here too (`HarnessIn`, `HarnessOut` and their
//! packing), with the build of the harness by clang and its run; `vectors_gen.rs` checks three
//! cases of it by hand and holds the ignored writer.
//!
//! The harness carries one window of 64 data bytes a case, so a form's operands are placed to
//! fit one: a memory operand sixteen-aligned in the data buffer, the string pointers 24 bytes
//! apart with a count of up to four, a stack form's window around `esp`, an `esp`-based operand
//! just above it. Windows dispatches the exception that ends a case on the case's own stack, so
//! the bytes of a stack window below the final `esp` are the dispatcher's, not the case's: the
//! generator records them as unchanged.
//!
//! The x87 unit rides along as an fsave image in both records ([`X87Image`]); the integer forms
//! give the harness a freshly initialized unit and read nothing back. The x87 generator, which
//! builds on the instances, states and cases of this file, is `x87.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use ra_asm_testing::vectors::{self, Case, Outcome, State};
use ra_asm_x86::iced_x86::{
    Code, Encoder, EncodingKind, Formatter, Instruction, IntelFormatter, Mnemonic, OpCodeOperandKind as K, OpKind, Register as R,
    RflagsBits,
};

use super::Rng;

// region The harness

/// The layout `x86_32.c` maps: the arena of the loads, stores and the stack, with the data at
/// `+0x2000` and the stack from `+0x4000` to `+0x8000` as the x86-64 vectors have them, and the
/// page the case runs in.
pub(crate) const ARENA32: u32 = 0x2000_0000;
pub(crate) const ARENA32_LEN: u32 = 0x10000;
pub(crate) const DATA32: u32 = ARENA32 + 0x2000;
pub(crate) const DATA32_LEN: u32 = 0x2000;
pub(crate) const STACK32: u32 = ARENA32 + 0x4000;
pub(crate) const STACK32_END: u32 = ARENA32 + 0x8000;
pub(crate) const CASE_IP32: u32 = 0x3000_2000;
pub(crate) const WINDOW32: usize = 64;
pub(crate) const CODE32_MAX: usize = 16;
/// The 32-bit protected-mode fsave image: seven dwords of words and pointers, then the eight
/// registers of ten bytes in stack order.
pub(crate) const FSAVE_LEN: usize = 108;
pub(crate) const IN32_LEN: usize = 240;
pub(crate) const OUT32_LEN: usize = 232;

pub(crate) const STATUS_BREAKPOINT: u32 = 0x8000_0003;
pub(crate) const STATUS_ACCESS_VIOLATION: u32 = 0xC000_0005;
/// The index of each register in the harness's record, iced's order.
pub(crate) const EAX: usize = 0;
pub(crate) const ECX: usize = 1;
pub(crate) const EDX: usize = 2;
pub(crate) const EBX: usize = 3;
pub(crate) const ESP: usize = 4;
pub(crate) const EBP: usize = 5;
pub(crate) const ESI: usize = 6;
pub(crate) const EDI: usize = 7;
pub(crate) const GPR_NAMES32: [&str; 8] = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
pub(crate) const FLAG_NAMES: [&str; 7] = ["cf", "pf", "af", "zf", "sf", "df", "of"];
/// The EFLAGS bit of each flag, as the `undefined` mask of a case names them.
pub(crate) const FLAG_BITS: [u32; 7] = [0, 2, 4, 6, 7, 10, 11];
const FLAG_RFLAGS: [u32; 7] =
    [RflagsBits::CF, RflagsBits::PF, RflagsBits::AF, RflagsBits::ZF, RflagsBits::SF, RflagsBits::DF, RflagsBits::OF];

/// The x87 unit of a case as its fsave image carries it: the control, status and tag words and
/// the eight registers in stack order (`st(0)` first), ten bytes each as the `tbyte` form lays
/// them, the significand first. The instruction and data pointers of the image are not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct X87Image {
    pub(crate) cw: u16,
    pub(crate) sw: u16,
    pub(crate) tw: u16,
    pub(crate) st: [[u8; 10]; 8],
}

impl X87Image {
    /// The unit as `fninit` leaves it: every register empty, the default control word.
    pub(crate) fn init() -> Self {
        Self { cw: 0x037f, sw: 0, tw: 0xffff, st: [[0; 10]; 8] }
    }

    fn pack(&self, blob: &mut Vec<u8>) {
        for word in [self.cw, self.sw, self.tw] {
            blob.extend_from_slice(&u32::from(word).to_le_bytes());
        }
        blob.extend_from_slice(&[0u8; 16]);
        for st in &self.st {
            blob.extend_from_slice(st);
        }
    }

    fn unpack(image: &[u8]) -> Self {
        assert_eq!(image.len(), FSAVE_LEN);
        let word = |i: usize| u16::from_le_bytes([image[4 * i], image[4 * i + 1]]);
        let mut st = [[0u8; 10]; 8];
        for (i, reg) in st.iter_mut().enumerate() {
            reg.copy_from_slice(&image[28 + 10 * i..38 + 10 * i]);
        }
        Self { cw: word(0), sw: word(1), tw: word(2), st }
    }
}

/// One case in: the registers in the record's order, the flags as EFLAGS bits, the encoding, the
/// window of data bytes with its address and the x87 unit.
pub(crate) struct HarnessIn {
    pub(crate) r: [u32; 8],
    pub(crate) eflags: u32,
    pub(crate) code: Vec<u8>,
    pub(crate) window_at: u32,
    pub(crate) window: [u8; WINDOW32],
    pub(crate) x87: X87Image,
}

/// One case out, as `struct case_out` of `x86_32.c` holds it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HarnessOut {
    pub(crate) code: u32,
    pub(crate) eip: u32,
    pub(crate) fault_at: u32,
    pub(crate) eflags: u32,
    pub(crate) r: [u32; 8],
    pub(crate) stray: u32,
    pub(crate) window: [u8; WINDOW32],
    pub(crate) x87: X87Image,
}

pub(crate) fn pack_in32(blob: &mut Vec<u8>, case: &HarnessIn) {
    assert!(case.code.len() <= CODE32_MAX, "an encoding of up to {CODE32_MAX} bytes");
    let start = blob.len();
    for r in case.r {
        blob.extend_from_slice(&r.to_le_bytes());
    }
    blob.extend_from_slice(&case.eflags.to_le_bytes());
    blob.extend_from_slice(&(case.code.len() as u32).to_le_bytes());
    blob.extend_from_slice(&case.window_at.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    blob.extend_from_slice(&case.code);
    blob.resize(start + 128 - WINDOW32, 0);
    blob.extend_from_slice(&case.window);
    case.x87.pack(blob);
    blob.resize(start + IN32_LEN, 0);
}

pub(crate) fn unpack_out32(record: &[u8]) -> HarnessOut {
    assert_eq!(record.len(), OUT32_LEN);
    let word = |i: usize| u32::from_le_bytes(record[4 * i..4 * i + 4].try_into().unwrap());
    let mut r = [0u32; 8];
    for (k, slot) in r.iter_mut().enumerate() {
        *slot = word(4 + k);
    }
    let mut window = [0u8; WINDOW32];
    window.copy_from_slice(&record[56..56 + WINDOW32]);
    let x87 = X87Image::unpack(&record[120..120 + FSAVE_LEN]);
    HarnessOut { code: word(0), eip: word(1), fault_at: word(2), eflags: word(3), r, stray: word(12), window, x87 }
}

/// Builds the harness with clang for i686 into `scratch`, as `harness/build.ps1` does.
pub(crate) fn build_harness32(clang: &Path, scratch: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness/x86_32.c");
    let exe = scratch.join("x86_32.exe");
    let output = Command::new(clang)
        .args(["-target", "i686-pc-windows-msvc", "-O1", "-nostdlib", "-fno-builtin", "-fuse-ld=lld"])
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .args(["-lkernel32", "-Wl,/entry:mainCRTStartup", "-Wl,/subsystem:console"])
        .output()
        .expect("clang runs");
    assert!(output.status.success(), "the harness did not build:\n{}", String::from_utf8_lossy(&output.stderr));
    exe
}

/// Runs the cases of `blob` through the harness under WOW64: how it ended and the records it
/// wrote, which stop short of the cases when a case took the process down.
pub(crate) fn run_harness32(exe: &Path, scratch: &Path, group: &str, blob: &[u8]) -> (ExitStatus, Vec<u8>) {
    let input = scratch.join(format!("{group}.in"));
    let output = scratch.join(format!("{group}.out"));
    fs::write(&input, blob).unwrap();
    let run = Command::new(exe).arg(&input).arg(&output).output().expect("the harness runs");
    (run.status, fs::read(&output).unwrap_or_default())
}

// endregion

// region The forms

/// The seed of the files.
const SEED32: u64 = 0x5EED_0032;
/// The samples a form gets: the 32-bit table has about 440 integer forms (the 64-bit one's
/// without the r64 forms), and sixteen a form puts the files over 6 000 cases where eleven stops
/// near 4 800.
const SAMPLES32: u32 = 16;
/// Where `esp` starts: the middle of the stack, sixteen-aligned, so a stack form's window around
/// it is aligned too.
const STACK32_MID: u32 = STACK32 + 0x2000;
/// The frame `leave` unwinds to.
const FRAME32: u32 = STACK32_MID - 0x40;
/// The string pointers: both inside one window, apart enough for four elements of four bytes
/// either way.
const STRING_WINDOW: u32 = DATA32 + 0x800;
const STRING_SI: u32 = STRING_WINDOW + 0x10;
const STRING_DI: u32 = STRING_WINDOW + 0x28;

const GPR8_32: [R; 8] = [R::AL, R::CL, R::DL, R::BL, R::AH, R::CH, R::DH, R::BH];
const GPR16_32: [R; 7] = [R::AX, R::CX, R::DX, R::BX, R::BP, R::SI, R::DI];
const GPR32_32: [R; 7] = [R::EAX, R::ECX, R::EDX, R::EBX, R::EBP, R::ESI, R::EDI];

/// The mnemonics of `lift_instr` the harness does not run, each with its reason: control flow
/// (a case is one instruction; where a branch lands is the breakpoint fill) and the environment
/// (another answer every run or a host call). The 32-bit-only forms the 64-bit oracle skips run
/// here.
const NOT_RUN32: &[(&str, &str)] = &[
    ("Int", "an interrupt leaves the process (int 0x29 ends it)"),
    ("Syscall", "a system call runs the host's kernel"),
    ("Call", "control flow"),
    ("Ret", "control flow"),
    ("Retf", "control flow"),
    ("Jmp", "control flow"),
    ("Jo", "control flow"),
    ("Jno", "control flow"),
    ("Jb", "control flow"),
    ("Jae", "control flow"),
    ("Je", "control flow"),
    ("Jne", "control flow"),
    ("Jbe", "control flow"),
    ("Ja", "control flow"),
    ("Js", "control flow"),
    ("Jns", "control flow"),
    ("Jp", "control flow"),
    ("Jnp", "control flow"),
    ("Jl", "control flow"),
    ("Jge", "control flow"),
    ("Jle", "control flow"),
    ("Jg", "control flow"),
    ("Jecxz", "control flow"),
    ("Jcxz", "control flow"),
    ("Loop", "control flow"),
    ("Loope", "control flow"),
    ("Loopne", "control flow"),
    ("Cpuid", "the host answers"),
    ("Xgetbv", "the host answers"),
    ("Rdtsc", "another value every run"),
    ("Stmxcsr", "the runtime's own control register"),
    ("Ldmxcsr", "the runtime's own control register"),
];

/// The 32-bit forms the port needs that `lift_instr` has no arm for yet: run all the same, so
/// their vectors are ready and the replay lists them in `PENDING.txt` until the arm lands.
const NOT_LIFTED_YET: &[&str] = &[];

/// The mnemonic groups of the files, by name, as the x86-64 files have them. A form over
/// `esi`/`edi` is a string one, a `setcc` or `cmovcc` a condition; what no group names is `misc`.
const GROUPS32: &[(&str, &[&str])] = &[
    ("arith", &["Add", "Adc", "Sub", "Sbb", "Inc", "Dec", "Neg", "Cmp", "Mul", "Imul", "Div", "Idiv", "Cbw", "Cwde", "Cwd", "Cdq"]),
    ("logic", &["And", "Or", "Xor", "Not", "Test"]),
    ("shift", &["Shl", "Shr", "Sar", "Sal", "Rol", "Ror", "Rcl", "Rcr", "Shld", "Shrd"]),
    ("bits", &["Bt", "Bts", "Btr", "Btc", "Bsf", "Bsr", "Popcnt", "Lzcnt", "Tzcnt", "Bswap"]),
    ("moves", &["Mov", "Movzx", "Movsx", "Lea", "Xchg", "Xadd", "Cmpxchg", "Cmpxchg8b"]),
    ("stack", &["Push", "Pop", "Pushf", "Pushfd", "Pushad", "Popf", "Popfd", "Popad", "Leave"]),
];

/// The mnemonics with an arm in `lift_instr`, read off the source as the x86 oracle reads it.
fn lifted_mnemonics() -> BTreeSet<String> {
    let source = include_str!("../../src/x86.rs");
    let start = source.find("fn lift_instr(").expect("lift_instr");
    let end = source[start..].find("fn widen_to(").expect("the end of lift_instr") + start;
    let body = &source[start..end];
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find("M::") {
        let ident: String = rest[at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !ident.is_empty() {
            out.insert(ident);
        }
        rest = &rest[at + 3..];
    }
    out
}

fn mnemonic_name(m: Mnemonic) -> String {
    format!("{m:?}")
}

/// The operand kinds of the integer forms at 32 bits: the general registers of the three widths,
/// memory, the immediates and the implied registers; no vector or x87 operand, which the harness
/// does not carry.
fn operand_kind_supported32(k: K) -> bool {
    matches!(
        k,
        K::None
            | K::r8_reg
            | K::r8_or_mem
            | K::r8_opcode
            | K::r16_reg
            | K::r16_rm
            | K::r16_or_mem
            | K::r16_opcode
            | K::r32_reg
            | K::r32_rm
            | K::r32_or_mem
            | K::r32_opcode
            | K::mem
            | K::mem_offs
            | K::imm8
            | K::imm8_const_1
            | K::imm8sex16
            | K::imm8sex32
            | K::imm16
            | K::imm32
            | K::al
            | K::cl
            | K::ax
            | K::dx
            | K::eax
            | K::seg_rSI
            | K::es_rDI
    )
}

/// The forms the harness runs: every legacy-encoded code of 32-bit mode whose mnemonic the lift
/// has an arm for (or `NOT_LIFTED_YET` names) and whose operands the generator can produce.
pub(crate) fn forms32() -> Vec<Code> {
    let lifted = lifted_mnemonics();
    let skipped: BTreeSet<&str> = NOT_RUN32.iter().map(|(m, _)| *m).collect();
    Code::values()
        .filter(|&code| {
            let op = code.op_code();
            let name = mnemonic_name(code.mnemonic());
            op.is_instruction()
                && op.encoding() == EncodingKind::Legacy
                && op.is_available_in_mode(32)
                && (lifted.contains(&name) || NOT_LIFTED_YET.contains(&name.as_str()))
                && !skipped.contains(name.as_str())
                && op.op_kinds().iter().all(|&k| operand_kind_supported32(k))
                // bswap of a 16-bit register leaves it undefined.
                && code != Code::Bswap_r16
        })
        .collect()
}

pub(crate) fn group32(code: Code) -> &'static str {
    let kinds = code.op_code().op_kinds();
    if kinds.iter().any(|k| matches!(k, K::seg_rSI | K::es_rDI)) {
        return "string";
    }
    let name = mnemonic_name(code.mnemonic());
    if name.starts_with("Set") || name.starts_with("Cmov") {
        return "conditions";
    }
    GROUPS32.iter().find(|(_, names)| names.contains(&name.as_str())).map_or("misc", |(group, _)| *group)
}

// endregion

// region An instance

/// The registers a sample pins to a value before it runs, by register number.
pub(super) type Pins = Vec<(usize, u32)>;

/// A random instance of `code`: the instruction, and the registers it needs at certain values
/// (the bases of its memory operands, the string registers, a divisor's high half).
fn instance32(code: Code, rng: &mut Rng) -> Option<(Instruction, Pins)> {
    let data_mid = DATA32 + DATA32_LEN / 2;
    let mut ins = Instruction::default();
    ins.set_code(code);
    let mut pins = Pins::new();
    let m = code.mnemonic();
    // A push or pop of a memory operand touches the stack too: the operand lies just above esp,
    // inside the window of the stack forms.
    let on_stack = matches!(m, Mnemonic::Push | Mnemonic::Pop);
    let kinds = code.op_code().op_kinds();
    for (n, &k) in kinds.iter().enumerate() {
        let n = n as u32;
        let reg = |ins: &mut Instruction, r: R| {
            ins.set_op_kind(n, OpKind::Register);
            ins.set_op_register(n, r);
        };
        let rm = |ins: &mut Instruction, rng: &mut Rng, pins: &mut Pins, regs: &[R]| {
            if rng.chance(2) {
                reg(ins, rng.pick(regs));
            } else {
                memory32(ins, n, rng, pins, data_mid, on_stack);
            }
        };
        match k {
            K::None => {}
            K::r8_reg | K::r8_opcode => reg(&mut ins, rng.pick(&GPR8_32)),
            K::r16_reg | K::r16_rm | K::r16_opcode => reg(&mut ins, rng.pick(&GPR16_32)),
            K::r32_reg | K::r32_rm | K::r32_opcode => reg(&mut ins, rng.pick(&GPR32_32)),
            K::r8_or_mem => rm(&mut ins, rng, &mut pins, &GPR8_32),
            K::r16_or_mem => rm(&mut ins, rng, &mut pins, &GPR16_32),
            K::r32_or_mem => rm(&mut ins, rng, &mut pins, &GPR32_32),
            K::mem => memory32(&mut ins, n, rng, &mut pins, data_mid, on_stack),
            K::mem_offs => {
                ins.set_op_kind(n, OpKind::Memory);
                ins.set_memory_base(R::None);
                ins.set_memory_displ_size(4);
                ins.set_memory_displacement64(u64::from(data_mid.wrapping_add_signed(16 * (rng.below(128) as i32 - 64))));
            }
            K::imm8 => {
                ins.set_op_kind(n, OpKind::Immediate8);
                ins.set_immediate8(immediate8_32(code, rng));
            }
            K::imm8_const_1 => {
                ins.set_op_kind(n, OpKind::Immediate8);
                ins.set_immediate8(1);
            }
            K::imm8sex16 => {
                ins.set_op_kind(n, OpKind::Immediate8to16);
                ins.set_immediate8to16(rng.value() as i8 as i16);
            }
            K::imm8sex32 => {
                ins.set_op_kind(n, OpKind::Immediate8to32);
                ins.set_immediate8to32(rng.value() as i8 as i32);
            }
            K::imm16 => {
                ins.set_op_kind(n, OpKind::Immediate16);
                ins.set_immediate16(rng.value() as u16);
            }
            K::imm32 => {
                ins.set_op_kind(n, OpKind::Immediate32);
                ins.set_immediate32(rng.value() as u32);
            }
            K::al => reg(&mut ins, R::AL),
            K::cl => reg(&mut ins, R::CL),
            K::ax => reg(&mut ins, R::AX),
            K::dx => reg(&mut ins, R::DX),
            K::eax => reg(&mut ins, R::EAX),
            K::seg_rSI => ins.set_op_kind(n, OpKind::MemorySegESI),
            K::es_rDI => ins.set_op_kind(n, OpKind::MemoryESEDI),
            _ => return None,
        }
    }
    if kinds.iter().any(|k| matches!(k, K::seg_rSI | K::es_rDI)) {
        // The string registers point into one window, with a count of up to four either way; a
        // repeat half the time, by the mnemonic's own prefix.
        pins.push((ESI, STRING_SI));
        pins.push((EDI, STRING_DI));
        pins.push((ECX, rng.below(5) as u32));
        if rng.chance(2) {
            match m {
                Mnemonic::Cmpsb | Mnemonic::Cmpsw | Mnemonic::Cmpsd | Mnemonic::Scasb | Mnemonic::Scasw | Mnemonic::Scasd => {
                    if rng.chance(2) {
                        ins.set_has_repe_prefix(true);
                    } else {
                        ins.set_has_repne_prefix(true);
                    }
                }
                _ => ins.set_has_rep_prefix(true),
            }
        }
    }
    match m {
        Mnemonic::Leave => pins.push((EBP, FRAME32)),
        // A random high half overflows nearly every quotient: most samples keep it small.
        Mnemonic::Div if !rng.chance(4) => pins.push((EDX, rng.below(4) as u32)),
        Mnemonic::Idiv if !rng.chance(4) => pins.push((EDX, if rng.chance(2) { 0 } else { u32::MAX })),
        // A 16-bit double shift by more than 16 is undefined.
        Mnemonic::Shld | Mnemonic::Shrd if kinds[0] == K::r16_or_mem && kinds[2] == K::cl => {
            pins.push((ECX, (rng.next() as u32 & !0xFF) | rng.below(17) as u32));
        }
        // A bit offset in a register reaches as far as the register says: within 128 bits either
        // way, the byte it names stays in the window.
        Mnemonic::Bt | Mnemonic::Bts | Mnemonic::Btr | Mnemonic::Btc
            if ins.op0_kind() == OpKind::Memory && ins.op1_kind() == OpKind::Register =>
        {
            let offset = rng.below(0x100) as i32 - 0x80;
            pins.push((ins.op1_register().full_register().number(), offset as u32));
        }
        _ => {}
    }
    Some((ins, pins))
}

/// A memory operand `[base + index * scale + disp]` sixteen-aligned inside the data buffer, or
/// just above the stack pointer one time in eight (and always for a push or a pop): never below
/// it, where the dispatcher writes once the case ends.
pub(super) fn memory32(ins: &mut Instruction, n: u32, rng: &mut Rng, pins: &mut Pins, data_mid: u32, on_stack: bool) {
    ins.set_op_kind(n, OpKind::Memory);
    let disp = if on_stack || rng.chance(8) {
        ins.set_memory_base(R::ESP);
        4 * rng.below(7) as i32
    } else {
        let base = rng.pick(&GPR32_32);
        ins.set_memory_base(base);
        pins.push((base.number(), data_mid));
        let mut disp = 16 * (rng.below(128) as i32 - 64);
        if rng.chance(4) {
            let index = rng.pick(&GPR32_32);
            if index != base {
                let scale = [1u32, 2, 4, 8][rng.below(4) as usize];
                ins.set_memory_index(index);
                ins.set_memory_index_scale(scale);
                pins.push((index.number(), (16 / scale) * rng.below(4) as u32));
                // The index reaches forward: keep the whole operand in the buffer.
                disp = disp.min(0x3C0);
            }
        }
        disp
    };
    ins.set_memory_displ_size(if (-128..=127).contains(&disp) { 1 } else { 4 });
    ins.set_memory_displacement64(disp as i64 as u64);
}

/// A byte immediate that means what the instruction can use: a 16-bit double shift's count is
/// at most 16, a shift count is whatever.
fn immediate8_32(code: Code, rng: &mut Rng) -> u8 {
    match code.mnemonic() {
        Mnemonic::Shld | Mnemonic::Shrd if code.op_code().op_kinds()[0] == K::r16_or_mem => rng.below(17) as u8,
        _ => rng.value() as u8,
    }
}

pub(super) fn encode32(ins: &Instruction) -> Result<Vec<u8>, String> {
    let mut encoder = Encoder::new(32);
    encoder.encode(ins, u64::from(CASE_IP32)).map_err(|e| e.to_string())?;
    Ok(encoder.take_buffer())
}

pub(crate) fn format32(ins: &Instruction) -> String {
    let mut out = String::new();
    IntelFormatter::new().format(ins, &mut out);
    out
}

// endregion

// region A state

/// A machine state of the harness: the registers, the flags one byte each in `FLAG_NAMES`
/// order, and the window of data bytes with its address.
pub(super) struct Picture32 {
    pub(super) r: [u32; 8],
    pub(super) flags: [u8; 7],
    pub(super) window_at: u32,
    pub(super) window: [u8; WINDOW32],
}

impl Picture32 {
    pub(super) fn flags_word(&self) -> u32 {
        FLAG_BITS.iter().zip(self.flags).fold(0u32, |w, (&bit, f)| w | u32::from(f) << bit)
    }
}

/// The address a memory operand names in the state.
pub(super) fn effective_address32(ins: &Instruction, r: &[u32; 8]) -> Option<u32> {
    (0..ins.op_count()).find(|&n| ins.op_kind(n) == OpKind::Memory)?;
    let mut at = ins.memory_displacement32();
    if ins.memory_base() != R::None {
        at = at.wrapping_add(r[ins.memory_base().number()]);
    }
    if ins.memory_index() != R::None {
        at = at.wrapping_add(r[ins.memory_index().number()].wrapping_mul(ins.memory_index_scale()));
    }
    Some(at)
}

/// The byte ranges the instruction can touch from the state: its memory operand (a bit test's
/// register offset followed to the bytes it can reach), the string operands over their count and
/// direction, the stack around `esp` for the forms that push or pop, the frame at `ebp` for
/// `leave`. The window of the case is placed over them.
fn touched32(ins: &Instruction, r: &[u32; 8], backwards: bool) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let width = ins.memory_size().size() as u32;
    if let Some(at) = effective_address32(ins, r) {
        let bit_test = matches!(ins.mnemonic(), Mnemonic::Bt | Mnemonic::Bts | Mnemonic::Btr | Mnemonic::Btc);
        if bit_test && ins.op1_kind() == OpKind::Register {
            out.push((at.wrapping_sub(16), at.wrapping_add(20)));
        } else {
            out.push((at, at.wrapping_add(width.max(1))));
        }
    }
    let kinds = ins.code().op_code().op_kinds();
    let rep = ins.has_rep_prefix() || ins.has_repe_prefix() || ins.has_repne_prefix();
    let count = if rep { r[ECX] } else { 1 };
    for (kind, reg) in [(K::seg_rSI, ESI), (K::es_rDI, EDI)] {
        if kinds.contains(&kind) && count != 0 {
            let p = r[reg];
            let total = count.wrapping_mul(width);
            // Backwards, the last element is `total - width` below the pointer.
            if backwards {
                out.push((p.wrapping_add(width).wrapping_sub(total), p.wrapping_add(width)));
            } else {
                out.push((p, p.wrapping_add(total)));
            }
        }
    }
    match ins.mnemonic() {
        Mnemonic::Push
        | Mnemonic::Pop
        | Mnemonic::Pushf
        | Mnemonic::Pushfd
        | Mnemonic::Pushad
        | Mnemonic::Popf
        | Mnemonic::Popfd
        | Mnemonic::Popad => out.push((r[ESP].wrapping_sub(32), r[ESP].wrapping_add(32))),
        Mnemonic::Leave => out.push((r[EBP], r[EBP].wrapping_add(16))),
        _ => {}
    }
    out
}

/// The sixteen-aligned window that holds every range, inside the arena; `None` when they do not
/// fit one, and the data buffer's start for a form that touches no memory.
fn window_for(ranges: &[(u32, u32)]) -> Option<u32> {
    let lo = ranges.iter().map(|r| r.0).min().unwrap_or(DATA32);
    let hi = ranges.iter().map(|r| r.1).max().unwrap_or(DATA32);
    let at = lo & !15;
    let end = at.checked_add(WINDOW32 as u32)?;
    (at >= ARENA32 && lo <= hi && hi <= end && end <= ARENA32 + ARENA32_LEN).then_some(at)
}

/// A state for `ins`: every register random but for the pins and `esp`, the flags random, the
/// window of random bytes over what the instruction touches. `None` when the operands do not fit
/// one window.
pub(super) fn picture32(rng: &mut Rng, ins: &Instruction, pins: &Pins) -> Option<Picture32> {
    let mut r = [0u32; 8];
    for v in &mut r {
        *v = rng.value32();
    }
    r[ESP] = STACK32_MID;
    for &(n, v) in pins {
        r[n] = v;
    }
    let mut flags = [0u8; 7];
    for f in &mut flags {
        *f = u8::from(rng.chance(2));
    }
    let window_at = window_for(&touched32(ins, &r, flags[5] == 1))?;
    let mut window = [0u8; WINDOW32];
    rng.fill(&mut window);
    // What a pop of the flags loads: nothing that traps (no TF) or the harness cannot restore.
    if matches!(ins.mnemonic(), Mnemonic::Popf | Mnemonic::Popfd) {
        let safe = (rng.next() as u32 & 0xCD5) | 2;
        let at = (r[ESP] - window_at) as usize;
        window[at..at + 4].copy_from_slice(&safe.to_le_bytes());
    }
    Some(Picture32 { r, flags, window_at, window })
}

/// One sample of a form: the instruction, its encoding and the state it runs over.
struct Sample {
    ins: Instruction,
    bytes: Vec<u8>,
    picture: Picture32,
}

impl Sample {
    fn record(&self) -> HarnessIn {
        HarnessIn {
            r: self.picture.r,
            eflags: self.picture.flags_word(),
            code: self.bytes.clone(),
            window_at: self.picture.window_at,
            window: self.picture.window,
            x87: X87Image::init(),
        }
    }
}

fn sample32(code: Code, rng: &mut Rng) -> Option<Sample> {
    let (ins, pins) = instance32(code, rng)?;
    let bytes = encode32(&ins).ok()?;
    let picture = picture32(rng, &ins, &pins)?;
    Some(Sample { ins, bytes, picture })
}

// endregion

// region A case

/// The name a fault takes in the files.
fn fault_kind32(code: u32) -> String {
    match code {
        STATUS_ACCESS_VIOLATION => "access".to_owned(),
        0xC000_001D => "illegal".to_owned(),
        0xC000_0094 => "divide".to_owned(),
        0xC000_0096 => "privileged".to_owned(),
        STATUS_BREAKPOINT => "breakpoint".to_owned(),
        0x8000_0004 => "step".to_owned(),
        _ => format!("exception_{code:x}"),
    }
}

/// The flags the manual leaves undefined for one run of `ins`: iced's answer, and OF of a rotate
/// through carry whenever the masked count is above 1, as the x86-64 oracle found the processor
/// to write it.
fn undefined_flags32(ins: &Instruction, input: &Picture32) -> u32 {
    let mut undefined = ins.rflags_undefined();
    if matches!(ins.mnemonic(), Mnemonic::Rcl | Mnemonic::Rcr) {
        let width = if ins.op0_kind() == OpKind::Register { ins.op0_register().size() } else { ins.memory_size().size() };
        let count = match ins.op_count() {
            0 | 1 => 1,
            _ if ins.op1_kind() == OpKind::Register => u64::from(input.r[ECX] & 0xff),
            _ => ins.immediate(1),
        };
        let mask = if width == 8 { 0x3f } else { 0x1f };
        if count & mask > 1 {
            undefined |= RflagsBits::OF;
        }
    }
    undefined
}

/// The registers and the window of a state as the files name them: the general registers, one
/// byte a flag, the window as four sixteen-byte cells.
fn state32(p: &Picture32) -> State {
    let mut state = State::default();
    for (name, v) in GPR_NAMES32.iter().zip(p.r) {
        state.registers.insert((*name).to_owned(), v.to_be_bytes().to_vec());
    }
    for (name, f) in FLAG_NAMES.iter().zip(p.flags) {
        state.registers.insert((*name).to_owned(), vec![f]);
    }
    for (i, cell) in p.window.chunks_exact(16).enumerate() {
        state.memory.insert(u64::from(p.window_at) + 16 * i as u64, cell.to_vec());
    }
    state
}

/// One case from the harness's run of `ins` over `input`: the registers, the flags and the window
/// in; what changed out, the flags the manual leaves undefined left out of it and named in
/// `undefined`, the EFLAGS mask of those flags; or the fault. A case that ended anywhere but at
/// the breakpoint after its own bytes is a fault.
pub(super) fn vector_case32(ins: &Instruction, bytes: Vec<u8>, input: &Picture32, out: &HarnessOut) -> Case {
    let before = state32(input);
    let outcome = if out.code == STATUS_BREAKPOINT && out.eip == CASE_IP32 + bytes.len() as u32 {
        assert!(
            out.stray == 0,
            "`{}` ({}) stored outside its window at {:#x}: the generator's model of what the form touches is wrong",
            format32(ins),
            vectors::hex(&bytes),
            out.stray.wrapping_sub(1)
        );
        let mut flags = [0u8; 7];
        for (f, &bit) in flags.iter_mut().zip(&FLAG_BITS) {
            *f = ((out.eflags >> bit) & 1) as u8;
        }
        // The dispatcher's frames lie below the final esp: those bytes of a stack window are not
        // the case's, and a single instruction stores nothing below where it leaves esp.
        let mut window = out.window;
        for (i, byte) in window.iter_mut().enumerate() {
            let at = input.window_at + i as u32;
            if (STACK32..STACK32_END).contains(&at) && at < out.r[ESP] {
                *byte = input.window[i];
            }
        }
        let mut after = state32(&Picture32 { r: out.r, flags, window_at: input.window_at, window });
        let undefined = undefined_flags32(ins, input);
        let mut mask = 0u16;
        for (i, name) in FLAG_NAMES.iter().enumerate() {
            if undefined & FLAG_RFLAGS[i] != 0 {
                after.registers.remove(*name);
                mask |= 1 << FLAG_BITS[i];
            }
        }
        if mask != 0 {
            after.registers.insert("undefined".to_owned(), mask.to_be_bytes().to_vec());
        }
        Outcome::State(vectors::changed(&before, &after))
    } else {
        Outcome::Fault(fault_kind32(out.code))
    };
    Case { bytes, input: before, outcome }
}

// endregion

/// The directory of the 32-bit x86 vectors.
pub(crate) fn x86_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/x86")
}

/// Writes `<dir>/<group>.vec` for every group: every form over `SAMPLES32` states at the fixed
/// seed, run by the harness built with `clang`; the cases written, by group.
pub(crate) fn write_vectors(clang: &Path, dir: &Path) -> BTreeMap<&'static str, usize> {
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "x86_32_vectors");
    let exe = build_harness32(clang, &scratch);
    let mut rng = Rng::new(SEED32);
    let mut groups: BTreeMap<&'static str, Vec<Sample>> = BTreeMap::new();
    for code in forms32() {
        for _ in 0..SAMPLES32 {
            let Some(sample) = (0..8).find_map(|_| sample32(code, &mut rng)) else { break };
            groups.entry(group32(code)).or_default().push(sample);
        }
    }
    fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut counts = BTreeMap::new();
    for (group, samples) in &groups {
        let mut blob = Vec::new();
        for sample in samples {
            pack_in32(&mut blob, &sample.record());
        }
        let (status, results) = run_harness32(&exe, &scratch, group, &blob);
        let answered = results.len() / OUT32_LEN;
        assert!(
            status.success() && answered == samples.len(),
            "{group}: the harness answered {answered} of {} cases and ended with {status}; the next case was {}",
            samples.len(),
            samples.get(answered).map_or_else(|| "none".to_owned(), |s| format!("`{}` ({})", format32(&s.ins), vectors::hex(&s.bytes)))
        );
        let cases: Vec<Case> = samples
            .iter()
            .zip(results.chunks_exact(OUT32_LEN))
            .map(|(sample, record)| vector_case32(&sample.ins, sample.bytes.clone(), &sample.picture, &unpack_out32(record)))
            .collect();
        let text = format!(
            "# 32-bit x86 vectors of the {group} group: the processor's answer under WOW64 to every integer\n\
             # form of the group over {SAMPLES32} states at seed {SEED32:#x}, written by the harness of\n\
             # testdata/vectors/harness/x86_32.c through crates/ra-asm-lift/tests/common/x86_32.rs. In: the\n\
             # general registers, the flags cf pf af zf sf df of one byte each, the sixteen-byte cells of the\n\
             # case's window; the arena is at {ARENA32:#x}. Out: what changed; a flag the manual leaves\n\
             # undefined is left out and `undefined` holds the EFLAGS mask of those flags.\n{}",
            vectors::write(&cases)
        );
        let path = dir.join(format!("{group}.vec"));
        fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        counts.insert(*group, cases.len());
    }
    counts
}
