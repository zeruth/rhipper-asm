//! The names of the functions, from the symbols of the build and from what the source could have
//! called them: a PDB beside the image names every function it describes and tells which static
//! library each came from; the functions of a C runtime library are the host's to supply.
//!
//! Every function ends with a name that is a C identifier, unique in the program, and not one the
//! port's own runtime or the host's C library owns: `main` becomes `main_`, since the driver has
//! `main`, and a second `helper` becomes `helper_<address>`.

use std::collections::{BTreeMap, HashSet};

use ra_asm_pe::Image;

use crate::function::{self, Function, Kind};
use crate::rtti::identifier;

/// The ranges of the image that the objects of the C runtime libraries fill, from the PDB.
pub struct RuntimeRanges(Vec<(u64, u64)>);

impl RuntimeRanges {
    pub fn of(image: &Image) -> Self {
        let mut ranges: Vec<(u64, u64)> =
            image.libraries.iter().filter(|r| is_runtime_library(&r.library)).map(|r| (r.va, r.va + u64::from(r.size))).collect();
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
/// name. A thunk stays a thunk. A runtime function the host cannot link by that name in C (a C++
/// function, an exception handling funclet such as `common_control87$filt$0`) is the host's all
/// the same, under the name made an identifier: nothing ported calls one, and a call would fail to
/// link by name rather than lift the runtime's code.
pub fn classify(image: &Image, runtime: &RuntimeRanges, f: &mut Function) {
    if f.is_thunk() || !runtime.contains(f.entry) {
        return;
    }
    let name = match image.symbols.binary_search_by_key(&f.entry, |s| s.va) {
        Ok(i) => identifier(&undecorate(&image.symbols[i].name, image.is_64())),
        Err(_) => function::default_name(f.entry),
    };
    f.kind = Kind::Runtime(name);
}

/// Names every function the PDB names. An export keeps its export name; the placeholders
/// (`F_<address>`, `entry`, `thunk_<import>`) give way.
pub fn name_functions(image: &Image, functions: &mut BTreeMap<u64, Function>) {
    for symbol in &image.symbols {
        let Some(f) = functions.get_mut(&symbol.va) else { continue };
        if f.name == function::name_for(image, f) && !f.origins.iter().any(|o| matches!(o, crate::Origin::Export(_))) {
            f.name = undecorate(&symbol.name, image.is_64());
        }
    }
}

/// The function the port starts at: `main` when the image's entry is a C runtime's startup, which
/// the host's own startup replaces, else the image's entry point.
pub fn program_entry(image: &Image, functions: &BTreeMap<u64, Function>) -> Option<u64> {
    let entry = image.entry_point()?;
    if !functions.get(&entry).is_some_and(|f| matches!(f.kind, Kind::Runtime(_))) {
        return Some(entry);
    }
    functions.values().find(|f| f.name == "main" && matches!(f.kind, Kind::Normal)).map(|f| f.entry).or(Some(entry))
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
/// reserves (`__*`) and the functions of the host's C library, which the port links against.
fn is_reserved(name: &str) -> bool {
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
    name.starts_with("imp_") || name.starts_with("U_") || name.starts_with("__") || KEYWORDS.contains(&name) || C_LIBRARY.contains(&name)
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

    #[test]
    fn runtime_libraries_are_known_by_name() {
        assert!(is_runtime_library("libucrt.lib"));
        assert!(is_runtime_library("libcmtd.lib"));
        assert!(is_runtime_library("libvcruntime.lib"));
        assert!(!is_runtime_library("kernel32.lib"));
        assert!(!is_runtime_library("mylib.lib"));
    }
}
