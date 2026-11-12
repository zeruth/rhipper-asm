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
//! names) names what it lists, a function nothing names takes the name of its vtable slot, and
//! every name becomes an identifier unique in the program. The signatures of a library
//! ([`signatures`]) name the functions of a build without symbols the same way: applied to the
//! image first, they fill the symbols and the library ranges the analysis reads. The facts of a
//! traced run ([`facts`]) come last among the seeds and first among the resolutions: a target the
//! run reached is a function, and an indirect call the run resolved calls what it reached. A
//! function's [`identity`] is what it is apart from its address, how a new build of the program
//! finds the functions the person annotated in the last; [`matching`] matches every function of
//! two builds at once, by their symbols or by structure alone, for a binary diff.

pub mod builder;
pub mod facts;
pub mod function;
pub mod identity;
pub mod imports;
pub mod jumptable;
pub mod matching;
pub mod noreturn;
pub mod rtti;
pub mod signatures;
pub mod symbols;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_arch::{Arch, Operand};
use ra_asm_pe::{CxxAbi, CxxFrame, Image, Machine, Toolset};

pub use facts::Facts;
pub use function::{Block, Function, Kind, Origin, Terminator, VirtualSlot};
pub use identity::Identity;
pub use rtti::{Class, Vtable};
pub use signatures::{Signature, SignatureSet};

/// The decoder of an image's machine.
pub fn arch_of(image: &Image) -> Box<dyn Arch> {
    match image.machine() {
        Machine::Arm64 => Box::new(ra_asm_arm64::Arm64),
        Machine::Arm => Box::new(ra_asm_arm64::thumb::Thumb),
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
    /// The function a port starts at: the image's entry point, or `main` when the entry is the
    /// startup of a C runtime linked into the image, whose work the host's own startup does.
    pub program_entry: Option<u64>,
    /// The facts of the traced run the analysis took, empty without one.
    pub facts: Facts,
    /// The toolset that built the image: its C++ ABI decides whose RTTI the classes are read
    /// from, its runtime key which signatures and prototypes the C runtime takes.
    pub toolset: Toolset,
    /// What the analysis could not resolve: unresolved indirect jumps, invalid bytes, overlaps.
    pub warnings: Vec<String>,
}

impl Analysis<'_> {
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
    /// which the host's runtime supplies.
    pub fn ported(&self) -> impl Iterator<Item = &Function> {
        self.functions.values().filter(|f| !matches!(f.kind, Kind::Runtime(_)))
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

    // The seeds, in the order that names them best: the entry, the exports, the callbacks, .pdata.
    if let Some(entry) = image.entry_point() {
        state.seed(entry, Origin::Entry);
    }
    if let Some(exports) = &image.exports {
        for export in &exports.exports {
            if let Some(va) = export.va
                && image.is_code(va)
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
    state.run();

    // The virtual functions: the vtables of the RTTI name them before a bare pointer scan does.
    // The RTTI read is Microsoft's; a toolset of the Itanium ABI lays its classes out otherwise.
    let toolset = Toolset::of(image);
    let classes = if toolset.cxx_abi() == CxxAbi::Msvc { rtti::classes(image) } else { Vec::new() };
    for class in &classes {
        for vtable in &class.vtables {
            for &va in &vtable.functions {
                if !state.functions.values().any(|f| f.entry != va && f.contains(va)) {
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
        let seeds = immediate_seeds(image, &state.functions);
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
        if state.functions.values().any(|f| f.contains(va)) || state.functions.contains_key(&va) {
            continue;
        }
        state.seed(va, origin);
    }
    state.run();

    // The immediates of the functions the pointers named.
    for _ in 0..4 {
        let seeds = immediate_seeds(image, &state.functions);
        if seeds.is_empty() {
            break;
        }
        for (va, origin) in seeds {
            state.seed(va, origin);
        }
        state.run();
    }

    // The targets a traced run reached: a function only a computed address names.
    for call in &facts.calls {
        for &va in &call.targets {
            if !state.functions.values().any(|f| f.contains(va)) {
                state.seed(va, Origin::Trace(call.site));
            }
        }
    }
    state.run();

    // Code after the `int 3` fill that nothing names: a function the linker kept without a caller
    // (`/Od` keeps every function of an object it pulls in). An ARM64 image has no such fill: its
    // exception table names every function with a frame, and a Thumb image keeps its literal pools
    // in the gaps.
    for _ in 0..MAX_ROUNDS {
        let seeds = if matches!(image.machine(), Machine::Arm64 | Machine::Arm) { Vec::new() } else { gap_seeds(image, &state) };
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
        if let Some(f) = functions.values_mut().find(|f| f.contains(call.site)) {
            for &t in &targets {
                if !f.calls.contains(&t) {
                    f.calls.push(t);
                }
            }
            f.traced_calls.insert(call.site, targets);
        }
    }
    // The names, strongest first: the exports are set, the symbols of the build (the PDB, else the
    // link map) name what they list, a vtable slot names what is left, and the last pass makes
    // them identifiers unique in the program.
    // The runtime's functions were marked as they were built.
    symbols::name_functions(image, &mut functions);
    name_virtual_functions(&classes, &mut functions);
    let program_entry = symbols::program_entry(image, &functions);
    symbols::finish_names(&mut functions);
    let (noreturn, warnings) = (state.noreturn, state.warnings);
    Analysis { image, arch, functions, noreturn, classes, cxx_frames, program_entry, facts: facts.clone(), toolset, warnings }
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
fn immediate_seeds(image: &Image, functions: &BTreeMap<u64, Function>) -> Vec<(u64, Origin)> {
    let bits = u16::from(image.pointer_size()) * 8;
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
                if !image.is_code(va) || functions.contains_key(&va) || functions.values().any(|g| g.contains(va)) {
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
/// `nop` fill in a code section, outside every function, whose trial build has no warning, covers
/// the bytes from the address to the next fill, function or section end without a hole, and ends
/// in returns and tail calls only, never in a trap or an unresolved jump. A jump table or a string
/// in the code section fails the trial; a function does not.
fn gap_seeds(image: &Image, state: &State<'_>) -> Vec<(u64, Origin)> {
    let mut seeds = Vec::new();
    let mut accepted: Vec<(u64, u64)> = Vec::new();
    for section in image.sections.iter().filter(|s| s.is_code()) {
        let start = image.va(section.virtual_address);
        let Ok(bytes) = image.read_to_section_end(start) else { continue };
        let end = start + bytes.len() as u64;
        for (offset, pair) in bytes.windows(2).enumerate() {
            let va = start + offset as u64 + 1;
            if !va.is_multiple_of(16) || pair[1] == 0xcc || (pair[0] != 0xcc && !nop_fill_ends_at(bytes, offset + 1)) {
                continue;
            }
            if state.known_starts.contains(&va)
                || accepted.iter().any(|&(b, e)| va >= b && va < e)
                || state.functions.values().any(|f| f.contains(va))
            {
                continue;
            }
            let ranges = state.ranges_of(va);
            let built = builder::build(image, state.arch, va, &ranges, &state.known_starts, &state.noreturn, state.frames.get(&va));
            let blocks = &built.function.blocks;
            let contiguous = blocks.first().is_some_and(|b| b.start == va) && blocks.windows(2).all(|w| w[0].end == w[1].start);
            let last = blocks.last().map_or(va, |b| b.end);
            let bounded = last == end || state.known_starts.contains(&last) || fill_starts_at(image, last);
            let ends_well = blocks
                .iter()
                .all(|b| !matches!(b.terminator, Terminator::Trap | Terminator::Halt | Terminator::Invalid | Terminator::IndirectJump));
            // An import thunk nothing calls is the linker's leftover, not a function of the program.
            if !(built.warnings.is_empty() && contiguous && bounded && ends_well) || built.function.is_thunk() {
                continue;
            }
            accepted.push((va, last));
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

/// The working state of the fixpoint.
struct State<'a> {
    image: &'a Image,
    arch: &'a dyn Arch,
    functions: BTreeMap<u64, Function>,
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
            known_starts,
            pending: Vec::new(),
            origins: HashMap::new(),
            noreturn: HashSet::new(),
            extents,
            frames,
            funclets: frames.values().flat_map(|f| f.funclets.iter().map(|r| r.begin)).collect(),
            runtime: symbols::RuntimeRanges::of(image),
            warnings: Vec::new(),
        }
    }

    fn seed(&mut self, va: u64, origin: Origin) {
        if !self.image.is_code(va) {
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
                    self.functions.remove(&caller);
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
        let split: Vec<u64> = self.functions.values().filter(|f| f.entry != entry && f.contains(entry)).map(|f| f.entry).collect();
        for other in split {
            self.functions.remove(&other);
            self.pending.push(other);
        }

        let ranges = self.ranges_of(entry);
        let built = builder::build(self.image, self.arch, entry, &ranges, &self.known_starts, &self.noreturn, self.frames.get(&entry));
        let mut function = built.function;
        function.register_import_calls = imports::resolve(self.image, &HashMap::new(), &mut function).calls;
        function.origins = self.origins.get(&entry).cloned().unwrap_or_default();
        function.name = function::name_for(self.image, &function);
        // A function of a linked-in C runtime is the host's: the no-return settle judges it by
        // name, not by the body it has here.
        symbols::classify(self.image, &self.runtime, &mut function);
        self.warnings.extend(built.warnings);
        for &target in function.calls.iter().chain(&function.tail_calls) {
            if self.image.is_code(target) && !self.functions.contains_key(&target) && !self.pending.contains(&target) {
                self.known_starts.insert(target);
                self.pending.push(target);
                let origin = if function.tail_calls.contains(&target) { Origin::TailCall(entry) } else { Origin::Call(entry) };
                self.origins.entry(target).or_default().push(origin);
            }
        }
        self.functions.insert(entry, function);
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
                let value = if width == 8 {
                    u64::from_le_bytes(chunk.try_into().unwrap())
                } else {
                    u64::from(u32::from_le_bytes(chunk.try_into().unwrap()))
                };
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
