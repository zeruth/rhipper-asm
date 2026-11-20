//! The R5900 lift, one group of the multimedia instructions at a time: a function of the ps2sdk
//! fixture is overwritten with a program that loads two 128-bit operands with `lq`, runs each
//! instruction of the group, stores its result with `sq` and writes the result's sixteen bytes
//! to the EE's serial port. The interpreter runs it over the mapped image, and the bytes it
//! prints are compared with the values the manual's definition of each instruction gives.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The function the programs overwrite: `_svfprintf_r`, whose 14 KB the tests' code fits in and
/// which nothing the run reaches calls.
const AT: u64 = 0x10_6ff0;
/// `.text` starts the file at this offset, at `0x100000`.
const TEXT_OFFSET: u64 = 0x80;
const TEXT: u64 = 0x10_0000;

/// The two operands: `rs` is `$9`, `rt` is `$10`, and each result goes to `$11`.
const A: u128 = 0xffff_fffe_0000_0010_8000_0005_7fff_fff0;
const B: u128 = 0x0000_0003_1234_5678_ffff_fff0_0000_0020;
const RS: u32 = 9;
const RT: u32 = 10;
const RD: u32 = 11;

fn r(op: u32, rs: u32, rt: u32, rd: u32, sa: u32, funct: u32) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | (rd << 11) | (sa << 6) | funct
}

fn i(op: u32, rs: u32, rt: u32, imm: u16) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | u32::from(imm)
}

/// An instruction of an MMI group: `rd = rs op rt`.
fn mmi(funct: u32, sa: u32) -> u32 {
    r(0x1c, RS, RT, RD, sa, funct)
}

/// `por $11, $0, $0`: the result register cleared, upper half and all.
fn clear() -> u32 {
    r(0x1c, 0, 0, RD, 18, 0x29)
}

/// The program: the operands from the data after the code, each case's words, then its result's
/// bytes, low first, to `SIO_TXFIFO`; `v0` is zero at the return.
fn program(cases: &[(&str, Vec<u32>, u128)]) -> Vec<u32> {
    let mut code = Vec::new();
    let mut body = vec![i(0x09, 29, 29, 0xffe0), i(0x1e, 8, RS, 0), i(0x1e, 8, RT, 16), i(0x0f, 0, 12, 0x1000), i(0x0d, 12, 12, 0xf180)];
    for (_, words, _) in cases {
        body.extend(words);
        body.push(i(0x1f, 29, RD, 0));
        for k in 0..16 {
            body.push(i(0x24, 29, 13, k));
            body.push(i(0x28, 12, 13, 0));
        }
    }
    body.extend([i(0x09, 29, 29, 32), 0x03e0_0008, r(0, 0, 0, 2, 0, 0x21)]);
    // The data's address takes two words ahead of the body, and sits on the next 16 bytes.
    let data = (AT + 4 * (body.len() as u64 + 2)).next_multiple_of(16);
    code.push(i(0x0f, 0, 8, (data >> 16) as u16));
    code.push(i(0x0d, 8, 8, data as u16));
    code.extend(body);
    while (AT + 4 * code.len() as u64) < data {
        code.push(0);
    }
    for v in [A, B] {
        code.extend((0..4).map(|k| (v >> (32 * k)) as u32));
    }
    code
}

/// Runs the cases and compares the bytes each prints with its expected value.
fn check(group: &str, cases: &[(&str, Vec<u32>, u128)]) {
    let mut bytes = std::fs::read(root().join("testdata/ps2/hello.elf")).unwrap();
    let at = (AT - TEXT + TEXT_OFFSET) as usize;
    for (k, word) in program(cases).iter().enumerate() {
        bytes[at + 4 * k..at + 4 * k + 4].copy_from_slice(&word.to_le_bytes());
    }
    let image = ra_asm_pe::Image::parse(bytes).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    let trapped: Vec<_> = coverage.functions.iter().filter(|(entry, ..)| *entry == AT).collect();
    assert!(trapped.is_empty(), "{group}: the program traps: {trapped:?}\n{:?}", lifted.warnings);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.entry = Some(AT);
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    assert_eq!(outcome.stop, None, "{group}: the run stopped ({} steps)", outcome.steps);
    assert_eq!(outcome.output.len(), 16 * cases.len(), "{group}: the bytes printed");
    for ((name, _, expected), got) in cases.iter().zip(outcome.output.chunks(16)) {
        let got = u128::from_le_bytes(got.try_into().unwrap());
        assert_eq!(got, *expected, "{group}: {name} gives {got:#034x}, the manual {expected:#034x}");
    }
}

// region The manual's definitions

fn lane_mask(bits: u32) -> u128 {
    if bits == 128 { u128::MAX } else { (1 << bits) - 1 }
}

fn split(v: u128, bits: u32) -> Vec<u128> {
    (0..128 / bits).map(|k| (v >> (k * bits)) & lane_mask(bits)).collect()
}

fn join(lanes: &[u128], bits: u32) -> u128 {
    lanes.iter().zip(0..).fold(0, |acc, (l, k)| acc | ((l & lane_mask(bits)) << (k * bits)))
}

fn signed(lane: u128, bits: u32) -> i128 {
    ((lane << (128 - bits)) as i128) >> (128 - bits)
}

/// `f` on each pair of signed lanes of `rs` and `rt`.
fn lanes(bits: u32, f: impl Fn(i128, i128) -> i128) -> u128 {
    let out: Vec<u128> = split(A, bits).iter().zip(split(B, bits)).map(|(&a, b)| f(signed(a, bits), signed(b, bits)) as u128).collect();
    join(&out, bits)
}

/// `f` on each pair of unsigned lanes.
fn ulanes(bits: u32, f: impl Fn(i128, i128) -> i128) -> u128 {
    let out: Vec<u128> = split(A, bits).iter().zip(split(B, bits)).map(|(&a, b)| f(a as i128, b as i128) as u128).collect();
    join(&out, bits)
}

fn clamp(v: i128, bits: u32, signed: bool) -> i128 {
    let (min, max) = if signed { (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1) } else { (0, (1i128 << bits) - 1) };
    v.clamp(min, max)
}

/// The low (or the high) halves of `rt` and `rs` interleaved, `rt`'s lane first.
fn extend(bits: u32, high: bool) -> u128 {
    let (s, t) = (split(A, bits), split(B, bits));
    let half = s.len() / 2;
    let base = if high { half } else { 0 };
    let out: Vec<u128> = (0..half).flat_map(|k| [t[base + k], s[base + k]]).collect();
    join(&out, bits)
}

/// The even lanes of `rt`, then those of `rs`.
fn pack(bits: u32) -> u128 {
    let out: Vec<u128> = split(B, bits).iter().chain(&split(A, bits)).step_by(2).copied().collect();
    join(&out, bits)
}

/// The lanes of `v` in the order `order` names them.
fn order(v: u128, bits: u32, order: &[usize]) -> u128 {
    let l = split(v, bits);
    join(&order.iter().map(|&k| l[k]).collect::<Vec<_>>(), bits)
}

fn sext32(v: i128) -> u128 {
    i128::from(v as i32) as u128 & lane_mask(64)
}

// endregion

#[test]
fn mmi0_sums_compares_and_extends_lanes() {
    let max = |a: i128, b: i128| a.max(b);
    let gt = |a: i128, b: i128| if a > b { -1 } else { 0 };
    let cases = vec![
        ("paddw", vec![mmi(0x08, 0)], lanes(32, |a, b| a + b)),
        ("psubw", vec![mmi(0x08, 1)], lanes(32, |a, b| a - b)),
        ("pcgtw", vec![mmi(0x08, 2)], lanes(32, gt)),
        ("pmaxw", vec![mmi(0x08, 3)], lanes(32, max)),
        ("paddh", vec![mmi(0x08, 4)], lanes(16, |a, b| a + b)),
        ("psubh", vec![mmi(0x08, 5)], lanes(16, |a, b| a - b)),
        ("pcgth", vec![mmi(0x08, 6)], lanes(16, gt)),
        ("pmaxh", vec![mmi(0x08, 7)], lanes(16, max)),
        ("paddb", vec![mmi(0x08, 8)], lanes(8, |a, b| a + b)),
        ("psubb", vec![mmi(0x08, 9)], lanes(8, |a, b| a - b)),
        ("pcgtb", vec![mmi(0x08, 10)], lanes(8, gt)),
        ("paddsw", vec![mmi(0x08, 16)], lanes(32, |a, b| clamp(a + b, 32, true))),
        ("psubsw", vec![mmi(0x08, 17)], lanes(32, |a, b| clamp(a - b, 32, true))),
        ("pextlw", vec![mmi(0x08, 18)], extend(32, false)),
        ("ppacw", vec![mmi(0x08, 19)], pack(32)),
        ("paddsh", vec![mmi(0x08, 20)], lanes(16, |a, b| clamp(a + b, 16, true))),
        ("psubsh", vec![mmi(0x08, 21)], lanes(16, |a, b| clamp(a - b, 16, true))),
        ("pextlh", vec![mmi(0x08, 22)], extend(16, false)),
        ("ppach", vec![mmi(0x08, 23)], pack(16)),
        ("paddsb", vec![mmi(0x08, 24)], lanes(8, |a, b| clamp(a + b, 8, true))),
        ("psubsb", vec![mmi(0x08, 25)], lanes(8, |a, b| clamp(a - b, 8, true))),
        ("pextlb", vec![mmi(0x08, 26)], extend(8, false)),
        ("ppacb", vec![mmi(0x08, 27)], pack(8)),
    ];
    check("MMI0", &cases);
}

#[test]
fn mmi1_unsigned_sums_equalities_minima_and_absolutes() {
    let min = |a: i128, b: i128| a.min(b);
    let eq = |a: i128, b: i128| if a == b { -1 } else { 0 };
    // The absolute value of `rt`'s lanes, the most negative number's the most positive.
    let abs = |bits: u32| lanes(bits, move |_, b| clamp(b.abs(), bits, true));
    // `rs`'s halfword 3 is 0x8000.
    let abs_of_rs = join(&split(A, 16).into_iter().map(|h| clamp(signed(h, 16).abs(), 16, true) as u128).collect::<Vec<_>>(), 16);
    let cases = vec![
        ("pabsw", vec![mmi(0x28, 1)], abs(32)),
        ("pabsh of 0x8000", vec![r(0x1c, 0, RS, RD, 5, 0x28)], abs_of_rs),
        ("pceqw", vec![mmi(0x28, 2)], lanes(32, eq)),
        ("pminw", vec![mmi(0x28, 3)], lanes(32, min)),
        ("pabsh", vec![mmi(0x28, 5)], abs(16)),
        ("pceqh", vec![mmi(0x28, 6)], lanes(16, eq)),
        ("pminh", vec![mmi(0x28, 7)], lanes(16, min)),
        ("pceqb", vec![mmi(0x28, 10)], lanes(8, eq)),
        ("padduw", vec![mmi(0x28, 16)], ulanes(32, |a, b| clamp(a + b, 32, false))),
        ("psubuw", vec![mmi(0x28, 17)], ulanes(32, |a, b| clamp(a - b, 32, false))),
        ("pextuw", vec![mmi(0x28, 18)], extend(32, true)),
        ("padduh", vec![mmi(0x28, 20)], ulanes(16, |a, b| clamp(a + b, 16, false))),
        ("psubuh", vec![mmi(0x28, 21)], ulanes(16, |a, b| clamp(a - b, 16, false))),
        ("pextuh", vec![mmi(0x28, 22)], extend(16, true)),
        ("paddub", vec![mmi(0x28, 24)], ulanes(8, |a, b| clamp(a + b, 8, false))),
        ("psubub", vec![mmi(0x28, 25)], ulanes(8, |a, b| clamp(a - b, 8, false))),
        ("pextub", vec![mmi(0x28, 26)], extend(8, true)),
    ];
    check("MMI1", &cases);
}

/// The variable shifts: each even word of `rt` by the low five bits of `rs`'s, sign-extended.
fn shift_words(f: impl Fn(u32, u32) -> u32) -> u128 {
    let (s, t) = (split(A, 32), split(B, 32));
    join(&[sext32(i128::from(f(t[0] as u32, s[0] as u32 & 31))), sext32(i128::from(f(t[2] as u32, s[2] as u32 & 31)))], 64)
}

#[test]
fn mmi2_logic_copies_and_the_wide_hi_lo() {
    let pmthi = r(0x1c, RS, 0, 0, 8, 0x29);
    let pmtlo = r(0x1c, RT, 0, 0, 9, 0x29);
    let (s, t) = (split(A, 64), split(B, 64));
    let (sh, th) = (split(A, 16), split(B, 16));
    let cases = vec![
        ("psllvw", vec![mmi(0x09, 2)], shift_words(|v, n| v << n)),
        ("psrlvw", vec![mmi(0x09, 3)], shift_words(|v, n| v >> n)),
        ("pmthi, pmfhi", vec![pmthi, mmi(0x09, 8)], A),
        ("pmtlo, pmflo", vec![pmtlo, mmi(0x09, 9)], B),
        ("pinth", vec![mmi(0x09, 10)], join(&[th[0], sh[4], th[1], sh[5], th[2], sh[6], th[3], sh[7]], 16)),
        ("pcpyld", vec![mmi(0x09, 14)], join(&[t[0], s[0]], 64)),
        ("pand", vec![mmi(0x09, 18)], A & B),
        ("pxor", vec![mmi(0x09, 19)], A ^ B),
        ("pexeh", vec![mmi(0x09, 26)], order(B, 16, &[2, 1, 0, 3, 6, 5, 4, 7])),
        ("prevh", vec![mmi(0x09, 27)], order(B, 16, &[3, 2, 1, 0, 7, 6, 5, 4])),
        ("pexew", vec![mmi(0x09, 30)], order(B, 32, &[2, 1, 0, 3])),
        ("prot3w", vec![mmi(0x09, 31)], order(B, 32, &[1, 2, 0, 3])),
    ];
    check("MMI2", &cases);
}

#[test]
fn mmi3_logic_copies_and_exchanges() {
    let (s, t) = (split(A, 64), split(B, 64));
    let (sh, th) = (split(A, 16), split(B, 16));
    let cases = vec![
        ("psravw", vec![mmi(0x29, 3)], shift_words(|v, n| ((v as i32) >> n) as u32)),
        ("pinteh", vec![mmi(0x29, 10)], join(&[th[0], sh[0], th[2], sh[2], th[4], sh[4], th[6], sh[6]], 16)),
        ("pcpyud", vec![mmi(0x29, 14)], join(&[s[1], t[1]], 64)),
        ("por", vec![mmi(0x29, 18)], A | B),
        ("pnor", vec![mmi(0x29, 19)], !(A | B)),
        ("pexch", vec![mmi(0x29, 26)], order(B, 16, &[0, 2, 1, 3, 4, 6, 5, 7])),
        ("pcpyh", vec![mmi(0x29, 27)], order(B, 16, &[0, 0, 0, 0, 4, 4, 4, 4])),
        ("pexcw", vec![mmi(0x29, 30)], order(B, 32, &[0, 2, 1, 3])),
    ];
    check("MMI3", &cases);
}

#[test]
fn parallel_shifts_by_the_shift_amount() {
    let shift = |funct: u32, sa: u32| r(0x1c, 0, RT, RD, sa, funct);
    let each = |bits: u32, f: &dyn Fn(u128) -> u128| join(&split(B, bits).into_iter().map(f).collect::<Vec<_>>(), bits);
    let cases = vec![
        ("psllh", vec![shift(0x34, 3)], each(16, &|h| h << 3)),
        ("psrlh", vec![shift(0x36, 3)], each(16, &|h| h >> 3)),
        ("psrah", vec![shift(0x37, 3)], each(16, &|h| (signed(h, 16) >> 3) as u128)),
        ("psllw", vec![shift(0x3c, 5)], each(32, &|w| w << 5)),
        ("psrlw", vec![shift(0x3e, 5)], each(32, &|w| w >> 5)),
        ("psraw", vec![shift(0x3f, 5)], each(32, &|w| (signed(w, 32) >> 5) as u128)),
    ];
    check("the parallel shifts", &cases);
}

/// `COP1`.
const COP1: u32 = 0x11 << 26;

/// `lui`, `ori` and `mtc1`: the bits of a single into `$f<f>`, through `$14`.
fn mtc1(f: u32, bits: u32) -> [u32; 3] {
    [i(0x0f, 0, 14, (bits >> 16) as u16), i(0x0d, 14, 14, bits as u16), COP1 | (4 << 21) | (14 << 16) | (f << 11)]
}

/// An instruction of the `S` format.
fn single(funct: u32, fd: u32, fs: u32, ft: u32) -> u32 {
    COP1 | (16 << 21) | (ft << 16) | (fs << 11) | (fd << 6) | funct
}

/// `$f1` and `$f2` set to `a` and `b`, the instructions, then the result: `cfc1 $11, $31` with
/// `control`, else `mfc1 $11, $f3`.
fn fpu(a: u32, b: u32, ops: &[u32], control: bool) -> Vec<u32> {
    let mut words = vec![clear()];
    words.extend(mtc1(1, a));
    words.extend(mtc1(2, b));
    words.extend(ops);
    words.push(if control { COP1 | (2 << 21) | (RD << 16) | (31 << 11) } else { COP1 | (RD << 16) | (3 << 11) });
    words
}

/// A single's bits as `mfc1` leaves them: sign-extended into the low 64 bits.
fn word(bits: u32) -> u128 {
    sext32(i128::from(bits as i32))
}

/// The R5900's FPU by the manual's definitions: a denormal is zero, exponent 255 a number, every
/// result rounded toward zero, clamped to `0x7fffffff` and flushed below the smallest normal.
#[test]
fn the_fpu_flushes_denormals_and_rounds_toward_zero() {
    let (one, two, three) = (0x3f80_0000, 0x4000_0000, 0x4040_0000);
    let op = |funct: u32| [single(funct, 3, 1, 2)];
    let flag = 0x0080_0000;
    let cases = vec![
        ("add.s 1.5 + 2.25", fpu(0x3fc0_0000, 0x4010_0000, &op(0), false), word(0x4070_0000)),
        ("add.s 1 + 1.5 * 2^-24", fpu(one, 0x33c0_0000, &op(0), false), word(one)),
        ("sub.s 1 - 2^-60", fpu(one, 0x2180_0000, &op(1), false), word(0x3f7f_ffff)),
        ("add.s past the largest", fpu(0x7fff_ffff, 0x7fff_ffff, &op(0), false), word(0x7fff_ffff)),
        ("mul.s 2^128 * 0.5", fpu(0x7f80_0000, 0x3f00_0000, &op(2), false), word(0x7f00_0000)),
        ("mul.s below the smallest normal", fpu(0x0080_0000, 0x3f00_0000, &op(2), false), word(0)),
        ("mul.s of a denormal", fpu(0x0040_0000, two, &op(2), false), word(0)),
        ("div.s 1 / 3", fpu(one, three, &op(3), false), word(0x3eaa_aaaa)),
        ("div.s 1 / -0", fpu(one, 0x8000_0000, &op(3), false), word(0xffff_ffff)),
        ("sqrt.s of -4", fpu(0, 0xc080_0000, &[single(4, 3, 0, 2)], false), word(two)),
        ("rsqrt.s 1 / sqrt(4)", fpu(one, 0x4080_0000, &op(0x16), false), word(0x3f00_0000)),
        ("cvt.w.s 2.75", fpu(0x4030_0000, 0, &[single(0x24, 3, 1, 0)], false), word(2)),
        ("cvt.w.s 2^31", fpu(0x4f00_0000, 0, &[single(0x24, 3, 1, 0)], false), word(0x7fff_ffff)),
        ("cvt.s.w 2^24 + 3", fpu(0x0100_0003, 0, &[COP1 | (20 << 21) | (1 << 11) | (3 << 6) | 0x20], false), word(0x4b80_0001)),
        ("mula.s, madd.s", fpu(two, three, &[single(0x1a, 0, 1, 2), single(0x1c, 3, 1, 2)], false), word(0x4140_0000)),
        ("adda.s, msub.s", fpu(two, three, &[single(0x18, 0, 1, 2), single(0x1d, 3, 1, 2)], false), word(0xbf80_0000)),
        (
            "mula.s, madda.s, madd.s",
            fpu(two, three, &[single(0x1a, 0, 1, 2), single(0x1e, 0, 1, 2), single(0x1c, 3, 1, 2)], false),
            word(0x4190_0000),
        ),
        ("max.s 1, -2", fpu(one, 0xc000_0000, &op(0x28), false), word(one)),
        ("min.s 1, -2", fpu(one, 0xc000_0000, &op(0x29), false), word(0xc000_0000)),
        ("neg.s 1", fpu(one, 0, &[single(7, 3, 1, 0)], false), word(0xbf80_0000)),
        ("c.lt.s 1 < 2", fpu(one, two, &[single(0x34, 0, 1, 2)], true), word(flag)),
        ("c.eq.s 0 = a negative denormal", fpu(0, 0x8040_0000, &[single(0x32, 0, 1, 2)], true), word(flag)),
        ("c.le.s 2 <= 1", fpu(two, one, &[single(0x36, 0, 1, 2)], true), word(0)),
    ];
    check("the FPU", &cases);
}

#[test]
fn the_second_pipeline_multiplies_into_hi1_and_lo1() {
    let (a, b) = (i64::from(A as u32 as i32), i64::from(B as u32 as i32));
    let product = a * b;
    let (ua, ub) = (u64::from(A as u32), u64::from(B as u32));
    // Each half of the pair, sign-extended into its 64-bit register.
    let lo = |p: i64| sext32(i128::from(p as i32));
    let hi = |p: i64| sext32(i128::from((p >> 32) as i32));
    let mfhi1 = r(0x1c, 0, 0, RD, 0, 0x10);
    let mflo1 = r(0x1c, 0, 0, RD, 0, 0x12);
    let cases = vec![
        ("mult1", vec![clear(), r(0x1c, RS, RT, RD, 0, 0x18)], lo(product)),
        ("mfhi1", vec![clear(), mfhi1], hi(product)),
        ("madd1", vec![clear(), r(0x1c, RS, RT, RD, 0, 0x20)], lo(2 * product)),
        ("mfhi1 after madd1", vec![clear(), mfhi1], hi(2 * product)),
        ("mult with rd", vec![clear(), r(0, RS, RT, RD, 0, 0x18)], lo(product)),
        ("multu1", vec![clear(), r(0x1c, RS, RT, RD, 0, 0x19)], lo((ua * ub) as i64)),
        ("divu1, mflo1", vec![r(0x1c, RS, RT, 0, 0, 0x1b), clear(), mflo1], lo((ua / ub) as i64)),
        ("divu1, mfhi1", vec![clear(), mfhi1], lo((ua % ub) as i64)),
        ("mtlo1, mflo1", vec![r(0x1c, RS, 0, 0, 0, 0x13), clear(), mflo1], A & lane_mask(64)),
    ];
    check("the second pipeline", &cases);
}

/// ps2sdk's kernel stubs (`li $v1, n; syscall; jr $ra`) lift to calls on the EE kernel's
/// functions by their names, and the console's runtime answers them: `FlushCache` returns, `KExit`
/// ends the run, and `CreateThread` makes a thread. Only the stubs the program calls are functions of the analysis: the
/// fixture's `ExitThread`, `iSignalSema` and `_iGetThreadId` are never reached.
#[test]
fn the_ee_kernel_calls_are_named() {
    let image = ra_asm_pe::load(&root().join("testdata/ps2/hello.elf")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let symbol = |name: &str| image.symbols.iter().find(|s| s.name == name).map(|s| s.va).unwrap_or_else(|| panic!("no symbol {name}"));
    let stubs = [
        ("FlushCache", "FlushCache"),
        ("CreateThread", "CreateThread"),
        ("KExit", "KExit"),
        ("sceSifSetDma", "SifSetDma"),
        ("isceSifSetDma", "iSifSetDma"),
        ("setup", "SetSyscall"),
    ];
    for (stub, kernel) in stubs {
        let name = lifted.functions.get(&symbol(stub)).cloned().unwrap_or_else(|| panic!("{stub} is not lifted"));
        let f = lifted.module.functions.iter().find(|f| f.name == name).unwrap();
        let callees: Vec<_> = f
            .blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .filter_map(|i| match &i.inst {
                ra_asm_ir::Inst::Call { callee: ra_asm_ir::Callee::Direct(callee), .. } => Some(callee.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(callees, [format!("ra_ee_{kernel}")], "{stub} calls the kernel's {kernel}");
    }
    let ll = lifted.module.to_ll();
    for kernel in ["KExit", "FlushCache", "CreateThread", "SifSetDma"] {
        let called = ["void", "i64"].iter().any(|ret| ll.contains(&format!("call {ret} @ra_ee_{kernel}(")));
        assert!(called, "the .ll calls {kernel}");
    }

    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let mut run = |stub: &str| {
        lifted.entry = Some(symbol(stub));
        ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap()
    };
    let flushed = run("FlushCache");
    assert_eq!(flushed.stop, None, "FlushCache returns");
    let exited = run("KExit");
    assert_eq!(exited.stop, None, "KExit ends the run");
    let created = run("CreateThread");
    assert_eq!(created.stop, None, "CreateThread makes a thread and returns");
}
