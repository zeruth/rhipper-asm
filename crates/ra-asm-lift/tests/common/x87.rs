//! The x87 generator: iced's code table at 32 bits kept to the x87 forms, each given random
//! operands over a random register stack, control word and status word, and run on this machine's
//! own processor under WOW64 by the harness of `x86_32.rs`, whose records carry the unit as an
//! fsave image. The answers are the vector files of `testdata/vectors/x87`, one a group, in the
//! format of `ra_asm_testing::vectors`; `vectors/x87.rs` replays them through the exact unit of
//! the lift. The transcendental forms, which the lift has no arm for, are generated all the same:
//! their vectors are the processor's, and they wait in `PENDING.txt` for the arm.
//!
//! A state names `fcw`, `fsw` (the top of stack in bits 11 to 13), `ftw` (two bits a physical
//! register, 11 for empty) and the physical registers `fr0` to `fr7` as ten-byte values, sign and
//! exponent first. A form's stack is deep enough for what it reads and leaves room for what it
//! pushes, so no case overflows or underflows the stack; the exceptions are masked, the rounding
//! and the precision control random, and the values finite normals: an integer or an integer and
//! a half half the time for the rounding forms, positive where a root or a logarithm needs it,
//! inside the unit interval for `f2xm1` and `fyl2xp1`, and a zero now and then for the forms that
//! classify or move a value. A memory operand gets a value of its kind written at its address, and
//! `fldcw` a control word the unit keeps as it is.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ra_asm_testing::vectors::{self, Case, Outcome, State, Value};
use ra_asm_x86::iced_x86::{Code, EncodingKind, Instruction, MemorySize, Mnemonic, OpCodeOperandKind as K, OpKind, Register as R};

use super::Rng;
use super::x86_32::{
    DATA32, DATA32_LEN, HarnessIn, HarnessOut, OUT32_LEN, Picture32, Pins, X87Image, build_harness32, effective_address32, encode32,
    format32, memory32, pack_in32, picture32, run_harness32, unpack_out32, vector_case32,
};

// region The forms

/// The seed of the files.
const SEED87: u64 = 0x5EED_0087;
/// The samples a form gets: about 135 forms, and twenty a form puts the files near 2 700 cases.
const SAMPLES87: u32 = 20;

const ST: [R; 8] = [R::ST0, R::ST1, R::ST2, R::ST3, R::ST4, R::ST5, R::ST6, R::ST7];

/// The forms `lift_instr` has no arm for: run for the record, listed in `PENDING.txt` until the
/// arm lands.
const TRANSCENDENTAL: &[&str] =
    &["F2xm1", "Fyl2x", "Fptan", "Fpatan", "Fxtract", "Fprem1", "Fprem", "Fyl2xp1", "Fsincos", "Fscale", "Fsin", "Fcos"];

/// The mnemonic groups of the files, by name; every mnemonic of `x87::is_x87` is in one, and a
/// mnemonic in none is not a form of these files.
const GROUPS87: &[(&str, &[&str])] = &[
    ("load", &["Fld", "Fld1", "Fldz", "Fldpi", "Fldl2e", "Fldln2", "Fldlg2", "Fldl2t", "Fild"]),
    ("store", &["Fst", "Fstp", "Fist", "Fistp", "Fisttp"]),
    (
        "arith",
        &[
            "Fadd", "Faddp", "Fiadd", "Fsub", "Fsubp", "Fisub", "Fsubr", "Fsubrp", "Fisubr", "Fmul", "Fmulp", "Fimul", "Fdiv", "Fdivp",
            "Fidiv", "Fdivr", "Fdivrp", "Fidivr", "Fchs", "Fabs", "Fsqrt", "Frndint",
        ],
    ),
    (
        "compare",
        &[
            "Fcom", "Fcomp", "Fcompp", "Fucom", "Fucomp", "Fucompp", "Ficom", "Ficomp", "Fcomi", "Fcomip", "Fucomi", "Fucomip", "Ftst",
            "Fxam", "Fcmovb", "Fcmove", "Fcmovbe", "Fcmovu", "Fcmovnb", "Fcmovne", "Fcmovnbe", "Fcmovnu",
        ],
    ),
    (
        "control",
        &[
            "Fxch", "Fnstsw", "Fstsw", "Fnstcw", "Fstcw", "Fldcw", "Fninit", "Finit", "Fnclex", "Fclex", "Ffree", "Ffreep", "Fincstp",
            "Fdecstp", "Fnop",
        ],
    ),
    ("transcendental", TRANSCENDENTAL),
];

fn mnemonic_name(m: Mnemonic) -> String {
    format!("{m:?}")
}

pub(crate) fn group87(code: Code) -> Option<&'static str> {
    let name = mnemonic_name(code.mnemonic());
    GROUPS87.iter().find(|(_, names)| names.contains(&name.as_str())).map(|(group, _)| *group)
}

/// iced names the undocumented aliases of a register form (`DC D0+i` for `fcom st(i)`, `DF D0+i`
/// for `fstp st(i)`) by their first bytes: the same arm lifts them and the form above them runs.
fn is_alias(name: &str) -> bool {
    name.rsplit_once('_')
        .is_some_and(|(_, suffix)| suffix.len() == 4 && suffix.starts_with('D') && suffix.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The operand kinds of the x87 forms: the register stack, memory of any width, `ax` for the
/// status word.
fn operand_kind_supported87(k: K) -> bool {
    matches!(k, K::None | K::st0 | K::sti_opcode | K::mem | K::ax)
}

/// The forms the harness runs: every legacy-encoded x87 code of 32-bit mode whose mnemonic has a
/// group, the undocumented aliases left out.
pub(crate) fn forms87() -> Vec<Code> {
    Code::values()
        .filter(|&code| {
            let op = code.op_code();
            let name = format!("{code:?}");
            let grouped = group87(code).is_some();
            let m = code.mnemonic();
            assert!(
                grouped || !ra_asm_lift::x87::is_x87(m) || !op.op_kinds().iter().all(|&k| operand_kind_supported87(k)),
                "{m:?} has an arm in x87.rs and no group in GROUPS87"
            );
            op.is_instruction()
                && op.encoding() == EncodingKind::Legacy
                && op.is_available_in_mode(32)
                && grouped
                && !is_alias(&name)
                && op.op_kinds().iter().all(|&k| operand_kind_supported87(k))
        })
        .collect()
}

// endregion

// region The register stack a form needs

/// Whether the form pushes a value: the new `st(0)` must be empty.
fn pushes(m: Mnemonic) -> bool {
    matches!(
        m,
        Mnemonic::Fld
            | Mnemonic::Fld1
            | Mnemonic::Fldz
            | Mnemonic::Fldpi
            | Mnemonic::Fldl2e
            | Mnemonic::Fldln2
            | Mnemonic::Fldlg2
            | Mnemonic::Fldl2t
            | Mnemonic::Fild
            | Mnemonic::Fsincos
            | Mnemonic::Fptan
            | Mnemonic::Fxtract
    )
}

/// How many entries of the stack the form reads, `st(i)` included: none for the control forms
/// and the loads from memory, two for the forms over `st(0)` and `st(1)`, else one.
fn reads(m: Mnemonic, sti: Option<usize>) -> usize {
    let base = match m {
        Mnemonic::Fnstsw
        | Mnemonic::Fstsw
        | Mnemonic::Fnstcw
        | Mnemonic::Fstcw
        | Mnemonic::Fldcw
        | Mnemonic::Fninit
        | Mnemonic::Finit
        | Mnemonic::Fnclex
        | Mnemonic::Fclex
        | Mnemonic::Fnop
        | Mnemonic::Fincstp
        | Mnemonic::Fdecstp
        | Mnemonic::Ffree
        | Mnemonic::Ffreep => return 0,
        Mnemonic::Fcompp
        | Mnemonic::Fucompp
        | Mnemonic::Fpatan
        | Mnemonic::Fyl2x
        | Mnemonic::Fyl2xp1
        | Mnemonic::Fscale
        | Mnemonic::Fprem
        | Mnemonic::Fprem1 => 2,
        _ if pushes(m) => 0,
        _ => 1,
    };
    base.max(sti.map_or(0, |i| i + 1))
}

/// The shape of the values a form's stack holds.
#[derive(Clone, Copy)]
struct Shape {
    /// No negative value: a root, a logarithm.
    positive: bool,
    /// A zero one time in eight: the forms that classify, compare or move a value.
    zero_ok: bool,
    /// An integer or an integer and a half, half the time: the rounding forms.
    integral: bool,
    /// Inside the unit interval: `f2xm1` and `fyl2xp1` take `st(0)` there.
    small: bool,
}

fn shape_of(m: Mnemonic) -> Shape {
    let integral = matches!(m, Mnemonic::Fist | Mnemonic::Fistp | Mnemonic::Fisttp | Mnemonic::Frndint | Mnemonic::Fscale);
    let positive = matches!(m, Mnemonic::Fsqrt | Mnemonic::Fyl2x);
    let small = matches!(m, Mnemonic::F2xm1 | Mnemonic::Fyl2xp1);
    let zero_ok = matches!(
        m,
        Mnemonic::Fld
            | Mnemonic::Fst
            | Mnemonic::Fstp
            | Mnemonic::Fist
            | Mnemonic::Fistp
            | Mnemonic::Fisttp
            | Mnemonic::Fxch
            | Mnemonic::Fchs
            | Mnemonic::Fabs
            | Mnemonic::Frndint
            | Mnemonic::Fcom
            | Mnemonic::Fcomp
            | Mnemonic::Fcompp
            | Mnemonic::Fucom
            | Mnemonic::Fucomp
            | Mnemonic::Fucompp
            | Mnemonic::Ficom
            | Mnemonic::Ficomp
            | Mnemonic::Fcomi
            | Mnemonic::Fcomip
            | Mnemonic::Fucomi
            | Mnemonic::Fucomip
            | Mnemonic::Ftst
            | Mnemonic::Fxam
            | Mnemonic::Fcmovb
            | Mnemonic::Fcmove
            | Mnemonic::Fcmovbe
            | Mnemonic::Fcmovu
            | Mnemonic::Fcmovnb
            | Mnemonic::Fcmovne
            | Mnemonic::Fcmovnbe
            | Mnemonic::Fcmovnu
    );
    Shape { positive, zero_ok, integral, small }
}

/// An 80-bit value of the shape as its significand and its sign-and-exponent word: a finite
/// normal with a random 64-bit significand (exact in a double one time in four) within thirty
/// binades of one, or what the shape asks instead.
fn f80_bits(rng: &mut Rng, shape: Shape) -> (u64, u16) {
    if shape.zero_ok && rng.chance(8) {
        return (0, if rng.chance(2) { 0x8000 } else { 0 });
    }
    let sign = if shape.positive { 0 } else { (rng.below(2) as u16) << 15 };
    if shape.integral && rng.chance(2) {
        // n halves, n below 2^16: a 16-bit store fits most of them.
        let n = rng.below(1 << 16);
        if n == 0 {
            return (0, sign);
        }
        let shift = n.leading_zeros();
        return (n << shift, sign | (0x3ffe + 63 - shift) as u16);
    }
    let exponent: u64 = if shape.small { 0x3ffe - rng.below(20) } else { 0x3fff - 30 + rng.below(61) };
    let mut significand = rng.next() | 1 << 63;
    if rng.chance(4) {
        significand &= !0x7ff;
    }
    (significand, sign | exponent as u16)
}

/// The ten bytes of a value as the registers and `tbyte` memory hold them, the significand first.
fn tbyte(significand: u64, sign_exponent: u16) -> [u8; 10] {
    let mut out = [0u8; 10];
    out[..8].copy_from_slice(&significand.to_le_bytes());
    out[8..].copy_from_slice(&sign_exponent.to_le_bytes());
    out
}

/// The tag of a register holding the value: valid, or zero.
fn tag_of(significand: u64) -> u16 {
    u16::from(significand == 0)
}

/// The unit's state as the files name it: the words and the physical registers.
struct X87State {
    cw: u16,
    sw: u16,
    tw: u16,
    fr: [[u8; 10]; 8],
}

impl X87State {
    fn top(&self) -> usize {
        usize::from((self.sw >> 11) & 7)
    }

    /// The fsave image: the registers in stack order from the top.
    fn image(&self) -> X87Image {
        let mut st = [[0u8; 10]; 8];
        for (i, reg) in st.iter_mut().enumerate() {
            *reg = self.fr[(self.top() + i) & 7];
        }
        X87Image { cw: self.cw, sw: self.sw, tw: self.tw, st }
    }

    fn from_image(image: &X87Image) -> Self {
        let mut state = Self { cw: image.cw, sw: image.sw, tw: image.tw, fr: [[0; 10]; 8] };
        let top = state.top();
        for (i, reg) in image.st.iter().enumerate() {
            state.fr[(top + i) & 7] = *reg;
        }
        state
    }

    /// The registers of the files: the words as two bytes, a register as its ten bytes with the
    /// sign and exponent first.
    fn registers(&self) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for (name, word) in [("fcw", self.cw), ("fsw", self.sw), ("ftw", self.tw)] {
            out.insert(name.to_owned(), word.to_be_bytes().to_vec());
        }
        for (i, reg) in self.fr.iter().enumerate() {
            out.insert(format!("fr{i}"), reg.iter().rev().copied().collect());
        }
        out
    }
}

/// A random stack for the form: a random top, `depth` entries from it holding values of the
/// shape, the rest empty; the exceptions masked, the rounding mode and the precision control
/// random, the condition codes random.
fn x87_state(rng: &mut Rng, m: Mnemonic, sti: Option<usize>) -> X87State {
    let shape = shape_of(m);
    let lo = reads(m, sti);
    let hi = if pushes(m) { 7 } else { 8 };
    let depth = lo + rng.below((hi - lo + 1) as u64) as usize;
    let top = rng.below(8) as usize;
    let mut fr = [[0u8; 10]; 8];
    let mut tw = 0xffffu16;
    for i in 0..depth {
        let (significand, sign_exponent) = f80_bits(rng, shape);
        let physical = (top + i) & 7;
        fr[physical] = tbyte(significand, sign_exponent);
        tw = (tw & !(3 << (2 * physical))) | (tag_of(significand) << (2 * physical));
    }
    // Bit 6 of the control word reads as one; precision control 01 is reserved.
    let precision = [0u16, 2, 3][rng.below(3) as usize];
    let cw = 0x7f | precision << 8 | (rng.below(4) as u16) << 10;
    let sw = ((top as u16) << 11) | (rng.next() as u16 & 0x4700);
    X87State { cw, sw, tw, fr }
}

// endregion

// region An instance

/// A random instance of `code`: the instruction, the registers its memory operand pins and the
/// index of the `st(i)` it names.
fn instance87(code: Code, rng: &mut Rng) -> Option<(Instruction, Pins, Option<usize>)> {
    let mut ins = Instruction::default();
    ins.set_code(code);
    let mut pins = Pins::new();
    let mut sti = None;
    // A pushing form needs an empty register, so its `st(i)` leaves one.
    let registers = if pushes(code.mnemonic()) { 7 } else { 8 };
    for (n, &k) in code.op_code().op_kinds().iter().enumerate() {
        let n = n as u32;
        let reg = |ins: &mut Instruction, r: R| {
            ins.set_op_kind(n, OpKind::Register);
            ins.set_op_register(n, r);
        };
        match k {
            K::None => {}
            K::st0 => reg(&mut ins, R::ST0),
            K::sti_opcode => {
                let i = rng.below(registers) as usize;
                sti = Some(i);
                reg(&mut ins, ST[i]);
            }
            K::mem => memory32(&mut ins, n, rng, &mut pins, DATA32 + DATA32_LEN / 2, false),
            K::ax => reg(&mut ins, R::AX),
            _ => return None,
        }
    }
    Some((ins, pins, sti))
}

/// Writes a value of the memory operand's kind at its address in the window: a float or a double
/// of the shape, a `tbyte`, an integer of the width, or for a word the control word `fldcw` loads,
/// with its reserved bits as the unit keeps them.
fn fill_operand(rng: &mut Rng, ins: &Instruction, picture: &mut Picture32, shape: Shape) {
    let Some(at) = effective_address32(ins, &picture.r) else { return };
    let bytes: Vec<u8> = match ins.memory_size() {
        MemorySize::Float32 => (rng.f32_bits(shape.positive) as u32).to_le_bytes().to_vec(),
        MemorySize::Float64 => rng.f64_bits(shape.positive).to_le_bytes().to_vec(),
        MemorySize::Float80 => {
            let (significand, sign_exponent) = f80_bits(rng, shape);
            tbyte(significand, sign_exponent).to_vec()
        }
        MemorySize::Int16 => (rng.value() as u16).to_le_bytes().to_vec(),
        MemorySize::Int32 => (rng.value() as u32).to_le_bytes().to_vec(),
        MemorySize::Int64 => rng.value().to_le_bytes().to_vec(),
        MemorySize::UInt16 => {
            let mut cw = (rng.next() as u16 & 0x1f3f) | 0x40;
            if cw & 0x300 == 0x100 {
                cw |= 0x200;
            }
            cw.to_le_bytes().to_vec()
        }
        _ => return,
    };
    let start = (at - picture.window_at) as usize;
    picture.window[start..start + bytes.len()].copy_from_slice(&bytes);
}

/// One sample of a form: the instruction, its encoding, the integer state and the unit's.
struct Sample87 {
    ins: Instruction,
    bytes: Vec<u8>,
    picture: Picture32,
    x87: X87State,
}

impl Sample87 {
    fn record(&self) -> HarnessIn {
        HarnessIn {
            r: self.picture.r,
            eflags: self.picture.flags_word(),
            code: self.bytes.clone(),
            window_at: self.picture.window_at,
            window: self.picture.window,
            x87: self.x87.image(),
        }
    }
}

fn sample87(code: Code, rng: &mut Rng) -> Option<Sample87> {
    let (ins, pins, sti) = instance87(code, rng)?;
    let bytes = encode32(&ins).ok()?;
    let mut picture = picture32(rng, &ins, &pins)?;
    let m = code.mnemonic();
    fill_operand(rng, &ins, &mut picture, shape_of(m));
    let x87 = x87_state(rng, m, sti);
    Some(Sample87 { ins, bytes, picture, x87 })
}

// endregion

// region A case

/// The case of a sample from the harness's answer: the integer case of `x86_32.rs` with the
/// unit's words and registers in, and what changed of them out.
fn vector_case87(sample: &Sample87, out: &HarnessOut) -> Case {
    let mut case = vector_case32(&sample.ins, sample.bytes.clone(), &sample.picture, out);
    let before = State { registers: sample.x87.registers(), ..State::default() };
    if let Outcome::State(output) = &mut case.outcome {
        let after = State { registers: X87State::from_image(&out.x87).registers(), ..State::default() };
        output.registers.extend(vectors::changed(&before, &after).registers);
    }
    case.input.registers.extend(before.registers);
    case
}

// endregion

/// The directory of the x87 vectors.
pub(crate) fn x87_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/x87")
}

/// Writes `<dir>/<group>.vec` for every group: every form over `SAMPLES87` states at the fixed
/// seed, run by the harness built with `clang`; the cases written, by group.
pub(crate) fn write_vectors(clang: &Path, dir: &Path) -> BTreeMap<&'static str, usize> {
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "x87_vectors");
    let exe = build_harness32(clang, &scratch);
    let mut rng = Rng::new(SEED87);
    let mut groups: BTreeMap<&'static str, Vec<Sample87>> = BTreeMap::new();
    for code in forms87() {
        let group = group87(code).expect("a form has a group");
        for _ in 0..SAMPLES87 {
            let Some(sample) = (0..8).find_map(|_| sample87(code, &mut rng)) else { break };
            groups.entry(group).or_default().push(sample);
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
            .map(|(sample, record)| vector_case87(sample, &unpack_out32(record)))
            .collect();
        let text = format!(
            "# x87 vectors of the {group} group: the processor's answer under WOW64 to every x87 form of the\n\
             # group over {SAMPLES87} states at seed {SEED87:#x}, written by the harness of\n\
             # testdata/vectors/harness/x86_32.c through crates/ra-asm-lift/tests/common/x87.rs. In: the\n\
             # general registers, the flags one byte each, the sixteen-byte cells of the case's window (the\n\
             # arena is at 0x20000000), the unit's control word fcw, status word fsw (the top of stack in\n\
             # bits 11 to 13), tag word ftw (two bits a physical register, 11 empty) and the physical\n\
             # registers fr0 to fr7 as ten bytes, sign and exponent first. Out: what changed.\n{}",
            vectors::write(&cases)
        );
        let path = dir.join(format!("{group}.vec"));
        fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        counts.insert(*group, cases.len());
    }
    counts
}
