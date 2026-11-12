//! The C backend.
//!
//! Every IR value is a C local of its type (`uint64_t`, `double`, `uint8_t *`), assigned where
//! its instruction stands; a pure value with one use in the same block is written inline at that
//! use instead, recursively, so an expression reads as one. Signedness is a cast at the operation
//! that needs it, as the IR says. Phis become copies at the end of each predecessor, through
//! temporaries so parallel copies stay parallel. The data globals are byte arrays with address
//! words; a reference into one is an address plus an offset. The runtime header `rhipper.h` holds
//! the helpers the intrinsics map to. A constant passed to a Windows API parameter the prototype
//! table binds to an enumeration, or compared or masked with such a parameter or result, reads as
//! the SDK's name, defined at the top of the output with the value the constant has; a `switch`
//! over a plain value reads over an `enum` of its cases.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, Const, FCond, Function, ICond, Inst, Module, Operand, Type, ValueId};
use ra_asm_lift::raise::prototypes;

use crate::common::{Defs, parenthesized, strip_parens, trap_count};
use crate::layout::{Elem, Layout};
use crate::structure::{Cond, Graph, Stmt, do_while, structure};

/// The language the backend writes: C, or C++ (`extern "C"` linkage for what the driver and the
/// data address, `.cpp` files), where the RTTI classes are classes and their virtual functions
/// methods ([`crate::cpp`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    #[default]
    C,
    Cpp,
}

impl Dialect {
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" => Some(Self::Cpp),
            _ => None,
        }
    }

    /// The file extension of a source file.
    pub fn extension(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "cpp",
        }
    }
}

/// What the backend writes.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The source name of each function, by IR name: the comment above it.
    pub names: HashMap<String, String>,
    pub dialect: Dialect,
    /// The classes of the program ([`crate::cpp::classes`]), which the C++ dialect declares; the C
    /// dialect ignores them.
    pub classes: Vec<crate::cpp::Class>,
    /// Records every function's arguments at entry and its result at each return through the
    /// runtime's trace hooks, under the name of the function table, so the port's run compares
    /// with the reference's event by event.
    pub trace: bool,
    /// The functions the hand-written sources of the port define, by IR name: each is written as
    /// its declaration under its C name and linkage, and its body is left to them. In the C++
    /// dialect such a function is no method of its class either.
    pub hand: HashSet<String>,
}

/// One function's C: the comment naming it, the definition line and the body; or, for a
/// hand-written function, the comment and the declaration.
#[derive(Debug, Clone)]
pub struct EmittedFunction {
    pub ir_name: String,
    pub address: Option<u64>,
    /// The lines above the definition line: the comment.
    pub header_lines: usize,
    /// The `ra_trap` calls in the body: instructions the lift did not translate.
    pub traps: usize,
    /// The blocks the structuring wrote a second time in place of a `goto`.
    pub copies: usize,
    /// Whether the hand-written sources own the function: the text is its declaration.
    pub hand: bool,
    pub text: String,
}

/// The translation in pieces: the shared declarations, the data and the functions, so the output
/// can be one file or several, and the function table can say where each function is.
#[derive(Debug, Clone)]
pub struct Unit {
    /// The comment and the include of the runtime header.
    pub include: String,
    /// The structs the type recovery found behind the pointers, before the functions that use them.
    pub types: String,
    /// The imports, as `extern` declarations.
    pub externs: String,
    /// The prototypes of the functions.
    pub prototypes: String,
    /// The structure types of the data globals.
    pub global_types: String,
    /// The `extern` declarations of the data globals, for the other files of a split output.
    pub global_decls: String,
    /// The definitions of the data globals, with their initializers.
    pub global_defs: String,
    /// The definition of each data global by itself, by name, in the module's order: what a split
    /// by module places in the file of the one module that addresses the global.
    pub global_parts: Vec<(String, String)>,
    /// What `global_defs` holds beyond the globals' own definitions: the pointers of the imports
    /// by ordinal and the constructor that writes the narrow address words.
    pub global_rest: String,
    /// The functions that address each data global, by the global's name and their IR names.
    pub global_users: HashMap<String, HashSet<String>>,
    /// The prototype of each function, by IR name, without its `;`.
    pub prototype_of: HashMap<String, String>,
    pub functions: Vec<EmittedFunction>,
    pub dialect: Dialect,
}

/// One file of the output, and the 1-based definition line of each function in it.
#[derive(Debug, Clone)]
pub struct EmittedFile {
    pub name: String,
    pub text: String,
    pub functions: Vec<(String, usize)>,
}

/// The functions one file of a split output holds. A program with more functions than this is
/// written as a header, a data file, and a file per this many functions.
pub const FUNCTIONS_PER_FILE: usize = 500;

/// The first line of a module's source and header, by which [`crate::write_files`] knows the
/// modules of an earlier output, whatever the build named them.
pub const MODULE_MARK: &str = "// Generated by ras emit: the module ";

/// The file names a module's stem must not take: the other files of the output and the runtime,
/// and the device names Windows reserves.
const RESERVED_STEMS: [&str; 12] =
    ["program", "data", "image", "rhipper", "driver", "exports", "resources", "functions", "con", "prn", "aux", "nul"];

impl Unit {
    /// The parts every single-file output starts with. The `extern` declarations come before the
    /// definitions in one file too: in C++ a `const` global has internal linkage without them.
    fn preamble(&self) -> [&String; 7] {
        [&self.include, &self.types, &self.externs, &self.prototypes, &self.global_types, &self.global_decls, &self.global_defs]
    }

    /// The whole translation as one unit.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for part in self.preamble() {
            out.push_str(part);
            out.push('\n');
        }
        for f in &self.functions {
            out.push_str(&f.text);
            out.push('\n');
        }
        out
    }

    /// The files of the output: `program.c` alone up to `max_functions` functions, else
    /// `program.h` with the declarations, `data.c` with the globals, and `program_NNN.c` each
    /// with up to `max_functions` functions.
    pub fn files(&self, max_functions: usize) -> Vec<EmittedFile> {
        if self.functions.len() <= max_functions.max(1) {
            let mut text = String::new();
            for part in self.preamble() {
                text.push_str(part);
                text.push('\n');
            }
            let functions = Self::append_functions(&mut text, &self.functions);
            return vec![EmittedFile { name: format!("program.{}", self.dialect.extension()), text, functions }];
        }
        let mut files = Vec::new();
        let mut header = String::from("#pragma once\n");
        for part in [&self.include, &self.types, &self.externs, &self.prototypes, &self.global_types, &self.global_decls] {
            header.push_str(part);
            header.push('\n');
        }
        files.push(EmittedFile { name: "program.h".into(), text: header, functions: Vec::new() });
        let mut data = String::from("#include \"program.h\"\n\n");
        data.push_str(&self.global_defs);
        files.push(EmittedFile { name: format!("data.{}", self.dialect.extension()), text: data, functions: Vec::new() });
        for (index, chunk) in self.functions.chunks(max_functions.max(1)).enumerate() {
            let mut text = String::from("#include \"program.h\"\n\n");
            let functions = Self::append_functions(&mut text, chunk);
            files.push(EmittedFile { name: format!("program_{index:03}.{}", self.dialect.extension()), text, functions });
        }
        files
    }

    /// The files of the output split by the modules of the build, from `objects`, the object file
    /// each function came from by IR name (`core.obj`, `msvcrt.lib:gs_report.obj`): `program.h`
    /// with the shared declarations and an include of each module's header, `<module>.h` with the
    /// prototypes of the module's functions, `<module>.c` with their definitions and the data
    /// globals only they address, `data.c` with the other globals, and `program.c` with the
    /// functions of no module. A function the symbols do not place lies in the module of its
    /// neighbours when both are of one: the linker lays an object's code out in one run. `None`
    /// when the functions come from fewer than two modules, which the other forms write as well.
    pub fn module_files(&self, objects: &HashMap<String, String>) -> Option<Vec<EmittedFile>> {
        let object_of = |name: &str| objects.get(name).map(String::as_str);
        let mut by_address: Vec<(u64, &str)> = self.functions.iter().filter_map(|f| Some((f.address?, f.ir_name.as_str()))).collect();
        by_address.sort_unstable();
        let mut order: Vec<&str> = Vec::new();
        for &(_, name) in &by_address {
            if let Some(object) = object_of(name)
                && !order.contains(&object)
            {
                order.push(object);
            }
        }
        if order.len() < 2 {
            return None;
        }
        let mut assigned: HashMap<&str, &str> = HashMap::new();
        for (i, &(_, name)) in by_address.iter().enumerate() {
            let object = object_of(name).or_else(|| {
                let before = by_address[..i].iter().rev().find_map(|&(_, n)| object_of(n))?;
                let after = by_address[i + 1..].iter().find_map(|&(_, n)| object_of(n))?;
                (before == after).then_some(before)
            });
            if let Some(object) = object {
                assigned.insert(name, object);
            }
        }
        let stems = module_stems(&order);
        let index: HashMap<&str, usize> = order.iter().enumerate().map(|(i, o)| (*o, i)).collect();
        let module_of = |name: &str| assigned.get(name).map(|o| index[o]);
        let mut members: Vec<Vec<&EmittedFunction>> = vec![Vec::new(); order.len()];
        let mut rest: Vec<&EmittedFunction> = Vec::new();
        for f in &self.functions {
            match module_of(&f.ir_name) {
                Some(m) => members[m].push(f),
                None => rest.push(f),
            }
        }
        // A global addressed by the functions of one module alone is that module's data.
        let mut owned: Vec<String> = vec![String::new(); order.len()];
        let mut shared = String::new();
        for (name, definition) in &self.global_parts {
            let mut owners = self.global_users.get(name).into_iter().flatten().map(|f| module_of(f));
            let owner = owners.next().flatten().filter(|first| owners.all(|o| o == Some(*first)));
            match owner {
                Some(m) => owned[m].push_str(definition),
                None => shared.push_str(definition),
            }
        }
        shared.push_str(&self.global_rest);

        let extension = self.dialect.extension();
        let mut files = Vec::new();
        let mut header = String::from("#pragma once\n");
        for part in [&self.include, &self.types, &self.externs] {
            header.push_str(part);
            header.push('\n');
        }
        header.push_str(&self.prototype_block(&rest));
        for stem in &stems {
            let _ = writeln!(header, "#include \"{stem}.h\"");
        }
        header.push('\n');
        for part in [&self.global_types, &self.global_decls] {
            header.push_str(part);
            header.push('\n');
        }
        files.push(EmittedFile { name: "program.h".into(), text: header, functions: Vec::new() });
        let mut data = String::from("#include \"program.h\"\n\n");
        data.push_str(&self.linkage(shared));
        files.push(EmittedFile { name: format!("data.{extension}"), text: data, functions: Vec::new() });
        for (((object, stem), functions), globals) in order.iter().zip(&stems).zip(&members).zip(owned) {
            let mut module_header = format!("{MODULE_MARK}{object}.\n#pragma once\n#include \"program.h\"\n\n");
            module_header.push_str(&self.prototype_block(functions));
            files.push(EmittedFile { name: format!("{stem}.h"), text: module_header, functions: Vec::new() });
            let mut text = format!("{MODULE_MARK}{object}.\n#include \"{stem}.h\"\n\n");
            if !globals.is_empty() {
                text.push_str(&self.linkage(globals));
                text.push('\n');
            }
            let functions = Self::append_functions(&mut text, functions.iter().copied());
            files.push(EmittedFile { name: format!("{stem}.{extension}"), text, functions });
        }
        if !rest.is_empty() {
            let mut text = String::from("#include \"program.h\"\n\n");
            let functions = Self::append_functions(&mut text, rest);
            files.push(EmittedFile { name: format!("program.{extension}"), text, functions });
        }
        Some(files)
    }

    /// The prototypes of `functions`, with C linkage in C++.
    fn prototype_block(&self, functions: &[&EmittedFunction]) -> String {
        let mut out = String::new();
        for f in functions {
            if let Some(prototype) = self.prototype_of.get(&f.ir_name) {
                let _ = writeln!(out, "{prototype};");
            }
        }
        let mut out = self.linkage(out);
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// `part` with C linkage in C++: the driver is C, and a definition under a prototype with C
    /// linkage inherits it.
    fn linkage(&self, part: String) -> String {
        if self.dialect == Dialect::Cpp && !part.trim().is_empty() { format!("extern \"C\" {{\n{part}}}\n") } else { part }
    }

    /// Appends `functions` to `text` and returns each one's definition line in it.
    fn append_functions<'f>(text: &mut String, functions: impl IntoIterator<Item = &'f EmittedFunction>) -> Vec<(String, usize)> {
        let mut line = text.lines().count() + 1;
        let mut lines = Vec::new();
        for f in functions {
            lines.push((f.ir_name.clone(), line + f.header_lines));
            text.push_str(&f.text);
            text.push('\n');
            line += f.text.lines().count() + 1;
        }
        lines
    }
}

/// The file stem of each module of `objects`: the object's own (`core.obj` is `core`), as an
/// identifier. The program's objects choose first; a library's member whose stem is taken
/// prefixes its library's (`msvcrt_main`), and a stem taken still counts on (`core_2`). Windows
/// compares file names without case, so the stems do too.
fn module_stems(objects: &[&str]) -> Vec<String> {
    let clean = |name: &str| -> String {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let stem: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        if stem.is_empty() { "module".to_owned() } else { stem }
    };
    let mut taken: HashSet<String> = RESERVED_STEMS.iter().map(|s| (*s).to_owned()).collect();
    let reserved = |stem: &str, taken: &HashSet<String>| {
        let lower = stem.to_ascii_lowercase();
        let port = lower.starts_with("com") || lower.starts_with("lpt");
        let device = port && lower.len() == 4 && lower.ends_with(|c: char| c.is_ascii_digit());
        device || taken.contains(&lower)
    };
    let mut stems = vec![String::new(); objects.len()];
    let mut order: Vec<usize> = (0..objects.len()).collect();
    order.sort_by_key(|&i| objects[i].contains(':'));
    for i in order {
        let (library, file) = objects[i].rsplit_once(':').map_or((None, objects[i]), |(l, f)| (Some(l), f));
        let mut stem = clean(file);
        if reserved(&stem, &taken)
            && let Some(library) = library
        {
            stem = format!("{}_{stem}", clean(library));
        }
        let base = stem.clone();
        let mut n = 2;
        while reserved(&stem, &taken) {
            stem = format!("{base}_{n}");
            n += 1;
        }
        taken.insert(stem.to_ascii_lowercase());
        stems[i] = stem;
    }
    stems
}

/// Writes `module` as one C translation unit.
pub fn emit_c(module: &Module, options: &Options) -> String {
    emit_unit(module, options).text()
}

/// Writes `module` as C, in pieces.
pub fn emit_unit(module: &Module, options: &Options) -> Unit {
    let mut include = String::new();
    let _ = writeln!(include, "// Generated by ras emit from {}.", module.name);
    let _ = writeln!(include, "#include \"rhipper.h\"");

    // The functions defined here, with their prototypes, so calls in any order resolve.
    let mut prototypes: HashMap<String, String> = HashMap::new();
    for f in &module.functions {
        prototypes.insert(f.name.clone(), prototype(f));
    }
    let mut externs = String::new();
    for d in &module.declarations {
        // The intrinsics and the runtime are in rhipper.h.
        if d.name.starts_with("llvm.") || d.name.starts_with("ra_") || prototypes.contains_key(&d.name) {
            continue;
        }
        let params: Vec<String> = d.ty.params.iter().map(c_type).collect();
        let _ = writeln!(
            externs,
            "extern {} {}({});",
            c_type(&d.ty.ret),
            d.name,
            if params.is_empty() { "void".to_owned() } else { params.join(", ") }
        );
    }
    let mut prototype_lines = String::new();
    for f in &module.functions {
        let _ = writeln!(prototype_lines, "{};", prototypes[&f.name]);
    }
    let (mut global_types, mut global_decls, mut global_defs) = (String::new(), String::new(), String::new());
    let mut init = String::new();
    // The data sections as the code's uses type them: arrays, scalars and strings over the bytes.
    let layouts = crate::layout::layouts(module);
    let mut global_parts = Vec::with_capacity(module.globals.len());
    for g in &module.globals {
        let layout = layouts.get(&g.name);
        global_type(&mut global_types, g, layout);
        declare_global(&mut global_decls, g, layout);
        let mut definition = String::new();
        define_global(&mut definition, &mut init, g, layout);
        global_defs.push_str(&definition);
        global_parts.push((g.name.clone(), definition));
    }
    let globals_end = global_defs.len();
    // An import by ordinal has no symbol to link: a pointer the runtime fills in before main from
    // the DLL's export table.
    for d in &module.declarations {
        let Some((dll, ordinal)) = crate::common::ordinal_import(&d.name) else { continue };
        let params: Vec<String> = d.ty.params.iter().map(c_type).collect();
        let pointer =
            format!("{} (*{})({})", c_type(&d.ty.ret), d.name, if params.is_empty() { "void".to_owned() } else { params.join(", ") });
        let _ = writeln!(externs, "extern {pointer};");
        let _ = writeln!(global_defs, "{pointer};");
        let _ = writeln!(init, "    {} = ({})ra_resolve_ordinal(\"{dll}\", {ordinal});", d.name, pointer.replacen(&d.name, "", 1));
    }
    // The address words narrower than a host pointer (a 32-bit program's) are written before
    // main: a compiler folds no truncated address into a static initializer.
    if !init.is_empty() {
        let _ = writeln!(global_defs, "RA_CONSTRUCTOR(ra_init_data) {{\n{init}}}");
    }
    let global_rest = global_defs[globals_end..].to_owned();
    let global_users = global_users(module);
    // In C++ the imports, the functions and the data keep C linkage: the driver is C, the imports
    // are the DLLs' own names, and a definition under a prototype with C linkage inherits it.
    if options.dialect == Dialect::Cpp {
        for part in [&mut externs, &mut prototype_lines, &mut global_decls, &mut global_defs] {
            if !part.trim().is_empty() {
                *part = format!("extern \"C\" {{\n{part}}}\n");
            }
        }
    }
    // In C++ a virtual function is a method of its class; one the module has no body for, or one
    // whose parameters the raise dropped, cannot take `this`.
    // A hand-written function stays out of the classes too: the hand source defines it under its C
    // name, as the declaration says.
    let by_name: HashMap<&str, &Function> =
        module.functions.iter().filter(|f| !options.hand.contains(&f.name)).map(|f| (f.name.as_str(), f)).collect();
    let classes: &[crate::cpp::Class] = if options.dialect == Dialect::Cpp { &options.classes } else { &[] };
    let method_of: HashMap<&str, (&crate::cpp::Class, crate::cpp::Method)> = classes
        .iter()
        .flat_map(|c| c.methods.iter().map(move |m| (c, m)))
        .filter_map(|(c, m)| {
            let f = by_name.get(m.ir_name.as_str())?;
            let method = crate::cpp::Method { this: m.this && !f.params.is_empty(), ..m.clone() };
            Some((m.ir_name.as_str(), (c, method)))
        })
        .collect();
    let mut sdk_names: BTreeMap<String, String> = BTreeMap::new();
    let functions = module
        .functions
        .iter()
        .map(|f| {
            let mut text = String::new();
            let method = method_of.get(f.name.as_str()).map(|(c, m)| (*c, m));
            let source_name = options.names.get(&f.name).map(String::as_str);
            if options.hand.contains(&f.name) {
                let header_lines = declare_hand(&mut text, f, &prototypes[&f.name], source_name);
                return EmittedFunction {
                    ir_name: f.name.clone(),
                    address: f.address,
                    header_lines,
                    traps: 0,
                    copies: 0,
                    hand: true,
                    text,
                };
            }
            let (header_lines, copies) = emit_function(
                &mut text,
                f,
                &prototypes[&f.name],
                source_name,
                &module.types,
                &layouts,
                method,
                &mut sdk_names,
                options.trace,
            );
            EmittedFunction { ir_name: f.name.clone(), address: f.address, header_lines, traps: trap_count(f), copies, hand: false, text }
        })
        .collect();
    let mut types = define_types(module);
    // The SDK's names the functions read their constants as, each with the value it stands for in
    // the type it was used at; two uses at different widths keep the second numeric.
    if !sdk_names.is_empty() {
        let mut defines = String::from("// The SDK's names of the constants passed to and read from the Windows API.\n");
        for (name, literal) in &sdk_names {
            let _ = writeln!(defines, "#define {name} {literal}");
        }
        defines.push('\n');
        types.insert_str(0, &defines);
    }
    if !classes.is_empty() {
        types.push('\n');
        types.push_str(&crate::cpp::declare(classes, &by_name, &module.types));
    }
    Unit {
        include,
        types,
        externs,
        prototypes: prototype_lines,
        global_types,
        global_decls,
        global_defs,
        global_parts,
        global_rest,
        global_users,
        prototype_of: prototypes,
        functions,
        dialect: options.dialect,
    }
}

/// The functions that address each data global of `module`, by the global's name.
fn global_users(module: &Module) -> HashMap<String, HashSet<String>> {
    let globals: HashSet<&str> = module.globals.iter().map(|g| g.name.as_str()).collect();
    let mut users: HashMap<String, HashSet<String>> = HashMap::new();
    for f in &module.functions {
        for b in &f.blocks {
            for operand in b.instrs.iter().flat_map(|i| i.inst.operands()).chain(b.terminator.operands()) {
                if let Operand::Global(name) = operand
                    && globals.contains(name.as_str())
                {
                    users.entry(name.clone()).or_default().insert(f.name.clone());
                }
            }
        }
    }
    users
}

/// The named structs the functions address fields of, as C structs: a field is `f<offset>`, the
/// padding between two is `pad<offset>`. The machine model's `%State` and any other type no
/// function reaches stay out.
fn define_types(module: &Module) -> String {
    let used: HashSet<&str> = module
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter().flat_map(|b| &b.instrs))
        .filter_map(|i| match &i.inst {
            Inst::Gep { base_ty: Type::Named(name), .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let mut out = String::new();
    for (name, fields) in &module.types {
        if !used.contains(name.as_str()) {
            continue;
        }
        let Some((offsets, _)) = ra_asm_ir::types::layout(fields, 8) else { continue };
        let _ = writeln!(out, "struct {name} {{");
        for (ty, offset) in fields.iter().zip(offsets) {
            let _ = writeln!(out, "    {};", declaration(ty, &field_name(ty, offset)));
        }
        let _ = writeln!(out, "}};");
    }
    out
}

/// The C name of a struct field at `offset`: the padding is named apart from the fields.
fn field_name(ty: &Type, offset: u64) -> String {
    match ty {
        Type::Array(_, e) if **e == Type::I8 => format!("pad{offset}"),
        _ => format!("f{offset}"),
    }
}

fn prototype(f: &Function) -> String {
    let params: Vec<String> = f.ty.params.iter().zip(&f.params).map(|(t, v)| format!("{} {}", c_type(t), name(f, *v))).collect();
    let noreturn = if f.attrs.iter().any(|a| a == "noreturn") { "RA_NORETURN " } else { "" };
    format!("{noreturn}{} {}({})", c_type(&f.ty.ret), f.name, if params.is_empty() { "void".to_owned() } else { params.join(", ") })
}

/// The C type of an IR type.
pub fn c_type(ty: &Type) -> String {
    match ty {
        Type::Void => "void".into(),
        Type::Int(1 | 8) => "uint8_t".into(),
        Type::Int(16) | Type::Half => "uint16_t".into(),
        Type::Int(32) => "uint32_t".into(),
        Type::Int(64) => "uint64_t".into(),
        Type::Int(128) => "ra_u128".into(),
        Type::Int(n) => format!("uint{n}_t"),
        Type::Float => "float".into(),
        Type::Double => "double".into(),
        Type::Fp80 => "long double".into(),
        Type::Ptr => "uint8_t *".into(),
        Type::Vector(n, e) => format!("ra_v{n}{}", vector_suffix(e)),
        Type::Array(n, e) => format!("{}[{n}]", c_type(e)),
        Type::Struct(_) | Type::Packed(_) => "struct ra_opaque".into(),
        Type::Named(name) => format!("struct {name}"),
        Type::Func(_) => "void *".into(),
    }
}

/// The C type a memory access of `ty` goes through: the unaligned twin of the type when the
/// access promises less alignment than the type has, which every machine access does.
fn access_type(ty: &Type, align: u32) -> String {
    let size = ty.size(8).unwrap_or(1);
    if size <= 1 || u64::from(align) >= size {
        return c_type(ty);
    }
    let name = c_type(ty);
    match name.as_str() {
        "uint16_t" | "uint32_t" | "uint64_t" | "float" | "double" => format!("ra_ua_{}", name.trim_end_matches("_t")),
        n if n.starts_with("ra_") => format!("ra_ua_{}", &n[3..]),
        _ => name,
    }
}

/// The integer vector a compare of `ty` (a vector) yields in C: one lane of the compared width per lane.
fn mask_type(ty: &Type) -> Type {
    match ty {
        Type::Vector(n, e) => Type::vector(*n, Type::Int(e.bits().unwrap_or(32) as u16)),
        other => other.clone(),
    }
}

fn vector_suffix(e: &Type) -> String {
    match e {
        Type::Int(n) => format!("u{n}"),
        Type::Float => "f32".into(),
        Type::Double => "f64".into(),
        other => c_type(other),
    }
}

/// A declaration of `name` with type `ty`: the array brackets go after the name in C.
pub(crate) fn declaration(ty: &Type, name: &str) -> String {
    match ty {
        Type::Array(n, e) => format!("{} {name}[{n}]", c_type(e)),
        Type::Ptr => format!("uint8_t *{name}"),
        other => format!("{} {name}", c_type(other)),
    }
}

/// An integer constant read as a signed number of `ty`'s width: `-1` for `0xffffffff` in `i32`.
fn signed_constant(ty: &Type, v: i128) -> String {
    let bits = ty.bits().unwrap_or(64).min(64);
    let shift = 64 - bits;
    let signed = (((v as u64) << shift) as i64) >> shift;
    match bits {
        64 => format!("{signed}ll"),
        _ => format!("{signed}"),
    }
}

/// The signed C type of an integer type.
fn signed_type(ty: &Type) -> String {
    match ty {
        Type::Int(1 | 8) => "int8_t".into(),
        Type::Int(16) => "int16_t".into(),
        Type::Int(32) => "int32_t".into(),
        Type::Int(64) => "int64_t".into(),
        Type::Int(128) => "ra_i128".into(),
        Type::Vector(n, e) => format!("ra_v{n}{}", vector_suffix(e).replace('u', "i")),
        other => c_type(other),
    }
}

/// The C name of a value.
pub(crate) fn name(f: &Function, v: ValueId) -> String {
    let n = f.value_name(v);
    let mut s: String = n.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, 'v');
    }
    if KEYWORDS.contains(&s.as_str()) || s == "inline" {
        s.push('_');
    }
    s
}

const KEYWORDS: [&str; 32] = [
    "int", "char", "long", "short", "float", "double", "void", "return", "if", "else", "for", "while", "do", "switch", "case", "default",
    "break", "continue", "goto", "static", "const", "struct", "union", "signed", "unsigned", "register", "auto", "extern", "volatile",
    "sizeof", "typedef", "enum",
];

fn label(b: BlockId, f: &Function) -> String {
    let n = f.block(b).map_or_else(|| format!("b{}", b.0), |b| b.name.clone());
    let s: String = n.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("L_{s}")
}

/// The structure type of a packed data global, which its declaration and its definition share:
/// the typed fields of its layout when its uses gave it one, the IR's byte runs and words else.
fn global_type(out: &mut String, g: &ra_asm_ir::Global, layout: Option<&Layout>) {
    if let Some(layout) = layout {
        let _ = write!(out, "RA_PACKED_BEGIN struct {}_t {{ ", g.name);
        for field in &layout.fields {
            let ty = match &field.elem {
                Elem::Bytes => "uint8_t".to_owned(),
                Elem::Char => "char".to_owned(),
                Elem::Word(ty) | Elem::Value(ty) => c_type(ty),
            };
            if field.array {
                let _ = write!(out, "{ty} {}[{}]; ", field.name(), field.count);
            } else {
                let _ = write!(out, "{ty} {}; ", field.name());
            }
        }
        let _ = writeln!(out, "}} RA_PACKED_END;");
        return;
    }
    if let (Type::Packed(fields), Some(Const::Struct(_))) = (&g.ty, &g.init) {
        let _ = write!(out, "RA_PACKED_BEGIN struct {}_t {{ ", g.name);
        for (i, field) in fields.iter().enumerate() {
            match field {
                Type::Array(n, _) => {
                    let _ = write!(out, "uint8_t f{i}[{n}]; ");
                }
                other => {
                    let _ = write!(out, "{} f{i}; ", c_type(other));
                }
            }
        }
        let _ = writeln!(out, "}} RA_PACKED_END;");
    }
}

/// Whether the global holds an address in a word narrower than a host pointer, which a statement
/// writes at startup: then the global cannot be `const`, or the compiler folds the zero it sees.
fn written_at_startup(g: &ra_asm_ir::Global) -> bool {
    if let (Type::Packed(fields), Some(Const::Struct(items))) = (&g.ty, &g.init) {
        items
            .iter()
            .zip(fields)
            .any(|(item, field)| matches!(item, Const::Address { .. } | Const::Global(_)) && field.size(8).is_some_and(|s| s < 8))
    } else {
        false
    }
}

fn storage_of(g: &ra_asm_ir::Global) -> &'static str {
    if g.constant && !written_at_startup(g) { "const " } else { "" }
}

/// The `extern` declaration of a data global, for the files that do not define it.
fn declare_global(out: &mut String, g: &ra_asm_ir::Global, layout: Option<&Layout>) {
    let storage = storage_of(g);
    if layout.is_some() {
        let _ = writeln!(out, "extern {storage}struct {}_t {};", g.name, g.name);
        return;
    }
    match (&g.ty, &g.init) {
        (Type::Array(n, e), Some(Const::Bytes(_) | Const::Zero)) if **e == Type::I8 => {
            let _ = writeln!(out, "extern {storage}uint8_t {}[{n}];", g.name);
        }
        (Type::Packed(_), Some(Const::Struct(_))) => {
            let _ = writeln!(out, "extern {storage}struct {}_t {};", g.name, g.name);
        }
        (Type::Array(_, e), Some(Const::Array(items))) if matches!(**e, Type::Struct(_)) => {
            let _ = writeln!(out, "extern const struct ra_function_entry {}[{}];", g.name, items.len().max(1));
        }
        (Type::Int(bits), Some(Const::Int(_))) => {
            let _ = writeln!(out, "extern {storage}uint{bits}_t {};", g.name);
        }
        (Type::Array(n, e), Some(Const::Zero)) if **e == Type::Double => {
            let _ = writeln!(out, "extern {storage}double {}[{n}];", g.name);
        }
        _ => {}
    }
}

/// The text of a constant the lift made rather than read from a section (a string decrypted at
/// lift time), when its bytes are one printable run and the NUL that ends it: the literal says
/// what the bytes would only encode.
fn literal_text<'a>(g: &ra_asm_ir::Global, n: u64, bytes: &'a [u8]) -> Option<&'a [u8]> {
    let (&0, text) = bytes.split_last()? else { return None };
    let printable = |&b: &u8| matches!(b, b'\t' | b'\n' | b'\r' | b' '..=b'~');
    (g.constant && g.address.is_none() && bytes.len() as u64 == n && !text.is_empty() && text.iter().all(printable)).then_some(text)
}

/// The definition of a data global, with its initializer. The type of a packed global comes from
/// [`global_type`] first. An address in a word narrower than a pointer is written by a statement
/// in `init` instead, at run time.
fn define_global(out: &mut String, init: &mut String, g: &ra_asm_ir::Global, layout: Option<&Layout>) {
    let storage = storage_of(g);
    if let Some(layout) = layout {
        define_laid_out(out, init, g, layout, storage);
        return;
    }
    if let (Type::Array(n, e), Some(Const::Bytes(bytes))) = (&g.ty, &g.init)
        && **e == Type::I8
        && let Some(text) = literal_text(g, *n, bytes)
    {
        let _ = writeln!(out, "{storage}uint8_t {}[{n}] RA_ALIGNED(16) = {};", g.name, crate::layout::string_literal(text));
        return;
    }
    match (&g.ty, &g.init) {
        (Type::Array(n, e), Some(Const::Bytes(bytes))) if **e == Type::I8 => {
            let _ = write!(out, "{storage}uint8_t {}[{n}] RA_ALIGNED(16) = {{", g.name);
            emit_bytes(out, bytes);
            let _ = writeln!(out, "}};");
        }
        (Type::Array(n, e), Some(Const::Zero)) if **e == Type::I8 => {
            let _ = writeln!(out, "{storage}uint8_t {}[{n}] RA_ALIGNED(16);", g.name);
        }
        (Type::Packed(fields), Some(Const::Struct(items))) => {
            // A packed structure of byte runs and address words. The initializer casts each
            // address to an integer, which the compilers accept in a static initializer.
            let _ = write!(out, "{storage}struct {}_t {} RA_ALIGNED(16) = {{ ", g.name, g.name);
            let mut offset_in = 0u64;
            for (i, (item, field)) in items.iter().zip(fields).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match item {
                    Const::Bytes(bytes) => {
                        out.push('{');
                        emit_bytes(out, bytes);
                        out.push('}');
                    }
                    other => word_item(out, init, &g.name, field, other, offset_in),
                }
                offset_in += field.size(8).unwrap_or(0);
            }
            let _ = writeln!(out, " }};");
        }
        (Type::Array(_, e), Some(Const::Array(items))) if matches!(**e, Type::Struct(_)) => {
            // The function table of the runtime: { address, function } pairs.
            let _ = writeln!(out, "const struct ra_function_entry {}[{}] = {{", g.name, items.len().max(1));
            for item in items {
                if let Const::Struct(fields) = item
                    && let [Const::Int(address), Const::Global(function)] = fields.as_slice()
                {
                    let _ = writeln!(out, "    {{ {address}ull, (void *)&{function} }},");
                }
            }
            let _ = writeln!(out, "}};");
        }
        (Type::Int(bits), Some(Const::Int(v))) => {
            let _ = writeln!(out, "{storage}uint{bits}_t {} = {v}ull;", g.name);
        }
        (Type::Array(n, e), Some(Const::Zero)) if **e == Type::Double => {
            let _ = writeln!(out, "{storage}double {}[{n}] RA_ALIGNED(16);", g.name);
        }
        (ty, init) => {
            let _ = writeln!(out, "// global {} of type {ty} with initializer {init:?} is not written", g.name);
        }
    }
}

/// The initializer of one address word of a data global, of the IR type `field`, at `offset_in`
/// bytes into the global: an integer as it is, an address cast to the word's type, and an
/// address in a word narrower than a host pointer as `0` plus a statement in `init`.
fn word_item(out: &mut String, init: &mut String, global_name: &str, field: &Type, item: &Const, offset_in: u64) {
    let narrow = field.size(8).is_some_and(|s| s < 8);
    match item {
        Const::Int(v) => {
            let _ = write!(out, "{v}ull");
        }
        Const::Address { global, offset } if narrow => {
            out.push('0');
            let _ = writeln!(
                init,
                "    *({} *)((uint8_t *)&{global_name} + {offset_in}) = ({})(uintptr_t)((uint8_t *)&{global} + {offset});",
                access_type(field, 1),
                c_type(field)
            );
        }
        Const::Address { global, offset } => {
            let _ = write!(out, "({})((uint8_t *)&{global} + {offset})", c_type(field));
        }
        Const::Global(name) if narrow => {
            out.push('0');
            let _ = writeln!(
                init,
                "    *({} *)((uint8_t *)&{global_name} + {offset_in}) = ({})(uintptr_t)&{name};",
                access_type(field, 1),
                c_type(field)
            );
        }
        Const::Global(name) => {
            let _ = write!(out, "({})&{name}", c_type(field));
        }
        other => {
            let _ = write!(out, "0 /* {other:?} */");
        }
    }
}

/// The definition of a data global through its layout: each field initialized from the bytes it
/// covers, the words as [`word_item`] writes them, a string as its literal, a float as its
/// decimal. A zero-initialized section has no initializer.
fn define_laid_out(out: &mut String, init: &mut String, g: &ra_asm_ir::Global, layout: &Layout, storage: &str) {
    let _ = write!(out, "{storage}struct {}_t {} RA_ALIGNED(16)", g.name, g.name);
    if matches!(g.init, Some(Const::Zero)) {
        let _ = writeln!(out, ";");
        return;
    }
    out.push_str(" = { ");
    for (i, field) in layout.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match &field.elem {
            Elem::Bytes => {
                out.push('{');
                emit_bytes(out, &layout.bytes[field.offset as usize..field.end() as usize]);
                out.push('}');
            }
            Elem::Char => {
                // The run without its NUL: the literal supplies it.
                out.push_str(&crate::layout::string_literal(&layout.bytes[field.offset as usize..field.end() as usize - 1]));
            }
            Elem::Word(ty) => {
                if field.array {
                    out.push_str("{ ");
                }
                for n in 0..field.count {
                    if n > 0 {
                        out.push_str(", ");
                    }
                    let offset = field.offset + n * field.elem_size();
                    match layout.words.get(&offset) {
                        Some((_, item)) => word_item(out, init, &g.name, ty, item, offset),
                        None => out.push('0'),
                    }
                }
                if field.array {
                    out.push_str(" }");
                }
            }
            Elem::Value(ty) => {
                if field.array {
                    out.push('{');
                }
                for n in 0..field.count {
                    if field.array && n % 16 == 0 {
                        out.push_str("\n    ");
                    } else if n > 0 {
                        out.push(' ');
                    }
                    out.push_str(&value_literal(ty, layout.element(field, n)));
                    if field.array {
                        out.push(',');
                    }
                }
                if field.array {
                    out.push_str("\n}");
                }
            }
        }
    }
    let _ = writeln!(out, " }};");
}

/// The literal of an integer or float value from its little-endian bytes, in its C type.
fn value_literal(ty: &Type, bytes: &[u8]) -> String {
    if let Some(literal) = crate::layout::float_literal(ty, bytes) {
        return literal;
    }
    let mut v = 0u64;
    for (n, b) in bytes.iter().enumerate() {
        v |= u64::from(*b) << (8 * n);
    }
    match ty {
        Type::Int(64) => format!("{v}ull"),
        Type::Int(32) => format!("{v}u"),
        _ => v.to_string(),
    }
}

fn emit_bytes(out: &mut String, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if i % 32 == 0 {
            out.push_str("\n    ");
        }
        let _ = write!(out, "{b},");
    }
    out.push('\n');
}

/// Writes one function, with one comment line above it naming it: the source name when the image
/// gives one, and its address. As a `method` of a C++ class, the body is the method's definition,
/// its first parameter recovered from `this`, and the function under its own name follows as the
/// vtable's entry. Returns the number of lines above the definition and the number of blocks
/// written twice.
fn emit_function(
    out: &mut String,
    f: &Function,
    prototype: &str,
    source_name: Option<&str>,
    types: &[(String, Vec<Type>)],
    layouts: &HashMap<String, Layout>,
    method: Option<(&crate::cpp::Class, &crate::cpp::Method)>,
    sdk_names: &mut BTreeMap<String, String>,
    trace: bool,
) -> (usize, usize) {
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
    match method {
        Some((class, m)) => {
            let _ = writeln!(out, "{} {{", crate::cpp::method_prototype(f, class, m, true));
        }
        None => {
            let _ = writeln!(out, "{prototype} {{");
        }
    }
    // The entry hook: nothing, unless the port is built with RA_TRACE, when it names the function.
    let _ = writeln!(out, "    RA_ENTER(\"{}\");", f.name);
    if let Some((_, m)) = method
        && m.this
    {
        let ty = &f.ty.params[0];
        let _ = writeln!(out, "    {} = ({})(uintptr_t)this;", declaration(ty, &name(f, f.params[0])), c_type(ty));
    }
    // The trace hooks name the function as the table does, so the reference's trace, recorded by
    // address and named through the table, reads the same.
    let traced = trace.then(|| source_name.unwrap_or(&f.name).to_owned());
    if let Some(traced) = &traced {
        let mut kinds = String::new();
        let mut args = String::new();
        for (ty, v) in f.ty.params.iter().zip(&f.params) {
            let v = name(f, *v);
            let (kind, arg) = match ty {
                Type::Int(_) => ('u', format!("(uint64_t){v}")),
                Type::Ptr | Type::Func(_) => ('p', format!("(uint64_t)(uintptr_t){v}")),
                Type::Double => ('f', format!("ra_f64_bits({v})")),
                Type::Float => ('g', format!("ra_f32_bits({v})")),
                _ => ('?', "(uint64_t)0".to_owned()),
            };
            kinds.push(kind);
            args.push_str(", ");
            args.push_str(&arg);
        }
        let _ = writeln!(out, "    ra_trace_enter(\"{traced}\", \"{kinds}\"{args});");
    }
    let graph = Graph::of(f);
    let structured = structure(f, &graph);
    let mut e = Emitter::new(f, &structured.labels, types, layouts, std::mem::take(sdk_names), traced);
    e.plan_loops(&structured.body, &graph);
    e.declare_locals(out);
    e.stmts(out, &structured.body, 1);
    let _ = writeln!(out, "}}");
    *sdk_names = e.names.into_inner();
    if let Some((class, m)) = method {
        out.push('\n');
        crate::cpp::trampoline(out, f, class, m, prototype);
    }
    (header_lines, structured.copies())
}

/// A hand-written function in place of its definition: the comment that names it and says who
/// owns it, and the declaration the hand source must meet. Returns the number of lines above the
/// declaration.
fn declare_hand(out: &mut String, f: &Function, prototype: &str, source_name: Option<&str>) -> usize {
    let at = f.address.map_or_else(String::new, |a| format!(" at {a:#x}"));
    match source_name {
        Some(n) if n != f.name => {
            let _ = writeln!(out, "// {n}{at}: hand-written in the port's sources");
        }
        _ => {
            let _ = writeln!(out, "// {}{at}: hand-written in the port's sources", f.name);
        }
    }
    let _ = writeln!(out, "{prototype};");
    1
}

/// The values of `f` the Windows API types with an enumeration: the result of a call to an import
/// whose prototype binds one, a value passed in a parameter that binds one, and, to a fixpoint,
/// what is cast, frozen, selected or carried by a phi from them.
fn enumerations(f: &Function) -> HashMap<ValueId, &'static str> {
    let mut out: HashMap<ValueId, &'static str> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Call { callee: Callee::Direct(callee), args, .. } = &i.inst else { continue };
        let Some((result, params)) = prototypes::named(callee) else { continue };
        if let (Some(v), Some(e)) = (i.result, result) {
            out.insert(v, e);
        }
        for ((_, arg), e) in args.iter().zip(params) {
            if let (Operand::Local(v), Some(e)) = (arg, e) {
                out.entry(*v).or_insert(e);
            }
        }
    }
    loop {
        let mut changed = false;
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            let Some(v) = i.result else { continue };
            if out.contains_key(&v) {
                continue;
            }
            let sources: Vec<&Operand> = match &i.inst {
                Inst::Cast { cast: Cast::Trunc | Cast::ZExt | Cast::SExt, value, .. } | Inst::Freeze { value, .. } => vec![value],
                Inst::Select { then, els, .. } => vec![then, els],
                Inst::Phi { incoming, .. } => incoming.iter().map(|(o, _)| o).collect(),
                _ => continue,
            };
            let typed = sources.iter().find_map(|o| match o {
                Operand::Local(s) => out.get(s).copied(),
                _ => None,
            });
            if let Some(e) = typed {
                out.insert(v, e);
                changed = true;
            }
        }
        if !changed {
            return out;
        }
    }
}

/// The literal a name's `#define` holds: the value in the type of the use, in hexadecimal.
fn define_literal(ty: &Type, value: u64, bits: u32) -> String {
    let value = if bits >= 64 { value } else { value & ((1u64 << bits) - 1) };
    match ty {
        Type::Ptr => format!("((uint8_t *){value:#x}ull)"),
        Type::Int(64) => format!("{value:#x}ull"),
        Type::Int(32) => format!("{value:#x}u"),
        _ => format!("(({}){value:#x})", c_type(ty)),
    }
}

struct Emitter<'a> {
    f: &'a Function,
    labels: &'a HashSet<BlockId>,
    defs: Defs<'a>,
    /// The named struct types of the module.
    types: &'a [(String, Vec<Type>)],
    /// The typed layouts of the data sections, by global: an access into one reads as its field.
    layouts: &'a HashMap<String, Layout>,
    /// The locals that point at a struct: each is declared as `struct S *`, and reads as a
    /// `uint8_t *` through a cast everywhere but at its fields.
    struct_of: HashMap<ValueId, &'a str>,
    /// The loops written as a `for` over their header's phis, by header.
    for_loops: HashMap<BlockId, ForLoop>,
    /// The loops written as a call to `ra_strlen`, by header.
    strlen_loops: HashMap<BlockId, StrlenLoop>,
    /// The phis a `for` carries: no `_next`, no copy at the header, no move at the edges.
    hoisted: HashSet<ValueId>,
    /// The values a `for` step absorbs (`i++` for `i_next = i + 1`): not declared, not assigned.
    skipped: HashSet<ValueId>,
    /// The name a value reads as: a step's value reads as the phi it steps into once the loop
    /// is left, where the two are equal.
    alias: HashMap<ValueId, ValueId>,
    /// The aliases of the `for` condition being written: the condition runs after the step, where
    /// each next-round value is already in its phi.
    cond_alias: Vec<(ValueId, ValueId)>,
    /// The enumeration of the values the Windows API typed: the result of a call whose prototype
    /// names one, a value passed in such a parameter, and what is cast or carried from them.
    enum_of: HashMap<ValueId, &'static str>,
    /// The SDK's names written so far, each with the literal its `#define` holds: a name read at
    /// another width keeps that use numeric, so the define stays one.
    names: RefCell<BTreeMap<String, String>>,
    /// The switches written so far: each over a plain value gets its own `enum`.
    switches: Cell<usize>,
    /// The name every return records its result under, when the function is traced.
    traced: Option<String>,
}

/// A `do ... while` loop written as `for (init; cond; step)` over the phis of its header, which
/// holds when the condition is decided by the stepped values alone and is true before the first
/// round, as the compiler knew when it rotated the loop.
struct ForLoop {
    phis: Vec<Hoisted>,
    /// The next-round values defined in the loop, read as their phi in the condition.
    aliases: Vec<(ValueId, ValueId)>,
}

/// One phi of a `for`: its first value, its next-round value, and the constant the step adds
/// when the next value is `phi + k`, written `phi += k`. A next value computed in the latch for
/// the phi alone is absorbed: the step writes its expression, `n = (n & 1) ? 3 * n + 1 : n / 2`.
struct Hoisted {
    phi: ValueId,
    init: Operand,
    next: Operand,
    add: Option<i128>,
    absorbed: bool,
}

/// A one-block loop that only walks to the end of a string, `while (s[i]) i++` as the compiler
/// wrote it: the index phi with its first value and its increment, and the byte it reads at
/// `base + phi + offset` in the integer type of that sum. The loop leaves with the phi at the
/// index of the terminator: `init + strlen(base + init + offset)`.
struct StrlenLoop {
    phi: ValueId,
    init: Operand,
    next: ValueId,
    base: Operand,
    ty: Type,
    offset: i128,
    /// The pure values the header computes for the code after the loop (a widened copy of the
    /// index): written after the call, at their last iteration's value.
    carried: Vec<ValueId>,
}

impl<'a> Emitter<'a> {
    fn new(
        f: &'a Function,
        labels: &'a HashSet<BlockId>,
        types: &'a [(String, Vec<Type>)],
        layouts: &'a HashMap<String, Layout>,
        names: BTreeMap<String, String>,
        traced: Option<String>,
    ) -> Self {
        let defs = Defs::new(f);
        let mut struct_of = HashMap::new();
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            if let Inst::Gep { base_ty: Type::Named(name), ptr: Operand::Local(v), indices, .. } = &i.inst
                && indices.len() == 2
                && !f.params.contains(v)
                && !defs.inlinable(*v)
            {
                struct_of.entry(*v).or_insert(name.as_str());
            }
        }
        Self {
            f,
            labels,
            defs,
            types,
            layouts,
            struct_of,
            for_loops: HashMap::new(),
            strlen_loops: HashMap::new(),
            hoisted: HashSet::new(),
            skipped: HashSet::new(),
            alias: HashMap::new(),
            cond_alias: Vec::new(),
            enum_of: enumerations(f),
            names: RefCell::new(names),
            switches: Cell::new(0),
            traced,
        }
    }

    /// The SDK's name of the constant `v` of `ty` in `enumeration`, or `None` where the names do
    /// not cover it or the name is already defined at another width. Flags read or-ed in
    /// parentheses. Each name's define is recorded with the literal of its own value in `ty`.
    fn sdk_name(&self, ty: &Type, enumeration: &str, v: i128) -> Option<String> {
        let bits = match ty {
            Type::Ptr => 64,
            Type::Int(n) if *n >= 8 => u32::from(*n),
            _ => return None,
        };
        let parts = prototypes::constant_name(enumeration, v as u64, bits)?;
        let mut names = self.names.borrow_mut();
        // One name stands for the constant as it is (a `DWORD` member in a 64-bit argument keeps
        // the argument's bits); flags each for their own bit.
        let literals: Vec<(&str, String)> =
            parts.iter().map(|(name, value)| (*name, define_literal(ty, if parts.len() == 1 { v as u64 } else { *value }, bits))).collect();
        if literals.iter().any(|(name, literal)| names.get(*name).is_some_and(|known| known != literal)) {
            return None;
        }
        for (name, literal) in &literals {
            names.entry((*name).to_owned()).or_insert_with(|| literal.clone());
        }
        let text: Vec<&str> = parts.iter().map(|(name, _)| *name).collect();
        Some(if text.len() == 1 { text[0].to_owned() } else { format!("({})", text.join(" | ")) })
    }

    /// The `enum` of a switch over a plain value: its tag (`classify_case`, numbered from the
    /// second switch of a function on) and the prefix of its enumerators (`CLASSIFY_0`), when the
    /// subject is a C integer of at most 32 bits and every case is an `int`, which C's enumerators
    /// must be. A wider subject cast to the enum would alias cases; it keeps its numbers.
    fn case_enum(&self, ty: &Type, cases: &[crate::structure::Case]) -> Option<(String, String)> {
        if !matches!(ty, Type::Int(8 | 16 | 32)) {
            return None;
        }
        let values: Vec<i128> = cases.iter().flat_map(|c| c.values.iter().map(|v| sign_extended(ty, *v))).collect();
        if values.is_empty() || values.iter().any(|v| i32::try_from(*v).is_err()) {
            return None;
        }
        let n = self.switches.get() + 1;
        self.switches.set(n);
        let base = self.f.name.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        let tag = if n == 1 { format!("{base}_case") } else { format!("{base}_case{n}") };
        let prefix = if n == 1 { base.to_ascii_uppercase() } else { format!("{}_{n}", base.to_ascii_uppercase()) };
        Some((tag, prefix))
    }

    /// The constant `k` as the name the other operand's enumeration gives it, else as the operand
    /// it is, which splats it over a vector's lanes.
    fn named_or_constant(&self, ty: &Type, other: &Operand, k: i128) -> String {
        let named = match other {
            Operand::Local(v) => self.enum_of.get(v).and_then(|e| self.sdk_name(ty, e, k)),
            _ => None,
        };
        named.unwrap_or_else(|| self.operand(ty, &Operand::Int(k)))
    }

    /// Decides the `for` loops of the body, at any depth, before the locals are declared.
    fn plan_loops(&mut self, stmts: &[Stmt], g: &Graph) {
        for s in stmts {
            match s {
                Stmt::Loop { header, body } => {
                    if let Some((rest, cond)) = do_while(body)
                        && let Some(plan) = self.strlen_loop(*header, rest, &cond, g)
                    {
                        self.hoisted.insert(plan.phi);
                        self.strlen_loops.insert(*header, plan);
                    } else if let Some((rest, cond)) = do_while(body)
                        && let Some(plan) = self.for_loop(*header, rest, &cond, g)
                    {
                        for h in &plan.phis {
                            self.hoisted.insert(h.phi);
                            if (h.add.is_some() || h.absorbed)
                                && let Some(n) = h.next.local()
                                && !self.inlinable(n)
                            {
                                self.skipped.insert(n);
                                self.alias.insert(n, h.phi);
                            }
                        }
                        self.for_loops.insert(*header, plan);
                    }
                    self.plan_loops(body, g);
                }
                Stmt::If { then, els, .. } => {
                    self.plan_loops(then, g);
                    self.plan_loops(els, g);
                }
                Stmt::Switch { cases, .. } => {
                    for case in cases {
                        self.plan_loops(&case.body, g);
                    }
                }
                _ => {}
            }
        }
    }

    /// The `for` form of the `do ... while` loop at `header` whose body is `rest` and whose test
    /// is `cond`, when it has one: entered from one block and continued from one, the latch at
    /// the body's end; a condition that compares the next-round values and constants, inlined,
    /// reading no phi (the step changed them), and true for the first values.
    fn for_loop(&self, header: BlockId, rest: &[Stmt], cond: &Cond, g: &Graph) -> Option<ForLoop> {
        if self.labels.contains(&header) {
            return None;
        }
        let members = g.loops.get(&header)?;
        let (inside, outside): (Vec<BlockId>, Vec<BlockId>) = g.preds.get(&header)?.iter().partition(|p| members.contains(p));
        let (&[pre], &[latch]) = (outside.as_slice(), inside.as_slice()) else { return None };
        if !matches!(rest.last(), Some(Stmt::Block(b) | Stmt::Copy(b)) if *b == latch) {
            return None;
        }
        let mut phis: Vec<(ValueId, Operand, Operand)> = Vec::new();
        for i in &self.f.block(header)?.instrs {
            if let (Some(p), Inst::Phi { incoming, .. }) = (i.result, &i.inst) {
                if self.struct_of.contains_key(&p) {
                    return None;
                }
                let from = |b: BlockId| incoming.iter().find(|(_, x)| *x == b).map(|(o, _)| o.clone());
                phis.push((p, from(pre)?, from(latch)?));
            }
        }
        if phis.is_empty() {
            return None;
        }
        let names: Vec<ValueId> = phis.iter().map(|(p, ..)| *p).collect();
        // The steps run one after the other: one that reads another phi would read its new value.
        if phis.iter().any(|(_, _, next)| next.local().is_some_and(|v| names.contains(&v))) {
            return None;
        }
        let (c, negated) = match cond {
            Cond::Is(Operand::Local(c)) => (*c, false),
            Cond::Not(inner) => match inner.as_ref() {
                Cond::Is(Operand::Local(c)) => (*c, true),
                _ => return None,
            },
            _ => return None,
        };
        let Some((_, _, Inst::ICmp { cond: pred, ty: Type::Int(bits), lhs, rhs })) = self.def(c) else { return None };
        if !self.inlinable(c) || self.reads_any(c, &names) {
            return None;
        }
        let at_start = |o: &Operand| match o {
            Operand::Int(v) => Some(*v),
            Operand::Local(v) => match phis.iter().find(|(_, _, next)| *next == Operand::Local(*v)) {
                Some((_, Operand::Int(init), _)) => Some(*init),
                _ => None,
            },
            _ => None,
        };
        if compare(*pred, *bits, at_start(lhs)?, at_start(rhs)?) == negated {
            return None;
        }
        let defined_in = |v: ValueId| self.def(v).is_some_and(|(b, ..)| members.contains(&b.id));
        let mut aliases = Vec::new();
        let hoisted = phis
            .into_iter()
            .map(|(phi, init, next)| {
                let (mut add, mut absorbed) = (None, false);
                if let Some(n) = next.local().filter(|n| defined_in(*n)) {
                    aliases.push((n, phi));
                    let for_phi_alone = self.inlinable(n) || self.used_only_by(n, phi, c, members);
                    if let Some((
                        _,
                        _,
                        Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub), lhs: Operand::Local(base), rhs: Operand::Int(k), .. },
                    )) = self.def(n)
                        && *base == phi
                        && for_phi_alone
                    {
                        add = Some(if *op == BinOp::Sub { -k } else { *k });
                    } else if !self.inlinable(n) && for_phi_alone {
                        // The steps run in order: an expression that reads another phi would see
                        // its new value, so only one over its own phi moves to the step.
                        let others: Vec<ValueId> = names.iter().copied().filter(|p| *p != phi).collect();
                        absorbed = self.def(n).is_some_and(|(b, ..)| b.id == latch)
                            && self.defs.holds_to_block_end(n)
                            && !self.reads_any(n, &others);
                    }
                }
                Hoisted { phi, init, next, add, absorbed }
            })
            .collect();
        Some(ForLoop { phis: hoisted, aliases })
    }

    /// The `strlen` the one-block loop at `header` is, when its only work is to step an index
    /// while the byte at `base + index + offset` is not zero: nothing else in the block but what
    /// the condition inlines, the index stepped by one.
    fn strlen_loop(&self, header: BlockId, rest: &[Stmt], cond: &Cond, g: &Graph) -> Option<StrlenLoop> {
        if self.labels.contains(&header) || !matches!(rest, [Stmt::Block(b)] if *b == header) || g.loops.get(&header)?.len() != 1 {
            return None;
        }
        let (inside, outside): (Vec<BlockId>, Vec<BlockId>) = g.preds.get(&header)?.iter().partition(|p| **p == header);
        let (&[pre], &[_]) = (outside.as_slice(), inside.as_slice()) else { return None };
        let mut phi: Option<(ValueId, Operand, Operand)> = None;
        let mut next = None;
        let mut carried = Vec::new();
        let block = self.f.block(header)?;
        for i in &block.instrs {
            match (&i.inst, i.result) {
                (Inst::Phi { incoming, .. }, Some(p)) => {
                    if phi.is_some() {
                        return None;
                    }
                    let from = |b: BlockId| incoming.iter().find(|(_, x)| *x == b).map(|(o, _)| o.clone());
                    phi = Some((p, from(pre)?, from(header)?));
                }
                (Inst::Bin { op: BinOp::Add, lhs: Operand::Local(base), rhs: Operand::Int(1), .. }, Some(n))
                    if phi.as_ref().is_some_and(|(p, ..)| p == base) && next.is_none() =>
                {
                    next = Some(n);
                }
                (_, Some(v)) if self.inlinable(v) => {}
                (inst, Some(v)) if !inst.has_side_effects() => carried.push(v),
                _ => return None,
            }
        }
        let (phi, init, stepped) = phi?;
        let next = next?;
        if stepped != Operand::Local(next) {
            return None;
        }
        // A carried value is for the code after the loop only: nothing in the loop reads it.
        let read_in_loop = |v: ValueId| block.instrs.iter().any(|i| i.inst.uses().contains(&v)) || block.terminator.uses().contains(&v);
        if carried.iter().any(|&v| read_in_loop(v)) {
            return None;
        }
        // The test: the byte is not zero.
        let (c, negated) = match cond {
            Cond::Is(Operand::Local(c)) => (*c, false),
            Cond::Not(inner) => match inner.as_ref() {
                Cond::Is(Operand::Local(c)) => (*c, true),
                _ => return None,
            },
            _ => return None,
        };
        let Some((_, _, Inst::ICmp { cond: pred, ty: Type::Int(8), lhs: Operand::Local(byte), rhs: Operand::Int(0) })) = self.def(c) else {
            return None;
        };
        if !self.inlinable(c) || !self.inlinable(*byte) || (*pred != ICond::Ne || negated) && (*pred != ICond::Eq || !negated) {
            return None;
        }
        let Some((_, _, Inst::Load { ty: Type::Int(8), ptr, .. })) = self.def(*byte) else { return None };
        // The address: an integer sum of the index, one value from outside the loop and constants.
        let (mut address, mut ty) = (ptr.clone(), Type::Ptr);
        while let Some((_, _, Inst::Cast { cast: Cast::IntToPtr | Cast::ZExt, from, value, .. })) =
            address.local().filter(|v| self.inlinable(*v)).and_then(|v| self.def(v))
        {
            ty = from.clone();
            address = value.clone();
        }
        let Type::Int(bits) = ty else { return None };
        let mut terms = Vec::new();
        self.sum_terms(&address, &mut terms);
        let (mut base, mut offset, mut index) = (None, 0i128, false);
        for t in terms {
            match t {
                Operand::Int(k) => offset += k,
                Operand::Local(v) if v == phi && !index => index = true,
                Operand::Local(v) if self.def(v).is_some_and(|(b, ..)| b.id == header) => return None,
                other if base.is_none() => base = Some(other),
                _ => return None,
            }
        }
        if !index {
            return None;
        }
        let mask = if bits >= 128 { -1 } else { (1i128 << bits) - 1 };
        Some(StrlenLoop { phi, init, next, base: base?, ty, offset: offset & mask, carried })
    }

    /// The terms of a sum written inline: `(a + b) + 3` is `a`, `b`, `3`.
    fn sum_terms(&self, o: &Operand, terms: &mut Vec<Operand>) {
        if let Some((_, _, Inst::Bin { op: BinOp::Add, lhs, rhs, .. })) = o.local().filter(|v| self.inlinable(*v)).and_then(|v| self.def(v))
        {
            self.sum_terms(lhs, terms);
            self.sum_terms(rhs, terms);
        } else {
            terms.push(o.clone());
        }
    }

    /// The loop at `header` as its `strlen`: the index phi takes its first value plus the length
    /// of the string from the first byte read, the increment follows when something reads it,
    /// then the values the exit's phis take from the block.
    fn strlen_call(&mut self, out: &mut String, header: BlockId, depth: usize) {
        let pad = "    ".repeat(depth);
        let Some(s) = self.strlen_loops.remove(&header) else { return };
        let f = self.f;
        let ty = f.value_type(s.phi);
        let mut start = self.operand(&s.ty, &s.base);
        let mut offset = sign_extended(&s.ty, s.offset);
        match &s.init {
            Operand::Int(k) => offset = sign_extended(&s.ty, offset + k),
            init => {
                let _ = write!(start, " + {}", self.operand(ty, init));
            }
        }
        if offset < 0 {
            let _ = write!(start, " - {}", Self::int_constant(&s.ty, -offset));
        } else if offset != 0 {
            let _ = write!(start, " + {}", Self::int_constant(&s.ty, offset));
        }
        let length = format!("({})ra_strlen((const char *)(uintptr_t)({start}))", c_type(ty));
        let value = match &s.init {
            Operand::Int(0) => length,
            Operand::Int(k) if negative_in(ty, *k) => format!("{length} - {}", Self::int_constant(ty, -sign_extended(ty, *k))),
            init => format!("{} + {length}", self.operand(ty, init)),
        };
        let _ = writeln!(out, "{pad}{} = {value};", name(f, s.phi));
        if !self.inlinable(s.next)
            && let Some((_, _, inst)) = self.def(s.next)
        {
            let _ = writeln!(out, "{pad}{} = {};", name(f, s.next), strip_parens(&self.expr(inst)));
        }
        for v in &s.carried {
            if let Some((_, _, inst)) = self.def(*v) {
                let _ = writeln!(out, "{pad}{} = {};", name(f, *v), strip_parens(&self.expr(inst)));
            }
        }
        self.moves(out, header, depth);
    }

    /// Whether the expression of `v`, as it is written inline, reads one of `names`.
    fn reads_any(&self, v: ValueId, names: &[ValueId]) -> bool {
        let Some((_, _, inst)) = self.def(v) else { return false };
        inst.uses().iter().any(|&u| names.contains(&u) || ((self.inlinable(u) || self.field_pointer(u)) && self.reads_any(u, names)))
    }

    /// Whether `n` is used only by `phi`, by `cond`, and outside the loop of `members` by
    /// something other than a phi: the places where the phi it steps into holds the same value.
    fn used_only_by(&self, n: ValueId, phi: ValueId, cond: ValueId, members: &HashSet<BlockId>) -> bool {
        self.f.blocks.iter().all(|b| {
            let instrs = b.instrs.iter().all(|i| {
                i.result == Some(phi)
                    || i.result == Some(cond)
                    || !i.inst.uses().contains(&n)
                    || (!members.contains(&b.id) && !matches!(i.inst, Inst::Phi { .. }))
            });
            instrs && (!members.contains(&b.id) || !b.terminator.uses().contains(&n))
        })
    }

    /// Whether `v` is a field pointer: a `getelementptr` of a named struct's field. It is written
    /// at each use as the field itself, however many uses it has.
    fn field_pointer(&self, v: ValueId) -> bool {
        matches!(self.def(v), Some((_, _, Inst::Gep { base_ty, indices, .. })) if crate::common::field_of(self.types, base_ty, indices).is_some())
    }

    /// The field a pointer operand addresses, when it is a field pointer: the access expression
    /// `p->f8` and the field's type.
    fn field_access(&self, ptr: &Operand) -> Option<(String, &'a Type)> {
        let Operand::Local(g) = ptr else { return None };
        let Some((_, _, Inst::Gep { base_ty, ptr: base, indices, .. })) = self.def(*g) else { return None };
        let field = crate::common::field_of(self.types, base_ty, indices)?;
        Some((format!("{}->{}", self.struct_base(base, field.name), field_name(field.ty, field.offset)), field.ty))
    }

    /// A pointer operand as a pointer to `struct_name`: the local declared as one, or a cast of
    /// the integer it was made of, or of the pointer.
    fn struct_base(&self, base: &Operand, struct_name: &str) -> String {
        if let Operand::Local(v) = base {
            if self.struct_of.get(v) == Some(&struct_name) {
                return name(self.f, *v);
            }
            if self.inlinable(*v)
                && let Some((_, _, Inst::Cast { cast: Cast::IntToPtr, from, value, .. })) = self.def(*v)
            {
                return format!("((struct {struct_name} *){})", self.operand(from, value));
            }
        }
        format!("((struct {struct_name} *){})", self.operand(&Type::Ptr, base))
    }

    /// The expression of a memory access of `ty` at `ptr`: the field itself when `ptr` names one
    /// of its type, the field's bytes through a cast when the types differ, a data section's
    /// typed field or element when `ptr` addresses one, the raw access else.
    fn access(&self, ty: &Type, align: u32, ptr: &Operand) -> String {
        match self.field_access(ptr) {
            Some((field, field_ty)) if field_ty == ty => field,
            Some((field, _)) => format!("*({} *)&{field}", access_type(ty, align)),
            None => match self.section_access(ty, ptr) {
                Some(access) => access,
                None => format!("*({} *)({})", access_type(ty, align), self.operand(&Type::Ptr, ptr)),
            },
        }
    }

    /// A data section's field at `ptr` in the type of the access: `sec_rdata.f96` for a scalar,
    /// `sec_rdata.f32[i]` for an element, `None` where the access does not fit the layout.
    fn section_access(&self, ty: &Type, ptr: &Operand) -> Option<String> {
        let p = crate::layout::place(&self.defs, ptr)?;
        let layout = self.layouts.get(&p.global)?;
        let offset = u64::try_from(p.offset).ok()?;
        let field = layout.field_at(offset)?;
        if field.elem_type().as_ref() != Some(ty) {
            return None;
        }
        let size = field.elem_size();
        let rel = offset - field.offset;
        if rel % size != 0 {
            return None;
        }
        let base = rel / size;
        let name = format!("{}.{}", p.global, field.name());
        match p.index {
            None if field.array => Some(format!("{name}[{base}]")),
            None => Some(name),
            Some(index) if field.array && index.scale == size => {
                let i = self.operand(&index.ty, &index.value);
                Some(if base == 0 { format!("{name}[{i}]") } else { format!("{name}[{i} + {base}]") })
            }
            Some(_) => None,
        }
    }

    fn def(&self, v: ValueId) -> Option<(&'a Block, usize, &'a Inst)> {
        self.defs.def(v)
    }

    fn inlinable(&self, v: ValueId) -> bool {
        self.defs.inlinable(v)
    }

    fn declare_locals(&mut self, out: &mut String) {
        let mut declared: HashSet<ValueId> = self.f.params.iter().copied().collect();
        for b in &self.f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                if self.inlinable(v) || self.field_pointer(v) || self.skipped.contains(&v) || !declared.insert(v) {
                    continue;
                }
                match &i.inst {
                    Inst::Alloca { ty, align } => {
                        let _ = writeln!(out, "    {} RA_ALIGNED({align});", declaration(ty, &format!("{}_mem", name(self.f, v))));
                        let _ = writeln!(out, "    uint8_t *{} = (uint8_t *)&{}_mem;", name(self.f, v), name(self.f, v));
                    }
                    Inst::Phi { .. } if let Some(s) = self.struct_of.get(&v) => {
                        let _ = writeln!(out, "    struct {s} *{}, *{}_next;", name(self.f, v), name(self.f, v));
                    }
                    Inst::Phi { ty, .. } if self.hoisted.contains(&v) => {
                        let _ = writeln!(out, "    {};", declaration(ty, &name(self.f, v)));
                    }
                    Inst::Phi { ty, .. } => {
                        let _ = writeln!(out, "    {} {}, {}_next;", c_type(ty), name(self.f, v), name(self.f, v));
                    }
                    _ if let Some(s) = self.struct_of.get(&v) => {
                        let _ = writeln!(out, "    struct {s} *{};", name(self.f, v));
                    }
                    // A vector compare's result is a vector of lane masks, the width of what it
                    // compared: C has no vector of conditions.
                    Inst::ICmp { ty: ty @ Type::Vector(..), .. } | Inst::FCmp { ty: ty @ Type::Vector(..), .. } => {
                        let _ = writeln!(out, "    {};", declaration(&mask_type(ty), &name(self.f, v)));
                    }
                    inst => {
                        let _ = writeln!(out, "    {};", declaration(&inst.result_type(), &name(self.f, v)));
                    }
                }
            }
        }
    }

    /// The type of the lane masks a vector compare leaves, when `o` is one.
    fn compare_mask_type(&self, o: &Operand) -> Option<Type> {
        let Operand::Local(v) = o else { return None };
        match self.def(*v) {
            Some((_, _, Inst::ICmp { ty: ty @ Type::Vector(..), .. } | Inst::FCmp { ty: ty @ Type::Vector(..), .. })) => {
                Some(mask_type(ty))
            }
            _ => None,
        }
    }

    /// An integer constant in `ty`, as C reads it best: small, signed where negative, without a
    /// cast for the types C does not promote.
    fn int_constant(ty: &Type, v: i128) -> String {
        match ty {
            Type::Int(1) => {
                if v & 1 == 1 {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            Type::Int(128) => {
                // C has no 128-bit literal: the high half shifted over the low one.
                let (high, low) = ((v as u128 >> 64) as u64, v as u64);
                if high == 0 { format!("((ra_u128){low}ull)") } else { format!("(((ra_u128){high}ull << 64) | {low}ull)") }
            }
            Type::Int(64) => {
                let u = v as u64;
                if u > 0x8000_0000_0000_0000 { format!("(uint64_t)-{}", u.wrapping_neg()) } else { format!("{u}ull") }
            }
            Type::Int(32) => {
                let u = (v as u64) & 0xffff_ffff;
                if u > 0x8000_0000 { format!("(uint32_t)-{}", (u as u32).wrapping_neg()) } else { format!("{u}u") }
            }
            Type::Int(n) => format!("(uint{n}_t){}", (v as u64) & ((1u64 << n) - 1)),
            Type::Ptr => format!("(uint8_t *){}ull", v as u64),
            _ => format!("{v}"),
        }
    }

    fn operand(&self, ty: &Type, o: &Operand) -> String {
        match o {
            Operand::Local(v) => {
                let aliased = self.cond_alias.iter().find(|(n, _)| n == v).map(|(_, p)| *p).or_else(|| self.alias.get(v).copied());
                let v = &aliased.unwrap_or(*v);
                if self.inlinable(*v)
                    && let Some((_, _, inst)) = self.def(*v)
                {
                    return self.expr(inst);
                }
                if self.struct_of.contains_key(v) {
                    return format!("((uint8_t *){})", name(self.f, *v));
                }
                if self.field_pointer(*v)
                    && let Some((_, _, inst)) = self.def(*v)
                {
                    return self.expr(inst);
                }
                name(self.f, *v)
            }
            Operand::Int(v) => match ty {
                Type::Vector(n, e) => {
                    let lane = Self::int_constant(e, *v);
                    let lanes: Vec<String> = (0..*n).map(|_| lane.clone()).collect();
                    format!("(({}){{{}}})", c_type(ty), lanes.join(", "))
                }
                other => Self::int_constant(other, *v),
            },
            Operand::Float(v) => match ty {
                Type::Float => format!("ra_f32({:#x}u)", (*v as f32).to_bits()),
                _ => format!("ra_f64({:#x}ull)", v.to_bits()),
            },
            Operand::Global(name) => format!("((uint8_t *)&{name})"),
            Operand::Address { global, offset } => {
                let pointer = if *offset == 0 { format!("(uint8_t *)&{global}") } else { format!("((uint8_t *)&{global} + {offset})") };
                match ty {
                    Type::Ptr => pointer,
                    // Through uintptr_t: a 32-bit program's address is the low half of the host's.
                    other => format!("({})(uintptr_t){pointer}", c_type(other)),
                }
            }
            Operand::Null => "(uint8_t *)0".into(),
            Operand::Undef => match ty {
                Type::Ptr => "(uint8_t *)0".into(),
                Type::Float | Type::Double => "0.0".into(),
                Type::Vector(..) => format!("(({}){{0}})", c_type(ty)),
                _ => "0".into(),
            },
            Operand::Zero => format!("(({}){{0}})", c_type(ty)),
        }
    }

    /// The expression of an instruction.
    fn expr(&self, inst: &Inst) -> String {
        match inst {
            Inst::Alloca { .. } => "0".into(),
            Inst::Load { ty, ptr, align, .. } => self.access(ty, *align, ptr),
            Inst::Store { .. } | Inst::Phi { .. } => String::new(),
            Inst::Bin { op, ty, lhs, rhs } => {
                let c = c_type(ty);
                let s = signed_type(ty);
                let a = self.operand(ty, lhs);
                // A shift's left operand at its own width: a bare literal is an int, too narrow
                // for a 64-bit shift, whether it came as a constant or as a folded value.
                let literal = a.trim_start_matches('-').chars().all(|ch| ch.is_ascii_hexdigit() || ch == 'x');
                let shifted = if literal && matches!(op, BinOp::Shl | BinOp::LShr) { format!("({c}){a}") } else { a.clone() };
                // `x + -1` is `x - 1`.
                let (op, rhs) = match (op, rhs) {
                    (BinOp::Add, Operand::Int(v)) if negative_in(ty, *v) => (&BinOp::Sub, Operand::Int(-v)),
                    (BinOp::Sub, Operand::Int(v)) if negative_in(ty, *v) => (&BinOp::Add, Operand::Int(-v)),
                    (op, rhs) => (op, rhs.clone()),
                };
                // `x ^ 1` on a condition is `!x`.
                if *ty == Type::I1 && *op == BinOp::Xor && rhs == Operand::Int(1) {
                    return format!("!{}", parenthesized(&a));
                }
                // A mask over a value the API typed reads as the SDK's flags.
                let b = match (&op, &rhs) {
                    (BinOp::And | BinOp::Or | BinOp::Xor, Operand::Int(k)) => self.named_or_constant(ty, lhs, *k),
                    _ => self.operand(ty, &rhs),
                };
                // C promotes a type narrower than int, so the result needs the cast back; a
                // 32-bit or 64-bit operation keeps its type.
                let c = if matches!(ty, Type::Int(32 | 64)) { String::new() } else { format!("({c})") };
                match op {
                    BinOp::Add => format!("{c}({a} + {b})"),
                    BinOp::Sub => format!("{c}({a} - {b})"),
                    BinOp::Mul => format!("{c}({a} * {b})"),
                    BinOp::UDiv | BinOp::FDiv => format!("({a} / {b})"),
                    BinOp::SDiv => format!("{c}(({s}){a} / ({s}){b})"),
                    BinOp::URem => format!("({a} % {b})"),
                    BinOp::SRem => format!("{c}(({s}){a} % ({s}){b})"),
                    BinOp::Shl => format!("{c}({shifted} << {b})"),
                    BinOp::LShr => format!("{c}({shifted} >> {b})"),
                    BinOp::AShr => format!("({}){}", c_type(ty), format_args!("(({s}){a} >> {b})")),
                    BinOp::And => format!("({a} & {b})"),
                    BinOp::Or => format!("({a} | {b})"),
                    BinOp::Xor => format!("({a} ^ {b})"),
                    BinOp::FAdd => format!("({a} + {b})"),
                    BinOp::FSub => format!("({a} - {b})"),
                    BinOp::FMul => format!("({a} * {b})"),
                    BinOp::FRem => format!("ra_frem({a}, {b})"),
                }
            }
            Inst::ICmp { cond, ty, lhs, rhs } => {
                // A constant against a value the API typed reads as the SDK's name.
                let side = |o: &Operand, other: &Operand| match o {
                    Operand::Int(k) => self.named_or_constant(ty, other, *k),
                    _ => self.operand(ty, o),
                };
                let (a, b) = (side(lhs, rhs), side(rhs, lhs));
                // A signed compare casts its operands; a constant reads as a signed number instead.
                let signed = |o: &Operand, text: String| match o {
                    Operand::Int(v) => signed_constant(ty, *v),
                    _ => format!("({}){text}", signed_type(ty)),
                };
                let (a, b, op) = match cond {
                    ICond::Eq => (a, b, "=="),
                    ICond::Ne => (a, b, "!="),
                    ICond::Ugt => (a, b, ">"),
                    ICond::Uge => (a, b, ">="),
                    ICond::Ult => (a, b, "<"),
                    ICond::Ule => (a, b, "<="),
                    ICond::Sgt => (signed(lhs, a), signed(rhs, b), ">"),
                    ICond::Sge => (signed(lhs, a), signed(rhs, b), ">="),
                    ICond::Slt => (signed(lhs, a), signed(rhs, b), "<"),
                    ICond::Sle => (signed(lhs, a), signed(rhs, b), "<="),
                };
                format!("({a} {op} {b})")
            }
            Inst::FCmp { cond, ty, lhs, rhs } => {
                let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
                match cond {
                    FCond::Oeq => format!("({a} == {b})"),
                    FCond::Ogt => format!("({a} > {b})"),
                    FCond::Oge => format!("({a} >= {b})"),
                    FCond::Olt => format!("({a} < {b})"),
                    FCond::Ole => format!("({a} <= {b})"),
                    FCond::One => format!("({a} < {b} || {a} > {b})"),
                    FCond::Ord => format!("(!ra_isnan({a}) && !ra_isnan({b}))"),
                    FCond::Ueq => format!("(!({a} < {b} || {a} > {b}))"),
                    FCond::Ugt => format!("(!({a} <= {b}))"),
                    FCond::Uge => format!("(!({a} < {b}))"),
                    FCond::Ult => format!("(!({a} >= {b}))"),
                    FCond::Ule => format!("(!({a} > {b}))"),
                    FCond::Une => format!("({a} != {b})"),
                    FCond::Uno => format!("(ra_isnan({a}) || ra_isnan({b}))"),
                }
            }
            Inst::Select { cond, ty, then, els } => {
                // A select between the two values its compare orders is the minimum or maximum.
                if let Some((_, _, Inst::ICmp { cond: pred, ty: cmp_ty, lhs, rhs })) =
                    cond.local().filter(|c| self.inlinable(*c)).and_then(|c| self.def(c))
                    && cmp_ty == ty
                    && let Type::Int(bits @ (8 | 16 | 32 | 64)) = ty
                    && (lhs == then && rhs == els || lhs == els && rhs == then)
                {
                    let straight = lhs == then;
                    let helper = match pred {
                        ICond::Slt | ICond::Sle => Some(if straight { "smin" } else { "smax" }),
                        ICond::Sgt | ICond::Sge => Some(if straight { "smax" } else { "smin" }),
                        ICond::Ult | ICond::Ule => Some(if straight { "umin" } else { "umax" }),
                        ICond::Ugt | ICond::Uge => Some(if straight { "umax" } else { "umin" }),
                        ICond::Eq | ICond::Ne => None,
                    };
                    if let Some(helper) = helper {
                        return format!(
                            "ra_{helper}_i{bits}({}, {})",
                            strip_parens(&self.operand(ty, lhs)),
                            strip_parens(&self.operand(ty, rhs))
                        );
                    }
                }
                format!("({} ? {} : {})", self.operand(&Type::I1, cond), self.operand(ty, then), self.operand(ty, els))
            }
            // The address of a global as a pointer needs no trip through an integer.
            Inst::Cast { cast: Cast::IntToPtr, value: value @ Operand::Address { .. }, .. } => self.operand(&Type::Ptr, value),
            Inst::Cast { cast, from, value, to } => {
                let v = self.operand(from, value);
                // A conversion lane by lane between vectors is the builtin; a C cast between vector
                // types is a reinterpretation, and only of the same size.
                if let (Type::Vector(..), Type::Vector(..)) = (from, to)
                    && *cast != Cast::Bitcast
                {
                    // A vector compare in C is already a mask of all ones per lane, at the width
                    // of what it compared: a cast when that is the target's lane width, a lane
                    // conversion otherwise. A condition vector from elsewhere is 0 or 1 per lane.
                    if *cast == Cast::SExt
                        && let Type::Vector(_, e) = from
                        && **e == Type::I1
                    {
                        let t = c_type(to);
                        return match self.compare_mask_type(value) {
                            Some(mask) if mask.size(8) == to.size(8) => format!("({t})({v})"),
                            Some(mask) => format!("__builtin_convertvector(({})({v}), {t})", signed_type(&mask)),
                            None => format!("({t})(0 - __builtin_convertvector({v}, {t}))"),
                        };
                    }
                    return match cast {
                        Cast::SIToFP | Cast::SExt => format!("__builtin_convertvector(({}){v}, {})", signed_type(from), c_type(to)),
                        Cast::FPToSI => format!("({})__builtin_convertvector({v}, {})", c_type(to), signed_type(to)),
                        _ => format!("__builtin_convertvector({v}, {})", c_type(to)),
                    };
                }
                match cast {
                    Cast::Trunc
                    | Cast::ZExt
                    | Cast::UIToFP
                    | Cast::FPToUI
                    | Cast::FPTrunc
                    | Cast::FPExt
                    | Cast::IntToPtr
                    | Cast::PtrToInt => format!("({}){v}", c_type(to)),
                    // A condition sign-extended is a mask of all ones, as `cmpltsd` leaves it.
                    Cast::SExt if *from == Type::I1 => format!("({})(0u - ({})({v}))", c_type(to), c_type(to)),
                    Cast::SExt | Cast::SIToFP => format!("({})({}){v}", c_type(to), signed_type(from)),
                    // The integer indefinite when the value does not fit, as the machine's conversion
                    // makes it; a C cast leaves that undefined.
                    Cast::FPToSI if let Type::Int(n @ (16 | 32 | 64)) = to => format!("({})ra_fptosi_{n}({v})", c_type(to)),
                    Cast::FPToSI => format!("({})({}){v}", c_type(to), signed_type(to)),
                    Cast::Bitcast => format!("ra_bitcast_{}_{}({v})", short_type(from), short_type(to)),
                }
            }
            Inst::Gep { base_ty, ptr, indices, .. } => {
                // A field's address, taken as the bytes it is.
                if let Some(field) = crate::common::field_of(self.types, base_ty, indices) {
                    return format!("((uint8_t *)&{}->{})", self.struct_base(ptr, field.name), field_name(field.ty, field.offset));
                }
                // Constant indexes fold into one offset; the rest scale by the element size.
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
                        other => dynamic.push(if scale == 1 {
                            self.operand(ty, other)
                        } else {
                            format!("{scale} * {}", self.operand(ty, other))
                        }),
                    }
                }
                let mut s = self.operand(&Type::Ptr, ptr);
                if constant < 0 {
                    let _ = write!(s, " - {}", -constant);
                } else if constant > 0 {
                    let _ = write!(s, " + {constant}");
                }
                for d in dynamic {
                    let _ = write!(s, " + {d}");
                }
                format!("({s})")
            }
            Inst::ExtractElement { ty, vector, index } => format!("({})[{}]", self.operand(ty, vector), self.operand(&Type::I32, index)),
            Inst::InsertElement { ty, vector, element, index } => {
                let element_ty = match ty {
                    Type::Vector(_, e) => (**e).clone(),
                    other => other.clone(),
                };
                format!(
                    "ra_insert({}, {}, {})",
                    self.operand(ty, vector),
                    self.operand(&Type::I32, index),
                    self.operand(&element_ty, element)
                )
            }
            Inst::ShuffleVector { ty, a, b, mask } => {
                let m: Vec<String> = mask.iter().map(|i| if *i < 0 { "-1".to_owned() } else { i.to_string() }).collect();
                format!("__builtin_shufflevector({}, {}, {})", self.operand(ty, a), self.operand(ty, b), m.join(", "))
            }
            Inst::Call { fn_ty, callee, args, .. } => {
                // A constant in a parameter the prototype table binds to an enumeration reads as
                // the SDK's name.
                let params = match callee {
                    Callee::Direct(name) => prototypes::named(name).map(|(_, params)| params).unwrap_or_default(),
                    Callee::Indirect(_) => Vec::new(),
                };
                let a: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(n, (t, o))| match (o, params.get(n).copied().flatten()) {
                        (Operand::Int(k), Some(e)) => {
                            self.sdk_name(t, e, *k).map_or_else(|| strip_parens(&self.operand(t, o)), |s| strip_parens(&s))
                        }
                        _ => strip_parens(&self.operand(t, o)),
                    })
                    .collect();
                match callee {
                    // A funnel shift of one value by both halves is its rotate.
                    Callee::Direct(name) if name.starts_with("llvm.fsh") && a.len() == 3 && a[0] == a[1] => {
                        format!("{}({}, {})", intrinsic(name).replace("fshl", "rotl").replace("fshr", "rotr"), a[0], a[2])
                    }
                    Callee::Direct(name) if name.starts_with("llvm.") => format!("{}({})", intrinsic(name), a.join(", ")),
                    Callee::Direct(name) => format!("{name}({})", a.join(", ")),
                    Callee::Indirect(p) => {
                        let params: Vec<String> = fn_ty.params.iter().map(c_type).collect();
                        format!("(({} (*)({}))({}))({})", c_type(&fn_ty.ret), params.join(", "), self.operand(&Type::Ptr, p), a.join(", "))
                    }
                }
            }
            Inst::Freeze { ty, value } => self.operand(ty, value),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Is(o) => self.operand(&Type::I1, o),
            // `!(a <= b)` is `a > b` for an integer comparison that inlines here.
            Cond::Not(inner) => {
                if let Cond::Is(Operand::Local(v)) = inner.as_ref()
                    && self.inlinable(*v)
                    && let Some((_, _, Inst::ICmp { cond, ty, lhs, rhs })) = self.def(*v)
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

    /// An `if`, opened after `lead`: the block's indentation, or `} else ` when it continues a
    /// chain. An else-branch that is one `if` continues the chain, so a dispatch over the classes
    /// of a receiver reads as `if ... else if ... else`.
    fn if_statement(&mut self, out: &mut String, cond: &Cond, then: &[Stmt], els: &[Stmt], depth: usize, lead: &str) {
        let pad = "    ".repeat(depth);
        // An empty then-branch reads better inverted: `if (!c) { ... }`.
        let (cond, then, els) =
            if then.is_empty() && !els.is_empty() { (Cond::Not(Box::new(cond.clone())), els, then) } else { (cond.clone(), then, els) };
        let _ = writeln!(out, "{lead}if ({}) {{", strip_parens(&self.cond(&cond)));
        self.stmts(out, then, depth + 1);
        match els {
            [] => {
                let _ = writeln!(out, "{pad}}}");
            }
            // The blocks before the inner `if` hold its condition alone, which the `if` inlines.
            [before @ .., Stmt::If { cond, then, els }] if before.iter().all(|s| self.silent(s)) => {
                let _ = write!(out, "{pad}}} else ");
                self.if_statement(out, cond, then, els, depth, "");
            }
            _ => {
                let _ = writeln!(out, "{pad}}} else {{");
                self.stmts(out, els, depth + 1);
                let _ = writeln!(out, "{pad}}}");
            }
        }
    }

    /// Whether `s` is a block that prints nothing: no label, every instruction inlined, skipped
    /// or an allocation, and no phi move to a successor.
    fn silent(&self, s: &Stmt) -> bool {
        let Stmt::Block(id) = s else { return false };
        let Some(block) = self.f.block(*id) else { return true };
        if self.labels.contains(id) {
            return false;
        }
        let moves = self.defs.phi_moves.get(id).is_some_and(|moves| moves.iter().any(|(phi, _)| !self.hoisted.contains(phi)));
        !moves
            && block.instrs.iter().all(|i| match (&i.inst, i.result) {
                (Inst::Phi { .. }, Some(v)) => self.hoisted.contains(&v),
                (Inst::Alloca { .. }, _) => true,
                (_, Some(v)) => self.skipped.contains(&v) || self.inlinable(v) || self.field_pointer(v),
                (_, None) => false,
            })
    }

    fn stmt(&mut self, out: &mut String, s: &Stmt, depth: usize) {
        let pad = "    ".repeat(depth);
        match s {
            Stmt::Block(b) => self.block(out, *b, depth, true),
            Stmt::Copy(b) => self.block(out, *b, depth, false),
            Stmt::Moves(b) => self.moves(out, *b, depth),
            Stmt::If { cond, then, els } => self.if_statement(out, cond, then, els, depth, &pad),
            Stmt::Loop { header, body } => match do_while(body) {
                Some(_) if self.strlen_loops.contains_key(header) => self.strlen_call(out, *header, depth),
                Some((rest, cond)) if let Some(plan) = self.for_loops.remove(header) => {
                    let inits: Vec<String> = plan.phis.iter().map(|h| self.assignment(h.phi, &h.init)).collect();
                    let steps: Vec<String> = plan.phis.iter().map(|h| self.step(h)).collect();
                    self.cond_alias = plan.aliases;
                    let cond = strip_parens(&self.cond(&cond));
                    self.cond_alias.clear();
                    let _ = writeln!(out, "{pad}for ({}; {cond}; {}) {{", inits.join(", "), steps.join(", "));
                    self.stmts(out, rest, depth + 1);
                    let _ = writeln!(out, "{pad}}}");
                }
                Some((rest, cond)) => {
                    let _ = writeln!(out, "{pad}do {{");
                    self.stmts(out, rest, depth + 1);
                    let _ = writeln!(out, "{pad}}} while ({});", strip_parens(&self.cond(&cond)));
                }
                None => {
                    let _ = writeln!(out, "{pad}for (;;) {{");
                    self.stmts(out, body, depth + 1);
                    let _ = writeln!(out, "{pad}}}");
                }
            },
            Stmt::Break => {
                let _ = writeln!(out, "{pad}break;");
            }
            Stmt::Continue => {
                let _ = writeln!(out, "{pad}continue;");
            }
            Stmt::Goto(b) => {
                let _ = writeln!(out, "{pad}goto {};", label(*b, self.f));
            }
            Stmt::Switch { ty, value, cases } => {
                let subject = strip_parens(&self.operand(ty, value));
                let enumeration = value.local().and_then(|v| self.enum_of.get(&v).copied());
                let mut labels: HashMap<i128, String> = HashMap::new();
                if let Some(e) = enumeration {
                    // The cases of a value the API typed read as the SDK's names.
                    for v in cases.iter().flat_map(|c| &c.values) {
                        if let Some(name) = self.sdk_name(ty, e, *v) {
                            labels.insert(*v, name);
                        }
                    }
                    let _ = writeln!(out, "{pad}switch ({subject}) {{");
                } else if let Some((tag, prefix)) = self.case_enum(ty, cases) {
                    // A switch over a plain value reads over an enum of its cases: one per
                    // switch, named after the function, with the subject cast to it.
                    let mut members = Vec::new();
                    for &raw in cases.iter().flat_map(|c| &c.values) {
                        let v = sign_extended(ty, raw);
                        let name = if v < 0 { format!("{prefix}_N{}", -v) } else { format!("{prefix}_{v}") };
                        members.push(format!("{name} = {v}"));
                        labels.insert(raw, name);
                    }
                    let _ = writeln!(out, "{pad}enum {tag} {{ {} }};", members.join(", "));
                    // The enumerators are the cases sign-extended; a narrow subject would promote
                    // zero-extended, so it passes through its signed type to meet them.
                    let subject =
                        if matches!(ty, Type::Int(8 | 16)) { format!("({}){}", signed_type(ty), parenthesized(&subject)) } else { subject };
                    let _ = writeln!(out, "{pad}switch ((enum {tag}){subject}) {{");
                } else {
                    let _ = writeln!(out, "{pad}switch ({subject}) {{");
                }
                for case in cases {
                    for v in &case.values {
                        match labels.get(v) {
                            Some(name) => {
                                let _ = writeln!(out, "{pad}case {name}:");
                            }
                            None => {
                                let _ = writeln!(out, "{pad}case {v}:");
                            }
                        }
                    }
                    if case.default {
                        let _ = writeln!(out, "{pad}default:");
                    }
                    self.stmts(out, &case.body, depth + 1);
                    if case.falls_through {
                        let _ = writeln!(out, "{pad}    /* falls through */");
                    } else if !ends_in_jump(&case.body) {
                        let _ = writeln!(out, "{pad}    break;");
                    }
                }
                let _ = writeln!(out, "{pad}}}");
            }
            Stmt::Return(None) => {
                if let Some(traced) = &self.traced {
                    let _ = writeln!(out, "{pad}ra_trace_leave(\"{traced}\");");
                }
                let _ = writeln!(out, "{pad}return;");
            }
            Stmt::Return(Some((ty, o))) => {
                let value = strip_parens(&self.operand(ty, o));
                let Some(traced) = &self.traced else {
                    let _ = writeln!(out, "{pad}return {value};");
                    return;
                };
                // The result passes through the hook, which hands it back, so it is evaluated once.
                match ty {
                    Type::Int(bits) if *bits <= 64 => {
                        let _ = writeln!(out, "{pad}return ({})ra_trace_ret(\"{traced}\", (uint64_t)({value}), {bits});", c_type(ty));
                    }
                    Type::Ptr | Type::Func(_) => {
                        let _ = writeln!(
                            out,
                            "{pad}return ({})(uintptr_t)ra_trace_ret_ptr(\"{traced}\", (uint64_t)(uintptr_t)({value}));",
                            c_type(ty)
                        );
                    }
                    Type::Double => {
                        let _ = writeln!(out, "{pad}return ra_trace_ret_f64(\"{traced}\", {value});");
                    }
                    Type::Float => {
                        let _ = writeln!(out, "{pad}return ra_trace_ret_f32(\"{traced}\", {value});");
                    }
                    _ => {
                        let _ = writeln!(out, "{pad}ra_trace_leave(\"{traced}\");");
                        let _ = writeln!(out, "{pad}return {value};");
                    }
                }
            }
            Stmt::Unreachable => {
                let _ = writeln!(out, "{pad}RA_UNREACHABLE();");
            }
        }
    }

    /// The instructions of a block, then the phi moves to its successors; `labeled` for the
    /// block's own place, where a `goto` lands, not for a copy.
    fn block(&mut self, out: &mut String, id: BlockId, depth: usize, labeled: bool) {
        let pad = "    ".repeat(depth);
        let f = self.f;
        let Some(block) = f.block(id) else { return };
        if labeled && self.labels.contains(&id) {
            let _ = writeln!(out, "{}:;", label(id, f));
        }
        for i in &block.instrs {
            match (&i.inst, i.result) {
                (Inst::Phi { .. }, Some(v)) if self.hoisted.contains(&v) => {}
                (Inst::Phi { .. }, Some(v)) => {
                    let _ = writeln!(out, "{pad}{} = {}_next;", name(f, v), name(f, v));
                }
                (Inst::Alloca { .. }, _) => {}
                (_, Some(v)) if self.skipped.contains(&v) => {}
                (Inst::Store { ty, value, ptr, align, .. }, _) => {
                    let _ = writeln!(out, "{pad}{} = {};", self.access(ty, *align, ptr), strip_parens(&self.operand(ty, value)));
                }
                (_, Some(v)) if self.inlinable(v) || self.field_pointer(v) => {}
                // A struct pointer made of an integer: one cast, from the integer.
                (Inst::Cast { cast: Cast::IntToPtr, from, value, .. }, Some(v)) if let Some(s) = self.struct_of.get(&v) => {
                    let _ = writeln!(out, "{pad}{} = (struct {s} *){};", name(f, v), self.operand(from, value));
                }
                (inst, Some(v)) => {
                    let _ = writeln!(out, "{pad}{} = {}{};", name(f, v), self.struct_cast(v), strip_parens(&self.expr(inst)));
                }
                (inst, None) => {
                    let _ = writeln!(out, "{pad}{};", self.expr(inst));
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
                if self.hoisted.contains(phi) {
                    continue;
                }
                let ty = f.value_type(*phi).clone();
                let _ = writeln!(out, "{pad}{}_next = {}{};", name(f, *phi), self.struct_cast(*phi), strip_parens(&self.operand(&ty, o)));
            }
        }
    }

    /// The cast an assignment to `v` needs when `v` is declared as a struct pointer.
    fn struct_cast(&self, v: ValueId) -> String {
        self.struct_of.get(&v).map(|s| format!("(struct {s} *)")).unwrap_or_default()
    }

    /// `phi = o`, as a `for` writes its first value.
    fn assignment(&self, phi: ValueId, o: &Operand) -> String {
        format!("{} = {}", name(self.f, phi), strip_parens(&self.operand(self.f.value_type(phi), o)))
    }

    /// The step of a `for` phi: `i++`, `i -= 2`, or `i = next`.
    fn step(&self, h: &Hoisted) -> String {
        let ty = self.f.value_type(h.phi);
        let p = name(self.f, h.phi);
        match h.add {
            Some(1) => format!("{p}++"),
            Some(k) if negative_in(ty, k) => {
                let k = -sign_extended(ty, k);
                if k == 1 { format!("{p}--") } else { format!("{p} -= {}", Self::int_constant(ty, k)) }
            }
            Some(k) => format!("{p} += {}", Self::int_constant(ty, k)),
            None if h.absorbed
                && let Some((_, _, inst)) = h.next.local().and_then(|n| self.def(n)) =>
            {
                format!("{p} = {}", strip_parens(&self.expr(inst)))
            }
            None => self.assignment(h.phi, &h.next),
        }
    }
}

/// `v` as a signed value of `ty`.
fn sign_extended(ty: &Type, v: i128) -> i128 {
    match ty {
        Type::Int(n) if *n < 128 => {
            let shift = 128 - u32::from(*n);
            (v << shift) >> shift
        }
        _ => v,
    }
}

/// An integer comparison of two constants of `bits` bits.
fn compare(cond: ICond, bits: u16, a: i128, b: i128) -> bool {
    let ty = Type::Int(bits);
    let mask = if bits >= 128 { -1 } else { (1i128 << bits) - 1 };
    let (ua, ub) = (a & mask, b & mask);
    let (sa, sb) = (sign_extended(&ty, a), sign_extended(&ty, b));
    match cond {
        ICond::Eq => ua == ub,
        ICond::Ne => ua != ub,
        ICond::Ugt => ua > ub,
        ICond::Uge => ua >= ub,
        ICond::Ult => ua < ub,
        ICond::Ule => ua <= ub,
        ICond::Sgt => sa > sb,
        ICond::Sge => sa >= sb,
        ICond::Slt => sa < sb,
        ICond::Sle => sa <= sb,
    }
}

/// Whether `v` is negative as a value of `ty`.
fn negative_in(ty: &Type, v: i128) -> bool {
    match ty {
        Type::Int(n) if *n < 128 => {
            let shift = 128 - u32::from(*n);
            ((v << shift) >> shift) < 0
        }
        _ => false,
    }
}

fn ends_in_jump(body: &[Stmt]) -> bool {
    matches!(body.last(), Some(Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Goto(_) | Stmt::Unreachable))
}

/// The C helper of an LLVM intrinsic: `llvm.ctpop.i8` is `ra_ctpop_i8`.
fn intrinsic(name: &str) -> String {
    format!("ra_{}", name.trim_start_matches("llvm.").replace('.', "_"))
}

fn short_type(ty: &Type) -> String {
    match ty {
        Type::Int(n) => format!("u{n}"),
        Type::Float => "f32".into(),
        Type::Double => "f64".into(),
        Type::Vector(n, e) => format!("v{n}{}", short_type(e)),
        other => c_type(other).replace([' ', '*'], ""),
    }
}
