//! The conformance of the host's Win32 structure adapters: `testdata/conformance/win32_structs.toml`
//! names each structure a 32-bit program hands the 64-bit host, or the host hands it, with the way it
//! crosses the call and whether the adapter converts it yet. Each structure is filled as the 32-bit
//! program lays it out, converted by the adapter and compared with what its layouts at both widths
//! in `ra-asm-protos` say the other side holds. A pending row that converts fails the test, so the
//! table follows the adapter.
//!
//! Beside it, `testdata/conformance/win32_com.toml` names the COM interfaces the host hands a 32-bit
//! program: an object of the host's is wrapped as the interface, and the program's calls go
//! through its guest object's vtable as an x86 `__stdcall` makes them. A pending row that calls
//! fails the test the same way.

use std::path::PathBuf;

use ra_asm_host::adapt;
use ra_asm_host::com::{self, Low, Objects};
use ra_asm_ir::interp::{Memory, Region, Stop};
use ra_asm_protos::{Kind, Place, Record, converted_fields, layout};
use ra_asm_testing::manifest;

/// The structures the ignore reasons of rhtest's misc, sys and win name.
const NAMED: [&str; 11] = [
    "WNDCLASSA",
    "WNDCLASSEXA",
    "CREATESTRUCTA",
    "MSG",
    "OVERLAPPED",
    "MEMORY_BASIC_INFORMATION",
    "SYSTEM_INFO",
    "SLIST_HEADER",
    "INIT_ONCE",
    "WSADATA",
    "CRITICAL_SECTION",
];
/// The rows still pending: a change that makes one convert marks it passing and lowers this.
const PENDING: usize = 4;

/// The way a structure crosses the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// The program fills it and the host reads it.
    In,
    /// The host fills it and the program reads it.
    Out,
    /// The host reads it and writes it back.
    Both,
}

impl Direction {
    fn widens(self) -> bool {
        matches!(self, Direction::In | Direction::Both)
    }

    fn narrows(self) -> bool {
        matches!(self, Direction::Out | Direction::Both)
    }
}

/// A row of the table.
#[derive(Debug)]
struct Row {
    name: String,
    direction: Direction,
    pending: bool,
    /// The line of its `[[struct]]` header.
    line: usize,
}

fn table() -> PathBuf {
    manifest::workspace().join("testdata/conformance/win32_structs.toml")
}

/// The rows of the table text, in order; `#` starts a comment, and a value holds none.
fn parse(text: &str) -> Result<Vec<Row>, String> {
    struct Partial {
        line: usize,
        name: Option<String>,
        direction: Option<Direction>,
        pending: Option<bool>,
    }
    fn finish(partial: Partial) -> Result<Row, String> {
        let line = partial.line;
        let missing = |key: &str| format!("win32_structs.toml:{line}: the [[struct]] has no `{key}`");
        Ok(Row {
            name: partial.name.ok_or_else(|| missing("name"))?,
            direction: partial.direction.ok_or_else(|| missing("direction"))?,
            pending: partial.pending.ok_or_else(|| missing("status"))?,
            line,
        })
    }
    let mut rows = Vec::new();
    let mut current: Option<Partial> = None;
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let wrong = |what: &str| format!("win32_structs.toml:{number}: {line:?}: {what}");
        if line == "[[struct]]" {
            if let Some(partial) = current.take() {
                rows.push(finish(partial)?);
            }
            current = Some(Partial { line: number, name: None, direction: None, pending: None });
            continue;
        }
        let Some(partial) = current.as_mut() else { return Err(wrong("is outside a [[struct]]")) };
        let Some((key, value)) = line.split_once('=') else { return Err(wrong("is not `key = \"value\"`")) };
        let value = value.trim();
        let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            return Err(wrong("the value is not a string"));
        };
        match key.trim() {
            "name" if partial.name.is_none() => partial.name = Some(value.to_owned()),
            "direction" if partial.direction.is_none() => {
                partial.direction = Some(match value {
                    "in" => Direction::In,
                    "out" => Direction::Out,
                    "both" => Direction::Both,
                    _ => return Err(wrong("the direction is not in, out or both")),
                });
            }
            "status" if partial.pending.is_none() => {
                partial.pending = Some(match value {
                    "pending" => true,
                    "passing" => false,
                    _ => return Err(wrong("the status is not pending or passing")),
                });
            }
            "name" | "direction" | "status" => return Err(wrong("the key repeats")),
            _ => return Err(wrong("the key is not name, direction or status")),
        }
    }
    if let Some(partial) = current {
        rows.push(finish(partial)?);
    }
    Ok(rows)
}

fn rows() -> Vec<Row> {
    let path = table();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    parse(&text).unwrap_or_else(|e| panic!("{e}"))
}

/// The host's conversion of a 32-bit program's structure to its own layout; a structure the host
/// has no adapter for fails here.
fn to_host(record: &Record, guest: &[u8]) -> Result<Vec<u8>, String> {
    adapt::adapter(record.name)?.to_host(guest)
}

/// The host's conversion of its own structure to the 32-bit program's layout.
fn to_guest(record: &Record, host: &[u8]) -> Result<Vec<u8>, String> {
    adapt::adapter(record.name)?.to_guest(host)
}

/// The bytes of a place at an offset from a base.
fn span(place: Place, base: usize) -> std::ops::Range<usize> {
    let start = base + place.offset as usize;
    start..start + place.size as usize
}

/// Fills every field of `record` at `base` in `guest` with bytes that differ along the structure,
/// so that a field moved, cut or extended the wrong way shows; padding stays zero.
fn fill(record: &Record, guest: &mut [u8], base: usize) -> Result<(), String> {
    for field in record.fields {
        if field.kind == Kind::Record {
            let inner = layout(field.record).ok_or_else(|| format!("{}: the layout table does not have {}", record.name, field.record))?;
            for element in 0..field.count as usize {
                fill(inner, guest, base + field.at32.offset as usize + element * inner.size32 as usize)?;
            }
        } else {
            for at in span(field.at32, base) {
                #[allow(clippy::cast_possible_truncation)]
                let byte = (at as u8).wrapping_mul(29) ^ 0x96;
                guest[at] = byte;
            }
        }
    }
    Ok(())
}

/// What the host holds for the 32-bit structure `guest` at `from` once widened, written at `to` in
/// `host`: a pointer and an unsigned integer zero-extended (the port keeps the program's 32-bit
/// addresses), a signed integer sign-extended, a float and a bit field's unit as they are; a
/// union by its members that keep their layout.
fn widen(record: &Record, guest: &[u8], from: usize, host: &mut [u8], to: usize) -> Result<(), String> {
    for field in converted_fields(record)? {
        let unequal = || {
            format!(
                "{}.{}: a {:?} field of {} bytes at 32 bits and {} at 64",
                record.name, field.name, field.kind, field.at32.size, field.at64.size
            )
        };
        match field.kind {
            Kind::Record => {
                let inner =
                    layout(field.record).ok_or_else(|| format!("{}: the layout table does not have {}", record.name, field.record))?;
                for element in 0..field.count as usize {
                    let at32 = from + field.at32.offset as usize + element * inner.size32 as usize;
                    let at64 = to + field.at64.offset as usize + element * inner.size64 as usize;
                    widen(inner, guest, at32, host, at64)?;
                }
            }
            Kind::Float | Kind::Bits => {
                if field.at32.size != field.at64.size {
                    return Err(unequal());
                }
                host[span(field.at64, to)].copy_from_slice(&guest[span(field.at32, from)]);
            }
            Kind::Pointer | Kind::Signed | Kind::Unsigned => {
                let count = field.count.max(1) as usize;
                let (narrow, wide) = (field.at32.size as usize / count, field.at64.size as usize / count);
                if wide < narrow {
                    return Err(unequal());
                }
                for element in 0..count {
                    let source = &guest[from + field.at32.offset as usize + element * narrow..][..narrow];
                    let target = &mut host[to + field.at64.offset as usize + element * wide..][..wide];
                    let negative = field.kind == Kind::Signed && source.last().is_some_and(|top| top & 0x80 != 0);
                    target[..narrow].copy_from_slice(source);
                    target[narrow..].fill(if negative { 0xFF } else { 0 });
                }
            }
        }
    }
    Ok(())
}

/// The first offset where two forms of a structure differ, or their lengths.
fn compare(what: &str, expected: &[u8], actual: &[u8]) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!("{what}: {} bytes, not {}", actual.len(), expected.len()));
    }
    match expected.iter().zip(actual).position(|(e, a)| e != a) {
        Some(at) => Err(format!("{what}: byte {at} is {:#04x}, not {:#04x}", actual[at], expected[at])),
        None => Ok(()),
    }
}

/// Converts the row's structure the way it crosses the call and compares with its layouts.
fn conform(row: &Row) -> Result<(), String> {
    let record = layout(&row.name).ok_or_else(|| format!("{}: the layout table does not have it", row.name))?;
    let mut guest = vec![0; record.size32 as usize];
    fill(record, &mut guest, 0)?;
    let mut host = vec![0; record.size64 as usize];
    widen(record, &guest, 0, &mut host, 0)?;
    if row.direction.widens() {
        compare(&format!("{} widened", row.name), &host, &to_host(record, &guest)?)?;
    }
    if row.direction.narrows() {
        compare(&format!("{} narrowed", row.name), &guest, &to_guest(record, &host)?)?;
    }
    Ok(())
}

#[test]
fn the_table_names_every_structure_of_the_ignore_reasons() {
    let rows = rows();
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, NAMED, "win32_structs.toml names other structures than the ignore reasons");
    let pending = rows.iter().filter(|row| row.pending).count();
    assert_eq!(pending, PENDING, "win32_structs.toml has {pending} rows pending, not {PENDING}");
}

#[test]
fn every_row_converts_as_its_status_says() {
    let mut failures = Vec::new();
    for row in rows() {
        match (conform(&row), row.pending) {
            (Ok(()), true) => failures.push(format!("win32_structs.toml:{}: {} converts: mark it passing", row.line, row.name)),
            (Err(e), false) => failures.push(format!("win32_structs.toml:{}: {} is passing but fails: {e}", row.line, row.name)),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{} rows are not as their status says:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn the_expected_host_form_extends_each_field_by_its_kind() {
    let record = layout("MSG").expect("the layout table has MSG");
    let mut guest = vec![0; record.size32 as usize];
    guest[0..4].copy_from_slice(&0x0040_1000_u32.to_le_bytes());
    guest[8..12].copy_from_slice(&0x8000_0001_u32.to_le_bytes());
    guest[12..16].copy_from_slice(&(-2_i32).to_le_bytes());
    guest[20..24].copy_from_slice(&(-7_i32).to_le_bytes());
    let mut host = vec![0; record.size64 as usize];
    widen(record, &guest, 0, &mut host, 0).expect("MSG widens");
    assert_eq!(host[0..8], 0x0040_1000_u64.to_le_bytes(), "hwnd is zero-extended");
    assert_eq!(host[16..24], 0x8000_0001_u64.to_le_bytes(), "wParam is zero-extended");
    assert_eq!(host[24..32], (-2_i64).to_le_bytes(), "lParam is sign-extended");
    assert_eq!(host[36..40], (-7_i32).to_le_bytes(), "pt.x keeps its width");
}

#[test]
fn the_parser_refuses_a_row_it_cannot_read() {
    let row = "[[struct]]\nname = \"MSG\"\ndirection = \"both\"\nstatus = \"pending\"\n";
    assert_eq!(parse(row).map(|rows| rows.len()), Ok(1));
    assert!(parse("[[struct]]\nname = \"MSG\"\nstatus = \"pending\"\n").is_err(), "a row without a direction");
    assert!(parse("[[struct]]\nname = \"MSG\"\ndirection = \"sideways\"\nstatus = \"pending\"\n").is_err(), "an unknown direction");
    assert!(parse("name = \"MSG\"\n").is_err(), "a key outside a row");
    assert!(parse(&format!("{row}size = \"4\"\n")).is_err(), "an unknown key");
}

// region The COM interfaces

/// The interface rows that call: a change that makes a pending one call marks it pass and raises
/// this.
const INTERFACES_PASSING: usize = 2;

/// A row of the interface table: its name, whether it is pending, the line of its header.
#[derive(Debug)]
struct InterfaceRow {
    name: String,
    pending: bool,
    line: usize,
}

/// The rows of the interface table text, in order; `#` starts a comment, and a value holds none.
fn parse_interfaces(text: &str) -> Result<Vec<InterfaceRow>, String> {
    fn finish(line: usize, name: Option<String>, pending: Option<bool>) -> Result<InterfaceRow, String> {
        let missing = |key: &str| format!("win32_com.toml:{line}: the [[interface]] has no `{key}`");
        Ok(InterfaceRow { name: name.ok_or_else(|| missing("name"))?, pending: pending.ok_or_else(|| missing("status"))?, line })
    }
    let mut rows = Vec::new();
    let mut current: Option<(usize, Option<String>, Option<bool>)> = None;
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let wrong = |what: &str| format!("win32_com.toml:{number}: {line:?}: {what}");
        if line == "[[interface]]" {
            if let Some((at, name, pending)) = current.take() {
                rows.push(finish(at, name, pending)?);
            }
            current = Some((number, None, None));
            continue;
        }
        let Some((_, name, pending)) = current.as_mut() else { return Err(wrong("is outside an [[interface]]")) };
        let Some((key, value)) = line.split_once('=') else { return Err(wrong("is not `key = \"value\"`")) };
        let Some(value) = value.trim().strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            return Err(wrong("the value is not a string"));
        };
        match key.trim() {
            "name" if name.is_none() => *name = Some(value.to_owned()),
            "status" if pending.is_none() => {
                *pending = Some(match value {
                    "pending" => true,
                    "pass" => false,
                    _ => return Err(wrong("the status is not pending or pass")),
                });
            }
            "name" | "status" => return Err(wrong("the key repeats")),
            _ => return Err(wrong("the key is not name or status")),
        }
    }
    if let Some((at, name, pending)) = current {
        rows.push(finish(at, name, pending)?);
    }
    Ok(rows)
}

use host_malloc::{E_NOINTERFACE, HOST_MALLOC, HostMalloc, S_OK};

/// The host's `IMalloc`, whose methods take the eight integers the thunks call every method with.
#[allow(clippy::too_many_arguments)]
mod host_malloc {
    use ra_asm_host::com::{self, Low};

    /// A method of the host object as the host's vtable holds it: `this` and seven integers, the
    /// shape the guest object's thunks call with.
    pub(super) type HostMethod = unsafe extern "system" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

    /// `IMalloc` as an object of the host's, the way the shell's is reached through its vtable: a
    /// count of references and a heap of its own below 4 GB, each block after an 8-byte size.
    #[repr(C)]
    pub(super) struct HostMalloc {
        pub(super) vtable: &'static [HostMethod; 9],
        pub(super) references: std::cell::Cell<u32>,
        pub(super) next: std::cell::Cell<u64>,
        pub(super) heap: Low,
    }

    pub(super) const S_OK: u64 = 0;
    pub(super) const E_NOINTERFACE: u64 = 0x8000_4002;

    pub(super) static HOST_MALLOC: [HostMethod; 9] =
        [query_interface, add_ref, release, alloc, realloc, free, get_size, did_alloc, heap_minimize];

    /// The host object behind `this`.
    ///
    /// # Safety
    /// `this` is a live `HostMalloc`.
    unsafe fn host_malloc<'a>(this: u64) -> &'a HostMalloc {
        unsafe { &*(this as usize as *const HostMalloc) }
    }

    unsafe extern "system" fn query_interface(this: u64, riid: u64, out: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let object = unsafe { host_malloc(this) };
        let iid = unsafe { *(riid as usize as *const [u8; 16]) };
        let out = out as usize as *mut u64;
        let known = ["IUnknown", "IMalloc"].iter().any(|name| com::interface(name).is_some_and(|i| i.iid == iid));
        if known {
            object.references.set(object.references.get() + 1);
            unsafe { out.write(this) };
            S_OK
        } else {
            unsafe { out.write(0) };
            E_NOINTERFACE
        }
    }

    unsafe extern "system" fn add_ref(this: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let object = unsafe { host_malloc(this) };
        object.references.set(object.references.get() + 1);
        u64::from(object.references.get())
    }

    unsafe extern "system" fn release(this: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let object = unsafe { host_malloc(this) };
        object.references.set(object.references.get() - 1);
        u64::from(object.references.get())
    }

    unsafe extern "system" fn alloc(this: u64, size: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let object = unsafe { host_malloc(this) };
        let at = object.next.get();
        let end = at + 8 + size.next_multiple_of(8);
        if end > object.heap.start() + object.heap.size() {
            return 0;
        }
        object.next.set(end);
        unsafe { (at as usize as *mut u64).write(size) };
        at + 8
    }

    unsafe extern "system" fn realloc(this: u64, block: u64, size: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let fresh = unsafe { alloc(this, size, 0, 0, 0, 0, 0, 0) };
        if fresh != 0 && block != 0 {
            let old = unsafe { get_size(this, block, 0, 0, 0, 0, 0, 0) };
            unsafe { std::ptr::copy_nonoverlapping(block as usize as *const u8, fresh as usize as *mut u8, old.min(size) as usize) };
        }
        fresh
    }

    unsafe extern "system" fn free(_: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        0
    }

    unsafe extern "system" fn get_size(this: u64, block: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        if unsafe { did_alloc(this, block, 0, 0, 0, 0, 0, 0) } == 1 { unsafe { *((block - 8) as usize as *const u64) } } else { u64::MAX }
    }

    unsafe extern "system" fn did_alloc(this: u64, block: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        let object = unsafe { host_malloc(this) };
        u64::from((object.heap.start() + 8..object.next.get()).contains(&block))
    }

    unsafe extern "system" fn heap_minimize(_: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> u64 {
        0
    }
}

/// The program's side of a call: its stack and the 32-bit machine state the lift keeps, in host
/// memory below 4 GB that the interpreter's memory maps at its own addresses.
struct Guest {
    memory: Memory,
    objects: Objects,
    low: Low,
}

/// Where the machine state, the stack's top and the identifiers lie in the guest's memory.
const STATE: u64 = 0;
const IIDS: u64 = 0x100;
const STACK_TOP: u64 = 0x8000;
const RETURN: u32 = 0x0040_1234;
const ESP: u64 = 32;
const EAX: u64 = 0;

fn fault(stop: &Stop) -> String {
    format!("{stop:?}")
}

impl Guest {
    fn new() -> Result<Self, String> {
        let mut memory = Memory::new();
        let objects = Objects::new(&mut memory).map_err(|e| fault(&e))?;
        let low = Low::new(0x1_0000).ok_or("no room below 4 GB for the guest's stack")?;
        memory.add_region(Region::new(low.start(), low.size(), "guest"));
        for (n, name) in ["IUnknown", "IMalloc"].iter().enumerate() {
            let iid = com::interface(name).ok_or_else(|| format!("the host has no prototype of {name}"))?.iid;
            memory.write(low.start() + IIDS + 16 * n as u64, &iid).map_err(|e| fault(&e))?;
        }
        memory.write(low.start() + IIDS + 32, &[0x5A; 16]).map_err(|e| fault(&e))?;
        Ok(Self { memory, objects, low })
    }

    /// The guest address of the identifier of `IUnknown` (0), `IMalloc` (1) or one the host does
    /// not know (2).
    fn iid(&self, which: u64) -> u32 {
        (self.low.start() + IIDS + 16 * which) as u32
    }

    /// The program's call of the method in `slot` of the object `this` through its vtable, as
    /// x86 code makes it: the arguments pushed right to left, `this`, the return address, then
    /// an indirect call; the callee pops them and leaves its result in `eax`.
    fn call(&mut self, this: u32, slot: u32, args: &[u32]) -> Result<u32, String> {
        let state = self.low.start() + STATE;
        let mut esp = self.low.start() + STACK_TOP;
        let top = esp;
        for &value in args.iter().rev().chain([this, RETURN].iter()) {
            esp -= 4;
            self.memory.write_u32(esp, value).map_err(|e| fault(&e))?;
        }
        self.memory.write_u64(state + ESP, esp).map_err(|e| fault(&e))?;
        self.memory.write_u64(state + EAX, 0xDEAD_BEEF).map_err(|e| fault(&e))?;
        let vtable = self.memory.read_u32(u64::from(this)).map_err(|e| fault(&e))?;
        let target = self.memory.read_u32(u64::from(vtable) + 4 * u64::from(slot)).map_err(|e| fault(&e))?;
        if !self.objects.answers(u64::from(target)) {
            return Err(format!("slot {slot} of {this:#x} holds {target:#x}, which is no thunk"));
        }
        self.objects.call(&mut self.memory, state, u64::from(target)).map_err(|e| fault(&e))?;
        let after = self.memory.read_u64(state + ESP).map_err(|e| fault(&e))?;
        if after != top {
            return Err(format!("the call left esp at {after:#x}, not {top:#x}: the callee pops `this` and the arguments"));
        }
        Ok(self.memory.read_u64(state + EAX).map_err(|e| fault(&e))? as u32)
    }

    /// The guest object `QueryInterface` wrote at the out slot.
    fn out(&mut self) -> Result<u32, String> {
        self.memory.read_u32(self.low.start() + IIDS + 48).map_err(|e| fault(&e))
    }

    fn out_slot(&self) -> u32 {
        (self.low.start() + IIDS + 48) as u32
    }
}

/// A host `IMalloc` with the one reference the host hands out.
fn host_object() -> Result<Box<HostMalloc>, String> {
    let heap = Low::new(0x1_0000).ok_or("no room below 4 GB for the host's heap")?;
    let next = heap.start();
    Ok(Box::new(HostMalloc { vtable: &HOST_MALLOC, references: std::cell::Cell::new(1), next: std::cell::Cell::new(next), heap }))
}

fn expect(what: &str, actual: u64, expected: u64) -> Result<(), String> {
    if actual == expected { Ok(()) } else { Err(format!("{what}: {actual:#x}, not {expected:#x}")) }
}

/// `IUnknown` through the vtable: `QueryInterface` hands back the same guest object for the same
/// interface and another for `IMalloc`, refuses an identifier it does not know, and every
/// `AddRef` and `Release` reaches the host's count, the last retiring the guest objects.
fn exercise_iunknown() -> Result<(), String> {
    let host = host_object()?;
    let address = (&raw const *host).expose_provenance() as u64;
    let mut guest = Guest::new()?;
    let unknown = com::interface("IUnknown").ok_or("the host has no prototype of IUnknown")?;
    // The host object implements IUnknown and gives its reference to the guest object.
    let object = unsafe { guest.objects.wrap(&mut guest.memory, address, unknown) }.map_err(|e| fault(&e))?;
    let count = || u64::from(host.references.get());

    let (iid_unknown, iid_malloc, iid_other, out) = (guest.iid(0), guest.iid(1), guest.iid(2), guest.out_slot());
    expect("QueryInterface(IID_IUnknown)", u64::from(guest.call(object, 0, &[iid_unknown, out])?), S_OK)?;
    expect("the object QueryInterface(IID_IUnknown) hands back", u64::from(guest.out()?), u64::from(object))?;
    expect("the guest references after QueryInterface(IID_IUnknown)", u64::from(guest.objects.references(object).unwrap_or(0)), 2)?;
    expect("QueryInterface(IID_IMalloc)", u64::from(guest.call(object, 0, &[iid_malloc, out])?), S_OK)?;
    let malloc = guest.out()?;
    if malloc == 0 || malloc == object {
        return Err(format!("QueryInterface(IID_IMalloc) handed back {malloc:#x}, not a guest object of its own"));
    }
    expect("the host object behind the IMalloc", guest.objects.host(malloc).unwrap_or(0), address)?;
    expect("QueryInterface of an unknown identifier", u64::from(guest.call(object, 0, &[iid_other, out])?), E_NOINTERFACE)?;
    expect("the object an unknown identifier hands back", u64::from(guest.out()?), 0)?;
    expect("the host's references", count(), 3)?;

    expect("AddRef", u64::from(guest.call(object, 1, &[])?), 4)?;
    for (expected, through) in [(3, malloc), (2, object), (1, object), (0, object)] {
        expect("Release", u64::from(guest.call(through, 2, &[])?), expected)?;
    }
    expect("the host's references after the last Release", count(), 0)?;
    if guest.objects.references(object).is_some() || guest.objects.references(malloc).is_some() {
        return Err("a guest object outlived its last Release".to_owned());
    }
    guest.memory.remove_region(guest.objects.start());
    Ok(())
}

/// `IMalloc` through the vtable: `Alloc`'s block lies below 4 GB and the program writes it, the
/// host knows its size, and `Release` drops the reference the host handed out.
fn exercise_imalloc() -> Result<(), String> {
    let host = host_object()?;
    let address = (&raw const *host).expose_provenance() as u64;
    let mut guest = Guest::new()?;
    let interface = com::interface("IMalloc").ok_or("the host has no prototype of IMalloc")?;
    // The host object implements IMalloc and gives its reference to the guest object.
    let object = unsafe { guest.objects.wrap(&mut guest.memory, address, interface) }.map_err(|e| fault(&e))?;
    if object == 0 {
        return Err("the guest object is at 0".to_owned());
    }

    let block = guest.call(object, 3, &[100])?;
    if block == 0 {
        return Err("IMalloc::Alloc(100) through the vtable returned 0".to_owned());
    }
    // The host's own block, a 64-bit address that fits the program's 4 bytes.
    if !(host.heap.start()..host.heap.start() + host.heap.size()).contains(&u64::from(block)) {
        return Err(format!("IMalloc::Alloc returned {block:#x}, not a block of the host's heap"));
    }
    guest.memory.add_region(Region::new(host.heap.start(), host.heap.size(), "host heap"));
    guest.memory.write(u64::from(block), &[0xA5; 100]).map_err(|e| fault(&e))?;
    expect("IMalloc::GetSize", u64::from(guest.call(object, 6, &[block])?), 100)?;
    expect("IMalloc::DidAlloc", u64::from(guest.call(object, 7, &[block])?), 1)?;
    let foreign = guest.iid(0);
    expect("IMalloc::GetSize of a block it did not allocate", u64::from(guest.call(object, 6, &[foreign])?), u64::from(u32::MAX))?;
    guest.call(object, 5, &[block])?;

    expect("Release", u64::from(guest.call(object, 2, &[])?), 0)?;
    expect("the host's references after Release", u64::from(host.references.get()), 0)?;
    if guest.objects.references(object).is_some() {
        return Err("the guest object outlived its last Release".to_owned());
    }
    if guest.call(object, 3, &[16]).is_ok() {
        return Err("a call through the released object reached the host".to_owned());
    }
    guest.memory.remove_region(host.heap.start());
    guest.memory.remove_region(guest.objects.start());
    Ok(())
}

/// The calls of the row's interface through its guest object's vtable.
fn exercise(name: &str) -> Result<(), String> {
    match name {
        "IUnknown" => exercise_iunknown(),
        "IMalloc" => exercise_imalloc(),
        _ if com::interface(name).is_none() => Err(format!("the host has no prototype of {name}")),
        _ => Err(format!("{name}: the conformance test makes no calls of it")),
    }
}

#[test]
fn the_host_interfaces_call_through_a_32_bit_vtable_as_their_status_says() {
    let path = manifest::workspace().join("testdata/conformance/win32_com.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rows = parse_interfaces(&text).unwrap_or_else(|e| panic!("{e}"));
    let mut failures = Vec::new();
    let mut passing = Vec::new();
    for row in &rows {
        match (exercise(&row.name), row.pending) {
            (Ok(()), true) => failures.push(format!("win32_com.toml:{}: {} calls: mark it pass", row.line, row.name)),
            (Err(e), false) => failures.push(format!("win32_com.toml:{}: {} is pass but fails: {e}", row.line, row.name)),
            (Ok(()), false) => passing.push(row.name.as_str()),
            (Err(_), true) => {}
        }
    }
    assert!(failures.is_empty(), "{} rows are not as their status says:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(passing.len(), INTERFACES_PASSING, "win32_com.toml has {passing:?} pass, not {INTERFACES_PASSING} rows");
    assert_eq!(passing[..2], ["IUnknown", "IMalloc"], "the first two rows are IUnknown and IMalloc");
}

#[test]
fn the_interfaces_parser_refuses_a_row_it_cannot_read() {
    assert_eq!(parse_interfaces("[[interface]]\nname = \"IMalloc\"\nstatus = \"pass\"\n").map(|rows| rows.len()), Ok(1));
    assert!(parse_interfaces("[[interface]]\nname = \"IMalloc\"\n").is_err(), "a row without a status");
    assert!(parse_interfaces("[[interface]]\nname = \"IMalloc\"\nstatus = \"passing\"\n").is_err(), "an unknown status");
    assert!(parse_interfaces("status = \"pass\"\n").is_err(), "a key outside a row");
}

// endregion
