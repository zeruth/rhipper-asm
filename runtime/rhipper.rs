//! The runtime module of a program ras emit writes as Rust: the aligned storage of the data and
//! the frames, the vector helpers the lift's packed operations map to, the funnel shifts, and
//! the trap that stops the port at an instruction the lift did not cover.

#![allow(dead_code, non_camel_case_types, clippy::all)]

/// Storage aligned to 16 bytes: the data sections, the frames and the slots. A packed structure
/// of a data section goes inside it, since one type cannot be both packed and aligned.
#[repr(C, align(16))]
pub struct Aligned<T>(pub T);

/// A data section of the image, a table of the runtime laid out as the program reads it, or a
/// local a function keeps as bytes: read and written at any offset as any plain value, as the
/// machine does. Every bit pattern is a value of a plain type and the port's threads are the
/// program's own, so the accessors are safe Rust; an access past the end stops the port where
/// the machine would have read what lies beyond.
#[repr(C, align(16))]
pub struct Section<T>(core::cell::UnsafeCell<T>);

unsafe impl<T> Sync for Section<T> {}

impl<T> Section<T> {
    pub const fn new(bytes: T) -> Self {
        Self(core::cell::UnsafeCell::new(bytes))
    }

    /// The address of the section, for the program's own pointer arithmetic.
    pub fn ptr(&self) -> *mut u8 {
        self.0.get().cast()
    }

    /// The value at byte `at`.
    #[track_caller]
    pub fn get<U: Pod>(&self, at: usize) -> U {
        bounds(at, core::mem::size_of::<U>(), core::mem::size_of::<T>());
        unsafe { core::ptr::read_unaligned(self.ptr().add(at).cast()) }
    }

    /// Writes `value` at byte `at`.
    #[track_caller]
    pub fn set<U: Pod>(&self, at: usize, value: U) {
        bounds(at, core::mem::size_of::<U>(), core::mem::size_of::<T>());
        unsafe { core::ptr::write_unaligned(self.ptr().add(at).cast(), value) }
    }

    /// Element `index` of the array of `U` at byte `base`. The address wraps as the machine's
    /// does, and the bound is the section's: the machine reads past an array into what follows.
    #[track_caller]
    pub fn at<U: Pod>(&self, base: usize, index: isize) -> U {
        self.get(element::<U>(base, index))
    }

    /// Writes `value` as element `index` of the array of `U` at byte `base`.
    #[track_caller]
    pub fn put<U: Pod>(&self, base: usize, index: isize, value: U) {
        self.set(element::<U>(base, index), value)
    }
}

/// The byte offset of element `index` of an array of `U` at `base`; a negative one is past the
/// end as an unsigned offset, so the access stops.
fn element<U>(base: usize, index: isize) -> usize {
    base.wrapping_add_signed(index.wrapping_mul(core::mem::size_of::<U>() as isize))
}

/// A type every bit pattern is a value of: the integers, the floats, the raw pointers and
/// arrays of them. What a section or a local holds is read as one of these.
pub unsafe trait Pod: Copy {}

macro_rules! pod {
    ($($t:ty),*) => { $(unsafe impl Pod for $t {})* };
}
pod!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, *mut u8, *const u8);
unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}

/// Stops the port at an access past the end, naming the line of the translation: a panic would
/// abort through the `extern "C"` frame without one.
#[track_caller]
fn bounds(at: usize, size: usize, len: usize) {
    if at.checked_add(size).is_none_or(|end| end > len) {
        let at_line = core::panic::Location::caller();
        eprintln!("ra: an access of {size} bytes at offset {at} of {len} at {at_line}");
        std::process::exit(0xE000_0006u32 as i32)
    }
}

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

/// A float to a signed integer as `cvttsd2si` and `fist` make it: truncated, and the integer
/// indefinite (the minimum) when it does not fit or is NaN, where `as` would saturate.
pub fn fptosi16(v: f64) -> i16 {
    if v > -32769.0 && v < 32768.0 { v as i16 } else { i16::MIN }
}

pub fn fptosi32(v: f64) -> i32 {
    if v > -2147483649.0 && v < 2147483648.0 { v as i32 } else { i32::MIN }
}

pub fn fptosi64(v: f64) -> i64 {
    if v >= -9223372036854775808.0 && v < 9223372036854775808.0 { v as i64 } else { i64::MIN }
}

/// The port reached an instruction the lift does not cover, or a check of the lift failed:
/// the address and the code name it, and the process ends as the C runtime ends it.
pub fn ra_trap(address: u64, code: u64) -> ! {
    let name = match code {
        1 => "a breakpoint",
        2 => "an undefined instruction",
        3 => "a halt",
        4 => "a division by zero",
        10 => "a call through a vtable slot to a function outside the vtables",
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
    fn LoadLibraryA(name: *const u8) -> usize;
    fn LoadLibraryW(name: *const u16) -> usize;
    fn LoadLibraryExA(name: *const u8, file: usize, flags: u32) -> usize;
    fn LoadLibraryExW(name: *const u16, file: usize, flags: u32) -> usize;
    fn GetModuleHandleA(name: *const u8) -> usize;
    fn GetModuleHandleW(name: *const u16) -> usize;
    fn GetProcAddress(module: usize, name: usize) -> usize;
    fn VirtualQuery(address: usize, info: *mut MemoryInfo, size: usize) -> usize;
    fn WaitForMultipleObjects(count: u32, handles: *const usize, all: i32, milliseconds: u32) -> u32;
    fn InitializeCriticalSection(section: *mut CriticalSection);
    fn EnterCriticalSection(section: *mut CriticalSection);
    fn LeaveCriticalSection(section: *mut CriticalSection);
    fn TryEnterCriticalSection(section: *mut CriticalSection) -> i32;
    fn DeleteCriticalSection(section: *mut CriticalSection);
}

/// The host's 40-byte `CRITICAL_SECTION`.
#[repr(C, align(8))]
struct CriticalSection([u8; 40]);

/// The host sections the runtime keeps for a 32-bit program's 24-byte ones: the lock is one place
/// every thread sees, and the program's section holds its slot under a tag.
const CS32_SLOTS: usize = 256;
const CS32_TAG: u32 = 0x5243_0000;
static mut CS32: [CriticalSection; CS32_SLOTS] = [const { CriticalSection([0; 40]) }; CS32_SLOTS];
static CS32_USED: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

unsafe fn cs32(program: u64) -> *mut CriticalSection {
    let tag = *(program as usize as *const u32);
    let slot = (tag & 0xffff) as usize;
    if tag & 0xffff_0000 != CS32_TAG || slot >= CS32_SLOTS {
        eprintln!("ra: a critical section the runtime did not initialize: {tag:#x}");
        std::process::exit(0xE000_0005u32 as i32);
    }
    &raw mut CS32[slot]
}

/// `InitializeCriticalSection` of a 32-bit program's section.
pub unsafe fn ra_x86_initialize_critical_section(program: u64) -> u64 {
    let slot = CS32_USED.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    if slot >= CS32_SLOTS {
        eprintln!("ra: more critical sections than the runtime keeps");
        std::process::exit(0xE000_0005u32 as i32);
    }
    InitializeCriticalSection(&raw mut CS32[slot]);
    core::ptr::write_bytes(program as usize as *mut u8, 0, 24);
    *(program as usize as *mut u32) = CS32_TAG | slot as u32;
    0
}

/// `InitializeCriticalSectionAndSpinCount`: the spin count is the host's to choose.
pub unsafe fn ra_x86_initialize_critical_section_and_spin_count(program: u64, _spin: u64) -> u64 {
    ra_x86_initialize_critical_section(program);
    1
}

/// `InitializeCriticalSectionEx`.
pub unsafe fn ra_x86_initialize_critical_section_ex(program: u64, _spin: u64, _flags: u64) -> u64 {
    ra_x86_initialize_critical_section(program);
    1
}

/// `EnterCriticalSection`.
pub unsafe fn ra_x86_enter_critical_section(program: u64) -> u64 {
    EnterCriticalSection(cs32(program));
    0
}

/// `LeaveCriticalSection`.
pub unsafe fn ra_x86_leave_critical_section(program: u64) -> u64 {
    LeaveCriticalSection(cs32(program));
    0
}

/// `TryEnterCriticalSection`.
pub unsafe fn ra_x86_try_enter_critical_section(program: u64) -> u64 {
    u64::from(TryEnterCriticalSection(cs32(program)) as u32)
}

/// `DeleteCriticalSection`.
pub unsafe fn ra_x86_delete_critical_section(program: u64) -> u64 {
    DeleteCriticalSection(cs32(program));
    0
}

/// `MEMORY_BASIC_INFORMATION` of the 64-bit host.
#[repr(C)]
#[derive(Default)]
struct MemoryInfo {
    base: usize,
    allocation_base: usize,
    allocation_protect: u32,
    partition: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

/// An import by ordinal has no symbol to link: the host's function, found at startup.
pub unsafe fn ra_resolve_ordinal(dll: &str, ordinal: u32) -> usize {
    let name = std::ffi::CString::new(dll).unwrap();
    let module = LoadLibraryA(name.as_ptr().cast());
    let function = if module != 0 { GetProcAddress(module, ordinal as usize) } else { 0 };
    if function == 0 {
        eprintln!("ra: {dll} has no ordinal {ordinal}");
        std::process::exit(0xE000_0004u32 as i32);
    }
    function
}

/// The binding table of a 32-bit program's imports, as `ra_import_pops` holds it: three words per
/// import, the linked symbol, its DLL and name as `dll\0name\0` (zero for a function of the host's
/// runtime) and the bytes a call to it pops, then a zero triple; and the DLL's own address of each
/// name, which a bound slot or `GetProcAddress` hands the program, resolved at startup. `POPS` is
/// the port's `ra_pops`, which a universal entry point writes.
static mut IMPORT_POPS: *const u64 = core::ptr::null();
static mut IMPORT_ADDRESSES: [u64; 1024] = [0; 1024];
static mut IMPORT_COUNT: usize = 0;
static mut POPS: *mut u64 = core::ptr::null_mut();

pub unsafe fn ra_init_import_pops(table: *const u64, pops: *mut u64) {
    IMPORT_POPS = table;
    POPS = pops;
    let mut i = 0;
    while core::ptr::read_unaligned(table.add(3 * i)) != 0 && i < 1024 {
        let names = core::ptr::read_unaligned(table.add(3 * i + 1)) as usize;
        if names != 0 {
            let dll = core::ffi::CStr::from_ptr(names as *const core::ffi::c_char);
            let name = core::ffi::CStr::from_ptr((names + dll.to_bytes().len() + 1) as *const core::ffi::c_char);
            let module = LoadLibraryA(dll.as_ptr().cast());
            if module != 0 {
                let text = name.to_bytes();
                let by = if text.first() == Some(&b'#') {
                    core::str::from_utf8(&text[1..]).ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0)
                } else {
                    name.as_ptr() as usize
                };
                IMPORT_ADDRESSES[i] = GetProcAddress(module, by) as u64;
            }
        }
        i += 1;
    }
    IMPORT_COUNT = i;
}

/// What the x86 function last called through a pointer popped beyond its return address: an
/// import's pops from the binding table, else what its universal entry point recorded in
/// `ra_pops`. The record is consumed, so a native function outside the table reads as popping
/// nothing rather than what an earlier callee popped.
pub unsafe fn ra_pops_of(target: u64) -> u64 {
    let recorded = if POPS.is_null() {
        0
    } else {
        let v = *POPS;
        *POPS = 0;
        v
    };
    for i in 0..IMPORT_COUNT {
        let entry = IMPORT_POPS.add(3 * i);
        if core::ptr::read_unaligned(entry) == target || (IMPORT_ADDRESSES[i] != 0 && IMPORT_ADDRESSES[i] == target) {
            return core::ptr::read_unaligned(entry.add(2));
        }
    }
    recorded
}

/// The value of an ordinal import before `ra_init` resolved it.
pub unsafe extern "system" fn ra_unresolved(_: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
    eprintln!("ra: an import by ordinal called before it was resolved");
    std::process::exit(0xE000_0004u32 as i32)
}

/// A C++ exception: the thrown object's copy and the types it can be caught as (key,
/// displacement and size of each). The lift writes the frame handler's walk into the code as a
/// protocol over these functions: one exception is in flight from its throw until a handler takes
/// it, suspended while the destructors of the way out run, staged between its match and the
/// start of its catch body, and caught while the body runs.
struct Exception {
    object: Vec<u8>,
    types: Vec<(u64, u64, u64)>,
    /// The x86 frame handler's protocol: each catchable type's copy constructor, and the thrown
    /// object's destructor, which the end of its last catch runs.
    copies: Vec<u64>,
    destructor: u64,
}

#[derive(Default)]
struct Exceptions {
    in_flight: Option<std::rc::Rc<Exception>>,
    staged: Option<std::rc::Rc<Exception>>,
    caught: Vec<std::rc::Rc<Exception>>,
    /// The esp the x86 handler saved as each catch began, beside `caught`.
    catch_sp: Vec<u64>,
    suspended: Vec<Option<std::rc::Rc<Exception>>>,
}

thread_local! {
    static EH: std::cell::RefCell<Exceptions> = std::cell::RefCell::new(Exceptions::default());
}

fn eh_fatal(what: &str) -> ! {
    eprintln!("ra: {what}");
    std::process::exit(0xE000_0006u32 as i32)
}

pub unsafe fn ra_eh_throw(object: u64, size: u64) {
    let copy = std::slice::from_raw_parts(object as usize as *const u8, size as usize).to_vec();
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        if eh.in_flight.is_some() {
            eh_fatal("a throw while an exception is in flight");
        }
        eh.in_flight = Some(std::rc::Rc::new(Exception { object: copy, types: Vec::new(), copies: Vec::new(), destructor: 0 }));
    });
}

pub unsafe fn ra_eh_throw_type(key: u64, displacement: u64, size: u64) {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let Some(e) = eh.in_flight.as_mut().and_then(std::rc::Rc::get_mut) else { eh_fatal("a catchable type with no throw to belong to") };
        e.types.push((key, displacement, size));
        e.copies.push(0);
    });
}

pub unsafe fn ra_eh_throw_copy(copy: u64) {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let Some(slot) = eh.in_flight.as_mut().and_then(std::rc::Rc::get_mut).and_then(|e| e.copies.last_mut()) else {
            eh_fatal("a copy constructor with no catchable type to belong to")
        };
        *slot = copy;
    });
}

pub unsafe fn ra_eh_throw_destructor(destructor: u64) {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let Some(e) = eh.in_flight.as_mut().and_then(std::rc::Rc::get_mut) else { eh_fatal("a destructor with no throw to belong to") };
        e.destructor = destructor;
    });
}

/// The object and the destructor of the catch now ending, the destructor 0 when the exception is
/// held elsewhere (in flight again, staged, caught further out or suspended).
fn ending(eh: &Exceptions) -> (u64, u64) {
    let Some(e) = eh.caught.last() else { eh_fatal("the end of a catch body with no exception caught") };
    let same = |other: &Option<std::rc::Rc<Exception>>| other.as_ref().is_some_and(|o| std::rc::Rc::ptr_eq(o, e));
    let held = same(&eh.in_flight)
        || same(&eh.staged)
        || eh.caught[..eh.caught.len() - 1].iter().any(|o| std::rc::Rc::ptr_eq(o, e))
        || eh.suspended.iter().any(same);
    (e.object.as_ptr() as usize as u64, if held { 0 } else { e.destructor })
}

pub unsafe fn ra_eh_ending_object() -> u64 {
    EH.with(|eh| ending(&eh.borrow()).0)
}

pub unsafe fn ra_eh_ending_destructor() -> u64 {
    EH.with(|eh| ending(&eh.borrow()).1)
}

pub unsafe fn ra_eh_catch_sp(sp: u64) {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let Some(slot) = eh.catch_sp.last_mut() else { eh_fatal("a saved esp with no catch being run") };
        *slot = sp;
    });
}

pub unsafe fn ra_eh_ending_sp() -> u64 {
    EH.with(|eh| match eh.borrow().catch_sp.last() {
        Some(&sp) => sp,
        None => eh_fatal("the end of a catch body with no exception caught"),
    })
}

pub unsafe fn ra_eh_rethrow() {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        if eh.in_flight.is_some() {
            eh_fatal("a rethrow while an exception is in flight");
        }
        let Some(e) = eh.caught.last().cloned() else { eh_fatal("a rethrow with no exception caught") };
        eh.in_flight = Some(e);
    });
}

pub unsafe fn ra_eh_pending() -> u64 {
    EH.with(|eh| u64::from(eh.borrow().in_flight.is_some()))
}

pub unsafe fn ra_eh_catches(key: u64, slot: u64, flags: u64) -> u64 {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let Some(e) = eh.in_flight.clone() else { eh_fatal("a catch with no exception in flight") };
        let Some(&(_, displacement, size)) = e.types.iter().find(|t| key == 0 || t.0 == key) else { return 0 };
        if slot != 0 {
            let subobject = e.object.as_ptr().add(displacement as usize);
            if flags & 8 != 0 {
                // By reference: the slot holds a pointer of the program's width.
                if flags >> 8 == 32 {
                    *(slot as usize as *mut u32) = subobject as usize as u32;
                } else {
                    *(slot as usize as *mut u64) = subobject as usize as u64;
                }
            } else {
                std::ptr::copy_nonoverlapping(subobject, slot as usize as *mut u8, size as usize);
            }
        }
        eh.staged = Some(e);
        eh.in_flight = None;
        1
    })
}

pub unsafe fn ra_eh_enter_catch() {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        if eh.in_flight.is_some() {
            eh_fatal("an exception escaped a destructor on the way to a catch");
        }
        let Some(e) = eh.staged.take() else { eh_fatal("a catch body with no exception matched") };
        eh.caught.push(e);
        eh.catch_sp.push(0);
    });
}

pub unsafe fn ra_eh_leave_catch() {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        if eh.caught.pop().is_none() {
            eh_fatal("the end of a catch body with no exception caught");
        }
        eh.catch_sp.pop();
    });
}

pub unsafe fn ra_eh_suspend() {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        let e = eh.in_flight.take();
        eh.suspended.push(e);
    });
}

pub unsafe fn ra_eh_resume() {
    EH.with(|eh| {
        let mut eh = eh.borrow_mut();
        if eh.in_flight.is_some() {
            eh_fatal("an exception escaped a destructor while another was unwinding");
        }
        let Some(e) = eh.suspended.pop() else { eh_fatal("a resume with no exception suspended") };
        eh.in_flight = e;
    });
}

/// The structured exception in flight, in the 32-bit layout the program reads: the
/// `EXCEPTION_RECORD` and the `EXCEPTION_POINTERS`, with no context, so a filter that reads one
/// faults rather than reads made-up registers.
struct Structured {
    record: [u32; 20],
    pointers: [u32; 2],
    in_flight: bool,
}

static mut SEH: Structured = Structured { record: [0; 20], pointers: [0; 2], in_flight: false };

/// Whether `size` bytes at `address` are committed and open to the access.
unsafe fn accessible(address: u64, size: u64, write: bool) -> bool {
    // PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY and their executable forms; PAGE_GUARD.
    let open = if write { 0x04 | 0x08 | 0x40 | 0x80 } else { 0x02 | 0x04 | 0x08 | 0x20 | 0x40 | 0x80 };
    let end = address.saturating_add(size.max(1));
    let mut at = address;
    while at < end {
        let mut info = MemoryInfo::default();
        if VirtualQuery(at as usize, &raw mut info, std::mem::size_of::<MemoryInfo>()) == 0
            || info.state != 0x1000
            || info.protect & open == 0
            || info.protect & 0x100 != 0
        {
            return false;
        }
        at = (info.base + info.region_size) as u64;
    }
    true
}

pub unsafe fn ra_eh_probe(address: u64, size: u64, write: u64, site: u64) -> u64 {
    if accessible(address, size, write != 0) {
        return 0;
    }
    let seh = &mut *(&raw mut SEH);
    if seh.in_flight {
        eh_fatal("a fault while a structured exception is in flight");
    }
    seh.record = [0; 20];
    seh.record[0] = 0xC000_0005;
    seh.record[3] = site as u32;
    seh.record[4] = 2;
    seh.record[5] = u32::from(write != 0);
    seh.record[6] = address as u32;
    seh.pointers = [seh.record.as_ptr() as usize as u32, 0];
    seh.in_flight = true;
    1
}

pub unsafe fn ra_eh_raise(code: u64, flags: u64, count: u64, arguments: u64, site: u64) {
    let seh = &mut *(&raw mut SEH);
    if seh.in_flight {
        eh_fatal("RaiseException while a structured exception is in flight");
    }
    seh.record = [0; 20];
    seh.record[0] = code as u32;
    seh.record[1] = flags as u32;
    seh.record[3] = site as u32;
    let count = count.min(15) as usize;
    seh.record[4] = count as u32;
    for n in 0..count {
        seh.record[5 + n] = *(arguments as usize as *const u32).add(n);
    }
    seh.pointers = [seh.record.as_ptr() as usize as u32, 0];
    seh.in_flight = true;
}

pub unsafe fn ra_eh_pointers() -> u64 {
    let seh = &mut *(&raw mut SEH);
    if !seh.in_flight {
        eh_fatal("a filter with no structured exception in flight");
    }
    seh.pointers.as_ptr() as usize as u64
}

pub unsafe fn ra_eh_handled() {
    let seh = &mut *(&raw mut SEH);
    if !seh.in_flight {
        eh_fatal("a handler with no structured exception in flight");
    }
    seh.in_flight = false;
}

pub unsafe fn ra_eh_unhandled(site: u64) {
    let code = (*(&raw const SEH)).record[0];
    eprintln!("ra: the exception {code:#x} at {site:#x} passes a frame the port does not unwind");
    std::process::exit(code as i32)
}

/// After the entry returns: an exception nothing caught ends the program, as the native
/// runtime's terminate would.
pub unsafe fn ra_eh_check_uncaught() {
    if ra_eh_pending() != 0 {
        eh_fatal("an exception the program did not catch");
    }
}

/// `setjmp` returns 0 once; `longjmp` cannot come back to it: Rust has no non-local jump, so the
/// port stops where the program would jump.
pub unsafe fn ra_x86_setjmp3(_buffer: u64, _count: u64) -> u64 {
    0
}

pub unsafe fn ra_x86_longjmp(_buffer: u64, value: u64) -> u64 {
    eprintln!("ra: a longjmp with {value}, which the Rust output cannot express");
    std::process::exit(0xE000_0005u32 as i32)
}

#[link(name = "ucrt")]
unsafe extern "C" {
    fn strtol(s: *const u8, end: *mut *mut u8, base: i32) -> i32;
    fn strtoul(s: *const u8, end: *mut *mut u8, base: i32) -> u32;
    fn strtoll(s: *const u8, end: *mut *mut u8, base: i32) -> i64;
    fn strtoull(s: *const u8, end: *mut *mut u8, base: i32) -> u64;
    fn strtod(s: *const u8, end: *mut *mut u8) -> f64;
    fn __stdio_common_vsprintf(options: u64, buffer: *mut u8, count: usize, format: *const u8, locale: usize, args: *const u64) -> i32;
    fn __stdio_common_vsprintf_s(options: u64, buffer: *mut u8, count: usize, format: *const u8, locale: usize, args: *const u64) -> i32;
    fn __stdio_common_vsnprintf_s(
        options: u64,
        buffer: *mut u8,
        count: usize,
        max: usize,
        format: *const u8,
        locale: usize,
        args: *const u64,
    ) -> i32;
    fn __stdio_common_vfprintf(options: u64, stream: usize, format: *const u8, locale: usize, args: *const u64) -> i32;
    fn __stdio_common_vsscanf(options: u64, buffer: *const u8, count: usize, format: *const u8, locale: usize, args: *const u64) -> i32;
    fn _get_stream_buffer_pointers(stream: usize, base: *mut *mut *mut u8, pointer: *mut *mut *mut u8, count: *mut *mut i32) -> i32;
}

/// The addresses of a FILE's buffer fields, which the C++ runtime's filebuf reads the stream
/// through: the host writes three pointers of eight bytes where the program's locals are dwords,
/// so they go to locals here and their low halves to the program's.
pub unsafe fn ra_x86_get_stream_buffer_pointers(stream: u64, base: u64, pointer: u64, count: u64) -> u64 {
    let mut b: *mut *mut u8 = core::ptr::null_mut();
    let mut p: *mut *mut u8 = core::ptr::null_mut();
    let mut c: *mut i32 = core::ptr::null_mut();
    let r = _get_stream_buffer_pointers(stream as usize, &mut b, &mut p, &mut c);
    for (at, value) in [(base, b as usize), (pointer, p as usize), (count, c as usize)] {
        if at != 0 {
            core::ptr::write_unaligned(at as usize as *mut u32, value as u32);
        }
    }
    r as i64 as u64
}

/// The strto* family writes a `char *` of eight bytes where the program expects four: the host's
/// end pointer goes to a local, and its low half to the program's.
unsafe fn store_end(end: u64, e: *mut u8) {
    if end != 0 {
        core::ptr::write_unaligned(end as usize as *mut u32, e as usize as u32);
    }
}

pub unsafe fn ra_x86_strtol(s: u64, end: u64, base: u64) -> u64 {
    let mut e = core::ptr::null_mut();
    let v = strtol(s as usize as *const u8, &mut e, base as i32);
    store_end(end, e);
    v as i64 as u64
}

pub unsafe fn ra_x86_strtoul(s: u64, end: u64, base: u64) -> u64 {
    let mut e = core::ptr::null_mut();
    let v = strtoul(s as usize as *const u8, &mut e, base as i32);
    store_end(end, e);
    u64::from(v)
}

pub unsafe fn ra_x86_strtoll(s: u64, end: u64, base: u64) -> u64 {
    let mut e = core::ptr::null_mut();
    let v = strtoll(s as usize as *const u8, &mut e, base as i32);
    store_end(end, e);
    v as u64
}

pub unsafe fn ra_x86_strtoull(s: u64, end: u64, base: u64) -> u64 {
    let mut e = core::ptr::null_mut();
    let v = strtoull(s as usize as *const u8, &mut e, base as i32);
    store_end(end, e);
    v
}

pub unsafe fn ra_x86_strtod(s: u64, end: u64) -> f64 {
    let mut e = core::ptr::null_mut();
    let v = strtod(s as usize as *const u8, &mut e);
    store_end(end, e);
    v
}

/// The printf family through its va_list: the program's arguments are dwords (a double or a
/// 64-bit integer two of them), the host's va_list a row of 8-byte slots. The format says what
/// each argument is; the adapter walks it and widens each argument into a slot.
unsafe fn widen_printf_args(format: *const u8, args: u64) -> [u64; 64] {
    let mut slots = [0u64; 64];
    let dword = |k: usize| core::ptr::read_unaligned((args as usize + 4 * k) as *const u32) as u64;
    let qword = |k: usize| core::ptr::read_unaligned((args as usize + 4 * k) as *const u64);
    let (mut n, mut k) = (0usize, 0usize);
    let mut p = format;
    while *p != 0 && n + 1 < 64 {
        if *p != b'%' {
            p = p.add(1);
            continue;
        }
        p = p.add(1);
        if *p == b'%' {
            p = p.add(1);
            continue;
        }
        while b"-+ #0'".contains(&*p) {
            p = p.add(1);
        }
        if *p == b'*' {
            slots[n] = dword(k);
            n += 1;
            k += 1;
            p = p.add(1);
        } else {
            while p.read().is_ascii_digit() {
                p = p.add(1);
            }
        }
        if *p == b'.' {
            p = p.add(1);
            if *p == b'*' {
                slots[n] = dword(k);
                n += 1;
                k += 1;
                p = p.add(1);
            } else {
                while p.read().is_ascii_digit() {
                    p = p.add(1);
                }
            }
        }
        let mut wide = false;
        if *p == b'l' && *p.add(1) == b'l' {
            wide = true;
            p = p.add(2);
        } else if *p == b'I' && *p.add(1) == b'6' && *p.add(2) == b'4' {
            wide = true;
            p = p.add(3);
        } else if *p == b'I' && *p.add(1) == b'3' && *p.add(2) == b'2' {
            p = p.add(3);
        } else if *p == b'j' {
            wide = true;
            p = p.add(1);
        } else if *p == b'h' {
            p = p.add(1);
            if *p == b'h' {
                p = p.add(1);
            }
        } else if b"lLwIzt".contains(&*p) {
            p = p.add(1);
        }
        match *p {
            b'a' | b'A' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => {
                slots[n] = qword(k);
                n += 1;
                k += 2;
            }
            b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'c' | b'C' | b's' | b'S' | b'p' | b'n' | b'Z' => {
                if wide {
                    slots[n] = qword(k);
                    k += 2;
                } else {
                    slots[n] = dword(k);
                    k += 1;
                }
                n += 1;
            }
            0 => break,
            _ => {}
        }
        p = p.add(1);
    }
    slots
}

/// The program's `%p` prints eight hex digits, the host's sixteen: it becomes `%08X` of the
/// dword. The format to hand the host: the program's own, or a copy.
unsafe fn narrow_pointers(format: *const u8) -> (Option<Vec<u8>>, *const u8) {
    let bytes = std::ffi::CStr::from_ptr(format.cast()).to_bytes();
    if !bytes.windows(2).any(|w| w == b"%p") {
        return (None, format);
    }
    let mut copy = Vec::with_capacity(bytes.len() * 2 + 1);
    let mut k = 0;
    while k < bytes.len() {
        match (bytes[k], bytes.get(k + 1)) {
            (b'%', Some(b'p')) => {
                copy.extend_from_slice(b"%08X");
                k += 2;
            }
            (b'%', Some(b'%')) => {
                copy.extend_from_slice(b"%%");
                k += 2;
            }
            (c, _) => {
                copy.push(c);
                k += 1;
            }
        }
    }
    copy.push(0);
    let p = copy.as_ptr();
    (Some(copy), p)
}

/// scanf's arguments are all pointers, a dword each: one per conversion that is not suppressed.
unsafe fn widen_scanf_args(format: *const u8, args: u64) -> [u64; 64] {
    let mut slots = [0u64; 64];
    let mut n = 0usize;
    let mut p = format;
    while *p != 0 && n < 64 {
        if *p == b'%' {
            p = p.add(1);
            if *p == 0 {
                break;
            }
            if *p != b'%' && *p != b'*' {
                slots[n] = core::ptr::read_unaligned((args as usize + 4 * n) as *const u32) as u64;
                n += 1;
            }
        }
        p = p.add(1);
    }
    slots
}

pub unsafe fn ra_x86_vsprintf(options: u64, buffer: u64, count: u64, format: u64, locale: u64, args: u64) -> u64 {
    let (_copy, f) = narrow_pointers(format as usize as *const u8);
    let slots = widen_printf_args(f, args);
    __stdio_common_vsprintf(options, buffer as usize as *mut u8, count as usize, f, locale as usize, slots.as_ptr()) as i64 as u64
}

pub unsafe fn ra_x86_vsprintf_s(options: u64, buffer: u64, count: u64, format: u64, locale: u64, args: u64) -> u64 {
    let (_copy, f) = narrow_pointers(format as usize as *const u8);
    let slots = widen_printf_args(f, args);
    __stdio_common_vsprintf_s(options, buffer as usize as *mut u8, count as usize, f, locale as usize, slots.as_ptr()) as i64 as u64
}

pub unsafe fn ra_x86_vsnprintf_s(options: u64, buffer: u64, count: u64, max: u64, format: u64, locale: u64, args: u64) -> u64 {
    let (_copy, f) = narrow_pointers(format as usize as *const u8);
    let slots = widen_printf_args(f, args);
    __stdio_common_vsnprintf_s(options, buffer as usize as *mut u8, count as usize, max as usize, f, locale as usize, slots.as_ptr()) as i64 as u64
}

pub unsafe fn ra_x86_vfprintf(options: u64, stream: u64, format: u64, locale: u64, args: u64) -> u64 {
    let (_copy, f) = narrow_pointers(format as usize as *const u8);
    let slots = widen_printf_args(f, args);
    __stdio_common_vfprintf(options, stream as usize, f, locale as usize, slots.as_ptr()) as i64 as u64
}

pub unsafe fn ra_x86_vsscanf(options: u64, buffer: u64, count: u64, format: u64, locale: u64, args: u64) -> u64 {
    let slots = widen_scanf_args(format as usize as *const u8, args);
    __stdio_common_vsscanf(options, buffer as usize as *const u8, count as usize, format as usize as *const u8, locale as usize, slots.as_ptr())
        as i64 as u64
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

/// `InitializeSListHead`: the program's list head is 8 bytes, the host's 16, and the host's own
/// function refuses a head that is not 16-byte aligned; the empty list is zeros.
pub unsafe fn ra_x86_initialize_slist_head(head: u64) -> u64 {
    core::ptr::write_bytes(head as usize as *mut u8, 0, 8);
    0
}

/// The program's list head, {the first entry, a 16-bit depth, a 16-bit sequence} in one word,
/// swapped whole by each operation; a push counts in the sequence, so a pop that raced a pop and a
/// push fails its swap. An entry's first dword links the next.
unsafe fn slist32(head: u64) -> &'static core::sync::atomic::AtomicU64 {
    core::sync::atomic::AtomicU64::from_ptr(head as usize as *mut u64)
}

/// The depth field of a head.
fn slist32_depth(word: u64) -> u16 {
    (word >> 32) as u16
}

/// `InterlockedPushEntrySList` over the program's 8-byte head: the previous first entry.
pub unsafe fn ra_x86_interlocked_push_entry_slist(head: u64, entry: u64) -> u64 {
    let ordering = core::sync::atomic::Ordering::SeqCst;
    let h = slist32(head);
    let mut old = h.load(ordering);
    loop {
        core::ptr::write_volatile(entry as usize as *mut u32, old as u32);
        let next = u64::from(entry as u32)
            | (u64::from(slist32_depth(old).wrapping_add(1)) << 32)
            | (u64::from(((old >> 48) as u16).wrapping_add(1)) << 48);
        match h.compare_exchange(old, next, ordering, ordering) {
            Ok(_) => return u64::from(old as u32),
            Err(now) => old = now,
        }
    }
}

/// `InterlockedPopEntrySList`: the first entry, or zero for an empty list.
pub unsafe fn ra_x86_interlocked_pop_entry_slist(head: u64) -> u64 {
    let ordering = core::sync::atomic::Ordering::SeqCst;
    let h = slist32(head);
    let mut old = h.load(ordering);
    loop {
        let first = old as u32;
        if first == 0 {
            return 0;
        }
        let link = core::ptr::read_volatile(first as usize as *const u32);
        let next = u64::from(link) | (u64::from(slist32_depth(old).wrapping_sub(1)) << 32) | (old & 0xffff_0000_0000_0000);
        match h.compare_exchange(old, next, ordering, ordering) {
            Ok(_) => return u64::from(first),
            Err(now) => old = now,
        }
    }
}

/// `InterlockedFlushSList`: the list's first entry, the head left empty with its sequence.
pub unsafe fn ra_x86_interlocked_flush_slist(head: u64) -> u64 {
    let ordering = core::sync::atomic::Ordering::SeqCst;
    let old = slist32(head).fetch_and(0xffff_0000_0000_0000, ordering);
    u64::from(old as u32)
}

/// `QueryDepthSList`.
pub unsafe fn ra_x86_query_depth_slist(head: u64) -> u64 {
    u64::from(slist32_depth(slist32(head).load(core::sync::atomic::Ordering::SeqCst)))
}

/// `InitOnceExecuteOnce` over the program's dword `INIT_ONCE`: done is bit 1, the context above
/// the reserved bits; the callback, the program's universal entry, writes a dword context.
pub unsafe fn ra_x86_init_once_execute_once(once: u64, function: u64, parameter: u64, context: u64) -> u64 {
    let state = once as usize as *mut u32;
    if *state & 2 == 0 {
        let mut made = 0u32;
        let out: *mut u32 = if context != 0 { context as usize as *mut u32 } else { &raw mut made };
        let callback: unsafe extern "system" fn(u64, u64, u64) -> u32 = core::mem::transmute(function as usize);
        if callback(once, parameter, out as usize as u64) == 0 {
            return 0;
        }
        *state = (*out & !3) | 2;
    }
    if context != 0 {
        *(context as usize as *mut u32) = *state & !3;
    }
    1
}

/// `WaitForMultipleObjects` over the program's array of dword handles, widened as WoW64 does.
pub unsafe fn ra_x86_wait_for_multiple_objects(count: u64, handles: u64, all: u64, milliseconds: u64) -> u64 {
    let n = (count as u32).min(64) as usize;
    let mut wide = [0usize; 64];
    for (i, slot) in wide.iter_mut().enumerate().take(n) {
        *slot = *(handles as usize as *const i32).add(i) as isize as usize;
    }
    u64::from(WaitForMultipleObjects(n as u32, wide.as_ptr(), all as i32, milliseconds as u32))
}

/// The value at `target`, a 32-bit program's 32-bit `LONG`, as an atomic.
unsafe fn interlocked(target: u64) -> &'static core::sync::atomic::AtomicI32 {
    core::sync::atomic::AtomicI32::from_ptr(target as usize as *mut i32)
}

/// The key [`ra_x86_encode_pointer`] flips: the C runtime's `RA_POINTER_KEY`.
const POINTER_KEY: u32 = 0x5ca1_ab1e;

/// `EncodePointer` within the program's dword: the 64-bit kernel32's encoding takes all 64 bits,
/// so its decode of the dword the program kept is no pointer. The null pointer encodes to the key.
pub unsafe fn ra_x86_encode_pointer(pointer: u64) -> u64 {
    u64::from(pointer as u32 ^ POINTER_KEY)
}

/// `DecodePointer` of what [`ra_x86_encode_pointer`] gave.
pub unsafe fn ra_x86_decode_pointer(pointer: u64) -> u64 {
    u64::from(pointer as u32 ^ POINTER_KEY)
}

/// The interlocked operations of the 32-bit kernel32, which the 64-bit one does not export: each a
/// full barrier, the increment and the decrement returning the new value, the others the old one.
pub unsafe fn ra_x86_interlocked_increment(target: u64) -> u64 {
    u64::from(interlocked(target).fetch_add(1, core::sync::atomic::Ordering::SeqCst).wrapping_add(1) as u32)
}

pub unsafe fn ra_x86_interlocked_decrement(target: u64) -> u64 {
    u64::from(interlocked(target).fetch_sub(1, core::sync::atomic::Ordering::SeqCst).wrapping_sub(1) as u32)
}

pub unsafe fn ra_x86_interlocked_exchange(target: u64, value: u64) -> u64 {
    u64::from(interlocked(target).swap(value as i32, core::sync::atomic::Ordering::SeqCst) as u32)
}

pub unsafe fn ra_x86_interlocked_exchange_add(target: u64, value: u64) -> u64 {
    u64::from(interlocked(target).fetch_add(value as i32, core::sync::atomic::Ordering::SeqCst) as u32)
}

pub unsafe fn ra_x86_interlocked_compare_exchange(target: u64, exchange: u64, comparand: u64) -> u64 {
    let ordering = core::sync::atomic::Ordering::SeqCst;
    let old = interlocked(target).compare_exchange(comparand as i32, exchange as i32, ordering, ordering).unwrap_or_else(|v| v);
    u64::from(old as u32)
}

/// The module handles a 32-bit program holds: a host module below 4 GB is its own handle, one
/// above is the address of the cell here that keeps it, which the program's `GetProcAddress` reads
/// back.
static mut MODULE_CELLS: [usize; 256] = [0; 256];

unsafe fn module_dword(module: usize) -> u64 {
    if module as u64 <= u64::from(u32::MAX) {
        return module as u64;
    }
    let cells = (&raw mut MODULE_CELLS).cast::<usize>();
    for i in 0..256 {
        let cell = cells.add(i);
        if *cell == 0 {
            *cell = module;
        }
        if *cell == module {
            return u64::from(cell as usize as u32);
        }
    }
    eprintln!("ra: more than 256 modules above 4 GB");
    std::process::exit(0xE000_0004u32 as i32)
}

unsafe fn module_of_dword(handle: u64) -> usize {
    let at = handle as u32 as usize;
    let cells = (&raw const MODULE_CELLS).cast::<usize>() as usize;
    if at >= cells && at < cells + 256 * 8 && (at - cells) % 8 == 0 { *(at as *const usize) } else { at }
}

pub unsafe fn ra_x86_get_module_handle_a(name: u64) -> u64 {
    module_dword(GetModuleHandleA(name as usize as *const u8))
}

pub unsafe fn ra_x86_get_module_handle_w(name: u64) -> u64 {
    module_dword(GetModuleHandleW(name as usize as *const u16))
}

pub unsafe fn ra_x86_load_library_a(name: u64) -> u64 {
    module_dword(LoadLibraryA(name as usize as *const u8))
}

pub unsafe fn ra_x86_load_library_w(name: u64) -> u64 {
    module_dword(LoadLibraryW(name as usize as *const u16))
}

pub unsafe fn ra_x86_load_library_ex_a(name: u64, file: u64, flags: u64) -> u64 {
    module_dword(LoadLibraryExA(name as usize as *const u8, file as usize, flags as u32))
}

pub unsafe fn ra_x86_load_library_ex_w(name: u64, file: u64, flags: u64) -> u64 {
    module_dword(LoadLibraryExW(name as usize as *const u16, file as usize, flags as u32))
}

/// `GetProcAddress` of a 32-bit program: a function below 4 GB as it is. The Rust runtime has no
/// trampoline to hand out for one above, which a dword cannot name: the port stops there.
pub unsafe fn ra_x86_get_proc_address(module: u64, name: u64) -> u64 {
    let f = GetProcAddress(module_of_dword(module), name as u32 as usize);
    if f as u64 > u64::from(u32::MAX) {
        eprintln!("ra: GetProcAddress hands out a function above 4 GB, which the Rust runtime has no trampoline for");
        std::process::exit(0xE000_0004u32 as i32);
    }
    f as u64
}

unsafe extern "C" {
    fn malloc(size: usize) -> *mut u8;
    fn free(block: *mut u8);
}

/// The C++ runtime's `operator new` a 32-bit program links statically: never null, a distinct
/// block for a size of zero, and the end of the program where it would throw `bad_alloc`.
pub unsafe fn ra_x86_operator_new(size: u64) -> u64 {
    let block = malloc(size.max(1) as usize);
    if block.is_null() {
        std::process::abort();
    }
    block as usize as u64
}

/// The C++ runtime's `operator delete`, plain or sized: the caller pops the size.
pub unsafe fn ra_x86_operator_delete(block: u64) -> u64 {
    free(block as usize as *mut u8);
    0
}

// The run-time type information of a 32-bit program, which the host's `__RTDynamicCast`,
// `__RTCastToVoid` and `__RTtypeid` would read in the 64-bit layout: the program's structures hold
// absolute dwords. The dword before an object's vtable names its complete object locator
// {signature, offset, cdOffset, type descriptor, class hierarchy descriptor}; the hierarchy's base
// class array lists the complete class and every base, depth first, each {type descriptor,
// contained bases, PMD {mdisp, pdisp, vdisp}, attributes}, so the bases a class contains follow it
// in the array. The two images of a port each carry a descriptor for a type they share, so types
// compare by their decorated names.
unsafe fn rtti32_u32(at: u32) -> u32 {
    (at as usize as *const u32).read_unaligned()
}

unsafe fn rtti32_i32(at: u32) -> i32 {
    rtti32_u32(at) as i32
}

unsafe fn rtti32_same_type(a: u32, b: u32) -> bool {
    let name = |descriptor: u32| std::ffi::CStr::from_ptr((descriptor + 8) as usize as *const std::ffi::c_char);
    a == b || name(a) == name(b)
}

unsafe fn rtti32_locator(object: u32) -> u32 {
    rtti32_u32(rtti32_u32(object).wrapping_sub(4))
}

/// The offset of a base in the complete object, through the vbtable for a virtual one.
unsafe fn rtti32_base_offset(complete: u32, pmd: u32) -> i32 {
    let pdisp = rtti32_i32(pmd + 4);
    let mut offset = 0;
    if pdisp >= 0 {
        offset = pdisp.wrapping_add(rtti32_i32(rtti32_u32(complete.wrapping_add(pdisp as u32)).wrapping_add(rtti32_i32(pmd + 8) as u32)));
    }
    offset.wrapping_add(rtti32_i32(pmd))
}

/// The complete object `object` is a subobject of.
unsafe fn rtti32_complete(object: u32) -> u32 {
    let locator = rtti32_locator(object);
    let mut complete = object.wrapping_sub(rtti32_u32(locator + 4));
    let cd_offset = rtti32_u32(locator + 8);
    if cd_offset != 0 {
        complete = complete.wrapping_sub(rtti32_u32(object.wrapping_sub(cd_offset)));
    }
    complete
}

/// `dynamic_cast`: the target instance that contains the source instance (a downcast), unless two
/// at different offsets do; else a cross cast to a target the complete object holds once and
/// publicly, from a public source. A failed cast of a reference would throw `bad_cast`, which no
/// handler of the port can take: it ends the program.
pub unsafe fn ra_x86_rt_dynamic_cast(object: u64, vf_delta: u64, source: u64, target: u64, is_reference: u64) -> u64 {
    if object == 0 {
        return 0;
    }
    let (object, source, target) = (object as u32, source as u32, target as u32);
    let complete = rtti32_complete(object);
    let source_offset = object.wrapping_sub(vf_delta as u32).wrapping_sub(complete) as i32;
    let hierarchy = rtti32_u32(rtti32_locator(object) + 16);
    let (bases, array) = (rtti32_u32(hierarchy + 8), rtti32_u32(hierarchy + 12));
    let (mut found, mut cross, mut source_base) = (None, None, None);
    let mut ambiguous = false;
    for i in 0..bases {
        if ambiguous {
            break;
        }
        let base = rtti32_u32(array + 4 * i);
        let is_source = rtti32_same_type(rtti32_u32(base), source) && rtti32_base_offset(complete, base + 8) == source_offset;
        if is_source && source_base.is_none() {
            source_base = Some(base);
        }
        if !rtti32_same_type(rtti32_u32(base), target) {
            continue;
        }
        if cross.is_none() {
            cross = Some(base);
        }
        let contained = rtti32_u32(base + 4);
        for j in 1..=contained {
            if i + j >= bases {
                break;
            }
            let inner = rtti32_u32(array + 4 * (i + j));
            if !rtti32_same_type(rtti32_u32(inner), source) || rtti32_base_offset(complete, inner + 8) != source_offset {
                continue;
            }
            // BCD_NOTVISIBLE: the source is a private base of this target.
            if rtti32_u32(inner + 20) & 1 != 0 {
                continue;
            }
            let offset = rtti32_base_offset(complete, base + 8);
            if found.is_some_and(|(_, found_offset)| found_offset != offset) {
                ambiguous = true;
            }
            found = Some((base, offset));
        }
    }
    let result = if ambiguous {
        0
    } else if let Some((_, offset)) = found {
        complete.wrapping_add(offset as u32)
    } else {
        match (cross, source_base) {
            // BCD_NOTVISIBLE | BCD_AMBIGUOUS on the target, BCD_PRIVORPROTINCOMPOBJ on the source.
            (Some(cross), Some(source_base)) if rtti32_u32(cross + 20) & 3 == 0 && rtti32_u32(source_base + 20) & 4 == 0 => {
                complete.wrapping_add(rtti32_base_offset(complete, cross + 8) as u32)
            }
            _ => 0,
        }
    };
    if result == 0 && is_reference != 0 {
        eh_fatal("a dynamic_cast of a reference failed");
    }
    u64::from(result)
}

pub unsafe fn ra_x86_rt_cast_to_void(object: u64) -> u64 {
    if object == 0 { 0 } else { u64::from(rtti32_complete(object as u32)) }
}

/// `typeid` of an object: its complete object's type descriptor, which is the program's own.
pub unsafe fn ra_x86_rt_typeid(object: u64) -> u64 {
    if object == 0 {
        eh_fatal("typeid of a null pointer");
    }
    u64::from(rtti32_u32(rtti32_locator(object as u32) + 12))
}

/// `type_info::name()`: the host's `__std_type_info_name` reads the cached undecorated name in an
/// eight-byte slot before the decorated name, where the program's descriptor keeps a dword, and
/// would take the dword and the name's first bytes for a cached pointer. The host undecorates a
/// copy of the descriptor in its own layout, and the name it allocates lies under 4 GB (the port is
/// not large-address-aware), so the program's dword caches it and a descriptor is undecorated once.
#[cfg(windows)]
#[repr(C)]
struct StdTypeInfoData64 {
    undecorated: *const std::ffi::c_char,
    decorated: [std::ffi::c_char; 1],
}

#[cfg(windows)]
unsafe extern "C" {
    fn __std_type_info_name(data: *mut StdTypeInfoData64, root: *mut TypeInfoNode64) -> *const std::ffi::c_char;
}

/// The list of the names the host allocated: an interlocked list's header, sixteen bytes on a
/// sixteen-byte boundary, which the host's push faults on anywhere else.
#[cfg(windows)]
#[repr(C, align(16))]
struct TypeInfoNode64(std::cell::UnsafeCell<[u64; 2]>);

// The host's interlocked push is the only writer.
#[cfg(windows)]
unsafe impl Sync for TypeInfoNode64 {}

#[cfg(windows)]
static TYPE_INFO_NODE: TypeInfoNode64 = TypeInfoNode64(std::cell::UnsafeCell::new([0; 2]));

#[cfg(windows)]
pub unsafe fn ra_x86_std_type_info_name(data: u64, _root: u64) -> u64 {
    let cached = data as usize as *mut u32;
    if cached.read_unaligned() != 0 {
        return u64::from(cached.read_unaligned());
    }
    let decorated = std::ffi::CStr::from_ptr((data + 4) as usize as *const std::ffi::c_char);
    let size = decorated.to_bytes_with_nul().len();
    let copy = malloc(std::mem::size_of::<*const std::ffi::c_char>() + size).cast::<StdTypeInfoData64>();
    if copy.is_null() {
        std::process::abort();
    }
    (*copy).undecorated = std::ptr::null();
    std::ptr::copy_nonoverlapping(decorated.as_ptr(), (*copy).decorated.as_mut_ptr(), size);
    let name = __std_type_info_name(copy, TYPE_INFO_NODE.0.get().cast::<TypeInfoNode64>());
    cached.write_unaligned(name as usize as u32);
    u64::from(name as usize as u32)
}

/// A host without the C++ runtime's undecorator cannot answer `type_info::name()`.
#[cfg(not(windows))]
pub unsafe fn ra_x86_std_type_info_name(_data: u64, _root: u64) -> u64 {
    eh_fatal("type_info::name() needs the host's C++ runtime to undecorate a name")
}

unsafe extern "C" {
    fn _controlfp(new: u32, mask: u32) -> u32;
    fn _clearfp() -> u32;
    fn _statusfp() -> u32;
    fn fetestexcept(excepts: i32) -> i32;
    fn feclearexcept(excepts: i32) -> i32;
}

// The floating-point environment of an x86 program is the x87 control and status words as much as
// the SSE register: `_controlfp`, `_clearfp` and the C99 fenv functions read and write both. The
// port keeps the lifted words (the exact x87 unit rounds by the control word and raises its flags
// in the status word; the double unit and the host's own arithmetic use the host's) and the host's
// in step: a mode goes to both, a report merges both. The host's word has no precision control, so
// that field is the lifted word's alone.
const MCW_EM: u32 = 0x8001f;
const MCW_RC: u32 = 0x300;
const MCW_PC: u32 = 0x30000;
const MCW_IC: u32 = 0x40000;

/// The exception bits of the C runtime's word (`_EM_INEXACT` 1, `_EM_UNDERFLOW` 2, `_EM_OVERFLOW`
/// 4, `_EM_ZERODIVIDE` 8, `_EM_INVALID` 0x10, `_EM_DENORMAL` 0x80000) from the x87's order
/// (invalid, denormal, zero divide, overflow, underflow, precision in bits 0 to 5).
fn crt_flags(x87: u32) -> u32 {
    ((x87 >> 5) & 1) | ((x87 >> 3) & 2) | ((x87 >> 1) & 4) | ((x87 << 1) & 8) | ((x87 << 4) & 0x10) | if x87 & 2 != 0 { 0x80000 } else { 0 }
}

fn x87_flags(crt: u32) -> u32 {
    ((crt >> 4) & 1) | ((crt >> 18) & 2) | ((crt >> 1) & 4) | ((crt << 1) & 8) | ((crt << 3) & 0x10) | ((crt << 5) & 0x20)
}

/// The C runtime's control word from the x87's: the masks, the rounding mode (bits 10 and 11 to
/// bits 8 and 9) and the precision (24 bits is `_PC_24` 0x20000, 53 `_PC_53` 0x10000, 64 `_PC_64` 0).
fn crt_control(cw: u32) -> u32 {
    let pc = match (cw >> 8) & 3 {
        0 => 0x20000,
        2 => 0x10000,
        _ => 0,
    };
    crt_flags(cw & 0x3f) | ((cw >> 2) & MCW_RC) | pc
}

fn x87_control(crt: u32, cw: u16) -> u16 {
    let pc = match crt & MCW_PC {
        0x20000 => 0,
        0x10000 => 2,
        _ => 3,
    };
    (u32::from(cw) & 0xf0c0 | x87_flags(crt & MCW_EM) | ((crt & MCW_RC) << 2) | (pc << 8)) as u16
}

unsafe fn x87_cw() -> u16 {
    *(&raw const crate::data::ra_x87_cw)
}

unsafe fn x87_sw_flags() -> u32 {
    crt_flags(u32::from(*(&raw const crate::data::ra_x87_sw)) & 0x3f)
}

pub unsafe fn ra_x86_controlfp(new: u64, mask: u64) -> u64 {
    let host = _controlfp(new as u32 & !(MCW_PC | MCW_IC), mask as u32 & !(MCW_PC | MCW_IC));
    let x87 = (crt_control(u32::from(x87_cw())) & !(mask as u32)) | (new as u32 & mask as u32);
    *(&raw mut crate::data::ra_x87_cw) = x87_control(x87, x87_cw());
    let own = MCW_EM | MCW_RC | MCW_PC;
    u64::from((host & !own) | (x87 & own))
}

pub unsafe fn ra_x86_controlfp_s(current: u64, new: u64, mask: u64) -> u64 {
    let word = ra_x86_controlfp(new, mask) as u32;
    if current != 0 {
        core::ptr::write_unaligned(current as usize as *mut u32, word);
    }
    0
}

pub unsafe fn ra_x86_fesetround(mode: u64) -> u64 {
    if mode & !u64::from(MCW_RC) != 0 {
        return 1;
    }
    ra_x86_controlfp(mode, u64::from(MCW_RC));
    0
}

pub unsafe fn ra_x86_statusfp() -> u64 {
    u64::from(_statusfp() | x87_sw_flags())
}

pub unsafe fn ra_x86_clearfp() -> u64 {
    let flags = _clearfp() | x87_sw_flags();
    *(&raw mut crate::data::ra_x87_sw) &= !0xff;
    u64::from(flags)
}

pub unsafe fn ra_x86_fetestexcept(excepts: u64) -> u64 {
    u64::from((fetestexcept(excepts as i32) as u32 | x87_sw_flags()) & excepts as u32)
}

pub unsafe fn ra_x86_feclearexcept(excepts: u64) -> u64 {
    *(&raw mut crate::data::ra_x87_sw) &= !(x87_flags(excepts as u32) as u16);
    u64::from(feclearexcept(excepts as i32) as u32)
}

/// The x86 C runtime's square root is the x87's: the result rounded to the precision control, so
/// a program that set 24 bits (Direct3D's default) finds a float's mantissa in its double. The
/// double's root rounded to a float is the float's root: 53 bits are enough for the second
/// rounding to be harmless.
pub unsafe fn ra_x86_sqrt(x: f64) -> f64 {
    let r = x.sqrt();
    if (x87_cw() >> 8) & 3 == 0 { f64::from(r as f32) } else { r }
}

/// The x86 C runtime's SSE2 power (`_libm_sse2_pow_precise`, the operands in xmm0 and xmm1),
/// which the 64-bit runtime does not export.
pub unsafe fn ra_x86_pow_sse2(x: f64, y: f64) -> f64 {
    x.powf(y)
}

static mut ARGV32: Vec<u32> = Vec::new();
static mut ARGV32_ADDRESS: u32 = 0;
static mut ARG_STRINGS: Vec<std::ffi::CString> = Vec::new();
static mut ENVIRONMENT32: Vec<u32> = Vec::new();

static mut COMMODE32: u32 = 0;
static mut FMODE32: u32 = 0;
static mut ARGC32: u32 = 0;
static mut ERRNO32: u32 = 0;

/// `__p__commode`: the dword the C runtime's commit mode lives in.
pub unsafe fn ra_x86_p_commode() -> u64 {
    &raw mut COMMODE32 as usize as u64
}

/// `__p__fmode`: the dword the C runtime's file mode lives in.
pub unsafe fn ra_x86_p_fmode() -> u64 {
    &raw mut FMODE32 as usize as u64
}

/// `__p___argc`: the dword that holds the argument count.
pub unsafe fn ra_x86_p_argc() -> u64 {
    *(&raw mut ARGC32) = std::env::args().count() as u32;
    &raw mut ARGC32 as usize as u64
}

/// `_errno`: the dword errno lives in.
pub unsafe fn ra_x86_errno() -> u64 {
    &raw mut ERRNO32 as usize as u64
}

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

/// The fail-fast reports of the x86 /GS checks (`__report_gsfailure` after a cookie mismatch,
/// `__report_rangecheckfailure` on an index out of range): the program ends as it would natively,
/// with STATUS_STACK_BUFFER_OVERRUN.
pub fn ra_x86_report_gsfailure() -> ! {
    eprintln!("ra: the stack cookie check failed");
    std::process::exit(0xC000_0409u32 as i32)
}

pub fn ra_x86_report_rangecheckfailure() -> ! {
    eprintln!("ra: an array index was out of range");
    std::process::exit(0xC000_0409u32 as i32)
}

/// The thread environment block the lifted code reads through `fs:` (x86) or `gs:` (x64), one per
/// thread of the port: the fields a program and its C runtime look at, in the layout of the
/// program's pointer width, from the host's own thread, and at 0x2c (x86) or 0x58 (x64) the
/// thread-local storage pointer, an array whose one slot (the program's `__tls_index` is 0) is
/// the thread's own copy of the image's TLS template. The stack bounds come from the host's block;
/// the addresses fit the width, since a 32-bit port runs with every address below 4 GB.
#[repr(C, align(16))]
struct Teb([u8; 4096]);

thread_local! {
    static TEB: core::cell::UnsafeCell<Teb> = const { core::cell::UnsafeCell::new(Teb([0; 4096])) };
    static TLS_SLOT: core::cell::UnsafeCell<u64> = const { core::cell::UnsafeCell::new(0) };
    static ATTACHED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// The image's thread-local storage as `ra_tls` holds it: the template's start and end,
/// `__tls_index`, the zero fill and the callbacks array; zero throughout without a TLS directory.
static mut TLS: [u64; 5] = [0; 5];
static mut TLS_BITS: u32 = 64;

/// The x64 RTTI names its parts by offsets from the image's base, which the host's `__RTtypeid`,
/// `__RTDynamicCast` and `__RTCastToVoid` take as the locator's address less its own offset. The
/// port's sections are statics placed apart, so each field is rewritten to the offset of what it
/// names from the port's module: `ra_rtti` holds the pairs of a field's address and the address it
/// names, then a zero pair. The statics are writable: `ra_init` wrote their address words.
pub unsafe fn ra_init_rtti(table: *const u64) {
    let module = GetModuleHandleA(core::ptr::null()) as u64;
    let mut i = 0;
    loop {
        let field = core::ptr::read_unaligned(table.add(2 * i));
        if field == 0 {
            break;
        }
        let target = core::ptr::read_unaligned(table.add(2 * i + 1));
        core::ptr::write_unaligned(field as usize as *mut u32, target.wrapping_sub(module) as u32);
        i += 1;
    }
}

/// The loader's work before the entry: `__tls_index` written, the main thread's block, and the
/// TLS callbacks with `DLL_PROCESS_ATTACH`.
pub unsafe fn ra_init_tls(tls: *const u64, bits: u32) {
    TLS = core::ptr::read_unaligned(tls as *const [u64; 5]);
    TLS_BITS = bits;
    if TLS[0] != 0 && TLS[2] != 0 {
        core::ptr::write_unaligned(TLS[2] as usize as *mut u32, 0);
    }
    attach_thread(1);
}

/// The block of the calling thread, attached at its first access (before it reads anything of its
/// own, as the loader would have) with `DLL_THREAD_ATTACH` to the callbacks. Nothing runs at a
/// thread's exit.
pub unsafe fn ra_teb() -> u64 {
    if !ATTACHED.with(|a| a.get()) {
        attach_thread(2);
    }
    TEB.with(|t| t.get() as usize as u64)
}

unsafe fn attach_thread(reason: u64) {
    ATTACHED.with(|a| a.set(true));
    let teb = TEB.with(|t| t.get() as *mut u8);
    let slot = TLS_SLOT.with(|s| s.get() as usize as u64);
    let host: *const u8;
    core::arch::asm!("mov {}, gs:[0x30]", out(reg) host, options(nostack, readonly, preserves_flags));
    let stack_base = core::ptr::read_unaligned(host.add(0x08) as *const u64);
    let stack_limit = core::ptr::read_unaligned(host.add(0x10) as *const u64);
    let (pid, tid) = (GetCurrentProcessId(), GetCurrentThreadId());
    if TLS_BITS == 32 {
        core::ptr::write_unaligned(teb.add(0x04) as *mut u32, stack_base as u32);
        core::ptr::write_unaligned(teb.add(0x08) as *mut u32, stack_limit as u32);
        core::ptr::write_unaligned(teb.add(0x18) as *mut u32, teb as usize as u32);
        core::ptr::write_unaligned(teb.add(0x20) as *mut u32, pid);
        core::ptr::write_unaligned(teb.add(0x24) as *mut u32, tid);
        core::ptr::write_unaligned(teb.add(0x2c) as *mut u32, slot as u32);
    } else {
        core::ptr::write_unaligned(teb.add(0x08) as *mut u64, stack_base);
        core::ptr::write_unaligned(teb.add(0x10) as *mut u64, stack_limit);
        core::ptr::write_unaligned(teb.add(0x30) as *mut u64, teb as usize as u64);
        core::ptr::write_unaligned(teb.add(0x40) as *mut u64, u64::from(pid));
        core::ptr::write_unaligned(teb.add(0x48) as *mut u64, u64::from(tid));
        core::ptr::write_unaligned(teb.add(0x58) as *mut u64, slot);
    }
    if TLS[0] == 0 {
        return;
    }
    let initialized = (TLS[1] - TLS[0]) as usize;
    let mut block = vec![0u8; initialized + TLS[3] as usize + 16];
    core::ptr::copy_nonoverlapping(TLS[0] as usize as *const u8, block.as_mut_ptr(), initialized);
    TLS_SLOT.with(|s| *s.get() = Box::leak(block.into_boxed_slice()).as_mut_ptr() as usize as u64);
    // The callbacks, in their order: host functions, through their universal entry (the module
    // handle the loader passes is unknown to a port that stands alone).
    let width = if TLS_BITS == 32 { 4 } else { 8 };
    let mut p = TLS[4] as usize;
    while p != 0 {
        let target = if width == 4 { u64::from(core::ptr::read_unaligned(p as *const u32)) } else { core::ptr::read_unaligned(p as *const u64) };
        if target == 0 {
            break;
        }
        let f: Universal = core::mem::transmute(target as usize);
        f(0, reason, 0, 0, 0, 0, 0, 0);
        p += width;
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

/// A Linux system call of a MIPS o32 program, by its number (`4000 + n`) on the four argument
/// registers: the kernel's result, negative with the error number on a failure. `write` and
/// `exit` are answered by the host, as the C runtime answers them; the arguments are o32's 32-bit
/// words, its pointers the port's own below 4 GB.
pub unsafe fn ra_syscall(number: u64, a0: u64, a1: u64, a2: u64, _a3: u64) -> u64 {
    use std::io::Write;
    match number {
        // exit, exit_group
        4001 | 4246 => {
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            std::process::exit(a0 as u32 as i32)
        }
        // write
        4004 => {
            let (fd, len) = (a0 as u32, a2 as u32);
            let bytes: &[u8] = if len == 0 { &[] } else { core::slice::from_raw_parts(a1 as u32 as usize as *const u8, len as usize) };
            let written = match fd {
                1 => std::io::stdout().write_all(bytes).and_then(|()| std::io::stdout().flush()),
                2 => std::io::stderr().write_all(bytes),
                // -EBADF
                _ => return 9u64.wrapping_neg(),
            };
            // -EIO
            if written.is_err() { 5u64.wrapping_neg() } else { u64::from(len) }
        }
        _ => {
            eprintln!("ra: the system call {number} is not answered");
            std::process::exit(0xE000_0002u32 as i32)
        }
    }
}

// The exact x87 unit: a software f80. A slot holds an 80-bit value as the x87 stores a tbyte, the
// 64-bit significand with its explicit integer bit then the sign and the 15-bit exponent, and six
// bytes of padding. Every operation unpacks its operands, works in 128 bits with a sticky bit for
// what fell below, and rounds the result by the control word: the rounding mode in bits 10 and 11
// and the precision in bits 8 and 9 (24, 53 or 64 bits), as the hardware does with the exceptions
// masked. The C runtime's driver.c has the same unit, line for line.

#[derive(Clone, Copy, PartialEq, Eq)]
enum F80Kind {
    Zero,
    Finite,
    Inf,
    Nan,
}

/// An unpacked value: `m * 2^(exp - 63)`, `m` normalized with bit 63 set when finite.
#[derive(Clone, Copy)]
struct F80 {
    kind: F80Kind,
    sign: bool,
    exp: i32,
    m: u64,
}

// The exception flags of the status word, which an operation raises as the hardware does with the
// exceptions masked: invalid, zero divide, overflow, underflow (a tiny and inexact result) and
// precision (an inexact one).
const F80_IE: u16 = 0x01;
const F80_ZE: u16 = 0x04;
const F80_OE: u16 = 0x08;
const F80_UE: u16 = 0x10;
const F80_PE: u16 = 0x20;

fn f80_raise(flags: u16) {
    unsafe { *(&raw mut crate::data::ra_x87_sw) |= flags }
}

unsafe fn f80_unpack(slot: *const u8) -> F80 {
    let m = core::ptr::read_unaligned(slot as *const u64);
    let se = core::ptr::read_unaligned(slot.add(8) as *const u16);
    let mut v = F80 { kind: F80Kind::Finite, sign: se >> 15 != 0, exp: 0, m };
    let e = i32::from(se & 0x7fff);
    if e == 0x7fff {
        v.kind = if m << 1 == 0 { F80Kind::Inf } else { F80Kind::Nan };
        return v;
    }
    if m == 0 {
        v.kind = F80Kind::Zero;
        return v;
    }
    // A denormal has exponent 0 and the integer bit clear; an unnormal is normalized the same way.
    let shift = m.leading_zeros() as i32;
    v.m = m << shift;
    v.exp = (if e == 0 { 1 } else { e }) - 16383 - shift;
    v
}

unsafe fn f80_write(slot: *mut u8, sign: bool, exponent: u32, m: u64) {
    let se = ((sign as u16) << 15) | exponent as u16;
    core::ptr::write_unaligned(slot as *mut u64, m);
    core::ptr::write_unaligned(slot.add(8) as *mut u16, se);
    core::ptr::write_bytes(slot.add(10), 0, 6);
}

/// The real indefinite: the result of an invalid operation.
unsafe fn f80_write_indefinite(slot: *mut u8) {
    f80_raise(F80_IE);
    f80_write(slot, true, 0x7fff, 0xc000_0000_0000_0000);
}

unsafe fn f80_write_inf(slot: *mut u8, sign: bool) {
    f80_write(slot, sign, 0x7fff, 0x8000_0000_0000_0000);
}

unsafe fn f80_write_quiet(slot: *mut u8, v: F80) {
    f80_write(slot, v.sign, 0x7fff, v.m | 0xc000_0000_0000_0000);
}

/// A NaN, an infinity or a zero goes out as it is, the NaN quiet.
unsafe fn f80_write_special(slot: *mut u8, v: F80) {
    match v.kind {
        F80Kind::Nan => f80_write_quiet(slot, v),
        F80Kind::Inf => f80_write_inf(slot, v.sign),
        _ => f80_write(slot, v.sign, 0, 0),
    }
}

/// The NaN of a two-operand operation: the one with the larger significand, quiet.
unsafe fn f80_nan_result(slot: *mut u8, a: F80, b: F80) -> bool {
    if a.kind != F80Kind::Nan && b.kind != F80Kind::Nan {
        return false;
    }
    let n = if a.kind != F80Kind::Nan {
        b
    } else if b.kind != F80Kind::Nan {
        a
    } else if b.m > a.m {
        b
    } else {
        a
    };
    f80_write_quiet(slot, n);
    true
}

fn f80_rc(cw: u64) -> u32 {
    ((cw >> 10) & 3) as u32
}

fn f80_precision(cw: u64) -> u32 {
    match (cw >> 8) & 3 {
        0 => 24,
        2 => 53,
        _ => 64,
    }
}

/// A result rounded into a binary format: the value `m * 2^(exp - 127)` with bit 127 set (sticky
/// for what lies below), to `p` bits in the mode `rc`, with the exponent `bias` and the all-ones
/// exponent `emax` of the format; the significand comes back left-aligned in 64 bits with its
/// integer bit, and a denormal with the exponent 0.
fn f80_round(sign: bool, exp: i32, mut m: u128, mut sticky: bool, p: u32, rc: u32, emax: i32, bias: i32) -> (i32, u64) {
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
        f80_raise(F80_OE | F80_PE);
        let to_inf = rc == 0 || (rc == 1 && sign) || (rc == 2 && !sign);
        return if to_inf { (emax, 0x8000_0000_0000_0000) } else { (emax - 1, !0u64 << (64 - p)) };
    }
    if inexact {
        f80_raise(if tiny { F80_UE | F80_PE } else { F80_PE });
    }
    (biased, (m >> 64) as u64)
}

/// Whether a finite value below 2^63 has a fraction: the bits of `m` below the units position.
fn f80_has_fraction(v: F80) -> bool {
    v.exp < 0 || v.m << (v.exp + 1) != 0
}

unsafe fn f80_write_rounded(slot: *mut u8, sign: bool, exp: i32, m: u128, sticky: bool, cw: u64) {
    let (biased, m) = f80_round(sign, exp, m, sticky, f80_precision(cw), f80_rc(cw), 0x7fff, 16383);
    f80_write(slot, sign, biased as u32, m);
}

/// A finite or zero value, rounded to the precision of the control word.
unsafe fn f80_write_value(slot: *mut u8, v: F80, cw: u64) {
    let m = if v.kind == F80Kind::Zero { 0 } else { u128::from(v.m) << 64 };
    f80_write_rounded(slot, v.sign, v.exp, m, false, cw);
}

/// The magnitude of the integer nearest a finite value below 2^63, by the mode `rc`.
fn f80_to_integer(v: F80, rc: u32) -> u64 {
    let shift = 63 - v.exp;
    let (q, half, sticky) = if shift > 64 {
        (0, false, true)
    } else if shift == 64 {
        (0, v.m >> 63 != 0, v.m << 1 != 0)
    } else {
        let rest = v.m & ((1u64 << shift) - 1);
        (v.m >> shift, (rest >> (shift - 1)) & 1 != 0, rest & ((1u64 << (shift - 1)) - 1) != 0)
    };
    let up = match rc {
        0 => half && (sticky || q & 1 != 0),
        1 => v.sign && (half || sticky),
        2 => !v.sign && (half || sticky),
        _ => false,
    };
    q + u64::from(up)
}

pub unsafe fn ra_f80_load(slot: *mut u8, memory: *const u8) {
    core::ptr::copy_nonoverlapping(memory, slot, 10);
    core::ptr::write_bytes(slot.add(10), 0, 6);
}

pub unsafe fn ra_f80_store(memory: *mut u8, slot: *const u8) {
    core::ptr::copy_nonoverlapping(slot, memory, 10);
}

pub unsafe fn ra_f80_pack(slot: *mut u8, significand: u64, sign_exponent: u64) {
    f80_write(slot, sign_exponent >> 15 & 1 != 0, sign_exponent as u32 & 0x7fff, significand);
}

pub unsafe fn ra_f80_from_f64(slot: *mut u8, value: f64) {
    let bits = value.to_bits();
    let sign = bits >> 63 != 0;
    let e = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if e == 0x7ff {
        f80_write(slot, sign, 0x7fff, if frac != 0 { 0xc000_0000_0000_0000 | (frac << 11) } else { 0x8000_0000_0000_0000 });
    } else if e == 0 && frac == 0 {
        f80_write(slot, sign, 0, 0);
    } else if e == 0 {
        let shift = frac.leading_zeros();
        f80_write(slot, sign, 15372 - shift, frac << shift);
    } else {
        f80_write(slot, sign, (e - 1023 + 16383) as u32, 0x8000_0000_0000_0000 | (frac << 11));
    }
}

pub unsafe fn ra_f80_from_i64(slot: *mut u8, value: u64) {
    let v = value as i64;
    if v == 0 {
        f80_write(slot, false, 0, 0);
        return;
    }
    let u = v.unsigned_abs();
    let shift = u.leading_zeros();
    f80_write(slot, v < 0, 16383 + 63 - shift, u << shift);
}

pub unsafe fn ra_f80_to_f64(slot: *const u8, cw: u64) -> f64 {
    let v = f80_unpack(slot);
    let sign = u64::from(v.sign) << 63;
    let bits = match v.kind {
        F80Kind::Nan => sign | 0x7ff << 52 | 1 << 51 | ((v.m << 1) >> 12),
        F80Kind::Inf => sign | 0x7ff << 52,
        F80Kind::Zero => sign,
        F80Kind::Finite => {
            let (biased, m) = f80_round(v.sign, v.exp, u128::from(v.m) << 64, false, 53, f80_rc(cw), 0x7ff, 1023);
            sign | (biased as u64) << 52 | ((m << 1) >> 12)
        }
    };
    f64::from_bits(bits)
}

pub unsafe fn ra_f80_to_f32(slot: *const u8, cw: u64) -> f32 {
    let v = f80_unpack(slot);
    let sign = u32::from(v.sign) << 31;
    let bits = match v.kind {
        F80Kind::Nan => sign | 0xff << 23 | 1 << 22 | ((v.m << 1) >> 41) as u32,
        F80Kind::Inf => sign | 0xff << 23,
        F80Kind::Zero => sign,
        F80Kind::Finite => {
            let (biased, m) = f80_round(v.sign, v.exp, u128::from(v.m) << 64, false, 24, f80_rc(cw), 0xff, 127);
            sign | (biased as u32) << 23 | ((m << 1) >> 41) as u32
        }
    };
    f32::from_bits(bits)
}

/// An integer of `bits` bits, rounded by `rounding` (the mode alone, 3 for a truncation), or the
/// integer indefinite when the value is not a number or does not fit.
pub unsafe fn ra_f80_to_int(slot: *const u8, rounding: u64, bits: u64) -> u64 {
    let v = f80_unpack(slot);
    let limit = 1u64 << (bits - 1);
    if v.kind == F80Kind::Zero {
        return 0;
    }
    if v.kind != F80Kind::Finite || v.exp >= bits as i32 - 1 {
        f80_raise(F80_IE);
        return limit;
    }
    let q = f80_to_integer(v, (rounding & 3) as u32);
    if if v.sign { q > limit } else { q >= limit } {
        f80_raise(F80_IE);
        return limit;
    }
    if f80_has_fraction(v) {
        f80_raise(F80_PE);
    }
    if v.sign { 0u64.wrapping_sub(q) } else { q }
}

unsafe fn f80_add(result: *mut u8, pa: *const u8, pb: *const u8, cw: u64, negate: bool) {
    let a = f80_unpack(pa);
    let mut b = f80_unpack(pb);
    b.sign ^= negate;
    let rc = f80_rc(cw);
    if f80_nan_result(result, a, b) {
        return;
    }
    if a.kind == F80Kind::Inf || b.kind == F80Kind::Inf {
        if a.kind == F80Kind::Inf && b.kind == F80Kind::Inf && a.sign != b.sign {
            f80_write_indefinite(result);
        } else {
            f80_write_special(result, if a.kind == F80Kind::Inf { a } else { b });
        }
        return;
    }
    if a.kind == F80Kind::Zero && b.kind == F80Kind::Zero {
        f80_write(result, (a.sign && b.sign) || (rc == 1 && (a.sign || b.sign)), 0, 0);
        return;
    }
    if a.kind == F80Kind::Zero {
        f80_write_value(result, b, cw);
        return;
    }
    if b.kind == F80Kind::Zero {
        f80_write_value(result, a, cw);
        return;
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
            f80_write(result, rc == 1, 0, 0);
            return;
        }
        let lz = diff.leading_zeros() as i32;
        exp -= lz;
        diff << lz
    };
    f80_write_rounded(result, a.sign, exp, m, false, cw);
}

pub unsafe fn ra_f80_add(result: *mut u8, a: *const u8, b: *const u8, cw: u64) {
    f80_add(result, a, b, cw, false);
}

pub unsafe fn ra_f80_sub(result: *mut u8, a: *const u8, b: *const u8, cw: u64) {
    f80_add(result, a, b, cw, true);
}

pub unsafe fn ra_f80_mul(result: *mut u8, pa: *const u8, pb: *const u8, cw: u64) {
    let (a, b) = (f80_unpack(pa), f80_unpack(pb));
    let sign = a.sign ^ b.sign;
    if f80_nan_result(result, a, b) {
        return;
    }
    if a.kind == F80Kind::Inf || b.kind == F80Kind::Inf {
        if a.kind == F80Kind::Zero || b.kind == F80Kind::Zero {
            f80_write_indefinite(result);
        } else {
            f80_write_inf(result, sign);
        }
        return;
    }
    if a.kind == F80Kind::Zero || b.kind == F80Kind::Zero {
        f80_write(result, sign, 0, 0);
        return;
    }
    let mut m = u128::from(a.m) * u128::from(b.m);
    let mut exp = a.exp + b.exp + 1;
    if m >> 127 == 0 {
        m <<= 1;
        exp -= 1;
    }
    f80_write_rounded(result, sign, exp, m, false, cw);
}

pub unsafe fn ra_f80_div(result: *mut u8, pa: *const u8, pb: *const u8, cw: u64) {
    let (a, b) = (f80_unpack(pa), f80_unpack(pb));
    let sign = a.sign ^ b.sign;
    if f80_nan_result(result, a, b) {
        return;
    }
    if a.kind == F80Kind::Inf {
        if b.kind == F80Kind::Inf {
            f80_write_indefinite(result);
        } else {
            f80_write_inf(result, sign);
        }
        return;
    }
    if b.kind == F80Kind::Inf {
        f80_write(result, sign, 0, 0);
        return;
    }
    if b.kind == F80Kind::Zero {
        if a.kind == F80Kind::Zero {
            f80_write_indefinite(result);
        } else {
            f80_raise(F80_ZE);
            f80_write_inf(result, sign);
        }
        return;
    }
    if a.kind == F80Kind::Zero {
        f80_write(result, sign, 0, 0);
        return;
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
    f80_write_rounded(result, sign, exp, q << 63, remainder != 0, cw);
}

pub unsafe fn ra_f80_sqrt(result: *mut u8, pa: *const u8, cw: u64) {
    let v = f80_unpack(pa);
    if v.kind == F80Kind::Nan || v.kind == F80Kind::Zero {
        f80_write_special(result, v);
        return;
    }
    if v.sign {
        f80_write_indefinite(result);
        return;
    }
    if v.kind == F80Kind::Inf {
        f80_write_inf(result, false);
        return;
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
    f80_write_rounded(result, false, 63 + k2, quotient, rest != 0, cw);
}

pub unsafe fn ra_f80_rndint(result: *mut u8, pa: *const u8, cw: u64) {
    let v = f80_unpack(pa);
    if v.kind != F80Kind::Finite {
        f80_write_special(result, v);
        return;
    }
    if v.exp >= 63 {
        f80_write(result, v.sign, (v.exp + 16383) as u32, v.m);
        return;
    }
    let q = f80_to_integer(v, f80_rc(cw));
    if f80_has_fraction(v) {
        f80_raise(F80_PE);
    }
    if q == 0 {
        f80_write(result, v.sign, 0, 0);
        return;
    }
    let shift = q.leading_zeros();
    f80_write(result, v.sign, 16383 + 63 - shift, q << shift);
}

pub unsafe fn ra_f80_chs(result: *mut u8, a: *const u8) {
    let mut bytes = [0u8; 16];
    core::ptr::copy_nonoverlapping(a, bytes.as_mut_ptr(), 16);
    bytes[9] ^= 0x80;
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), result, 16);
}

pub unsafe fn ra_f80_abs(result: *mut u8, a: *const u8) {
    let mut bytes = [0u8; 16];
    core::ptr::copy_nonoverlapping(a, bytes.as_mut_ptr(), 16);
    bytes[9] &= 0x7f;
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), result, 16);
}

/// Less in bit 0, equal in bit 1, unordered in bit 2 (then all three, as the hardware sets its
/// condition codes).
pub unsafe fn ra_f80_cmp(pa: *const u8, pb: *const u8) -> u64 {
    let (a, b) = (f80_unpack(pa), f80_unpack(pb));
    if a.kind == F80Kind::Nan || b.kind == F80Kind::Nan {
        return 7;
    }
    if a.kind == F80Kind::Zero && b.kind == F80Kind::Zero {
        return 2;
    }
    let order = if a.sign != b.sign {
        if a.sign { -1 } else { 1 }
    } else {
        // The magnitudes: an infinity beyond everything, a zero below, else by exponent then significand.
        let magnitude = if a.kind == F80Kind::Inf || b.kind == F80Kind::Inf {
            i32::from(a.kind == F80Kind::Inf) - i32::from(b.kind == F80Kind::Inf)
        } else if a.kind == F80Kind::Zero || b.kind == F80Kind::Zero {
            i32::from(b.kind == F80Kind::Zero) - i32::from(a.kind == F80Kind::Zero)
        } else if a.exp != b.exp {
            if a.exp > b.exp { 1 } else { -1 }
        } else {
            match a.m.cmp(&b.m) {
                core::cmp::Ordering::Greater => 1,
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
            }
        };
        if a.sign { -magnitude } else { magnitude }
    };
    match order {
        0 => 2,
        o if o < 0 => 1,
        _ => 0,
    }
}

/// A jump the structuring could not express: Rust has no `goto`, and the block was too long to
/// write a second time. Names the function and the block, then stops the port.
pub fn ra_goto(function: &str, label: &str) -> ! {
    eprintln!("ra: a goto to {label} in {function}, which the Rust output cannot express");
    std::process::exit(0xE000_0005u32 as i32)
}
