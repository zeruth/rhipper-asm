//! The host's COM objects behind a 32-bit program's vtables: an interface pointer the 64-bit host
//! hands the program (the shell's `IMalloc`, an object of the port's runtime) points to a vtable of
//! 8-byte slots, and the program calls through 4-byte ones, pushing its arguments for `__stdcall`.
//! Each host object becomes a guest object below 4 GB, a vtable pointer in its first word, whose
//! slots are thunks: addresses the host answers a call of ([`Objects::call`]) by the slot's
//! method of the host object, each argument widened and the result narrowed by the interface's
//! prototype ([`interface`]). The guest object holds the program's references to the host's: an
//! `AddRef` or a `Release` through it is the host's, and its last `Release` retires it. An
//! interface pointer a method hands back (`QueryInterface`'s) becomes a guest object of its own,
//! or the same one for the same host object and interface, which COM's identity rule needs.
//!
//! The lift is exact or it stops: a result that does not fit the program's 4 bytes, a call
//! through no live object, or an interface the host has no prototype of stops the call with the
//! method named.

use std::collections::HashMap;
use std::ffi::c_void;

use ra_asm_ir::interp::{Memory, Region, Stop};

/// A method of an interface, by its prototype's kind letters: the first is the result's, the rest
/// are the parameters' after `this`, in order. `v` is nothing (a result only); `i` a 32-bit integer
/// of one width at both (`HRESULT`, `ULONG`, `int`); `u` an unsigned integer of the address width
/// (`SIZE_T`), zero-extended and narrowed only when it fits; `s` a signed one, sign-extended and
/// narrowed only when it fits as a signed 32-bit value; `p` a pointer, as `u`; `g` a pointer to
/// an interface identifier (`REFIID`); `o` the `void **` the host writes an interface pointer of
/// the identifier `g` names through.
#[derive(Debug, PartialEq, Eq)]
pub struct Method {
    pub name: &'static str,
    pub kinds: &'static str,
}

/// An interface: its identifier, the interface it derives from, and its own methods in vtable
/// order after its base's.
#[derive(Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: &'static str,
    /// The identifier as its 16 bytes lie in memory.
    pub iid: [u8; 16],
    pub base: Option<&'static Interface>,
    pub methods: &'static [Method],
}

impl Interface {
    /// Its methods in vtable order, its bases' first.
    pub fn slots(&self) -> Vec<&'static Method> {
        let mut slots = self.base.map_or_else(Vec::new, Interface::slots);
        slots.extend(self.methods.iter());
        slots
    }
}

/// The bytes of a GUID as `{data1-data2-data3-data4}` lays them out.
const fn guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> [u8; 16] {
    let (a, b, c, d) = (data1.to_le_bytes(), data2.to_le_bytes(), data3.to_le_bytes(), data4);
    [a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]
}

/// The tail of the identifiers OLE's own interfaces have.
const OLE: [u8; 8] = [0xC0, 0, 0, 0, 0, 0, 0, 0x46];

static IUNKNOWN: Interface = Interface {
    name: "IUnknown",
    iid: guid(0, 0, 0, OLE),
    base: None,
    methods: &[
        Method { name: "QueryInterface", kinds: "igo" },
        Method { name: "AddRef", kinds: "i" },
        Method { name: "Release", kinds: "i" },
    ],
};

static IMALLOC: Interface = Interface {
    name: "IMalloc",
    iid: guid(2, 0, 0, OLE),
    base: Some(&IUNKNOWN),
    methods: &[
        Method { name: "Alloc", kinds: "pu" },
        Method { name: "Realloc", kinds: "ppu" },
        Method { name: "Free", kinds: "vp" },
        // Its answer for a block it did not allocate is `(SIZE_T)-1`, the program's as well.
        Method { name: "GetSize", kinds: "sp" },
        Method { name: "DidAlloc", kinds: "ip" },
        Method { name: "HeapMinimize", kinds: "v" },
    ],
};

/// The interfaces the host has a prototype of.
static INTERFACES: [&Interface; 2] = [&IUNKNOWN, &IMALLOC];

/// The slots of `IUnknown` every interface begins with.
const ADD_REF: usize = 1;
const RELEASE: usize = 2;

/// The interface the host has a prototype of by its name.
pub fn interface(name: &str) -> Option<&'static Interface> {
    INTERFACES.iter().copied().find(|i| i.name == name)
}

/// The interface the host has a prototype of by its identifier.
pub fn by_iid(iid: &[u8; 16]) -> Option<&'static Interface> {
    INTERFACES.iter().copied().find(|i| &i.iid == iid)
}

/// An identifier as the registry writes it.
fn iid_text(iid: &[u8; 16]) -> String {
    let data1 = u32::from_le_bytes([iid[0], iid[1], iid[2], iid[3]]);
    let (data2, data3) = (u16::from_le_bytes([iid[4], iid[5]]), u16::from_le_bytes([iid[6], iid[7]]));
    let d = &iid[8..];
    format!(
        "{{{data1:08X}-{data2:04X}-{data3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

/// The registers of the 32-bit machine state a lifted function takes, 8 bytes each in the lift's
/// order.
const EAX: u64 = 0;
const ESP: u64 = 32;

/// The space of the objects: a page of thunks, then the vtables and the objects.
const SPACE: u64 = 0x1_0000;
const THUNKS: u64 = 0x1000;
const THUNK: u64 = 16;
/// A guest object: its vtable pointer, and a word of padding that keeps the next one aligned.
const CELL: u64 = 8;

/// A guest object: the host's object it stands for and the program's references to it.
struct Proxy {
    host: u64,
    interface: &'static Interface,
    references: u32,
}

/// The host objects a 32-bit program holds, as the guest objects it calls them through.
pub struct Objects {
    space: Low,
    /// The method of each thunk, by its index in the thunk page.
    thunks: Vec<(&'static Interface, usize)>,
    /// The vtable of each interface, made at its first object.
    vtables: HashMap<&'static str, u32>,
    /// The first byte of the space no vtable or object has taken.
    next: u64,
    /// The cells of retired objects.
    free: Vec<u32>,
    proxies: HashMap<u32, Proxy>,
    /// The guest object of a host object as an interface.
    by_host: HashMap<(u64, &'static str), u32>,
}

impl Objects {
    /// The objects' space below 4 GB, a region of `memory`. The space is released with the
    /// objects: remove its region from the memory first when the memory outlives them.
    pub fn new(memory: &mut Memory) -> Result<Self, Stop> {
        let space = Low::new(SPACE).ok_or_else(|| Stop::fault("no room below 4 GB for the host's COM objects"))?;
        memory.add_region(Region::new(space.start, space.size, "com objects"));
        // A thunk is no code of the program's: read as code, it is a breakpoint.
        memory.fill(space.start, THUNKS, 0xCC)?;
        let next = space.start + THUNKS;
        Ok(Self {
            space,
            thunks: Vec::new(),
            vtables: HashMap::new(),
            next,
            free: Vec::new(),
            proxies: HashMap::new(),
            by_host: HashMap::new(),
        })
    }

    /// The first address of the objects' space, their region's in the memory.
    pub fn start(&self) -> u64 {
        self.space.start
    }

    /// The guest object of the host's interface pointer `host`, which takes over the reference
    /// the host handed out: a new one, or the one the program already holds for it, which then
    /// holds one reference more.
    ///
    /// # Safety
    /// `host` points to an object of the host's that implements `interface`, and holds a
    /// reference the caller gives up.
    pub unsafe fn wrap(&mut self, memory: &mut Memory, host: u64, interface: &'static Interface) -> Result<u32, Stop> {
        if let Some(&guest) = self.by_host.get(&(host, interface.name))
            && let Some(proxy) = self.proxies.get_mut(&guest)
        {
            proxy.references += 1;
            return Ok(guest);
        }
        let vtable = self.vtable(memory, interface)?;
        let guest = match self.free.pop() {
            Some(cell) => cell,
            None => self.take(CELL, interface)?,
        };
        memory.write_u32(u64::from(guest), vtable)?;
        memory.write_u32(u64::from(guest) + 4, 0)?;
        self.proxies.insert(guest, Proxy { host, interface, references: 1 });
        self.by_host.insert((host, interface.name), guest);
        Ok(guest)
    }

    /// The program's references to the host object the guest object `guest` stands for; `None`
    /// when no live object is there.
    pub fn references(&self, guest: u32) -> Option<u32> {
        self.proxies.get(&guest).map(|proxy| proxy.references)
    }

    /// The host object the guest object `guest` stands for.
    pub fn host(&self, guest: u32) -> Option<u64> {
        self.proxies.get(&guest).map(|proxy| proxy.host)
    }

    /// Whether `target` is a thunk of a host object's method.
    pub fn answers(&self, target: u64) -> bool {
        self.thunk(target).is_some()
    }

    fn thunk(&self, target: u64) -> Option<(&'static Interface, usize)> {
        let offset = target.checked_sub(self.space.start).filter(|&offset| offset < THUNKS && offset % THUNK == 0)?;
        self.thunks.get((offset / THUNK) as usize).copied()
    }

    /// `len` bytes of the space.
    fn take(&mut self, len: u64, interface: &Interface) -> Result<u32, Stop> {
        let at = self.next;
        if at + len > self.space.start + self.space.size {
            return Err(Stop::fault(format!("no room below 4 GB for another {} object", interface.name)));
        }
        self.next += len;
        Ok(at as u32)
    }

    /// The vtable of `interface`'s guest objects, one thunk a slot.
    fn vtable(&mut self, memory: &mut Memory, interface: &'static Interface) -> Result<u32, Stop> {
        if let Some(&vtable) = self.vtables.get(interface.name) {
            return Ok(vtable);
        }
        let slots = interface.slots().len();
        if (self.thunks.len() + slots) as u64 > THUNKS / THUNK {
            return Err(Stop::fault(format!("no room for the thunks of {}", interface.name)));
        }
        let vtable = self.take(4 * slots as u64, interface)?;
        for slot in 0..slots {
            let thunk = self.space.start + THUNK * self.thunks.len() as u64;
            self.thunks.push((interface, slot));
            memory.write_u32(u64::from(vtable) + 4 * slot as u64, thunk as u32)?;
        }
        self.vtables.insert(interface.name, vtable);
        Ok(vtable)
    }

    /// The program's call of the thunk `target` over the 32-bit machine state at `state`, as the
    /// lift makes an indirect call: `this` and the arguments are on the program's stack above its
    /// return address, the result goes to `eax`, and the callee pops all of them, as `__stdcall`
    /// does.
    pub fn call(&mut self, memory: &mut Memory, state: u64, target: u64) -> Result<(), Stop> {
        let (interface, slot) =
            self.thunk(target).ok_or_else(|| Stop::fault(format!("a call of {target:#x}, which is no method of a host object")))?;
        let method = interface.slots()[slot];
        let what = format!("{}::{}", interface.name, method.name);
        let esp = memory.read_u64(state + ESP)? as u32;
        let this = memory.read_u32(u64::from(esp) + 4)?;
        let host = match self.proxies.get(&this) {
            Some(proxy) if std::ptr::eq(proxy.interface, interface) => proxy.host,
            _ => return Err(Stop::fault(format!("{what} of {this:#x}, which is no {} object the host handed out", interface.name))),
        };
        let (result, params) = method.kinds.split_at(1);
        let mut args = Vec::with_capacity(params.len());
        let (mut iid, mut out) = (None, None);
        let mut written = 0u64;
        for (n, kind) in params.chars().enumerate() {
            let value = memory.read_u32(u64::from(esp) + 8 + 4 * n as u64)?;
            args.push(match kind {
                'i' | 'u' | 'p' => u64::from(value),
                's' => value as i32 as i64 as u64,
                'g' => {
                    iid = Some(value);
                    u64::from(value)
                }
                'o' => {
                    out = Some(value);
                    (&raw mut written).expose_provenance() as u64
                }
                _ => return Err(Stop::fault(format!("{what}: its prototype has the kind {kind:?}, which the host does not read"))),
            });
        }
        if args.len() > 7 {
            return Err(Stop::fault(format!("{what}: {} parameters, more than the host calls with", args.len())));
        }
        // `wrap` took the object as the host's of this interface, whose vtable has the slot.
        let returned = unsafe { invoke(host, slot, &args) };
        let narrow = |fits: bool| {
            if fits {
                Ok(returned as u32)
            } else {
                Err(Stop::fault(format!("{what}: the host's {returned:#x} does not fit in the program's 4 bytes")))
            }
        };
        let eax = match result {
            "v" => None,
            "i" => Some(returned as u32),
            "s" => Some(narrow(i32::try_from(returned as i64).is_ok())?),
            _ => Some(narrow(u32::try_from(returned).is_ok())?),
        };
        if let Some(at) = out {
            let guest = if written == 0 { 0 } else { self.handed_back(memory, &what, iid, written)? };
            memory.write_u32(u64::from(at), guest)?;
        }
        match slot {
            ADD_REF => {
                if let Some(proxy) = self.proxies.get_mut(&this) {
                    proxy.references += 1;
                }
            }
            RELEASE => self.release(memory, this)?,
            _ => {}
        }
        if let Some(eax) = eax {
            memory.write_u64(state + EAX, u64::from(eax))?;
        }
        let pops = 4 * (1 + params.len() as u32);
        memory.write_u64(state + ESP, u64::from(esp.wrapping_add(4 + pops)))?;
        Ok(())
    }

    /// The guest object of the interface pointer `host` a method wrote, of the interface the
    /// identifier at `iid` names.
    fn handed_back(&mut self, memory: &mut Memory, what: &str, iid: Option<u32>, host: u64) -> Result<u32, Stop> {
        let at = iid.ok_or_else(|| Stop::fault(format!("{what}: its prototype names no identifier for the interface it hands back")))?;
        let mut bytes = [0; 16];
        memory.read(u64::from(at), &mut bytes)?;
        let interface = by_iid(&bytes).ok_or_else(|| {
            Stop::fault(format!("{what}: the host handed back an interface {}, which it has no prototype of", iid_text(&bytes)))
        })?;
        // The host object implements the interface it answered for, and handed out a reference.
        unsafe { self.wrap(memory, host, interface) }
    }

    /// One of the program's references to the guest object `guest` dropped: the last retires it.
    fn release(&mut self, memory: &mut Memory, guest: u32) -> Result<(), Stop> {
        let Some(proxy) = self.proxies.get_mut(&guest) else { return Ok(()) };
        proxy.references = proxy.references.saturating_sub(1);
        if proxy.references > 0 {
            return Ok(());
        }
        let key = (proxy.host, proxy.interface.name);
        self.proxies.remove(&guest);
        self.by_host.remove(&key);
        // A call through the object after its last release reads no vtable.
        memory.write_u32(u64::from(guest), 0)?;
        self.free.push(guest);
        Ok(())
    }
}

/// A method of a host object as the host calls it: `this` and seven integers.
type Method8 = unsafe extern "system" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

/// Calls the method in `slot` of the vtable of the host object `host` with `args` after `this`.
///
/// # Safety
/// `host` points to an object whose vtable has `slot`, a method that takes at most seven integers
/// after `this`, and `args` are what it expects.
unsafe fn invoke(host: u64, slot: usize, args: &[u64]) -> u64 {
    let mut a = [0u64; 7];
    a[..args.len()].copy_from_slice(args);
    unsafe {
        let vtable = *(host as usize as *const *const usize);
        let method = std::mem::transmute::<usize, Method8>(*vtable.add(slot));
        method(host, a[0], a[1], a[2], a[3], a[4], a[5], a[6])
    }
}

/// Memory of the host's below 4 GB, where a 32-bit program's pointers reach, released when
/// dropped.
pub struct Low {
    start: u64,
    size: u64,
}

impl Low {
    /// `size` bytes at the first free spot of a walk from 256 MB up in 16 MB steps.
    pub fn new(size: u64) -> Option<Self> {
        (0x1000_0000u64..0x8000_0000).step_by(0x0100_0000).find_map(|base| reserve(base, size))
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for Low {
    fn drop(&mut self) {
        release(self.start, self.size);
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
        pub(super) fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
    }

    pub(super) const MEM_COMMIT: u32 = 0x1000;
    pub(super) const MEM_RESERVE: u32 = 0x2000;
    pub(super) const MEM_RELEASE: u32 = 0x8000;
    pub(super) const PAGE_READWRITE: u32 = 0x04;
    pub(super) const PAGE_NOACCESS: u32 = 0x01;
}

/// Memory at `base` exactly, or `None` when the process has something there.
#[cfg(windows)]
fn reserve(base: u64, size: u64) -> Option<Low> {
    // Reserve before committing: a commit succeeds on pages anyone reserved, a reservation only
    // on free ones.
    let p = unsafe { sys::VirtualAlloc(base as usize as *mut c_void, size as usize, sys::MEM_RESERVE, sys::PAGE_NOACCESS) };
    if p as usize as u64 != base {
        return None;
    }
    let committed = unsafe { sys::VirtualAlloc(p, size as usize, sys::MEM_COMMIT, sys::PAGE_READWRITE) };
    if committed.is_null() {
        release(base, size);
        return None;
    }
    Some(Low { start: base, size })
}

#[cfg(windows)]
fn release(start: u64, _size: u64) {
    unsafe { sys::VirtualFree(start as usize as *mut c_void, 0, sys::MEM_RELEASE) };
}

#[cfg(not(windows))]
mod sys {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub(super) fn mmap(address: *mut c_void, len: usize, protect: i32, flags: i32, fd: i32, offset: i64) -> *mut c_void;
        pub(super) fn munmap(address: *mut c_void, len: usize) -> i32;
    }

    pub(super) const PROT_READ: i32 = 1;
    pub(super) const PROT_WRITE: i32 = 2;
    pub(super) const MAP_PRIVATE: i32 = 2;
    #[cfg(target_os = "linux")]
    pub(super) const MAP_ANONYMOUS: i32 = 0x20;
    #[cfg(not(target_os = "linux"))]
    pub(super) const MAP_ANONYMOUS: i32 = 0x1000;
}

/// The address is a hint, not `MAP_FIXED`: a mapping someone holds is never replaced, and one the
/// kernel placed elsewhere is given back.
#[cfg(not(windows))]
fn reserve(base: u64, size: u64) -> Option<Low> {
    let flags = sys::MAP_PRIVATE | sys::MAP_ANONYMOUS;
    let p = unsafe { sys::mmap(base as usize as *mut c_void, size as usize, sys::PROT_READ | sys::PROT_WRITE, flags, -1, 0) };
    // MAP_FAILED is all ones.
    if p as usize == usize::MAX {
        return None;
    }
    if p as usize as u64 != base {
        unsafe { sys::munmap(p, size as usize) };
        return None;
    }
    Some(Low { start: base, size })
}

#[cfg(not(windows))]
fn release(start: u64, size: u64) {
    unsafe { sys::munmap(start as usize as *mut c_void, size as usize) };
}

#[cfg(test)]
mod tests;
