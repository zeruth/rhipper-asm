//! The VFP unit of 32-bit ARM: the floating-point instructions, which are the coprocessors ten
//! (singles) and eleven (doubles) of the instruction set. ARM code and Thumb-2 write them with
//! the same low twenty-eight bits, so one decoder reads both: [`decode`] takes the word of the
//! one or the two halfwords of the other, and looks at nothing above bit 27.
//!
//! What decodes is VFPv2, the unit of an ARM11, with the registers VFPv3 added to it: the
//! arithmetic, the multiply-accumulates, the moves, the compares, the conversions between the
//! two widths and to and from whole numbers, the moves to and from the core registers and the
//! status register, and the loads and stores of one register and of many. A single register is
//! `s0` to `s31`, a double `d0` to `d31`; `s(2n)` and `s(2n+1)` are the halves of `d(n)`.

/// An operation of two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    /// The product, negated.
    Nmul,
    Div,
}

/// An operation of one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Mov,
    Abs,
    Neg,
    Sqrt,
}

/// A floating-point instruction. A register is its number among those of its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fp {
    /// `vadd`, `vsub`, `vmul`, `vnmul`, `vdiv`: `d = n op m`.
    Arith { op: Arith, double: bool, d: u8, n: u8, m: u8 },
    /// `vmla`, `vmls`, `vnmla`, `vnmls`: `d = ±d ± n * m`, the product rounded before the sum;
    /// `negate_sum` is of the `d` the product is added to.
    Accumulate { double: bool, d: u8, n: u8, m: u8, negate_sum: bool, negate_product: bool },
    /// `vmov`, `vabs`, `vneg`, `vsqrt` between registers.
    Unary { op: Unary, double: bool, d: u8, m: u8 },
    /// `vmov` of a constant the instruction holds: the bits of the single or the double.
    Constant { double: bool, d: u8, bits: u64 },
    /// `vcmp` and `vcmpe`, with a register or with zero: the flags of the status register.
    Compare { double: bool, d: u8, m: Option<u8> },
    /// `vcvt` between the widths: `d` is a double when `to_double`, and `m` then a single.
    Width { to_double: bool, d: u8, m: u8 },
    /// `vcvt` from a whole number, the bits of the single register `m`.
    FromInt { double: bool, signed: bool, d: u8, m: u8 },
    /// `vcvt` to a whole number, into the bits of the single register `d`: toward zero, or by
    /// the status register's rounding mode when not `toward_zero` (`vcvtr`).
    ToInt { double: bool, signed: bool, toward_zero: bool, d: u8, m: u8 },
    /// `vmov` between a core register and a single one, the bits as they are.
    Core { to_core: bool, s: u8, rt: u8 },
    /// `vmov` between two core registers and a double, the low word in `rt`.
    CorePair { to_core: bool, d: u8, rt: u8, rt2: u8 },
    /// `vmrs` and `vmsr`: the status register; `vmrs` into the PC is into the flags.
    Status { to_core: bool, rt: u8 },
    /// `vldr` and `vstr`: a base register and an offset in bytes.
    Mem { load: bool, double: bool, d: u8, rn: u8, offset: i32 },
    /// `vldm`, `vstm`, `vpush`, `vpop`: `count` registers from `d`, up from the base, or down
    /// from it `before`.
    Multiple { load: bool, double: bool, d: u8, count: u8, rn: u8, wback: bool, before: bool },
}

impl Fp {
    /// The instruction's mnemonic, without the suffix of its width.
    pub fn name(self) -> &'static str {
        match self {
            Self::Arith { op, .. } => match op {
                Arith::Add => "vadd",
                Arith::Sub => "vsub",
                Arith::Mul => "vmul",
                Arith::Nmul => "vnmul",
                Arith::Div => "vdiv",
            },
            Self::Accumulate { negate_sum, negate_product, .. } => match (negate_sum, negate_product) {
                (false, false) => "vmla",
                (false, true) => "vmls",
                (true, false) => "vnmls",
                (true, true) => "vnmla",
            },
            Self::Unary { op, .. } => match op {
                Unary::Mov => "vmov",
                Unary::Abs => "vabs",
                Unary::Neg => "vneg",
                Unary::Sqrt => "vsqrt",
            },
            Self::Compare { .. } => "vcmp",
            Self::Width { .. } | Self::FromInt { .. } | Self::ToInt { .. } => "vcvt",
            Self::Core { .. } | Self::CorePair { .. } | Self::Constant { .. } => "vmov",
            Self::Status { to_core: true, .. } => "vmrs",
            Self::Status { .. } => "vmsr",
            Self::Mem { load: true, .. } => "vldr",
            Self::Mem { .. } => "vstr",
            Self::Multiple { load: true, .. } => "vldm",
            Self::Multiple { .. } => "vstm",
        }
    }
}

fn field(w: u32, hi: u32, lo: u32) -> u32 {
    (w >> lo) & ((1u32 << (hi - lo + 1)) - 1)
}

fn bit(w: u32, n: u32) -> bool {
    w & (1 << n) != 0
}

/// A register of a four-bit field and the bit that widens it: the bit is the low one of a
/// single's number and the high one of a double's.
fn register(double: bool, four: u32, one: bool) -> u8 {
    (if double { u32::from(one) << 4 | four } else { four << 1 | u32::from(one) }) as u8
}

/// The floating-point instruction of the low twenty-eight bits of `word`; `None` when they are
/// none of the unit's.
#[must_use]
pub fn decode(word: u32) -> Option<Fp> {
    let w = word;
    if field(w, 11, 9) != 0b101 {
        return None;
    }
    let double = bit(w, 8);
    let (vd, vn, vm) = (field(w, 15, 12), field(w, 19, 16), field(w, 3, 0));
    let (d, n, m) = (register(double, vd, bit(w, 22)), register(double, vn, bit(w, 7)), register(double, vm, bit(w, 5)));
    match field(w, 27, 24) {
        0b1110 if !bit(w, 4) => data(w, double, d, n, m),
        0b1110 => {
            // Between a core register and the unit.
            let (to_core, rt) = (bit(w, 20), vd as u8);
            match (field(w, 23, 21), double) {
                (0b000, false) if w & 0x6f == 0 => Some(Fp::Core { to_core, s: register(false, vn, bit(w, 7)), rt }),
                (0b111, false) if vn == 1 && w & 0xef == 0 => Some(Fp::Status { to_core, rt }),
                _ => None,
            }
        }
        0b1100 | 0b1101 => {
            let (p, u, wback, load, rn) = (bit(w, 24), bit(w, 23), bit(w, 21), bit(w, 20), vn as u8);
            let words = (w & 0xff) as u8;
            if !p && !u && !wback {
                // Two core registers and a double: the other forms of this corner are not the unit's.
                return (bit(w, 22) && double && w & 0xd0 == 0x10).then_some(Fp::CorePair {
                    to_core: load,
                    d: register(true, vm, bit(w, 5)),
                    rt: vd as u8,
                    rt2: rn,
                });
            }
            if p && !wback {
                let offset = 4 * i32::from(words);
                return Some(Fp::Mem { load, double, d, rn, offset: if u { offset } else { -offset } });
            }
            // Up from the base, or down before it with the base written back; nothing else.
            if p == u {
                return None;
            }
            let count = if double { words / 2 } else { words };
            (count != 0).then_some(Fp::Multiple { load, double, d, count, rn, wback, before: p })
        }
        _ => None,
    }
}

/// The data-processing instructions: bit four clear.
fn data(w: u32, double: bool, d: u8, n: u8, m: u8) -> Option<Fp> {
    let op = bit(w, 6);
    // The three bits that name the operation: bits 23, 21 and 20, the register's widening bit
    // between them left out.
    Some(match (field(w, 23, 23) << 2 | field(w, 21, 20), op) {
        (0b000, _) => Fp::Accumulate { double, d, n, m, negate_sum: false, negate_product: op },
        (0b001, _) => Fp::Accumulate { double, d, n, m, negate_sum: true, negate_product: op },
        (0b010, _) => Fp::Arith { op: if op { Arith::Nmul } else { Arith::Mul }, double, d, n, m },
        (0b011, _) => Fp::Arith { op: if op { Arith::Sub } else { Arith::Add }, double, d, n, m },
        (0b100, false) => Fp::Arith { op: Arith::Div, double, d, n, m },
        // `vmov` of a constant (VFPv3): eight bits, a sign, three of exponent and four of fraction.
        (0b111, false) => {
            let imm = field(w, 19, 16) << 4 | field(w, 3, 0);
            let (sign, low, fraction) = (u64::from(imm >> 7), u64::from((imm >> 4) & 7), u64::from(imm & 15));
            // The exponent's top bit is the inverse of the third's, which fills the bits between.
            let top = (low >> 2) ^ 1;
            let fill = low >> 2;
            let bits = if double {
                sign << 63 | top << 62 | (if fill == 1 { 0xff } else { 0 }) << 54 | (low & 3) << 52 | fraction << 48
            } else {
                sign << 31 | top << 30 | (if fill == 1 { 0x1f } else { 0 }) << 25 | (low & 3) << 23 | fraction << 19
            };
            Fp::Constant { double, d, bits }
        }
        (0b111, true) => {
            let high = bit(w, 7);
            match field(w, 19, 16) {
                0b0000 => Fp::Unary { op: if high { Unary::Abs } else { Unary::Mov }, double, d, m },
                0b0001 => Fp::Unary { op: if high { Unary::Sqrt } else { Unary::Neg }, double, d, m },
                0b0100 => Fp::Compare { double, d, m: Some(m) },
                0b0101 => Fp::Compare { double, d, m: None },
                // The source is of this instruction's width, the target of the other.
                0b0111 if high => Fp::Width { to_double: !double, d: register(!double, field(w, 15, 12), bit(w, 22)), m },
                // From a whole number in a single register, signed when bit seven is set.
                0b1000 => Fp::FromInt { double, signed: high, d, m: register(false, field(w, 3, 0), bit(w, 5)) },
                kind @ (0b1100 | 0b1101) => {
                    Fp::ToInt { double, signed: kind == 0b1101, toward_zero: high, d: register(false, field(w, 15, 12), bit(w, 22)), m }
                }
                _ => return None,
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arithmetic_decodes() {
        // vadd.f64 d0, d1, d2; vsub.f32 s0, s1, s2; vmul.f64 d16, d17, d18; vdiv.f32 s3, s4, s5
        assert_eq!(decode(0xee31_0b02), Some(Fp::Arith { op: Arith::Add, double: true, d: 0, n: 1, m: 2 }));
        assert_eq!(decode(0xee30_0ac1), Some(Fp::Arith { op: Arith::Sub, double: false, d: 0, n: 1, m: 2 }));
        assert_eq!(decode(0xee61_0ba2), Some(Fp::Arith { op: Arith::Mul, double: true, d: 16, n: 17, m: 18 }));
        assert_eq!(decode(0xeec2_1a22), Some(Fp::Arith { op: Arith::Div, double: false, d: 3, n: 4, m: 5 }));
        // vneg.f64 d0, d1; vmov.f64 d0, d1; vsqrt.f32 s0, s1
        assert_eq!(decode(0xeeb1_0b41), Some(Fp::Unary { op: Unary::Neg, double: true, d: 0, m: 1 }));
        assert_eq!(decode(0xeeb0_0b41), Some(Fp::Unary { op: Unary::Mov, double: true, d: 0, m: 1 }));
        assert_eq!(decode(0xeeb1_0ae0), Some(Fp::Unary { op: Unary::Sqrt, double: false, d: 0, m: 1 }));
        // vmla.f64 d0, d1, d2
        assert_eq!(decode(0xee01_0b02), Some(Fp::Accumulate { double: true, d: 0, n: 1, m: 2, negate_sum: false, negate_product: false }));
    }

    #[test]
    fn the_compares_and_the_conversions_decode() {
        // vcmp.f64 d0, d1; vcmpe.f32 s0, #0
        assert_eq!(decode(0xeeb4_0b41), Some(Fp::Compare { double: true, d: 0, m: Some(1) }));
        assert_eq!(decode(0xeeb5_0ac0), Some(Fp::Compare { double: false, d: 0, m: None }));
        // vcvt.f64.f32 d0, s2; vcvt.f32.f64 s0, d1
        assert_eq!(decode(0xeeb7_0ac1), Some(Fp::Width { to_double: true, d: 0, m: 2 }));
        assert_eq!(decode(0xeeb7_0bc1), Some(Fp::Width { to_double: false, d: 0, m: 1 }));
        // vcvt.f64.s32 d0, s2; vcvt.f32.u32 s0, s1; vcvt.s32.f64 s0, d1
        assert_eq!(decode(0xeeb8_0bc1), Some(Fp::FromInt { double: true, signed: true, d: 0, m: 2 }));
        assert_eq!(decode(0xeeb8_0a60), Some(Fp::FromInt { double: false, signed: false, d: 0, m: 1 }));
        assert_eq!(decode(0xeebd_0bc1), Some(Fp::ToInt { double: true, signed: true, toward_zero: true, d: 0, m: 1 }));
    }

    #[test]
    fn the_moves_and_the_memory_forms_decode() {
        // vmov s0, r1; vmov r0, s3; vmov d1, r2, r3; vmrs APSR_nzcv, fpscr
        assert_eq!(decode(0xee00_1a10), Some(Fp::Core { to_core: false, s: 0, rt: 1 }));
        assert_eq!(decode(0xee11_0a90), Some(Fp::Core { to_core: true, s: 3, rt: 0 }));
        assert_eq!(decode(0xec43_2b11), Some(Fp::CorePair { to_core: false, d: 1, rt: 2, rt2: 3 }));
        assert_eq!(decode(0xeef1_fa10), Some(Fp::Status { to_core: true, rt: 15 }));
        // vldr d0, [r1, #8]; vstr s1, [r2, #-4]
        assert_eq!(decode(0xed91_0b02), Some(Fp::Mem { load: true, double: true, d: 0, rn: 1, offset: 8 }));
        assert_eq!(decode(0xed42_0a01), Some(Fp::Mem { load: false, double: false, d: 1, rn: 2, offset: -4 }));
        // vpush {d8, d9}; vpop {d8, d9}; vldmia r0, {d0, d1, d2}
        assert_eq!(
            decode(0xed2d_8b04),
            Some(Fp::Multiple { load: false, double: true, d: 8, count: 2, rn: 13, wback: true, before: true })
        );
        assert_eq!(
            decode(0xecbd_8b04),
            Some(Fp::Multiple { load: true, double: true, d: 8, count: 2, rn: 13, wback: true, before: false })
        );
        assert_eq!(
            decode(0xec90_0b06),
            Some(Fp::Multiple { load: true, double: true, d: 0, count: 3, rn: 0, wback: false, before: false })
        );
        // vmov.f64 d0, #1.0; vmov.f32 s0, #-2.0; vmov.f64 d1, #0.5
        assert_eq!(decode(0xeeb7_0b00), Some(Fp::Constant { double: true, d: 0, bits: 1.0f64.to_bits() }));
        assert_eq!(decode(0xeeb8_0a00), Some(Fp::Constant { double: false, d: 0, bits: u64::from((-2.0f32).to_bits()) }));
        assert_eq!(decode(0xeeb6_1b00), Some(Fp::Constant { double: true, d: 1, bits: 0.5f64.to_bits() }));
        // An instruction of another coprocessor is none of the unit's.
        assert_eq!(decode(0xee00_0f10), None);
    }
}
