//! What the interpreter's runtime takes from the operating system: memory at chosen addresses,
//! the functions of the host's DLLs, and the calls into them. Windows answers all of it; another
//! host has no DLLs of the program's to bind, so it gets none of these and the runtime stubs or
//! stops instead.

use std::ffi::c_void;

/// A range of host memory the runtime allocated, released with it.
pub(super) struct Allocation {
    pub(super) start: u64,
    pub(super) len: u64,
}

impl Drop for Allocation {
    fn drop(&mut self) {
        release(self.start);
    }
}

/// The eight-integer shape every import is called with when nothing better is known: the Win64
/// convention's four registers and four stack slots.
pub(super) type Native8 = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    #[repr(C)]
    pub(super) struct MemoryBasicInformation {
        pub base: *mut c_void,
        pub allocation_base: *mut c_void,
        pub allocation_protect: u32,
        pub partition: u16,
        pub region_size: usize,
        pub state: u32,
        pub protect: u32,
        pub kind: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
        pub(super) fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
        pub(super) fn VirtualQuery(address: *const c_void, info: *mut MemoryBasicInformation, len: usize) -> usize;
        pub(super) fn LoadLibraryA(name: *const u8) -> usize;
        pub(super) fn GetModuleHandleA(name: *const u8) -> usize;
        pub(super) fn GetProcAddress(module: usize, name: *const u8) -> usize;
        pub(super) fn GetCurrentProcessId() -> u32;
        pub(super) fn GetCurrentThreadId() -> u32;
        pub(super) fn GetStdHandle(which: u32) -> usize;
    }

    pub(super) const MEM_COMMIT: u32 = 0x1000;
    pub(super) const MEM_RESERVE: u32 = 0x2000;
    pub(super) const MEM_RELEASE: u32 = 0x8000;
    pub(super) const PAGE_READWRITE: u32 = 0x04;
    pub(super) const PAGE_NOACCESS: u32 = 0x01;
    pub(super) const PAGE_GUARD: u32 = 0x100;
}

/// Memory at `base` exactly, or `None` when the process has something there.
#[cfg(windows)]
pub(super) fn allocate_at(base: u64, len: u64) -> Option<Allocation> {
    // Reserve before committing: a commit at an address succeeds on pages anyone reserved there,
    // a reservation only on free ones.
    let p = unsafe { sys::VirtualAlloc(base as usize as *mut c_void, len as usize, sys::MEM_RESERVE, sys::PAGE_NOACCESS) };
    if p as usize as u64 != base || base == 0 {
        return None;
    }
    let c = unsafe { sys::VirtualAlloc(p, len as usize, sys::MEM_COMMIT, sys::PAGE_READWRITE) };
    if c.is_null() {
        release(base);
        return None;
    }
    Some(Allocation { start: base, len })
}

#[cfg(not(windows))]
pub(super) fn allocate_at(_base: u64, _len: u64) -> Option<Allocation> {
    None
}

/// Memory below 4 GB, where a 32-bit program's pointers reach: the first free spot of a walk
/// from 256 MB up in 16 MB steps.
pub(super) fn allocate_low(len: u64) -> Option<Allocation> {
    (0x1000_0000u64..0x8000_0000).step_by(0x0100_0000).find_map(|base| allocate_at(base, len))
}

#[cfg(windows)]
fn release(start: u64) {
    unsafe { sys::VirtualFree(start as usize as *mut c_void, 0, sys::MEM_RELEASE) };
}

#[cfg(not(windows))]
fn release(_start: u64) {}

/// The committed, readable range of the process that holds `address`, when there is one: a page
/// a DLL handed the program.
#[cfg(windows)]
pub(super) fn committed(address: u64) -> Option<(u64, u64)> {
    let mut info = std::mem::MaybeUninit::<sys::MemoryBasicInformation>::uninit();
    let n = unsafe { sys::VirtualQuery(address as usize as *const c_void, info.as_mut_ptr(), size_of::<sys::MemoryBasicInformation>()) };
    if n == 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let readable = info.state == sys::MEM_COMMIT && info.protect & (sys::PAGE_NOACCESS | sys::PAGE_GUARD) == 0 && info.protect != 0;
    readable.then_some((info.base as usize as u64, info.region_size as u64))
}

#[cfg(not(windows))]
pub(super) fn committed(_address: u64) -> Option<(u64, u64)> {
    None
}

/// The handle of a DLL, loaded if need be.
#[cfg(windows)]
pub(super) fn library(name: &str) -> Option<usize> {
    let c = std::ffi::CString::new(name).ok()?;
    let h = unsafe { sys::GetModuleHandleA(c.as_ptr().cast()) };
    let h = if h == 0 { unsafe { sys::LoadLibraryA(c.as_ptr().cast()) } } else { h };
    (h != 0).then_some(h)
}

#[cfg(not(windows))]
pub(super) fn library(_name: &str) -> Option<usize> {
    None
}

/// A function of a DLL, by name or by ordinal.
#[cfg(windows)]
pub(super) fn procedure(module: usize, name: &str) -> Option<usize> {
    let p = if let Some(ordinal) = name.strip_prefix('#') {
        unsafe { sys::GetProcAddress(module, ordinal.parse::<usize>().ok()? as *const u8) }
    } else {
        let c = std::ffi::CString::new(name).ok()?;
        unsafe { sys::GetProcAddress(module, c.as_ptr().cast()) }
    };
    (p != 0).then_some(p)
}

#[cfg(not(windows))]
pub(super) fn procedure(_module: usize, _name: &str) -> Option<usize> {
    None
}

/// The process and thread of the host, for the thread environment block.
#[cfg(windows)]
pub(super) fn ids() -> (u32, u32) {
    unsafe { (sys::GetCurrentProcessId(), sys::GetCurrentThreadId()) }
}

#[cfg(not(windows))]
pub(super) fn ids() -> (u32, u32) {
    (std::process::id(), 1)
}

/// The host's standard handle: input, output or error by `which` (-10, -11, -12).
#[cfg(windows)]
pub(super) fn std_handle(which: u32) -> u64 {
    unsafe { sys::GetStdHandle(which) as u64 }
}

#[cfg(not(windows))]
pub(super) fn std_handle(which: u32) -> u64 {
    u64::from(which.wrapping_neg() - 10)
}

/// Calls a host function with eight integers.
///
/// # Safety
///
/// `f` must be a function of the host that takes at most eight integer arguments and returns an
/// integer or nothing, and the arguments must be what it expects.
pub(super) unsafe fn call8(f: usize, a: &[u64; 8]) -> u64 {
    let f: Native8 = unsafe { std::mem::transmute(f) };
    unsafe { f(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]) }
}

/// A host call with floating-point arguments or result, by the shape of its prototype: one
/// letter per parameter (`i` an integer, `d` a double, `f` a float) and the result after `>`.
/// The shapes are the ones the import prototype table has; another returns `None`.
///
/// # Safety
///
/// `f` must be a host function of the shape.
#[allow(clippy::many_single_char_names)]
pub(super) unsafe fn call_shaped(f: usize, shape: &str, ints: &[u64], floats: &[f64]) -> Option<Floated> {
    let i = |n: usize| ints.get(n).copied().unwrap_or(0);
    let d = |n: usize| floats.get(n).copied().unwrap_or(0.0);
    unsafe {
        Some(match shape {
            "d>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(f64) -> f64>(f)(d(0))),
            "dd>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(f64, f64) -> f64>(f)(d(0), d(1))),
            "ddd>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(f64, f64, f64) -> f64>(f)(d(0), d(1), d(2))),
            "f>f" => Floated::Float(std::mem::transmute::<usize, unsafe extern "C" fn(f32) -> f32>(f)(d(0) as f32)),
            "ff>f" => Floated::Float(std::mem::transmute::<usize, unsafe extern "C" fn(f32, f32) -> f32>(f)(d(0) as f32, d(1) as f32)),
            "d>i" => Floated::Int(std::mem::transmute::<usize, unsafe extern "C" fn(f64) -> u64>(f)(d(0))),
            "f>i" => Floated::Int(std::mem::transmute::<usize, unsafe extern "C" fn(f32) -> u64>(f)(d(0) as f32)),
            "i>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(u64) -> f64>(f)(i(0))),
            "ii>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(u64, u64) -> f64>(f)(i(0), i(1))),
            "di>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(f64, u64) -> f64>(f)(d(0), i(0))),
            "id>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(u64, f64) -> f64>(f)(i(0), d(0))),
            "fi>f" => Floated::Float(std::mem::transmute::<usize, unsafe extern "C" fn(f32, u64) -> f32>(f)(d(0) as f32, i(0))),
            "ddi>d" => Floated::Double(std::mem::transmute::<usize, unsafe extern "C" fn(f64, f64, u64) -> f64>(f)(d(0), d(1), i(0))),
            _ => return None,
        })
    }
}

/// The result of a shaped call.
pub(super) enum Floated {
    Int(u64),
    Float(f32),
    Double(f64),
}
