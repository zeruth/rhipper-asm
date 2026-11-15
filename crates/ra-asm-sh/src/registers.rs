//! The SH-4 registers by the decoder's numbers: the sixteen general registers, the bank of `r0` to
//! `r7` that `sr.rb` switches out, the control and system registers, the bits of `sr` the
//! instructions name, and both banks of the FPU.
//!
//! A floating-point register is a part of a wider one: `fr0` to `fr3` are the vector `fv0`, and the
//! pair `dr0` is `fr0` above `fr1`, as the double mode reads them. The back bank is the matrix
//! `xmtrx`, whose parts are `xf0` to `xf15` and the pairs `xd0` to `xd14`.

use ra_asm_arch::{RegClass, Register};

/// `r0`; `r15` is the stack pointer.
pub const R0: u16 = 0;
pub const SP: u16 = 15;
/// `r0_bank` to `r7_bank`.
pub const BANK0: u16 = 16;
pub const SR: u16 = 24;
pub const GBR: u16 = 25;
pub const VBR: u16 = 26;
pub const SSR: u16 = 27;
pub const SPC: u16 = 28;
pub const SGR: u16 = 29;
pub const DBR: u16 = 30;
pub const MACH: u16 = 31;
pub const MACL: u16 = 32;
pub const PR: u16 = 33;
pub const PC: u16 = 34;
pub const FPSCR: u16 = 35;
pub const FPUL: u16 = 36;
/// The bits of `sr`: `t`, `s`, `q` and `m`.
pub const T: u16 = 37;
pub const S: u16 = 38;
pub const Q: u16 = 39;
pub const M: u16 = 40;
/// `fv0`, `fv4`, `fv8` and `fv12`.
pub const FV0: u16 = 41;
/// `fr0` to `fr15`.
pub const FR0: u16 = 45;
/// `dr0`, `dr2` to `dr14`.
pub const DR0: u16 = 61;
pub const XMTRX: u16 = 69;
/// `xf0` to `xf15`.
pub const XF0: u16 = 70;
/// `xd0`, `xd2` to `xd14`.
pub const XD0: u16 = 86;
pub const COUNT: usize = 94;

const GENERAL: [&str; 16] = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
const BANKS: [&str; 8] = ["r0_bank", "r1_bank", "r2_bank", "r3_bank", "r4_bank", "r5_bank", "r6_bank", "r7_bank"];
/// From `sr` to `fpul`, in the order of their numbers.
const SYSTEM: [&str; 13] = ["sr", "gbr", "vbr", "ssr", "spc", "sgr", "dbr", "mach", "macl", "pr", "pc", "fpscr", "fpul"];
const SR_BITS: [(&str, u16); 4] = [("t", 0), ("s", 1), ("q", 8), ("m", 9)];
const VECTORS: [&str; 4] = ["fv0", "fv4", "fv8", "fv12"];
const SINGLES: [&str; 16] =
    ["fr0", "fr1", "fr2", "fr3", "fr4", "fr5", "fr6", "fr7", "fr8", "fr9", "fr10", "fr11", "fr12", "fr13", "fr14", "fr15"];
const DOUBLES: [&str; 8] = ["dr0", "dr2", "dr4", "dr6", "dr8", "dr10", "dr12", "dr14"];
const BACK_SINGLES: [&str; 16] =
    ["xf0", "xf1", "xf2", "xf3", "xf4", "xf5", "xf6", "xf7", "xf8", "xf9", "xf10", "xf11", "xf12", "xf13", "xf14", "xf15"];
const BACK_DOUBLES: [&str; 8] = ["xd0", "xd2", "xd4", "xd6", "xd8", "xd10", "xd12", "xd14"];

const fn full(id: u16, name: &'static str, class: RegClass, bits: u16) -> Register {
    Register { id, name, class, full: id, bits, shift: 0 }
}

const fn part(id: u16, name: &'static str, class: RegClass, full: u16, bits: u16, shift: u16) -> Register {
    Register { id, name, class, full, bits, shift }
}

/// Where `fr<n>` sits in the 128 bits of its vector, or `xf<n>` in the 512 of the matrix: the pair
/// `n / 2` takes 64 bits, the even register the upper half of them.
const fn single_shift(n: u16) -> u16 {
    64 * (n / 2) + if n.is_multiple_of(2) { 32 } else { 0 }
}

const fn entry(id: u16) -> Register {
    let i = id as usize;
    match id {
        0..=14 => full(id, GENERAL[i], RegClass::General, 32),
        SP => full(id, GENERAL[i], RegClass::StackPointer, 32),
        16..=23 => full(id, BANKS[i - 16], RegClass::General, 32),
        SR => full(id, SYSTEM[0], RegClass::Flags, 32),
        PC => full(id, SYSTEM[i - 24], RegClass::InstructionPointer, 32),
        FPUL => full(id, SYSTEM[i - 24], RegClass::Float, 32),
        25..=35 => full(id, SYSTEM[i - 24], RegClass::Other, 32),
        37..=40 => {
            let (name, shift) = SR_BITS[i - 37];
            part(id, name, RegClass::Flags, SR, 1, shift)
        }
        41..=44 => full(id, VECTORS[i - 41], RegClass::Vector, 128),
        45..=60 => {
            let n = id - FR0;
            part(id, SINGLES[n as usize], RegClass::Float, FV0 + n / 4, 32, single_shift(n % 4))
        }
        61..=68 => {
            let j = id - DR0;
            part(id, DOUBLES[j as usize], RegClass::Float, FV0 + j / 2, 64, 64 * (j % 2))
        }
        XMTRX => full(id, "xmtrx", RegClass::Vector, 512),
        70..=85 => {
            let n = id - XF0;
            part(id, BACK_SINGLES[n as usize], RegClass::Float, XMTRX, 32, single_shift(n))
        }
        _ => {
            let j = id - XD0;
            part(id, BACK_DOUBLES[j as usize], RegClass::Float, XMTRX, 64, 64 * j)
        }
    }
}

const fn table() -> [Register; COUNT] {
    let mut registers = [full(0, "", RegClass::General, 32); COUNT];
    let mut id: u16 = 0;
    while (id as usize) < COUNT {
        registers[id as usize] = entry(id);
        id += 1;
    }
    registers
}

/// Every register, at its number.
pub static REGISTERS: [Register; COUNT] = table();

/// The register with the decoder's number `id`.
pub fn register(id: u16) -> Register {
    REGISTERS[usize::from(id)]
}

/// The general register `r<n>`.
pub fn general(n: u16) -> Register {
    register(R0 + n)
}

/// `r<n>_bank`, the register of the other bank.
pub fn bank(n: u16) -> Register {
    register(BANK0 + n)
}

/// `fr<n>`.
pub fn single(n: u16) -> Register {
    register(FR0 + n)
}

/// `dr<n>`, for an even `n`.
pub fn double(n: u16) -> Register {
    register(DR0 + n / 2)
}

/// `xd<n>`, for an even `n`.
pub fn back_double(n: u16) -> Register {
    register(XD0 + n / 2)
}

/// `fv<n>`, for a multiple of four.
pub fn vector(n: u16) -> Register {
    register(FV0 + n / 4)
}
