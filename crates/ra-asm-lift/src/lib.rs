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

pub mod arm64;
pub mod data;
pub mod eh;
pub mod raise;
pub mod run;
pub mod state;
pub mod thumb;
pub mod x86;
pub mod x87;

pub use raise::{Level as RaiseLevel, Options as RaiseOptions, raise};
pub use state::Isa;
pub use x87::X87Unit;

use std::collections::{BTreeMap, BTreeSet, HashMap};

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
    /// The functions of a C runtime linked into the image, by address, with the host's name for
    /// each: a call to one is a call to the host's runtime, and none of them is lifted.
    pub runtime: BTreeMap<u64, String>,
    /// The import thunks, by address, with the import each jumps to.
    pub thunks: BTreeMap<u64, String>,
    /// Whether the program throws C++ exceptions, which the lift ports over the runtime's
    /// `ra_eh_*` protocol (x64 only so far): see [`eh`].
    pub eh: bool,
    /// The function the port starts at: the analysis's program entry.
    pub entry: Option<u64>,
    /// The functions whose callers the lift does not see: the entry, the exports, the callbacks,
    /// the functions a pointer names.
    pub roots: BTreeSet<String>,
    /// The import wrappers the code calls: the import's symbol, the stack offset of its first
    /// stack argument at the call site, and whether the call is a tail call.
    pub wrappers: BTreeSet<(String, i64, bool)>,
    /// The imports called `fastcall` (the Xbox kernel's), whose wrappers read `ecx` and `edx`
    /// ahead of the stack.
    pub fastcall: BTreeSet<String>,
    /// Whether the imports take the System V convention of a Linux or a macOS binary (an ELF or a
    /// Mach-O image) rather than the Windows one: in 64 bits six integers in `rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9`,
    /// eight floats in `xmm0` to `xmm7`, and no shadow space above the return address.
    pub sysv: bool,
    /// The data sections written as globals (standalone mode).
    pub data: Vec<DataSection>,
    /// The vtables of the program's classes, from the RTTI: the raise resolves a call through a
    /// slot of one to the functions that fill the slot.
    pub vtables: Vec<ra_asm_analysis::rtti::Vtable>,
    /// Whether the lift references data through globals rather than the mapped image.
    pub standalone: bool,
    /// 32 or 64: the address width of the image.
    pub bits: u32,
    /// The instruction set of the image: whose registers `%State` holds.
    pub isa: Isa,
    /// The x87 unit the lift modelled.
    pub x87: X87Unit,
    /// The addresses of the words the base relocations name: an immediate or a displacement at
    /// one of them holds an address of the image, which 32-bit code writes as a plain number.
    pub relocated: std::collections::HashSet<u64>,
    /// The indirect calls and jumps a traced run resolved, by the address of the instruction,
    /// with the lifted functions each reached: the raise calls them directly.
    pub traced_calls: BTreeMap<u64, Vec<u64>>,
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

/// The convention an import's wrapper reads its arguments in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convention {
    /// Win64 in 64 bits; `cdecl` or `stdcall` in 32, whose arguments are all on the stack.
    Windows,
    /// The Xbox kernel's 32-bit `fastcall`: `ecx` and `edx` first.
    Fastcall,
    /// The System V AMD64 convention of a Linux binary.
    SystemV,
}

/// Builds the wrapper of an import. In 64 bits it reads `rcx`, `rdx`, `r8`, `r9` and four stack
/// slots from `rsp + stack_offset` out of the state, calls the import through the eight-integer
/// adapter, and stores the result to `rax`. In 32 bits every argument is on the stack: it reads
/// eight dwords from `esp + stack_offset`, zero-extended, and stores the result to `eax` and its
/// high half to `edx`. The caller pops what a `stdcall` import pops, since it knows the import.
/// An import with a typed prototype (the C runtime's math) takes its own arguments instead, a
/// double read as one, and its result goes where the convention puts it: `xmm0`, or in 32 bits
/// the x87 stack. A `fastcall` import (the Xbox kernel's) takes its first two integers from `ecx`
/// and `edx`, and the stack holds the rest. A System V import (a Linux binary's) takes its
/// integers from `rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9` and its floats from `xmm0` to `xmm7`, each
/// class counted on its own, and the stack holds what is left in order.
pub fn import_wrapper(symbol: &str, stack_offset: i64, tail: bool, bits: u32, x87: X87Unit, convention: Convention) -> ra_asm_ir::Function {
    use ra_asm_ir::{BinOp, Callee, Cast, FnType, FunctionBuilder, Terminator};
    use ra_asm_x86::iced_x86::Register as R;
    use raise::abi::Kind;
    let name = import_wrapper_name(symbol, tail);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_linkage(Linkage::Internal);
    let state = b.param(0);
    let block = b.block("entry");
    let mut m = state::Machine { b: &mut b, state, bits, x87, isa: state::Isa::X86 };
    let typed = raise::prototypes::typed(symbol);
    let kinds: Vec<Kind> = typed.as_ref().map_or_else(|| vec![Kind::Int; 8], |t| t.params.clone());
    let fn_ty = typed.as_ref().map_or_else(state::import_adapter_type, raise::prototypes::Typed::fn_type);
    let mut args = Vec::with_capacity(kinds.len());
    let sp = m.read_sp(block);
    if bits == 64 && convention == Convention::SystemV {
        let (mut ints, mut floats, mut slot) = (0usize, 0usize, 0i64);
        for kind in &kinds {
            let v = match kind {
                Kind::Int | Kind::Wide if ints < 6 => {
                    ints += 1;
                    m.read_gpr(block, [R::RDI, R::RSI, R::RDX, R::RCX, R::R8, R::R9][ints - 1])
                }
                Kind::Float | Kind::Double if floats < 8 => {
                    floats += 1;
                    m.read_xmm(block, [R::XMM0, R::XMM1, R::XMM2, R::XMM3, R::XMM4, R::XMM5, R::XMM6, R::XMM7][floats - 1], kind.ty())
                }
                _ => {
                    let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, stack_offset + 8 * slot, "arg.p");
                    slot += 1;
                    m.load(block, kind.ty(), addr, "arg")
                }
            };
            args.push((kind.ty(), v.into()));
        }
    } else if bits == 64 {
        // A position is one register of either class, then a stack slot of eight bytes.
        for (p, kind) in kinds.iter().enumerate() {
            let v = match (p, kind) {
                (0..=3, Kind::Int | Kind::Wide) => m.read_gpr(block, [R::RCX, R::RDX, R::R8, R::R9][p]),
                (0..=3, Kind::Float | Kind::Double) => m.read_xmm(block, [R::XMM0, R::XMM1, R::XMM2, R::XMM3][p], kind.ty()),
                _ => {
                    // The first stack slot is at the offset: a position past the four registers.
                    let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, stack_offset + 8 * (p as i64 - 4), "arg.p");
                    m.load(block, kind.ty(), addr, "arg")
                }
            };
            args.push((kind.ty(), v.into()));
        }
    } else {
        // Every argument is on the stack: a dword zero-extended, or the eight bytes of a wide
        // integer or a double.
        let mut offset = stack_offset;
        for (p, kind) in kinds.iter().enumerate() {
            if convention == Convention::Fastcall && p < 2 {
                let v = m.read_gpr(block, [R::ECX, R::EDX][p]);
                let v = m.zext(block, Type::I32, v, Type::I64, "arg.w");
                args.push((Type::I64, v.into()));
                continue;
            }
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

/// The stub of an import the host lacks: the import's own prototype when the table knows it (the
/// calls take that shape, and a host header may declare it so), else the adapter's eight integers;
/// a stop if it is ever reached.
fn host_lacking_stub(symbol: &str) -> ra_asm_ir::Function {
    use ra_asm_ir::{Callee, FnType, FunctionBuilder, Terminator};
    let ty = raise::prototypes::typed(symbol)
        .map_or_else(|| FnType::new(Type::I64, vec![Type::I64; raise::prototypes::arity(symbol).unwrap_or(8).min(8)]), |t| t.fn_type());
    let names = ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"];
    let arity = ty.params.len().min(8);
    let mut b = FunctionBuilder::new(symbol, ty, &names[..arity]);
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
    /// The x87 unit: eight doubles, or the exact 80-bit unit through the runtime's `f80`.
    pub x87: X87Unit,
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
        RaiseOptions {
            level,
            bits: self.bits,
            x87: self.x87,
            isa: self.isa,
            vtables: self.vtables.clone(),
            traced_calls: self.traced_calls.clone(),
        }
    }

    /// Gives every lifted function the analysis's name for it (`main_`, `Shape_vf1`, `fib`) in
    /// place of `F_<address>`, at its definition, its calls and wherever its address is taken; a
    /// function nothing names keeps `F_<address>`. Runs after the raise, whose passes know a
    /// lifted function by the `F_` prefix; `signatures`, keyed by the old names, follow.
    pub fn apply_names(&mut self, analysis: &Analysis<'_>, signatures: Option<&mut HashMap<String, raise::abi::Signature>>) {
        use ra_asm_ir::{Callee, Inst};
        let renamed: HashMap<String, String> =
            analysis.ported().filter(|f| f.name != function_name(f.entry)).map(|f| (function_name(f.entry), f.name.clone())).collect();
        if renamed.is_empty() {
            return;
        }
        for f in &mut self.module.functions {
            if let Some(name) = renamed.get(&f.name) {
                f.name = name.clone();
            }
            for b in &mut f.blocks {
                for i in &mut b.instrs {
                    if let Inst::Call { callee: Callee::Direct(callee), .. } = &mut i.inst
                        && let Some(name) = renamed.get(callee)
                    {
                        *callee = name.clone();
                    }
                    for o in i.inst.operands_mut() {
                        raise::stubs::rename_operand(o, &renamed);
                    }
                }
                for o in b.terminator.operands_mut() {
                    raise::stubs::rename_operand(o, &renamed);
                }
            }
        }
        for g in &mut self.module.globals {
            if let Some(init) = &mut g.init {
                raise::stubs::rename_const(init, &renamed);
            }
        }
        for name in self.functions.values_mut() {
            if let Some(new) = renamed.get(name) {
                *name = new.clone();
            }
        }
        self.roots = self.roots.iter().map(|r| renamed.get(r).unwrap_or(r).clone()).collect();
        if let Some(signatures) = signatures {
            *signatures = signatures.drain().map(|(k, v)| (renamed.get(&k).cloned().unwrap_or(k), v)).collect();
        }
    }

    /// A lift of the same tables with nothing lifted yet and an empty module: what one thread of a
    /// parallel lift writes into, whose outputs join the lift's after.
    fn worker(&self) -> Lifted {
        Lifted {
            module: Module::default(),
            imports: BTreeSet::new(),
            functions: self.functions.clone(),
            runtime: self.runtime.clone(),
            thunks: self.thunks.clone(),
            eh: self.eh,
            entry: self.entry,
            roots: BTreeSet::new(),
            wrappers: BTreeSet::new(),
            fastcall: BTreeSet::new(),
            sysv: self.sysv,
            data: self.data.clone(),
            vtables: Vec::new(),
            standalone: self.standalone,
            bits: self.bits,
            isa: self.isa,
            x87: self.x87,
            relocated: self.relocated.clone(),
            traced_calls: self.traced_calls.clone(),
            warnings: Vec::new(),
        }
    }

    /// The address operand of `address` in standalone mode: a byte of a data section, or a function.
    /// A base just before a section (within a page) is a byte of that section at a negative
    /// offset: the compiler folds the first index of a table into the displacement, so
    /// `[ecx*4 + table - 8]` names the table's section from before its start.
    pub fn address_of(&self, address: u64) -> Option<ra_asm_ir::Operand> {
        if let Some(name) = self.functions.get(&address) {
            return Some(ra_asm_ir::Operand::Address { global: name.clone(), offset: 0 });
        }
        if let Some(section) = self.data.iter().find(|s| address >= s.start && address < s.end) {
            return Some(ra_asm_ir::Operand::Address { global: section.global.clone(), offset: (address - section.start) as i64 });
        }
        let section = self.data.iter().filter(|s| address < s.start && s.start - address < 0x1000).min_by_key(|s| s.start - address)?;
        Some(ra_asm_ir::Operand::Address { global: section.global.clone(), offset: -((section.start - address) as i64) })
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
                // An ARM64 code is yaxpeax's, which names no mnemonic: the instruction at the
                // address does.
                let mnemonic = match lifted.isa {
                    Isa::Arm64 | Isa::Arm32 => analysis
                        .image
                        .read(*address as u64, 4)
                        .ok()
                        .and_then(|bytes| analysis.arch.decode(bytes, *address as u64))
                        .map_or_else(|| format!("code {code}"), |i| i.mnemonic),
                    Isa::X86 => usize::try_from(*code)
                        .ok()
                        .and_then(|c| Code::try_from(c).ok())
                        .map_or_else(|| format!("code {code}"), |c| format!("{:?}", c.mnemonic()).to_ascii_lowercase()),
                };
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
    lift_on(image, analysis, options, None)
}

/// [`lift_with`] on `threads` threads, or on as many as the host and the program's size call for.
fn lift_on(image: &Image, analysis: &Analysis<'_>, options: LiftOptions, threads: Option<usize>) -> Lifted {
    let isa = match image.machine() {
        ra_asm_pe::Machine::Arm64 => Isa::Arm64,
        ra_asm_pe::Machine::Arm => Isa::Arm32,
        _ => Isa::X86,
    };
    let name = image.path.as_ref().map_or_else(|| "image".to_owned(), |p| p.file_name().unwrap().to_string_lossy().into_owned());
    let mut module = match isa {
        Isa::X86 if image.elf.is_some() => Module::x86_64_linux(&name),
        // A 32-bit program's port is a program of the 64-bit host that keeps its 32-bit pointers.
        Isa::X86 | Isa::Arm32 => Module::x86_64_windows(&name),
        Isa::Arm64 => Module::aarch64_windows(&name),
    };
    module.types.push((state::STATE_TYPE.to_owned(), state::state_fields_for(isa)));
    state::declare_runtime(&mut module);
    if isa == Isa::Arm32 {
        state::declare_svc(&mut module);
    }
    state::define_runtime_globals(&mut module, options.x87);
    let runtime: BTreeMap<u64, String> = analysis
        .functions
        .values()
        .filter_map(|f| match &f.kind {
            Kind::Runtime(name) => Some((f.entry, name.clone())),
            _ => None,
        })
        .collect();
    let thunks: BTreeMap<u64, String> = analysis
        .functions
        .values()
        .filter_map(|f| match &f.kind {
            Kind::ImportThunk(name) => Some((f.entry, name.clone())),
            _ => None,
        })
        .collect();
    // A program throws when it has the throw to call or a frame to catch in; the x86 frame
    // handler's registration frames are not lifted yet.
    let throws = |name: &String| name == "_CxxThrowException";
    let eh = image.is_64()
        && (!analysis.cxx_frames.is_empty()
            || thunks.values().any(throws)
            || runtime.values().any(throws)
            || image.imports.iter().any(|i| throws(&i.display_name())));
    if eh {
        state::declare_eh_runtime(&mut module);
    }

    let mut lifted = Lifted {
        module,
        imports: BTreeSet::new(),
        functions: BTreeMap::new(),
        runtime,
        thunks,
        eh,
        entry: analysis.program_entry,
        roots: BTreeSet::new(),
        wrappers: BTreeSet::new(),
        fastcall: BTreeSet::new(),
        sysv: image.elf.is_some() || image.macho.is_some(),
        data: Vec::new(),
        vtables: ra_asm_analysis::rtti::vtables(&analysis.classes),
        standalone: options.standalone,
        bits: if image.is_64() { 64 } else { 32 },
        isa,
        x87: options.x87,
        relocated: image.relocations.iter().map(|r| r.va).collect(),
        traced_calls: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for f in analysis.ported() {
        let name = function_name(f.entry);
        // The program entry is a root too: `main`'s caller is the runtime's startup, which is not lifted.
        let root = f
            .origins
            .iter()
            .any(|o| matches!(o, Origin::Entry | Origin::Export(_) | Origin::TlsCallback | Origin::Pointer(_) | Origin::Vtable(_)));
        if root || matches!(f.kind, Kind::ImportThunk(_)) || analysis.program_entry == Some(f.entry) {
            lifted.roots.insert(name.clone());
        }
        lifted.functions.insert(f.entry, name);
    }
    // A traced call resolves to the targets that are lifted: a runtime function or an import the
    // run reached stays with the indirect call, which the host answers.
    for f in analysis.ported() {
        for (&site, targets) in &f.traced_calls {
            let lifted_targets: Vec<u64> = targets.iter().copied().filter(|t| lifted.functions.contains_key(t)).collect();
            if !lifted_targets.is_empty() {
                lifted.traced_calls.insert(site, lifted_targets);
            }
        }
    }
    if options.standalone {
        data::add_sections(image, &mut lifted);
    }
    let tls = state::tls_global(image, &lifted);
    lifted.module.globals.push(tls);
    lift_functions(image, analysis, &mut lifted, threads);
    // A function whose address the code or the data holds is a root as well: a window procedure
    // or a thread's start the system calls back, whose callers the raise never sees, so it keeps
    // the result it writes.
    for name in raise::stubs::address_taken(&lifted.module) {
        if name.starts_with("F_") {
            lifted.roots.insert(name);
        }
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
    // A 32-bit callee reached through a pointer pops what its convention says: the runtime finds
    // an import's pops in the binding table.
    if lifted.bits == 32 {
        lifted.module.globals.extend(state::import_pops_globals(image, &lifted));
    }
    for (symbol, offset, tail) in &lifted.wrappers {
        let wrapper = match isa {
            Isa::X86 => {
                let convention = if lifted.sysv {
                    Convention::SystemV
                } else if lifted.fastcall.contains(symbol) {
                    Convention::Fastcall
                } else {
                    Convention::Windows
                };
                import_wrapper(symbol, *offset, *tail, lifted.bits, lifted.x87, convention)
            }
            Isa::Arm64 => arm64::import_wrapper(symbol, lifted.x87),
            // The Thumb-2 lift calls no import: it traps at a call it cannot resolve.
            Isa::Arm32 => continue,
        };
        lifted.module.functions.push(wrapper);
    }
    lifted
}

/// The fewest functions a lift spreads over threads: below it a thread costs more than it saves.
const PARALLEL_FROM: usize = 64;

/// Lifts the ported functions of `analysis` into `lifted`, on `threads` threads or on every core of
/// the host for a program of [`PARALLEL_FROM`] functions or more. A function's lift reads only the
/// analysis and the lift's tables, so each thread lifts a run of consecutive functions into a
/// worker of its own and the runs join in address order: the module, the warnings, the imports
/// and every other output are the ones a lift on one thread writes.
fn lift_functions(image: &Image, analysis: &Analysis<'_>, lifted: &mut Lifted, threads: Option<usize>) {
    let lift_one = |lifted: &mut Lifted, f: &ra_asm_analysis::Function| {
        let built = match lifted.isa {
            Isa::X86 => x86::lift_function(image, analysis, lifted, f),
            Isa::Arm64 => arm64::lift_function(image, analysis, lifted, f),
            Isa::Arm32 => thumb::lift_function(image, analysis, lifted, f),
        };
        lifted.module.functions.push(built);
    };
    let ported: Vec<&ra_asm_analysis::Function> = analysis.ported().collect();
    let threads = threads.unwrap_or_else(|| {
        if ported.len() < PARALLEL_FROM { 1 } else { std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get) }
    });
    if threads <= 1 || ported.len() < 2 {
        for f in ported {
            lift_one(lifted, f);
        }
        return;
    }
    let run = ported.len().div_ceil(threads);
    let workers: Vec<Lifted> = std::thread::scope(|scope| {
        let handles: Vec<_> = ported
            .chunks(run)
            .map(|functions| {
                let mut worker = lifted.worker();
                scope.spawn(move || {
                    for f in functions {
                        lift_one(&mut worker, f);
                    }
                    worker
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))).collect()
    });
    let data_before = lifted.data.len();
    for worker in workers {
        // Every field by name, so a field the lift comes to write cannot be left behind here.
        let Lifted {
            module,
            imports,
            functions,
            runtime,
            thunks,
            eh: _,
            entry: _,
            roots,
            wrappers,
            fastcall,
            sysv: _,
            data,
            vtables,
            standalone: _,
            bits: _,
            isa: _,
            x87: _,
            relocated,
            traced_calls,
            warnings,
        } = worker;
        let Module { name: _, triple: _, data_layout: _, types, globals, declarations, functions: built } = module;
        lifted.module.types.extend(types);
        lifted.module.globals.extend(globals);
        lifted.module.declarations.extend(declarations);
        lifted.module.functions.extend(built);
        lifted.imports.extend(imports);
        lifted.roots.extend(roots);
        lifted.wrappers.extend(wrappers);
        lifted.fastcall.extend(fastcall);
        // A worker's sections start as a copy of the lift's: only the ones it added are new.
        lifted.data.extend(data.into_iter().skip(data_before));
        lifted.vtables.extend(vtables);
        lifted.warnings.extend(warnings);
        // The tables a function's lift reads and never writes.
        debug_assert_eq!(functions.len(), lifted.functions.len());
        debug_assert_eq!(runtime.len(), lifted.runtime.len());
        debug_assert_eq!(thunks.len(), lifted.thunks.len());
        debug_assert_eq!(relocated.len(), lifted.relocated.len());
        debug_assert_eq!(traced_calls.len(), lifted.traced_calls.len());
    }
}

/// The C header the driver includes: the base, the size, the entry, and the imports to bind.
/// With `signatures` (ABI mode), the entry is declared with its own prototype and the driver calls
/// it as a plain function. When the entry is the program's `main` rather than the image's entry
/// point (a C runtime's startup the host's replaces), the call hands it the driver's arguments.
/// A DLL's entry is its `DllMain`: `RA_DLL` is set and the call takes the instance, the reason and
/// the reserved pointer the loader gives the port's own.
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
    let dll = image.headers.coff.is_dll();
    if dll {
        let _ = writeln!(out, "#define RA_DLL 1");
    }
    if let Some(entry) = lifted.entry.or_else(|| image.entry_point()) {
        let name = lifted.functions.get(&entry).cloned().unwrap_or_else(|| function_name(entry));
        let is_main = lifted.entry.is_some() && lifted.entry != image.entry_point();
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
                // `main(argc, argv)` takes the driver's; the argv of a 32-bit program is the
                // runtime's table of dword pointers. A DLL's entry takes the loader's three.
                let args: Vec<&str> = params
                    .iter()
                    .enumerate()
                    .map(|(i, _)| match (is_main, dll, i, image.pointer_size()) {
                        (true, _, 0, _) => "(unsigned long long)ra_argc",
                        (true, _, 1, 8) => "(unsigned long long)(uintptr_t)ra_argv",
                        (true, _, 1, _) => "*(unsigned *)(uintptr_t)ra_x86_p_argv()",
                        (false, true, 0, _) => "(unsigned long long)(uintptr_t)(instance)",
                        (false, true, 1, _) => "(unsigned long long)(reason)",
                        (false, true, 2, _) => "(unsigned long long)(uintptr_t)(reserved)",
                        _ => "0",
                    })
                    .collect();
                let macro_params = if dll { "instance, reason, reserved" } else { "" };
                let _ = writeln!(out, "#define RA_ENTRY_CALL({macro_params}) {name}({})", args.join(", "));
                // `main`'s result is the exit code the runtime's startup would have passed to
                // `exit`; a DLL's entry answers the loader whether the attach succeeded.
                if (is_main || dll) && sig.returns.is_some() {
                    let _ = writeln!(out, "#define RA_ENTRY_RETURNS 1");
                }
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

#[cfg(test)]
mod parallel_tests {
    use super::*;

    /// A lift on many threads writes what a lift on one writes: the same IR, warnings, imports and
    /// wrappers, for 32- and 64-bit code, with exceptions, and over the image or standalone.
    #[test]
    fn a_lift_on_many_threads_is_the_lift_on_one() {
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin");
        let fixtures = [("control64.exe", true), ("control32.exe", true), ("classes32.exe", false), ("throws64.exe", true)];
        for (fixture, standalone) in fixtures {
            let image = ra_asm_pe::load(&bin.join(fixture)).unwrap();
            let analysis = ra_asm_analysis::analyze(&image);
            let options = || LiftOptions { standalone, x87: X87Unit::Double };
            let one = lift_on(&image, &analysis, options(), Some(1));
            let many = lift_on(&image, &analysis, options(), Some(5));
            assert!(analysis.ported().count() >= 5, "{fixture}: too few functions to spread over the threads");
            assert_eq!(one.module.to_ll(), many.module.to_ll(), "{fixture}: the IR differs");
            assert_eq!(one.warnings, many.warnings, "{fixture}");
            assert_eq!(one.imports, many.imports, "{fixture}");
            assert_eq!(one.wrappers, many.wrappers, "{fixture}");
            assert_eq!(one.fastcall, many.fastcall, "{fixture}");
            assert_eq!(one.roots, many.roots, "{fixture}");
            assert_eq!(one.data.len(), many.data.len(), "{fixture}");
        }
    }
}
