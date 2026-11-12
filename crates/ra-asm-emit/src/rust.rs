//! The Rust backend: the raised IR as Rust, with the same structuring as the C and a `rhipper`
//! runtime module beside it. Every lifted function is an `extern "C" fn`, so a pointer to it in
//! the data calls it as the machine would. What `safety` proves is safe Rust: a local that never
//! escapes is a value or an array indexed by element, an access of a data section at a known
//! place is a checked one, by element when it indexes an array. Any other memory goes through
//! `read_unaligned` and `write_unaligned` in an `unsafe` block that names why, as does a call of
//! an import or through an address; arithmetic wraps as the machine's does; a
//! vector is an array and its operations go lane by lane through the runtime's helpers. Rust has
//! no `goto`: a jump the structuring could not express stops the port, as a trap does.
//!
//! The output is a cargo package: `Cargo.toml`, `src/main.rs` (the modules and `main`),
//! `src/rhipper.rs` (the runtime), `src/data.rs` (the imports and the data, with `ra_init`, which
//! writes the address words the compiler cannot fold), and `src/program.rs`, or
//! `src/program_NNN.rs` when the program is large.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use ra_asm_ir::{BinOp, BlockId, Callee, Cast, Const, FCond, Function, ICond, Inst, Module, Operand, Type, ValueId};

use crate::c::{EmittedFile, EmittedFunction};
use crate::common::{Defs, identifier, identifiers, parenthesized, strip_parens, trap_count};
use crate::safety::{Local, Place, Root, Safety};
use crate::structure::{Case, Cond, Graph, Stmt, structure};

/// The runtime module, written beside the program.
pub const RUNTIME: &str = include_str!("../../../runtime/rhipper.rs");

/// What the backend writes.
#[derive(Debug, Clone)]
pub struct Options {
    /// The source name of each function, by IR name: the comment above it.
    pub names: HashMap<String, String>,
    /// The DLL of each import, by symbol, for the `#[link]` of its declaration.
    pub imports: HashMap<String, String>,
    /// The IR name of the entry function, which `main` calls.
    pub entry: Option<String>,
    /// The name of the cargo package and its binary.
    pub package: String,
    /// The functions one file holds before the output splits.
    pub functions_per_file: usize,
    /// The pointer width of the program, 64 or 32. A 32-bit program keeps its 32-bit pointers,
    /// so its port is linked to run with every address below 4 GB.
    pub pointer_bits: u32,
    /// Whether the image has resources: then `resources.res`, written beside the package, is
    /// linked into the binary, so the program's `FindResource` and `LoadString` find them.
    pub resources: bool,
    /// Whether the proofs of `safety` apply: a local that never escapes as a value, an access of
    /// a section or a local at a known offset as a checked one. Without them every access is
    /// raw, in its own `unsafe` block: the baseline the proofs are measured against.
    pub proofs: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            names: HashMap::new(),
            imports: HashMap::new(),
            entry: None,
            package: "port".into(),
            functions_per_file: crate::c::FUNCTIONS_PER_FILE,
            pointer_bits: 64,
            resources: false,
            proofs: true,
        }
    }
}

/// The files of the package and the functions in them.
pub struct Output {
    pub files: Vec<EmittedFile>,
    pub functions: Vec<EmittedFunction>,
}

const KEYWORDS: [&str; 52] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
    "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
    "unsized", "virtual", "yield", "try", "gen",
];

fn name(f: &Function, v: ValueId) -> String {
    identifier(f, v, &KEYWORDS)
}

/// The Rust type of an IR type. An `i1` is a `bool`; a vector is an array of its lanes.
pub fn rust_type(ty: &Type) -> String {
    match ty {
        Type::Void => "()".into(),
        Type::Int(1) => "bool".into(),
        Type::Int(n @ (8 | 16 | 32 | 64 | 128)) => format!("u{n}"),
        Type::Int(n) if *n < 8 => "u8".into(),
        Type::Int(n) if *n < 16 => "u16".into(),
        Type::Int(n) if *n < 32 => "u32".into(),
        Type::Int(n) if *n < 64 => "u64".into(),
        Type::Int(_) => "u128".into(),
        Type::Half => "u16".into(),
        Type::Float => "f32".into(),
        Type::Double | Type::Fp80 => "f64".into(),
        Type::Ptr | Type::Func(_) => "*mut u8".into(),
        Type::Vector(n, e) => format!("[{}; {n}]", rust_type(e)),
        Type::Array(n, e) => format!("[{}; {n}]", rust_type(e)),
        Type::Struct(_) | Type::Packed(_) | Type::Named(_) => format!("[u8; {}]", ty.size(8).unwrap_or(1)),
    }
}

/// The signed Rust type of an integer type, for the signed operations.
fn signed_type(ty: &Type) -> String {
    match ty {
        Type::Int(_) => rust_type(ty).replacen('u', "i", 1),
        other => rust_type(other),
    }
}

/// The lane type of a vector, or the type itself.
fn lane(ty: &Type) -> Type {
    match ty {
        Type::Vector(_, e) => (**e).clone(),
        other => other.clone(),
    }
}

/// The zero of a type, as every local starts.
fn zero(ty: &Type) -> String {
    match ty {
        Type::Int(1) => "false".into(),
        Type::Int(_) | Type::Half => format!("0{}", rust_type(ty)),
        Type::Float => "0.0f32".into(),
        Type::Double | Type::Fp80 => "0.0f64".into(),
        Type::Ptr | Type::Func(_) => "core::ptr::null_mut()".into(),
        Type::Vector(n, e) => format!("[{}; {n}]", zero(e)),
        Type::Array(n, e) => format!("[{}; {n}]", zero(e)),
        Type::Struct(_) | Type::Packed(_) | Type::Named(_) => format!("[0u8; {}]", ty.size(8).unwrap_or(1)),
        Type::Void => "()".into(),
    }
}

/// Writes `module` as a cargo package.
pub fn emit_rust(module: &Module, options: &Options) -> Output {
    // The names whose address is a function's: the lifted functions and the imports.
    let function_names: HashSet<String> =
        module.functions.iter().map(|f| f.name.clone()).chain(module.declarations.iter().map(|d| d.name.clone())).collect();
    let defined: HashSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    let sections: HashSet<String> = module.globals.iter().filter(|g| is_section(g)).map(|g| g.name.clone()).collect();
    let mut functions: Vec<EmittedFunction> = Vec::new();
    for f in &module.functions {
        let mut text = String::new();
        let names = Names { functions: &function_names, defined: &defined, sections: &sections };
        let (header_lines, copies, gotos) =
            emit_function(&mut text, f, options.names.get(&f.name).map(String::as_str), &names, &module.types, options.proofs);
        let casts = identifiers(&text).iter().filter(|(word, _)| *word == "as").count();
        functions.push(EmittedFunction {
            ir_name: f.name.clone(),
            address: f.address,
            header_lines,
            traps: trap_count(f),
            copies,
            grade: crate::functions::Grade::of(f, gotos, casts, text.lines().count() - header_lines),
            hand: false,
            text,
        });
    }

    let mut files = Vec::new();
    files.push(EmittedFile { name: "Cargo.toml".into(), text: cargo_toml(&options.package), functions: Vec::new() });
    if let Some(text) = build_rs(options) {
        files.push(EmittedFile { name: "build.rs".into(), text, functions: Vec::new() });
    }
    files.push(EmittedFile { name: "src/rhipper.rs".into(), text: RUNTIME.to_owned(), functions: Vec::new() });
    files.push(EmittedFile {
        name: "src/data.rs".into(),
        text: data_module(module, options, &function_names, &sections),
        functions: Vec::new(),
    });

    let per_file = options.functions_per_file.max(1);
    let mut modules: Vec<String> = vec!["data".into()];
    if functions.len() <= per_file {
        let (text, lines) = program_module(&functions);
        files.push(EmittedFile { name: "src/program.rs".into(), text, functions: lines });
        modules.push("program".into());
    } else {
        for (index, chunk) in functions.chunks(per_file).enumerate() {
            let (text, lines) = program_module(chunk);
            let module_name = format!("program_{index:03}");
            files.push(EmittedFile { name: format!("src/{module_name}.rs"), text, functions: lines });
            modules.push(module_name);
        }
    }
    files.insert(
        1,
        EmittedFile { name: "src/main.rs".into(), text: main_module(module, options, &modules, &sections), functions: Vec::new() },
    );
    Output { files, functions }
}

/// Whether `g` is written as a `rhipper::Section`: the bytes of a data section or a table of the
/// runtime, which the program reads at any offset as any type. A scalar global of the runtime
/// (`ra_x87_top`) stays a `static mut` the runtime names.
fn is_section(g: &ra_asm_ir::Global) -> bool {
    if g.name == "ra_functions" || g.name == "ra_function_count" {
        return false;
    }
    match (&g.ty, &g.init) {
        (Type::Array(_, e), Some(Const::Bytes(_) | Const::Zero)) => matches!(**e, Type::I8 | Type::Double),
        (Type::Packed(_), Some(Const::Struct(_))) => true,
        _ => false,
    }
}

/// `global` plus `offset` as a raw `*mut u8`: a section's own pointer, or the address of a
/// `static mut`.
fn global_pointer(global: &str, offset: i64, sections: &HashSet<String>) -> String {
    let base = if sections.contains(global) { format!("{global}.ptr()") } else { format!("((&raw mut {global}) as *mut u8)") };
    if offset == 0 { base } else { format!("{base}.wrapping_offset({offset})") }
}

/// The names of the module an emitter consults: every function and import, the lifted
/// functions among them, and the globals written as sections.
#[derive(Clone, Copy)]
struct Names<'a> {
    functions: &'a HashSet<String>,
    defined: &'a HashSet<String>,
    sections: &'a HashSet<String>,
}

/// The runtime functions that are `unsafe fn`, from the runtime's own text: a call to one of
/// them needs an `unsafe` block, a call to the others none.
fn runtime_is_unsafe(name: &str) -> bool {
    static UNSAFE: std::sync::OnceLock<HashSet<&'static str>> = std::sync::OnceLock::new();
    UNSAFE
        .get_or_init(|| {
            RUNTIME
                .lines()
                .filter_map(|l| l.strip_prefix("pub unsafe fn "))
                .map(|rest| rest.split(['(', '<']).next().unwrap_or(""))
                .collect()
        })
        .contains(name)
}

/// The build script of a 32-bit program's port: the program keeps its 32-bit pointers, so the
/// binary is linked to run with every address of its process below 4 GB.
/// The build script, when the link needs more than cargo's: the 32-bit address limit, the
/// program's resources.
fn build_rs(options: &Options) -> Option<String> {
    let mut text = String::from("// Generated by ras emit.\n");
    let mut body = String::new();
    if options.pointer_bits == 32 {
        text.push_str(
            "// The program is 32-bit: its pointers stay 32-bit, so every address of the process must fit\n\
             // in them, which the linker arranges for a binary that is not large-address-aware.\n",
        );
        body.push_str("    println!(\"cargo:rustc-link-arg-bins=/LARGEADDRESSAWARE:NO\");\n");
    }
    if options.resources {
        text.push_str(
            "// The program's resources (resources.res beside this file) are linked into the binary, so\n\
             // FindResource and LoadString find them as they did in the image.\n",
        );
        body.push_str(
            "    println!(\"cargo:rustc-link-arg-bins={}/resources.res\", std::env::var(\"CARGO_MANIFEST_DIR\").unwrap());\n    \
             println!(\"cargo:rerun-if-changed=resources.res\");\n",
        );
    }
    if body.is_empty() {
        return None;
    }
    let _ = write!(text, "fn main() {{\n{body}}}\n");
    Some(text)
}

fn cargo_toml(package: &str) -> String {
    format!(
        "# Generated by ras emit. The port of {package}: the translation under src/, with its runtime module.\n\
         [package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
         [[bin]]\nname = \"{package}\"\npath = \"src/main.rs\"\n\n\
         # The package is its own workspace, wherever the project lives.\n[workspace]\n\n\
         [profile.release]\ndebug = true\n"
    )
}

fn main_module(module: &Module, options: &Options, modules: &[String], sections: &HashSet<String>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// Generated by ras emit from {}.", module.name);
    // An import is declared as the program calls it, in machine words, and the runtime declares the
    // few it calls itself by their signature: the two declarations of one function differ.
    let _ = writeln!(
        out,
        "#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types, unused_mut, unused_variables, unused_assignments, \
         unused_unsafe, unused_parens, unused_imports, unreachable_code, dead_code, unsafe_op_in_unsafe_fn, \
         clashing_extern_declarations, clippy::all)]"
    );
    let _ = writeln!(out, "mod rhipper;");
    for m in modules {
        let _ = writeln!(out, "mod {m};");
    }
    for m in modules {
        let _ = writeln!(out, "pub use {m}::*;");
    }
    out.push('\n');
    let _ = writeln!(out, "fn main() {{");
    let _ = writeln!(out, "    unsafe {{ // the runtime's start, which writes the data's address words and tables");
    let _ = writeln!(out, "        ra_init();");
    if module.globals.iter().any(|g| g.name == "ra_tls") {
        let tls = global_pointer("ra_tls", 0, sections);
        let _ = writeln!(out, "        rhipper::ra_init_tls({tls} as *const u64, {});", options.pointer_bits);
    }
    if module.globals.iter().any(|g| g.name == "ra_import_pops") {
        let pops = global_pointer("ra_import_pops", 0, sections);
        let _ = writeln!(out, "        rhipper::ra_init_import_pops({pops} as *const u64, &raw mut ra_pops);");
    }
    if let Some(entry) = options.entry.as_ref().and_then(|e| module.functions.iter().find(|f| f.name == *e)) {
        let args: Vec<String> = entry.ty.params.iter().map(zero).collect();
        let call = format!("{}({})", entry.name, args.join(", "));
        // A program that throws may leave an exception nothing caught.
        let uncaught =
            if module.declarations.iter().any(|d| d.name == "ra_eh_pending") { "        rhipper::ra_eh_check_uncaught();\n" } else { "" };
        if entry.attrs.iter().any(|a| a == "noreturn") {
            let _ = writeln!(out, "        {call};");
        } else if entry.ty.ret == Type::Void {
            let _ = writeln!(out, "        {call};");
            let _ = write!(out, "{uncaught}");
            let _ = writeln!(out, "        std::process::exit(0);");
        } else {
            let _ = writeln!(out, "        let code = {call};");
            let _ = write!(out, "{uncaught}");
            let _ = writeln!(out, "        std::process::exit(code as i32);");
        }
    } else {
        let _ = writeln!(out, "        eprintln!(\"ra: the program has no entry function\");");
        let _ = writeln!(out, "        std::process::exit(1);");
    }
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    out
}

/// The imports, grouped by DLL, and the data globals with `ra_init`.
fn data_module(module: &Module, options: &Options, function_names: &HashSet<String>, sections: &HashSet<String>) -> String {
    let mut out = String::from("use super::*;\n\n");
    // The imports, by DLL.
    let defined: HashSet<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    // A host function that is no import of the image (a C runtime linked into it, which the host
    // supplies from its own libraries) names no library: the linker finds it in the default ones,
    // the static functions of `vcruntime` (`__report_rangecheckfailure`) among them.
    let mut by_dll: BTreeMap<Option<String>, Vec<&ra_asm_ir::Declaration>> = BTreeMap::new();
    for d in &module.declarations {
        if d.name.starts_with("llvm.") || d.name.starts_with("ra_") || defined.contains(d.name.as_str()) {
            continue;
        }
        by_dll.entry(options.imports.get(&d.name).cloned()).or_default().push(d);
    }
    for (dll, declarations) in &by_dll {
        if let Some(dll) = dll {
            let _ = writeln!(out, "#[link(name = \"{}\")]", import_library(dll));
        }
        let _ = writeln!(out, "unsafe extern \"system\" {{");
        for d in declarations {
            let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, t)| format!("a{i}: {}", rust_type(t))).collect();
            let ret = if d.ty.ret == Type::Void { String::new() } else { format!(" -> {}", rust_type(&d.ty.ret)) };
            let _ = writeln!(out, "    pub fn {}({}){ret};", d.name, params.join(", "));
        }
        let _ = writeln!(out, "}}\n");
    }

    // An import by ordinal has no symbol to link: a function pointer `ra_init` resolves from the
    // DLL's export table.
    let mut init = String::new();
    for d in &module.declarations {
        let Some((dll, ordinal)) = crate::common::ordinal_import(&d.name) else { continue };
        let params: Vec<String> = d.ty.params.iter().map(rust_type).collect();
        let ret = if d.ty.ret == Type::Void { String::new() } else { format!(" -> {}", rust_type(&d.ty.ret)) };
        let _ =
            writeln!(out, "pub static mut {}: unsafe extern \"system\" fn({}){ret} = rhipper::ra_unresolved;", d.name, params.join(", "));
        let _ = writeln!(init, "    {} = core::mem::transmute(rhipper::ra_resolve_ordinal(\"{dll}\", {ordinal}));", d.name);
    }

    // The data.
    for g in &module.globals {
        if g.name == "ra_functions" || g.name == "ra_function_count" {
            continue;
        }
        match (&g.ty, &g.init) {
            (Type::Array(n, e), Some(Const::Bytes(bytes))) if **e == Type::I8 => {
                let _ = write!(out, "pub static {}: rhipper::Section<[u8; {n}]> = rhipper::Section::new([", g.name);
                write_bytes(&mut out, bytes);
                let _ = writeln!(out, "]);");
            }
            (Type::Array(n, e), Some(Const::Zero)) if **e == Type::I8 => {
                let _ = writeln!(out, "pub static {}: rhipper::Section<[u8; {n}]> = rhipper::Section::new([0u8; {n}]);", g.name);
            }
            (Type::Packed(fields), Some(Const::Struct(items))) => {
                // A packed structure of byte runs and address words. An address word is a u64
                // the compiler cannot fold, so ra_init writes it at startup.
                let _ = write!(out, "#[repr(C, packed)]\npub struct {}_t {{ ", g.name);
                for (i, field) in fields.iter().enumerate() {
                    match field {
                        Type::Array(n, _) => {
                            let _ = write!(out, "pub f{i}: [u8; {n}], ");
                        }
                        other => {
                            let _ = write!(out, "pub f{i}: {}, ", rust_type(other));
                        }
                    }
                }
                let _ = writeln!(out, "}}");
                let _ = write!(out, "pub static {}: rhipper::Section<{}_t> = rhipper::Section::new({}_t {{ ", g.name, g.name, g.name);
                let mut offset = 0u64;
                for (i, (item, field)) in items.iter().zip(fields).enumerate() {
                    match item {
                        Const::Bytes(bytes) => {
                            let _ = write!(out, "f{i}: [");
                            write_bytes(&mut out, bytes);
                            out.push_str("], ");
                        }
                        Const::Int(v) => {
                            let _ = write!(out, "f{i}: {}, ", *v as u64);
                        }
                        Const::Address { .. } | Const::Global(_) => {
                            let _ = write!(out, "f{i}: 0, ");
                            let value = match item {
                                Const::Address { global, offset: o } => address_text(global, *o, function_names, sections),
                                Const::Global(target) => address_text(target, 0, function_names, sections),
                                _ => unreachable!(),
                            };
                            // The word's own width: a 32-bit program's words are dwords.
                            let word = rust_type(field);
                            let _ = writeln!(init, "    {}.set::<{word}>({offset}, ({value}) as {word});", g.name);
                        }
                        other => {
                            let _ = write!(out, "f{i}: {} /* {other:?} */, ", zero(field));
                        }
                    }
                    offset += field.size(8).unwrap_or(0);
                }
                let _ = writeln!(out, "}});");
            }
            (Type::Int(bits), Some(Const::Int(v))) => {
                let _ = writeln!(out, "pub static mut {}: u{bits} = {};", g.name, *v as u64);
            }
            (Type::Array(n, e), Some(Const::Zero)) if **e == Type::Double => {
                let _ = writeln!(out, "pub static {}: rhipper::Section<[f64; {n}]> = rhipper::Section::new([0.0f64; {n}]);", g.name);
            }
            (ty, init) => {
                let _ = writeln!(out, "// global {} of type {ty} with initializer {init:?} is not written", g.name);
            }
        }
    }
    let _ = writeln!(out, "\n/// Writes the address words of the data: the addresses of functions and data, which only exist at run time.");
    let _ = writeln!(out, "pub unsafe fn ra_init() {{");
    out.push_str(&init);
    let _ = writeln!(out, "}}");
    out
}

/// The import library that resolves the functions of `dll`: the DLL's own name for the system
/// DLLs, `ucrt` for the C runtime's API sets and `vcruntime` for the VC runtime, whose DLL names
/// are not library names.
fn import_library(dll: &str) -> String {
    let name = dll.trim_end_matches(".dll").trim_end_matches(".DLL");
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("api-ms-win-crt") || lower == "ucrtbase" {
        "ucrt".to_owned()
    } else if lower.starts_with("vcruntime") {
        "vcruntime".to_owned()
    } else {
        name.to_owned()
    }
}

/// The address of `global` plus `offset`, as a `u64` expression at run time.
fn address_text(global: &str, offset: i64, function_names: &HashSet<String>, sections: &HashSet<String>) -> String {
    if function_names.contains(global) {
        // A runtime function (an import slot that names a shim) lives in the runtime module.
        let path = if global.starts_with("ra_") { format!("rhipper::{global}") } else { global.to_owned() };
        format!("({path} as *const () as usize as u64).wrapping_add({offset} as u64)")
    } else {
        format!("({} as u64)", global_pointer(global, offset, sections))
    }
}

fn write_bytes(out: &mut String, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if i % 32 == 0 {
            out.push_str("\n    ");
        }
        let _ = write!(out, "{b},");
    }
    out.push('\n');
}

/// A module of functions: `use super::*;` brings the data, the imports, the runtime and the
/// other modules' functions in.
fn program_module(functions: &[EmittedFunction]) -> (String, Vec<(String, usize)>) {
    let mut text = String::from("use super::*;\n\n");
    let mut line = text.lines().count() + 1;
    let mut lines = Vec::with_capacity(functions.len());
    for f in functions {
        lines.push((f.ir_name.clone(), line + f.header_lines));
        text.push_str(&f.text);
        text.push('\n');
        line += f.text.lines().count() + 1;
    }
    (text, lines)
}

fn signature(f: &Function) -> String {
    let params: Vec<String> = f.ty.params.iter().zip(&f.params).map(|(t, v)| format!("{}: {}", name(f, *v), rust_type(t))).collect();
    let ret = if f.attrs.iter().any(|a| a == "noreturn") {
        " -> !".to_owned()
    } else if f.ty.ret == Type::Void {
        String::new()
    } else {
        format!(" -> {}", rust_type(&f.ty.ret))
    };
    format!("pub extern \"C\" fn {}({}){ret}", f.name, params.join(", "))
}

/// A function, with one comment line above it naming it. Returns the number of lines above the
/// definition, the number of blocks written twice and the number of jumps left.
fn emit_function(
    out: &mut String,
    f: &Function,
    source_name: Option<&str>,
    names: &Names<'_>,
    types: &[(String, Vec<Type>)],
    proofs: bool,
) -> (usize, usize, usize) {
    let mut header_lines = 0;
    if let Some(address) = f.address {
        match source_name {
            Some(n) if n != f.name => {
                let _ = writeln!(out, "// {n} at {address:#x}");
            }
            _ => {
                let _ = writeln!(out, "// {address:#x}");
            }
        }
        header_lines = 1;
    }
    let _ = writeln!(out, "{} {{", signature(f));
    let graph = Graph::of(f);
    let structured = structure(f, &graph);
    let safety = if proofs { Safety::analyze(f, names.sections, types) } else { Safety::none() };
    let mut e = Emitter { f, defs: Defs::new(f), labels: &structured.labels, names: *names, types, safety, reasons: RefCell::default() };
    e.declare_locals(out);
    e.stmts(out, &structured.body, 1);
    let _ = writeln!(out, "}}");
    (header_lines, structured.copies(), structured.gotos())
}

struct Emitter<'a> {
    f: &'a Function,
    defs: Defs<'a>,
    labels: &'a HashSet<BlockId>,
    names: Names<'a>,
    /// The named struct types of the module: a field pointer is its byte offset here.
    types: &'a [(String, Vec<Type>)],
    /// What is proven safe in the function.
    safety: Safety,
    /// Why the statement being written needs an `unsafe` block, gathered as its expressions
    /// are written and taken when the line is.
    reasons: RefCell<BTreeSet<&'static str>>,
}

/// The reasons an operation stays raw, as the `unsafe` block's comment names them.
const RAW_MEMORY: &str = "memory at a computed address";
const IMPORT: &str = "an import";
const INDIRECT_CALL: &str = "a call through an address";
const RUNTIME_CALL: &str = "the runtime";
const BIT_CAST: &str = "a bit cast";

impl Emitter<'_> {
    fn needs(&self, why: &'static str) {
        self.reasons.borrow_mut().insert(why);
    }

    /// The reasons gathered since the last line, joined.
    fn take(&self) -> Option<String> {
        let mut reasons = self.reasons.borrow_mut();
        if reasons.is_empty() {
            return None;
        }
        let why = reasons.iter().copied().collect::<Vec<_>>().join(", ");
        reasons.clear();
        Some(why)
    }

    /// `text` as an expression, in an `unsafe` block when what it does needs one.
    fn guarded(&self, text: &str) -> String {
        match self.take() {
            Some(why) => format!("unsafe {{ /* {why} */ {text} }}"),
            None => text.to_owned(),
        }
    }

    /// `text` as a statement line, in an `unsafe` block that names why when it needs one.
    fn line(&self, out: &mut String, pad: &str, text: &str) {
        let _ = match self.take() {
            Some(why) => writeln!(out, "{pad}unsafe {{ {text} }} // {why}"),
            None => writeln!(out, "{pad}{text}"),
        };
    }

    fn declare_locals(&mut self, out: &mut String) {
        let mut declared: HashSet<ValueId> = self.f.params.iter().copied().collect();
        for b in &self.f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                if self.defs.inlinable(v) || !declared.insert(v) {
                    continue;
                }
                let n = name(self.f, v);
                match &i.inst {
                    Inst::Alloca { ty, .. } => match self.safety.locals.get(&v) {
                        Some(Local::Value(ty)) => {
                            let _ = writeln!(out, "    let mut {n}: {} = {};", rust_type(ty), zero(ty));
                        }
                        Some(Local::Array(elem, count)) => {
                            let _ = writeln!(out, "    let mut {n}: [{}; {count}] = [{}; {count}];", rust_type(elem), zero(elem));
                        }
                        Some(Local::Bytes(size)) => {
                            let _ = writeln!(out, "    let {n} = rhipper::Section::new([0u8; {size}]);");
                        }
                        None => {
                            let size = ty.size(8).unwrap_or(8);
                            let _ = writeln!(out, "    let mut {n}_mem = rhipper::Aligned([0u8; {size}]);");
                            let _ = writeln!(out, "    let {n}: *mut u8 = {n}_mem.0.as_mut_ptr();");
                        }
                    },
                    _ if self.safety.folded.contains(&v) => {}
                    Inst::Phi { ty, .. } => {
                        let _ = writeln!(out, "    let mut {n}: {} = {};", rust_type(ty), zero(ty));
                        let _ = writeln!(out, "    let mut {n}_next: {} = {};", rust_type(ty), zero(ty));
                    }
                    inst => {
                        let ty = inst.result_type();
                        let _ = writeln!(out, "    let mut {n}: {} = {};", rust_type(&ty), zero(&ty));
                    }
                }
            }
        }
    }

    fn int_constant(ty: &Type, v: i128) -> String {
        match ty {
            Type::Int(1) => (if v & 1 == 1 { "true" } else { "false" }).into(),
            Type::Int(n) => {
                let bits = (*n).min(128);
                let masked = if bits >= 128 { v as u128 } else { (v as u128) & ((1u128 << bits) - 1) };
                format!("{masked}{}", rust_type(ty))
            }
            Type::Ptr | Type::Func(_) => format!("({}u64 as *mut u8)", v as u64),
            Type::Half => format!("{}u16", v as u16),
            other => format!("{v}{}", rust_type(other)),
        }
    }

    fn operand(&self, ty: &Type, o: &Operand) -> String {
        match o {
            Operand::Local(v) => {
                if self.defs.inlinable(*v)
                    && let Some((_, _, inst)) = self.defs.def(*v)
                {
                    return self.expr(inst);
                }
                name(self.f, *v)
            }
            Operand::Int(v) => match ty {
                Type::Vector(n, e) => format!("[{}; {n}]", Self::int_constant(e, *v)),
                other => Self::int_constant(other, *v),
            },
            Operand::Float(v) => match ty {
                Type::Float => format!("f32::from_bits({:#x})", (*v as f32).to_bits()),
                Type::Vector(n, e) if **e == Type::Float => format!("[f32::from_bits({:#x}); {n}]", (*v as f32).to_bits()),
                Type::Vector(n, _) => format!("[f64::from_bits({:#x}); {n}]", v.to_bits()),
                _ => format!("f64::from_bits({:#x})", v.to_bits()),
            },
            Operand::Global(g) => self.address(g, 0, ty),
            Operand::Address { global, offset } => self.address(global, *offset, ty),
            Operand::Null => "core::ptr::null_mut()".into(),
            Operand::Undef | Operand::Zero => zero(ty),
        }
    }

    /// The address of a global or a function, as a pointer or as the integer `ty`.
    fn address(&self, global: &str, offset: i64, ty: &Type) -> String {
        let pointer = if self.names.functions.contains(global) {
            format!("({global} as usize as u64).wrapping_add({offset} as u64) as *mut u8")
        } else {
            global_pointer(global, offset, self.names.sections)
        };
        match ty {
            Type::Ptr | Type::Func(_) => format!("({pointer})"),
            other => format!("(({pointer}) as {})", rust_type(other)),
        }
    }

    /// The byte offset of a place, as a `usize` expression when it has terms.
    fn place_offset(&self, p: &Place) -> String {
        if p.terms.is_empty() {
            return p.offset.to_string();
        }
        let mut s = format!("({}i64)", p.offset);
        for (ty, index, scale) in &p.terms {
            let term = format!("(({}) as {}) as i64", self.operand(ty, index), signed_type(ty));
            s = if *scale == 1 { format!("{s}.wrapping_add({term})") } else { format!("{s}.wrapping_add(({term}).wrapping_mul({scale}))") };
        }
        format!("({s}) as usize")
    }

    /// What a proven access of a local reads and writes: the local's value or its element as a
    /// place expression, or `None` for a local of bytes.
    fn local_place(&self, a: ValueId, p: &Place) -> Option<String> {
        match self.safety.locals.get(&a)? {
            Local::Value(_) => Some(name(self.f, a)),
            Local::Array(elem, _) => {
                let index = u64::try_from(p.offset).ok()? / elem.size(8)?;
                Some(format!("{}[{index}]", name(self.f, a)))
            }
            Local::Bytes(_) => None,
        }
    }

    /// The section access of an indexed place: one term scaled by the access's own size is an
    /// element of an array, `at::<T>(base, index)`; `None` for any other place.
    fn element_args(&self, ty: &Type, p: &Place) -> Option<String> {
        let [(index_ty, index, scale)] = p.terms.as_slice() else { return None };
        if Some(*scale) != ty.size(8).map(i128::from) || p.offset < 0 {
            return None;
        }
        Some(format!("{}, ({}) as {} as isize", p.offset, self.operand(index_ty, index), signed_type(index_ty)))
    }

    /// A proven load: the local's value or element, or a checked read of the local's bytes or
    /// the section, by element when the place indexes an array.
    fn load_text(&self, ty: &Type, p: &Place) -> String {
        match &p.root {
            Root::Local(a) => {
                self.local_place(*a, p).unwrap_or_else(|| format!("{}.get::<{}>({})", name(self.f, *a), rust_type(ty), p.offset))
            }
            Root::Section(g) => match self.element_args(ty, p) {
                Some(args) => format!("{g}.at::<{}>({args})", rust_type(ty)),
                None => format!("{g}.get::<{}>({})", rust_type(ty), self.place_offset(p)),
            },
        }
    }

    /// A proven store, as a statement.
    fn store_text(&self, ty: &Type, p: &Place, value: &str) -> String {
        match &p.root {
            Root::Local(a) => match self.local_place(*a, p) {
                Some(place) => format!("{place} = {value};"),
                None => format!("{}.set::<{}>({}, {value});", name(self.f, *a), rust_type(ty), p.offset),
            },
            Root::Section(g) => match self.element_args(ty, p) {
                Some(args) => format!("{g}.put::<{}>({args}, {value});", rust_type(ty)),
                None => format!("{g}.set::<{}>({}, {value});", rust_type(ty), self.place_offset(p)),
            },
        }
    }

    /// The expression of an instruction.
    fn expr(&self, inst: &Inst) -> String {
        match inst {
            Inst::Alloca { .. } => "core::ptr::null_mut()".into(),
            Inst::Load { ty, ptr, .. } => {
                if let Some(p) = self.safety.access(inst) {
                    return self.load_text(ty, p);
                }
                self.needs(RAW_MEMORY);
                format!("core::ptr::read_unaligned({} as *const {})", self.operand(&Type::Ptr, ptr), rust_type(ty))
            }
            Inst::Store { .. } | Inst::Phi { .. } => String::new(),
            Inst::Bin { op, ty, lhs, rhs } => self.bin(*op, ty, lhs, rhs),
            Inst::ICmp { cond, ty, lhs, rhs } => {
                if let Type::Vector(_, e) = ty {
                    let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
                    return format!("rhipper::vcmp({a}, {b}, true, false, |x, y| {})", Self::icmp_text(*cond, e, "x", "y"));
                }
                let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
                format!("({})", Self::icmp_text(*cond, ty, &a, &b))
            }
            Inst::FCmp { cond, ty, lhs, rhs } => {
                if let Type::Vector(_, _) = ty {
                    let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
                    return format!("rhipper::vcmp({a}, {b}, true, false, |x, y| {})", Self::fcmp_text(*cond, "x", "y"));
                }
                let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
                format!("({})", Self::fcmp_text(*cond, &a, &b))
            }
            Inst::Select { cond, ty, then, els } => {
                format!(
                    "(if {} {{ {} }} else {{ {} }})",
                    strip_parens(&self.operand(&Type::I1, cond)),
                    self.operand(ty, then),
                    self.operand(ty, els)
                )
            }
            Inst::Cast { cast, from, value, to } => self.cast(*cast, from, value, to),
            Inst::Gep { base_ty, ptr, indices, .. } => {
                if let Some(offset) = crate::common::field_offset(self.types, base_ty, indices) {
                    return format!("({}.wrapping_offset({offset}))", self.operand(&Type::Ptr, ptr));
                }
                let mut constant = 0i128;
                let mut dynamic: Vec<String> = Vec::new();
                let size = base_ty.size(8).unwrap_or(1) as i128;
                for (k, (ty, i)) in indices.iter().enumerate() {
                    let scale = if k == 0 {
                        size
                    } else {
                        match base_ty {
                            Type::Array(_, e) => e.size(8).unwrap_or(1) as i128,
                            _ => 1,
                        }
                    };
                    match i {
                        Operand::Int(v) => constant += scale * v,
                        other => dynamic.push(format!("(({}) as i64).wrapping_mul({scale}) as isize", self.operand(ty, other))),
                    }
                }
                let mut s = self.operand(&Type::Ptr, ptr);
                if constant != 0 {
                    s = format!("{s}.wrapping_offset({constant})");
                }
                for d in dynamic {
                    s = format!("{s}.wrapping_offset({d})");
                }
                format!("({s})")
            }
            Inst::ExtractElement { ty, vector, index } => {
                format!("({}[({}) as usize])", self.operand(ty, vector), self.operand(&Type::I32, index))
            }
            Inst::InsertElement { ty, vector, element, index } => {
                let e = lane(ty);
                format!(
                    "rhipper::insert({}, ({}) as usize, {})",
                    self.operand(ty, vector),
                    self.operand(&Type::I32, index),
                    self.operand(&e, element)
                )
            }
            Inst::ShuffleVector { ty, a, b, mask } => {
                let m: Vec<String> = mask.iter().map(i32::to_string).collect();
                format!("rhipper::shuffle({}, {}, [{}])", self.operand(ty, a), self.operand(ty, b), m.join(", "))
            }
            Inst::Call { fn_ty, callee, args, .. } => {
                let a: Vec<String> = args.iter().map(|(t, o)| strip_parens(&self.operand(t, o))).collect();
                match callee {
                    Callee::Direct(n) if n.starts_with("llvm.") => Self::intrinsic(n, fn_ty, &a),
                    // the runtime's own functions live in the rhipper module
                    Callee::Direct(n) if n.starts_with("ra_") && !self.names.defined.contains(n) => {
                        if runtime_is_unsafe(n) {
                            self.needs(RUNTIME_CALL);
                        }
                        format!("rhipper::{n}({})", a.join(", "))
                    }
                    Callee::Direct(n) => {
                        if !self.names.defined.contains(n) {
                            self.needs(IMPORT);
                        }
                        format!("{n}({})", a.join(", "))
                    }
                    Callee::Indirect(p) => {
                        self.needs(INDIRECT_CALL);
                        let params: Vec<String> = fn_ty.params.iter().map(rust_type).collect();
                        let ret = if fn_ty.ret == Type::Void { String::new() } else { format!(" -> {}", rust_type(&fn_ty.ret)) };
                        format!(
                            "(core::mem::transmute::<usize, unsafe extern \"C\" fn({}){ret}>(({}) as usize))({})",
                            params.join(", "),
                            self.operand(&Type::Ptr, p),
                            a.join(", ")
                        )
                    }
                }
            }
            Inst::Freeze { ty, value } => self.operand(ty, value),
        }
    }

    fn bin(&self, op: BinOp, ty: &Type, lhs: &Operand, rhs: &Operand) -> String {
        if let Type::Vector(_, e) = ty {
            let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
            return format!("rhipper::vbin({a}, {b}, |x, y| {})", Self::bin_text(op, e, "x", "y"));
        }
        let a = self.operand(ty, lhs);
        // `x ^ true` on a condition is `!x`.
        if *ty == Type::I1 && op == BinOp::Xor && *rhs == Operand::Int(1) {
            return format!("!{}", parenthesized(&a));
        }
        let b = self.operand(ty, rhs);
        format!("({})", Self::bin_text(op, ty, &a, &b))
    }

    /// `a op b` for scalars of `ty`.
    fn bin_text(op: BinOp, ty: &Type, a: &str, b: &str) -> String {
        let s = signed_type(ty);
        let u = rust_type(ty);
        let (a, b) = (parenthesized(a), parenthesized(b));
        match op {
            BinOp::Add if *ty == Type::I1 => format!("{a} ^ {b}"),
            BinOp::Add => format!("{a}.wrapping_add({b})"),
            BinOp::Sub => format!("{a}.wrapping_sub({b})"),
            BinOp::Mul => format!("{a}.wrapping_mul({b})"),
            BinOp::UDiv | BinOp::FDiv => format!("{a} / {b}"),
            BinOp::SDiv => format!("(({a} as {s}).wrapping_div({b} as {s}) as {u})"),
            BinOp::URem | BinOp::FRem => format!("{a} % {b}"),
            BinOp::SRem => format!("(({a} as {s}).wrapping_rem({b} as {s}) as {u})"),
            BinOp::Shl => format!("{a}.wrapping_shl({b} as u32)"),
            BinOp::LShr => format!("{a}.wrapping_shr({b} as u32)"),
            BinOp::AShr => format!("(({a} as {s}).wrapping_shr({b} as u32) as {u})"),
            BinOp::And => format!("{a} & {b}"),
            BinOp::Or => format!("{a} | {b}"),
            BinOp::Xor => format!("{a} ^ {b}"),
            BinOp::FAdd => format!("{a} + {b}"),
            BinOp::FSub => format!("{a} - {b}"),
            BinOp::FMul => format!("{a} * {b}"),
        }
    }

    fn icmp_text(cond: ICond, ty: &Type, a: &str, b: &str) -> String {
        let s = signed_type(ty);
        let (a, b) = (parenthesized(a), parenthesized(b));
        let signed = |x: &str| if *ty == Type::I1 { format!("({x} as i8)") } else { format!("({x} as {s})") };
        match cond {
            ICond::Eq => format!("{a} == {b}"),
            ICond::Ne => format!("{a} != {b}"),
            ICond::Ugt => format!("{a} > {b}"),
            ICond::Uge => format!("{a} >= {b}"),
            ICond::Ult => format!("{a} < {b}"),
            ICond::Ule => format!("{a} <= {b}"),
            ICond::Sgt => format!("{} > {}", signed(&a), signed(&b)),
            ICond::Sge => format!("{} >= {}", signed(&a), signed(&b)),
            ICond::Slt => format!("{} < {}", signed(&a), signed(&b)),
            ICond::Sle => format!("{} <= {}", signed(&a), signed(&b)),
        }
    }

    fn fcmp_text(cond: FCond, a: &str, b: &str) -> String {
        let (a, b) = (parenthesized(a), parenthesized(b));
        match cond {
            FCond::Oeq => format!("{a} == {b}"),
            FCond::Ogt => format!("{a} > {b}"),
            FCond::Oge => format!("{a} >= {b}"),
            FCond::Olt => format!("{a} < {b}"),
            FCond::Ole => format!("{a} <= {b}"),
            FCond::One => format!("{a} < {b} || {a} > {b}"),
            FCond::Ord => format!("!{a}.is_nan() && !{b}.is_nan()"),
            FCond::Ueq => format!("!({a} < {b} || {a} > {b})"),
            FCond::Ugt => format!("!({a} <= {b})"),
            FCond::Uge => format!("!({a} < {b})"),
            FCond::Ult => format!("!({a} >= {b})"),
            FCond::Ule => format!("!({a} > {b})"),
            FCond::Une => format!("{a} != {b}"),
            FCond::Uno => format!("{a}.is_nan() || {b}.is_nan()"),
        }
    }

    fn cast(&self, cast: Cast, from: &Type, value: &Operand, to: &Type) -> String {
        // The address of a global as a pointer needs no trip through an integer.
        if cast == Cast::IntToPtr
            && let Operand::Address { global, offset } = value
        {
            return self.address(global, *offset, &Type::Ptr);
        }
        let v = self.operand(from, value);
        // Lane by lane for a vector, except a bit cast, which keeps the bytes.
        if let (Type::Vector(_, fe), Type::Vector(_, te)) = (from, to)
            && cast != Cast::Bitcast
        {
            return format!("rhipper::vcast({v}, |x| {})", Self::cast_text(cast, fe, "x", te));
        }
        match (cast, from, to) {
            (Cast::Bitcast, Type::Vector(_, fe), Type::Vector(_, te)) if **fe == Type::I1 => {
                format!("rhipper::vcast({v}, |x| if x {{ !0{} }} else {{ 0{} }})", rust_type(te), rust_type(te))
            }
            (Cast::Bitcast, Type::Vector(_, fe), Type::Vector(_, te)) if **te == Type::I1 => {
                format!("rhipper::vcast({v}, |x| x != 0{})", rust_type(fe))
            }
            // The bits of a float and back are safe conversions.
            (Cast::Bitcast, Type::Int(64), Type::Double) | (Cast::Bitcast, Type::Int(32), Type::Float) => {
                format!("{}::from_bits({v})", rust_type(to))
            }
            (Cast::Bitcast, Type::Double, Type::Int(64)) | (Cast::Bitcast, Type::Float, Type::Int(32)) => {
                format!("{}.to_bits()", parenthesized(&v))
            }
            (Cast::Bitcast, _, _) => {
                self.needs(BIT_CAST);
                format!("core::mem::transmute::<{}, {}>({v})", rust_type(from), rust_type(to))
            }
            _ => format!("({})", Self::cast_text(cast, from, &v, to)),
        }
    }

    /// `v` cast from `from` to `to`, for scalars.
    fn cast_text(cast: Cast, from: &Type, v: &str, to: &Type) -> String {
        let v = parenthesized(v);
        let t = rust_type(to);
        match cast {
            Cast::Trunc if *to == Type::I1 => format!("({v} & 1) != 0"),
            Cast::Trunc | Cast::ZExt | Cast::FPTrunc | Cast::FPExt | Cast::UIToFP | Cast::FPToUI => format!("{v} as {t}"),
            Cast::SExt if *from == Type::I1 => format!("(0{t}).wrapping_sub({v} as {t})"),
            Cast::SExt => format!("(({v} as {}) as {}) as {t}", signed_type(from), signed_type(to)),
            Cast::SIToFP => format!("({v} as {}) as {t}", signed_type(from)),
            // The integer indefinite when the value does not fit, as the machine's conversion
            // makes it; `as` would saturate.
            Cast::FPToSI if let Type::Int(n @ (16 | 32 | 64)) = to => format!("rhipper::fptosi{n}({v} as f64) as {t}"),
            Cast::FPToSI => format!("({v} as {}) as {t}", signed_type(to)),
            Cast::IntToPtr => format!("{v} as usize as *mut u8"),
            Cast::PtrToInt => format!("{v} as usize as {t}"),
            Cast::Bitcast => format!("core::mem::transmute::<{}, {t}>({v})", rust_type(from)),
        }
    }

    /// An LLVM intrinsic as a method or a runtime helper: `llvm.ctpop.i32` is `count_ones`.
    fn intrinsic(name: &str, fn_ty: &ra_asm_ir::FnType, args: &[String]) -> String {
        let base = name.trim_start_matches("llvm.").split('.').next().unwrap_or("");
        let ty = fn_ty.params.first().cloned().unwrap_or(Type::I64);
        let scalar = lane(&ty);
        let a = args.first().cloned().unwrap_or_default();
        let u = rust_type(&scalar);
        let bits = scalar.bits().unwrap_or(64);
        let one = |x: &str| -> String {
            let x = parenthesized(x);
            match base {
                "ctpop" => format!("{x}.count_ones() as {u}"),
                "cttz" => format!("(if {x} == 0 {{ {bits} }} else {{ {x}.trailing_zeros() }}) as {u}"),
                "ctlz" => format!("(if {x} == 0 {{ {bits} }} else {{ {x}.leading_zeros() }}) as {u}"),
                "bswap" => format!("{x}.swap_bytes()"),
                "sqrt" => format!("{x}.sqrt()"),
                "fabs" => format!("{x}.abs()"),
                "trunc" => format!("{x}.trunc()"),
                "floor" => format!("{x}.floor()"),
                "ceil" => format!("{x}.ceil()"),
                "rint" => format!("{x}.round_ties_even()"),
                "round" => format!("{x}.round()"),
                other => format!("rhipper::{other}({x})"),
            }
        };
        let two = |x: &str, y: &str| -> String {
            let (x, y) = (parenthesized(x), parenthesized(y));
            match base {
                "minnum" => format!("{x}.min({y})"),
                "maxnum" => format!("{x}.max({y})"),
                "fshl" | "fshr" => format!("rhipper::{base}_i{bits}({x}, {y}, {})", args.get(2).cloned().unwrap_or_default()),
                other => format!("rhipper::{other}({x}, {y})"),
            }
        };
        let is_vector = matches!(ty, Type::Vector(..));
        match args.len() {
            1 if is_vector => format!("rhipper::vmap({a}, |x| {})", one("x")),
            1 => one(&a),
            2 if base == "cttz" || base == "ctlz" => one(&a),
            2 | 3 if is_vector => format!("rhipper::vbin({a}, {}, |x, y| {})", args[1], two("x", "y")),
            2 | 3 => two(&a, &args[1]),
            _ => format!("rhipper::{base}({})", args.join(", ")),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Is(o) => self.operand(&Type::I1, o),
            Cond::Not(inner) => {
                if let Cond::Is(Operand::Local(v)) = inner.as_ref()
                    && self.defs.inlinable(*v)
                    && let Some((_, _, Inst::ICmp { cond, ty, lhs, rhs })) = self.defs.def(*v)
                {
                    return self.expr(&Inst::ICmp { cond: cond.inverse(), ty: ty.clone(), lhs: lhs.clone(), rhs: rhs.clone() });
                }
                format!("!{}", parenthesized(&self.cond(inner)))
            }
            Cond::And(a, b) => format!("({} && {})", self.cond(a), self.cond(b)),
            Cond::Or(a, b) => format!("({} || {})", self.cond(a), self.cond(b)),
        }
    }

    fn stmts(&mut self, out: &mut String, stmts: &[Stmt], depth: usize) {
        for s in stmts {
            self.stmt(out, s, depth);
        }
    }

    fn stmt(&mut self, out: &mut String, s: &Stmt, depth: usize) {
        let pad = "    ".repeat(depth);
        match s {
            Stmt::Block(b) => self.block(out, *b, depth, true),
            Stmt::Copy(b) => self.block(out, *b, depth, false),
            Stmt::Moves(b) => self.moves(out, *b, depth),
            Stmt::If { cond, then, els } => {
                let (cond, then, els) = if then.is_empty() && !els.is_empty() {
                    (Cond::Not(Box::new(cond.clone())), els, then)
                } else {
                    (cond.clone(), then, els)
                };
                let c = self.guarded(&strip_parens(&self.cond(&cond)));
                let _ = writeln!(out, "{pad}if {c} {{");
                self.stmts(out, then, depth + 1);
                if els.is_empty() {
                    let _ = writeln!(out, "{pad}}}");
                } else {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.stmts(out, els, depth + 1);
                    let _ = writeln!(out, "{pad}}}");
                }
            }
            Stmt::Loop { body, .. } => {
                let _ = writeln!(out, "{pad}loop {{");
                self.stmts(out, body, depth + 1);
                let _ = writeln!(out, "{pad}}}");
            }
            Stmt::Break => {
                let _ = writeln!(out, "{pad}break;");
            }
            Stmt::Continue => {
                let _ = writeln!(out, "{pad}continue;");
            }
            Stmt::Goto(b) => {
                let _ = writeln!(out, "{pad}rhipper::ra_goto(\"{}\", \"{}\");", self.f.name, label(*b, self.f));
            }
            Stmt::Switch { ty, value, cases } => self.switch(out, ty, value, cases, depth),
            Stmt::Return(None) => {
                let _ = writeln!(out, "{pad}return;");
            }
            Stmt::Return(Some((ty, o))) => {
                let text = format!("return {};", strip_parens(&self.operand(ty, o)));
                self.line(out, &pad, &text);
            }
            // After a call that does not return; a panic should the lift be wrong about that.
            Stmt::Unreachable => {
                let _ = writeln!(out, "{pad}unreachable!();");
            }
        }
    }

    /// A `switch` as a `match`. Rust has no fall-through: the cases of a fall-through chain share
    /// one arm, where each body but the last runs when the value entered the chain at or above it
    /// (`if matches!(v, 1 | 2) { ... }`), and the last, the shared tail, runs for all of them. The
    /// arm of the default comes last, since `_` would hide the arms after it.
    fn switch(&mut self, out: &mut String, ty: &Type, value: &Operand, cases: &[Case], depth: usize) {
        let pad = "    ".repeat(depth);
        let constant = |v: &i128| Self::int_constant(ty, *v);
        let mut chains: Vec<&[Case]> = Vec::new();
        let mut start = 0;
        for (i, case) in cases.iter().enumerate() {
            if !case.falls_through || i + 1 == cases.len() {
                chains.push(&cases[start..=i]);
                start = i + 1;
            }
        }
        chains.sort_by_key(|chain| chain.iter().any(|c| c.default));
        // A chain tests the value again: a plain local is read as it is, anything else is bound
        // once, before the body can change what it reads. A local is assigned again only as a phi,
        // on an edge into the block that defines it; that block dominates the switch, so the edge
        // is a back edge that leaves the chain before its next test.
        let mut subject = self.guarded(&strip_parens(&self.operand(ty, value)));
        if chains.iter().any(|chain| chain.len() > 1) && !matches!(value, Operand::Local(v) if !self.defs.inlinable(*v)) {
            let _ = writeln!(out, "{pad}let ra_switch = {subject};");
            subject = "ra_switch".into();
        }
        let _ = writeln!(out, "{pad}match {subject} {{");
        for chain in chains {
            let default_at = chain.iter().position(|c| c.default);
            let pattern = match default_at {
                Some(_) => "_".to_string(),
                None => chain.iter().flat_map(|c| c.values.iter()).map(constant).collect::<Vec<_>>().join(" | "),
            };
            let _ = writeln!(out, "{pad}    {pattern} => {{");
            for (k, case) in chain.iter().enumerate() {
                if k + 1 == chain.len() {
                    self.stmts(out, &case.body, depth + 2);
                    break;
                }
                // Before the default, the value is one of the cases above; from the default on,
                // it is none of the cases below.
                let (not, values) = match default_at {
                    Some(d) if k >= d => ("!", &chain[k + 1..]),
                    _ => ("", &chain[..=k]),
                };
                let values: Vec<String> = values.iter().flat_map(|c| c.values.iter()).map(constant).collect();
                if values.is_empty() {
                    self.stmts(out, &case.body, depth + 2);
                    continue;
                }
                let _ = writeln!(out, "{pad}        if {not}matches!({subject}, {}) {{", values.join(" | "));
                self.stmts(out, &case.body, depth + 3);
                let _ = writeln!(out, "{pad}        }}");
            }
            let _ = writeln!(out, "{pad}    }}");
        }
        let _ = writeln!(out, "{pad}}}");
    }

    /// The instructions of a block, then the phi moves to its successors; `labeled` for the
    /// block's own place, not for a copy.
    fn block(&mut self, out: &mut String, id: BlockId, depth: usize, labeled: bool) {
        let pad = "    ".repeat(depth);
        let f = self.f;
        let Some(block) = f.block(id) else { return };
        if labeled && self.labels.contains(&id) {
            let _ = writeln!(out, "{pad}// {}:", label(id, f));
        }
        for i in &block.instrs {
            match (&i.inst, i.result) {
                (Inst::Phi { .. }, Some(v)) => {
                    let _ = writeln!(out, "{pad}{} = {}_next;", name(f, v), name(f, v));
                }
                (Inst::Alloca { .. }, _) => {}
                (inst @ Inst::Store { ty, value, ptr, .. }, _) => {
                    let value = strip_parens(&self.operand(ty, value));
                    let text = if let Some(p) = self.safety.access(inst) {
                        self.store_text(ty, p, &value)
                    } else {
                        self.needs(RAW_MEMORY);
                        format!("core::ptr::write_unaligned({} as *mut {}, {value});", self.operand(&Type::Ptr, ptr), rust_type(ty))
                    };
                    self.line(out, &pad, &text);
                }
                (_, Some(v)) if self.defs.inlinable(v) || self.safety.folded.contains(&v) => {}
                (inst, Some(v)) => {
                    let text = format!("{} = {};", name(f, v), strip_parens(&self.expr(inst)));
                    self.line(out, &pad, &text);
                }
                (inst, None) => {
                    let text = format!("{};", self.expr(inst));
                    self.line(out, &pad, &text);
                }
            }
        }
        self.moves(out, id, depth);
    }

    /// The phis of the successors of `id` take their values here, all at once.
    fn moves(&mut self, out: &mut String, id: BlockId, depth: usize) {
        let pad = "    ".repeat(depth);
        let f = self.f;
        if let Some(moves) = self.defs.phi_moves.get(&id).cloned() {
            for (phi, o) in &moves {
                let ty = f.value_type(*phi).clone();
                let text = format!("{}_next = {};", name(f, *phi), strip_parens(&self.operand(&ty, o)));
                self.line(out, &pad, &text);
            }
        }
    }
}

fn label(b: BlockId, f: &Function) -> String {
    let n = f.block(b).map_or_else(|| format!("b{}", b.0), |b| b.name.clone());
    let s: String = n.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("L_{s}")
}
