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

pub mod data;
pub mod raise;
pub mod state;
pub mod x86;
pub mod x87;

pub use raise::{Level as RaiseLevel, Options as RaiseOptions, raise};

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
    /// The import wrappers the code calls: the import's symbol, the stack offset of its first
    /// stack argument at the call site, and whether the call is a tail call.
    pub wrappers: BTreeSet<(String, i64, bool)>,
    /// The data sections written as globals (standalone mode).
    pub data: Vec<DataSection>,
    /// Whether the lift references data through globals rather than the mapped image.
    pub standalone: bool,
    /// 32 or 64: the address width of the image.
    pub bits: u32,
    /// The addresses of the words the base relocations name: an immediate or a displacement at
    /// one of them holds an address of the image, which 32-bit code writes as a plain number.
    pub relocated: std::collections::HashSet<u64>,
    pub warnings: Vec<String>,
}

/// The IR name of the lifted function at `va`: `F_<address>`, as the old pipeline named them.
pub fn function_name(va: u64) -> String {
    format!("F_{va:08x}")
}

/// The name of an import's wrapper: `imp_<symbol>`, or `imp_<symbol>.tail` for a tail call.
pub fn import_wrapper_name(symbol: &str, tail: bool) -> String {
    if tail { format!("imp_{symbol}.tail") } else { format!("imp_{symbol}") }
}

/// The import of a wrapper's name, or `None` for any other function.
pub fn import_of_wrapper(name: &str) -> Option<&str> {
    name.strip_prefix("imp_").map(|rest| rest.strip_suffix(".tail").unwrap_or(rest))
}

/// Builds the wrapper of an import. In 64 bits it reads `rcx`, `rdx`, `r8`, `r9` and four stack
/// slots from `rsp + stack_offset` out of the state, calls the import through the eight-integer
/// adapter, and stores the result to `rax`. In 32 bits every argument is on the stack: it reads
/// eight dwords from `esp + stack_offset`, zero-extended, and stores the result to `eax` and its
/// high half to `edx`. The caller pops what a `stdcall` import pops, since it knows the import.
/// An import with a typed prototype (the C runtime's math) takes its own arguments instead, a
/// double read as one, and its result goes where the convention puts it: `xmm0`, or in 32 bits
/// the x87 stack.
pub fn import_wrapper(symbol: &str, stack_offset: i64, tail: bool, bits: u32) -> ra_asm_ir::Function {
    use ra_asm_ir::{BinOp, Callee, Cast, FnType, FunctionBuilder, Terminator};
    use ra_asm_x86::iced_x86::Register as R;
    use raise::abi::Kind;
    let name = import_wrapper_name(symbol, tail);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_linkage(Linkage::Internal);
    let state = b.param(0);
    let block = b.block("entry");
    let mut m = state::Machine { b: &mut b, state, bits };
    let typed = raise::prototypes::typed(symbol);
    let kinds: Vec<Kind> = typed.as_ref().map_or_else(|| vec![Kind::Int; 8], |t| t.params.clone());
    let fn_ty = typed.as_ref().map_or_else(state::import_adapter_type, raise::prototypes::Typed::fn_type);
    let mut args = Vec::with_capacity(kinds.len());
    let sp = m.read_sp(block);
    if bits == 64 {
        // A position is one register of either class, then a stack slot of eight bytes.
        for (p, kind) in kinds.iter().enumerate() {
            let v = match (p, kind) {
                (0..=3, Kind::Int | Kind::Wide) => m.read_gpr(block, [R::RCX, R::RDX, R::R8, R::R9][p]),
                (0..=3, Kind::Float | Kind::Double) => m.read_xmm(block, [R::XMM0, R::XMM1, R::XMM2, R::XMM3][p], kind.ty()),
                _ => {
                    let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, stack_offset + 8 * p as i64, "arg.p");
                    m.load(block, kind.ty(), addr, "arg")
                }
            };
            args.push((kind.ty(), v.into()));
        }
    } else {
        // Every argument is on the stack: a dword zero-extended, or the eight bytes of a wide
        // integer or a double.
        let mut offset = stack_offset;
        for kind in &kinds {
            let addr = m.b.bin(block, BinOp::Add, Type::I32, sp, offset, "arg.p");
            let v = match kind {
                Kind::Int => {
                    let v = m.load(block, Type::I32, addr, "arg");
                    m.zext(block, Type::I32, v, Type::I64, "arg.w")
                }
                Kind::Wide => m.load(block, Type::I64, addr, "arg"),
                Kind::Float | Kind::Double => m.load(block, kind.ty(), addr, "arg"),
            };
            args.push((kind.ty(), v.into()));
            offset += if matches!(kind, Kind::Wide | Kind::Double) { 8 } else { 4 };
        }
    }
    let result = m.b.call(block, fn_ty, Callee::Direct(symbol.to_owned()), args, "ret");
    let returns = typed.as_ref().map_or(Some(Kind::Int), |t| t.returns);
    match (returns, result) {
        (Some(Kind::Int | Kind::Wide), Some(result)) if bits == 64 => m.write_gpr(block, R::RAX, result),
        (Some(Kind::Int | Kind::Wide), Some(result)) => {
            let low = m.trunc(block, Type::I64, result, Type::I32, "ret.lo");
            m.write_gpr(block, R::EAX, low);
            let shifted = m.b.bin(block, BinOp::LShr, Type::I64, result, 32i64, "ret.hs");
            let high = m.trunc(block, Type::I64, shifted, Type::I32, "ret.hi");
            m.write_gpr(block, R::EDX, high);
        }
        (Some(kind @ (Kind::Float | Kind::Double)), Some(result)) if bits == 64 => m.write_xmm(block, R::XMM0, kind.ty(), result, true),
        (Some(Kind::Double), Some(result)) => m.fpush(block, result),
        (Some(Kind::Float), Some(result)) => {
            let wide = m.b.cast(block, Cast::FPExt, Type::Float, result, Type::Double, "ret.d");
            m.fpush(block, wide);
        }
        _ => {}
    }
    b.terminate(block, Terminator::Ret(None));
    b.finish()
}

/// The stub of an import the host lacks: the import's own arity when the table knows it (the
/// calls take that many arguments), else the adapter's eight; a stop if it is ever reached.
fn host_lacking_stub(symbol: &str) -> ra_asm_ir::Function {
    use ra_asm_ir::{Callee, FnType, FunctionBuilder, Terminator};
    let arity = raise::prototypes::arity(symbol).unwrap_or(8).min(8);
    let names = ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"];
    let mut b = FunctionBuilder::new(symbol, FnType::new(Type::I64, vec![Type::I64; arity]), &names[..arity]);
    let block = b.block("entry");
    b.call(
        block,
        FnType::new(Type::Void, vec![Type::I64, Type::I64]),
        Callee::Direct("ra_unsupported".into()),
        vec![(Type::I64, 0i64.into()), (Type::I64, 0i64.into())],
        "",
    );
    b.terminate(block, Terminator::Unreachable);
    b.finish()
}

/// The options of a lift.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiftOptions {
    /// Write the data sections as globals and every reference into them as an address of one, so
    /// the output stands without the image. Without it, the code addresses the mapped image.
    pub standalone: bool,
}

/// A data section of the image as a global.
#[derive(Debug, Clone)]
pub struct DataSection {
    pub global: String,
    pub start: u64,
    pub end: u64,
}

impl Lifted {
    /// The options that raise this lift to `level`, by the address width of its image.
    pub fn raise_options(&self, level: RaiseLevel) -> RaiseOptions {
        RaiseOptions { level, bits: self.bits }
    }

    /// The address operand of `address` in standalone mode: a byte of a data section, or a function.
    pub fn address_of(&self, address: u64) -> Option<ra_asm_ir::Operand> {
        if let Some(name) = self.functions.get(&address) {
            return Some(ra_asm_ir::Operand::Address { global: name.clone(), offset: 0 });
        }
        let section = self.data.iter().find(|s| address >= s.start && address < s.end)?;
        Some(ra_asm_ir::Operand::Address { global: section.global.clone(), offset: (address - section.start) as i64 })
    }
}

/// What a lift of a whole program did not cover, so the next lift work is chosen by what a real
/// program needs rather than by a guess. Counted on the machine-level module, before any raise.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    /// The mnemonics the lift traps on, most frequent first.
    pub uncovered: Vec<Uncovered>,
    /// The functions with a trap: entry, name and trap count, most traps first.
    pub functions: Vec<(u64, String, usize)>,
    /// The imports the code calls that the prototype table does not know (and the host has).
    pub unknown_imports: Vec<String>,
    /// The indirect jumps the analysis did not resolve: the jump's address and its function's entry.
    pub unresolved_jumps: Vec<(u64, u64)>,
}

/// One mnemonic the lift does not cover, with the address of one instance.
#[derive(Debug, Clone)]
pub struct Uncovered {
    pub mnemonic: String,
    pub count: usize,
    pub example: u64,
}

impl Coverage {
    /// The traps of the whole program.
    pub fn traps(&self) -> usize {
        self.uncovered.iter().map(|u| u.count).sum()
    }
}

/// The coverage of `lifted`, counted from the `ra_unsupported` calls its IR holds: each names
/// the address and the iced code of the instruction it stands for.
pub fn coverage(analysis: &Analysis<'_>, lifted: &Lifted) -> Coverage {
    use ra_asm_ir::{Callee, Inst, Operand};
    use ra_asm_x86::iced_x86::Code;
    let mut by_mnemonic: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut functions = Vec::new();
    for f in &lifted.module.functions {
        let Some(entry) = f.address else { continue };
        let mut traps = 0;
        for block in &f.blocks {
            for i in &block.instrs {
                let Inst::Call { callee: Callee::Direct(name), args, .. } = &i.inst else { continue };
                if name != "ra_unsupported" {
                    continue;
                }
                let (Some((_, Operand::Int(address))), Some((_, Operand::Int(code)))) = (args.first(), args.get(1)) else { continue };
                let mnemonic = usize::try_from(*code)
                    .ok()
                    .and_then(|c| Code::try_from(c).ok())
                    .map_or_else(|| format!("code {code}"), |c| format!("{:?}", c.mnemonic()).to_ascii_lowercase());
                let slot = by_mnemonic.entry(mnemonic).or_insert((0, *address as u64));
                slot.0 += 1;
                slot.1 = slot.1.min(*address as u64);
                traps += 1;
            }
        }
        if traps > 0 {
            functions.push((entry, analysis.name_of(entry), traps));
        }
    }
    let mut uncovered: Vec<Uncovered> =
        by_mnemonic.into_iter().map(|(mnemonic, (count, example))| Uncovered { mnemonic, count, example }).collect();
    uncovered.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.mnemonic.cmp(&b.mnemonic)));
    functions.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let unknown_imports = lifted
        .imports
        .iter()
        .filter(|name| {
            raise::prototypes::arity(name).is_none() && raise::prototypes::typed(name).is_none() && !raise::prototypes::host_lacks(name)
        })
        .cloned()
        .collect();
    let unresolved_jumps = analysis
        .functions
        .values()
        .flat_map(|f| {
            f.blocks
                .iter()
                .filter(|b| matches!(b.terminator, ra_asm_analysis::Terminator::IndirectJump))
                .filter_map(|b| b.instrs.last().map(|i| (i.address, f.entry)))
        })
        .collect();
    Coverage { uncovered, functions, unknown_imports, unresolved_jumps }
}

/// Lifts every function of `analysis`.
pub fn lift(image: &Image, analysis: &Analysis<'_>) -> Lifted {
    lift_with(image, analysis, LiftOptions::default())
}

/// Lifts every function of `analysis` with `options`.
pub fn lift_with(image: &Image, analysis: &Analysis<'_>, options: LiftOptions) -> Lifted {
    let mut module = Module::x86_64_windows(
        &image.path.as_ref().map_or_else(|| "image".to_owned(), |p| p.file_name().unwrap().to_string_lossy().into_owned()),
    );
    module.types.push((state::STATE_TYPE.to_owned(), state::state_fields()));
    state::declare_runtime(&mut module);
    state::define_runtime_globals(&mut module);

    let mut lifted = Lifted {
        module,
        imports: BTreeSet::new(),
        functions: BTreeMap::new(),
        roots: BTreeSet::new(),
        wrappers: BTreeSet::new(),
        data: Vec::new(),
        standalone: options.standalone,
        bits: if image.is_64() { 64 } else { 32 },
        relocated: image.relocations.iter().map(|r| r.va).collect(),
        warnings: Vec::new(),
    };
    for f in analysis.functions.values() {
        let name = function_name(f.entry);
        let root = f
            .origins
            .iter()
            .any(|o| matches!(o, Origin::Entry | Origin::Export(_) | Origin::TlsCallback | Origin::Pointer(_) | Origin::Vtable(_)));
        if root || matches!(f.kind, Kind::ImportThunk(_)) {
            lifted.roots.insert(name.clone());
        }
        lifted.functions.insert(f.entry, name);
    }
    if options.standalone {
        data::add_sections(image, &mut lifted);
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
        // An import the 64-bit host has no DLL for (the x86 exception handling of the C
        // runtime, which only an exception would call) is a stub that stops the program.
        if raise::prototypes::host_lacks(import) {
            lifted.module.functions.push(host_lacking_stub(import));
            continue;
        }
        let ty = raise::prototypes::typed(import).map_or_else(state::import_adapter_type, |t| t.fn_type());
        lifted.module.declare(Declaration { name: import.clone(), ty, conv: CallConv::C, attrs: Vec::new() });
    }
    for (symbol, offset, tail) in &lifted.wrappers {
        lifted.module.functions.push(import_wrapper(symbol, *offset, *tail, lifted.bits));
    }
    lifted
}

/// The C header the driver includes: the base, the size, the entry, and the imports to bind.
/// With `signatures` (ABI mode), the entry is declared with its own prototype and the driver calls
/// it as a plain function.
pub fn image_header(
    image: &Image,
    analysis: &Analysis<'_>,
    lifted: &Lifted,
    signatures: Option<&std::collections::HashMap<String, raise::abi::Signature>>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "// Generated by ras lift. The image the lifted program runs against.");
    let _ = writeln!(out, "#define RA_IMAGE_BASE 0x{:x}ULL", image.image_base());
    let _ = writeln!(out, "#define RA_IMAGE_SIZE 0x{:x}", image.size_of_image());
    let _ = writeln!(out, "#define RA_POINTER_SIZE {}", image.pointer_size());
    if let Some(entry) = image.entry_point() {
        let name = function_name(entry);
        let _ = writeln!(out, "#define RA_ENTRY {name}");
        match signatures.and_then(|s| s.get(&name)) {
            Some(sig) => {
                let c_type = |k: raise::abi::Kind| match k {
                    raise::abi::Kind::Int | raise::abi::Kind::Wide => "unsigned long long",
                    raise::abi::Kind::Float => "float",
                    raise::abi::Kind::Double => "double",
                };
                let params: Vec<&str> = sig.params.iter().chain(&sig.stack_params).map(|k| c_type(*k)).collect();
                let ret = sig.returns.map_or("void", c_type);
                let _ = writeln!(out, "#define RA_ABI 1");
                if lifted.standalone {
                    let _ = writeln!(out, "#define RA_STANDALONE 1");
                }
                let _ = writeln!(out, "{ret} {name}({});", if params.is_empty() { "void".to_owned() } else { params.join(", ") });
                let zeros: Vec<&str> = params.iter().map(|_| "0").collect();
                let _ = writeln!(out, "#define RA_ENTRY_CALL() {name}({})", zeros.join(", "));
            }
            None => {
                let _ = writeln!(out, "void {name}(void *state);");
            }
        }
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
