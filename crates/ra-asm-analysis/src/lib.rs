//! The analysis of an image: the functions, their basic blocks and the control-flow graph, the
//! jump tables, and the functions that never return.
//!
//! [`analyze`] runs a fixpoint: seeds (the entry point, the exports, the TLS callbacks, the
//! exception table) become functions, each function's recursive descent finds callees and tail
//! calls that become functions in turn, a call target that lands inside a built function splits
//! it, and a function proven to never return makes its callers rebuild without the fall-through.
//! The vtables the RTTI names come next, then pointer-sized data slots that hold code addresses,
//! since a jump table on x86 holds such addresses too and the functions reached by code must own
//! them first. Last, each function learns the string literals it addresses and the vtable slots
//! that hold it, and the names settle: a PDB beside the image names what it describes and marks
//! the functions of a linked-in C runtime as the host's, a link map beside it (or one the project
//! names) names what it lists, a function the program instantiated from the standard library's
//! templates is named after its container and member ([`stl`]: `std::vector<unsigned int>::insert`
//! as the identifier `std_vector_unsigned_int_insert`), a function nothing names takes the name of
//! its vtable slot, and every name becomes an identifier unique in the program. The signatures of a library
//! ([`signatures`]) name the functions of a build without symbols the same way: applied to the
//! image first, they fill the symbols and the library ranges the analysis reads. The facts of a
//! traced run ([`facts`]) come last among the seeds and first among the resolutions: a target the
//! run reached is a function, and an indirect call the run resolved calls what it reached. A
//! function's [`identity`] is what it is apart from its address, how a new build of the program
//! finds the functions the person annotated in the last; [`matching`] matches every function of
//! two builds at once, by their symbols or by structure alone, for a binary diff.

pub mod builder;
pub mod eh32;
pub mod facts;
pub mod function;
pub mod gaps;
pub mod identity;
pub mod imports;
pub mod jumptable;
pub mod matching;
pub mod noreturn;
pub mod rpc;
pub mod rtti;
pub mod seh32;
pub mod signatures;
pub mod stl;
pub mod symbols;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_arch::{Arch, Flow, Operand};
use ra_asm_pe::{CxxAbi, CxxFrame, Image, Machine, Toolset};

pub use eh32::CxxFrame32;
pub use facts::Facts;
pub use function::{Block, Function, Kind, Origin, Reach, Terminator, VirtualSlot};
pub use identity::Identity;
pub use rtti::{Class, Vtable};
pub use seh32::{Scope, SehFrame};
pub use signatures::{Signature, SignatureSet};

/// ELF's `EM_MIPS` in a 64-bit file, which [`Machine::Mips`] (the 32-bit one) does not name.
const EM_MIPS: u16 = 8;

/// The decoder of an image's machine.
pub fn arch_of(image: &Image) -> Box<dyn Arch> {
    match image.machine() {
        Machine::Arm64 => Box::new(ra_asm_arm64::Arm64),
        Machine::Arm => Box::new(ra_asm_arm64::thumb::Thumb),
        Machine::Mips | Machine::Other(EM_MIPS) => Box::new(ra_asm_mips::Mips {
            big_endian: image.big_endian(),
            r5900: image.platform() == Some("ps2"),
            allegrex: image.platform() == Some("psp"),
            vr4300: image.platform() == Some("n64"),
            r3000: image.platform() == Some("psx"),
        }),
        Machine::Sh4 => Box::new(ra_asm_sh::Sh4 { big_endian: image.big_endian(), ..ra_asm_sh::Sh4::default() }),
        _ => Box::new(ra_asm_x86::X86::for_pointer_size(image.pointer_size())),
    }
}

/// The analysis of one image.
pub struct Analysis<'a> {
    pub image: &'a Image,
    pub arch: Box<dyn Arch>,
    /// Every function, by entry address.
    pub functions: BTreeMap<u64, Function>,
    /// The functions that never return, by entry address.
    pub noreturn: HashSet<u64>,
    /// The classes with virtual functions, from the RTTI.
    pub classes: Vec<Class>,
    /// The x64 functions under the C++ frame handler, by entry: their tables and their funclets,
    /// which are blocks of the function.
    pub cxx_frames: BTreeMap<u64, CxxFrame>,
    /// The 32-bit x86 functions that register a structured exception frame, by entry: their
    /// scope table and the ranges each `__try` protects ([`seh32`]).
    pub seh_frames: BTreeMap<u64, SehFrame>,
    /// The functions whose structured exception frame was found and not read, with the reason:
    /// their `__try` ranges are unknown.
    pub seh_unread: BTreeMap<u64, String>,
    /// The 32-bit x86 functions that register the C++ frame handler, by entry: their tables and
    /// the states their code is in ([`eh32`]).
    pub cxx_frames32: BTreeMap<u64, CxxFrame32>,
    /// The functions whose C++ frame was found and not read, with the reason: their states are
    /// unknown.
    pub cxx_unread32: BTreeMap<u64, String>,
    /// The function a port starts at: the image's entry point, or `main` when the entry is the
    /// startup of a C runtime linked into the image, whose work the host's own startup does.
    pub program_entry: Option<u64>,
    /// The functions whose address the program takes ([`gaps::address_taken`]), which a computed
    /// call or jump can reach and the port resolves by address; `None` when every function may be
    /// (an analysis of a few functions built by hand).
    pub address_taken: Option<BTreeSet<u64>>,
    /// The entries the gaps showed whose bytes are fill alone, which are no functions
    /// ([`gaps::classify`]).
    pub fill: Vec<u64>,
    /// The facts of the traced run the analysis took, empty without one.
    pub facts: Facts,
    /// The toolset that built the image: its C++ ABI decides whose RTTI the classes are read
    /// from, its runtime key which signatures and prototypes the C runtime takes.
    pub toolset: Toolset,
    /// What the analysis could not resolve: unresolved indirect jumps, invalid bytes, overlaps.
    pub warnings: Vec<String>,
}

/// Address ranges in order, disjoint: the spans of the functions ([`Analysis::function_spans`]).
pub struct Spans(Vec<(u64, u64)>);

impl Spans {
    /// Whether a span holds `va`.
    pub fn contains(&self, va: u64) -> bool {
        let i = self.0.partition_point(|&(start, _)| start <= va);
        i > 0 && va < self.0[i - 1].1
    }
}

impl Analysis<'_> {
    /// The spans of the functions, each from its lowest block to the end of its highest, merged
    /// where they overlap: an address outside them is data, or code no function was built over.
    pub fn function_spans(&self) -> Spans {
        let mut spans: Vec<(u64, u64)> = self
            .functions
            .values()
            .filter_map(|f| Some((f.blocks.iter().map(|b| b.start).min()?, f.blocks.iter().map(|b| b.end).max()?)))
            .collect();
        spans.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(spans.len());
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        Spans(merged)
    }

    /// The function that starts at `va`.
    pub fn function(&self, va: u64) -> Option<&Function> {
        self.functions.get(&va)
    }

    /// The function whose blocks hold `va`.
    pub fn containing(&self, va: u64) -> Option<&Function> {
        self.functions.values().find(|f| f.contains(va))
    }

    /// The name of the function at `va`, or `F_<address>` when none is there.
    pub fn name_of(&self, va: u64) -> String {
        self.functions.get(&va).map_or_else(|| function::default_name(va), |f| f.name.clone())
    }

    /// The functions that call `callee` directly.
    pub fn callers(&self, callee: u64) -> Vec<&Function> {
        self.functions.values().filter(|f| f.calls.contains(&callee)).collect()
    }

    /// The functions a port lifts: every one but those of a C runtime linked into the image,
    /// which the host's runtime supplies, and the dead code the gaps showed ([`Reach::Dead`]).
    pub fn ported(&self) -> impl Iterator<Item = &Function> {
        self.functions.values().filter(|f| !matches!(f.kind, Kind::Runtime(_)) && !f.is_dead())
    }

    /// Whether the program takes the address of the function at `va`: a computed call or jump can
    /// reach it, and the port resolves it by its address.
    pub fn is_address_taken(&self, va: u64) -> bool {
        self.address_taken.as_ref().is_none_or(|taken| taken.contains(&va))
    }

    /// How many functions only the gaps showed are dead and how many are not.
    pub fn gap_counts(&self) -> (usize, usize) {
        let dead = self.functions.values().filter(|f| f.is_dead()).count();
        let gap = self.functions.values().filter(|f| f.reach == Reach::Gap).count();
        (dead, gap)
    }

    /// Gives back the decoded instructions of every block, keeping the functions, their blocks'
    /// extents and terminators and everything the analysis found: for a translation past its
    /// lift, which reads the lifted IR and no longer the instructions. A large image's decoded
    /// instructions are most of its analysis (gigabytes for 100,000 functions); what still reads
    /// them (a checked translation's assumptions, `ras cfg`, the lift itself) runs before this.
    pub fn release_instructions(&mut self) {
        for f in self.functions.values_mut() {
            for b in &mut f.blocks {
                b.instrs = Vec::new();
            }
        }
    }
}

/// The protector or packer whose stub wraps the image, by the sections it leaves: the program's
/// own code is then not in the image's code at all, and the lift of what is there is a lift of
/// the stub. `None` when no known protector shows.
pub fn protector_of(image: &Image) -> Option<&'static str> {
    image.sections.iter().find_map(|s| protector_by_section(&s.name))
}

/// The protector a section name belongs to.
pub fn protector_by_section(name: &str) -> Option<&'static str> {
    let name = name.trim_end_matches('\0');
    Some(match name {
        "UPX0" | "UPX1" | "UPX2" | "UPX3" | ".UPX0" | ".UPX1" => "UPX",
        ".vmp0" | ".vmp1" | ".vmp2" => "VMProtect",
        ".themida" | ".winlice" => "Themida",
        ".aspack" | ".adata" => "ASPack",
        ".petite" => "Petite",
        ".enigma1" | ".enigma2" => "Enigma",
        ".MPRESS1" | ".MPRESS2" => "MPRESS",
        ".nsp0" | ".nsp1" | ".nsp2" => "NsPack",
        ".pelock" => "PELock",
        ".Upack" | ".ByDwing" => "Upack",
        ".neolite" | ".neolit" => "NeoLite",
        ".RLPack" | ".packed" => "RLPack",
        "PEC2TO" | "PEC2MO" | "PEC2" | "pec1" | "pec2" => "PECompact",
        ".yP" | ".y0da" => "yoda's Protector",
        "kkrunchy" => "kkrunchy",
        ".taz" => "PESpin",
        ".svkp" => "SVKP",
        ".tsustub" | ".tsuarch" => "TSULoader",
        ".perplex" => "Perplex",
        ".sforce3" => "StarForce",
        ".mackt" => "ImpREC",
        ".ccg" => "CCG",
        "DAStub" => "DAStub",
        ".boom" => "The Boomerang List Builder",
        _ => return None,
    })
}

#[cfg(test)]
mod protector_tests {
    use super::protector_by_section;

    #[test]
    fn the_common_protectors_are_known_by_their_sections() {
        assert_eq!(protector_by_section("UPX1"), Some("UPX"));
        assert_eq!(protector_by_section(".vmp0\0\0"), Some("VMProtect"));
        assert_eq!(protector_by_section(".themida"), Some("Themida"));
        assert_eq!(protector_by_section(".text"), None);
        assert_eq!(protector_by_section(".rdata"), None);
    }
}

/// The bound of every rebuild loop. A real image settles in a few rounds; the bound is for a
/// malformed one.
const MAX_ROUNDS: usize = 64;

/// Analyzes `image`: finds its functions and builds their control-flow graphs.
pub fn analyze(image: &Image) -> Analysis<'_> {
    analyze_with(image, &Facts::default())
}

/// [`analyze`] with the `facts` of a traced run: a target an indirect call reached is a function,
/// and the call resolves to the functions it reached.
pub fn analyze_with<'a>(image: &'a Image, facts: &Facts) -> Analysis<'a> {
    let arch = arch_of(image);
    let cxx_frames: BTreeMap<u64, CxxFrame> = ra_asm_pe::exception::cxx_frames(image).into_iter().map(|f| (f.entry.begin, f)).collect();
    let mut state = State::new(image, arch.as_ref(), &cxx_frames);

    // The RTTI read is Microsoft's; a toolset of the Itanium ABI lays its classes out otherwise.
    let toolset = Toolset::of(image);
    let classes = if toolset.cxx_abi() == CxxAbi::Msvc { rtti::classes(image) } else { Vec::new() };
    // The words of the vtables, with the locator slot before each: an export there is data.
    let ptr = u64::from(image.pointer_size());
    let vtable_words: HashSet<u64> = classes
        .iter()
        .flat_map(|c| &c.vtables)
        .flat_map(|v| (0..=v.functions.len() as u64).map(move |i| v.address + i * ptr - ptr))
        .collect();

    // The seeds, in the order that names them best: the entry, the exports, the callbacks, .pdata.
    if let Some(entry) = image.entry_point() {
        state.seed(entry, Origin::Entry);
    }
    if let Some(exports) = &image.exports {
        for export in &exports.exports {
            // A DLL exports its data too (a vtable, a vbtable, a constant), which lies in the code
            // section of an image that keeps its read-only data there.
            if let Some(va) = export.va
                && image.is_code(va)
                && !vtable_words.contains(&va)
                && !export.name.as_deref().is_some_and(is_data_export)
            {
                state.seed(va, Origin::Export(export.name.clone().unwrap_or_else(|| format!("ordinal_{}", export.ordinal))));
            }
        }
    }
    if let Some(tls) = &image.tls {
        for &callback in &tls.callbacks {
            state.seed(callback, Origin::TlsCallback);
        }
    }
    // A funclet of a C++ frame has an entry of its own but is code of its parent.
    for f in &image.runtime_functions {
        let chained = f.unwind_info(image).is_ok_and(|u| u.is_chained());
        if !chained && !state.funclets.contains(&f.begin) {
            state.seed(f.begin, Origin::ExceptionTable);
        }
    }
    // An ELF file's `.eh_frame` and a Mach-O file's `__unwind_info` record its functions as the
    // exception table records a PE's.
    for &(begin, _) in image.unwind_extents() {
        if image.is_code(begin) {
            state.seed(begin, Origin::ExceptionTable);
        }
    }
    // A cartridge ROM and a PS-X EXE map their code and data as one section, with no relocations
    // to tell their pointers apart: the symbols of the ELF they were made from are what name the
    // functions only a table or a registered handler reaches.
    if image.n64.is_some() || image.psx.is_some() {
        for symbol in &image.symbols {
            state.seed(symbol.va, Origin::Symbol);
        }
    }
    state.run();

    // The virtual functions: the vtables of the RTTI name them before a bare pointer scan does.
    for class in &classes {
        for vtable in &class.vtables {
            for &va in &vtable.functions {
                if !state.cover.owners(va).iter().any(|&f| f != va) {
                    state.seed(va, Origin::Vtable(vtable.address));
                }
            }
        }
    }
    state.run();

    // Code addresses in the instructions themselves: `push offset callback`, `mov eax, offset fn`.
    // With relocations they are marked; without, a 32-bit immediate that lands at the start of
    // code outside every function is one. They come before the pointers in data: a callback the
    // code names is built whole, with the switch tables of its own, before the entries of those
    // tables (relocated code addresses in `.rdata` too) could pass for functions.
    for _ in 0..4 {
        let seeds = immediate_seeds(image, &state.functions, &state.cover);
        if seeds.is_empty() {
            break;
        }
        for (va, origin) in seeds {
            state.seed(va, origin);
        }
        state.run();
    }

    // Pointers in data: a function only a pointer reaches, such as a callback in a table.
    for (va, origin) in pointer_seeds(image) {
        if state.cover.covered(va) || state.functions.contains_key(&va) {
            continue;
        }
        state.seed(va, origin);
    }
    state.run();

    // An IOP module hands the IOP's kernel its thread and its RPC dispatcher by an address its
    // code builds in a `lui` pair, which its relocations name.
    for (va, origin) in irx_address_seeds(image) {
        if state.cover.covered(va) || state.functions.contains_key(&va) {
            continue;
        }
        state.seed(va, origin);
    }
    state.run();

    // The immediates of the functions the pointers named, and on a PlayStation the addresses
    // its code builds in two halves.
    for _ in 0..4 {
        let mut seeds = immediate_seeds(image, &state.functions, &state.cover);
        if image.platform() == Some("psx") {
            seeds.extend(mips_pair_seeds(image, &state.functions, &state.cover));
        }
        if seeds.is_empty() {
            break;
        }
        for (va, origin) in seeds {
            state.seed(va, origin);
        }
        state.run();
    }

    // The pointers in the data an image keeps in its code section (a DLL whose `.rdata` is merged
    // into `.text`): the vtables and the initializer tables there hold functions nothing else
    // names. Last among the seeds from the image, when the code the others reached covers the
    // most, since an operand inside code not yet built would pass for such a pointer.
    if image.data_in_code() {
        for (va, origin) in merged_pointer_seeds(image, &state.cover) {
            if state.cover.covered(va) || state.functions.contains_key(&va) {
                continue;
            }
            state.seed(va, origin);
        }
        state.run();
        for _ in 0..4 {
            let seeds = immediate_seeds(image, &state.functions, &state.cover);
            if seeds.is_empty() {
                break;
            }
            for (va, origin) in seeds {
                state.seed(va, origin);
            }
            state.run();
        }
    }

    // The targets a traced run reached: a function only a computed address names.
    for call in &facts.calls {
        for &va in &call.targets {
            if !state.cover.covered(va) {
                state.seed(va, Origin::Trace(call.site));
            }
        }
    }
    // The entries a run of the port left to the residual engine: functions only an address the
    // program computed reaches, which the next port has lifted.
    for &va in &facts.residual {
        if !state.cover.covered(va) {
            state.seed(va, Origin::Residual);
        }
    }
    state.run();

    // Code after the `int 3` fill that nothing names: a function the linker kept without a caller
    // (`/Od` keeps every function of an object it pulls in). An ARM64 image has no such fill: its
    // exception table names every function with a frame, a Thumb image keeps its literal pools in
    // the gaps, and a MIPS image pads with zero words, which are `nop`s.
    for _ in 0..MAX_ROUNDS {
        let seeds = if matches!(image.machine(), Machine::Arm64 | Machine::Arm | Machine::Mips | Machine::Other(EM_MIPS) | Machine::Sh4) {
            Vec::new()
        } else {
            gap_seeds(image, &state)
        };
        if seeds.is_empty() {
            break;
        }
        for (va, origin) in seeds {
            state.seed(va, origin);
        }
        state.run();
    }

    let mut functions = state.functions;
    // The data words that hold an import slot's value show only across the whole program: with
    // them known, a call through such a word, or through a register loaded from one, resolves.
    let words = imports::import_words(image, &mut functions);
    if !words.is_empty() {
        for f in functions.values_mut() {
            f.register_import_calls = imports::resolve(image, &words, f).calls;
        }
    }
    for f in functions.values_mut() {
        f.noreturn = state.noreturn.contains(&f.entry);
        f.strings = strings_of(image, f);
    }
    // An indirect call the run resolved calls the functions it reached: they join the caller's
    // calls, and the lift reads the resolution by the call's address.
    let entries: HashSet<u64> = functions.keys().copied().collect();
    for call in &facts.calls {
        let targets: Vec<u64> = call.targets.iter().copied().filter(|t| entries.contains(t)).collect();
        if targets.is_empty() {
            continue;
        }
        if let Some(f) = state.cover.owners(call.site).first().and_then(|entry| functions.get_mut(entry)) {
            for &t in &targets {
                if !f.calls.contains(&t) {
                    f.calls.push(t);
                }
            }
            f.traced_calls.insert(call.site, targets);
        }
    }
    // With every function found and every call known, the ones only the gaps showed are judged:
    // the fill goes, the dead stay marked, and what anything names is the program's.
    // A RISC machine builds an address from two instructions (`lui` and `addiu`, `adrp` and `add`,
    // `movw` and `movt`) or loads it from a literal pool, which no one operand shows: there every
    // function may be address-taken, and the gaps are not searched. So is it for 32-bit x86 code
    // of an ELF or Mach-O file, which addresses its functions from the global offset table.
    let risc = matches!(image.machine(), Machine::Arm64 | Machine::Arm | Machine::Mips | Machine::Other(EM_MIPS) | Machine::Sh4);
    let pic = !image.is_64() && (image.elf.is_some() || image.macho.is_some());
    let mut address_taken = (!risc && !pic).then(|| gaps::address_taken(image, &functions, &classes, facts));
    let fill = match &address_taken {
        Some(taken) => gaps::classify(image, &mut functions, taken),
        None => gaps::Fill::default(),
    };
    // The names, strongest first: the exports are set, the symbols of the build (the PDB, else the
    // link map) name what they list, a vtable slot names what is left, and the last pass makes
    // them identifiers unique in the program.
    // The runtime's functions were marked as they were built.
    symbols::name_functions(image, &mut functions);
    name_virtual_functions(&classes, &mut functions);
    let program_entry = symbols::program_entry(image, &functions);
    // The port's startup calls the program entry by its address.
    if let Some(taken) = &mut address_taken {
        taken.extend(program_entry);
    }
    symbols::finish_names(&mut functions);
    // The x64 unwind data names a function's scope table; on 32-bit x86 its prologue does.
    let (seh_frames, seh_unread) =
        if matches!(image.machine(), Machine::I386) { seh32::frames(image, &functions) } else { (BTreeMap::new(), BTreeMap::new()) };
    let (cxx_frames32, cxx_unread32) = if matches!(image.machine(), Machine::I386) {
        eh32::frames(image, arch.as_ref(), &functions)
    } else {
        (BTreeMap::new(), BTreeMap::new())
    };
    let (noreturn, warnings) = (state.noreturn, state.warnings);
    Analysis {
        image,
        arch,
        functions,
        noreturn,
        classes,
        cxx_frames,
        seh_frames,
        seh_unread,
        cxx_frames32,
        cxx_unread32,
        program_entry,
        address_taken,
        fill: fill.0,
        facts: facts.clone(),
        toolset,
        warnings,
    }
}

/// The string literals a function addresses: each operand that names an address where printable
/// text of two or more characters ends in a zero, once per address, in first-use order.
fn strings_of(image: &Image, function: &Function) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    for instr in function.blocks.iter().flat_map(|b| &b.instrs) {
        for operand in &instr.operands {
            let target = match operand {
                Operand::Mem(m) if m.absolute.is_some() => m.absolute.unwrap(),
                // A 32-bit displacement with a base is an absolute address only when it maps.
                Operand::Mem(m) if !image.is_64() && m.disp > 0 && image.is_mapped(m.disp as u64) => m.disp as u64,
                Operand::Imm { value, .. } if !image.is_64() => *value as u64 & 0xffff_ffff,
                Operand::Imm { value, .. } => *value as u64,
                _ => continue,
            };
            // A word that holds an address of the image is a pointer whose bytes read as text
            // (`8A@` for 0x00404138): the start of a table of records, not a literal.
            if image.is_code(target) || out.iter().any(|(va, _)| *va == target) || image.pointer(target).is_ok_and(|p| image.is_mapped(p)) {
                continue;
            }
            if let Some(text) = image.printable_cstr(target, 2, 512) {
                out.push((target, text.to_owned()));
            }
        }
    }
    out
}

/// The code addresses the instructions of the built functions hold, outside every function: the
/// callbacks a program hands out by `push offset f` (an immediate of the address width) or by
/// `lea r8, [rip + f]` (x64, where a leaf callback has no unwind entry to find it by).
/// On a 32-bit image with relocations, an immediate is an address only where a relocation marks
/// it: `cmp eax, 0x416d62` with a four-character code that happens to fall in the code is none,
/// and a function seeded there cut the real one it fell into, whose code past the
/// cut became a function of its own that ran with the registers of neither.
fn immediate_seeds(image: &Image, functions: &BTreeMap<u64, Function>, cover: &Cover) -> Vec<(u64, Origin)> {
    let bits = u16::from(image.pointer_size()) * 8;
    let relocated: Option<std::collections::BTreeSet<u64>> =
        (bits == 32 && !image.relocations.is_empty()).then(|| image.relocations.iter().map(|r| r.va).collect());
    let marked = |i: &ra_asm_arch::Instr, va: u64| {
        relocated.as_ref().is_none_or(|set| set.range(i.address..i.end()).any(|&at| image.u32(at).is_ok_and(|v| u64::from(v) == va)))
    };
    let mut seeds = Vec::new();
    for f in functions.values() {
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            for operand in &i.operands {
                let va = match operand {
                    Operand::Imm { value, bits: width } if *width == bits => {
                        if bits == 32 {
                            *value as u64 & 0xffff_ffff
                        } else {
                            *value as u64
                        }
                    }
                    Operand::Mem(m) if i.mnemonic == "lea" => match m.absolute {
                        Some(va) => va,
                        None => continue,
                    },
                    _ => continue,
                };
                if !image.is_code(va) || functions.contains_key(&va) || cover.covered(va) || !marked(i, va) {
                    continue;
                }
                seeds.push((va, Origin::Pointer(i.address)));
            }
        }
    }
    seeds.sort_by_key(|(va, _)| *va);
    seeds.dedup_by_key(|(va, _)| *va);
    seeds
}

/// The functions a `PlayStation` program names only by an address it builds: a `lui` and the
/// `addiu` or `ori` that completes it in the same block, giving the address of code no function
/// holds yet that starts as a function with a frame does, `addiu $sp, $sp, -n`. A handler or a
/// callback is handed to the kernel that way, and the image has no relocation to say the
/// constant is an address; the frame's instruction is what tells it from a number.
fn mips_pair_seeds(image: &Image, functions: &BTreeMap<u64, Function>, cover: &Cover) -> Vec<(u64, Origin)> {
    // Code that makes its frame within its first four instructions, before any jump: the end of
    // one function with the start of the next after it is no start.
    let framed = |va: u64| {
        // A number that reads as the code's address in another segment is a number.
        if va % 4 != 0 || !image.is_code(va) || !image.names_own(va) {
            return false;
        }
        for n in 0..4 {
            let Ok(word) = image.u32(va + 4 * n) else { return false };
            if word >> 16 == 0x27bd && word & 0x8000 != 0 {
                return true;
            }
            let jump = matches!(word >> 26, 2 | 3) || (word >> 26 == 0 && matches!(word & 0x3f, 8 | 9));
            if jump {
                return false;
            }
        }
        false
    };
    // A function may make its frame late or have none: an address is a start all the same when
    // the code before it ends in a return or in the jump of a stub to one of the BIOS's tables
    // (`j 0xa0`, `j 0xb0`, `j 0xc0`), with its delay slot or a pad after. Any other jump is as
    // often the end of a switch's case, in the middle of its function.
    let starts = |va: u64| {
        let word = |at: u64| image.u32(at).unwrap_or(u32::MAX);
        let leaves = |at: u64| matches!(word(at), 0x03e0_0008 | 0x0800_0028 | 0x0800_002c | 0x0800_0030);
        let after_return = va % 4 == 0
            && image.is_code(va)
            && image.names_own(va)
            && (leaves(va.wrapping_sub(8)) || (word(va.wrapping_sub(4)) == 0 && leaves(va.wrapping_sub(12))));
        framed(va) || after_return
    };
    let mut seeds = Vec::new();
    for block in functions.values().flat_map(|f| &f.blocks) {
        let mut high: HashMap<u32, u32> = HashMap::new();
        for i in &block.instrs {
            let Ok(word) = image.u32(i.address) else { continue };
            let (op, rs, rt, low) = (word >> 26, (word >> 21) & 31, (word >> 16) & 31, word & 0xffff);
            match op {
                0x0f => {
                    high.insert(rt, low << 16);
                }
                0x09 | 0x0d if high.contains_key(&rs) => {
                    let low = if op == 0x09 { i32::from(low as u16 as i16).cast_unsigned() } else { low };
                    let built = u64::from(high[&rs].wrapping_add(low));
                    // The function is at the address the image holds it at.
                    let va = image.memory_region(built).map_or(built, |(_, folded)| folded);
                    if starts(built) && !functions.contains_key(&va) && !cover.covered(va) {
                        seeds.push((va, Origin::Pointer(i.address)));
                    }
                    high.remove(&rt);
                }
                _ => {}
            }
        }
    }
    // Where a call of the BIOS's `setjmp` returns is where the kernel's `longjmp` enters later, on
    // an interrupt: an entry of its own. The call goes to the stub that names the function, by
    // its table and its number.
    const SETJMP_STUB: [u32; 3] = [0x240a_00a0, 0x0140_0008, 0x2409_0013];
    let is_stub = |va: u64| (0u64..3).all(|n| image.u32(va + 4 * n).is_ok_and(|word| word == SETJMP_STUB[n as usize]));
    for i in functions.values().flat_map(|f| &f.blocks).flat_map(|b| &b.instrs) {
        let Ok(word) = image.u32(i.address) else { continue };
        let target = ((i.address + 4) & 0xf000_0000) | (u64::from(word & 0x03ff_ffff) << 2);
        let after = i.address + 8;
        if word >> 26 == 3 && is_stub(target) && !functions.contains_key(&after) {
            seeds.push((after, Origin::Pointer(i.address)));
        }
    }
    // The image's one section holds its data too: a word of it, outside the code found so far,
    // that holds such an address is a slot of a table of functions.
    for section in &image.sections {
        let start = image.va(section.virtual_address);
        let Ok(bytes) = image.read_to_section_end(start) else { continue };
        for (offset, chunk) in bytes.chunks_exact(4).enumerate() {
            let (slot, built) = (start + 4 * offset as u64, image.pointer_in(chunk));
            let va = image.memory_region(built).map_or(built, |(_, folded)| folded);
            if !cover.covered(slot) && starts(built) && !functions.contains_key(&va) && !cover.covered(va) {
                seeds.push((va, Origin::Pointer(slot)));
            }
        }
    }
    // A kernel keeps the code of its vectors as bytes it copies to where the processor enters
    // them: `lui r; addiu r, r, lo; jr r`, a jump to the handler, which nothing else names. The
    // three words together are a jump wherever they lie, in code the walk reached or not.
    if image.psx_kernel() {
        for section in &image.sections {
            let start = image.va(section.virtual_address);
            let Ok(bytes) = image.read_to_section_end(start) else { continue };
            let words: Vec<u32> = bytes.chunks_exact(4).map(|w| image.pointer_in(w) as u32).collect();
            for (offset, triple) in words.windows(3).enumerate() {
                let (high, low, jump) = (triple[0], triple[1], triple[2]);
                let register = (high >> 16) & 31;
                let completes = matches!(low >> 26, 0x09 | 0x0d) && (low >> 21) & 31 == register && (low >> 16) & 31 == register;
                if high >> 21 != 0x1e0 || !completes || jump != (register << 21) | 8 {
                    continue;
                }
                let half = if low >> 26 == 0x09 { i32::from(low as u16 as i16).cast_unsigned() } else { low & 0xffff };
                let built = u64::from(((high & 0xffff) << 16).wrapping_add(half));
                let va = image.memory_region(built).map_or(built, |(_, folded)| folded);
                if va % 4 == 0 && image.is_code(va) && image.names_own(built) && !functions.contains_key(&va) && !cover.covered(va) {
                    seeds.push((va, Origin::TailCall(start + 4 * offset as u64)));
                }
            }
        }
    }
    seeds.sort_by_key(|(va, _)| *va);
    seeds.dedup_by_key(|(va, _)| *va);
    seeds
}

/// Whether a `nop` fill ends at `offset` of `bytes`: a `90`, or the long `0f 1f /0` forms, which
/// clang writes between functions where MSVC writes `int 3`.
fn nop_fill_ends_at(bytes: &[u8], offset: usize) -> bool {
    const TAILS: [&[u8]; 5] = [&[0x00], &[0x40, 0x00], &[0x44, 0x00, 0x00], &[0x80, 0, 0, 0, 0], &[0x84, 0, 0, 0, 0, 0]];
    if offset >= 1 && bytes[offset - 1] == 0x90 {
        return true;
    }
    TAILS.iter().any(|tail| {
        let n = tail.len() + 2;
        offset >= n && bytes[offset - n..offset - tail.len()] == [0x0f, 0x1f] && bytes[offset - tail.len()..offset] == **tail
    })
}

/// Whether a fill starts at `va`: `int 3`, a `90`, or a long `nop` under its `66` and `2e` prefixes.
fn fill_starts_at(image: &Image, va: u64) -> bool {
    let byte = |i: u64| image.u8(va + i).unwrap_or(0);
    if matches!(byte(0), 0xcc | 0x90) {
        return true;
    }
    let prefixes = (0..4).take_while(|&i| matches!(byte(i), 0x66 | 0x2e)).count() as u64;
    byte(prefixes) == 0x0f && byte(prefixes + 1) == 0x1f
}

/// The functions in the gaps between the found ones: a 16-byte aligned address after `int 3` or
/// `nop` fill in a code section, or where a found function ends with no fill after it, outside
/// every function, whose trial build has no warning, covers the bytes from the address to the next
/// fill, function, 16-byte boundary or section end without a hole, and ends in returns and tail
/// calls only, never in a trap or an unresolved jump. A jump table or a string in the code section
/// fails the trial; a function does not. MSVC packs functions it aligns to 16 bytes end to end
/// when the last one fills its slot, so a run of them has no fill between them: the end of one is
/// where the next is tried.
fn gap_seeds(image: &Image, state: &State<'_>) -> Vec<(u64, Origin)> {
    let mut seeds = Vec::new();
    let mut accepted: Vec<(u64, u64)> = Vec::new();
    let mut ends: HashSet<u64> = state.functions.values().map(Function::end).collect();
    for section in image.sections.iter().filter(|s| s.is_code()) {
        let start = image.va(section.virtual_address);
        let Ok(bytes) = image.read_to_section_end(start) else { continue };
        let end = start + bytes.len() as u64;
        for (offset, pair) in bytes.windows(2).enumerate() {
            let va = start + offset as u64 + 1;
            if !va.is_multiple_of(16) || pair[1] == 0xcc || (pair[0] != 0xcc && !nop_fill_ends_at(bytes, offset + 1) && !ends.contains(&va))
            {
                continue;
            }
            if state.known_starts.contains(&va) || accepted.iter().any(|&(b, e)| va >= b && va < e) || state.cover.covered(va) {
                continue;
            }
            let ranges = state.ranges_of(va);
            let built = builder::build(image, state.arch, va, &ranges, &state.known_starts, &state.noreturn, state.frames.get(&va));
            let blocks = &built.function.blocks;
            let contiguous = blocks.first().is_some_and(|b| b.start == va) && blocks.windows(2).all(|w| w[0].end == w[1].start);
            let last = blocks.last().map_or(va, |b| b.end);
            let bounded = last == end || state.known_starts.contains(&last) || fill_starts_at(image, last) || last.is_multiple_of(16);
            // An import thunk nothing calls is the linker's leftover, not a function of the program.
            if !(built.warnings.is_empty() && contiguous && bounded && reads_as_code(blocks)) || built.function.is_thunk() {
                continue;
            }
            accepted.push((va, last));
            ends.insert(last);
            let after = state.functions.range(..va).next_back().map_or(va, |(entry, _)| *entry);
            seeds.push((va, Origin::Gap(after)));
        }
    }
    seeds
}

/// Fills each function's vtable slots from `classes`, and names a function nothing else names after
/// its first slot: the class with the fewest bases first, so an inherited function takes the name
/// of the class that introduced it.
fn name_virtual_functions(classes: &[Class], functions: &mut BTreeMap<u64, Function>) {
    let mut order: Vec<&Class> = classes.iter().collect();
    order.sort_by_key(|c| (c.bases.len(), c.type_descriptor));
    let mut taken: HashSet<String> = functions.values().map(|f| f.name.clone()).collect();
    for class in order {
        for vtable in &class.vtables {
            for (slot, &va) in vtable.functions.iter().enumerate() {
                let Some(f) = functions.get_mut(&va) else { continue };
                f.virtuals.push(VirtualSlot { class: class.name.clone(), offset: vtable.offset, slot });
                if f.name != function::default_name(va) {
                    continue;
                }
                let mut name = class.slot_name(vtable, slot);
                if taken.contains(&name) {
                    name = format!("{name}_{va:x}");
                }
                taken.insert(name.clone());
                f.name = name;
            }
        }
    }
}

/// Which functions own an address: the blocks of every built function by their start, kept in
/// step with the functions as they are built, split and rebuilt. Asking costs the blocks that start
/// within one block's length below the address, where asking every function cost the whole
/// program: an image of 87,000 functions spent most of its analysis there.
#[derive(Default)]
struct Cover {
    /// (start, owner) to end, for every block.
    blocks: BTreeMap<(u64, u64), u64>,
    /// The length of the longest block, which bounds how far below an address a block that holds
    /// it can start.
    longest: u64,
}

impl Cover {
    fn add(&mut self, f: &Function) {
        for b in &f.blocks {
            self.blocks.insert((b.start, f.entry), b.end);
            self.longest = self.longest.max(b.end.saturating_sub(b.start));
        }
    }

    fn remove(&mut self, f: &Function) {
        for b in &f.blocks {
            self.blocks.remove(&(b.start, f.entry));
        }
    }

    /// The entries of the functions with a block that holds `va`, each once, lowest first.
    fn owners(&self, va: u64) -> Vec<u64> {
        let mut owners: Vec<u64> = self
            .blocks
            .range((va.saturating_sub(self.longest), 0)..=(va, u64::MAX))
            .filter(|&(&(start, _), &end)| start <= va && va < end)
            .map(|(&(_, owner), _)| owner)
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    /// Whether a function holds `va`: [`Function::contains`] of any of them.
    fn covered(&self, va: u64) -> bool {
        !self.owners(va).is_empty()
    }
}

/// The working state of the fixpoint.
struct State<'a> {
    image: &'a Image,
    arch: &'a dyn Arch,
    /// Changed only through [`State::insert`] and [`State::take`], which keep `cover` in step.
    functions: BTreeMap<u64, Function>,
    cover: Cover,
    /// Every address that starts a function, built or pending: a descent stops at these.
    known_starts: BTreeSet<u64>,
    pending: Vec<u64>,
    origins: HashMap<u64, Vec<Origin>>,
    noreturn: HashSet<u64>,
    /// The entries of the exception table, with their pieces: the extent of an x64 function.
    extents: HashMap<u64, Vec<(u64, u64)>>,
    /// The x64 C++ frames, by entry, and the funclets they name, which are no functions.
    frames: &'a BTreeMap<u64, CxxFrame>,
    funclets: HashSet<u64>,
    /// The ranges of the C runtime libraries linked into the image, from the PDB.
    runtime: symbols::RuntimeRanges,
    /// Whether the image keeps its read-only data in its code section ([`Image::data_in_code`]),
    /// where a pointer into the section names data as readily as a function.
    data_in_code: bool,
    /// The starts a word of the data named that turned out to be no function's: inside an
    /// instruction of a function built since ([`State::unseed`]). Not seeded again.
    rejected: HashSet<u64>,
    warnings: Vec<String>,
}

impl<'a> State<'a> {
    fn new(image: &'a Image, arch: &'a dyn Arch, frames: &'a BTreeMap<u64, CxxFrame>) -> Self {
        let extents = exception_extents(image, frames);
        let mut known_starts = BTreeSet::new();
        known_starts.extend(extents.keys().copied());
        Self {
            image,
            arch,
            functions: BTreeMap::new(),
            cover: Cover::default(),
            known_starts,
            pending: Vec::new(),
            origins: HashMap::new(),
            noreturn: HashSet::new(),
            extents,
            frames,
            funclets: frames.values().flat_map(|f| f.funclets.iter().map(|r| r.begin)).collect(),
            runtime: symbols::RuntimeRanges::of(image),
            data_in_code: image.data_in_code(),
            rejected: HashSet::new(),
            warnings: Vec::new(),
        }
    }

    fn insert(&mut self, function: Function) {
        if let Some(old) = self.functions.remove(&function.entry) {
            self.cover.remove(&old);
        }
        self.cover.add(&function);
        self.functions.insert(function.entry, function);
    }

    fn take(&mut self, entry: u64) -> Option<Function> {
        let function = self.functions.remove(&entry)?;
        self.cover.remove(&function);
        Some(function)
    }

    /// Takes back the start `bogus`, which only words of the data named and which lies inside an
    /// instruction of the function at `entry`, being built: a word past the end of a vtable, or
    /// any dword of the data, that happens to hold an address of the code. The start cut the
    /// function it lies in, what ran on past the cut became functions of their own reached by
    /// jumps, and each ran on the first one's frame with nothing of it. With the start go the
    /// starts only it and those led to, by their calls and jumps, and the starts `entry` jumped
    /// to when it was built cut short; a function another still calls or jumps to stays, and a
    /// function cut at one of the starts builds again.
    fn unseed(&mut self, bogus: u64, entry: u64) {
        let mut going: HashSet<u64> = HashSet::from([entry]);
        let mut work = vec![bogus];
        // The jumps of the function being built, as its shorter builds recorded them.
        let mut emptied = self.forget_origins(entry, false);
        loop {
            for t in emptied.drain(..) {
                let held = self.extents.contains_key(&t)
                    || self.functions.values().any(|f| !going.contains(&f.entry) && (f.calls.contains(&t) || f.tail_calls.contains(&t)));
                if !held {
                    work.push(t);
                }
            }
            let Some(s) = work.pop() else { break };
            if !going.insert(s) {
                continue;
            }
            for cut in self.cover.owners(s.saturating_sub(1)) {
                if !going.contains(&cut) && self.take(cut).is_some() {
                    self.pending.push(cut);
                }
            }
            self.known_starts.remove(&s);
            self.take(s);
            self.pending.retain(|p| *p != s);
            self.origins.remove(&s);
            emptied = self.forget_origins(s, true);
        }
    }

    /// Drops the origins the function at `from` gave other starts, its jumps and (with `calls`)
    /// its calls: the starts left with none.
    fn forget_origins(&mut self, from: u64, calls: bool) -> Vec<u64> {
        let mut emptied = Vec::new();
        for (t, list) in &mut self.origins {
            let before = list.len();
            list.retain(|o| !matches!(o, Origin::TailCall(c) if *c == from) && !(calls && matches!(o, Origin::Call(c) if *c == from)));
            if list.len() != before && list.is_empty() {
                emptied.push(*t);
            }
        }
        emptied
    }

    fn seed(&mut self, va: u64, origin: Origin) {
        if !self.image.is_code(va) || self.rejected.contains(&va) {
            return;
        }
        // A pointer into the code section of an image that keeps its data there names a string, a
        // table or a vtable as readily as a function (a vtable's slots, read until they leave the
        // code, run on into the data after them; an export, a constant or a table as well as a
        // function); one that does not build as code is no seed. A function built over data would
        // claim the words there (which then stay bytes in the port instead of becoming addresses),
        // seed more of its kind through its calls and its operands, and cut the real functions its
        // starts fall into.
        if self.data_in_code
            && matches!(origin, Origin::Pointer(_) | Origin::Vtable(_) | Origin::Export(_))
            && !self.functions.contains_key(&va)
            && !self.builds_as_code(va)
        {
            return;
        }
        self.origins.entry(va).or_default().push(origin);
        if self.functions.contains_key(&va) || self.pending.contains(&va) {
            return;
        }
        self.known_starts.insert(va);
        self.pending.push(va);
    }

    /// Builds every pending function, then settles the no-return set, until nothing changes.
    fn run(&mut self) {
        for _ in 0..MAX_ROUNDS {
            while let Some(entry) = self.pending.pop() {
                self.build(entry);
            }
            let mut changed = false;
            let newly: Vec<u64> = noreturn::settle(&self.functions, &mut self.noreturn);
            for callee in newly {
                changed = true;
                // Every caller falls through after the call today; it must not.
                let callers: Vec<u64> = self.functions.values().filter(|f| f.calls.contains(&callee)).map(|f| f.entry).collect();
                for caller in callers {
                    self.take(caller);
                    self.pending.push(caller);
                }
            }
            if !changed && self.pending.is_empty() {
                return;
            }
        }
        self.warnings.push(format!("the analysis did not settle in {MAX_ROUNDS} rounds"));
    }

    fn build(&mut self, entry: u64) {
        if self.functions.contains_key(&entry) {
            return;
        }
        // A start inside a built function splits it: that function builds again with the new
        // boundary in `known_starts`.
        let split: Vec<u64> = self.cover.owners(entry).into_iter().filter(|&f| f != entry).collect();
        for other in split {
            self.take(other);
            self.pending.push(other);
        }

        let ranges = self.ranges_of(entry);
        let built = builder::build(self.image, self.arch, entry, &ranges, &self.known_starts, &self.noreturn, self.frames.get(&entry));
        // A start inside the last instruction of a block, that only the data names, is no
        // function: the function builds again without it.
        let inside: Vec<u64> = built
            .function
            .blocks
            .iter()
            .filter_map(|b| b.instrs.last())
            .flat_map(|i| self.known_starts.range(i.address + 1..i.end()).copied())
            .filter(|s| {
                self.origins.get(s).is_some_and(|o| !o.is_empty() && o.iter().all(|o| matches!(o, Origin::Pointer(_) | Origin::Vtable(_))))
            })
            .collect();
        if !inside.is_empty() {
            for s in inside {
                self.rejected.insert(s);
                self.unseed(s, entry);
            }
            self.pending.push(entry);
            return;
        }
        let mut function = built.function;
        function.register_import_calls = imports::resolve(self.image, &HashMap::new(), &mut function).calls;
        function.origins = self.origins.get(&entry).cloned().unwrap_or_default();
        function.name = function::name_for(self.image, &function);
        // A function of a linked-in C runtime is the host's: the no-return settle judges it by
        // name, not by the body it has here.
        symbols::classify(self.image, &self.runtime, &mut function);
        // A piece of the runtime's code the symbols do not name, which a helper the port keeps
        // calls or jumps to, is that helper's own (the range and error paths of `_dtol3`): the
        // host has nothing under `F_<address>`, and the port would not link.
        let unnamed = |f: &Function| matches!(&f.kind, Kind::Runtime(n) if *n == function::default_name(f.entry));
        let kept = |f: &Function, runtime: &symbols::RuntimeRanges| matches!(f.kind, Kind::Normal) && runtime.contains(f.entry);
        if unnamed(&function)
            && function.origins.iter().any(
                |o| matches!(o, Origin::Call(c) | Origin::TailCall(c) if self.functions.get(c).is_some_and(|c| kept(c, &self.runtime))),
            )
        {
            function.kind = Kind::Normal;
        }
        self.warnings.extend(built.warnings);
        for &target in function.calls.iter().chain(&function.tail_calls) {
            let origin = if function.tail_calls.contains(&target) { Origin::TailCall(entry) } else { Origin::Call(entry) };
            if self.image.is_code(target) && !self.functions.contains_key(&target) && !self.pending.contains(&target) {
                self.known_starts.insert(target);
                self.pending.push(target);
                self.origins.entry(target).or_default().push(origin);
            } else if self.image.is_code(target) && target != entry && kept(&function, &self.runtime) {
                // A piece queued or built before this caller was known: it builds again as the
                // helper's.
                let origins = self.origins.entry(target).or_default();
                if !origins.contains(&origin) {
                    origins.push(origin);
                }
                if self.functions.get(&target).is_some_and(unnamed) {
                    self.take(target);
                    self.pending.push(target);
                }
            }
        }
        debug_assert_eq!(function.entry, entry);
        self.insert(function);
    }

    /// Whether the bytes at `va` build as a function the way [`gap_seeds`] accepts one: the trial
    /// build starts there, warns of nothing, no block of it ends in a trap, a halt, bytes that are
    /// no instruction or an unresolved jump, and no instruction of it is one a compiler never
    /// writes. Data read as code fails one of these within a few instructions, except a run of
    /// zero bytes (`add [eax], al`, over and over), which reads as code until the function after
    /// it, and a build from there would claim that function: the bounds of an empty table lie in
    /// such a run, so a start on an all-zero instruction is no function's.
    fn builds_as_code(&self, va: u64) -> bool {
        let ranges = self.ranges_of(va);
        let built = builder::build(self.image, self.arch, va, &ranges, &self.known_starts, &self.noreturn, self.frames.get(&va));
        let blocks = &built.function.blocks;
        let zero_start = blocks
            .first()
            .and_then(|b| b.instrs.first())
            .is_some_and(|i| self.image.read(i.address, usize::from(i.len)).is_ok_and(|bytes| bytes.iter().all(|&b| b == 0)));
        built.warnings.is_empty() && blocks.first().is_some_and(|b| b.start == va) && !zero_start && reads_as_code(blocks)
    }

    /// The byte ranges a function may own: its exception table extent with its pieces, else from
    /// its entry to the next known start or the end of its section.
    fn ranges_of(&self, entry: u64) -> Vec<(u64, u64)> {
        if let Some(extent) = self.extents.get(&entry) {
            return extent.clone();
        }
        let section_end = self.image.section_of(entry).map_or(entry, |s| self.image.va(s.end_rva()));
        let next = self.known_starts.range(entry + 1..).next().copied().unwrap_or(section_end);
        vec![(entry, next.min(section_end))]
    }
}

/// Whether a trial build reads as code: no block ends in a trap, a halt, bytes that are no
/// instruction or an unresolved jump, and no instruction is one a compiler never writes into a
/// user-mode program: the I/O, the BCD arithmetic, the segment and descriptor loads, the
/// privileged and the far forms, `bound`, `enter`, the loops and an `int` other than the fast
/// fail's. Bytes of text and tables decode into these within a few instructions (the letters
/// `b`, `c`, `l` to `o` are `bound`, `arpl` and the string I/O), where a function never does. A
/// trap right after a call is the fill behind a call to a function that never returns (a throw at
/// the end of a function, with `int 3` up to the next), which the trial does not yet know as one:
/// the block is code, as is one the fast fail ends.
fn reads_as_code(blocks: &[Block]) -> bool {
    const NEVER_COMPILED: &[&str] = &[
        "aaa", "aad", "aam", "aas", "daa", "das", "arpl", "bound", "enter", "in", "out", "insb", "insw", "insd", "outsb", "outsw", "outsd",
        "hlt", "cli", "sti", "clts", "lgdt", "lidt", "lldt", "lmsw", "ltr", "sgdt", "sidt", "sldt", "smsw", "str", "verr", "verw", "lar",
        "lsl", "invd", "wbinvd", "invlpg", "rdmsr", "wrmsr", "into", "iret", "iretd", "iretq", "xlatb", "salc", "les", "lds", "lss", "lfs",
        "lgs", "loop", "loope", "loopne", "pushad", "popad", "pushaw", "popaw", "ud0", "ud1",
    ];
    let never = |i: &ra_asm_arch::Instr| match i.mnemonic.as_str() {
        "int" => !matches!(i.operands.first(), Some(Operand::Imm { value, .. }) if matches!(*value, 0x29 | 0x2c | 0x2d)),
        m => NEVER_COMPILED.contains(&m),
    };
    let fill_after_call = |b: &Block| {
        matches!(b.terminator, Terminator::Trap)
            && matches!(b.instrs.last().map(|i| i.flow), Some(Flow::Trap))
            && matches!(b.instrs.iter().nth_back(1).map(|i| i.flow), Some(Flow::Call(_)))
    };
    // The fast fail ends its block as a trap does, and a compiler writes it: the range check of an
    // inlined `std::string` destructor ends in `mov ecx, 5; int 0x29`.
    let fast_fail = |b: &Block| {
        matches!(b.terminator, Terminator::Trap)
            && b.instrs.last().is_some_and(|i| i.mnemonic == "int" && matches!(i.operands.first(), Some(Operand::Imm { value: 0x29, .. })))
    };
    blocks.iter().all(|b| {
        (fill_after_call(b)
            || fast_fail(b)
            || !matches!(b.terminator, Terminator::Trap | Terminator::Halt | Terminator::Invalid | Terminator::IndirectJump))
            && !b.instrs.iter().any(never)
    })
}

/// Whether a decorated export name is a data symbol's: a vftable (`??_7`), a vbtable (`??_8`), a
/// string literal (`??_C`) or an RTTI structure (`??_R`), which a DLL exports beside its functions.
fn is_data_export(name: &str) -> bool {
    ["??_7", "??_8", "??_C", "??_R"].iter().any(|p| name.starts_with(p))
}

/// The extent of each x64 function from the exception table: its own entry, the chained pieces
/// that name it as their parent, directly or through another piece, and the funclets of its C++
/// frame.
fn exception_extents(image: &Image, frames: &BTreeMap<u64, CxxFrame>) -> HashMap<u64, Vec<(u64, u64)>> {
    let mut parents: HashMap<u64, u64> = HashMap::new();
    let mut extents: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();
    for frame in frames.values() {
        for funclet in &frame.funclets {
            parents.insert(funclet.begin, frame.entry.begin);
        }
    }
    for f in &image.runtime_functions {
        match f.unwind_info(image) {
            Ok(u) if u.chained.is_some() => {
                parents.insert(f.begin, u.chained.unwrap().begin);
            }
            _ if parents.contains_key(&f.begin) => {}
            _ => {
                extents.entry(f.begin).or_default().push((f.begin, f.end));
            }
        }
    }
    for f in &image.runtime_functions {
        let Some(&parent) = parents.get(&f.begin) else { continue };
        // Follow the chain to a root; a cycle or a missing root leaves the piece on its own.
        let mut root = parent;
        for _ in 0..16 {
            match parents.get(&root) {
                Some(&next) => root = next,
                None => break,
            }
        }
        extents.entry(root).or_default().push((f.begin, f.end));
    }
    extents
}

/// Pointer-sized data slots that hold a code address. On x86 the relocations name every absolute
/// address; on x64, or on x86 with the relocations stripped, an aligned slot in an initialized
/// data section is the test.
fn pointer_seeds(image: &Image) -> Vec<(u64, Origin)> {
    let mut seeds = Vec::new();
    if image.is_64() || image.relocations.is_empty() {
        let width = usize::from(image.pointer_size());
        for section in &image.sections {
            if section.is_code() || section.is_executable() || section.is_uninitialized() || !section.is_readable() {
                continue;
            }
            let start = image.va(section.virtual_address);
            let Ok(bytes) = image.read_to_section_end(start) else { continue };
            for (offset, chunk) in bytes.chunks_exact(width).enumerate() {
                let value = image.pointer_in(chunk);
                // An import's slot is the loader's: a lazily bound ELF slot holds the address of its
                // PLT stub's second half until the first call binds it.
                let slot = start + (offset * width) as u64;
                if image.is_code(value) && image.import_at(slot).is_none() {
                    seeds.push((value, Origin::Pointer(slot)));
                }
            }
        }
    } else {
        for r in &image.relocations {
            let Ok(value) = image.u32(r.va) else { continue };
            let value = u64::from(value);
            if image.is_code(value) && !image.is_code(r.va) {
                seeds.push((value, Origin::Pointer(r.va)));
            }
        }
    }
    seeds
}

/// The code addresses an IOP module's `lui` pairs build, each a high half its relocations pair
/// with the low half after it, read from the relocated words; the library tables in its text are
/// data and seed nothing.
fn irx_address_seeds(image: &Image) -> Vec<(u64, Origin)> {
    const R_MIPS_HI16: u32 = 5;
    let Some(irx) = image.elf.as_ref().and_then(|e| e.irx.as_ref()) else { return Vec::new() };
    let tables: Vec<(u64, u64)> = irx.exports.iter().chain(&irx.imports).map(|l| l.table).collect();
    irx.relocations
        .iter()
        .filter(|r| r.kind == R_MIPS_HI16)
        .filter_map(|r| {
            let (high, low) = (image.u32(r.place).ok()?, image.u32(r.low?).ok()?);
            let target = u64::from((high << 16).wrapping_add_signed(i32::from(low as u16 as i16)));
            let data = tables.iter().any(|&(start, end)| (start..end).contains(&target));
            (image.is_code(target) && !data).then_some((target, Origin::Pointer(r.place)))
        })
        .collect()
}

/// The code addresses the relocated words of a code section hold outside every built function,
/// on a pointer boundary as the compiler lays data out: the pointers of the read-only data an
/// image keeps in its code section ([`Image::data_in_code`]). The operands of the code are
/// relocated too, which the functions over them tell apart; one of unreached code passes, and
/// what it names is a seed when it builds as code ([`State::seed`]).
fn merged_pointer_seeds(image: &Image, cover: &Cover) -> Vec<(u64, Origin)> {
    let width = u64::from(image.pointer_size());
    let mut seeds = Vec::new();
    for r in &image.relocations {
        if !image.is_code(r.va) || r.va % width != 0 || cover.covered(r.va) {
            continue;
        }
        let Ok(value) = image.pointer(r.va) else { continue };
        if image.is_code(value) {
            seeds.push((value, Origin::Pointer(r.va)));
        }
    }
    seeds
}
