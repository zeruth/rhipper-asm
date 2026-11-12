//! The Windows API's named constants, generated from the SDK headers: `src/raise/prototypes/win32_names.rs`
//! holds, for each enumeration the prototype table binds a parameter or a result to (the access
//! mask of `CreateFile`, the Win32 error codes, the window messages, ...), the SDK's name of every
//! value. The ignored `regenerate_win32_names` writes it: clang-cl prints the macros the headers
//! define (`/E` with `-dM`, the SDK on the PATH's clang-cl is the one used), and each enumeration's
//! members are picked by name or prefix and their bodies evaluated. The other tests check the
//! evaluator and the table that is in.
//!
//!   `cargo test -p ra-asm-lift --test win32_names -- --ignored regenerate`
//!
//! An enumeration is either a set of flags, whose members are single bits and read or-ed together
//! (`GENERIC_READ | GENERIC_WRITE`), or a plain enumeration, whose members read one at a time. A
//! macro the evaluator cannot read (a string, a character, a call) is left out: the table says
//! nothing rather than something wrong.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const HEADERS: &[&str] = &["windows.h", "winioctl.h"];

/// One enumeration: its name in the prototype table, whether its members are flags, the members
/// named one by one (in the order a value with two names prefers), and the prefixes whose macros
/// join them (single bits only for flags, so a composite mask never hides its bits), up to `max`.
struct Spec {
    name: &'static str,
    flags: bool,
    names: &'static [&'static str],
    prefixes: &'static [&'static str],
    max: u64,
}

const SPECS: &[Spec] = &[
    Spec {
        name: "ACCESS_MASK",
        flags: true,
        names: &[
            "GENERIC_READ",
            "GENERIC_WRITE",
            "GENERIC_EXECUTE",
            "GENERIC_ALL",
            "DELETE",
            "READ_CONTROL",
            "WRITE_DAC",
            "WRITE_OWNER",
            "SYNCHRONIZE",
            "ACCESS_SYSTEM_SECURITY",
            "MAXIMUM_ALLOWED",
            "FILE_READ_DATA",
            "FILE_WRITE_DATA",
            "FILE_APPEND_DATA",
            "FILE_READ_EA",
            "FILE_WRITE_EA",
            "FILE_EXECUTE",
            "FILE_DELETE_CHILD",
            "FILE_READ_ATTRIBUTES",
            "FILE_WRITE_ATTRIBUTES",
        ],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec { name: "FILE_SHARE_MODE", flags: true, names: &[], prefixes: &["FILE_SHARE_"], max: u64::MAX },
    Spec {
        name: "CREATION_DISPOSITION",
        flags: false,
        names: &["CREATE_NEW", "CREATE_ALWAYS", "OPEN_EXISTING", "OPEN_ALWAYS", "TRUNCATE_EXISTING"],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec {
        name: "FILE_FLAGS_AND_ATTRIBUTES",
        flags: true,
        names: &["INVALID_FILE_ATTRIBUTES"],
        prefixes: &["FILE_ATTRIBUTE_", "FILE_FLAG_"],
        max: u64::MAX,
    },
    Spec { name: "FILE_MOVE_METHOD", flags: false, names: &["FILE_BEGIN", "FILE_CURRENT", "FILE_END"], prefixes: &[], max: u64::MAX },
    Spec { name: "SET_FILE_POINTER_RESULT", flags: false, names: &["INVALID_SET_FILE_POINTER"], prefixes: &[], max: u64::MAX },
    Spec { name: "HANDLE_VALUE", flags: false, names: &["INVALID_HANDLE_VALUE"], prefixes: &[], max: u64::MAX },
    Spec {
        name: "STD_HANDLE",
        flags: false,
        names: &["STD_INPUT_HANDLE", "STD_OUTPUT_HANDLE", "STD_ERROR_HANDLE"],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec { name: "WIN32_ERROR", flags: false, names: &[], prefixes: &["ERROR_"], max: 0xffff },
    Spec { name: "WINDOW_MESSAGE", flags: false, names: &[], prefixes: &["WM_"], max: 0xffff },
    Spec {
        name: "SHOW_WINDOW_CMD",
        flags: false,
        names: &[
            "SW_HIDE",
            "SW_SHOWNORMAL",
            "SW_SHOWMINIMIZED",
            "SW_SHOWMAXIMIZED",
            "SW_SHOWNOACTIVATE",
            "SW_SHOW",
            "SW_MINIMIZE",
            "SW_SHOWMINNOACTIVE",
            "SW_SHOWNA",
            "SW_RESTORE",
            "SW_SHOWDEFAULT",
            "SW_FORCEMINIMIZE",
        ],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec { name: "PAGE_PROTECTION", flags: true, names: &[], prefixes: &["PAGE_"], max: u64::MAX },
    Spec {
        name: "MEM_ALLOCATION",
        flags: true,
        names: &[
            "MEM_COMMIT",
            "MEM_RESERVE",
            "MEM_DECOMMIT",
            "MEM_RELEASE",
            "MEM_FREE",
            "MEM_RESET",
            "MEM_RESET_UNDO",
            "MEM_TOP_DOWN",
            "MEM_WRITE_WATCH",
            "MEM_PHYSICAL",
            "MEM_LARGE_PAGES",
        ],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec {
        name: "HEAP_FLAGS",
        flags: true,
        names: &[
            "HEAP_NO_SERIALIZE",
            "HEAP_GENERATE_EXCEPTIONS",
            "HEAP_ZERO_MEMORY",
            "HEAP_REALLOC_IN_PLACE_ONLY",
            "HEAP_CREATE_ENABLE_EXECUTE",
        ],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec {
        name: "WAIT_RESULT",
        flags: false,
        names: &["WAIT_OBJECT_0", "WAIT_ABANDONED", "WAIT_IO_COMPLETION", "WAIT_TIMEOUT", "WAIT_FAILED"],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec { name: "TIMEOUT", flags: false, names: &["INFINITE"], prefixes: &[], max: u64::MAX },
    Spec {
        name: "CODE_PAGE",
        flags: false,
        names: &["CP_ACP", "CP_OEMCP", "CP_MACCP", "CP_THREAD_ACP", "CP_SYMBOL", "CP_UTF7", "CP_UTF8"],
        prefixes: &[],
        max: u64::MAX,
    },
    Spec {
        name: "HKEY",
        flags: false,
        names: &["HKEY_CLASSES_ROOT", "HKEY_CURRENT_USER", "HKEY_LOCAL_MACHINE", "HKEY_USERS", "HKEY_CURRENT_CONFIG"],
        prefixes: &[],
        max: u64::MAX,
    },
];

#[test]
#[ignore = "a maintenance aid: regenerates src/raise/prototypes/win32_names.rs from the SDK headers with clang-cl"]
fn regenerate_win32_names() {
    let dir = std::env::temp_dir().join("ra-asm-win32-names");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("headers.c");
    let mut includes = String::new();
    for header in HEADERS {
        let _ = writeln!(includes, "#include <{header}>");
    }
    std::fs::write(&source, format!("#define _WIN32_WINNT 0x0A00\n#define NTDDI_VERSION 0x0A000000\n{includes}")).unwrap();
    let out = Command::new("clang-cl")
        .args(["--target=i686-pc-windows-msvc", "/nologo", "/E", "/W0", "/clang:-dM"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(out.status.success(), "clang-cl /E -dM:\n{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let macros = macros_of(&text);
    assert!(macros.len() > 5000, "{} macros read, which is too few for windows.h", macros.len());
    let mut file = String::new();
    file.push_str("//! The Windows API's named constants, from the SDK headers: for each enumeration the prototype\n");
    file.push_str("//! table binds a parameter or a result to, whether it is a set of flags and the name of every\n");
    file.push_str("//! value, sorted by enumeration. Generated by the ignored test `regenerate_win32_names` in\n");
    file.push_str("//! `tests/win32_names.rs`; do not edit.\n\n");
    file.push_str("pub(super) static NAMES: &[(&str, bool, &[(&str, u64)])] = &[\n");
    let mut specs: Vec<&Spec> = SPECS.iter().collect();
    specs.sort_by_key(|s| s.name);
    for spec in specs {
        let members = members_of(spec, &macros);
        assert!(!members.is_empty(), "{}: no member evaluated", spec.name);
        // The shape `cargo fmt` gives the table, so a regeneration leaves the file as it is.
        let _ = writeln!(file, "    (\n        {:?},\n        {},\n        &[", spec.name, spec.flags);
        for (name, value) in &members {
            let _ = writeln!(file, "            ({name:?}, {value:#x}),");
        }
        file.push_str("        ],\n    ),\n");
    }
    file.push_str("];\n");
    let path = root().join("crates/ra-asm-lift/src/raise/prototypes/win32_names.rs");
    std::fs::write(&path, file).unwrap();
    eprintln!("{} enumerations -> {}", SPECS.len(), path.display());
}

/// The object-like macros of a `-dM` listing: name to body.
fn macros_of(text: &str) -> HashMap<&str, &str> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define ") else { continue };
        let (name, body) = rest.split_once(' ').unwrap_or((rest, ""));
        if name.contains('(') {
            continue;
        }
        out.insert(name, body.trim());
    }
    out
}

/// The members of an enumeration: the named ones in their order, then the prefixed ones by name,
/// each evaluated; a flags enumeration keeps one bit per prefixed member.
fn members_of(spec: &Spec, macros: &HashMap<&str, &str>) -> Vec<(String, u64)> {
    let mut members: Vec<(String, u64)> = Vec::new();
    for &name in spec.names {
        if let Some(value) = evaluate(name, macros, 0) {
            members.push((name.to_owned(), value));
        }
    }
    let mut prefixed: Vec<&str> =
        macros.keys().copied().filter(|name| spec.prefixes.iter().any(|p| name.starts_with(p)) && !spec.names.contains(name)).collect();
    prefixed.sort_unstable();
    for name in prefixed {
        let Some(value) = evaluate(name, macros, 0) else { continue };
        if value > spec.max || (spec.flags && !value.is_power_of_two()) {
            continue;
        }
        members.push((name.to_owned(), value));
    }
    members
}

/// The value of a macro, by name, through the macros its body names.
fn evaluate(name: &str, macros: &HashMap<&str, &str>, depth: usize) -> Option<u64> {
    if depth > 16 {
        return None;
    }
    let body = macros.get(name)?;
    let tokens = tokens_of(body);
    let mut parser = Parser { tokens: &tokens, at: 0, macros, depth };
    let value = parser.expression()?;
    (parser.at == tokens.len()).then_some(value as u64)
}

/// The tokens of a macro body: identifiers, numbers, the two-character shifts, and single
/// punctuation characters.
fn tokens_of(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_alphanumeric() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else if (c == b'<' || c == b'>') && bytes.get(i + 1) == Some(&c) {
            i += 2;
        } else {
            i += 1;
        }
        tokens.push(&text[start..i]);
    }
    tokens
}

/// A recursive-descent reader of the integer expressions the headers write: casts to the API's
/// types are skipped, a name is the macro it stands for.
struct Parser<'a> {
    tokens: &'a [&'a str],
    at: usize,
    macros: &'a HashMap<&'a str, &'a str>,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.at).copied()
    }

    fn eat(&mut self, t: &str) -> bool {
        if self.peek() == Some(t) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expression(&mut self) -> Option<i64> {
        let mut v = self.xor()?;
        while self.eat("|") {
            v |= self.xor()?;
        }
        Some(v)
    }

    fn xor(&mut self) -> Option<i64> {
        let mut v = self.and()?;
        while self.eat("^") {
            v ^= self.and()?;
        }
        Some(v)
    }

    fn and(&mut self) -> Option<i64> {
        let mut v = self.shift()?;
        while self.eat("&") {
            v &= self.shift()?;
        }
        Some(v)
    }

    fn shift(&mut self) -> Option<i64> {
        let mut v = self.sum()?;
        loop {
            if self.eat("<<") {
                v = v.checked_shl(u32::try_from(self.sum()?).ok()?)?;
            } else if self.eat(">>") {
                v = v.checked_shr(u32::try_from(self.sum()?).ok()?)?;
            } else {
                return Some(v);
            }
        }
    }

    fn sum(&mut self) -> Option<i64> {
        let mut v = self.unary()?;
        loop {
            if self.eat("+") {
                v = v.checked_add(self.unary()?)?;
            } else if self.eat("-") {
                v = v.checked_sub(self.unary()?)?;
            } else {
                return Some(v);
            }
        }
    }

    fn unary(&mut self) -> Option<i64> {
        if self.eat("-") {
            return self.unary().map(|v| v.wrapping_neg());
        }
        if self.eat("~") {
            return self.unary().map(|v| !v);
        }
        if self.eat("+") {
            return self.unary();
        }
        // `(TYPE)` before an operand is a cast: a run of names that evaluates to nothing.
        if self.peek() == Some("(") {
            let mut end = self.at + 1;
            while self.tokens.get(end).is_some_and(|t| t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || *t == "*") {
                end += 1;
            }
            let names = &self.tokens[self.at + 1..end];
            if !names.is_empty()
                && self.tokens.get(end) == Some(&")")
                && end + 1 < self.tokens.len()
                && names.iter().all(|n| !n.starts_with(|c: char| c.is_ascii_digit()) && !self.macros.contains_key(n))
            {
                self.at = end + 1;
                return self.unary();
            }
        }
        self.primary()
    }

    fn primary(&mut self) -> Option<i64> {
        if self.eat("(") {
            let v = self.expression()?;
            return self.eat(")").then_some(v);
        }
        let t = self.peek()?;
        self.at += 1;
        if t.starts_with(|c: char| c.is_ascii_digit()) {
            return number(t);
        }
        if t.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return evaluate(t, self.macros, self.depth + 1).map(|v| v as i64);
        }
        None
    }
}

/// An integer literal without its suffix: `0x80000000L`, `2u`, `0xFFFFFFFFi64`.
fn number(t: &str) -> Option<i64> {
    let digits = t.trim_end_matches(|c: char| matches!(c, 'u' | 'U' | 'l' | 'L')).trim_end_matches("i64").trim_end_matches("i32");
    let v = if let Some(hex) = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()?
    } else if digits.len() > 1 && digits.starts_with('0') {
        u64::from_str_radix(&digits[1..], 8).ok()?
    } else {
        digits.parse().ok()?
    };
    Some(v as i64)
}

#[test]
fn macro_bodies_evaluate() {
    let text = "\
#define GENERIC_READ (0x80000000L)
#define GENERIC_WRITE (0x40000000L)
#define INVALID_HANDLE_VALUE ((HANDLE)(LONG_PTR)-1)
#define STD_OUTPUT_HANDLE ((DWORD)-11)
#define INVALID_FILE_ATTRIBUTES ((DWORD)-1)
#define HKEY_LOCAL_MACHINE (( HKEY ) (ULONG_PTR)((LONG)0x80000002) )
#define FILE_GENERIC_READ (STANDARD_RIGHTS_READ | FILE_READ_DATA | SYNCHRONIZE)
#define STANDARD_RIGHTS_READ (READ_CONTROL)
#define READ_CONTROL (0x00020000L)
#define FILE_READ_DATA ( 0x0001 )
#define SYNCHRONIZE (0x00100000L)
#define WM_USER 0x0400
#define WM_APP (WM_USER+0x3C00)
#define MB_OK 0x00000000L
#define SOME_STRING \"text\"
#define SOME_CHAR 'A'
#define SOME_CALL MAKELONG(1, 2)
#define TRUNCATE_EXISTING 5
#define PAGE_READWRITE 0x04
#define FILE_SHARE_READ 0x00000001
";
    let macros = macros_of(text);
    assert_eq!(evaluate("GENERIC_READ", &macros, 0), Some(0x8000_0000));
    assert_eq!(evaluate("INVALID_HANDLE_VALUE", &macros, 0), Some(u64::MAX));
    assert_eq!(evaluate("STD_OUTPUT_HANDLE", &macros, 0), Some((-11i64) as u64));
    assert_eq!(evaluate("INVALID_FILE_ATTRIBUTES", &macros, 0), Some(u64::MAX));
    assert_eq!(evaluate("HKEY_LOCAL_MACHINE", &macros, 0), Some(0x8000_0002));
    assert_eq!(evaluate("FILE_GENERIC_READ", &macros, 0), Some(0x0012_0001));
    assert_eq!(evaluate("WM_APP", &macros, 0), Some(0x4000));
    assert_eq!(evaluate("MB_OK", &macros, 0), Some(0));
    for absent in ["SOME_STRING", "SOME_CHAR", "SOME_CALL", "UNDEFINED"] {
        assert_eq!(evaluate(absent, &macros, 0), None, "{absent} evaluated");
    }
    let spec = Spec { name: "ACCESS_MASK", flags: true, names: &["GENERIC_WRITE", "GENERIC_READ"], prefixes: &["FILE_"], max: u64::MAX };
    let members = members_of(&spec, &macros);
    // The named ones first in their order, then the single-bit prefixed ones by name.
    assert_eq!(
        members,
        vec![
            ("GENERIC_WRITE".to_owned(), 0x4000_0000),
            ("GENERIC_READ".to_owned(), 0x8000_0000),
            ("FILE_READ_DATA".to_owned(), 1),
            ("FILE_SHARE_READ".to_owned(), 1)
        ]
    );
}

/// The table that is in: the names the fixtures pass, read back through the prototype table.
#[test]
fn the_generated_table_names_the_common_constants() {
    use ra_asm_lift::raise::prototypes::constant_name;
    let parts = |e: &str, v: u64, bits: u32| constant_name(e, v, bits).map(|p| p.into_iter().map(|(n, _)| n).collect::<Vec<_>>());
    assert_eq!(parts("ACCESS_MASK", 0xc000_0000, 32), Some(vec!["GENERIC_READ", "GENERIC_WRITE"]));
    assert_eq!(parts("ACCESS_MASK", 0xc000_0000, 64), Some(vec!["GENERIC_READ", "GENERIC_WRITE"]));
    assert_eq!(parts("FILE_SHARE_MODE", 1, 64), Some(vec!["FILE_SHARE_READ"]));
    assert_eq!(parts("FILE_SHARE_MODE", 0, 64), None);
    assert_eq!(parts("CREATION_DISPOSITION", 2, 32), Some(vec!["CREATE_ALWAYS"]));
    assert_eq!(parts("CREATION_DISPOSITION", 3, 32), Some(vec!["OPEN_EXISTING"]));
    assert_eq!(parts("CREATION_DISPOSITION", 6, 32), None, "a plain enumeration never reads as a sum");
    assert_eq!(parts("FILE_FLAGS_AND_ATTRIBUTES", 0x80, 32), Some(vec!["FILE_ATTRIBUTE_NORMAL"]));
    assert_eq!(parts("FILE_FLAGS_AND_ATTRIBUTES", 0xffff_ffff, 32), Some(vec!["INVALID_FILE_ATTRIBUTES"]));
    assert_eq!(parts("FILE_FLAGS_AND_ATTRIBUTES", 0x10, 8), Some(vec!["FILE_ATTRIBUTE_DIRECTORY"]));
    assert_eq!(parts("HANDLE_VALUE", u64::MAX, 64), Some(vec!["INVALID_HANDLE_VALUE"]));
    assert_eq!(parts("HANDLE_VALUE", 0xffff_ffff, 32), Some(vec!["INVALID_HANDLE_VALUE"]));
    assert_eq!(parts("STD_HANDLE", 0xffff_fff5, 64), Some(vec!["STD_OUTPUT_HANDLE"]));
    assert_eq!(parts("WIN32_ERROR", 2, 32), Some(vec!["ERROR_FILE_NOT_FOUND"]));
    assert_eq!(parts("WIN32_ERROR", 3, 32), Some(vec!["ERROR_PATH_NOT_FOUND"]));
    assert_eq!(parts("WINDOW_MESSAGE", 0x0f, 32), Some(vec!["WM_PAINT"]));
    assert_eq!(parts("MEM_ALLOCATION", 0x3000, 32), Some(vec!["MEM_COMMIT", "MEM_RESERVE"]), "the flags read in the table's order");
    assert_eq!(parts("PAGE_PROTECTION", 0x40, 32), Some(vec!["PAGE_EXECUTE_READWRITE"]));
    assert_eq!(parts("PAGE_PROTECTION", 0x41, 32), Some(vec!["PAGE_EXECUTE_READWRITE", "PAGE_NOACCESS"]));
    assert_eq!(parts("FILE_SHARE_MODE", 0x10, 32), None, "a bit without a name leaves the value numeric");
}
