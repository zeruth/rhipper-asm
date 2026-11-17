//! The C backend.
//!
//! Every IR value is a C local of its type (`uint64_t`, `double`, `uint8_t *`), assigned where
//! its instruction stands; a pure value with one use in the same block is written inline at that
//! use instead, recursively, so an expression reads as one, and so is a call whose result the next
//! arithmetic alone reads, with no load or call between. Signedness is a cast at the operation
//! that needs it, as the IR says. Phis become copies at the end of each predecessor: straight into
//! the phi where nothing reads it past the copy, through temporaries elsewhere so parallel copies
//! stay parallel, and a value a phi copies shares its variable where their lives do not cross
//! ([`crate::coalesce`]). The IR is tidied for the printer first ([`crate::tidy`]): the exception
//! checks after callees that throw nothing go. The data globals are byte arrays with address
//! words; a reference into one is an address plus an offset. The runtime header `rhipper.h` holds
//! the helpers the intrinsics map to. A constant passed to a Windows API parameter the prototype
//! table binds to an enumeration, or compared or masked with such a parameter or result, reads as
//! the SDK's name, defined at the top of the output with the value the constant has; a `switch`
//! over a plain value reads over an `enum` of its cases.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use ra_asm_ir::{
    AtomicOp, BinOp, Block, BlockId, Callee, Cast, Const, FCond, Function, ICond, Inst, Module, Operand, Terminator, Type, ValueId,
};
use ra_asm_lift::raise::prototypes;

use crate::common::{Defs, identifiers, parenthesized, strip_parens, trap_count};
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
    /// The open assumptions of a checked build: each is written as `ra_assume(<id>, <condition>)`
    /// at its site, and the unit defines the table the runtime names a failed one from.
    pub checks: Vec<Check>,
    /// The module is a 32-bit program's: an import that takes a structure laid out for 4-byte
    /// pointers is called through its wrapper in `adapters.c` ([`crate::adapters`]).
    pub guest32: bool,
}

/// One open assumption a checked build watches: what a raise step relied on and nothing proved,
/// by the function and the address it was relied on at.
#[derive(Debug, Clone)]
pub struct Check {
    /// The number `ra_assume` names it by: its row in the unit's table.
    pub id: u32,
    /// The IR name of the function it was made in.
    pub function: String,
    /// What was assumed, as the reports name it (`callee-saved`).
    pub kind: String,
    /// The address of the instruction it was made at.
    pub site: u64,
    pub watch: Watch,
}

/// What a check compares, and by that where it is written. The values are the function's own: a
/// check reads them by name, so each is computed once, where the code computes it, and a value
/// read after a call is defined in the call's block or before it.
#[derive(Debug, Clone)]
pub enum Watch {
    /// After the call at the site: each register the callee is taken to keep, by its type, the
    /// value it held before the call and the one it holds after.
    Saved(Vec<(Type, Operand, Operand)>),
    /// Before the `switch` at the site: its subject against the length of the table that was
    /// read, one past the highest case.
    Index,
    /// Before the return at the site: the stack pointer there, by its type, against the one the
    /// signature says the function returns with.
    Stack(Type, Operand, Operand),
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
    /// How the function reads; the default for a hand-written function.
    pub grade: crate::functions::Grade,
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
    /// The `extern` declaration of each data global by itself, by name, in the module's order,
    /// without C++'s linkage: what a file split by size declares of the globals it names.
    pub global_decl_of: Vec<(String, String)>,
    /// The definitions of the data globals, with their initializers.
    pub global_defs: String,
    /// The definition of each data global by itself, by name, in the module's order: what a split
    /// by module places in the file of the one module that addresses the global.
    pub globals: Vec<(String, String)>,
    /// What `global_defs` holds beyond the globals' own definitions: the pointers of the imports
    /// by ordinal and the constructor that writes the narrow address words.
    pub global_rest: String,
    /// The functions that address each data global, by the global's name and their IR names.
    pub global_users: HashMap<String, HashSet<String>>,
    /// The bytes of the data globals the source embeds (`#embed "<name>.bin"`), by file name,
    /// which go beside the source.
    pub blobs: Vec<(String, Vec<u8>)>,
    /// The prototype of each function, by IR name, without its `;`.
    pub prototype_of: HashMap<String, String>,
    pub functions: Vec<EmittedFunction>,
    pub dialect: Dialect,
    /// The checks of the options that are written nowhere, by their numbers: the site is in no
    /// function of the module, in a hand-written one, or at no call, `switch` or return of its
    /// function. Such an assumption stays unwatched.
    pub unwatched: Vec<u32>,
    /// The text of `adapters.c`, C in either dialect: the wrappers of a 32-bit program's imports
    /// that convert the structures they take; empty when none does.
    pub adapters: String,
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
const RESERVED_STEMS: [&str; 13] =
    ["program", "data", "image", "rhipper", "driver", "exports", "resources", "functions", "adapters", "con", "prn", "aux", "nul"];

/// The file of a 32-bit program's import wrappers, C in either dialect.
pub const ADAPTERS_FILE: &str = "adapters.c";

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
    /// `program.h` with the types and the imports, `data.c` with the globals, and `program_NNN.c`
    /// each with up to `max_functions` functions. A split file declares only the functions and the
    /// globals it names: a large program's every prototype in every file was most of its lines and
    /// of what the compiler read.
    pub fn files(&self, max_functions: usize) -> Vec<EmittedFile> {
        if self.functions.len() <= max_functions.max(1) {
            let mut text = String::new();
            for part in self.preamble() {
                text.push_str(part);
                text.push('\n');
            }
            let functions = Self::append_functions(&mut text, &self.functions);
            return self.with_adapters(vec![EmittedFile { name: format!("program.{}", self.dialect.extension()), text, functions }]);
        }
        let mut files = Vec::new();
        let mut header = String::from("#pragma once\n");
        for part in [&self.include, &self.types, &self.externs, &self.global_types] {
            header.push_str(part);
            header.push('\n');
        }
        files.push(EmittedFile { name: "program.h".into(), text: header, functions: Vec::new() });
        // The data declares every global it defines: in C++ a `const` global has internal linkage
        // without its `extern` declaration first.
        let mut data = String::from("#include \"program.h\"\n\n");
        data.push_str(&self.declarations(&words(&self.global_defs).collect::<HashSet<_>>(), &HashSet::new(), true));
        data.push_str(&self.global_defs);
        files.push(EmittedFile { name: format!("data.{}", self.dialect.extension()), text: data, functions: Vec::new() });
        for (index, chunk) in self.functions.chunks(max_functions.max(1)).enumerate() {
            let mut text = String::from("#include \"program.h\"\n\n");
            text.push_str(&self.file_declarations(chunk));
            let functions = Self::append_functions(&mut text, chunk);
            files.push(EmittedFile { name: format!("program_{index:03}.{}", self.dialect.extension()), text, functions });
        }
        self.with_adapters(files)
    }

    /// `files` and, when an import has a wrapper, `adapters.c`.
    fn with_adapters(&self, mut files: Vec<EmittedFile>) -> Vec<EmittedFile> {
        if !self.adapters.is_empty() {
            files.push(EmittedFile { name: ADAPTERS_FILE.into(), text: self.adapters.clone(), functions: Vec::new() });
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
        let mut seen: HashSet<&str> = HashSet::new();
        for &(_, name) in &by_address {
            if let Some(object) = object_of(name)
                && seen.insert(object)
            {
                order.push(object);
            }
        }
        if order.len() < 2 {
            return None;
        }
        // The module of the nearest placed function after each, from the end.
        let mut after: Vec<Option<&str>> = vec![None; by_address.len()];
        let mut next = None;
        for (i, &(_, name)) in by_address.iter().enumerate().rev() {
            after[i] = next;
            next = object_of(name).or(next);
        }
        let mut assigned: HashMap<&str, &str> = HashMap::new();
        let mut before = None;
        for (&(_, name), after) in by_address.iter().zip(after) {
            let own = object_of(name);
            let object = own.or_else(|| before.zip(after).and_then(|(before, after)| (before == after).then_some(before)));
            if let Some(object) = object {
                assigned.insert(name, object);
            }
            before = own.or(before);
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
        for (name, definition) in &self.globals {
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
        Some(self.with_adapters(files))
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

    /// The declarations a file of `functions` starts with: the prototypes of the functions they
    /// name and the declarations of the globals they name. In C a function of the file whose
    /// definition comes before any of them names it is declared by its definition; in C++ every
    /// one takes its prototype, whose C linkage the definition inherits.
    fn file_declarations(&self, functions: &[EmittedFunction]) -> String {
        let (mut named, mut defined) = (HashSet::new(), HashSet::new());
        for f in functions {
            let head = f.text.lines().nth(f.header_lines);
            let declares = self.prototype_of.get(&f.ir_name).is_some_and(|p| head.is_some_and(|l| l.starts_with(p.as_str())));
            if self.dialect == Dialect::C && declares && !named.contains(f.ir_name.as_str()) {
                defined.insert(f.ir_name.as_str());
            }
            named.extend(words(&f.text));
            // the runtime's `ra_args_call` macro reads `ra_args`, which the text never names
            if named.contains(crate::tidy::ARGS_CALL) {
                named.insert(ra_asm_lift::state::ARGS);
            }
        }
        self.declarations(&named, &defined, false)
    }

    /// The prototypes of the functions in `named` but not in `defined` and the declarations of
    /// the globals in `named`, or of every global with `every_global`, in the module's order and
    /// with C linkage in C++.
    fn declarations(&self, named: &HashSet<&str>, defined: &HashSet<&str>, every_global: bool) -> String {
        let mut out = String::new();
        for f in self.functions.iter().filter(|f| named.contains(f.ir_name.as_str()) && !defined.contains(f.ir_name.as_str())) {
            if let Some(prototype) = self.prototype_of.get(&f.ir_name) {
                let _ = writeln!(out, "{prototype};");
            }
        }
        for (name, declaration) in &self.global_decl_of {
            if every_global || named.contains(name.as_str()) {
                out.push_str(declaration);
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

/// The words of the C `text` that can be names, those of its comments and literals among them
/// (unlike [`identifiers`]): a name only a comment holds costs a declaration the file does not
/// need, never one it does.
fn words(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < bytes.len() {
            if !word(bytes[at]) {
                at += 1;
                continue;
            }
            let start = at;
            while at < bytes.len() && word(bytes[at]) {
                at += 1;
            }
            // A number's digits and suffix are no name.
            if !bytes[start].is_ascii_digit() {
                return Some(&text[start..at]);
            }
        }
        None
    })
}

/// Writes `module` as one C translation unit.
pub fn emit_c(module: &Module, options: &Options) -> String {
    emit_unit(module, options).text()
}

/// Writes `module` as C, in pieces: its functions and the ones it holds frozen
/// ([`ra_asm_ir::freeze`]), which thaw a run at a time.
pub fn emit_unit(module: &Module, options: &Options) -> Unit {
    let mut parts = parts(module, options);
    let mut functions = Vec::with_capacity(module.functions.len() + module.frozen.len());
    each_run(module, &mut |run| functions.extend(parts.functions(run, &module.types, options)));
    parts.unit(functions, options)
}

/// [`emit_unit`] that drops each function's IR once its source is written, so the IR of a large
/// program and its source are never both whole in memory. A frozen function is thawed only while
/// its run is read and written, so the raised module is never whole either. The module keeps its
/// types, globals and declarations, and of each function, the frozen ones among its functions
/// after its own, its name, type, attributes and address.
pub fn emit_unit_dropping(module: &mut Module, options: &Options) -> Unit {
    use ra_asm_lift::raise::timed;
    let mut parts = timed("translate: module parts", || parts(module, options));
    let functions = timed("translate: functions", || {
        let mut functions = Vec::with_capacity(module.functions.len() + module.frozen.len());
        // A run of functions at a time: its IR goes once its source is written.
        for chunk in module.functions.chunks_mut(DROP_RUN) {
            functions.extend(parts.functions(chunk, &module.types, options));
            drop_bodies(chunk);
        }
        // The frozen bytes of a run go once it is thawed, and its IR once it is written.
        let mut frozen = std::mem::take(&mut module.frozen);
        while !frozen.is_empty() {
            let run: Vec<ra_asm_ir::freeze::Frozen> = frozen.drain(..DROP_RUN.min(frozen.len())).collect();
            let mut thawed = par_map(&run, ra_asm_ir::freeze::thaw);
            drop(run);
            functions.extend(parts.functions(&thawed, &module.types, options));
            drop_bodies(&mut thawed);
            module.functions.append(&mut thawed);
        }
        functions
    });
    timed("translate: unit", || parts.unit(functions, options))
}

/// Drops the IR of the bodies of `functions`: each keeps its name, type, attributes and address.
fn drop_bodies(functions: &mut [Function]) {
    for f in functions {
        f.blocks = Vec::new();
        f.value_names = Vec::new();
        f.value_types = Vec::new();
    }
}

/// `each` over the functions of `module` a run at a time, in the order a thaw of the module leaves
/// them: its own, then the frozen ones, each run of those thawed for `each` alone.
fn each_run(module: &Module, each: &mut dyn FnMut(&[Function])) {
    for run in module.functions.chunks(DROP_RUN) {
        each(run);
    }
    for run in module.frozen.chunks(DROP_RUN) {
        each(&par_map(run, ra_asm_ir::freeze::thaw));
    }
}

/// The functions [`emit_unit_dropping`] writes before it drops their IR, and the frozen functions
/// a run thaws: enough that every thread has work, few enough that their IR and source are a
/// small part of the program's.
const DROP_RUN: usize = 4096;

/// The functions a thread of [`par_map`] takes at a time.
const BATCH: usize = 8;

/// The stack of a thread of [`par_map`]: the one `ras` runs the emit on, for the structuring and
/// the printer recurse as deep as a function nests.
const STACK: usize = 64 << 20;

/// `each` of every item of `items`, in order, on the host's threads.
fn par_map<I: Sync, T: Send>(items: &[I], each: impl Fn(&I) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(items.len().div_ceil(BATCH));
    if threads <= 1 {
        return items.iter().map(each).collect();
    }
    // The threads take small batches from a shared counter: a program's large functions sit
    // together, and a thread given a fixed run of them would finish long after the others.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (each, next) = (&each, &next);
    let mut done: Vec<(usize, T)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                std::thread::Builder::new()
                    .name("ra-emit".into())
                    .stack_size(STACK)
                    .spawn_scoped(scope, move || {
                        let mut out = Vec::new();
                        loop {
                            let start = next.fetch_add(BATCH, std::sync::atomic::Ordering::Relaxed);
                            if start >= items.len() {
                                return out;
                            }
                            let end = (start + BATCH).min(items.len());
                            out.extend(items[start..end].iter().enumerate().map(|(i, x)| (start + i, each(x))));
                        }
                    })
                    .unwrap_or_else(|e| panic!("no thread for the emit: {e}"))
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, t)| t).collect()
}

/// One function's source written alone, from no SDK name: what it wrote, the checks it watched and
/// the names it defined.
type Alone = (EmittedFunction, Vec<u32>, BTreeMap<String, String>);

/// What a unit's functions are written with, made from the whole module before the first of them:
/// the rest of the unit, and what each function's text reads of the others.
struct Parts<'o> {
    include: String,
    externs: String,
    prototype_lines: String,
    global_types: String,
    global_decls: String,
    global_decl_of: Vec<(String, String)>,
    global_defs: String,
    globals: Vec<(String, String)>,
    global_rest: String,
    global_users: HashMap<String, HashSet<String>>,
    blobs: Vec<(String, Vec<u8>)>,
    prototypes: HashMap<String, String>,
    layouts: HashMap<String, Layout>,
    method_of: HashMap<&'o str, (&'o crate::cpp::Class, crate::cpp::Method)>,
    /// The structs and the classes; the SDK's names go above them once every function is written.
    types: String,
    sdk_names: BTreeMap<String, String>,
    /// The checks the functions written so far wrote, by their numbers.
    watched: HashSet<u32>,
    adapters: String,
    /// The functions that never return with an exception in flight ([`crate::tidy::Calls::nothrow`]).
    nothrow: HashSet<String>,
}

impl Parts<'_> {
    /// The source of each of `functions`, in their order, as one after the other writes them. The
    /// functions are written apart on the host's threads, each from no SDK name; one whose defines
    /// differ from those of the functions before it is written again after them, for a constant
    /// whose name an earlier function defined at another width stays numeric.
    fn functions(&mut self, functions: &[Function], types: &[(String, Vec<Type>)], options: &Options) -> Vec<EmittedFunction> {
        let alone: Vec<Alone> = par_map(functions, |f| {
            let mut names = BTreeMap::new();
            let (emitted, written) = self.function(f, types, options, &mut names);
            (emitted, written, names)
        });
        let mut out = Vec::with_capacity(functions.len());
        for (f, (emitted, written, names)) in functions.iter().zip(alone) {
            // A function that defines a name the earlier ones did not, or the same one alike,
            // makes the choices it makes after them.
            if names.iter().all(|(name, literal)| self.sdk_names.get(name).is_none_or(|known| known == literal)) {
                for (name, literal) in names {
                    self.sdk_names.entry(name).or_insert(literal);
                }
                self.watched.extend(written);
                out.push(emitted);
                continue;
            }
            let mut names = std::mem::take(&mut self.sdk_names);
            let (emitted, written) = self.function(f, types, options, &mut names);
            self.sdk_names = names;
            self.watched.extend(written);
            out.push(emitted);
        }
        out
    }

    /// One function's source, after the functions that defined the SDK's `names`, and the checks
    /// it wrote.
    fn function(
        &self,
        f: &Function,
        types: &[(String, Vec<Type>)],
        options: &Options,
        names: &mut BTreeMap<String, String>,
    ) -> (EmittedFunction, Vec<u32>) {
        let mut text = String::new();
        let method = self.method_of.get(f.name.as_str()).map(|(c, m)| (*c, m));
        let source_name = options.names.get(&f.name).map(String::as_str);
        if options.hand.contains(&f.name) {
            let header_lines = declare_hand(&mut text, f, &self.prototypes[&f.name], source_name);
            let emitted = EmittedFunction {
                ir_name: f.name.clone(),
                address: f.address,
                header_lines,
                traps: 0,
                copies: 0,
                grade: crate::functions::Grade::default(),
                hand: true,
                text,
            };
            return (emitted, Vec::new());
        }
        // A checked build reads the values the checks name as the raise left them.
        let tidied = if options.checks.iter().any(|c| c.function == f.name) { None } else { crate::tidy::tidy(f, &self.nothrow) };
        let f = tidied.as_ref().unwrap_or(f);
        let shared = Shared { types, layouts: &self.layouts, trace: options.trace, checks: &options.checks };
        let (header_lines, copies, gotos, end, written) =
            emit_function(&mut text, f, &self.prototypes[&f.name], source_name, method, names, &shared);
        // A method's grade is its definition's: the vtable's entry after it is the engine's.
        let definition = &text[..end];
        let grade = crate::functions::Grade::of(f, gotos, casts(definition), definition.lines().count() - header_lines);
        let emitted = EmittedFunction {
            ir_name: f.name.clone(),
            address: f.address,
            header_lines,
            traps: trap_count(f),
            copies,
            grade,
            hand: false,
            text,
        };
        (emitted, written)
    }

    /// The unit of `functions`, written with these parts.
    fn unit(self, functions: Vec<EmittedFunction>, options: &Options) -> Unit {
        let mut types = self.types;
        // The SDK's names the functions read their constants as, each with the value it stands for
        // in the type it was used at; two uses at different widths keep the second numeric.
        if !self.sdk_names.is_empty() {
            let mut defines = String::from("// The SDK's names of the constants passed to and read from the Windows API.\n");
            for (name, literal) in &self.sdk_names {
                let _ = writeln!(defines, "#define {name} {literal}");
            }
            defines.push('\n');
            types.insert_str(0, &defines);
        }
        Unit {
            include: self.include,
            types,
            externs: self.externs,
            prototypes: self.prototype_lines,
            global_types: self.global_types,
            global_decls: self.global_decls,
            global_decl_of: self.global_decl_of,
            global_defs: self.global_defs,
            globals: self.globals,
            global_rest: self.global_rest,
            global_users: self.global_users,
            blobs: self.blobs,
            prototype_of: self.prototypes,
            functions,
            dialect: options.dialect,
            unwatched: options.checks.iter().map(|c| c.id).filter(|id| !self.watched.contains(id)).collect(),
            adapters: self.adapters,
        }
    }
}

/// The [`Parts`] of `module`, which read every function's IR, the frozen ones a run at a time.
fn parts<'o>(module: &Module, options: &'o Options) -> Parts<'o> {
    let mut include = String::new();
    let _ = writeln!(include, "// Generated by ras emit from {}.", module.name);
    let _ = writeln!(include, "#include \"rhipper.h\"");

    // What the parts read of the functions, gathered one function at a time: the functions
    // defined here with their prototypes, so calls in any order resolve, the uses of the data,
    // the calls, the structs addressed and, in C++, the methods.
    let mut prototypes: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::with_capacity(module.functions.len() + module.frozen.len());
    let mut uses = crate::layout::LayoutUses::new(module);
    let mut calls = crate::tidy::Calls::default();
    let data: HashSet<&str> = module.globals.iter().map(|g| g.name.as_str()).collect();
    let mut global_users: HashMap<String, HashSet<String>> = HashMap::new();
    let mut structs: HashSet<String> = HashSet::new();
    // In C++ a virtual function is a method of its class, whose declaration reads its IR.
    let classes: &'o [crate::cpp::Class] = if options.dialect == Dialect::Cpp { &options.classes } else { &[] };
    let method_names: HashSet<&str> = classes.iter().flat_map(|c| c.methods.iter().map(|m| m.ir_name.as_str())).collect();
    let mut methods: HashMap<String, Function> = HashMap::new();
    each_run(module, &mut |run| {
        for f in run {
            prototypes.insert(f.name.clone(), prototype(f));
            order.push(f.name.clone());
            uses.add(f);
            calls.add(f, &options.hand);
            note_global_users(f, &data, &mut global_users);
            note_structs(f, &mut structs);
            if method_names.contains(f.name.as_str()) && !options.hand.contains(&f.name) {
                methods.insert(f.name.clone(), f.clone());
            }
        }
    });
    let adapters = if options.guest32 { crate::adapters::adapters(module) } else { crate::adapters::Adapters::default() };
    let mut externs = String::new();
    for d in &module.declarations {
        // The intrinsics and the runtime are in rhipper.h.
        if d.name.starts_with("llvm.") || d.name.starts_with("ra_") || prototypes.contains_key(&d.name) {
            continue;
        }
        // The calls keep the import's name and reach its wrapper, which converts the structures.
        if adapters.imports.contains(&d.name) {
            let _ = writeln!(externs, "#define {} {}{}", d.name, crate::adapters::WRAPPER_PREFIX, d.name);
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
    for name in &order {
        let _ = writeln!(prototype_lines, "{};", prototypes[name]);
    }
    drop(order);
    let (mut global_types, mut global_decls, mut global_defs) = (String::new(), String::new(), String::new());
    let mut init = String::new();
    // The data sections as the code's uses type them: arrays, scalars and strings over the bytes.
    let layouts = uses.layouts();
    let mut globals = Vec::with_capacity(module.globals.len());
    let mut global_decl_of = Vec::with_capacity(module.globals.len());
    let mut blobs = Vec::new();
    for g in &module.globals {
        let layout = layouts.get(&g.name);
        global_type(&mut global_types, g, layout);
        // C23's `#embed` takes a large global's bytes from a file of them; C++ has no `#embed`
        // before C++26, so a C++ translation keeps the initializer.
        if options.dialect == Dialect::C
            && let Some(e) = embedded(g, layout)
        {
            global_decls.push_str(&e.declaration);
            global_decl_of.push((g.name.clone(), e.declaration));
            init.push_str(&e.init);
            global_defs.push_str(&e.definition);
            globals.push((g.name.clone(), e.definition));
            blobs.push((format!("{}.bin", g.name), e.bytes));
            continue;
        }
        let mut declaration = String::new();
        declare_global(&mut declaration, g, layout);
        global_decls.push_str(&declaration);
        if !declaration.is_empty() {
            global_decl_of.push((g.name.clone(), declaration));
        }
        let mut definition = String::new();
        define_global(&mut definition, &mut init, g, layout);
        global_defs.push_str(&definition);
        globals.push((g.name.clone(), definition));
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
    global_defs.push_str(&assumption_table(&options.checks, &options.names));
    let global_rest = global_defs[globals_end..].to_owned();
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
    let by_name: HashMap<&str, &Function> = methods.iter().map(|(name, f)| (name.as_str(), f)).collect();
    let method_of: HashMap<&'o str, (&'o crate::cpp::Class, crate::cpp::Method)> = classes
        .iter()
        .flat_map(|c| c.methods.iter().map(move |m| (c, m)))
        .filter_map(|(c, m)| {
            let f = by_name.get(m.ir_name.as_str())?;
            let method = crate::cpp::Method { this: m.this && !f.params.is_empty(), ..m.clone() };
            Some((m.ir_name.as_str(), (c, method)))
        })
        .collect();
    // The struct types the functions address, read from their IR before any of it goes.
    let mut types = define_types(module, &structs);
    if !classes.is_empty() {
        types.push('\n');
        types.push_str(&crate::cpp::declare(classes, &by_name, &module.types));
    }
    Parts {
        include,
        externs,
        prototype_lines,
        global_types,
        global_decls,
        global_decl_of,
        global_defs,
        globals,
        global_rest,
        global_users,
        blobs,
        prototypes,
        layouts,
        method_of,
        types,
        sdk_names: BTreeMap::new(),
        watched: HashSet::new(),
        adapters: adapters.text,
        nothrow: calls.nothrow(),
    }
}

/// The table of a checked build: one row per check, at the index that is its number, with the
/// function (by its source name where `names` has one), what was assumed and the site, which the
/// runtime prints when the check fails. A number no check has is an empty row. Nothing without
/// checks: an unchecked build names no row.
fn assumption_table(checks: &[Check], names: &HashMap<String, String>) -> String {
    let Some(last) = checks.iter().map(|c| c.id).max() else { return String::new() };
    let mut out = String::from("// The open assumptions the checked build watches, by the number `ra_assume` names each by.\n");
    out.push_str("const struct ra_assumption ra_assumptions[] = {\n");
    let mut by_id: HashMap<u32, &Check> = HashMap::new();
    for c in checks {
        by_id.entry(c.id).or_insert(c);
    }
    for id in 0..=last {
        match by_id.get(&id) {
            Some(c) => {
                let function = names.get(&c.function).unwrap_or(&c.function);
                let _ = writeln!(out, "    {{\"{function}\", \"{}\", {:#x}ull}},", c.kind, c.site);
            }
            None => out.push_str("    {0, 0, 0},\n"),
        }
    }
    out.push_str("};\n");
    out
}

/// Notes `f` among the users of each data global of `globals` it addresses, by the global's name.
fn note_global_users(f: &Function, globals: &HashSet<&str>, users: &mut HashMap<String, HashSet<String>>) {
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

/// Notes the named structs `f` addresses fields of.
fn note_structs(f: &Function, used: &mut HashSet<String>) {
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        if let Inst::Gep { base_ty: Type::Named(name), .. } = &i.inst
            && !used.contains(name.as_str())
        {
            used.insert(name.clone());
        }
    }
}

/// The named structs of `used`, the ones the functions address fields of, as C structs: a field
/// is `f<offset>`, the padding between two is `pad<offset>`. The machine model's `%State` and any
/// other type no function reaches stay out.
fn define_types(module: &Module, used: &HashSet<String>) -> String {
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
    if per_thread(&g.name) {
        return "_Thread_local ";
    }
    if g.constant && !written_at_startup(g) { "const " } else { "" }
}

/// Whether the global is the lift's machine state of one thread: the x87 unit, the SSE control
/// word, what the last call through a pointer popped and the stack words it passed. A callback the
/// host makes on a thread of its own (an audio mixer's) runs the program's code there.
fn per_thread(name: &str) -> bool {
    use ra_asm_lift::state::{ARGS, MXCSR, POPS, X87_CW, X87_STACK, X87_SW, X87_TMP, X87_TOP};
    [X87_STACK, X87_TOP, X87_CW, X87_SW, X87_TMP, MXCSR, POPS, ARGS].contains(&name)
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

/// The size from which a data global's bytes go to a file of their own that the source embeds:
/// below it the initializer reads in the source, above it the source would be the image again
/// as text, ten times its size, which the compiler then reads back byte by byte.
pub const EMBED_FROM: u64 = 64 * 1024;

/// A data global written as a file of its bytes and an `#embed` of it: its bytes, its `extern`
/// declaration, its definition, and the statements that write its address words before `main`.
pub struct Embedded {
    pub bytes: Vec<u8>,
    pub declaration: String,
    pub definition: String,
    pub init: String,
}

/// `g` as an [`Embedded`] global, when it holds [`EMBED_FROM`] bytes or more of an initializer: the
/// byte runs and the integer words as they are, each address word zero in the bytes and written
/// by a statement before `main`, whatever its width. A global of a structure type is a union of
/// its bytes and the structure under the global's name, so the code addresses its fields as it
/// does when the source initializes them.
fn embedded(g: &ra_asm_ir::Global, layout: Option<&Layout>) -> Option<Embedded> {
    let (bytes, words): (Vec<u8>, Vec<(u64, Type, Const)>) = match (layout, &g.ty, &g.init) {
        (Some(layout), _, Some(init)) if !matches!(init, Const::Zero) => {
            let words = layout.words.iter().map(|(offset, (ty, item))| (*offset, ty.clone(), item.clone())).collect();
            (layout.bytes.clone(), words)
        }
        (None, Type::Array(_, e), Some(Const::Bytes(bytes))) if **e == Type::I8 => (bytes.clone(), Vec::new()),
        (None, Type::Packed(fields), Some(Const::Struct(items))) => {
            let mut bytes = Vec::new();
            let mut words = Vec::new();
            for (item, field) in items.iter().zip(fields) {
                match item {
                    Const::Bytes(run) => bytes.extend_from_slice(run),
                    other => {
                        words.push((bytes.len() as u64, field.clone(), other.clone()));
                        bytes.resize(bytes.len() + field.size(8).unwrap_or(0) as usize, 0);
                    }
                }
            }
            (bytes, words)
        }
        _ => return None,
    };
    if (bytes.len() as u64) < EMBED_FROM {
        return None;
    }
    let mut bytes = bytes;
    let mut init = String::new();
    let mut addresses = false;
    for (offset, field, item) in &words {
        let at = *offset as usize;
        let size = field.size(8).unwrap_or(0) as usize;
        let target = match item {
            Const::Int(v) => {
                bytes[at..at + size].copy_from_slice(&v.to_le_bytes()[..size]);
                continue;
            }
            Const::Address { global, offset } => format!("((uint8_t *)&{global} + {offset})"),
            Const::Global(name) => format!("&{name}"),
            _ => continue,
        };
        bytes[at..at + size].fill(0);
        addresses = true;
        let _ = writeln!(
            init,
            "    *({} *)((uint8_t *)&{} + {offset}) = ({})(uintptr_t){target};",
            access_type(field, 1),
            g.name,
            c_type(field)
        );
    }
    // A global the statements write cannot be `const`: the compiler would fold the zero it sees.
    let storage = if g.constant && !addresses { "const " } else { "" };
    let n = bytes.len();
    let file = format!("{}.bin", g.name);
    let structured = layout.is_some() || matches!(g.ty, Type::Packed(_));
    let (declaration, definition) = if structured {
        (
            format!(
                "union {name}_u {{ uint8_t bytes[{n}]; struct {name}_t value; }};\nextern {storage}union {name}_u {name}_storage;\n#define {name} ({name}_storage.value)\n",
                name = g.name
            ),
            format!("{storage}union {name}_u {name}_storage RA_ALIGNED(16) = {{ .bytes = {{\n#embed \"{file}\"\n}} }};\n", name = g.name),
        )
    } else {
        (
            format!("extern {storage}uint8_t {}[{n}];\n", g.name),
            format!("{storage}uint8_t {}[{n}] RA_ALIGNED(16) = {{\n#embed \"{file}\"\n}};\n", g.name),
        )
    };
    Some(Embedded { bytes, declaration, definition, init })
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
            let _ = write!(out, "{storage}uint8_t {}[{n}] RA_ALIGNED(16) = ", g.name);
            emit_bytes(out, bytes, *n);
            let _ = writeln!(out, ";");
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
                        let size = if let Type::Array(n, _) = field { *n } else { bytes.len() as u64 };
                        emit_bytes(out, bytes, size);
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
/// decimal, an array's trailing zeros left to the zero fill. A zero-initialized section has no
/// initializer.
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
            Elem::Bytes => emit_bytes(out, &layout.bytes[field.offset as usize..field.end() as usize], field.end() - field.offset),
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
            Elem::Value(ty) if field.array => {
                // No comma after the last value: the formatter writes a list that ends in one a
                // value a line.
                let count = (0..field.count).rev().find(|&n| layout.element(field, n).iter().any(|&b| b != 0)).map_or(0, |n| n + 1);
                if count == 0 {
                    out.push_str("{0}");
                    continue;
                }
                out.push('{');
                for n in 0..count {
                    out.push_str(if n % 16 == 0 { "\n    " } else { " " });
                    out.push_str(&value_literal(ty, layout.element(field, n)));
                    if n + 1 < count {
                        out.push(',');
                    }
                }
                out.push_str("\n}");
            }
            Elem::Value(ty) => out.push_str(&value_literal(ty, layout.element(field, 0))),
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

/// The text width of a line of a byte array's string literal.
const LITERAL_LINE: usize = 96;

/// The braced initializer of a byte array of `size` bytes from `bytes`: a string literal a line
/// of [`LITERAL_LINE`] at a time, its trailing zeros left to the zero fill. A literal needs room
/// for its NUL (C++ requires it, clang warns in C), so an array whose last byte is not zero is the
/// numbers, 32 a line and no comma after the last: the formatter writes a list that ends in one
/// a value a line.
fn emit_bytes(out: &mut String, bytes: &[u8], size: u64) {
    let bytes = &bytes[..bytes.iter().rposition(|&b| b != 0).map_or(0, |n| n + 1)];
    if bytes.is_empty() {
        out.push_str("{0}");
        return;
    }
    out.push('{');
    if bytes.len() as u64 >= size {
        for (i, b) in bytes.iter().enumerate() {
            out.push_str(if i % 32 == 0 { "\n    " } else { " " });
            let _ = write!(out, "{b}");
            if i + 1 < bytes.len() {
                out.push(',');
            }
        }
        out.push_str("\n}");
        return;
    }
    let mut line = 0;
    let mut previous = 0u8;
    for (i, &b) in bytes.iter().enumerate() {
        if i == 0 || out.len() - line >= LITERAL_LINE {
            if i > 0 {
                out.push('"');
            }
            out.push_str("\n    \"");
            line = out.len();
        }
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            // An escaped `?` after a `?`, so no trigraph forms.
            b'?' if previous == b'?' => out.push_str("\\?"),
            b' '..=b'~' => out.push(b as char),
            // An octal escape takes up to three digits: a short one only when no digit follows.
            _ if bytes.get(i + 1).is_some_and(|n| (b'0'..=b'7').contains(n)) => {
                let _ = write!(out, "\\{b:03o}");
            }
            _ => {
                let _ = write!(out, "\\{b:o}");
            }
        }
        previous = b;
    }
    out.push_str("\"\n}");
}

/// What every function of a unit is written with: the module's and the options' parts.
#[derive(Clone, Copy)]
struct Shared<'a> {
    /// The named structs of the module.
    types: &'a [(String, Vec<Type>)],
    /// The layouts of the data globals, by name.
    layouts: &'a HashMap<String, Layout>,
    /// Whether the trace hooks are written.
    trace: bool,
    /// The checks of the whole unit; a function writes its own.
    checks: &'a [Check],
}

/// Writes one function, with one comment line above it naming it: the source name when the image
/// gives one, and its address. As a `method` of a C++ class, the body is the method's definition,
/// its first parameter recovered from `this`, and the function under its own name follows as the
/// vtable's entry. Each of the function's own checks is written at its site. Returns the
/// number of lines above the definition, the number of blocks written twice, the number of
/// `goto`s, where in `out` the definition ends and the numbers of the checks that were written.
fn emit_function(
    out: &mut String,
    f: &Function,
    prototype: &str,
    source_name: Option<&str>,
    method: Option<(&crate::cpp::Class, &crate::cpp::Method)>,
    sdk_names: &mut BTreeMap<String, String>,
    shared: &Shared<'_>,
) -> (usize, usize, usize, usize, Vec<u32>) {
    let Shared { types, layouts, trace, .. } = *shared;
    let checks: Vec<&Check> = shared.checks.iter().filter(|c| c.function == f.name).collect();
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
    let head = match method {
        Some((class, m)) => crate::cpp::method_prototype(f, class, m, true),
        None => prototype.to_owned(),
    };
    // The entry hook, on the definition line rather than one of its own in every function: nothing,
    // unless the port is built with RA_TRACE, when it names the function.
    let _ = writeln!(out, "{head} {{ RA_ENTER(\"{}\");", f.name);
    // The trace hooks name the function as the table does, so the reference's trace, recorded by
    // address and named through the table, reads the same.
    let traced = trace.then(|| source_name.unwrap_or(&f.name).to_owned());
    let mut entry = String::new();
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
        let _ = writeln!(entry, "    ra_trace_enter(\"{traced}\", \"{kinds}\"{args});");
    }
    let graph = Graph::of(f);
    let structured = structure(f, &graph);
    let mut e = Emitter::new(f, &structured.labels, types, layouts, std::mem::take(sdk_names), traced, checks);
    e.plan_loops(&structured.body, &graph);
    e.plan_remat();
    e.plan_direct();
    e.plan_coalesce();
    let locals = e.declare_locals();
    let mut body = String::new();
    e.stmts(&mut body, &structured.body, 1);
    let written: Vec<u32> = e.written.iter().map(|n| e.checks[*n].id).collect();
    *sdk_names = e.names.into_inner();
    let declarations = prune_locals(&locals, &mut body);
    // A parameter the body does not read is one the convention passes and the function ignores.
    let read: HashSet<&str> =
        identifiers(&entry).into_iter().chain(identifiers(&body)).filter(|(_, assigned)| !assigned).map(|(n, _)| n).collect();
    let this = method.is_some_and(|(_, m)| m.this);
    if this && read.contains(name(f, f.params[0]).as_str()) {
        let ty = &f.ty.params[0];
        let _ = writeln!(out, "    {} = ({})(uintptr_t)this;", declaration(ty, &name(f, f.params[0])), c_type(ty));
    }
    for p in f.params.iter().skip(usize::from(this)) {
        let p = name(f, *p);
        if !read.contains(p.as_str()) {
            let _ = writeln!(out, "    (void){p};");
        }
    }
    out.push_str(&entry);
    out.push_str(&declarations);
    out.push_str(&body);
    let _ = writeln!(out, "}}");
    let end = out.len();
    if let Some((class, m)) = method {
        out.push('\n');
        crate::cpp::trampoline(out, f, class, m, prototype);
    }
    (header_lines, structured.copies(), structured.gotos(), end, written)
}

/// A local of a function's C, by the names it declares.
enum Local {
    /// One name, declared in `text`: one line, or an allocation's two, its storage and the
    /// pointer to it.
    One { name: String, text: String },
    /// A phi and its next value, declared together: `lead` is the type up to the first name,
    /// `star` what the second name takes of it.
    Phi { lead: String, star: &'static str, name: String },
    /// An allocation: its storage `<name>_mem`, which an access of its own type reads by name,
    /// and the pointer `<name>` to it, each declared when the body names it.
    Alloca { name: String, storage: String, pointer: String },
}

/// The declarations of the locals `body` reads, with `body` rid of the assignments to the others:
/// a value nothing reads, such as a call's unused result, is not declared, and its assignment
/// keeps only what may do something, the call. To a fixpoint: a phi nothing reads leaves its
/// next value unread too.
fn prune_locals(locals: &[Local], body: &mut String) -> String {
    let declared: Vec<String> = locals
        .iter()
        .flat_map(|l| match l {
            Local::One { name, .. } | Local::Alloca { name, .. } => vec![name.clone()],
            Local::Phi { name, .. } => vec![name.clone(), format!("{name}_next")],
        })
        .collect();
    // An allocation's storage, which its own accesses name: dead with its pointer, when nothing
    // reads it by name either; its stores go then.
    let storage: Vec<(String, String)> = locals
        .iter()
        .filter_map(|l| match l {
            Local::Alloca { name, .. } => Some((name.clone(), format!("{name}_mem"))),
            _ => None,
        })
        .collect();
    let mut dead: HashSet<String> = HashSet::new();
    loop {
        let read: HashSet<&str> = identifiers(body).into_iter().filter(|(_, assigned)| !assigned).map(|(n, _)| n).collect();
        let mut unread: Vec<&String> = declared.iter().filter(|n| !dead.contains(*n) && !read.contains(n.as_str())).collect();
        let pointer_gone = |p: &String, unread: &[&String]| dead.contains(p) || unread.contains(&p);
        let gone: Vec<&String> = storage
            .iter()
            .filter(|(p, m)| !dead.contains(m) && !read.contains(m.as_str()) && pointer_gone(p, &unread))
            .map(|(_, m)| m)
            .collect();
        unread.extend(gone);
        if unread.is_empty() {
            break;
        }
        let by_name = Unread::of(&unread);
        let mut kept = String::with_capacity(body.len());
        for line in body.lines() {
            if let Some(line) = unassigned(line, &by_name) {
                kept.push_str(&line);
                kept.push('\n');
            }
        }
        dead.extend(unread.into_iter().cloned());
        *body = kept;
    }
    // The storage of an allocation is named wherever an access of its own type reads or writes it.
    let named: HashSet<&str> = if locals.iter().any(|l| matches!(l, Local::Alloca { .. })) {
        identifiers(body).into_iter().map(|(n, _)| n).filter(|n| n.ends_with("_mem")).collect()
    } else {
        HashSet::new()
    };
    let mut out = String::new();
    for l in locals {
        match l {
            Local::One { name, text } if !dead.contains(name) => out.push_str(text),
            Local::One { .. } => {}
            Local::Alloca { name, storage, pointer } => {
                let pointed = !dead.contains(name);
                if pointed || named.contains(format!("{name}_mem").as_str()) {
                    out.push_str(storage);
                }
                if pointed {
                    out.push_str(pointer);
                }
            }
            Local::Phi { lead, star, name } => {
                let next = format!("{name}_next");
                match (dead.contains(name), dead.contains(&next)) {
                    (false, false) => {
                        let _ = writeln!(out, "    {lead}{name}, {star}{next};");
                    }
                    (false, true) => {
                        let _ = writeln!(out, "    {lead}{name};");
                    }
                    (true, false) => {
                        let _ = writeln!(out, "    {lead}{next};");
                    }
                    (true, true) => {}
                }
            }
        }
    }
    group_declarations(&out)
}

/// The widest line a group of declarations takes.
const DECLARATION_WIDTH: usize = 100;

/// `declarations` with the plain ones of each type on shared lines, `uint32_t a, b, c;`, in the
/// order each type first comes; a declaration with an initializer, an attribute or brackets keeps
/// its own line, ahead of them.
fn group_declarations(declarations: &str) -> String {
    let mut out = String::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for line in declarations.lines() {
        let body = line.strip_prefix("    ").and_then(|b| b.strip_suffix(';'));
        let plain = body.filter(|b| !b.contains(['=', '(', '[', '{']) && !b.starts_with(' '));
        let Some((ty, first)) = plain.and_then(|b| {
            let (head, rest) = b.split_once(", ").map_or((b, None), |(h, r)| (h, Some(r)));
            let (ty, first) = head.rsplit_once(' ')?;
            Some((ty, std::iter::once(first).chain(rest.into_iter().flat_map(|r| r.split(", "))).collect::<Vec<_>>()))
        }) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let names = first.into_iter().map(str::to_owned);
        match groups.iter_mut().find(|(t, _)| t == ty) {
            Some((_, list)) => list.extend(names),
            None => groups.push((ty.to_owned(), names.collect())),
        }
    }
    for (ty, names) in groups {
        let mut line = format!("    {ty} ");
        let start = line.len();
        for name in names {
            if line.len() > start && line.len() + name.len() + 3 > DECLARATION_WIDTH {
                line.truncate(line.len() - 2);
                let _ = writeln!(out, "{line};");
                line = format!("    {ty} ");
            }
            line.push_str(&name);
            line.push_str(", ");
        }
        line.truncate(line.len() - 2);
        let _ = writeln!(out, "{line};");
    }
    out
}

/// The names [`unassigned`] drops the assignments of: the identifiers, which a line's leading
/// word is looked up in, and any other name, tried at each line. A try of every name at every
/// line made the pruning quadratic in a function's length.
struct Unread<'n> {
    words: HashSet<&'n str>,
    others: Vec<&'n str>,
}

impl<'n> Unread<'n> {
    fn of(names: &[&'n String]) -> Self {
        let (words, others): (Vec<&str>, Vec<&str>) =
            names.iter().map(|&n| n.as_str()).partition(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        Self { words: words.into_iter().collect(), others }
    }
}

/// `line` without its assignment when it assigns one of `unread`: nothing for a copy of a name or
/// a constant, the bare call for a call, and the value cast to `void` for anything else, which may
/// read memory as the machine did.
fn unassigned(line: &str, unread: &Unread<'_>) -> Option<String> {
    let value = line.trim_start();
    let pad = &line[..line.len() - value.len()];
    let assigned = |n: &str| value.strip_prefix(n)?.strip_prefix(" = ")?.strip_suffix(';');
    // Only the line's leading word can be an identifier it assigns.
    let word = &value[..value.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(value.len())];
    let by_word = if unread.words.contains(word) { assigned(word) } else { None };
    let Some(rhs) = by_word.or_else(|| unread.others.iter().find_map(|n| assigned(n))) else {
        return Some(line.to_owned());
    };
    if rhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let callee = rhs.find('(').map_or(rhs, |at| &rhs[..at]);
    let args = &rhs[callee.len()..];
    let call =
        !callee.is_empty() && callee.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && strip_parens(args).len() + 2 == args.len();
    Some(if call { format!("{pad}{rhs};") } else { format!("{pad}(void)({rhs});") })
}

/// The casts a function's C writes: each parenthesized type before an operand, but the `(void)`
/// of a value read for nothing.
fn casts(text: &str) -> usize {
    let mut count = 0;
    for (at, _) in text.match_indices('(') {
        let before = text[..at].chars().next_back();
        // A call's or a function type's parameter list is no cast.
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == ')') {
            continue;
        }
        let Some(end) = text[at + 1..].find(')') else { continue };
        let inner = text[at + 1..at + 1 + end].trim_end_matches([' ', '*']);
        let word = inner.strip_prefix("struct ").or_else(|| inner.strip_prefix("enum ")).unwrap_or(inner);
        let plain = word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let typed = inner != word
            || (plain && (word.ends_with("_t") || word.starts_with("ra_v") || word == "ra_u128"))
            || ["float", "double", "long double"].contains(&word);
        if typed {
            count += 1;
        }
    }
    count
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
    /// The checks of the function, in the order of the options.
    checks: Vec<&'a Check>,
    /// The values the checks read: each keeps its name, so a check reads what the code computed
    /// where it computed it, not an expression evaluated again at the check.
    watched: HashSet<ValueId>,
    /// The checks written after an instruction, by block: the instruction's index and the check's.
    after: HashMap<BlockId, Vec<(usize, usize)>>,
    /// The checks written before the `switch` or the return a block ends in, by block.
    at_end: HashMap<BlockId, Vec<usize>>,
    /// The checks written so far, by index.
    written: BTreeSet<usize>,
    /// The phis each way in assigns at once, with no `_next` and no copy at the head: no other
    /// move of a predecessor, nor its branch, reads the phi, and no other way out of it does.
    direct: HashSet<ValueId>,
    /// The values that share a direct phi's variable, to the phi it is named after.
    rename: HashMap<ValueId, ValueId>,
    /// The allocations an access of their own type reads by name, with that type.
    named_allocas: HashMap<ValueId, Type>,
    /// The cheap values written at each of their uses ([`Self::plan_remat`]).
    remat: HashSet<ValueId>,
}

/// Where the checks of a function are written, and the values they read (see [`place_checks`]).
#[derive(Default)]
struct Placement {
    watched: HashSet<ValueId>,
    after: HashMap<BlockId, Vec<(usize, usize)>>,
    at_end: HashMap<BlockId, Vec<usize>>,
}

/// Places each of `checks` in `f`. A check of the saved registers follows the call at its site,
/// in every block that calls there (the arms of a dispatch each do): after the last instruction
/// of the site, and after the values it reads that the block defines later. A check of a table's
/// index goes before the `switch` of the block that holds an instruction of the site, or whose
/// default does: the lift's trap there names the jump. A check of the stack pointer goes before
/// the return of the block that holds an instruction of the site or, where the raise left none,
/// of the block the site lies in by its address. A check with no such place is written nowhere.
fn place_checks(f: &Function, defs: &Defs<'_>, checks: &[&Check]) -> Placement {
    let mut placed = Placement::default();
    let holds = |b: &Block, site: u64| b.instrs.iter().any(|i| i.address == Some(site));
    let returns = |b: &&Block| matches!(b.terminator, Terminator::Ret(_));
    for (n, check) in checks.iter().enumerate() {
        let site = check.site;
        match &check.watch {
            Watch::Saved(pairs) => {
                let read: Vec<ValueId> = pairs.iter().flat_map(|(_, was, is)| [was, is]).filter_map(Operand::local).collect();
                for b in &f.blocks {
                    if !b.instrs.iter().any(|i| i.address == Some(site) && matches!(i.inst, Inst::Call { .. })) {
                        continue;
                    }
                    let Some(mut at) = b.instrs.iter().rposition(|i| i.address == Some(site)) else { continue };
                    for v in &read {
                        if let Some((block, index, _)) = defs.def(*v)
                            && block.id == b.id
                        {
                            at = at.max(index);
                        }
                    }
                    placed.after.entry(b.id).or_default().push((at, n));
                }
                placed.watched.extend(read);
            }
            Watch::Index => {
                for b in &f.blocks {
                    let Terminator::Switch { default, .. } = &b.terminator else { continue };
                    if holds(b, site) || f.block(*default).is_some_and(|d| holds(d, site)) {
                        placed.at_end.entry(b.id).or_default().push(n);
                    }
                }
            }
            Watch::Stack(_, at, expected) => {
                placed.watched.extend([at, expected].into_iter().filter_map(Operand::local));
                let mut blocks: Vec<&Block> = f.blocks.iter().filter(returns).filter(|b| holds(b, site)).collect();
                if blocks.is_empty() {
                    let inside = f.blocks.iter().filter(|b| b.address.is_some_and(|a| a <= site)).max_by_key(|b| b.address);
                    blocks.extend(inside.filter(returns));
                }
                for b in blocks {
                    placed.at_end.entry(b.id).or_default().push(n);
                }
            }
        }
    }
    placed
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
    /// The value from outside the loop the address adds to the index; none when the index is the
    /// address.
    base: Option<Operand>,
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
        checks: Vec<&'a Check>,
    ) -> Self {
        let defs = Defs::with_moves(f);
        let Placement { watched, after, at_end } = place_checks(f, &defs, &checks);
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
            checks,
            watched,
            after,
            at_end,
            written: BTreeSet::new(),
            direct: HashSet::new(),
            rename: HashMap::new(),
            named_allocas: named_allocas(f),
            remat: HashSet::new(),
        }
    }

    /// Decides the phis assigned directly, once the loops are planned: a phi is when at each
    /// predecessor no other move's operand and not the branch reads it as written, the
    /// predecessor moves no phi a `for` carries (whose step runs after the moves), and the phi is
    /// not live into another successor of it. A checked function keeps every `_next`: a check
    /// at a block's end reads the phis after the moves.
    fn plan_direct(&mut self) {
        if !self.checks.is_empty() {
            return;
        }
        let f = self.f;
        let mut direct: HashSet<ValueId> = HashSet::new();
        let mut phi_block: HashMap<ValueId, BlockId> = HashMap::new();
        for b in &f.blocks {
            for i in &b.instrs {
                if let (Some(p), Inst::Phi { .. }) = (i.result, &i.inst)
                    && !self.hoisted.contains(&p)
                {
                    direct.insert(p);
                    phi_block.insert(p, b.id);
                }
            }
        }
        if direct.is_empty() {
            return;
        }
        let reads = |o: &Operand, p: ValueId| match o {
            Operand::Local(v) if *v == p => true,
            Operand::Local(v) => (self.inlinable(*v) || self.field_pointer(*v)) && self.reads_any(*v, &[p]),
            _ => false,
        };
        // The phis live into some block of more than one way out, found when first asked.
        let mut live: HashMap<ValueId, HashSet<BlockId>> = HashMap::new();
        let preds = f.predecessors();
        let mut readers: Option<HashMap<ValueId, Vec<BlockId>>> = None;
        for b in &f.blocks {
            let Some(moves) = self.defs.phi_moves.get(&b.id) else { continue };
            if moves.iter().any(|(q, _)| self.hoisted.contains(q)) {
                for (q, _) in moves {
                    direct.remove(q);
                }
                continue;
            }
            let mut successors = b.terminator.successors();
            successors.sort_unstable_by_key(|s| s.0);
            successors.dedup();
            for (p, _) in moves {
                if !direct.contains(p) {
                    continue;
                }
                let read_by_move = moves.iter().any(|(q, o)| q != p && reads(o, *p));
                let read_by_branch = b.terminator.operands().into_iter().any(|o| reads(o, *p));
                let home = phi_block[p];
                let escapes = successors.len() > 1 && {
                    let blocks = live.entry(*p).or_insert_with(|| {
                        let readers = readers.get_or_insert_with(|| reading_blocks(f));
                        live_blocks(*p, home, &preds, readers)
                    });
                    successors.iter().any(|s| *s != home && blocks.contains(s))
                };
                if read_by_move || read_by_branch || escapes {
                    direct.remove(p);
                }
            }
        }
        self.direct = direct;
    }

    /// Decides the values that share a direct phi's variable ([`crate::coalesce`]). A function
    /// with a `for` or a `strlen` loop keeps its variables apart: their steps are written away
    /// from the instructions they come from.
    fn plan_coalesce(&mut self) {
        if !self.checks.is_empty() || !self.for_loops.is_empty() || !self.strlen_loops.is_empty() || self.direct.is_empty() {
            return;
        }
        let f = self.f;
        let inline = |v: ValueId| self.inlinable(v) || self.field_pointer(v);
        // A value written at each use reads its base there: the base keeps its own variable.
        let bases: HashSet<ValueId> = self.remat.iter().filter_map(|v| self.def(*v)).flat_map(|(_, _, inst)| inst.uses()).collect();
        let keep = |v: ValueId| {
            bases.contains(&v)
                || f.params.contains(&v)
                || self.struct_of.contains_key(&v)
                || self.watched.contains(&v)
                || self.hoisted.contains(&v)
                || self.skipped.contains(&v)
                || self.alias.contains_key(&v)
                || matches!(
                    self.def(v),
                    Some((_, _, Inst::Alloca { .. } | Inst::ICmp { ty: Type::Vector(..), .. } | Inst::FCmp { ty: Type::Vector(..), .. }))
                )
        };
        let printer = crate::coalesce::Printer { f, direct: &self.direct, inline: &inline, keep: &keep, moves: &self.defs.phi_moves };
        self.rename = crate::coalesce::coalesce(&printer);
    }

    /// The C variable of `v`: its own, or the phi's whose variable it shares.
    fn var(&self, v: ValueId) -> String {
        name(self.f, self.rename.get(&v).copied().unwrap_or(v))
    }

    /// Whether `o` is the value already in `phi`'s variable.
    fn holds(&self, phi: ValueId, o: &Operand) -> bool {
        let rep = |v: ValueId| self.rename.get(&v).copied().unwrap_or(v);
        matches!(o, Operand::Local(v) if rep(*v) == rep(phi))
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
                Stmt::Tail { body, .. } => self.plan_loops(body, g),
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
        // A `for` tests before its first round: a call in the test would run once more.
        if !self.inlinable(c) || self.reads_any(c, &names) || self.defs.calls(c) {
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
        // A `for` tests before its first round, so to a C compiler a variable the body assigns
        // may be unset after the loop, though the condition holds on entry: a loop whose body
        // defines a value the code after it reads stays a `do ... while`, which says the body
        // runs. The header's phis are the `for`'s own, which its first clause assigns, and a phi's
        // next-round value reads after the loop as the phi itself.
        let nexts: Vec<ValueId> = phis.iter().filter_map(|(_, _, next)| next.local()).collect();
        let escapes = self.f.blocks.iter().filter(|b| !members.contains(&b.id)).any(|b| {
            let read =
                |o: &Operand| o.local().is_some_and(|v| !names.contains(&v) && !nexts.contains(&v) && defined_in(v) && !self.inlinable(v));
            b.instrs.iter().any(|i| match &i.inst {
                Inst::Phi { incoming, .. } => incoming.iter().any(|(o, from)| !members.contains(from) && read(o)),
                inst => inst.operands().into_iter().any(read),
            }) || b.terminator.operands().into_iter().any(read)
        });
        if escapes {
            return None;
        }
        // A next-round value written inline at its move is written in the step, after the body:
        // only one of the latch that holds to its end and reads no other phi moves there.
        for (phi, _, next) in &phis {
            let Some(n) = next.local().filter(|n| defined_in(*n) && self.inlinable(*n)) else { continue };
            let stepped = matches!(self.def(n), Some((_, _, Inst::Bin { op: BinOp::Add | BinOp::Sub, lhs: Operand::Local(base), rhs: Operand::Int(_), .. })) if base == phi);
            let others: Vec<ValueId> = names.iter().copied().filter(|p| p != phi).collect();
            let in_latch = self.def(n).is_some_and(|(b, ..)| b.id == latch);
            if !stepped && (!in_latch || !self.defs.holds_to_block_end(n) || self.reads_any(n, &others)) {
                return None;
            }
        }
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
                // The loop becomes one call: a call inside it would run once.
                (inst, Some(v)) if self.inlinable(v) && !inst.has_side_effects() => {}
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
        // No base: the index is the address itself, a pointer walked to its string's end.
        if base.is_none() && ty != *self.f.value_type(phi) {
            return None;
        }
        Some(StrlenLoop { phi, init, next, base, ty, offset: offset & mask, carried })
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
        let mut start = s.base.as_ref().map(|b| self.operand(&s.ty, b));
        let mut offset = sign_extended(&s.ty, s.offset);
        match &s.init {
            Operand::Int(k) => offset = sign_extended(&s.ty, offset + k),
            init => {
                let init = self.operand(ty, init);
                start = Some(match start {
                    Some(base) => format!("{base} + {init}"),
                    None => init,
                });
            }
        }
        let mut start = start.unwrap_or_else(|| {
            let at = Self::int_constant(&s.ty, offset);
            offset = 0;
            at
        });
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
        self.remat.contains(&v)
            || matches!(self.def(v), Some((_, _, Inst::Gep { base_ty, indices, .. })) if crate::common::field_of(self.types, base_ty, indices).is_some())
    }

    /// Decides the values written at each of their few uses instead of named: a constant step
    /// from a value that keeps its variable all its life (no phi, no shared variable), as a cast
    /// or an offset (`frame - 48`), which reads at its use what it read where it stands.
    fn plan_remat(&mut self) {
        if !self.checks.is_empty() {
            return;
        }
        const MOST_USES: usize = 6;
        let f = self.f;
        let entry = f.blocks[0].id;
        // The entry runs once when nothing jumps back into it: what it defines holds for good.
        let once = !f.blocks.iter().any(|b| b.terminator.successors().contains(&entry));
        // Where each value is read: a value read only in its own block, after it, reads its
        // base before anything could assign the base again.
        let mut read_in: HashMap<ValueId, HashSet<BlockId>> = HashMap::new();
        for b in &f.blocks {
            for i in &b.instrs {
                match &i.inst {
                    Inst::Phi { incoming, .. } => {
                        for (o, from) in incoming {
                            if let Operand::Local(v) = o {
                                read_in.entry(*v).or_default().insert(*from);
                            }
                        }
                    }
                    inst => {
                        for v in inst.uses() {
                            read_in.entry(v).or_default().insert(b.id);
                        }
                    }
                }
            }
            for v in b.terminator.uses() {
                read_in.entry(v).or_default().insert(b.id);
            }
        }
        let mut remat: HashSet<ValueId> = HashSet::new();
        // Whether the base `o` holds the same value wherever `v`, defined in `home`, is read.
        let steady = |o: &Operand, home: BlockId, v: ValueId, remat: &HashSet<ValueId>| match o {
            Operand::Global(_) | Operand::Address { .. } => true,
            Operand::Local(b) if f.params.contains(b) => true,
            // A base written inline (a load, a call) would run again at each use.
            Operand::Local(b) if self.inlinable(*b) || (self.field_pointer(*b) && !remat.contains(b)) => false,
            Operand::Local(b) => match self.def(*b) {
                Some((_, _, Inst::Phi { .. })) | None => false,
                Some((block, _, _)) if once && block.id == entry => true,
                Some(_) => !remat.contains(b) && read_in.get(&v).is_some_and(|r| r.iter().all(|x| *x == home)),
            },
            _ => false,
        };
        for block in &f.blocks {
            for i in &block.instrs {
                let Some(v) = i.result else { continue };
                if !(2..=MOST_USES).contains(&self.defs.uses(v))
                    || self.inlinable(v)
                    || self.struct_of.contains_key(&v)
                    || self.watched.contains(&v)
                    || self.hoisted.contains(&v)
                    || self.skipped.contains(&v)
                    || self.alias.contains_key(&v)
                {
                    continue;
                }
                let cheap = match &i.inst {
                    Inst::Bin { op: BinOp::Add | BinOp::Sub, ty: Type::Int(_), lhs, rhs: Operand::Int(_) } => {
                        steady(lhs, block.id, v, &remat)
                    }
                    Inst::Gep { ptr, indices, .. } => {
                        indices.iter().all(|(_, k)| matches!(k, Operand::Int(_))) && steady(ptr, block.id, v, &remat)
                    }
                    Inst::Cast { cast: Cast::ZExt | Cast::Trunc | Cast::IntToPtr | Cast::PtrToInt, value, .. } => {
                        steady(value, block.id, v, &remat)
                    }
                    _ => false,
                };
                if cheap {
                    remat.insert(v);
                }
            }
        }
        self.remat = remat;
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
        // An access at address zero is the program's deliberate fault (an assertion that stops it
        // by writing to null): C may delete a plain one, so it is volatile and faults as it did.
        if self.null_address(ptr) {
            return format!("*(volatile {} *)0", access_type(ty, align));
        }
        // An access of an allocation's own scalar type is its storage by name.
        if let Operand::Local(v) = ptr
            && self.named_allocas.get(v) == Some(ty)
        {
            return format!("{}_mem", name(self.f, *v));
        }
        match self.field_access(ptr) {
            Some((field, field_ty)) if field_ty == ty => field,
            Some((field, _)) => format!("*({} *)&{field}", access_type(ty, align)),
            None => {
                if let Some(access) = self.section_access(ty, ptr) {
                    access
                } else {
                    let address = self.int_address(ptr).map_or_else(|| self.operand(&Type::Ptr, ptr), |int| strip_parens(&int));
                    format!("*({} *)({address})", access_type(ty, align))
                }
            }
        }
    }

    /// The integer an address operand is made of, when it is an integer made a pointer in place:
    /// the access casts it to its own pointer type at once, with no byte pointer between.
    fn int_address(&self, ptr: &Operand) -> Option<String> {
        let Operand::Local(v) = ptr else { return None };
        if !self.inlinable(*v) || self.alias.contains_key(v) || self.cond_alias.iter().any(|(n, _)| n == v) {
            return None;
        }
        match self.def(*v) {
            Some((_, _, Inst::Cast { cast: Cast::IntToPtr, from, value, .. })) if !matches!(value, Operand::Address { .. }) => {
                Some(self.operand(from, value))
            }
            _ => None,
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

    /// Whether `ptr` is the constant address zero: null, undef (written as null), zero made a
    /// pointer through any chain of casts, or zero offset from one of these. Clang rejects a plain
    /// access through each of these forms (`-Wnull-dereference`), as it folds them all to null.
    fn null_address(&self, ptr: &Operand) -> bool {
        match ptr {
            Operand::Null | Operand::Undef | Operand::Int(0) => true,
            Operand::Local(v) => match self.def(*v) {
                Some((
                    _,
                    _,
                    Inst::Cast {
                        cast: Cast::IntToPtr | Cast::PtrToInt | Cast::Trunc | Cast::ZExt | Cast::SExt | Cast::Bitcast, value, ..
                    },
                )) => self.null_address(value),
                Some((_, _, Inst::Gep { ptr: base, indices, .. })) => {
                    self.null_address(base) && indices.iter().all(|(_, i)| matches!(i, Operand::Int(0)))
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn def(&self, v: ValueId) -> Option<(&'a Block, usize, &'a Inst)> {
        self.defs.def(v)
    }

    fn inlinable(&self, v: ValueId) -> bool {
        !self.watched.contains(&v) && self.defs.inlinable(v)
    }

    fn declare_locals(&self) -> Vec<Local> {
        let mut declared: HashSet<ValueId> = self.f.params.iter().copied().collect();
        let mut locals = Vec::new();
        for b in &self.f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                // A value that shares a phi's variable is declared as the phi.
                if self.inlinable(v)
                    || self.field_pointer(v)
                    || self.skipped.contains(&v)
                    || self.rename.contains_key(&v)
                    || !declared.insert(v)
                {
                    continue;
                }
                let name = name(self.f, v);
                let local = match &i.inst {
                    Inst::Alloca { ty, align } => {
                        let storage = format!("    {} RA_ALIGNED({align});\n", declaration(ty, &format!("{name}_mem")));
                        let pointer = format!("    uint8_t *{name} = (uint8_t *)&{name}_mem;\n");
                        Local::Alloca { name, storage, pointer }
                    }
                    Inst::Phi { .. }
                        if self.direct.contains(&v)
                            && let Some(s) = self.struct_of.get(&v) =>
                    {
                        let text = format!("    struct {s} *{name};\n");
                        Local::One { name, text }
                    }
                    Inst::Phi { .. } if let Some(s) = self.struct_of.get(&v) => {
                        Local::Phi { lead: format!("struct {s} *"), star: "*", name }
                    }
                    Inst::Phi { ty, .. } if self.hoisted.contains(&v) || self.direct.contains(&v) => {
                        let text = format!("    {};\n", declaration(ty, &name));
                        Local::One { name, text }
                    }
                    Inst::Phi { ty, .. } => Local::Phi { lead: format!("{} ", c_type(ty)), star: "", name },
                    _ if let Some(s) = self.struct_of.get(&v) => {
                        let text = format!("    struct {s} *{name};\n");
                        Local::One { name, text }
                    }
                    // A vector compare's result is a vector of lane masks, the width of what it
                    // compared: C has no vector of conditions.
                    Inst::ICmp { ty: ty @ Type::Vector(..), .. } | Inst::FCmp { ty: ty @ Type::Vector(..), .. } => {
                        let text = format!("    {};\n", declaration(&mask_type(ty), &name));
                        Local::One { name, text }
                    }
                    inst => {
                        let text = format!("    {};\n", declaration(&inst.result_type(), &name));
                        Local::One { name, text }
                    }
                };
                locals.push(local);
            }
        }
        locals
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
                self.var(*v)
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
                // A read-only section's string by its literal, another field of a laid-out section
                // by its name, the bytes else.
                let layout = self.layouts.get(global);
                let at = u64::try_from(*offset).ok();
                let literal = layout.zip(at).and_then(|(l, at)| l.literal_at(at));
                let field = layout.zip(at).and_then(|(l, at)| l.field_starting(at)).map(crate::layout::Field::name);
                // The address itself, and whether it needs a byte pointer's type.
                let (address, typed) = match (literal, field) {
                    (Some(literal), _) => (literal, false),
                    (None, Some(field)) => (format!("&{global}.{field}"), false),
                    _ if *offset == 0 => (format!("(uint8_t *)&{global}"), true),
                    _ => (format!("((uint8_t *)&{global} + {offset})"), true),
                };
                match ty {
                    Type::Ptr if typed => address,
                    Type::Ptr => format!("(uint8_t *){address}"),
                    // Through uintptr_t: a 32-bit program's address is the low half of the host's.
                    other => format!("({})(uintptr_t){address}", c_type(other)),
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
                // A compare of vectors is a mask of lanes, all ones where it holds: C has no `!`,
                // `||` or `&&` for one, so the unordered forms take their bitwise kin, and a lane is
                // NaN where it differs from itself.
                if matches!(ty, Type::Vector(..)) {
                    return match cond {
                        FCond::Oeq => format!("({a} == {b})"),
                        FCond::Ogt => format!("({a} > {b})"),
                        FCond::Oge => format!("({a} >= {b})"),
                        FCond::Olt => format!("({a} < {b})"),
                        FCond::Ole => format!("({a} <= {b})"),
                        FCond::One => format!("(({a} < {b}) | ({a} > {b}))"),
                        FCond::Ord => format!("(({a} == {a}) & ({b} == {b}))"),
                        FCond::Ueq => format!("(~(({a} < {b}) | ({a} > {b})))"),
                        FCond::Ugt => format!("(~({a} <= {b}))"),
                        FCond::Uge => format!("(~({a} < {b}))"),
                        FCond::Ult => format!("(~({a} >= {b}))"),
                        FCond::Ule => format!("(~({a} > {b}))"),
                        FCond::Une => format!("({a} != {b})"),
                        FCond::Uno => format!("(({a} != {a}) | ({b} != {b}))"),
                    };
                }
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
            // The compiler's atomic builtins, which C11's `<stdatomic.h>` is made of, read in C and
            // C++ alike and take a plain object rather than an `_Atomic` one.
            Inst::AtomicRmw { op, ty, ptr, value } => {
                let builtin = match op {
                    AtomicOp::Xchg => "__atomic_exchange_n",
                    AtomicOp::Add => "__atomic_fetch_add",
                    AtomicOp::Sub => "__atomic_fetch_sub",
                    AtomicOp::And => "__atomic_fetch_and",
                    AtomicOp::Or => "__atomic_fetch_or",
                    AtomicOp::Xor => "__atomic_fetch_xor",
                };
                let p = format!("({} *)({})", c_type(ty), self.operand(&Type::Ptr, ptr));
                format!("{builtin}({p}, {}, __ATOMIC_SEQ_CST)", strip_parens(&self.operand(ty, value)))
            }
            Inst::CmpXchg { ty, ptr, expected, new } => {
                let p = format!("({} *)({})", c_type(ty), self.operand(&Type::Ptr, ptr));
                let (e, n) = (strip_parens(&self.operand(ty, expected)), strip_parens(&self.operand(ty, new)));
                format!("__sync_val_compare_and_swap({p}, {e}, {n})")
            }
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            // An integer tested against zero is the test: `if (x)`.
            Cond::Is(Operand::Local(v))
                if let Some(v) = Some(
                    self.cond_alias.iter().find(|(n, _)| n == v).map(|(_, p)| *p).or_else(|| self.alias.get(v).copied()).unwrap_or(*v),
                ) && self.inlinable(v)
                    && let Some((_, _, Inst::ICmp { cond: ICond::Ne, ty: ty @ Type::Int(2..), lhs, rhs: Operand::Int(0) })) =
                        self.def(v) =>
            {
                self.operand(ty, lhs)
            }
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
        // The block whose instructions were just written: a `switch` or a return right after it
        // is the block's own end, where its checks go.
        let mut ended = None;
        for s in stmts {
            // A region around a `switch` opens with it.
            let at = match s {
                Stmt::Tail { body, .. } => body.first().unwrap_or(s),
                _ => s,
            };
            if let (Some(block), Stmt::Switch { .. } | Stmt::Return(_)) = (ended, at) {
                self.checks_at_end(out, block, depth);
            }
            ended = match s {
                Stmt::Block(b) | Stmt::Copy(b) => Some(*b),
                _ => None,
            };
            self.stmt(out, s, depth);
        }
    }

    /// The checks before the `switch` or the return the block `id` ends in.
    fn checks_at_end(&mut self, out: &mut String, id: BlockId, depth: usize) {
        for n in self.at_end.get(&id).cloned().unwrap_or_default() {
            self.check(out, n, id, depth);
        }
    }

    /// The check `n` as `ra_assume(<id>, <condition>)`, written in or after the block `id`. A
    /// check with nothing to compare (no register, a `switch` without a case) is not written.
    fn check(&mut self, out: &mut String, n: usize, id: BlockId, depth: usize) {
        let check = self.checks[n];
        let same = |ty: &Type, a: &Operand, b: &Operand| format!("{} == {}", self.operand(ty, a), self.operand(ty, b));
        let condition = match &check.watch {
            Watch::Saved(pairs) => pairs.iter().map(|(ty, was, is)| same(ty, was, is)).collect::<Vec<_>>().join(" && "),
            Watch::Stack(ty, at, expected) => same(ty, at, expected),
            Watch::Index => match self.f.block(id).map(|b| &b.terminator) {
                Some(Terminator::Switch { ty, value, cases, .. }) => cases
                    .iter()
                    .map(|(v, _)| *v)
                    .max()
                    .map(|last| format!("{} < {}", self.operand(ty, value), Self::int_constant(ty, last + 1)))
                    .unwrap_or_default(),
                _ => String::new(),
            },
        };
        if condition.is_empty() {
            return;
        }
        let _ = writeln!(out, "{}ra_assume({}, {condition});", "    ".repeat(depth), check.id);
        self.written.insert(n);
    }

    /// An `if`, opened after `lead`: the block's indentation, or `} else ` when it continues a
    /// chain. An else-branch that is one `if` continues the chain, so a dispatch over the classes
    /// of a receiver reads as `if ... else if ... else`.
    fn if_statement(&mut self, out: &mut String, cond: &Cond, then: &[Stmt], els: &[Stmt], depth: usize, lead: &str) {
        let pad = "    ".repeat(depth);
        // An empty then-branch reads better inverted: `if (!c) { ... }`.
        let (cond, then, els) =
            if then.is_empty() && !els.is_empty() { (Cond::Not(Box::new(cond.clone())), els, then) } else { (cond.clone(), then, els) };
        let test = strip_parens(&self.cond(&cond));
        let mut body = String::new();
        self.stmts(&mut body, then, depth + 1);
        // An arm that is one jump and nothing else reads on the line of its test: `if (c) return;`.
        if els.is_empty() && !lead.is_empty() && body.lines().count() == 1 {
            let jump = body.trim();
            if ["return;", "return ", "break;", "continue;", "goto "].iter().any(|j| jump.starts_with(j)) && !jump.contains("//") {
                let _ = writeln!(out, "{lead}if ({test}) {jump}");
                return;
            }
        }
        let _ = writeln!(out, "{lead}if ({test}) {{");
        out.push_str(&body);
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
            // C has no labeled block: the region is its statements and a leave is a `goto` to the
            // tail's label, which the tail's block writes after them.
            Stmt::Tail { body, .. } => self.stmts(out, body, depth),
            Stmt::Goto(b) | Stmt::Leave(b) => {
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
        let checks = self.after.get(&id).cloned().unwrap_or_default();
        for (index, i) in block.instrs.iter().enumerate() {
            match (&i.inst, i.result) {
                (Inst::Phi { .. }, Some(v)) if self.hoisted.contains(&v) || self.direct.contains(&v) => {}
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
                    let _ = writeln!(out, "{pad}{} = {}{};", self.var(v), self.struct_cast(v), strip_parens(&self.expr(inst)));
                }
                (inst, None) => {
                    let _ = writeln!(out, "{pad}{};", self.expr(inst));
                }
            }
            for &(_, n) in checks.iter().filter(|(at, _)| *at == index) {
                self.check(out, n, id, depth);
            }
        }
        self.moves(out, id, depth);
    }

    /// The phis of the successors of `id` take their values here, all at once.
    fn moves(&mut self, out: &mut String, id: BlockId, depth: usize) {
        let pad = "    ".repeat(depth);
        let f = self.f;
        if let Some(moves) = self.defs.phi_moves.get(&id).cloned() {
            // Two phis that share a variable and take one value here take one copy.
            let mut copied: HashSet<(String, String)> = HashSet::new();
            for (phi, o) in &moves {
                if self.hoisted.contains(phi) {
                    continue;
                }
                let ty = f.value_type(*phi).clone();
                let value = strip_parens(&self.operand(&ty, o));
                if self.direct.contains(phi) {
                    // A phi whose variable holds the value already takes no copy.
                    if !self.holds(*phi, o) && copied.insert((self.var(*phi), value.clone())) {
                        let _ = writeln!(out, "{pad}{} = {}{value};", self.var(*phi), self.struct_cast(*phi));
                    }
                    continue;
                }
                let _ = writeln!(out, "{pad}{}_next = {}{value};", name(f, *phi), self.struct_cast(*phi));
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

/// The allocations of a scalar type whose own accesses read their storage by name, with the
/// type: each whose address goes elsewhere too, or whose every load a store of it comes before on
/// every way there (a C compiler sees a read of an unset variable for what the machine read of
/// its stack).
fn named_allocas(f: &Function) -> HashMap<ValueId, Type> {
    let mut allocas: HashMap<ValueId, Type> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        if let (Some(a), Inst::Alloca { ty, .. }) = (i.result, &i.inst)
            && matches!(ty, Type::Int(8 | 16 | 32 | 64) | Type::Float | Type::Double | Type::Ptr)
        {
            allocas.insert(a, ty.clone());
        }
    }
    if allocas.is_empty() {
        return allocas;
    }
    let own = |o: &Operand, ty: &Type| match o {
        Operand::Local(a) => allocas.get(a).is_some_and(|s| s == ty).then_some(*a),
        _ => None,
    };
    let mut escapes: HashSet<ValueId> = HashSet::new();
    let mut loads: HashMap<ValueId, Vec<(BlockId, usize)>> = HashMap::new();
    let mut stores: HashMap<ValueId, Vec<(BlockId, usize)>> = HashMap::new();
    for b in &f.blocks {
        for (at, i) in b.instrs.iter().enumerate() {
            let accessed = match &i.inst {
                Inst::Load { ty, ptr, .. } => own(ptr, ty).inspect(|a| loads.entry(*a).or_default().push((b.id, at))),
                Inst::Store { ty, ptr, .. } => own(ptr, ty).inspect(|a| stores.entry(*a).or_default().push((b.id, at))),
                _ => None,
            };
            for v in i.inst.uses() {
                if allocas.contains_key(&v) && accessed != Some(v) {
                    escapes.insert(v);
                }
            }
            // A store of the allocation's address is an escape whatever it stores into.
            if let Inst::Store { value: Operand::Local(v), .. } = &i.inst
                && allocas.contains_key(v)
            {
                escapes.insert(*v);
            }
        }
        for v in b.terminator.uses() {
            if allocas.contains_key(&v) {
                escapes.insert(v);
            }
        }
    }
    let order = ra_asm_ir::dom::reverse_post_order(f);
    let idom = ra_asm_ir::dom::dominators(&order, &f.predecessors());
    allocas.retain(|a, _| {
        if escapes.contains(a) {
            return true;
        }
        // The first store of each block that stores `a`.
        let mut first: HashMap<BlockId, usize> = HashMap::new();
        for &(sb, si) in stores.get(a).into_iter().flatten() {
            first.entry(sb).and_modify(|at| *at = (*at).min(si)).or_insert(si);
        }
        let mut above = HashMap::new();
        loads
            .get(a)
            .into_iter()
            .flatten()
            .all(|&(lb, li)| first.get(&lb).is_some_and(|&si| si < li) || stored_above(&idom, &first, &mut above, lb))
    });
    allocas
}

/// Whether a block that strictly dominates `b` is one of `stored`, with the answers of the blocks
/// walked kept in `memo`: a walk to the root of the dominator tree from each load, against each
/// store, made the emit cubic in a chain of blocks that spill and reload one slot.
fn stored_above(idom: &HashMap<BlockId, BlockId>, stored: &HashMap<BlockId, usize>, memo: &mut HashMap<BlockId, bool>, b: BlockId) -> bool {
    let mut walked = Vec::new();
    let mut x = b;
    let found = loop {
        if let Some(&known) = memo.get(&x) {
            break known;
        }
        let Some(&up) = idom.get(&x) else { break false };
        if up == x {
            break false;
        }
        walked.push(x);
        if stored.contains_key(&up) {
            break true;
        }
        x = up;
    };
    for w in walked {
        memo.insert(w, found);
    }
    found
}

/// The blocks that read each value of `f`: the block of each instruction and branch that reads
/// it, and for a phi the predecessor whose move reads it. Made once for [`live_blocks`]: a scan of
/// the function for each phi asked about made the emit quadratic in a function's diamonds.
fn reading_blocks(f: &Function) -> HashMap<ValueId, Vec<BlockId>> {
    let mut out: HashMap<ValueId, Vec<BlockId>> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            match &i.inst {
                Inst::Phi { incoming, .. } => {
                    for (o, from) in incoming {
                        if let Operand::Local(v) = o {
                            out.entry(*v).or_default().push(*from);
                        }
                    }
                }
                inst => {
                    for v in inst.uses() {
                        out.entry(v).or_default().push(b.id);
                    }
                }
            }
        }
        for v in b.terminator.uses() {
            out.entry(v).or_default().push(b.id);
        }
    }
    out
}

/// The blocks `v`, defined in `home`, is live into: each block that reads it but `home`, each
/// predecessor whose phi move reads it, and back from those to `home`. `readers` is
/// [`reading_blocks`] of the function.
fn live_blocks(
    v: ValueId,
    home: BlockId,
    preds: &HashMap<BlockId, Vec<BlockId>>,
    readers: &HashMap<ValueId, Vec<BlockId>>,
) -> HashSet<BlockId> {
    let mut work: Vec<BlockId> = readers.get(&v).into_iter().flatten().copied().filter(|b| *b != home).collect();
    let mut live = HashSet::new();
    while let Some(b) = work.pop() {
        if live.insert(b) {
            work.extend(preds.get(&b).into_iter().flatten().filter(|p| **p != home));
        }
    }
    live
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
    matches!(body.last(), Some(Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Goto(_) | Stmt::Leave(_) | Stmt::Unreachable))
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ra_asm_ir::{BinOp, CallConv, Callee, Cast, Declaration, FnType, FunctionBuilder, Module, Operand, Terminator, Type};

    use super::{Options, Unit, emit_unit, emit_unit_dropping, parts};

    /// `module` written one function after the other on one thread, each after the SDK names the
    /// ones before it defined: the emit as it was before its functions ran apart.
    fn serial(module: &Module, options: &Options) -> Unit {
        let mut parts = parts(module, options);
        let mut names = BTreeMap::new();
        let mut functions = Vec::with_capacity(module.functions.len());
        for f in &module.functions {
            let (emitted, written) = parts.function(f, &module.types, options, &mut names);
            parts.watched.extend(written);
            functions.push(emitted);
        }
        parts.sdk_names = names;
        parts.unit(functions, options)
    }

    /// Functions that each open a file with an access mask the SDK names: `GENERIC_READ` alone or
    /// or-ed with `GENERIC_WRITE`, mostly as a 32-bit argument, every fifth as a 64-bit one, whose
    /// define has another literal; the first 64-bit one comes after a 32-bit one has defined it.
    fn openers(count: u64) -> Module {
        let mut module = Module::x86_64_windows("openers");
        let params = |access: Type| vec![Type::Ptr, access, Type::I32, Type::Ptr, Type::I32, Type::I32, Type::Ptr];
        module.declare(Declaration {
            name: "CreateFileA".to_owned(),
            ty: FnType::new(Type::Ptr, params(Type::I32)),
            conv: CallConv::default(),
            attrs: Vec::new(),
        });
        for i in 0..count {
            let mut b = FunctionBuilder::new(&format!("F_{:08x}", 0x1000 + i * 0x10), FnType::new(Type::Void, Vec::new()), &[]);
            b.set_address(0x1000 + i * 0x10);
            let entry = b.block("entry");
            let access_ty = if i % 5 == 3 { Type::I64 } else { Type::I32 };
            let access = if i % 2 == 0 { 0x8000_0000 } else { 0xc000_0000 };
            let args = vec![
                (Type::Ptr, Operand::Null),
                (access_ty.clone(), Operand::Int(access)),
                (Type::I32, Operand::Int(1)),
                (Type::Ptr, Operand::Null),
                (Type::I32, Operand::Int(3)),
                (Type::I32, Operand::Int(0x80)),
                (Type::Ptr, Operand::Null),
            ];
            b.call(entry, FnType::new(Type::Ptr, params(access_ty)), Callee::Direct("CreateFileA".into()), args, "handle");
            b.terminate(entry, Terminator::Ret(None));
            module.functions.push(b.finish());
        }
        module
    }

    #[test]
    fn functions_sharing_sdk_names_write_as_the_serial_emit() {
        let module = openers(300);
        let options = Options::default();
        let expected = serial(&module, &options).text();
        let calls: Vec<&str> = expected.lines().filter(|l| l.contains("CreateFileA(") && !l.starts_with("extern ")).collect();
        let named = calls.iter().filter(|l| l.contains("GENERIC_READ")).count();
        // Both choices are made: the names, and the numbers where a name has another width.
        assert!(named > 0 && named < calls.len(), "{named} of {} calls name the mask:\n{expected}", calls.len());
        assert!(expected.contains("#define GENERIC_READ "), "{expected}");
        assert_eq!(emit_unit(&module, &options).text(), expected);
        assert_eq!(emit_unit_dropping(&mut module.clone(), &options).text(), expected);
    }

    #[test]
    fn an_integer_address_is_cast_once_to_the_access_type() {
        let mut module = Module::x86_64_windows("address");
        let mut b = FunctionBuilder::new("F_00001000", FnType::new(Type::I32, vec![Type::I64]), &["rcx"]);
        b.set_address(0x1000);
        let entry = b.block("entry");
        let rcx = b.param(0);
        let base = b.bin(entry, BinOp::Xor, Type::I64, rcx, Operand::Int(7), "base");
        let byte = b.cast(entry, Cast::IntToPtr, Type::I64, base, Type::Ptr, "byte");
        b.store(entry, Type::I8, Operand::Int(1), byte);
        let at = b.bin(entry, BinOp::Or, Type::I64, rcx, Operand::Int(16), "at");
        let word = b.cast(entry, Cast::IntToPtr, Type::I64, at, Type::Ptr, "word");
        let read = b.load(entry, Type::I32, word, "read");
        b.terminate(entry, Terminator::Ret(Some((Type::I32, Operand::Local(read)))));
        module.functions.push(b.finish());
        let text = emit_unit(&module, &Options::default()).functions[0].text.clone();
        // The integer goes straight to the access's pointer type, without a byte pointer between.
        assert!(!text.contains("((uint8_t *)"), "{text}");
        assert!(text.contains("*(uint8_t *)(") && text.contains("*(uint32_t *)("), "{text}");
    }

    /// A module whose functions are frozen past its first ones writes as it does thawed, and the
    /// dropping emit leaves it every function's shell, in order, and nothing frozen.
    #[test]
    fn a_frozen_module_writes_as_it_does_thawed() {
        let module = openers(300);
        let options = Options::default();
        let expected = emit_unit(&module, &options).text();
        let mut frozen = module.clone();
        let rest = frozen.functions.split_off(120);
        frozen.frozen = rest.iter().map(ra_asm_ir::freeze::freeze).collect();
        assert_eq!(emit_unit(&frozen, &options).text(), expected);
        assert_eq!(emit_unit_dropping(&mut frozen, &options).text(), expected);
        assert!(frozen.frozen.is_empty(), "the dropping emit left {} functions frozen", frozen.frozen.len());
        let names: Vec<&str> = frozen.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, module.functions.iter().map(|f| f.name.as_str()).collect::<Vec<_>>());
        assert!(frozen.functions.iter().all(|f| f.blocks.is_empty()), "a function kept its body");
    }

    /// The bytes a C compiler reads from the braced initializer of a `size`-byte array that
    /// [`emit_bytes`] wrote: the literals' escapes or the numbers, then the zero fill.
    fn initialized(text: &str, size: usize) -> Vec<u8> {
        let inner = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')).expect(text).trim();
        let mut out = Vec::new();
        if inner.starts_with('"') {
            let mut chars = inner.chars().peekable();
            while let Some(c) = chars.next() {
                if c != '"' {
                    assert!(c.is_whitespace(), "{c:?} between the literals of {text}");
                    continue;
                }
                loop {
                    match chars.next().expect(text) {
                        '"' => break,
                        '\\' => match chars.next().expect(text) {
                            'n' => out.push(b'\n'),
                            't' => out.push(b'\t'),
                            'r' => out.push(b'\r'),
                            d @ '0'..='7' => {
                                let mut v = d.to_digit(8).unwrap();
                                for _ in 0..2 {
                                    match chars.peek().and_then(|c| c.to_digit(8)) {
                                        Some(n) => {
                                            v = v * 8 + n;
                                            chars.next();
                                        }
                                        None => break,
                                    }
                                }
                                out.push(u8::try_from(v).unwrap());
                            }
                            other => out.push(u8::try_from(other).unwrap()),
                        },
                        c => out.push(u8::try_from(c).unwrap()),
                    }
                }
            }
            assert!(out.len() < size, "no room for the NUL of {text}");
        } else {
            out.extend(inner.split(',').map(|n| n.trim().parse::<u8>().expect(text)));
            assert!(out.len() <= size, "{text}");
        }
        out.resize(size, 0);
        out
    }

    #[test]
    fn a_byte_array_is_packed_and_reads_back_as_its_bytes() {
        let mut every: Vec<u8> = (0..=255u8).chain(*b"??=??/?\"\\0\x017\x018").collect();
        every.extend([0; 300]);
        every.extend(b"tail\0\0\0");
        let ends_in_one = [7u8, 0, 0, 0, 9, 0, 0, 1];
        let cases: [&[u8]; 4] = [&every, &ends_in_one, &[0; 64], b"text\0"];
        for bytes in cases {
            let mut text = String::new();
            super::emit_bytes(&mut text, bytes, bytes.len() as u64);
            assert_eq!(initialized(&text, bytes.len()), bytes, "{text}");
            // The trailing zeros are the zero fill's, and a line holds many bytes.
            assert!(!text.contains("\\0\"\n}") && !text.contains(" 0\n}"), "{text}");
            assert!(text.lines().count() * 16 <= bytes.len() + 48, "{} lines for {} bytes:\n{text}", text.lines().count(), bytes.len());
        }
    }
}
