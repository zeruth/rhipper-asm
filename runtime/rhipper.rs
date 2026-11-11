//! The runtime module of a program ras emit writes as Rust: the aligned storage of the data and
//! the frames, the vector helpers the lift's packed operations map to, the funnel shifts, and
//! the trap that stops the port at an instruction the lift did not cover.

#![allow(dead_code, non_camel_case_types, clippy::all)]

/// Storage aligned to 16 bytes: the data sections, the frames and the slots. A packed structure
/// of a data section goes inside it, since one type cannot be both packed and aligned.
#[repr(C, align(16))]
pub struct Aligned<T>(pub T);

pub type v16u8 = [u8; 16];
pub type v8u16 = [u16; 8];
pub type v4u32 = [u32; 4];
pub type v2u64 = [u64; 2];
pub type v4f32 = [f32; 4];
pub type v2f64 = [f64; 2];

/// An operation lane by lane.
#[inline(always)]
pub fn vmap<T: Copy, const N: usize>(a: [T; N], f: impl Fn(T) -> T) -> [T; N] {
    let mut r = a;
    for i in 0..N {
        r[i] = f(a[i]);
    }
    r
}

/// An operation lane by lane over two vectors.
#[inline(always)]
pub fn vbin<T: Copy, const N: usize>(a: [T; N], b: [T; N], f: impl Fn(T, T) -> T) -> [T; N] {
    let mut r = a;
    for i in 0..N {
        r[i] = f(a[i], b[i]);
    }
    r
}

/// A comparison lane by lane: all ones where it holds, as the hardware masks are.
#[inline(always)]
pub fn vcmp<T: Copy, U: Copy, const N: usize>(a: [T; N], b: [T; N], all_ones: U, zero: U, f: impl Fn(T, T) -> bool) -> [U; N] {
    let mut r = [zero; N];
    for i in 0..N {
        r[i] = if f(a[i], b[i]) { all_ones } else { zero };
    }
    r
}

/// A conversion lane by lane into another lane type.
#[inline(always)]
pub fn vcast<T: Copy, U: Copy + Default, const N: usize>(a: [T; N], f: impl Fn(T) -> U) -> [U; N] {
    let mut r = [U::default(); N];
    for i in 0..N {
        r[i] = f(a[i]);
    }
    r
}

/// `shufflevector`: lane `i` of the result is lane `mask[i]` of `a` and `b` laid end to end; a
/// negative mask entry is any value.
#[inline(always)]
pub fn shuffle<T: Copy + Default, const N: usize, const M: usize>(a: [T; N], b: [T; N], mask: [i32; M]) -> [T; M] {
    let mut r = [T::default(); M];
    for i in 0..M {
        let k = mask[i];
        r[i] = if k < 0 {
            T::default()
        } else if (k as usize) < N {
            a[k as usize]
        } else {
            b[k as usize - N]
        };
    }
    r
}

#[inline(always)]
pub fn insert<T: Copy, const N: usize>(a: [T; N], index: usize, element: T) -> [T; N] {
    let mut r = a;
    r[index] = element;
    r
}

macro_rules! funnel {
    ($name_l:ident, $name_r:ident, $t:ty, $bits:expr) => {
        #[inline(always)]
        pub fn $name_l(a: $t, b: $t, c: $t) -> $t {
            let c = (c as u32) & ($bits - 1);
            if c == 0 { a } else { (a << c) | (b >> ($bits - c)) }
        }
        #[inline(always)]
        pub fn $name_r(a: $t, b: $t, c: $t) -> $t {
            let c = (c as u32) & ($bits - 1);
            if c == 0 { b } else { (a << ($bits - c)) | (b >> c) }
        }
    };
}
funnel!(fshl_i8, fshr_i8, u8, 8);
funnel!(fshl_i16, fshr_i16, u16, 16);
funnel!(fshl_i32, fshr_i32, u32, 32);
funnel!(fshl_i64, fshr_i64, u64, 64);

/// The port reached an instruction the lift does not cover, or a check of the lift failed:
/// the address and the code name it, and the process ends as the C runtime ends it.
pub fn ra_trap(address: u64, code: u64) -> ! {
    let name = match code {
        1 => "a breakpoint",
        2 => "an undefined instruction",
        3 => "a halt",
        4 => "a division by zero",
        _ => "a trap",
    };
    eprintln!("ra: {name} at {address:#x}");
    std::process::exit(0xE000_0001u32 as i32)
}

pub fn ra_unsupported(address: u64, code: u64) -> ! {
    eprintln!("ra: an instruction the lift does not cover at {address:#x} (code {code})");
    std::process::exit(0xE000_0002u32 as i32)
}

/// A jump the structuring could not express: Rust has no `goto`.
pub fn ra_goto(label: &str) -> ! {
    eprintln!("ra: a goto to {label}, which the Rust output cannot express");
    std::process::exit(0xE000_0005u32 as i32)
}
