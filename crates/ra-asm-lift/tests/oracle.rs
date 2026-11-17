//! The oracles of the lift. The execution oracle, which compiles the lifted fixtures with clang and
//! runs them, is the slow tier behind the `oracle` feature (`slow`); the instruction oracles here
//! run the interpreter alone and need no tool.

use std::path::{Path, PathBuf};

#[cfg(feature = "oracle")]
#[path = "oracle/slow.rs"]
mod slow;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// region The Allegrex's integer instructions

/// Where the PSP hello's `module_start` loads: the program overwrites it.
const PSP_ENTRY: u64 = 0x0880_40ec;
/// The operands: `$9` and `$10`; each case's result goes to `$11`.
const A: u32 = 0xf0e1_d2c3;
const B: u32 = 0x1234_5678;
const RS: u32 = 9;
const RT: u32 = 10;
const RD: u32 = 11;

fn r(op: u32, rs: u32, rt: u32, rd: u32, sa: u32, funct: u32) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | (rd << 11) | (sa << 6) | funct
}

fn i(op: u32, rs: u32, rt: u32, imm: u16) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | u32::from(imm)
}

/// `lui` and `ori`: the word `v` in register `n`.
fn constant(n: u32, v: u32) -> [u32; 2] {
    [i(0x0f, 0, n, (v >> 16) as u16), i(0x0d, n, n, v as u16)]
}

/// A `SPECIAL3` byte or bit shuffle (`BSHFL`): `rd = op(rt)`.
fn shuffle(sa: u32) -> u32 {
    r(0x1f, 0, RS, RD, sa, 0x20)
}

/// `madd`, `msub` and the rest on `hi:lo` set to `B:A`, then the half `mfhi` (`0x10`) or `mflo`
/// (`0x12`) moves to the result.
fn accumulate(funct: u32, half: u32) -> Vec<u32> {
    vec![r(0, RT, 0, 0, 0, 0x11), r(0, RS, 0, 0, 0, 0x13), r(0, RS, RT, 0, 0, funct), r(0, 0, 0, RD, 0, half)]
}

/// `hi:lo` after `madd` (or `msub` with `subtract`) on `B:A` with `A * B`, by the manual's definition.
fn accumulated(subtract: bool) -> u64 {
    let acc = (u64::from(B) << 32) | u64::from(A);
    let product = (i64::from(A as i32) * i64::from(B as i32)) as u64;
    if subtract { acc.wrapping_sub(product) } else { acc.wrapping_add(product) }
}

/// Each case's words and the word its definition gives.
fn allegrex_cases() -> Vec<(&'static str, Vec<u32>, u32)> {
    let (sa, sb) = (A as i32, B as i32);
    vec![
        // ext $11, $9, 4, 12: the twelve bits from bit 4.
        ("ext", vec![r(0x1f, RS, RD, 11, 4, 0)], (A >> 4) & 0xfff),
        // ins $11, $9, 8, 8 over a copy of $10: bits 8 to 15 from the low byte of $9.
        ("ins", vec![r(0, RT, 0, RD, 0, 0x25), r(0x1f, RS, RD, 15, 8, 4)], (B & !0xff00) | ((A & 0xff) << 8)),
        ("seb", vec![shuffle(0x10)], A as u8 as i8 as i32 as u32),
        ("seh", vec![shuffle(0x18)], A as u16 as i16 as i32 as u32),
        ("wsbh", vec![shuffle(0x02)], ((A << 8) & 0xff00_ff00) | ((A >> 8) & 0x00ff_00ff)),
        ("bitrev", vec![shuffle(0x14)], A.reverse_bits()),
        ("max", vec![r(0, RS, RT, RD, 0, 0x2c)], sa.max(sb) as u32),
        ("min", vec![r(0, RS, RT, RD, 0, 0x2d)], sa.min(sb) as u32),
        ("clz", vec![r(0, RT, 0, RD, 0, 0x16)], B.leading_zeros()),
        ("clz of zero", vec![r(0, 0, 0, RD, 0, 0x16)], 32),
        ("clo", vec![r(0, RS, 0, RD, 0, 0x17)], A.leading_ones()),
        ("madd lo", accumulate(0x1c, 0x12), accumulated(false) as u32),
        ("madd hi", accumulate(0x1c, 0x10), (accumulated(false) >> 32) as u32),
        ("msub lo", accumulate(0x2e, 0x12), accumulated(true) as u32),
        ("msub hi", accumulate(0x2e, 0x10), (accumulated(true) >> 32) as u32),
    ]
}

/// The program: the operands, then each case's words and the comparison of its result with the
/// expected word, which sets the case's bit of `v0` when they differ; `v0` is the run's exit code.
fn allegrex_program(cases: &[(&str, Vec<u32>, u32)]) -> Vec<u32> {
    let mut code = vec![r(0, 0, 0, 2, 0, 0x25)];
    code.extend(constant(RS, A));
    code.extend(constant(RT, B));
    for (bit, (_, words, expected)) in (0..).zip(cases) {
        code.extend(words);
        code.extend(constant(12, *expected));
        // xor, sltu, sll, or: the case's bit, set when the words differ.
        code.extend([r(0, RD, 12, 13, 0, 0x26), r(0, 0, 13, 13, 0, 0x2b), r(0, 0, 13, 13, bit, 0), r(0, 2, 13, 2, 0, 0x25)]);
    }
    code.extend([0x03e0_0008, 0]);
    code
}

/// The Allegrex's own integer instructions: the PSP hello's `module_start` is overwritten with a
/// program that runs each one on two words and compares the result with the one the manual's
/// definition gives; the interpreter runs it over the mapped module.
#[test]
fn allegrex_integer_ops() {
    let cases = allegrex_cases();
    assert!(cases.len() < 32, "a case's bit in the exit code");
    let mut image = ra_asm_pe::load(&root().join("testdata/psp/hello.prx")).unwrap();
    assert_eq!((image.platform(), image.entry_point()), (Some("psp"), Some(PSP_ENTRY)), "the PSP hello");
    let bytes: Vec<u8> = allegrex_program(&cases).iter().flat_map(|w| w.to_le_bytes()).collect();
    image.write(PSP_ENTRY, &bytes).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    let trapped: Vec<_> = coverage.functions.iter().filter(|(entry, ..)| *entry == PSP_ENTRY).collect();
    assert!(trapped.is_empty(), "the program traps: {trapped:?}\n{:?}", lifted.warnings);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.entry = Some(PSP_ENTRY);
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    assert_eq!(outcome.stop, None, "the run stopped ({} steps)", outcome.steps);
    let wrong: Vec<_> = (0..).zip(&cases).filter(|(bit, _)| outcome.exit as u32 & (1 << bit) != 0).map(|(_, (name, ..))| *name).collect();
    assert!(wrong.is_empty(), "the lift differs from the manual on {wrong:?}");
}

// endregion
