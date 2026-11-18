//! The import prototype tables that come from the SDK headers, and their lookup: a crate of its
//! own, so that the tables compile when the SDK's tables change and not with every edit of the
//! lift, which reads them through `raise::prototypes`.
//!
//! The Windows API's prototypes are in `win32`, generated from the headers (`tests/win32_protos.rs`
//! regenerates it): [`win32_kinds`] reads one, as the kind letters the table keeps. Beside the
//! kinds, the parameters and results that take one of the SDK's named constants: the enumeration
//! each belongs to is bound here by hand ([`named`]), and the names and values of the enumerations
//! come from `win32_names`, generated from the headers (`tests/win32_names.rs`), so the C reads
//! `CreateFileA(name, GENERIC_READ | GENERIC_WRITE, ...)` and `GetLastError() ==
//! ERROR_FILE_NOT_FOUND` ([`constant_name`]).
//!
//! The structures the prototypes point to are laid out at 32 and at 64 bits in `win32_layouts`,
//! generated from the headers (`tests/win32_layouts.rs`): [`layout`] reads one by its tag or
//! typedef name, [`pointees`] the ones a function's parameters point to, so a 32-bit program's
//! `WNDCLASSEXA` can be widened for the 64-bit host field by field. Beside them, bound here by
//! hand: the fields a converter crosses a union by ([`converted_fields`]), the code pointers the
//! host calls back ([`callback`]), and the fields and parameters that hold a structure's size
//! ([`holds_size`], [`size_parameter`]), which the layouts alone do not say.
//!
//! The kinds a signature's parameters and result take are [`kind::Kind`], beside the lookup that
//! reads them; the lift gives each its IR type.

pub mod kind;
mod layouts;
mod openal;
mod opengl;
#[rustfmt::skip]
mod sdl2_layouts;
mod win32;
#[rustfmt::skip]
mod win32_layouts;
mod win32_names;

pub use layouts::{
    Field, Kind, POINTER_CELL, Place, Record, frees_object, is_kept, is_object, layout, object_field, pointees, records, result,
    sdl2_records,
};

/// The Windows API functions whose parameters or result take a named constant: the enumeration
/// of the result (empty for none), then of each parameter in order (empty for none; a parameter
/// past the end takes none). An enumeration's names are in the generated table.
const NAMED: &[(&str, &str, &[&str])] = &[
    ("CreateFileA", "HANDLE_VALUE", &["", "ACCESS_MASK", "FILE_SHARE_MODE", "", "CREATION_DISPOSITION", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("CreateFileW", "HANDLE_VALUE", &["", "ACCESS_MASK", "FILE_SHARE_MODE", "", "CREATION_DISPOSITION", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("FindFirstFileA", "HANDLE_VALUE", &[]),
    ("FindFirstFileW", "HANDLE_VALUE", &[]),
    ("GetFileAttributesA", "FILE_FLAGS_AND_ATTRIBUTES", &[]),
    ("GetFileAttributesW", "FILE_FLAGS_AND_ATTRIBUTES", &[]),
    ("SetFileAttributesA", "", &["", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("SetFileAttributesW", "", &["", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("SetFilePointer", "SET_FILE_POINTER_RESULT", &["", "", "", "FILE_MOVE_METHOD"]),
    ("SetFilePointerEx", "", &["", "", "", "FILE_MOVE_METHOD"]),
    ("GetStdHandle", "HANDLE_VALUE", &["STD_HANDLE"]),
    ("SetStdHandle", "", &["STD_HANDLE"]),
    ("GetLastError", "WIN32_ERROR", &[]),
    ("SetLastError", "", &["WIN32_ERROR"]),
    ("VirtualAlloc", "", &["", "", "MEM_ALLOCATION", "PAGE_PROTECTION"]),
    ("VirtualAllocEx", "", &["", "", "", "MEM_ALLOCATION", "PAGE_PROTECTION"]),
    ("VirtualFree", "", &["", "", "MEM_ALLOCATION"]),
    ("VirtualFreeEx", "", &["", "", "", "MEM_ALLOCATION"]),
    ("VirtualProtect", "", &["", "", "PAGE_PROTECTION"]),
    ("VirtualProtectEx", "", &["", "", "", "PAGE_PROTECTION"]),
    ("HeapCreate", "", &["HEAP_FLAGS"]),
    ("HeapAlloc", "", &["", "HEAP_FLAGS"]),
    ("HeapReAlloc", "", &["", "HEAP_FLAGS"]),
    ("HeapFree", "", &["", "HEAP_FLAGS"]),
    ("HeapSize", "", &["", "HEAP_FLAGS"]),
    ("WaitForSingleObject", "WAIT_RESULT", &["", "TIMEOUT"]),
    ("WaitForSingleObjectEx", "WAIT_RESULT", &["", "TIMEOUT"]),
    ("WaitForMultipleObjects", "WAIT_RESULT", &["", "", "", "TIMEOUT"]),
    ("WaitForMultipleObjectsEx", "WAIT_RESULT", &["", "", "", "TIMEOUT"]),
    ("Sleep", "", &["TIMEOUT"]),
    ("SleepEx", "", &["TIMEOUT"]),
    ("MultiByteToWideChar", "", &["CODE_PAGE"]),
    ("WideCharToMultiByte", "", &["CODE_PAGE"]),
    ("ShowWindow", "", &["", "SHOW_WINDOW_CMD"]),
    ("SendMessageA", "", &["", "WINDOW_MESSAGE"]),
    ("SendMessageW", "", &["", "WINDOW_MESSAGE"]),
    ("PostMessageA", "", &["", "WINDOW_MESSAGE"]),
    ("PostMessageW", "", &["", "WINDOW_MESSAGE"]),
    ("DefWindowProcA", "", &["", "WINDOW_MESSAGE"]),
    ("DefWindowProcW", "", &["", "WINDOW_MESSAGE"]),
    ("RegOpenKeyExA", "WIN32_ERROR", &["HKEY"]),
    ("RegOpenKeyExW", "WIN32_ERROR", &["HKEY"]),
    ("RegCreateKeyExA", "WIN32_ERROR", &["HKEY"]),
    ("RegCreateKeyExW", "WIN32_ERROR", &["HKEY"]),
    ("RegQueryValueExA", "WIN32_ERROR", &["HKEY"]),
    ("RegQueryValueExW", "WIN32_ERROR", &["HKEY"]),
    ("RegSetValueExA", "WIN32_ERROR", &["HKEY"]),
    ("RegSetValueExW", "WIN32_ERROR", &["HKEY"]),
    ("RegCloseKey", "WIN32_ERROR", &["HKEY"]),
];

/// The enumerations an import's result and parameters take, by name: `None` for a function the
/// binding does not know; inside, `None` for a result or parameter that takes a plain number.
pub fn named(name: &str) -> Option<(Option<&'static str>, Vec<Option<&'static str>>)> {
    let &(_, result, params) = NAMED.iter().find(|(n, ..)| *n == name)?;
    let some = |e: &'static str| (!e.is_empty()).then_some(e);
    Some((some(result), params.iter().map(|e| some(e)).collect()))
}

/// The SDK's name of `value`, a constant of `bits` bits in `enumeration`: one member whose value
/// it is, or, in a flags enumeration, the members whose bits make it up exactly, in the table's
/// order, each with its own value; `None` when the names do not cover it. A member's value is
/// compared at the constant's width, and a 64-bit constant also as the zero-extended `DWORD` the
/// API takes (`STD_OUTPUT_HANDLE`, `(DWORD)-11`, passed in a 64-bit argument).
pub fn constant_name(enumeration: &str, value: u64, bits: u32) -> Option<Vec<(&'static str, u64)>> {
    let i = win32_names::NAMES.binary_search_by_key(&enumeration, |(n, ..)| n).ok()?;
    let (_, flags, members) = win32_names::NAMES[i];
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let value = value & mask;
    let same = |member: u64| (member & mask) == value || (bits >= 64 && (member & 0xffff_ffff) == value);
    if let Some(&(name, member)) = members.iter().find(|(_, m)| same(*m)) {
        return Some(vec![(name, member)]);
    }
    if !flags || value == 0 {
        return None;
    }
    let mut rest = value;
    let mut parts = Vec::new();
    for &(name, member) in members {
        let bit = member & mask;
        if bit != 0 && rest & bit == bit {
            parts.push((name, member));
            rest &= !bit;
        }
    }
    (rest == 0).then_some(parts)
}

/// The prototype of a Windows API function from the generated table, as its kind letters: the
/// first is the result's (`v` for a `void` function), the rest are the parameters' in order (`i`
/// an integer or pointer of the address width, `w` a 64-bit integer or an 8-byte structure by
/// value, `d` a double, `f` a float).
pub fn win32_kinds(name: &str) -> Option<&'static str> {
    let i = win32::TABLE.binary_search_by_key(&name, |(n, _)| n).ok()?;
    Some(win32::TABLE[i].1)
}

/// The kinds of an SDL2, SDL2_mixer or SDL2_net function, as [`win32_kinds`] writes them: the
/// `cdecl` functions of SDL's headers (`SDLCALL`), generated with their layouts into
/// `sdl2_layouts`. A variadic function, and one that takes or returns a structure by value that
/// is not 8 bytes, is not in the table.
pub fn sdl2_kinds(name: &str) -> Option<&'static str> {
    let i = sdl2_layouts::KINDS.binary_search_by_key(&name, |(n, _)| n).ok()?;
    Some(sdl2_layouts::KINDS[i].1)
}

/// The kinds of an OpenAL 1.1 function, as [`win32_kinds`] writes them: the `cdecl` functions of
/// `al.h` and `alc.h`, from the hand-written table.
pub fn openal_kinds(name: &str) -> Option<&'static str> {
    openal::KINDS.iter().find(|(n, _)| *n == name).map(|(_, kinds)| *kinds)
}

/// The kinds of an OpenGL function or a WGL extension's, as [`win32_kinds`] writes them: `stdcall`
/// on 32-bit Windows, from the table written from `GL.h` and `glext.h`, which is in the order of
/// the names.
pub fn opengl_kinds(name: &str) -> Option<&'static str> {
    opengl::KINDS.binary_search_by(|(n, _)| (*n).cmp(name)).ok().map(|at| opengl::KINDS[at].1)
}

/// The fields of `record` a converter crosses the widths by, in order: a field the 32-bit layout
/// does not have (`MEMORY_BASIC_INFORMATION`'s `PartitionId`) is left out, the host's being zero
/// and the program having none to read back; and the members of a union are crossed by those that
/// keep one layout at both widths and tile it (`OVERLAPPED`'s `Offset` and `OffsetHigh`), the
/// others being read through them (its `Pointer`, which the API reserves to the system).
///
/// # Errors
/// A union whose members that keep their layout do not tile it at both widths: its layouts do
/// not say which member it holds.
pub fn converted_fields(record: &Record) -> Result<Vec<&'static Field>, String> {
    let fields: Vec<&'static Field> = record.fields.iter().filter(|f| f.at32.size > 0).collect();
    if !record.overlapping {
        return Ok(fields);
    }
    let meet = |a: Place, b: Place| a.offset < b.offset + b.size && b.offset < a.offset + a.size;
    // Bit fields share their unit by design, and copy as they are at both widths.
    let share = |a: &Field, b: &Field| a.kind != Kind::Bits && b.kind != Kind::Bits && (meet(a.at32, b.at32) || meet(a.at64, b.at64));
    let mut group: Vec<usize> = (0..fields.len()).collect();
    for i in 0..fields.len() {
        for j in i + 1..fields.len() {
            if share(fields[i], fields[j]) && group[i] != group[j] {
                let (from, to) = (group[j], group[i]);
                group.iter_mut().filter(|g| **g == from).for_each(|g| *g = to);
            }
        }
    }
    let mut out = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let members: Vec<&'static Field> = (0..fields.len()).filter(|&j| group[j] == group[i]).map(|j| fields[j]).collect();
        if members.len() == 1 {
            out.push(*field);
        } else if std::ptr::eq(members[0], *field) {
            out.extend(carriers(record, &members)?);
        }
    }
    Ok(out)
}

/// The members of a union that keep one layout at both widths and tile its bytes at both, by
/// offset.
fn carriers(record: &Record, members: &[&'static Field]) -> Result<Vec<&'static Field>, String> {
    let span = |place: fn(&Field) -> Place| {
        let start = members.iter().map(|f| place(f).offset).min().unwrap_or(0);
        let end = members.iter().map(|f| place(f).offset + place(f).size).max().unwrap_or(0);
        (start, end)
    };
    let ((start32, end32), (start64, end64)) = (span(|f| f.at32), span(|f| f.at64));
    let unconverted = || {
        format!(
            "{}: its fields share storage at {start32}, and no members of one layout at both widths say which one it holds",
            record.name
        )
    };
    if end32 - start32 != end64 - start64 {
        return Err(unconverted());
    }
    let keeps = |f: &Field| {
        f.at32.size == f.at64.size
            && f.at32.offset - start32 == f.at64.offset - start64
            && matches!(f.kind, Kind::Signed | Kind::Unsigned | Kind::Float)
    };
    let mut kept: Vec<&'static Field> = members.iter().copied().filter(|f| keeps(f)).collect();
    kept.sort_by_key(|f| (f.at32.offset, std::cmp::Reverse(f.at32.size)));
    let mut out = Vec::new();
    let mut at = start32;
    while at < end32 {
        let next = kept.iter().find(|f| f.at32.offset == at).ok_or_else(unconverted)?;
        out.push(*next);
        at += next.at32.size;
    }
    Ok(out)
}

/// A code pointer the host calls back: the program's procedure, called through an adapter that
/// narrows each argument to the program's width and widens the result to the host's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callback {
    /// The SDK's name of the pointer's type (`WNDPROC`).
    pub name: &'static str,
    /// The result's kind and its size at 32 bits; the host reads it in 8 bytes.
    pub result: (Kind, u32),
    /// Each parameter's kind, its size at 32 bits and at 64, in order.
    pub params: &'static [(Kind, u32, u32)],
}

/// `LRESULT (CALLBACK *)(HWND, UINT, WPARAM, LPARAM)`.
const WNDPROC: Callback = Callback {
    name: "WNDPROC",
    result: (Kind::Signed, 4),
    params: &[(Kind::Pointer, 4, 8), (Kind::Unsigned, 4, 4), (Kind::Unsigned, 4, 8), (Kind::Signed, 4, 8)],
};

/// The fields of the layout table that hold a code pointer the host calls back, by the record's
/// name in the table.
const CALLBACKS: &[(&str, &str, Callback)] = &[("tagWNDCLASSA", "lpfnWndProc", WNDPROC), ("tagWNDCLASSEXA", "lpfnWndProc", WNDPROC)];

/// The callback the field `field` of the record `record` (its name in the layout table) holds;
/// `None` for a field the host does not call.
pub fn callback(record: &str, field: &str) -> Option<&'static Callback> {
    CALLBACKS.iter().find(|(r, f, _)| *r == record && *f == field).map(|(.., c)| c)
}

/// SDL_mixer's music hook, `void (*)(void *udata, Uint8 *stream, int len)`, which the mixer
/// calls on its audio thread for each buffer: a program's own music synthesizer fills it.
const MIX_MUSIC_HOOK: Callback = Callback {
    name: "Mix_MusicHook",
    result: (Kind::Signed, 4),
    params: &[(Kind::Pointer, 4, 8), (Kind::Pointer, 4, 8), (Kind::Signed, 4, 4)],
};

/// `Mix_EffectFunc_t`, `void (*)(int chan, void *stream, int len, void *udata)`.
const MIX_EFFECT: Callback = Callback {
    name: "Mix_EffectFunc_t",
    result: (Kind::Signed, 4),
    params: &[(Kind::Signed, 4, 4), (Kind::Pointer, 4, 8), (Kind::Signed, 4, 4), (Kind::Pointer, 4, 8)],
};

/// `Mix_EffectDone_t`, `void (*)(int chan, void *udata)`.
const MIX_EFFECT_DONE: Callback =
    Callback { name: "Mix_EffectDone_t", result: (Kind::Signed, 4), params: &[(Kind::Signed, 4, 4), (Kind::Pointer, 4, 8)] };

/// `SDL_ThreadFunction`, `int (*)(void *data)`, which a thread SDL starts runs.
const SDL_THREAD_FUNCTION: Callback = Callback { name: "SDL_ThreadFunction", result: (Kind::Signed, 4), params: &[(Kind::Pointer, 4, 8)] };

/// The parameters of the libraries' functions that hand the host a code pointer it calls back
/// after the call, by the function and the parameter's index.
const PARAMETER_CALLBACKS: &[(&str, usize, Callback)] = &[
    ("Mix_HookMusic", 0, MIX_MUSIC_HOOK),
    ("Mix_RegisterEffect", 1, MIX_EFFECT),
    ("Mix_RegisterEffect", 2, MIX_EFFECT_DONE),
    ("SDL_CreateThread", 0, SDL_THREAD_FUNCTION),
];

/// The parameters of `function` that hand the host a code pointer it calls back: the index and
/// the callback, in order; empty for a function with none.
pub fn parameter_callbacks(function: &str) -> Vec<(usize, &'static Callback)> {
    PARAMETER_CALLBACKS.iter().filter(|(f, ..)| *f == function).map(|(_, p, c)| (*p, c)).collect()
}

/// The fields that hold their own structure's size, which the host checks against its layout's.
const SIZE_FIELDS: &[(&str, &str)] = &[("tagWNDCLASSEXA", "cbSize")];

/// Whether the field `field` of the record `record` holds the structure's own size (`cbSize`):
/// the program's size there stands for the host's.
pub fn holds_size(record: &str, field: &str) -> bool {
    SIZE_FIELDS.iter().any(|(r, f)| *r == record && *f == field)
}

/// The functions with a parameter that holds the size of the structure another points to: that
/// parameter, the structure's, and whether the result is the size written.
const SIZE_PARAMETERS: &[(&str, usize, usize, bool)] = &[("VirtualQuery", 2, 1, true), ("VirtualQueryEx", 3, 2, true)];

/// The parameter of `function` that holds the size of the buffer another points to, that other
/// parameter, and whether the result is the size written; `None` for a function without one.
pub fn size_parameter(function: &str) -> Option<(usize, usize, bool)> {
    SIZE_PARAMETERS.iter().find(|(name, ..)| *name == function).map(|&(_, size, of, result)| (size, of, result))
}

/// Whether the import `name` is known to return with nothing a handler of the program sees: it
/// runs no code of the program (no procedure, hook, callback, comparator, APC, handler, new
/// handler, invalid parameter handler or thread start of it) and raises nothing, a hardware fault
/// included. An exception left in flight would pass the import, so every import not listed here
/// may return with one. The rule leaves out every import that reads or writes through a pointer
/// of the program (a bad one faults into a structured exception: `memcpy`, `strlen`,
/// `EnterCriticalSection`, `GetSystemTime`), every one that takes a handle (an invalid one raises
/// under a debugger: `CloseHandle`, `WaitForSingleObject`), every allocator and free (a corrupt
/// heap or `HEAP_GENERATE_EXCEPTIONS` raises: `HeapAlloc`, `HeapFree`, `VirtualAlloc`), the
/// character classes (an argument out of range reads past their table) and the loader (a DLL's
/// entry). What is listed takes only integers and reads only the process's own state. When unsure,
/// an import stays out.
pub fn calls_no_code(name: &str) -> bool {
    matches!(
        name,
        // kernel32: errors, the process, the thread, the clocks and the code pages
        "GetLastError"
            | "SetLastError"
            | "GetCurrentProcess"
            | "GetCurrentProcessId"
            | "GetCurrentThread"
            | "GetCurrentThreadId"
            | "GetTickCount"
            | "GetTickCount64"
            | "GetVersion"
            | "IsDebuggerPresent"
            | "IsProcessorFeaturePresent"
            | "GetCommandLineA"
            | "GetCommandLineW"
            | "GetProcessHeap"
            | "TlsAlloc"
            | "TlsFree"
            | "TlsGetValue"
            | "FlsGetValue"
            | "Sleep"
            | "SwitchToThread"
            | "GetStdHandle"
            | "GetConsoleCP"
            | "GetConsoleOutputCP"
            | "GetACP"
            | "GetOEMCP"
            | "IsValidCodePage"
            | "GetUserDefaultLCID"
            // the C runtime: the integer functions and the pointers to its globals
            | "abs"
            | "labs"
            | "llabs"
            | "__p___argc"
            | "__p___argv"
            | "__p__commode"
            | "__p__fmode"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand binding reads by position, and its enumerations are in the generated names.
    #[test]
    fn the_binding_names_the_constants() {
        let (result, params) = named("CreateFileA").expect("CreateFileA is bound");
        assert_eq!(result, Some("HANDLE_VALUE"));
        assert_eq!(params[..3], [None, Some("ACCESS_MASK"), Some("FILE_SHARE_MODE")]);
        assert!(named("printf").is_none());
        assert_eq!(
            constant_name("ACCESS_MASK", 0xc000_0000, 32),
            Some(vec![("GENERIC_READ", 0x8000_0000), ("GENERIC_WRITE", 0x4000_0000)])
        );
        assert_eq!(constant_name("WIN32_ERROR", 2, 32).map(|p| p[0].0), Some("ERROR_FILE_NOT_FOUND"));
    }

    /// An import is taken to call no code of the program only when it is listed.
    #[test]
    fn only_listed_imports_call_no_code() {
        assert!(calls_no_code("GetTickCount"));
        assert!(calls_no_code("GetLastError"));
        for name in ["SleepEx", "SetWindowTextA", "CreateThread", "malloc", "qsort", "MessageBoxA", "LoadLibraryA", "strcpy_s"] {
            assert!(!calls_no_code(name), "{name} may run code of the program");
        }
        // Each may raise: a corrupt heap, an invalid handle under a debugger, a bad pointer.
        for name in [
            "HeapAlloc",
            "HeapFree",
            "HeapDestroy",
            "LocalAlloc",
            "GlobalAlloc",
            "VirtualAlloc",
            "WaitForSingleObject",
            "WaitForMultipleObjects",
            "CloseHandle",
            "DuplicateHandle",
            "memcpy",
            "strcpy",
            "strcat",
            "strncpy",
            "wcscpy",
            "strlen",
            "EnterCriticalSection",
        ] {
            assert!(!calls_no_code(name), "{name} may raise");
        }
    }
}
