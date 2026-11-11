//! The lift: each function of the analysis becomes an IR function over an explicit machine state.
//!
//! The model is the machine's. Every lifted function is `void F_<address>(ptr %state)`: the
//! registers and the flags live in `%State`, memory is the host's memory at the image's own
//! addresses (the driver maps the image at its base), the native stack is a region the driver
//! gives `rsp`, and a `call` pushes its return address as the machine does before it calls the
//! lifted callee. An import is called through an adapter that takes the four register arguments
//! and four stack slots, which is what the Win64 convention lets a caller of unknown arity do.
//! A trap, an unresolved jump or an indirect call goes to a runtime function of the driver.
//!
//! The raising passes of milestone E turn this into source-shaped IR; this milestone's job is to
//! be right, which the execution oracle checks against the native program.

pub mod raise;
pub mod state;
pub mod x86;

pub use raise::{Options as RaiseOptions, raise};

use std::collections::{BTreeMap, BTreeSet};

use ra_asm_analysis::{Analysis, Kind, Origin};
use ra_asm_ir::{CallConv, Const, Declaration, Global, Linkage, Module, Type};
use ra_asm_pe::Image;

/// What the lift produced.
pub struct Lifted {
    pub module: Module,
    /// The imports the lifted code calls, by display name.
    pub imports: BTreeSet<String>,
    /// The functions, by address and IR name.
    pub functions: BTreeMap<u64, String>,
    /// The functions whose callers the lift does not see: the entry, the exports, the callbacks,
    /// the functions a pointer names.
    pub roots: BTreeSet<String>,
    /// The import wrappers the code calls: the import's symbol and the stack offset of its first
    /// stack argument at the call site.
    pub wrappers: BTreeSet<(String, i64)>,
    pub warnings: Vec<String>,
}

/// The IR name of the lifted function at `va`: `F_<address>`, as the old pipeline named them.
pub fn function_name(va: u64) -> String {
    format!("F_{va:08x}")
}

/// The name of an import's wrapper: `imp_<symbol>`, or `imp_<symbol>.tail` for a tail call.
pub fn import_wrapper_name(symbol: &str, stack_offset: i64) -> String {
    if stack_offset == 0x20 { format!("imp_{symbol}") } else { format!("imp_{symbol}.tail") }
}

/// The import of a wrapper's name, or `None` for any other function.
pub fn import_of_wrapper(name: &str) -> Option<&str> {
    name.strip_prefix("imp_").map(|rest| rest.strip_suffix(".tail").unwrap_or(rest))
}

/// Builds the wrapper of an import: it reads `rcx`, `rdx`, `r8`, `r9` and four stack slots from
/// `rsp + stack_offset` out of the state, calls the import through the eight-integer adapter, and
/// stores the result to `rax`.
pub fn import_wrapper(symbol: &str, stack_offset: i64) -> ra_asm_ir::Function {
    use ra_asm_ir::{BinOp, Callee, FnType, FunctionBuilder, Terminator};
    use ra_asm_x86::iced_x86::Register as R;
    let name = import_wrapper_name(symbol, stack_offset);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_linkage(Linkage::Internal);
    let state = b.param(0);
    let block = b.block("entry");
    let mut m = state::Machine { b: &mut b, state, bits: 64 };
    let mut args = Vec::with_capacity(8);
    for r in [R::RCX, R::RDX, R::R8, R::R9] {
        let v = m.read_gpr(block, r);
        args.push((Type::I64, v.into()));
    }
    let sp = m.read_sp(block);
    for n in 0..4 {
        let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, stack_offset + 8 * n, "arg.p");
        let v = m.load(block, Type::I64, addr, "arg");
        args.push((Type::I64, v.into()));
    }
    let result = m.b.call(block, state::import_adapter_type(), Callee::Direct(symbol.to_owned()), args, "ret").unwrap();
    m.write_gpr(block, R::RAX, result);
    b.terminate(block, Terminator::Ret(None));
    b.finish()
}

/// Lifts every function of `analysis`.
pub fn lift(image: &Image, analysis: &Analysis<'_>) -> Lifted {
    let mut module = Module::x86_64_windows(
        &image.path.as_ref().map_or_else(|| "image".to_owned(), |p| p.file_name().unwrap().to_string_lossy().into_owned()),
    );
    module.types.push((state::STATE_TYPE.to_owned(), state::state_fields()));
    state::declare_runtime(&mut module);

    let mut lifted = Lifted {
        module,
        imports: BTreeSet::new(),
        functions: BTreeMap::new(),
        roots: BTreeSet::new(),
        wrappers: BTreeSet::new(),
        warnings: Vec::new(),
    };
    for f in analysis.functions.values() {
        let name = function_name(f.entry);
        let root = f.origins.iter().any(|o| matches!(o, Origin::Entry | Origin::Export(_) | Origin::TlsCallback | Origin::Pointer(_)));
        if root || matches!(f.kind, Kind::ImportThunk(_)) {
            lifted.roots.insert(name.clone());
        }
        lifted.functions.insert(f.entry, name);
    }
    for f in analysis.functions.values() {
        let built = x86::lift_function(image, analysis, &mut lifted, f);
        lifted.module.functions.push(built);
    }

    // The table the runtime resolves an indirect call through: { address, function } pairs.
    let entries: Vec<Const> =
        lifted.functions.iter().map(|(va, name)| Const::Struct(vec![Const::Int(i128::from(*va)), Const::Global(name.clone())])).collect();
    let entry_ty = Type::Struct(vec![Type::I64, Type::Ptr]);
    lifted.module.globals.push(Global {
        name: "ra_functions".to_owned(),
        ty: Type::array(entries.len() as u64, entry_ty),
        init: Some(Const::Array(entries)),
        constant: true,
        linkage: Linkage::External,
        align: 16,
        address: None,
    });
    lifted.module.globals.push(Global {
        name: "ra_function_count".to_owned(),
        ty: Type::I64,
        init: Some(Const::Int(lifted.functions.len() as i128)),
        constant: true,
        linkage: Linkage::External,
        align: 8,
        address: None,
    });
    for import in &lifted.imports {
        lifted.module.declare(Declaration { name: import.clone(), ty: state::import_adapter_type(), conv: CallConv::C, attrs: Vec::new() });
    }
    for (symbol, offset) in &lifted.wrappers {
        lifted.module.functions.push(import_wrapper(symbol, *offset));
    }
    lifted
}

/// The C header the driver includes: the base, the size, the entry, and the imports to bind.
pub fn image_header(image: &Image, analysis: &Analysis<'_>, lifted: &Lifted) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "// Generated by ras lift. The image the lifted program runs against.");
    let _ = writeln!(out, "#define RA_IMAGE_BASE 0x{:x}ULL", image.image_base());
    let _ = writeln!(out, "#define RA_IMAGE_SIZE 0x{:x}", image.size_of_image());
    let _ = writeln!(out, "#define RA_POINTER_SIZE {}", image.pointer_size());
    if let Some(entry) = image.entry_point() {
        let _ = writeln!(out, "#define RA_ENTRY {}", function_name(entry));
        let _ = writeln!(out, "void {}(void *state);", function_name(entry));
    }
    let _ = writeln!(
        out,
        "static const struct {{ const char *dll; const char *name; unsigned ordinal; unsigned long long slot; }} ra_imports[] = {{"
    );
    for import in &image.imports {
        let _ = writeln!(
            out,
            "    {{ \"{}\", {}, {}, 0x{:x}ULL }},",
            import.dll,
            import.name.as_ref().map_or_else(|| "0".to_owned(), |n| format!("\"{n}\"")),
            import.ordinal.unwrap_or(0),
            import.iat_va
        );
    }
    let _ = writeln!(out, "    {{ 0, 0, 0, 0 }}");
    let _ = writeln!(out, "}};");
    let _ = writeln!(out, "static const unsigned long long ra_noreturn_imports = {};", analysis.noreturn.len());
    let _ = writeln!(out, "static const unsigned ra_lifted_function_count = {};", lifted.functions.len());
    out
}
