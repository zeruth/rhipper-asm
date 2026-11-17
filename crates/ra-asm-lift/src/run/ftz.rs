//! The arithmetic of the R5900's FPU, which is not IEEE's, on the bits of its single registers: the
//! `ra_f32_ftz_*` functions the lift calls, as the interpreter answers them (`runtime/driver.c`
//! has the same in C).
//!
//! A zero exponent is zero, so a denormal operand counts as a zero of its sign; every other
//! exponent is a normal number, 255 too, since the FPU has no infinity and no NaN. A result is the
//! exact one rounded toward zero, the FPU's only rounding: one past the largest magnitude
//! (`0x7fffffff`) clamps to it, one below the smallest normal flushes to a zero of its sign. A
//! division by zero gives the largest magnitude with the sign of the quotient, a square root
//! takes the magnitude of its operand. The flags the FPU sets in its control register are not
//! kept.

/// The largest magnitude, exponent 255 and every fraction bit.
const FMAX: u32 = 0x7fff_ffff;
const SIGN: u32 = 0x8000_0000;
/// The smallest normal magnitude.
const MIN_NORMAL: u32 = 0x0080_0000;

/// The value a register holds, exactly: a double has room for every exponent and fraction.
fn value(bits: u32) -> f64 {
    let sign = u64::from(bits & SIGN) << 32;
    let exponent = u64::from((bits >> 23) & 0xff);
    if exponent == 0 {
        return f64::from_bits(sign);
    }
    let fraction = u64::from(bits & 0x7f_ffff);
    f64::from_bits(sign | ((exponent + 1023 - 127) << 52) | (fraction << 29))
}

/// `x` rounded toward zero into a register, clamped above and flushed below.
fn bits(x: f64) -> u32 {
    let b = x.to_bits();
    let sign = (b >> 32) as u32 & SIGN;
    let exponent = ((b >> 52) & 0x7ff) as i64 - 1023 + 127;
    if x == 0.0 || exponent < 1 {
        return sign;
    }
    if exponent > 255 {
        return sign | FMAX;
    }
    sign | ((exponent as u32) << 23) | ((b >> 29) as u32 & 0x7f_ffff)
}

/// The register one step nearer zero than `r`, a normal: below the smallest normal it is zero.
fn toward_zero(r: u32) -> u32 {
    if r & FMAX == MIN_NORMAL { r & SIGN } else { r - 1 }
}

/// `a + b`, rounded toward zero. The double's sum rounds to nearest; its error, which the
/// two-sum gives exactly, says on which side of the sum the exact value lies, and only a sum on
/// the register's grid with the exact value nearer zero rounds otherwise.
fn sum(a: f64, b: f64) -> u32 {
    let s = a + b;
    let bb = s - a;
    let error = (a - (s - bb)) + (b - bb);
    let r = bits(s);
    #[allow(clippy::float_cmp)]
    let exact = value(r) == s;
    if error != 0.0 && exact && (error < 0.0) != (s < 0.0) { toward_zero(r) } else { r }
}

pub(super) fn add(a: u32, b: u32) -> u32 {
    sum(value(a), value(b))
}

pub(super) fn sub(a: u32, b: u32) -> u32 {
    sum(value(a), -value(b))
}

/// The product of two 24-bit fractions has 48 bits: the double holds it exactly.
pub(super) fn mul(a: u32, b: u32) -> u32 {
    bits(value(a) * value(b))
}

/// The double's quotient of two 24-bit fractions is never rounded onto or across a point of the
/// register's grid it does not equal, so its truncation is the exact one's.
pub(super) fn div(a: u32, b: u32) -> u32 {
    if b & 0x7f80_0000 == 0 {
        return ((a ^ b) & SIGN) | FMAX;
    }
    bits(value(a) / value(b))
}

/// The root of the magnitude; a zero keeps its sign. The double's root is never rounded onto a
/// point of the grid, as the quotient is not.
pub(super) fn sqrt(a: u32) -> u32 {
    if a & 0x7f80_0000 == 0 {
        return a & SIGN;
    }
    bits(value(a & FMAX).sqrt())
}

/// The 24-bit fraction with its leading one and the exponent of its last bit.
fn parts(x: u32) -> (u128, i32) {
    (u128::from((x & 0x7f_ffff) | MIN_NORMAL), ((x >> 23) & 0xff) as i32 - 127 - 23)
}

/// Whether `g * sqrt(b) <= a`, that is `g * g * b <= a * a`, for the normal magnitudes `g`, `a`
/// and `b`, in integers: `g * g * b`'s fraction has 70 to 72 bits and `a * a`'s 47 or 48, so only
/// a shift of `a * a` up by up to 48 bits is in doubt.
fn root_at_most(g: u32, a: u32, b: u32) -> bool {
    let ((mg, eg), (ma, ea), (mb, eb)) = (parts(g), parts(a), parts(b));
    let (left, right) = (mg * mg * mb, ma * ma);
    let k = 2 * ea - 2 * eg - eb;
    if k < 0 {
        false
    } else if k > 48 {
        true
    } else {
        left <= right << k
    }
}

/// `a / sqrt(|b|)`: the double's two roundings put the result within a step of the exact one,
/// which the comparison in integers settles.
pub(super) fn rsqrt(a: u32, b: u32) -> u32 {
    let sign = a & SIGN;
    if b & 0x7f80_0000 == 0 {
        return sign | FMAX;
    }
    if a & 0x7f80_0000 == 0 {
        return sign;
    }
    let (a, b) = (a & FMAX, b & FMAX);
    // A result the double's rounding flushed may still be the smallest normal.
    let mut r = bits(value(a) / value(b).sqrt()).max(MIN_NORMAL);
    while r > MIN_NORMAL && !root_at_most(r, a, b) {
        r -= 1;
    }
    if r == MIN_NORMAL && !root_at_most(r, a, b) {
        return sign;
    }
    while r < FMAX && root_at_most(r + 1, a, b) {
        r += 1;
    }
    sign | r
}

/// `cvt.s.w`: the word's value, exact in the double, rounded toward zero.
pub(super) fn cvt_s_w(a: u32) -> u32 {
    bits(f64::from(a as i32))
}

/// `cvt.w.s`: truncated toward zero, whatever the rounding mode; a magnitude of 2^31 or more
/// gives the largest word of its sign.
pub(super) fn cvt_w_s(a: u32) -> u32 {
    if a & 0x7f80_0000 > 0x4e80_0000 {
        return if a & SIGN == 0 { 0x7fff_ffff } else { 0x8000_0000 };
    }
    value(a).trunc() as i32 as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sum_is_rounded_toward_zero_from_the_exact_value() {
        assert_eq!(add(0x3fc0_0000, 0x4010_0000), 0x4070_0000, "1.5 + 2.25");
        assert_eq!(add(0x3f80_0000, 0x33c0_0000), 0x3f80_0000, "1 + 1.5 * 2^-24");
        assert_eq!(sub(0x3f80_0000, 0x2180_0000), 0x3f7f_ffff, "1 - 2^-60");
        assert_eq!(sub(0xbf80_0000, 0xa180_0000), 0xbf7f_ffff, "-1 + 2^-60");
        assert_eq!(add(0x7f7f_ffff, 0x7f7f_ffff), 0x7fff_ffff, "exponent 255 is a number");
        assert_eq!(add(0x7fff_ffff, 0x7fff_ffff), 0x7fff_ffff, "an overflow clamps");
        assert_eq!(sub(0x0080_0001, 0x0080_0000), 0, "an underflow flushes");
    }

    #[test]
    fn products_quotients_and_roots() {
        assert_eq!(mul(0x7f80_0000, 0x3f00_0000), 0x7f00_0000, "2^128 / 2");
        assert_eq!(mul(0x0080_0000, 0x3f00_0000), 0, "half the smallest normal");
        assert_eq!(mul(0x0040_0000, 0x4000_0000), 0, "a denormal is zero");
        assert_eq!(div(0x3f80_0000, 0), 0x7fff_ffff);
        assert_eq!(div(0x3f80_0000, 0x8000_0000), 0xffff_ffff);
        assert_eq!(div(0x3f80_0000, 0x4040_0000), 0x3eaa_aaaa, "1 / 3 truncated");
        assert_eq!(sqrt(0xc080_0000), 0x4000_0000, "the root of -4's magnitude");
        assert_eq!(sqrt(0x8000_0000), 0x8000_0000);
        assert_eq!(rsqrt(0x3f80_0000, 0x4080_0000), 0x3f00_0000, "1 / sqrt(4)");
        assert_eq!(rsqrt(0x3f80_0000, 0x4000_0000), 0x3f35_04f3, "1 / sqrt(2) truncated");
    }

    #[test]
    fn conversions_truncate_and_clamp() {
        assert_eq!(cvt_s_w(16_777_219), 0x4b80_0001, "2^24 + 3 truncated");
        assert_eq!(cvt_w_s(0x4030_0000), 2, "2.75");
        assert_eq!(cvt_w_s(0xc030_0000), (-2i32) as u32);
        assert_eq!(cvt_w_s(0x4f00_0000), 0x7fff_ffff);
        assert_eq!(cvt_w_s(0xcf00_0000), 0x8000_0000);
    }
}
