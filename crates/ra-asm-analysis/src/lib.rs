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
//! that hold it, and a function nothing names takes the name of its slot.

pub mod builder;
pub mod function;
pub mod jumptable;
pub mod noreturn;
pub mod rtti;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_arch::Operand;
use ra_asm_pe::Image;
use ra_asm_x86::X86;

pub use function::{Block, Function, Kind, Origin, Terminator, VirtualSlot};
pub use rtti::{Class, Vtable};

/// The analysis of one image.
pub struct Analysis<'a> {
    pub image: &'a Image,
    pub arch: X86,
    /// Every function, by entry address.
    pub functions: BTreeMap<u64, Function>,
    /// The functions that never return, by entry address.
    pub noreturn: HashSet<u64>,
    /// The classes with virtual functions, from the RTTI.
    pub classes: Vec<Class>,
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
}

/// The bound of every rebuild loop. A real image settles in a few rounds; the bound is for a
/// malformed one.
const MAX_ROUNDS: usize = 64;

/// Analyzes `image`: finds its functions and builds their control-flow graphs.
pub fn analyze(image: &Image) -> Analysis<'_> {
    let arch = X86::for_pointer_size(image.pointer_size());
    let mut state = State::new(image, arch);

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
    for f in &image.runtime_functions {
        let chained = f.unwind_info(image).is_ok_and(|u| u.is_chained());
        if !chained {
            state.seed(f.begin, Origin::ExceptionTable);
        }
    }
    state.run();

    // The virtual functions: the vtables of the RTTI name them before a bare pointer scan does.
    let classes = rtti::classes(image);
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

    // Pointers in data: a function only a pointer reaches, such as a callback in a table.
    for (va, origin) in pointer_seeds(image) {
        if state.functions.values().any(|f| f.contains(va)) || state.functions.contains_key(&va) {
            continue;
        }
        state.seed(va, origin);
    }
    state.run();

    let mut functions = state.functions;
    for f in functions.values_mut() {
        f.noreturn = state.noreturn.contains(&f.entry);
        f.strings = strings_of(image, f);
    }
    name_virtual_functions(&classes, &mut functions);
    Analysis { image, arch, functions, noreturn: state.noreturn, classes, warnings: state.warnings }
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
            if image.is_code(target) || out.iter().any(|(va, _)| *va == target) {
                continue;
            }
            if let Some(text) = image.printable_cstr(target, 2, 512) {
                out.push((target, text.to_owned()));
            }
        }
    }
    out
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
    arch: X86,
    functions: BTreeMap<u64, Function>,
    /// Every address that starts a function, built or pending: a descent stops at these.
    known_starts: BTreeSet<u64>,
    pending: Vec<u64>,
    origins: HashMap<u64, Vec<Origin>>,
    noreturn: HashSet<u64>,
    /// The entries of the exception table, with their pieces: the extent of an x64 function.
    extents: HashMap<u64, Vec<(u64, u64)>>,
    warnings: Vec<String>,
}

impl<'a> State<'a> {
    fn new(image: &'a Image, arch: X86) -> Self {
        let extents = exception_extents(image);
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
        let built = builder::build(self.image, self.arch, entry, &ranges, &self.known_starts, &self.noreturn);
        let mut function = built.function;
        function.origins = self.origins.get(&entry).cloned().unwrap_or_default();
        function.name = function::name_for(self.image, &function);
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

/// The extent of each x64 function from the exception table: its own entry and the chained pieces
/// that name it as their parent, directly or through another piece.
fn exception_extents(image: &Image) -> HashMap<u64, Vec<(u64, u64)>> {
    let mut parents: HashMap<u64, u64> = HashMap::new();
    let mut extents: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();
    for f in &image.runtime_functions {
        match f.unwind_info(image) {
            Ok(u) if u.chained.is_some() => {
                parents.insert(f.begin, u.chained.unwrap().begin);
            }
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
/// address; on x64 an aligned slot in an initialized data section is the test.
fn pointer_seeds(image: &Image) -> Vec<(u64, Origin)> {
    let mut seeds = Vec::new();
    if image.is_64() {
        for section in &image.sections {
            if section.is_code() || section.is_executable() || section.is_uninitialized() || !section.is_readable() {
                continue;
            }
            let start = image.va(section.virtual_address);
            let Ok(bytes) = image.read_to_section_end(start) else { continue };
            for (offset, chunk) in bytes.chunks_exact(8).enumerate() {
                let value = u64::from_le_bytes(chunk.try_into().unwrap());
                if image.is_code(value) {
                    seeds.push((value, Origin::Pointer(start + offset as u64 * 8)));
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
