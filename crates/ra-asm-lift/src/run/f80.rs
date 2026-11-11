//! The exact x87 unit of the interpreter: a software f80 over the runtime's slots. A slot holds
//! an 80-bit value as the x87 stores a tbyte, the 64-bit significand with its explicit integer
//! bit then the sign and the 15-bit exponent, and six bytes of padding. Every operation unpacks
//! its operands, works in 128 bits with a sticky bit for what fell below, and rounds the result by
//! the control word: the rounding mode in bits 10 and 11 and the precision in bits 8 and 9 (24,
//! 53 or 64 bits), as the hardware does with the exceptions masked. The flags an operation raises
//! come back in [`Unit::flags`], for the runtime to merge into the program's status word. The C
//! driver and the Rust runtime module have the same unit over pointers; this one works on copies,
//! since the interpreter owns the memory.

#![allow(clippy::many_single_char_names)]

pub(super) type Slot = [u8; 16];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zero,
    Finite,
    Inf,
    Nan,
}

/// An unpacked value: `m * 2^(exp - 63)`, `m` normalized with bit 63 set when finite.
#[derive(Clone, Copy)]
struct F80 {
    kind: Kind,
    sign: bool,
    exp: i32,
    m: u64,
}

// The exception flags of the status word: invalid, zero divide, overflow, underflow and precision.
const IE: u16 = 0x01;
const ZE: u16 = 0x04;
const OE: u16 = 0x08;
const UE: u16 = 0x10;
const PE: u16 = 0x20;

/// The unit: the flags the operations since the last take raised.
#[derive(Default)]
pub(super) struct Unit {
    pub(super) flags: u16,
}

fn unpack(slot: &Slot) -> F80 {
    let m = u64::from_le_bytes(slot[..8].try_into().unwrap());
    let se = u16::from_le_bytes([slot[8], slot[9]]);
    let mut v = F80 { kind: Kind::Finite, sign: se >> 15 != 0, exp: 0, m };
    let e = i32::from(se & 0x7fff);
    if e == 0x7fff {
        v.kind = if m << 1 == 0 { Kind::Inf } else { Kind::Nan };
        return v;
    }
    if m == 0 {
        v.kind = Kind::Zero;
        return v;
    }
    // A denormal has exponent 0 and the integer bit clear; an unnormal is normalized the same way.
    let shift = m.leading_zeros() as i32;
    v.m = m << shift;
    v.exp = (if e == 0 { 1 } else { e }) - 16383 - shift;
    v
}

fn write(sign: bool, exponent: u32, m: u64) -> Slot {
    let mut slot = [0u8; 16];
    slot[..8].copy_from_slice(&m.to_le_bytes());
    let se = (u16::from(sign) << 15) | exponent as u16;
    slot[8..10].copy_from_slice(&se.to_le_bytes());
    slot
}

fn write_inf(sign: bool) -> Slot {
    write(sign, 0x7fff, 0x8000_0000_0000_0000)
}

fn write_quiet(v: F80) -> Slot {
    write(v.sign, 0x7fff, v.m | 0xc000_0000_0000_0000)
}

/// A NaN, an infinity or a zero goes out as it is, the NaN quiet.
fn write_special(v: F80) -> Slot {
    match v.kind {
        Kind::Nan => write_quiet(v),
        Kind::Inf => write_inf(v.sign),
        _ => write(v.sign, 0, 0),
    }
}

/// The NaN of a two-operand operation: the one with the larger significand, quiet.
fn nan_result(a: F80, b: F80) -> Option<Slot> {
    if a.kind != Kind::Nan && b.kind != Kind::Nan {
        return None;
    }
    let n = if a.kind != Kind::Nan {
        b
    } else if b.kind != Kind::Nan {
        a
    } else if b.m > a.m {
        b
    } else {
        a
    };
    Some(write_quiet(n))
}

fn rc(cw: u64) -> u32 {
    ((cw >> 10) & 3) as u32
}

fn precision(cw: u64) -> u32 {
    match (cw >> 8) & 3 {
        0 => 24,
        2 => 53,
        _ => 64,
    }
}

/// Whether a finite value below 2^63 has a fraction: the bits of `m` below the units position.
fn has_fraction(v: F80) -> bool {
    v.exp < 0 || v.m << (v.exp + 1) != 0
}

/// The magnitude of the integer nearest a finite value below 2^63, by the mode `rc`.
fn to_integer(v: F80, rc: u32) -> u64 {
    let shift = 63 - v.exp;
    let (q, half, sticky) = match shift {
        65.. => (0, false, true),
        64 => (0, v.m >> 63 != 0, v.m << 1 != 0),
        _ => {
            let rest = v.m & ((1u64 << shift) - 1);
            (v.m >> shift, (rest >> (shift - 1)) & 1 != 0, rest & ((1u64 << (shift - 1)) - 1) != 0)
        }
    };
    let up = match rc {
        0 => half && (sticky || q & 1 != 0),
        1 => v.sign && (half || sticky),
        2 => !v.sign && (half || sticky),
        _ => false,
    };
    q + u64::from(up)
}

impl Unit {
    fn raise(&mut self, flags: u16) {
        self.flags |= flags;
    }

    /// The real indefinite: the result of an invalid operation.
    fn indefinite(&mut self) -> Slot {
        self.raise(IE);
        write(true, 0x7fff, 0xc000_0000_0000_0000)
    }

    /// A result rounded into a binary format: the value `m * 2^(exp - 127)` with bit 127 set
    /// (sticky for what lies below), to `p` bits in the mode `rc`, with the exponent `bias` and
    /// the all-ones exponent `emax` of the format; the significand comes back left-aligned in 64
    /// bits with its integer bit, and a denormal with the exponent 0.
    #[allow(clippy::too_many_arguments)]
    fn round(&mut self, sign: bool, exp: i32, mut m: u128, mut sticky: bool, p: u32, rc: u32, emax: i32, bias: i32) -> (i32, u64) {
        if m == 0 {
            return (0, 0);
        }
        let mut biased = exp + bias;
        let tiny = biased < 1;
        if tiny {
            // A denormal: the bits below the format's smallest exponent go into the rounding.
            let s = 1 - biased;
            if s >= 128 {
                sticky |= m != 0;
                m = 0;
            } else {
                sticky |= m & ((1u128 << s) - 1) != 0;
                m >>= s;
            }
            biased = 1;
        }
        let shift = 128 - p;
        let mask = (1u128 << shift) - 1;
        let half = 1u128 << (shift - 1);
        let low = m & mask;
        m &= !mask;
        let inexact = low != 0 || sticky;
        let up = match rc {
            0 => low > half || (low == half && (sticky || m & (1u128 << shift) != 0)),
            1 => sign && (low != 0 || sticky),
            2 => !sign && (low != 0 || sticky),
            _ => false,
        };
        if up {
            let (sum, overflow) = m.overflowing_add(1u128 << shift);
            m = if overflow { 1u128 << 127 } else { sum };
            if overflow {
                biased += 1;
            }
        }
        if biased == 1 && m >> 127 == 0 {
            biased = 0;
        }
        if biased >= emax {
            self.raise(OE | PE);
            let to_inf = rc == 0 || (rc == 1 && sign) || (rc == 2 && !sign);
            return if to_inf { (emax, 0x8000_0000_0000_0000) } else { (emax - 1, !0u64 << (64 - p)) };
        }
        if inexact {
            self.raise(if tiny { UE | PE } else { PE });
        }
        (biased, (m >> 64) as u64)
    }

    fn write_rounded(&mut self, sign: bool, exp: i32, m: u128, sticky: bool, cw: u64) -> Slot {
        let (biased, m) = self.round(sign, exp, m, sticky, precision(cw), rc(cw), 0x7fff, 16383);
        write(sign, biased as u32, m)
    }

    /// A finite or zero value, rounded to the precision of the control word.
    fn write_value(&mut self, v: F80, cw: u64) -> Slot {
        let m = if v.kind == Kind::Zero { 0 } else { u128::from(v.m) << 64 };
        self.write_rounded(v.sign, v.exp, m, false, cw)
    }

    /// A tbyte of memory as a slot.
    pub(super) fn load(memory: &[u8]) -> Slot {
        let mut slot = [0u8; 16];
        slot[..10].copy_from_slice(&memory[..10]);
        slot
    }

    /// The tbyte a slot stores.
    pub(super) fn store(slot: &Slot) -> [u8; 10] {
        slot[..10].try_into().unwrap()
    }

    pub(super) fn pack(significand: u64, sign_exponent: u64) -> Slot {
        write(sign_exponent >> 15 & 1 != 0, sign_exponent as u32 & 0x7fff, significand)
    }

    pub(super) fn from_f64(value: f64) -> Slot {
        let bits = value.to_bits();
        let sign = bits >> 63 != 0;
        let e = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & ((1u64 << 52) - 1);
        if e == 0x7ff {
            write(sign, 0x7fff, if frac == 0 { 0x8000_0000_0000_0000 } else { 0xc000_0000_0000_0000 | (frac << 11) })
        } else if e == 0 && frac == 0 {
            write(sign, 0, 0)
        } else if e == 0 {
            let shift = frac.leading_zeros();
            write(sign, 15372 - shift, frac << shift)
        } else {
            write(sign, (e - 1023 + 16383) as u32, 0x8000_0000_0000_0000 | (frac << 11))
        }
    }

    pub(super) fn from_i64(value: u64) -> Slot {
        let v = value as i64;
        if v == 0 {
            return write(false, 0, 0);
        }
        let u = v.unsigned_abs();
        let shift = u.leading_zeros();
        write(v < 0, 16383 + 63 - shift, u << shift)
    }

    /// The slot as a double, rounded by the control word; the flags of the rounding are raised.
    pub(super) fn f64_of(&mut self, slot: &Slot, cw: u64) -> f64 {
        let v = unpack(slot);
        let sign = u64::from(v.sign) << 63;
        let bits = match v.kind {
            Kind::Nan => sign | 0x7ff << 52 | 1 << 51 | ((v.m << 1) >> 12),
            Kind::Inf => sign | 0x7ff << 52,
            Kind::Zero => sign,
            Kind::Finite => {
                let (biased, m) = self.round(v.sign, v.exp, u128::from(v.m) << 64, false, 53, rc(cw), 0x7ff, 1023);
                sign | (biased as u64) << 52 | ((m << 1) >> 12)
            }
        };
        f64::from_bits(bits)
    }

    pub(super) fn f32_of(&mut self, slot: &Slot, cw: u64) -> f32 {
        let v = unpack(slot);
        let sign = u32::from(v.sign) << 31;
        let bits = match v.kind {
            Kind::Nan => sign | 0xff << 23 | 1 << 22 | ((v.m << 1) >> 41) as u32,
            Kind::Inf => sign | 0xff << 23,
            Kind::Zero => sign,
            Kind::Finite => {
                let (biased, m) = self.round(v.sign, v.exp, u128::from(v.m) << 64, false, 24, rc(cw), 0xff, 127);
                sign | (biased as u32) << 23 | ((m << 1) >> 41) as u32
            }
        };
        f32::from_bits(bits)
    }

    /// An integer of `bits` bits, rounded by `rounding` (the mode alone, 3 for a truncation), or
    /// the integer indefinite when the value is not a number or does not fit.
    pub(super) fn int_of(&mut self, slot: &Slot, rounding: u64, bits: u64) -> u64 {
        let v = unpack(slot);
        let limit = 1u64 << (bits - 1);
        if v.kind == Kind::Zero {
            return 0;
        }
        if v.kind != Kind::Finite || v.exp >= bits as i32 - 1 {
            self.raise(IE);
            return limit;
        }
        let q = to_integer(v, (rounding & 3) as u32);
        if if v.sign { q > limit } else { q >= limit } {
            self.raise(IE);
            return limit;
        }
        if has_fraction(v) {
            self.raise(PE);
        }
        if v.sign { 0u64.wrapping_sub(q) } else { q }
    }

    fn add_signed(&mut self, pa: &Slot, pb: &Slot, cw: u64, negate: bool) -> Slot {
        let a = unpack(pa);
        let mut b = unpack(pb);
        b.sign ^= negate;
        let rc = rc(cw);
        if let Some(nan) = nan_result(a, b) {
            return nan;
        }
        if a.kind == Kind::Inf || b.kind == Kind::Inf {
            return if a.kind == Kind::Inf && b.kind == Kind::Inf && a.sign != b.sign {
                self.indefinite()
            } else {
                write_special(if a.kind == Kind::Inf { a } else { b })
            };
        }
        if a.kind == Kind::Zero && b.kind == Kind::Zero {
            return write((a.sign && b.sign) || (rc == 1 && (a.sign || b.sign)), 0, 0);
        }
        if a.kind == Kind::Zero {
            return self.write_value(b, cw);
        }
        if b.kind == Kind::Zero {
            return self.write_value(a, cw);
        }
        // The larger magnitude first; the smaller one is aligned to it, what falls off kept as a
        // sticky bit far below the rounding position.
        let (a, b) = if a.exp < b.exp || (a.exp == b.exp && a.m < b.m) { (b, a) } else { (a, b) };
        let ma = u128::from(a.m) << 64;
        let mut mb = u128::from(b.m) << 64;
        let d = a.exp - b.exp;
        if d >= 128 {
            mb = 1;
        } else if d > 0 {
            let lost = mb & ((1u128 << d) - 1);
            mb = (mb >> d) | u128::from(lost != 0);
        }
        let mut exp = a.exp;
        let m = if a.sign == b.sign {
            let (sum, carry) = ma.overflowing_add(mb);
            if carry {
                exp += 1;
                (sum >> 1) | (1u128 << 127) | (sum & 1)
            } else {
                sum
            }
        } else {
            let diff = ma - mb;
            if diff == 0 {
                return write(rc == 1, 0, 0);
            }
            let lz = diff.leading_zeros() as i32;
            exp -= lz;
            diff << lz
        };
        self.write_rounded(a.sign, exp, m, false, cw)
    }

    pub(super) fn add(&mut self, a: &Slot, b: &Slot, cw: u64) -> Slot {
        self.add_signed(a, b, cw, false)
    }

    pub(super) fn sub(&mut self, a: &Slot, b: &Slot, cw: u64) -> Slot {
        self.add_signed(a, b, cw, true)
    }

    pub(super) fn mul(&mut self, pa: &Slot, pb: &Slot, cw: u64) -> Slot {
        let (a, b) = (unpack(pa), unpack(pb));
        let sign = a.sign ^ b.sign;
        if let Some(nan) = nan_result(a, b) {
            return nan;
        }
        if a.kind == Kind::Inf || b.kind == Kind::Inf {
            return if a.kind == Kind::Zero || b.kind == Kind::Zero { self.indefinite() } else { write_inf(sign) };
        }
        if a.kind == Kind::Zero || b.kind == Kind::Zero {
            return write(sign, 0, 0);
        }
        let mut m = u128::from(a.m) * u128::from(b.m);
        let mut exp = a.exp + b.exp + 1;
        if m >> 127 == 0 {
            m <<= 1;
            exp -= 1;
        }
        self.write_rounded(sign, exp, m, false, cw)
    }

    pub(super) fn div(&mut self, pa: &Slot, pb: &Slot, cw: u64) -> Slot {
        let (a, b) = (unpack(pa), unpack(pb));
        let sign = a.sign ^ b.sign;
        if let Some(nan) = nan_result(a, b) {
            return nan;
        }
        if a.kind == Kind::Inf {
            return if b.kind == Kind::Inf { self.indefinite() } else { write_inf(sign) };
        }
        if b.kind == Kind::Inf {
            return write(sign, 0, 0);
        }
        if b.kind == Kind::Zero {
            if a.kind == Kind::Zero {
                return self.indefinite();
            }
            self.raise(ZE);
            return write_inf(sign);
        }
        if a.kind == Kind::Zero {
            return write(sign, 0, 0);
        }
        // The quotient of the significands to 65 bits: the 64 of the result and a guard bit, the
        // remainder as the sticky bit.
        let n = u128::from(a.m) << 64;
        let d = u128::from(b.m);
        let mut q = n / d;
        let mut remainder = n - q * d;
        let mut exp = a.exp - b.exp;
        if a.m < b.m {
            let guard = 2 * remainder >= d;
            remainder = 2 * remainder - if guard { d } else { 0 };
            q = (q << 1) | u128::from(guard);
            exp -= 1;
        }
        self.write_rounded(sign, exp, q << 63, remainder != 0, cw)
    }

    pub(super) fn sqrt(&mut self, pa: &Slot, cw: u64) -> Slot {
        let v = unpack(pa);
        if v.kind == Kind::Nan || v.kind == Kind::Zero {
            return write_special(v);
        }
        if v.sign {
            return self.indefinite();
        }
        if v.kind == Kind::Inf {
            return write_inf(false);
        }
        // The value as m * 2^k with k even, m in [2^126, 2^128), so the root of m has 64 bits; a
        // guard bit and the remainder follow.
        let k = v.exp - 63;
        let odd = k & 1 != 0;
        let mut n = if odd { u128::from(v.m) << 63 } else { u128::from(v.m) << 64 };
        let k2 = (if odd { k - 63 } else { k - 64 }) / 2;
        let mut root = 0u128;
        let mut bit = 1u128 << 126;
        while bit != 0 {
            if n >= root + bit {
                n -= root + bit;
                root = (root >> 1) + bit;
            } else {
                root >>= 1;
            }
            bit >>= 2;
        }
        let guard = n > root;
        let rest = if guard { 4 * n - 4 * root - 1 } else { n };
        let quotient = ((root << 1) | u128::from(guard)) << 63;
        self.write_rounded(false, 63 + k2, quotient, rest != 0, cw)
    }

    pub(super) fn rndint(&mut self, pa: &Slot, cw: u64) -> Slot {
        let v = unpack(pa);
        if v.kind != Kind::Finite {
            return write_special(v);
        }
        if v.exp >= 63 {
            return write(v.sign, (v.exp + 16383) as u32, v.m);
        }
        let q = to_integer(v, rc(cw));
        if has_fraction(v) {
            self.raise(PE);
        }
        if q == 0 {
            return write(v.sign, 0, 0);
        }
        let shift = q.leading_zeros();
        write(v.sign, 16383 + 63 - shift, q << shift)
    }

    pub(super) fn chs(a: &Slot) -> Slot {
        let mut slot = *a;
        slot[9] ^= 0x80;
        slot
    }

    pub(super) fn abs(a: &Slot) -> Slot {
        let mut slot = *a;
        slot[9] &= 0x7f;
        slot
    }

    /// Less in bit 0, equal in bit 1, unordered in bit 2 (then all three, as the hardware sets
    /// its condition codes).
    pub(super) fn cmp(pa: &Slot, pb: &Slot) -> u64 {
        let (a, b) = (unpack(pa), unpack(pb));
        if a.kind == Kind::Nan || b.kind == Kind::Nan {
            return 7;
        }
        if a.kind == Kind::Zero && b.kind == Kind::Zero {
            return 2;
        }
        let order = if a.sign == b.sign {
            // The magnitudes: an infinity beyond everything, a zero below, else by exponent then
            // significand.
            let magnitude = if a.kind == Kind::Inf || b.kind == Kind::Inf {
                i32::from(a.kind == Kind::Inf) - i32::from(b.kind == Kind::Inf)
            } else if a.kind == Kind::Zero || b.kind == Kind::Zero {
                i32::from(b.kind == Kind::Zero) - i32::from(a.kind == Kind::Zero)
            } else if a.exp != b.exp {
                if a.exp > b.exp { 1 } else { -1 }
            } else {
                match a.m.cmp(&b.m) {
                    std::cmp::Ordering::Greater => 1,
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                }
            };
            if a.sign { -magnitude } else { magnitude }
        } else if a.sign {
            -1
        } else {
            1
        };
        match order {
            0 => 2,
            o if o < 0 => 1,
            _ => 0,
        }
    }
}
