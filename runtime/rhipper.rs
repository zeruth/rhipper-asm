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

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentThreadId() -> u32;
    fn GetEnvironmentStringsA() -> *const u8;
}

/// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
/// pointers. A pointer the program holds is a dword, and a function it names is its universal
/// entry point, which takes eight integers and ignores the ones it does not need; the host's own
/// versions would read the tables with 64-bit strides.
type Universal = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

unsafe fn call_universal(function: u32) -> u64 {
    let f: Universal = core::mem::transmute(function as usize);
    f(0, 0, 0, 0, 0, 0, 0, 0)
}

pub unsafe fn ra_x86_initterm(first: u64, last: u64) -> u64 {
    let mut p = first as usize;
    while p < last as usize {
        let f = core::ptr::read_unaligned(p as *const u32);
        if f != 0 {
            call_universal(f);
        }
        p += 4;
    }
    0
}

pub unsafe fn ra_x86_initterm_e(first: u64, last: u64) -> u64 {
    let mut p = first as usize;
    while p < last as usize {
        let f = core::ptr::read_unaligned(p as *const u32);
        if f != 0 {
            let result = call_universal(f) as u32;
            if result != 0 {
                return u64::from(result);
            }
        }
        p += 4;
    }
    0
}

static mut ONEXIT_TABLES: Vec<(u32, Vec<u32>)> = Vec::new();

pub unsafe fn ra_x86_initialize_onexit_table(table: u64) -> u64 {
    if table == 0 {
        return u64::MAX;
    }
    core::ptr::write_bytes(table as usize as *mut u8, 0, 12);
    let tables = &mut *(&raw mut ONEXIT_TABLES);
    match tables.iter_mut().find(|(a, _)| *a == table as u32) {
        Some(entry) => entry.1.clear(),
        None => tables.push((table as u32, Vec::new())),
    }
    0
}

pub unsafe fn ra_x86_register_onexit_function(table: u64, function: u64) -> u64 {
    let tables = &mut *(&raw mut ONEXIT_TABLES);
    match tables.iter_mut().find(|(a, _)| *a == table as u32) {
        Some(entry) => entry.1.push(function as u32),
        None => tables.push((table as u32, vec![function as u32])),
    }
    0
}

pub unsafe fn ra_x86_execute_onexit_table(table: u64) -> u64 {
    let tables = &mut *(&raw mut ONEXIT_TABLES);
    if let Some(entry) = tables.iter_mut().find(|(a, _)| *a == table as u32) {
        while let Some(function) = entry.1.pop() {
            if function != 0 {
                call_universal(function);
            }
        }
    }
    0
}

static mut ARGV32: Vec<u32> = Vec::new();
static mut ARGV32_ADDRESS: u32 = 0;
static mut ARG_STRINGS: Vec<std::ffi::CString> = Vec::new();
static mut ENVIRONMENT32: Vec<u32> = Vec::new();

/// `__p___argv`: a pointer to the dword that holds the address of the dword argv.
pub unsafe fn ra_x86_p_argv() -> u64 {
    let argv = &mut *(&raw mut ARGV32);
    if argv.is_empty() {
        let strings = &mut *(&raw mut ARG_STRINGS);
        for arg in std::env::args() {
            strings.push(std::ffi::CString::new(arg).unwrap_or_default());
        }
        for s in strings.iter() {
            argv.push(s.as_ptr() as usize as u32);
        }
        argv.push(0);
        *(&raw mut ARGV32_ADDRESS) = argv.as_ptr() as usize as u32;
    }
    (&raw const ARGV32_ADDRESS) as usize as u64
}

/// `_get_initial_narrow_environment`: the dword pointers to the environment strings.
pub unsafe fn ra_x86_initial_narrow_environment() -> u64 {
    let env = &mut *(&raw mut ENVIRONMENT32);
    if env.is_empty() {
        let mut p = GetEnvironmentStringsA();
        while *p != 0 {
            env.push(p as usize as u32);
            while *p != 0 {
                p = p.add(1);
            }
            p = p.add(1);
        }
        env.push(0);
    }
    env.as_ptr() as usize as u64
}

/// Fills the thread environment block the lifted code reads through `fs:` (x86) or `gs:` (x64):
/// the fields a program and its C runtime look at, in the layout of the program's pointer width,
/// from the host's own thread. The stack bounds come from the host's block; the block's own
/// address fits the width, since a 32-bit port runs with every address below 4 GB.
pub unsafe fn ra_init_teb(teb: *mut u8, bits: u32) {
    let host: *const u8;
    core::arch::asm!("mov {}, gs:[0x30]", out(reg) host, options(nostack, readonly, preserves_flags));
    let stack_base = core::ptr::read_unaligned(host.add(0x08) as *const u64);
    let stack_limit = core::ptr::read_unaligned(host.add(0x10) as *const u64);
    let (pid, tid) = (GetCurrentProcessId(), GetCurrentThreadId());
    if bits == 32 {
        core::ptr::write_unaligned(teb.add(0x04) as *mut u32, stack_base as u32);
        core::ptr::write_unaligned(teb.add(0x08) as *mut u32, stack_limit as u32);
        core::ptr::write_unaligned(teb.add(0x18) as *mut u32, teb as usize as u32);
        core::ptr::write_unaligned(teb.add(0x20) as *mut u32, pid);
        core::ptr::write_unaligned(teb.add(0x24) as *mut u32, tid);
    } else {
        core::ptr::write_unaligned(teb.add(0x08) as *mut u64, stack_base);
        core::ptr::write_unaligned(teb.add(0x10) as *mut u64, stack_limit);
        core::ptr::write_unaligned(teb.add(0x30) as *mut u64, teb as usize as u64);
        core::ptr::write_unaligned(teb.add(0x40) as *mut u64, u64::from(pid));
        core::ptr::write_unaligned(teb.add(0x48) as *mut u64, u64::from(tid));
    }
}

/// The processor's answers, from the host: the port runs on a processor at least as capable as
/// the one the program asks about.
pub unsafe fn ra_cpuid(leaf: u64, subleaf: u64, out: *mut u8) {
    let r = core::arch::x86_64::__cpuid_count(leaf as u32, subleaf as u32);
    let out = out as *mut u32;
    core::ptr::write_unaligned(out, r.eax);
    core::ptr::write_unaligned(out.add(1), r.ebx);
    core::ptr::write_unaligned(out.add(2), r.ecx);
    core::ptr::write_unaligned(out.add(3), r.edx);
}

#[target_feature(enable = "xsave")]
pub unsafe fn ra_xgetbv(which: u64) -> u64 {
    core::arch::x86_64::_xgetbv(which as u32)
}

pub unsafe fn ra_rdtsc() -> u64 {
    core::arch::x86_64::_rdtsc()
}

/// A jump the structuring could not express: Rust has no `goto`.
pub fn ra_goto(label: &str) -> ! {
    eprintln!("ra: a goto to {label}, which the Rust output cannot express");
    std::process::exit(0xE000_0005u32 as i32)
}
