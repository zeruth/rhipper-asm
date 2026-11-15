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
//! `WNDCLASSEXA` can be widened for the 64-bit host field by field.

mod layouts;
mod win32;
#[rustfmt::skip]
mod win32_layouts;
mod win32_names;

pub use layouts::{Field, Kind, Place, Record, layout, pointees, records};

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
}
