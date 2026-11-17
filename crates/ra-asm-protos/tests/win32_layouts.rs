//! The layouts of the structures the Windows API's prototypes point to, generated from the SDK
//! headers: the crate's `win32_layouts` module holds, for every structure a parameter of a
//! function of the prototype table points to, its fields with offset, size and kind at 32 and at
//! 64 bits. The ignored `regenerate_win32_layouts` writes it: clang-cl preprocesses the headers
//! at both widths for the functions and the typedef names of the structures, then compiles, at
//! both widths, a probe that holds a pointer to each function and to each name, dumping every
//! record's layout with canonical types: a function's field reads the records its parameters
//! point to, a name's field the record it stands for. The other tests read sample dumps.
//!
//!   `cargo test -p ra-asm-protos --test win32_layouts -- --ignored regenerate`
//!
//! A structure that holds a type the reading does not know is left out, with every structure
//! that holds it: the table says nothing rather than something wrong. So are the handle
//! structures (`struct HWND__ { int unused; }`), which no program fills.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The file of the `win32_layouts` module, from the repository's root.
const TABLE_FILE: &str = "crates/ra-asm-protos/src/win32_layouts.rs";

/// The headers of the prototype table (`tests/win32_protos.rs`).
const HEADERS: &[&str] = &[
    "winsock2.h",
    "ws2tcpip.h",
    "windows.h",
    "shellapi.h",
    "shlobj.h",
    "commctrl.h",
    "commdlg.h",
    "psapi.h",
    "tlhelp32.h",
    "wininet.h",
    "mmsystem.h",
    "dbghelp.h",
    "uxtheme.h",
    "dwmapi.h",
];

/// The 32-bit and the 64-bit target, in the table's order.
const TARGETS: [&str; 2] = ["i686-pc-windows-msvc", "x86_64-pc-windows-msvc"];

/// A library whose layouts a table holds: its headers, where they are, which of its functions the
/// table covers, and the module the table is written to.
struct Library {
    /// The text that includes the headers, with what they want defined first.
    includes: String,
    /// The directories the headers are found in beside the SDK's.
    dirs: Vec<PathBuf>,
    /// The functions of the preprocessed headers the table covers.
    functions: fn(&[&str]) -> BTreeSet<String>,
    /// The fewest functions and structures a complete reading of the headers finds.
    least: (usize, usize),
    /// The module's file, from the repository's root.
    file: &'static str,
    /// The first lines of the module's documentation: what the table is of.
    of: &'static str,
    /// The ignored test that writes the module.
    by: &'static str,
    /// Whether the module also holds the kinds of the functions' results and parameters, for a
    /// library the Windows API's prototype table does not cover.
    kinds: bool,
}

#[test]
#[ignore = "a maintenance aid: regenerates the win32 layouts from the SDK headers with clang-cl"]
fn regenerate_win32_layouts() {
    let mut includes = String::from("#define _WIN32_WINNT 0x0A00\n#define NTDDI_VERSION 0x0A000000\n");
    for header in HEADERS {
        let _ = writeln!(includes, "#include <{header}>");
    }
    regenerate(&Library {
        includes,
        dirs: Vec::new(),
        functions: |tokens| stdcall_functions(tokens).into_iter().filter(|name| ra_asm_protos::win32_kinds(name).is_some()).collect(),
        least: (1000, 100),
        file: TABLE_FILE,
        of: "the Windows API's prototypes point to, from the SDK headers",
        by: "regenerate_win32_layouts` in `tests/win32_layouts.rs",
        kinds: false,
    });
}

/// The file of the `sdl2_layouts` module, from the repository's root.
const SDL2_FILE: &str = "crates/ra-asm-protos/src/sdl2_layouts.rs";

/// The include directory of SDL2 and its mixer and network libraries: `RA_SDL2_INCLUDE`, else
/// vcpkg's under `RA_VCPKG_ROOT` (`D:/rhipper/vcpkg` without it). SDL's headers are one for every
/// target.
fn sdl2_include() -> PathBuf {
    if let Some(dir) = std::env::var_os("RA_SDL2_INCLUDE") {
        return PathBuf::from(dir);
    }
    let vcpkg = std::env::var_os("RA_VCPKG_ROOT").map_or_else(|| PathBuf::from("D:/rhipper/vcpkg"), PathBuf::from);
    vcpkg.join("installed/x86-windows/include")
}

#[test]
#[ignore = "a maintenance aid: regenerates the SDL2 layouts from SDL2's headers with clang-cl"]
fn regenerate_sdl2_layouts() {
    let include = sdl2_include();
    regenerate(&Library {
        includes: "#include <SDL2/SDL.h>\n#include <SDL2/SDL_mixer.h>\n#include <SDL2/SDL_net.h>\n".to_owned(),
        dirs: vec![include.clone(), include.join("SDL2")],
        functions: |tokens| {
            cdecl_functions(tokens).into_iter().filter(|name| ["SDL_", "Mix_", "SDLNet_"].iter().any(|p| name.starts_with(p))).collect()
        },
        least: (500, 10),
        file: SDL2_FILE,
        of: "SDL2's, SDL2_mixer's and SDL2_net's prototypes point to, from their headers",
        by: "regenerate_sdl2_layouts` in `tests/win32_layouts.rs",
        kinds: true,
    });
}

/// Writes the layouts of `library`'s structures to its module.
fn regenerate(library: &Library) {
    let stem = Path::new(library.file).file_stem().and_then(|s| s.to_str()).unwrap_or("layouts");
    let dir = ra_asm_testing::Scratch::new("ra-asm-protos-tests", stem);
    let headers = dir.join("headers.c");
    std::fs::write(&headers, &library.includes).unwrap();
    let dirs: Vec<String> = library.dirs.iter().map(|d| format!("/I{}", d.display())).collect();
    let with_dirs = |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).chain(dirs.iter().cloned()).collect() };
    // A function or a name the headers declare at one width only is not probed.
    let mut functions: Option<BTreeSet<String>> = None;
    let mut names: Option<BTreeSet<String>> = None;
    let mut enums: BTreeSet<String> = BTreeSet::new();
    for target in TARGETS {
        let target = format!("--target={target}");
        let args = with_dirs(&[&target, "/nologo", "/E", "/W0"]);
        let text = clang(&args.iter().map(String::as_str).collect::<Vec<_>>(), &headers);
        let tokens = tokens_of(&text);
        enums.extend(enum_typedefs(&tokens));
        let found = (library.functions)(&tokens);
        functions = Some(match functions {
            Some(before) => before.intersection(&found).cloned().collect(),
            None => found,
        });
        let found = record_typedefs(&tokens);
        names = Some(match names {
            Some(before) => before.intersection(&found).cloned().collect(),
            None => found,
        });
    }
    let (functions, names) = (functions.unwrap(), names.unwrap());
    assert!(functions.len() > library.least.0, "{} functions read, which is too few for {}", functions.len(), library.file);
    let mut probe = library.includes.clone();
    probe.push_str("struct ra_functions {\n");
    for function in &functions {
        let _ = writeln!(probe, "    __typeof__({function}) *ra_{function};");
    }
    probe.push_str("};\nstruct ra_names {\n");
    for name in &names {
        let _ = writeln!(probe, "    {name} *ra_{name};");
    }
    probe.push_str("};\n");
    let source = dir.join("probe.c");
    std::fs::write(&source, probe).unwrap();
    let object = format!("/Fo{}", dir.join("probe.obj").display());
    let [at32, at64] = TARGETS.map(|target| {
        let target = format!("--target={target}");
        let flags = ["/nologo", "/c", "/W0", "-Xclang", "-fdump-record-layouts-canonical", "-Xclang", "-fdump-record-layouts-complete"];
        let mut args = vec![target.as_str()];
        args.extend(flags);
        args.push(&object);
        let args = with_dirs(&args);
        dumps_of(&clang(&args.iter().map(String::as_str).collect::<Vec<_>>(), &source))
    });
    let table = Table::of(&at32, &at64);
    assert!(table.records.len() > library.least.1, "{} structures laid out, which is too few for {}", table.records.len(), library.file);
    let path = root().join(library.file);
    let mut text = table.render(library.of, library.by);
    if library.kinds {
        text.push_str(&render_kinds(&kinds_of(&at32, &enums)));
    }
    std::fs::write(&path, text).unwrap();
    eprintln!(
        "{} structures, {} names, {} functions -> {}",
        table.records.len(),
        table.aliases.len(),
        table.pointees.len(),
        path.display()
    );
}

/// The standard output of clang-cl with `args` on `source`.
fn clang(args: &[&str], source: &Path) -> String {
    let out = Command::new("clang-cl").args(args).arg(source).output().unwrap();
    assert!(out.status.success(), "clang-cl {}:\n{}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The tokens of preprocessed C: identifiers, numbers, string literals and single punctuation
/// characters, with the line markers left out.
fn tokens_of(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let bytes = line.as_bytes();
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
            } else if c == b'"' || c == b'\'' {
                i += 1;
                while i < bytes.len() && bytes[i] != c {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
            } else if c == b'.' && bytes[i..].starts_with(b"...") {
                i += 3;
            } else {
                i += 1;
            }
            tokens.push(&line[start..i.min(bytes.len())]);
        }
    }
    tokens
}

fn is_identifier(token: &str) -> bool {
    token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The top-level statements, each to its `;`, or to the `}` of a function's body: the body of a
/// structure stays in its statement, with the declarators after it.
fn statements<'a>(tokens: &'a [&'a str]) -> Vec<&'a [&'a str]> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, &t) in tokens.iter().enumerate() {
        match t {
            "{" => depth += 1,
            "}" => {
                depth = depth.saturating_sub(1);
                if depth == 0 && !matches!(tokens.get(start), Some(&("typedef" | "struct" | "union" | "enum"))) {
                    out.push(&tokens[start..=i]);
                    start = i + 1;
                }
            }
            ";" if depth == 0 => {
                out.push(&tokens[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

/// The `__stdcall` functions the statements declare, as the prototype table reads them.
fn stdcall_functions(tokens: &[&str]) -> BTreeSet<String> {
    functions_of(tokens, "__stdcall")
}

/// The `__cdecl` functions the statements declare: a library's `SDLCALL` ones.
fn cdecl_functions(tokens: &[&str]) -> BTreeSet<String> {
    functions_of(tokens, "__cdecl")
}

/// The functions of the `convention` the statements declare.
fn functions_of(tokens: &[&str], convention: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for statement in statements(tokens) {
        let Some(at) = statement.iter().position(|&t| t == convention) else { continue };
        let (Some(&name), Some(&"(")) = (statement.get(at + 1), statement.get(at + 2)) else { continue };
        if is_identifier(name)
            && !statement[..at].iter().any(|&t| matches!(t, "typedef" | "static" | "inline" | "__inline" | "__forceinline"))
        {
            out.insert(name.to_owned());
        }
    }
    out
}

/// The names `typedef` gives to a structure or a union, or to another such name: the plain
/// declarators of `typedef struct T { ... } A, *PA;`, `typedef struct T A;` and `typedef A B;`.
fn record_typedefs(tokens: &[&str]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for statement in statements(tokens) {
        if statement.first() != Some(&"typedef") {
            continue;
        }
        let declarators = if let Some(at) = record_keyword(statement) {
            let mut i = at + 1;
            if statement.get(i).is_some_and(|t| is_identifier(t)) {
                i += 1;
            }
            if statement.get(i) == Some(&"{") {
                let Some(close) = closing_brace(statement, i) else { continue };
                &statement[close + 1..]
            } else {
                &statement[i..]
            }
        } else if statement.get(1).is_some_and(|t| names.contains(*t)) {
            &statement[2..]
        } else {
            continue;
        };
        for declarator in declarators.split(|&t| t == ",") {
            if let [name] = declarator
                && is_identifier(name)
            {
                names.insert((*name).to_owned());
            }
        }
    }
    names
}

/// Where a typedef's `struct` or `union` keyword is, when it names one: before any other type
/// word, past a `__declspec(...)` and qualifiers.
fn record_keyword(statement: &[&str]) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &t) in statement.iter().enumerate().skip(1) {
        match t {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "struct" | "union" if depth == 0 => return Some(i),
            "*" | "{" | "," => return None,
            _ if depth == 0 && is_identifier(t) && !matches!(t, "const" | "volatile" | "__declspec") => return None,
            _ => {}
        }
    }
    None
}

/// The `}` that closes the `{` at `open`.
fn closing_brace(tokens: &[&str], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &t) in tokens.iter().enumerate().skip(open) {
        match t {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A record's layout as clang dumps it at one width.
struct Dump {
    union: bool,
    size: u32,
    align: u32,
    members: Vec<Member>,
}

/// A member the dump does not lay out further: its path through the named nested members, its
/// byte offset, whether it is a bit field, and its canonical type.
struct Member {
    name: String,
    offset: u32,
    bits: bool,
    ty: String,
}

/// The records of a `-fdump-record-layouts` output, by the name clang prints for them (the tag,
/// or `(unnamed at <file>:<line>:<column>)`).
fn dumps_of(text: &str) -> BTreeMap<String, Dump> {
    let mut out = BTreeMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "*** Dumping AST Record Layout" {
            continue;
        }
        let Some((_, header)) = lines.next().and_then(|l| l.split_once('|')) else { continue };
        let header = header.trim();
        let (union, identity) = if let Some(rest) = header.strip_prefix("struct ") {
            (false, rest)
        } else if let Some(rest) = header.strip_prefix("union ") {
            (true, rest)
        } else {
            continue;
        };
        let mut entries = Vec::new();
        let mut size_align = None;
        for line in lines.by_ref() {
            let Some((left, right)) = line.split_once('|') else { break };
            let right = right.strip_prefix(' ').unwrap_or(right);
            if let Some(rest) = right.trim_start().strip_prefix("[sizeof=") {
                size_align = sizeof_align(rest);
                break;
            }
            let left = left.trim();
            let (offset, bits) = match left.split_once(':') {
                Some((offset, _)) => (offset, true),
                None => (left, false),
            };
            let Ok(offset) = offset.parse::<u32>() else { break };
            entries.push(Entry { indent: right.len() - right.trim_start().len(), offset, bits, content: right.trim() });
        }
        let Some((size, align)) = size_align else { continue };
        out.insert(identity.to_owned(), Dump { union, size, align, members: leaves(&entries) });
    }
    out
}

/// A line of a record's dump: the indent of its member, which is a nested member of the line
/// above it with less, its offset, whether it is a bit field, and its type and name.
struct Entry<'a> {
    indent: usize,
    offset: u32,
    bits: bool,
    content: &'a str,
}

/// The size and the alignment of `[sizeof=48, align=8]`, past its `[sizeof=`.
fn sizeof_align(rest: &str) -> Option<(u32, u32)> {
    let (size, rest) = rest.split_once(',')?;
    let align = rest.trim().strip_prefix("align=")?.trim_end_matches([']', ',']);
    Some((size.trim().parse().ok()?, align.parse().ok()?))
}

/// The members of a dump that hold no nested member, named by their path.
fn leaves(entries: &[Entry<'_>]) -> Vec<Member> {
    let mut out = Vec::new();
    let mut path: Vec<(usize, &str)> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        while path.last().is_some_and(|&(indent, _)| indent >= entry.indent) {
            path.pop();
        }
        // An anonymous member's type, `union _X::(unnamed at x.h:1:2)`, ends the line.
        let (ty, name) =
            if entry.content.ends_with(')') { (entry.content, "") } else { entry.content.rsplit_once(' ').unwrap_or((entry.content, "")) };
        if entries.get(i + 1).is_none_or(|next| next.indent <= entry.indent) {
            let full: Vec<&str> = path.iter().map(|&(_, n)| n).chain([name]).filter(|n| !n.is_empty()).collect();
            out.push(Member { name: full.join("."), offset: entry.offset, bits: entry.bits, ty: ty.trim().to_owned() });
        }
        path.push((entry.indent, name));
    }
    out
}

/// The parameters' types of a canonical function pointer type, `int (*)(struct tagMSG *, ...)`.
fn params(ty: &str) -> Option<Vec<&str>> {
    let open = ty.find("(*)(")? + 3;
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut out = Vec::new();
    for (i, c) in ty[open..].char_indices() {
        let i = open + i;
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    out.push(ty[start..i].trim());
                    if out == [""] || out == ["void"] {
                        out.clear();
                    }
                    return Some(out);
                }
            }
            ',' if depth == 1 => {
                out.push(ty[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    None
}

/// The names `typedef enum { ... } NAME;` gives an enumeration without a tag, which clang's
/// canonical types keep (`SDL_bool`).
fn enum_typedefs(tokens: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for statement in statements(tokens) {
        if statement.first() != Some(&"typedef") || statement.get(1) != Some(&"enum") {
            continue;
        }
        let Some(open) = statement.iter().position(|&t| t == "{") else { continue };
        let Some(close) = closing_brace(statement, open) else { continue };
        for declarator in statement[close + 1..].split(|&t| t == ",") {
            if let [name] = declarator
                && is_identifier(name)
            {
                out.insert((*name).to_owned());
            }
        }
    }
    out
}

/// The kind letter of a canonical type at 32 bits, as the prototype tables write it: `i` an
/// integer, an enumeration or a pointer of up to 4 bytes, `w` a 64-bit integer or an 8-byte
/// structure by value, `d` a double, `f` a float, `v` none (a result alone); `None` for a type a
/// table cannot say (a structure of another size by value). `enums` are the names of enumerations
/// without a tag.
fn kind_letter(ty: &str, dumps: &BTreeMap<String, Dump>, enums: &BTreeSet<String>) -> Option<char> {
    let ty = ty.trim();
    let ty = ty.strip_prefix("const ").unwrap_or(ty);
    if ty == "void" {
        return Some('v');
    }
    if ty.ends_with('*') || ty.contains("(*)") || ty.starts_with("enum ") || enums.contains(ty) {
        return Some('i');
    }
    if let Some(identity) = ty.strip_prefix("struct ").or_else(|| ty.strip_prefix("union ")) {
        return (dumps.get(identity)?.size == 8).then_some('w');
    }
    match ty {
        "char" | "signed char" | "unsigned char" | "_Bool" | "short" | "unsigned short" | "int" | "unsigned int" | "long"
        | "unsigned long" => Some('i'),
        "long long" | "unsigned long long" => Some('w'),
        "double" => Some('d'),
        "float" => Some('f'),
        _ => None,
    }
}

/// The kinds of every probed function the reading can say all of, from the 32-bit dump: the
/// result's letter, then each parameter's. A variadic function is left out.
fn kinds_of(at32: &BTreeMap<String, Dump>, enums: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(probe) = at32.get("ra_functions") else { return out };
    for member in &probe.members {
        let (Some(function), Some(types), Some(result)) = (member.name.strip_prefix("ra_"), params(&member.ty), result(&member.ty)) else {
            continue;
        };
        if types.contains(&"...") {
            continue;
        }
        let mut letters = String::new();
        let all = std::iter::once(result).chain(types.iter().copied()).enumerate().all(|(i, ty)| match kind_letter(ty, at32, enums) {
            Some('v') if i > 0 => false,
            Some(c) => {
                letters.push(c);
                true
            }
            None => false,
        });
        if all {
            out.insert(function.to_owned(), letters);
        }
    }
    out
}

/// The `KINDS` table of a library's module, sorted by name.
fn render_kinds(kinds: &BTreeMap<String, String>) -> String {
    let mut out = String::from("\npub(super) static KINDS: &[(&str, &str)] = &[\n");
    for (function, letters) in kinds {
        let _ = writeln!(out, "    ({function:?}, {letters:?}),");
    }
    out.push_str("];\n");
    out
}

/// The result's type of a canonical function pointer type: `struct SDL_Surface *` of
/// `struct SDL_Surface *(*)(unsigned int, int)`.
fn result(ty: &str) -> Option<&str> {
    Some(ty[..ty.find("(*)(")?].trim())
}

/// The record a canonical type points to, through one pointer: `const struct tagMSG *` is
/// `tagMSG`. A `void **`, where the callee writes a pointer back (`SDL_LockTexture`'s pixels), is
/// [`ra_asm_protos::POINTER_CELL`]'s.
fn pointee(ty: &str) -> Option<&str> {
    let ty = ty.trim();
    if ty == "void **" {
        return Some(ra_asm_protos::POINTER_CELL);
    }
    let inner = ty.strip_prefix("const ").unwrap_or(ty).strip_suffix(" *")?;
    let identity = inner.strip_prefix("struct ").or_else(|| inner.strip_prefix("union "))?;
    (!identity.contains('*')).then_some(identity)
}

/// What a canonical type is at a width of `pointer` bytes: its kind letter, its element count,
/// its size, and the record of its elements; `None` for a type the reading does not know.
fn type_of(ty: &str, bits: bool, pointer: u32, dumps: &BTreeMap<String, Dump>) -> Option<(char, u32, u32, Option<String>)> {
    let mut element = ty.trim();
    // An array of code pointers (`SDL_AudioCVT`'s `void (*[10])(...)` filters): pointers, which
    // the host writes and calls itself.
    if let Some(open) = element.find("(*[") {
        let rest = &element[open + 3..];
        let count: u32 = rest[..rest.find(']')?].parse().ok()?;
        return (!bits).then_some(('p', count, pointer * count, None));
    }
    let mut count = 1u32;
    while element.ends_with(']') {
        let open = element.rfind('[')?;
        count *= element[open + 1..element.len() - 1].parse::<u32>().ok()?;
        element = element[..open].trim_end();
    }
    for qualifier in ["const ", "volatile "] {
        element = element.strip_prefix(qualifier).unwrap_or(element);
    }
    for qualifier in [" const", " volatile"] {
        element = element.strip_suffix(qualifier).unwrap_or(element);
    }
    if element.ends_with('*') || element.contains("(*)") {
        return (!bits).then_some(('p', count, pointer * count, None));
    }
    if let Some(identity) = element.strip_prefix("struct ").or_else(|| element.strip_prefix("union ")) {
        let size = dumps.get(identity)?.size;
        return (!bits).then(|| ('r', count, size * count, Some(identity.to_owned())));
    }
    let (kind, size) = if element.starts_with("enum ") {
        ('i', 4)
    } else {
        match element {
            "char" | "signed char" => ('i', 1),
            "unsigned char" | "_Bool" => ('u', 1),
            "short" => ('i', 2),
            "unsigned short" => ('u', 2),
            "int" | "long" => ('i', 4),
            "unsigned int" | "unsigned long" => ('u', 4),
            "long long" => ('i', 8),
            "unsigned long long" => ('u', 8),
            "float" => ('f', 4),
            "double" | "long double" => ('f', 8),
            _ => return None,
        }
    };
    Some((if bits { 'b' } else { kind }, count, size * count, None))
}

/// A field of the table: its path, kind letter, count, offset and size at 32 then at 64 bits, and
/// the record of its elements by the name clang prints.
#[derive(Debug, PartialEq, Eq)]
struct Row {
    name: String,
    kind: char,
    count: u32,
    at32: (u32, u32),
    at64: (u32, u32),
    record: Option<String>,
}

/// A structure of the table, its fields at both widths.
#[derive(Debug, PartialEq, Eq)]
struct Laid {
    union: bool,
    size32: u32,
    align32: u32,
    size64: u32,
    align64: u32,
    rows: Vec<Row>,
}

impl Laid {
    /// The layouts of a record at both widths, when the reading knows every member's type and
    /// the members agree. The members pair by position, or by name when the 64-bit layout orders
    /// them otherwise (`WSAData`) or has one the 32-bit layout does not (`PartitionId`), which
    /// sits at no place at 32 bits; a member of the 32-bit layout alone leaves the record out.
    fn of(identity: &str, at32: &BTreeMap<String, Dump>, at64: &BTreeMap<String, Dump>) -> Option<Laid> {
        let (d32, d64) = (at32.get(identity)?, at64.get(identity)?);
        if d32.union != d64.union {
            return None;
        }
        let positional = d32.members.len() == d64.members.len() && d32.members.iter().zip(&d64.members).all(|(a, b)| a.name == b.name);
        let unique = |members: &[Member]| members.iter().enumerate().all(|(i, m)| members[..i].iter().all(|n| n.name != m.name));
        if !positional
            && (!unique(&d32.members) || !unique(&d64.members) || d32.members.iter().any(|m| d64.members.iter().all(|n| n.name != m.name)))
        {
            return None;
        }
        let mut rows = Vec::new();
        for (i, m64) in d64.members.iter().enumerate() {
            let (k64, n64, s64, r64) = type_of(&m64.ty, m64.bits, 8, at64)?;
            let m32 = if positional { d32.members.get(i) } else { d32.members.iter().find(|m| m.name == m64.name) };
            let (k32, n32, place32, r32) = match m32 {
                Some(m32) => {
                    let (k32, n32, s32, r32) = type_of(&m32.ty, m32.bits, 4, at32)?;
                    (k32, n32, (m32.offset, s32), r32)
                }
                None => (k64, n64, (0, 0), r64.clone()),
            };
            if k32 != k64 || n32 != n64 || r32 != r64 {
                return None;
            }
            rows.push(Row { name: m64.name.clone(), kind: k64, count: n64, at32: place32, at64: (m64.offset, s64), record: r64 });
        }
        Some(Laid { union: d64.union, size32: d32.size, align32: d32.align, size64: d64.size, align64: d64.align, rows })
    }

    /// No field moves or changes its size with the width.
    fn same(&self) -> bool {
        self.size32 == self.size64 && self.align32 == self.align64 && self.rows.iter().all(|r| r.at32 == r.at64)
    }

    /// Two fields share bytes at either width (bit fields share their unit by design).
    fn overlapping(&self) -> bool {
        let rows: Vec<&Row> = self.rows.iter().filter(|r| r.kind != 'b' && r.at64.1 > 0).collect();
        let meet = |a: (u32, u32), b: (u32, u32)| a.0 < b.0 + b.1 && b.0 < a.0 + a.1;
        rows.iter().enumerate().any(|(i, a)| rows[i + 1..].iter().any(|b| meet(a.at32, b.at32) || meet(a.at64, b.at64)))
    }

    /// A `DECLARE_HANDLE` structure, `struct HWND__ { int unused; }`.
    fn is_handle(&self, identity: &str) -> bool {
        identity.ends_with("__") && matches!(&self.rows[..], [row] if row.name == "unused")
    }
}

/// The generated table: the structures by their name in the table, the typedef names of each,
/// the parameters of each function that point to one, and the functions whose result does.
struct Table {
    records: BTreeMap<String, Laid>,
    aliases: BTreeMap<String, String>,
    pointees: BTreeMap<String, Vec<(usize, String)>>,
    results: BTreeMap<String, String>,
}

impl Table {
    /// The table from the dumps of the probe at 32 and at 64 bits.
    fn of(at32: &BTreeMap<String, Dump>, at64: &BTreeMap<String, Dump>) -> Table {
        let members = |probe: &str| at64.get(probe).map(|d| &d.members[..]).unwrap_or_default();
        let mut names: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for member in members("ra_names") {
            if let (Some(alias), Some(identity)) = (member.name.strip_prefix("ra_"), pointee(&member.ty)) {
                names.entry(identity).or_default().insert(alias);
            }
        }
        // A record without a tag goes by its first typedef name, and without one not at all.
        let key = |identity: &str| -> Option<String> {
            if identity.contains("(unnamed") { names.get(identity)?.first().map(|n| (*n).to_owned()) } else { Some(identity.to_owned()) }
        };
        let mut calls: BTreeMap<String, Vec<(usize, &str)>> = BTreeMap::new();
        let mut returned: BTreeMap<String, &str> = BTreeMap::new();
        for member in members("ra_functions") {
            let (Some(function), Some(types)) = (member.name.strip_prefix("ra_"), params(&member.ty)) else { continue };
            let pointed: Vec<(usize, &str)> = types.iter().enumerate().filter_map(|(i, ty)| Some((i, pointee(ty)?))).collect();
            if !pointed.is_empty() {
                calls.insert(function.to_owned(), pointed);
            }
            if let Some(identity) = result(&member.ty).and_then(pointee) {
                returned.insert(function.to_owned(), identity);
            }
        }
        // Every record a parameter or a result points to, and every record an array of the first
        // holds.
        let mut laid: BTreeMap<&str, Option<Laid>> = BTreeMap::new();
        let mut work: Vec<&str> = calls.values().flatten().map(|&(_, identity)| identity).chain(returned.values().copied()).collect();
        while let Some(identity) = work.pop() {
            if laid.contains_key(identity) || identity == ra_asm_protos::POINTER_CELL {
                continue;
            }
            let layout = Laid::of(identity, at32, at64).filter(|l| !l.is_handle(identity));
            if let Some(layout) = &layout {
                work.extend(
                    layout.rows.iter().filter_map(|r| r.record.as_deref()).filter_map(|r| at64.get_key_value(r).map(|(k, _)| k.as_str())),
                );
            }
            laid.insert(identity, layout);
        }
        // A record that holds one the table leaves out goes too.
        let mut kept: BTreeSet<&str> =
            laid.iter().filter(|(identity, layout)| layout.is_some() && key(identity).is_some()).map(|(identity, _)| *identity).collect();
        loop {
            let before = kept.len();
            let holds_missing = |identity: &str| {
                laid[identity].as_ref().is_some_and(|l| l.rows.iter().any(|r| r.record.as_deref().is_some_and(|r| !kept.contains(r))))
            };
            let dropped: Vec<&str> = kept.iter().copied().filter(|identity| holds_missing(identity)).collect();
            for identity in dropped {
                kept.remove(identity);
            }
            if kept.len() == before {
                break;
            }
        }
        let mut records = BTreeMap::new();
        for &identity in &kept {
            let mut layout = laid.remove(identity).flatten().expect("a kept record is laid out");
            for row in &mut layout.rows {
                row.record = row.record.as_deref().and_then(key);
            }
            records.insert(key(identity).expect("a kept record has a name"), layout);
        }
        let mut aliases = BTreeMap::new();
        for (identity, aliases_of) in &names {
            let Some(name) = kept.contains(identity).then(|| key(identity)).flatten() else { continue };
            for &alias in aliases_of {
                if alias != name && !records.contains_key(alias) {
                    aliases.insert(alias.to_owned(), name.clone());
                }
            }
        }
        // A pointer cell is the protos crate's own record.
        let name_of = |identity: &str| -> Option<String> {
            if identity == ra_asm_protos::POINTER_CELL {
                Some(identity.to_owned())
            } else {
                kept.contains(identity).then(|| key(identity)).flatten()
            }
        };
        let mut pointees = BTreeMap::new();
        for (function, pointed) in calls {
            let pointed: Vec<(usize, String)> = pointed.into_iter().filter_map(|(i, identity)| Some((i, name_of(identity)?))).collect();
            if !pointed.is_empty() {
                pointees.insert(function, pointed);
            }
        }
        let results = returned
            .into_iter()
            .filter(|&(_, identity)| identity != ra_asm_protos::POINTER_CELL)
            .filter_map(|(function, identity)| Some((function, name_of(identity)?)))
            .collect();
        Table { records, aliases, pointees, results }
    }

    /// The table's module: the layouts of the structures `of`, written by the test `by`.
    fn render(&self, of: &str, by: &str) -> String {
        let mut file = String::new();
        let doc = format!(
            "The layouts of the structures {of} at 32 and at 64 bits: every field's kind (`p` pointer, `i` signed, \
             `u` unsigned, `f` floating, `b` bit field, `r` record), count, and offset and size at each width; the \
             typedef names of the structures; and the parameters of each function that point to one. Generated by \
             the ignored test `{by}`; do not edit."
        );
        let mut line = String::from("//!");
        for word in doc.split(' ') {
            if line.len() + 1 + word.len() > 104 {
                file.push_str(&line);
                file.push('\n');
                line = String::from("//!");
            }
            line.push(' ');
            line.push_str(word);
        }
        file.push_str(&line);
        file.push_str("\n\n");
        file.push_str("use crate::layouts::{Record, field as f, record as r};\n\n");
        file.push_str("pub(super) static RECORDS: &[Record] = &[\n");
        for (name, layout) in &self.records {
            let _ = writeln!(
                file,
                "    r({name:?}, {}, {}, {}, {}, {}, {}, &[",
                layout.overlapping(),
                layout.same(),
                layout.size32,
                layout.align32,
                layout.size64,
                layout.align64
            );
            for row in &layout.rows {
                let _ = writeln!(
                    file,
                    "        f({:?}, {:?}, {}, {}, {}, {}, {}, {:?}),",
                    row.name,
                    row.kind,
                    row.count,
                    row.at32.0,
                    row.at32.1,
                    row.at64.0,
                    row.at64.1,
                    row.record.as_deref().unwrap_or("")
                );
            }
            file.push_str("    ]),\n");
        }
        file.push_str("];\n\npub(super) static ALIASES: &[(&str, &str)] = &[\n");
        for (alias, name) in &self.aliases {
            let _ = writeln!(file, "    ({alias:?}, {name:?}),");
        }
        file.push_str("];\n\npub(super) static POINTEES: &[(&str, &[(u8, &str)])] = &[\n");
        for (function, pointed) in &self.pointees {
            let list: Vec<String> = pointed.iter().map(|(i, name)| format!("({i}, {name:?})")).collect();
            let _ = writeln!(file, "    ({function:?}, &[{}]),", list.join(", "));
        }
        file.push_str("];\n\npub(super) static RESULTS: &[(&str, &str)] = &[\n");
        for (function, name) in &self.results {
            let _ = writeln!(file, "    ({function:?}, {name:?}),");
        }
        file.push_str("];\n");
        file
    }
}

/// The 32-bit dump of a probe, clang-cl's output with the canonical types.
const DUMP32: &str = "\
*** Dumping AST Record Layout
         0 | struct tagPOINT
         0 |   long x
         4 |   long y
           | [sizeof=8, align=4]

*** Dumping AST Record Layout
         0 | struct HWND__
         0 |   int unused
           | [sizeof=4, align=4]

*** Dumping AST Record Layout
         0 | struct tagMSG
         0 |   struct HWND__ * hwnd
         4 |   unsigned int message
         8 |   unsigned int wParam
        12 |   long lParam
        16 |   unsigned long time
        20 |   struct tagPOINT pt
        20 |     long x
        24 |     long y
           | [sizeof=28, align=4]

*** Dumping AST Record Layout
         0 | union bits::(unnamed at probe.c:4:61)
         0 |   int i
         0 |   void * p
           | [sizeof=4, align=4]

*** Dumping AST Record Layout
         0 | struct (unnamed at probe.c:3:9)
     0:0-2 |   unsigned int a
     0:3-7 |   unsigned int b
         4 |   struct tagPOINT[2] pts
        20 |   union bits::(unnamed at probe.c:4:61) u
        20 |     int i
        20 |     void * p
        24 |   __int128 wide
           | [sizeof=40, align=8]

*** Dumping AST Record Layout
         0 | struct ra_functions
         0 |   int (*)(struct tagMSG *, struct HWND__ *, unsigned int, unsigned int) __attribute__((stdcall)) ra_GetMessageA
         4 |   int (*)(struct tagPOINT *) __attribute__((stdcall)) ra_GetCursorPos
         8 |   int (*)(void) __attribute__((stdcall)) ra_GetLastInputTick
           | [sizeof=12, align=4]

*** Dumping AST Record Layout
         0 | struct ra_names
         0 |   struct tagMSG * ra_MSG
         4 |   struct tagPOINT * ra_POINT
         8 |   struct (unnamed at probe.c:3:9) * ra_ODD
           | [sizeof=12, align=4]
";

/// The 64-bit dump of the same probe.
const DUMP64: &str = "\
*** Dumping AST Record Layout
         0 | struct tagPOINT
         0 |   long x
         4 |   long y
           | [sizeof=8, align=4]

*** Dumping AST Record Layout
         0 | struct HWND__
         0 |   int unused
           | [sizeof=4, align=4]

*** Dumping AST Record Layout
         0 | struct tagMSG
         0 |   struct HWND__ * hwnd
         8 |   unsigned int message
        16 |   unsigned long long wParam
        24 |   long long lParam
        32 |   unsigned long time
        36 |   struct tagPOINT pt
        36 |     long x
        40 |     long y
           | [sizeof=48, align=8]

*** Dumping AST Record Layout
         0 | struct ra_functions
         0 |   int (*)(struct tagMSG *, struct HWND__ *, unsigned int, unsigned int) ra_GetMessageA
         8 |   int (*)(struct tagPOINT *) ra_GetCursorPos
        16 |   int (*)(void) ra_GetLastInputTick
           | [sizeof=24, align=8]

*** Dumping AST Record Layout
         0 | struct ra_names
         0 |   struct tagMSG * ra_MSG
         8 |   struct tagPOINT * ra_POINT
        16 |   struct (unnamed at probe.c:3:9) * ra_ODD
           | [sizeof=24, align=8]
";

#[test]
fn dumps_read_as_leaf_members() {
    let dumps = dumps_of(DUMP32);
    let msg = &dumps["tagMSG"];
    assert_eq!((msg.union, msg.size, msg.align), (false, 28, 4));
    let names: Vec<(&str, u32)> = msg.members.iter().map(|m| (m.name.as_str(), m.offset)).collect();
    assert_eq!(names, [("hwnd", 0), ("message", 4), ("wParam", 8), ("lParam", 12), ("time", 16), ("pt.x", 20), ("pt.y", 24)]);
    assert_eq!(msg.members[0].ty, "struct HWND__ *");
    let odd = &dumps["(unnamed at probe.c:3:9)"];
    let names: Vec<(&str, bool)> = odd.members.iter().map(|m| (m.name.as_str(), m.bits)).collect();
    assert_eq!(names, [("a", true), ("b", true), ("pts", false), ("u.i", false), ("u.p", false), ("wide", false)]);
    assert_eq!(odd.members[2].ty, "struct tagPOINT[2]");
    assert!(dumps["bits::(unnamed at probe.c:4:61)"].union);
}

#[test]
fn types_read_as_kinds() {
    let dumps = dumps_of(DUMP32);
    assert_eq!(type_of("const char *", false, 8, &dumps), Some(('p', 1, 8, None)));
    assert_eq!(type_of("long (*)(struct HWND__ *, unsigned int) __attribute__((stdcall))", false, 4, &dumps), Some(('p', 1, 4, None)));
    assert_eq!(type_of("unsigned short[32]", false, 4, &dumps), Some(('u', 32, 64, None)));
    assert_eq!(type_of("struct tagPOINT[2]", false, 4, &dumps), Some(('r', 2, 16, Some("tagPOINT".to_owned()))));
    assert_eq!(type_of("unsigned int", true, 4, &dumps), Some(('b', 1, 4, None)));
    assert_eq!(type_of("enum _FINDEX_INFO_LEVELS", false, 4, &dumps), Some(('i', 1, 4, None)));
    assert_eq!(type_of("__int128", false, 8, &dumps), None);
    assert_eq!(type_of("struct _UNDUMPED", false, 8, &dumps), None);
}

#[test]
fn parameters_point_to_records() {
    let ty = "int (*)(struct tagMSG *, struct HWND__ *, unsigned int, unsigned int) __attribute__((stdcall))";
    assert_eq!(params(ty), Some(vec!["struct tagMSG *", "struct HWND__ *", "unsigned int", "unsigned int"]));
    assert_eq!(params("int (*)(void)"), Some(vec![]));
    assert_eq!(params("void (*)(void (*)(int, int), const struct tagRECT *)"), Some(vec!["void (*)(int, int)", "const struct tagRECT *"]));
    assert_eq!(pointee("const struct tagRECT *"), Some("tagRECT"));
    assert_eq!(pointee("struct tagMSG **"), None);
    assert_eq!(pointee("unsigned int *"), None);
}

#[test]
fn typedefs_name_records() {
    let text = "\
typedef struct tagMSG { HWND hwnd; struct { int a; } inner; } MSG, *PMSG, *LPMSG;
typedef MSG MSG2;
typedef unsigned long DWORD;
typedef struct { int a; } ANON;
typedef struct _FWD FWD;
typedef __declspec(align(16)) struct _ALIGNED { int a; } ALIGNED;
typedef int (__stdcall *F)(struct X *);
typedef enum _E { A, B } E;
static __inline int __stdcall Helper(int x) { return x; }
__declspec(dllimport) BOOL __stdcall GetMessageA(LPMSG lpMsg, HWND hWnd, UINT a, UINT b);
";
    let tokens = tokens_of(text);
    let names: Vec<String> = record_typedefs(&tokens).into_iter().collect();
    assert_eq!(names, ["ALIGNED", "ANON", "FWD", "MSG", "MSG2"]);
    assert_eq!(stdcall_functions(&tokens).into_iter().collect::<Vec<_>>(), ["GetMessageA"]);
}

#[test]
fn the_probe_reads_as_a_table() {
    let table = Table::of(&dumps_of(DUMP32), &dumps_of(DUMP64));
    let names: Vec<&str> = table.records.keys().map(String::as_str).collect();
    // No function points to the unnamed record, and the handle is not a structure of the table.
    assert_eq!(names, ["tagMSG", "tagPOINT"]);
    let msg = &table.records["tagMSG"];
    assert!(!msg.same() && !msg.overlapping());
    assert_eq!(msg.rows[2], Row { name: "wParam".to_owned(), kind: 'u', count: 1, at32: (8, 4), at64: (16, 8), record: None });
    assert!(table.records["tagPOINT"].same());
    assert_eq!(table.aliases.iter().map(|(a, n)| (a.as_str(), n.as_str())).collect::<Vec<_>>(), [("MSG", "tagMSG"), ("POINT", "tagPOINT")]);
    assert_eq!(table.pointees["GetMessageA"], [(0, "tagMSG".to_owned())]);
    assert!(!table.pointees.contains_key("GetLastInputTick"));
    let file = table.render("the probe's functions point to", "a test");
    assert!(file.contains("    r(\"tagMSG\", false, false, 28, 4, 48, 8, &[\n        f(\"hwnd\", 'p', 1, 0, 4, 0, 8, \"\"),\n"), "{file}");
    assert!(file.contains("    (\"GetCursorPos\", &[(0, \"tagPOINT\")]),\n"), "{file}");
}
