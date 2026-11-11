//! The C backend.
//!
//! Every IR value is a C local of its type (`uint64_t`, `double`, `uint8_t *`), assigned where
//! its instruction stands; a pure value with one use in the same block is written inline at that
//! use instead, recursively, so an expression reads as one. Signedness is a cast at the operation
//! that needs it, as the IR says. Phis become copies at the end of each predecessor, through
//! temporaries so parallel copies stay parallel. The data globals are byte arrays with address
//! words; a reference into one is an address plus an offset. The runtime header `rhipper.h` holds
//! the helpers the intrinsics map to.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use ra_asm_ir::{BinOp, Block, BlockId, Callee, Cast, Const, FCond, Function, ICond, Inst, Module, Operand, Type, ValueId};

use crate::common::{Defs, parenthesized, strip_parens, trap_count};
use crate::structure::{Cond, Graph, Stmt, structure};

/// The language the backend writes: C, or the same source as C++ (`extern "C"` linkage, `.cpp`
/// files), until the class recovery gives the C++ output a shape of its own.
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
}

/// One function's C: the comment naming it, the definition line and the body.
#[derive(Debug, Clone)]
pub struct EmittedFunction {
    pub ir_name: String,
    pub address: Option<u64>,
    /// The lines above the definition line: the comment.
    pub header_lines: usize,
    /// The `ra_trap` calls in the body: instructions the lift did not translate.
    pub traps: usize,
    pub text: String,
}

/// The translation in pieces: the shared declarations, the data and the functions, so the output
/// can be one file or several, and the function table can say where each function is.
#[derive(Debug, Clone)]
pub struct Unit {
    /// The comment and the include of the runtime header.
    pub include: String,
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

impl Unit {
    /// The parts every single-file output starts with. The `extern` declarations come before the
    /// definitions in one file too: in C++ a `const` global has internal linkage without them.
    fn preamble(&self) -> [&String; 6] {
        [&self.include, &self.externs, &self.prototypes, &self.global_types, &self.global_decls, &self.global_defs]
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
        for part in [&self.include, &self.externs, &self.prototypes, &self.global_types, &self.global_decls] {
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

    /// Appends `functions` to `text` and returns each one's definition line in it.
    fn append_functions(text: &mut String, functions: &[EmittedFunction]) -> Vec<(String, usize)> {
        let mut line = text.lines().count() + 1;
        let mut lines = Vec::with_capacity(functions.len());
        for f in functions {
            lines.push((f.ir_name.clone(), line + f.header_lines));
            text.push_str(&f.text);
            text.push('\n');
            line += f.text.lines().count() + 1;
        }
        lines
    }
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
    for g in &module.globals {
        global_type(&mut global_types, g);
        declare_global(&mut global_decls, g);
        define_global(&mut global_defs, g);
    }
    // In C++ the imports, the functions and the data keep C linkage: the driver is C, the imports
    // are the DLLs' own names, and a definition under a prototype with C linkage inherits it.
    if options.dialect == Dialect::Cpp {
        for part in [&mut externs, &mut prototype_lines, &mut global_decls, &mut global_defs] {
            if !part.trim().is_empty() {
                *part = format!("extern \"C\" {{\n{part}}}\n");
            }
        }
    }
    let functions = module
        .functions
        .iter()
        .map(|f| {
            let mut text = String::new();
            let header_lines = emit_function(&mut text, f, &prototypes[&f.name], options.names.get(&f.name).map(String::as_str));
            EmittedFunction { ir_name: f.name.clone(), address: f.address, header_lines, traps: trap_count(f), text }
        })
        .collect();
    Unit { include, externs, prototypes: prototype_lines, global_types, global_decls, global_defs, functions, dialect: options.dialect }
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
        Type::Struct(_) | Type::Packed(_) | Type::Named(_) => "struct ra_opaque".into(),
        Type::Func(_) => "void *".into(),
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
fn declaration(ty: &Type, name: &str) -> String {
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

fn name(f: &Function, v: ValueId) -> String {
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

/// The structure type of a packed data global, which its declaration and its definition share.
fn global_type(out: &mut String, g: &ra_asm_ir::Global) {
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

/// The `extern` declaration of a data global, for the files that do not define it.
fn declare_global(out: &mut String, g: &ra_asm_ir::Global) {
    let storage = if g.constant { "const " } else { "" };
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

/// The definition of a data global, with its initializer. The type of a packed global comes from
/// [`global_type`] first.
fn define_global(out: &mut String, g: &ra_asm_ir::Global) {
    let storage = if g.constant { "const " } else { "" };
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
                    Const::Int(v) => {
                        let _ = write!(out, "{v}ull");
                    }
                    Const::Address { global, offset } => {
                        let _ = write!(out, "({})((uint8_t *)&{global} + {offset})", c_type(field));
                    }
                    Const::Global(name) => {
                        let _ = write!(out, "({})&{name}", c_type(field));
                    }
                    other => {
                        let _ = write!(out, "0 /* {other:?} */");
                    }
                }
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

fn emit_bytes(out: &mut String, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if i % 32 == 0 {
            out.push_str("\n    ");
        }
        let _ = write!(out, "{b},");
    }
    out.push('\n');
}

/// Writes one function.
/// A function, with one comment line above it naming it: the source name when the image gives
/// one, and its address. Returns the number of lines above the definition.
fn emit_function(out: &mut String, f: &Function, prototype: &str, source_name: Option<&str>) -> usize {
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
    let _ = writeln!(out, "{prototype} {{");
    let graph = Graph::of(f);
    let structured = structure(f, &graph);
    let mut e = Emitter::new(f, &structured.labels);
    e.declare_locals(out);
    e.stmts(out, &structured.body, 1);
    let _ = writeln!(out, "}}");
    header_lines
}

struct Emitter<'a> {
    f: &'a Function,
    labels: &'a HashSet<BlockId>,
    defs: Defs<'a>,
}

impl<'a> Emitter<'a> {
    fn new(f: &'a Function, labels: &'a HashSet<BlockId>) -> Self {
        Self { f, labels, defs: Defs::new(f) }
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
                if self.inlinable(v) || !declared.insert(v) {
                    continue;
                }
                match &i.inst {
                    Inst::Alloca { ty, align } => {
                        let _ = writeln!(out, "    {} RA_ALIGNED({align});", declaration(ty, &format!("{}_mem", name(self.f, v))));
                        let _ = writeln!(out, "    uint8_t *{} = (uint8_t *)&{}_mem;", name(self.f, v), name(self.f, v));
                    }
                    Inst::Phi { ty, .. } => {
                        let _ = writeln!(out, "    {} {}, {}_next;", c_type(ty), name(self.f, v), name(self.f, v));
                    }
                    inst => {
                        let _ = writeln!(out, "    {};", declaration(&inst.result_type(), &name(self.f, v)));
                    }
                }
            }
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
            Type::Int(128) => format!("((ra_u128){}ull)", v as u64),
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
                if self.inlinable(*v)
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
            Operand::Address { global, offset } => match ty {
                Type::Ptr => format!("((uint8_t *)&{global} + {offset})"),
                other => format!("({})((uint8_t *)&{global} + {offset})", c_type(other)),
            },
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
            Inst::Load { ty, ptr, .. } => format!("*({} *)({})", c_type(ty), self.operand(&Type::Ptr, ptr)),
            Inst::Store { .. } | Inst::Phi { .. } => String::new(),
            Inst::Bin { op, ty, lhs, rhs } => {
                let c = c_type(ty);
                let s = signed_type(ty);
                let a = self.operand(ty, lhs);
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
                let b = self.operand(ty, &rhs);
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
                    BinOp::Shl => format!("{c}({a} << {b})"),
                    BinOp::LShr => format!("{c}({a} >> {b})"),
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
                let (a, b) = (self.operand(ty, lhs), self.operand(ty, rhs));
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
                format!("({} ? {} : {})", self.operand(&Type::I1, cond), self.operand(ty, then), self.operand(ty, els))
            }
            // The address of a global as a pointer needs no trip through an integer.
            Inst::Cast { cast: Cast::IntToPtr, value: value @ Operand::Address { .. }, .. } => self.operand(&Type::Ptr, value),
            Inst::Cast { cast, from, value, to } => {
                let v = self.operand(from, value);
                match cast {
                    Cast::Trunc
                    | Cast::ZExt
                    | Cast::UIToFP
                    | Cast::FPToUI
                    | Cast::FPTrunc
                    | Cast::FPExt
                    | Cast::IntToPtr
                    | Cast::PtrToInt => format!("({}){v}", c_type(to)),
                    Cast::SExt | Cast::SIToFP => format!("({})({}){v}", c_type(to), signed_type(from)),
                    Cast::FPToSI => format!("({})({}){v}", c_type(to), signed_type(to)),
                    Cast::Bitcast => format!("ra_bitcast_{}_{}({v})", short_type(from), short_type(to)),
                }
            }
            Inst::Gep { base_ty, ptr, indices, .. } => {
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
                let a: Vec<String> = args.iter().map(|(t, o)| strip_parens(&self.operand(t, o))).collect();
                match callee {
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

    fn stmt(&mut self, out: &mut String, s: &Stmt, depth: usize) {
        let pad = "    ".repeat(depth);
        match s {
            Stmt::Block(b) => self.block(out, *b, depth),
            Stmt::If { cond, then, els } => {
                // An empty then-branch reads better inverted: `if (!c) { ... }`.
                let (cond, then, els) = if then.is_empty() && !els.is_empty() {
                    (Cond::Not(Box::new(cond.clone())), els, then)
                } else {
                    (cond.clone(), then, els)
                };
                let _ = writeln!(out, "{pad}if ({}) {{", strip_parens(&self.cond(&cond)));
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
                let _ = writeln!(out, "{pad}for (;;) {{");
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
                let _ = writeln!(out, "{pad}goto {};", label(*b, self.f));
            }
            Stmt::Switch { ty, value, cases, default } => {
                let _ = writeln!(out, "{pad}switch ({}) {{", strip_parens(&self.operand(ty, value)));
                for (values, body) in cases {
                    for v in values {
                        let _ = writeln!(out, "{pad}case {v}:");
                    }
                    self.stmts(out, body, depth + 1);
                    if !ends_in_jump(body) {
                        let _ = writeln!(out, "{pad}    break;");
                    }
                }
                let _ = writeln!(out, "{pad}default:");
                self.stmts(out, default, depth + 1);
                if !ends_in_jump(default) {
                    let _ = writeln!(out, "{pad}    break;");
                }
                let _ = writeln!(out, "{pad}}}");
            }
            Stmt::Return(None) => {
                let _ = writeln!(out, "{pad}return;");
            }
            Stmt::Return(Some((ty, o))) => {
                let _ = writeln!(out, "{pad}return {};", strip_parens(&self.operand(ty, o)));
            }
            Stmt::Unreachable => {
                let _ = writeln!(out, "{pad}RA_UNREACHABLE();");
            }
        }
    }

    /// The instructions of a block, then the phi moves to its successors.
    fn block(&mut self, out: &mut String, id: BlockId, depth: usize) {
        let pad = "    ".repeat(depth);
        let f = self.f;
        let Some(block) = f.block(id) else { return };
        if self.labels.contains(&id) {
            let _ = writeln!(out, "{}:;", label(id, f));
        }
        for i in &block.instrs {
            match (&i.inst, i.result) {
                (Inst::Phi { .. }, Some(v)) => {
                    let _ = writeln!(out, "{pad}{} = {}_next;", name(f, v), name(f, v));
                }
                (Inst::Alloca { .. }, _) => {}
                (Inst::Store { ty, value, ptr, .. }, _) => {
                    let _ = writeln!(
                        out,
                        "{pad}*({} *)({}) = {};",
                        c_type(ty),
                        strip_parens(&self.operand(&Type::Ptr, ptr)),
                        strip_parens(&self.operand(ty, value))
                    );
                }
                (_, Some(v)) if self.inlinable(v) => {}
                (inst, Some(v)) => {
                    let _ = writeln!(out, "{pad}{} = {};", name(f, v), strip_parens(&self.expr(inst)));
                }
                (inst, None) => {
                    let _ = writeln!(out, "{pad}{};", self.expr(inst));
                }
            }
        }
        // The phis of the successors take their values here, all at once.
        if let Some(moves) = self.defs.phi_moves.get(&id).cloned() {
            for (phi, o) in &moves {
                let ty = f.value_type(*phi).clone();
                let _ = writeln!(out, "{pad}{}_next = {};", name(f, *phi), strip_parens(&self.operand(&ty, o)));
            }
        }
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
