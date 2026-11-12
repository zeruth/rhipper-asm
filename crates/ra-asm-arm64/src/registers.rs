//! The `AArch64` registers as the neutral [`Register`]: the 31 general registers with their `w`
//! halves, the stack pointer, the zero register, the program counter, the flags and the 32 vector
//! registers with their scalar views.
//!
//! The ids are the lift's too: `x<n>` is `n`, `sp` is [`SP`], `xzr` is [`ZR`], `pc` is [`PC`],
//! `nzcv` is [`NZCV`]; a `w` register is its `x` plus [`W`]; `v<n>` is [`V`] plus `n`, and its
//! `b`, `h`, `s`, `d` and `q` views add [`B`], [`H`], [`S`], [`D`] and nothing.

use ra_asm_arch::{RegClass, Register};

pub const SP: u16 = 31;
pub const ZR: u16 = 32;
pub const PC: u16 = 33;
pub const NZCV: u16 = 34;
/// The offset of a `w` register's id from its `x` register's.
pub const W: u16 = 64;
/// The id of `v0`; `v<n>` is `V + n`.
pub const V: u16 = 128;
pub const B: u16 = 32;
pub const H: u16 = 64;
pub const S: u16 = 96;
pub const D: u16 = 160;

const X_NAMES: [&str; 31] = [
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x18", "x19",
    "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29", "x30",
];
const W_NAMES: [&str; 31] = [
    "w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10", "w11", "w12", "w13", "w14", "w15", "w16", "w17", "w18", "w19",
    "w20", "w21", "w22", "w23", "w24", "w25", "w26", "w27", "w28", "w29", "w30",
];
const V_NAMES: [&str; 32] = [
    "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19",
    "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
];
const B_NAMES: [&str; 32] = [
    "b0", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "b10", "b11", "b12", "b13", "b14", "b15", "b16", "b17", "b18", "b19",
    "b20", "b21", "b22", "b23", "b24", "b25", "b26", "b27", "b28", "b29", "b30", "b31",
];
const H_NAMES: [&str; 32] = [
    "h0", "h1", "h2", "h3", "h4", "h5", "h6", "h7", "h8", "h9", "h10", "h11", "h12", "h13", "h14", "h15", "h16", "h17", "h18", "h19",
    "h20", "h21", "h22", "h23", "h24", "h25", "h26", "h27", "h28", "h29", "h30", "h31",
];
const S_NAMES: [&str; 32] = [
    "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "s12", "s13", "s14", "s15", "s16", "s17", "s18", "s19",
    "s20", "s21", "s22", "s23", "s24", "s25", "s26", "s27", "s28", "s29", "s30", "s31",
];
const D_NAMES: [&str; 32] = [
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9", "d10", "d11", "d12", "d13", "d14", "d15", "d16", "d17", "d18", "d19",
    "d20", "d21", "d22", "d23", "d24", "d25", "d26", "d27", "d28", "d29", "d30", "d31",
];
const Q_NAMES: [&str; 32] = [
    "q0", "q1", "q2", "q3", "q4", "q5", "q6", "q7", "q8", "q9", "q10", "q11", "q12", "q13", "q14", "q15", "q16", "q17", "q18", "q19",
    "q20", "q21", "q22", "q23", "q24", "q25", "q26", "q27", "q28", "q29", "q30", "q31",
];

/// General register `n` (0..=30) at 64 or 32 bits.
pub fn general(n: u16, bits: u16) -> Register {
    let n = n.min(30);
    if bits == 64 {
        Register { id: n, name: X_NAMES[n as usize], class: RegClass::General, full: n, bits: 64, shift: 0 }
    } else {
        Register { id: n + W, name: W_NAMES[n as usize], class: RegClass::General, full: n, bits: 32, shift: 0 }
    }
}

/// The stack pointer, or its low half `wsp`.
pub fn sp(bits: u16) -> Register {
    if bits == 64 {
        Register { id: SP, name: "sp", class: RegClass::StackPointer, full: SP, bits: 64, shift: 0 }
    } else {
        Register { id: SP + W, name: "wsp", class: RegClass::StackPointer, full: SP, bits: 32, shift: 0 }
    }
}

/// The zero register at either width: reads as zero, swallows a write.
pub fn zero(bits: u16) -> Register {
    if bits == 64 {
        Register { id: ZR, name: "xzr", class: RegClass::Other, full: ZR, bits: 64, shift: 0 }
    } else {
        Register { id: ZR + W, name: "wzr", class: RegClass::Other, full: ZR, bits: 32, shift: 0 }
    }
}

pub fn pc() -> Register {
    Register { id: PC, name: "pc", class: RegClass::InstructionPointer, full: PC, bits: 64, shift: 0 }
}

pub fn nzcv() -> Register {
    Register { id: NZCV, name: "nzcv", class: RegClass::Flags, full: NZCV, bits: 32, shift: 0 }
}

/// Vector register `n` (0..=31) seen at `bits` wide: 8, 16, 32, 64 or 128.
pub fn vector(n: u16, bits: u16) -> Register {
    let n = n.min(31);
    let (offset, names) = match bits {
        8 => (B, &B_NAMES),
        16 => (H, &H_NAMES),
        32 => (S, &S_NAMES),
        64 => (D, &D_NAMES),
        _ => (0, &Q_NAMES),
    };
    Register {
        id: V + offset + n,
        name: names[n as usize],
        class: RegClass::Vector,
        full: V + n,
        bits: if offset == 0 { 128 } else { bits },
        shift: 0,
    }
}

/// The neutral name of a vector register, `v<n>`.
pub fn vector_name(n: u16) -> &'static str {
    V_NAMES[n.min(31) as usize]
}

/// The full register that holds `id`.
pub fn full_of(id: u16) -> Register {
    match id {
        0..=30 => general(id, 64),
        SP => sp(64),
        ZR => zero(64),
        PC => pc(),
        NZCV => nzcv(),
        _ if id >= V => vector(id - V - vector_view_offset(id - V), 128),
        _ => full_of(id - W),
    }
}

fn vector_view_offset(id: u16) -> u16 {
    match id {
        0..=31 => 0,
        32..=63 => B,
        64..=95 => H,
        96..=127 => S,
        _ => D,
    }
}
