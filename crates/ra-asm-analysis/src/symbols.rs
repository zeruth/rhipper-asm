//! The names of the functions, from the symbols of the build and from what the source could have
//! called them: a PDB beside the image names every function it describes and tells which static
//! library each came from, and a link map beside it names what it lists; the functions of a C
//! runtime library are the host's to supply.
//!
//! Every function ends with a name that is a C identifier, unique in the program, and not one the
//! port's own runtime or the host's C library owns: `main` becomes `main_`, since the driver has
//! `main`, and a second `helper` becomes `helper_<address>`.

use std::collections::{BTreeMap, HashSet};

use ra_asm_pe::Image;

use crate::function::{self, Function, Kind};
use crate::rtti::identifier;

/// The ranges of the image that the objects of the C runtime libraries fill: from the PDB's
/// section contributions, else from the link map, where a function of a runtime library owns the
/// code from its address to the next named function's, or to the end of its section (a build
/// with `/MT` and no PDB, such as the VS2010 programs, has only the map).
pub struct RuntimeRanges(Vec<(u64, u64)>);

impl RuntimeRanges {
    pub fn of(image: &Image) -> Self {
        let mut ranges: Vec<(u64, u64)> = if image.libraries.is_empty() {
            let section_end = |va: u64| {
                image
                    .sections
                    .iter()
                    .map(|s| (image.va(s.virtual_address), image.va(s.end_rva())))
                    .find(|&(start, end)| start <= va && va < end)
            };
            image
                .symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| s.library.as_deref().is_some_and(is_runtime_library))
                .map(|(i, s)| {
                    let next = image.symbols.get(i + 1).map(|n| n.va);
                    let end = section_end(s.va).map(|(_, end)| end).into_iter().chain(next).min().unwrap_or(s.va + 1);
                    (s.va, end)
                })
                .collect()
        } else {
            image.libraries.iter().filter(|r| is_runtime_library(&r.library)).map(|r| (r.va, r.va + u64::from(r.size))).collect()
        };
        ranges.sort_unstable();
        Self(ranges)
    }

    pub fn contains(&self, va: u64) -> bool {
        let i = self.0.partition_point(|&(start, _)| start <= va);
        i > 0 && va < self.0[i - 1].1
    }
}

/// Marks a function built inside a C runtime library's range as the host's, under the name the
/// host links it by: the PDB's name for it, or `F_<address>` for a piece of code the PDB does not
/// name. A thunk stays a thunk. A C++ function of a 64-bit runtime keeps its decorated name, which
/// the port links by; any other runtime function the host cannot link by that name in C (a 32-bit
/// C++ function, an exception handling funclet such as `common_control87$filt$0`) is the host's
/// all the same, under the name made an identifier, and a call would fail to link by name rather
/// than lift the runtime's code. A helper the port keeps ([`is_ported_helper`]) stays the program's.
pub fn classify(image: &Image, runtime: &RuntimeRanges, f: &mut Function) {
    if f.is_thunk() || !runtime.contains(f.entry) {
        return;
    }
    // The host links `_snprintf` and `__security_init_cookie` by those names: a plain C name
    // keeps its underscores, and only a mangled one is made an identifier.
    let name = match image.symbols.binary_search_by_key(&f.entry, |s| s.va) {
        // A C++ function of a 64-bit runtime keeps its decorated name, which the port declares
        // under an identifier with the decorated one as its assembler label, so the port links
        // the host library's own: the STL of `stl-clang64.exe` calls the `operator delete` of
        // `msvcrt.lib` and the `std::_Facet_Register` of `msvcprt.lib`, which under their
        // undecorated names nothing defined. A 32-bit one keeps the undecorated name its shim
        // is known by ([`ra_asm_lift`'s `x86_shim`]).
        Ok(i) if image.is_64() && image.symbols[i].name.starts_with('?') => image.symbols[i].name.clone(),
        Ok(i) => {
            let plain = source_name(&image.symbols[i], image.is_64());
            if plain.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { plain } else { identifier(&plain) }
        }
        Err(_) => function::default_name(f.entry),
    };
    if is_ported_helper(&name) {
        return;
    }
    f.kind = Kind::Runtime(name);
}

/// The helpers of the 32-bit C runtime that a port keeps as the program's own code even when a
/// runtime library supplies them: the 64-bit arithmetic (`_alldiv`), the conversions between
/// floating point and integers (`_ftol2`, and `_ftol3` and its kin of the newer compilers) and the `_CI*` intrinsics that take their operands in the x87 stack, the stack
/// probe, and the security cookie's check, which compares against the program's own cookie. The
/// host has no function of the same shape, and their code lifts exactly.
pub fn is_ported_helper(name: &str) -> bool {
    matches!(
        name,
        "_alldiv"
            | "_allmul"
            | "_allrem"
            | "_allshl"
            | "_allshr"
            | "_alldvrm"
            | "_aulldiv"
            | "_aullrem"
            | "_aullshr"
            | "_aulldvrm"
            | "_ftol"
            | "_ftol2"
            | "_ftol2_sse"
            | "_ftol2_sse_excpt"
            | "_ftoul2_legacy"
            | "_ftol3"
            | "_ftoui3"
            | "_ftoul3"
            | "_ftoul3_legacy"
            | "_dtol3"
            | "_dtoui3"
            | "_dtoul3"
            | "_dtoul3_legacy"
            | "_ltof3"
            | "_ultof3"
            | "_ltod3"
            | "_ultod3"
            | "_chkstk"
            | "_alloca_probe"
            | "_alloca_probe_8"
            | "_alloca_probe_16"
            | "__security_check_cookie"
    ) || name.starts_with("_CI")
}

/// The import library whose static part a DLL's own port takes along: `msvcprt.lib` links
/// `std::_Facet_Register` and the vector algorithms into a program of `msvcp140.dll`.
fn import_library_of(dll: &str) -> Option<&'static str> {
    let stem = dll.strip_suffix(".dll").unwrap_or(dll);
    ["msvcp140", "msvcp140d", "msvcp140_1", "msvcp140_2", "msvcp140_atomic_wait", "msvcp140_codecvt_ids"]
        .iter()
        .any(|s| s.eq_ignore_ascii_case(stem))
        .then_some("msvcprt")
}

/// Gives the functions of a ported DLL's import library back to the program: a DLL ported with
/// the program ([`crate::Analysis`] of a second image) answers its imports with objects of its own
/// layout, and the static part of its import library (`msvcprt.lib` for `msvcp140.dll`), which
/// [`classify`] made the host's as a runtime library's, takes those objects, so it lifts with the
/// program and the host's C++ runtime, which lays them out otherwise, supplies none of it. The
/// library's code checks each call it makes through a pointer with the control-flow guard, whose
/// no-op the C runtime's startup library holds and `__guard_check_icall_fptr` points at: the no-op
/// of the program `image` lifts with the code that calls it, under whichever name the linker's
/// folding left its address.
pub fn port_import_library(image: &Image, functions: &mut BTreeMap<u64, Function>, with: &Image) {
    let file = with.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let linked = with.exports.as_ref().map(|t| t.dll_name.clone());
    let Some(library) = file.iter().chain(&linked).find_map(|dll| import_library_of(dll)) else { return };
    let of_library = |object: &str| {
        let (lib, _) = object.split_once(':').unwrap_or((object, ""));
        lib.strip_suffix(".lib").unwrap_or(lib).eq_ignore_ascii_case(library)
    };
    let mut ported = false;
    for f in functions.values_mut() {
        if matches!(f.kind, Kind::Runtime(_)) && f.object.as_deref().is_some_and(of_library) {
            f.kind = Kind::Normal;
            ported = true;
        }
    }
    if !ported {
        return;
    }
    for symbol in image.symbols.iter().filter(|s| s.name.contains("_guard_check_icall_nop")) {
        if let Some(f) = functions.get_mut(&symbol.va)
            && matches!(f.kind, Kind::Runtime(_))
        {
            f.kind = Kind::Normal;
        }
    }
}

/// Names every function the symbols (the PDB, else the link map) name, and keeps each one's symbol
/// as the build has it. An export keeps its export name; the placeholders (`F_<address>`, `entry`)
/// give way. An import thunk keeps `thunk_<import>`: the PDB names the linker's `jmp [slot]` after
/// the import, which is the name the port calls the import by.
pub fn name_functions(image: &Image, functions: &mut BTreeMap<u64, Function>) {
    for symbol in &image.symbols {
        let Some(f) = functions.get_mut(&symbol.va) else { continue };
        f.symbol = Some(symbol.name.clone());
        f.object = symbol.object.as_ref().map(|o| symbol.library.as_ref().map_or_else(|| o.clone(), |l| format!("{l}:{o}")));
        if !f.is_thunk() && f.name == function::name_for(image, f) && !f.origins.iter().any(|o| matches!(o, crate::Origin::Export(_))) {
            f.name = source_name(symbol, image.is_64());
        }
    }
}

/// The source name of a symbol: a PDB procedure's display name as it is, a linkage name
/// [`undecorate`]d.
fn source_name(symbol: &ra_asm_pe::Symbol, is_64: bool) -> String {
    if symbol.display { symbol.name.clone() } else { undecorate(&symbol.name, is_64) }
}

/// The entry of a Windows program the runtime's `WinMainCRTStartup` starts.
pub const WIN_MAIN: &str = "WinMain";

/// The C name of the program's [`WIN_MAIN`]: `windows.h` declares `WinMain` ([`is_reserved`]).
pub const WIN_MAIN_C: &str = "WinMain_";

/// The function the port starts at: `main`, else `WinMain`, when the image's entry is a C
/// runtime's startup, which the host's own startup replaces, else the image's entry point. A dead
/// one is not it: SDL's `SDL2main.lib` links a `main` beside its `WinMain` that nothing calls.
pub fn program_entry(image: &Image, functions: &BTreeMap<u64, Function>) -> Option<u64> {
    let entry = image.entry_point()?;
    if !functions.get(&entry).is_some_and(|f| matches!(f.kind, Kind::Runtime(_))) {
        return Some(entry);
    }
    let named =
        |name: &str| functions.values().find(|f| f.name == name && matches!(f.kind, Kind::Normal) && !f.is_dead()).map(|f| f.entry);
    named("main").or_else(|| named(WIN_MAIN)).or(Some(entry))
}

/// Makes every name an identifier, unique in the program and free of the names the port's
/// runtime and the host's C library own. A function the host's runtime supplies keeps the host's
/// name: it is what the port calls, never what it defines.
pub fn finish_names(functions: &mut BTreeMap<u64, Function>) {
    let mut taken: HashSet<String> = HashSet::new();
    for f in functions.values_mut() {
        let mut name = identifier(&f.name);
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            name = format!("f_{name}");
        }
        if is_reserved(&name) && !matches!(f.kind, Kind::Runtime(_)) {
            name.push('_');
        }
        if taken.contains(&name) && name != function::default_name(f.entry) {
            name = format!("{name}_{:x}", f.entry);
        }
        taken.insert(name.clone());
        f.name = name;
    }
}

/// The static libraries of the Microsoft C and C++ runtimes, by file name.
pub fn is_runtime_library(library: &str) -> bool {
    let stem = library.strip_suffix(".lib").unwrap_or(library);
    let stem = stem.strip_suffix('d').filter(|s| s.len() + 1 == stem.len() && !s.ends_with("ol")).unwrap_or(stem);
    matches!(stem, "libcmt" | "libucrt" | "libvcruntime" | "libcpmt" | "libcpmt1" | "libconcrt" | "msvcrt" | "msvcprt" | "oldnames")
}

/// The source name of a linkage name: on x86 the `_` of `cdecl`, the `_name@N` of `stdcall` and
/// the `@name@N` of `fastcall` come off; a C++ name (`?area@Rect@@UEBANXZ`) reads as `Rect::area`,
/// with `ctor`, `dtor` and `operator_N` for the special names and the whole mangled name, made
/// an identifier, for a template or anything else beyond nested names.
pub fn undecorate(name: &str, is_64: bool) -> String {
    if let Some(rest) = name.strip_prefix('?') {
        return demangle(rest).unwrap_or_else(|| identifier(name));
    }
    if is_64 {
        return name.to_owned();
    }
    if let Some(rest) = name.strip_prefix('@')
        && let Some((base, suffix)) = rest.rsplit_once('@')
        && suffix.bytes().all(|b| b.is_ascii_digit())
    {
        return base.to_owned();
    }
    let Some(rest) = name.strip_prefix('_') else { return name.to_owned() };
    match rest.rsplit_once('@') {
        Some((base, suffix)) if !base.is_empty() && !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) => base.to_owned(),
        _ => rest.to_owned(),
    }
}

fn demangle(rest: &str) -> Option<String> {
    let (first, scopes) = if let Some(special) = rest.strip_prefix('?') {
        let (code, scopes) = match special.as_bytes().first()? {
            b'_' => (&special[..2], &special[2..]),
            _ => (&special[..1], &special[1..]),
        };
        let name = match code {
            "0" => "ctor",
            "1" => "dtor",
            "2" => "operator_new",
            "3" => "operator_delete",
            "4" => "operator_assign",
            "8" => "operator_eq",
            "9" => "operator_ne",
            "_G" => "scalar_deleting_dtor",
            "_E" => "vector_deleting_dtor",
            "_U" => "operator_new_array",
            "_V" => "operator_delete_array",
            _ => return None,
        };
        (name.to_owned(), scopes)
    } else {
        let (name, scopes) = rest.split_once('@')?;
        (name.to_owned(), scopes)
    };
    // The scopes end at `@@`; a name at the global scope has none and starts straight into them.
    let end = if scopes.starts_with('@') { 0 } else { scopes.find("@@")? };
    let mut parts = vec![first];
    for scope in scopes[..end].split('@').filter(|s| !s.is_empty()) {
        if !is_identifier(scope) {
            return None;
        }
        parts.push(scope.to_owned());
    }
    parts.reverse();
    Some(parts.join("::"))
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The names a lifted function cannot take: the keywords of C, C++ and Rust, the driver's `main`,
/// the lift's own prefixes (the import wrappers, the universal entries), the identifiers C
/// reserves (`__*`), the functions of the host's C library, which the port links against, and
/// the entry points of the host's C runtime, which the linker would take from the port instead,
/// and the ones `windows.h` declares, which the driver includes.
pub fn is_reserved(name: &str) -> bool {
    const HOST_RUNTIME: &[&str] = &[
        "mainCRTStartup",
        "wmainCRTStartup",
        "WinMainCRTStartup",
        "wWinMainCRTStartup",
        "_DllMainCRTStartup",
        "DllMainCRTStartup",
        "WinMain",
        "wWinMain",
        "DllMain",
    ];
    const KEYWORDS: &[&str] = &[
        "auto",
        "break",
        "case",
        "char",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "float",
        "for",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "register",
        "restrict",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "typedef",
        "union",
        "unsigned",
        "void",
        "volatile",
        "while",
        "bool",
        "true",
        "false",
        "class",
        "new",
        "delete",
        "this",
        "template",
        "typename",
        "namespace",
        "using",
        "public",
        "private",
        "protected",
        "virtual",
        "operator",
        "throw",
        "try",
        "catch",
        "friend",
        "explicit",
        "mutable",
        "export",
        "nullptr",
        "static_cast",
        "dynamic_cast",
        "reinterpret_cast",
        "const_cast",
        "typeid",
        "wchar_t",
        "char8_t",
        "char16_t",
        "char32_t",
        "alignas",
        "alignof",
        "asm",
        "decltype",
        "noexcept",
        "constexpr",
        "consteval",
        "constinit",
        "concept",
        "requires",
        "co_await",
        "co_return",
        "co_yield",
        "and",
        "or",
        "not",
        "xor",
        "bitand",
        "bitor",
        "compl",
        "fn",
        "let",
        "mut",
        "impl",
        "trait",
        "type",
        "match",
        "loop",
        "move",
        "ref",
        "self",
        "Self",
        "super",
        "crate",
        "mod",
        "pub",
        "use",
        "where",
        "as",
        "in",
        "dyn",
        "unsafe",
        "async",
        "await",
        "abstract",
        "become",
        "box",
        "final",
        "macro",
        "override",
        "priv",
        "typeof",
        "unsized",
        "yield",
        "main",
        "errno",
        "stdin",
        "stdout",
        "stderr",
        "NULL",
        "EOF",
    ];
    const C_LIBRARY: &[&str] = &[
        "abort",
        "abs",
        "acos",
        "asctime",
        "asin",
        "atan",
        "atan2",
        "atexit",
        "atof",
        "atoi",
        "atol",
        "atoll",
        "bsearch",
        "calloc",
        "ceil",
        "clearerr",
        "clock",
        "cos",
        "cosh",
        "ctime",
        "difftime",
        "div",
        "exit",
        "exp",
        "fabs",
        "fclose",
        "feof",
        "ferror",
        "fflush",
        "fgetc",
        "fgetpos",
        "fgets",
        "floor",
        "fmod",
        "fopen",
        "fprintf",
        "fputc",
        "fputs",
        "fread",
        "free",
        "freopen",
        "frexp",
        "fscanf",
        "fseek",
        "fsetpos",
        "ftell",
        "fwrite",
        "getc",
        "getchar",
        "getenv",
        "gets",
        "gmtime",
        "isalnum",
        "isalpha",
        "iscntrl",
        "isdigit",
        "isgraph",
        "islower",
        "isprint",
        "ispunct",
        "isspace",
        "isupper",
        "isxdigit",
        "labs",
        "ldexp",
        "ldiv",
        "localeconv",
        "localtime",
        "log",
        "log10",
        "longjmp",
        "malloc",
        "mblen",
        "mbstowcs",
        "mbtowc",
        "memchr",
        "memcmp",
        "memcpy",
        "memmove",
        "memset",
        "mktime",
        "modf",
        "perror",
        "pow",
        "printf",
        "putc",
        "putchar",
        "puts",
        "qsort",
        "raise",
        "rand",
        "realloc",
        "remove",
        "rename",
        "rewind",
        "scanf",
        "setbuf",
        "setjmp",
        "setlocale",
        "setvbuf",
        "signal",
        "sin",
        "sinh",
        "snprintf",
        "sprintf",
        "sqrt",
        "srand",
        "sscanf",
        "strcat",
        "strchr",
        "strcmp",
        "strcoll",
        "strcpy",
        "strcspn",
        "strerror",
        "strftime",
        "strlen",
        "strncat",
        "strncmp",
        "strncpy",
        "strpbrk",
        "strrchr",
        "strspn",
        "strstr",
        "strtod",
        "strtok",
        "strtol",
        "strtoul",
        "strxfrm",
        "system",
        "tan",
        "tanh",
        "time",
        "tmpfile",
        "tmpnam",
        "tolower",
        "toupper",
        "ungetc",
        "vfprintf",
        "vprintf",
        "vsprintf",
        "vsnprintf",
        "wcstombs",
        "wctomb",
        "strdup",
        "strtoll",
        "strtoull",
        "llabs",
        "lldiv",
        "fseeko",
        "ftello",
        "getline",
        "exp2",
        "log2",
        "cbrt",
        "hypot",
        "trunc",
        "round",
        "fmin",
        "fmax",
        "fma",
    ];
    name.starts_with("imp_")
        || name.starts_with("U_")
        || name.starts_with("__")
        || KEYWORDS.contains(&name)
        || C_LIBRARY.contains(&name)
        || HOST_RUNTIME.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linkage_names_read_as_source_names() {
        assert_eq!(undecorate("_main", false), "main");
        assert_eq!(undecorate("_WinMain@16", false), "WinMain");
        assert_eq!(undecorate("@fast@8", false), "fast");
        assert_eq!(undecorate("__scrt_common_main_seh", false), "_scrt_common_main_seh");
        assert_eq!(undecorate("main", true), "main");
        assert_eq!(undecorate("__security_check_cookie", true), "__security_check_cookie");
        assert_eq!(undecorate("?area@Rect@@UEBANXZ", true), "Rect::area");
        assert_eq!(undecorate("?run@Inner@Outer@@QEAAXXZ", true), "Outer::Inner::run");
        assert_eq!(undecorate("??0Rect@@QEAA@NN@Z", true), "Rect::ctor");
        assert_eq!(undecorate("??1Rect@@UEAA@XZ", true), "Rect::dtor");
        assert_eq!(undecorate("??_GRect@@UEAAPEAXI@Z", true), "Rect::scalar_deleting_dtor");
        assert_eq!(undecorate("?f@@YAXXZ", true), "f");
        assert_eq!(undecorate("??$f@H@@YAXXZ", true), "f_H_YAXXZ");
    }

    /// The program's `WinMain` is defined under [`WIN_MAIN_C`], since the driver's `windows.h`
    /// declares `WinMain` with a prototype of its own.
    #[test]
    fn the_programs_win_main_takes_a_name_windows_h_leaves_free() {
        assert!(is_reserved(WIN_MAIN));
        assert_eq!(format!("{WIN_MAIN}_"), WIN_MAIN_C);
        assert!(!is_reserved(WIN_MAIN_C));
    }

    #[test]
    fn runtime_libraries_are_known_by_name() {
        assert!(is_runtime_library("libucrt.lib"));
        assert!(is_runtime_library("libcmtd.lib"));
        assert!(is_runtime_library("libvcruntime.lib"));
        assert!(!is_runtime_library("kernel32.lib"));
        assert!(!is_runtime_library("mylib.lib"));
    }

    #[test]
    fn the_x86_helpers_stay_the_programs() {
        for name in ["_alldiv", "_aulldvrm", "_ftol2", "_ftol3", "_ultod3", "_CIpow", "_alloca_probe_16", "__security_check_cookie"] {
            assert!(is_ported_helper(name), "{name}");
        }
        for name in ["printf", "malloc", "_mainCRTStartup", "_tmainCRTStartup", "__security_init_cookie"] {
            assert!(!is_ported_helper(name), "{name}");
        }
    }
}
