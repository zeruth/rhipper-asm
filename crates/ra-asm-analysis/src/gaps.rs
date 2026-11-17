//! The functions the gaps alone showed, judged once the whole program is found: what names each
//! one, and what the bytes of the ones nothing names prove.
//!
//! The gap seeds ([`crate::analyze`]) build a function wherever code follows the fill between the
//! found functions. Most are functions the linker kept without a caller; some are the fill itself
//! (a run of `nop`s that falls into the next function), and some are the padding a function
//! aligns its own code with, read from the middle. Three outcomes, on evidence only:
//!
//! - a candidate whose every instruction is fill (`int 3`, `nop` in its one- and multi-byte
//!   forms, and the instructions that change nothing a compiler pads with: `lea esi, [esi + 0]`,
//!   `mov edi, edi`, `xchg ax, ax`) is no function: it leaves the analysis ([`Fill`]);
//! - a candidate nothing names is [`Reach::Dead`] on positive evidence alone, and then stays in
//!   the table and is not lifted: it starts with fill and runs into a block of another function
//!   (it was read from the middle of that function's alignment run); or, in an image whose
//!   relocations list every absolute address, the code before it ends in a `ret` or another way
//!   out that never falls through with nothing but fill between, so no path reaches it; or every
//!   function that calls or jumps to it is dead;
//! - every other one is [`Reach::Gap`], and lifted as any function is.
//!
//! A computed call or jump to a dead entry finds no function there: the port traps with the
//! address (or hands it to the residual engine), as it does for any address it has no function
//! for.
//!
//! What any call, jump, switch, pointer, immediate, relocation, vtable, export, TLS callback, the
//! exception table or a traced run names ([`address_taken`], and the calls of every function) is
//! never judged: it is [`Reach::Named`].

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_arch::Operand;
use ra_asm_pe::Image;

use crate::Facts;
use crate::function::{Function, Origin, Reach, Terminator};
use crate::rtti::Class;

/// The entries the gaps showed whose bytes are fill alone, which left the analysis.
#[derive(Debug, Clone, Default)]
pub struct Fill(pub Vec<u64>);

/// The entries of `functions` whose address the program takes, which a computed call or jump can
/// reach: the image's entry, the exports, the TLS callbacks, the exception table, the vtables, a
/// relocated word or (without relocations, or in a 64-bit image) an aligned data word that holds
/// the entry, an immediate or an absolute `lea` of an instruction, a switch of another function
/// that jumps there, and what a traced run reached.
pub fn address_taken(image: &Image, functions: &BTreeMap<u64, Function>, classes: &[Class], facts: &Facts) -> BTreeSet<u64> {
    let mut taken = BTreeSet::new();
    let mut take = |va: u64| {
        if functions.contains_key(&va) {
            taken.insert(va);
        }
    };
    if let Some(entry) = image.entry_point() {
        take(entry);
    }
    if let Some(exports) = &image.exports {
        for va in exports.exports.iter().filter_map(|e| e.va) {
            take(va);
        }
    }
    if let Some(tls) = &image.tls {
        for &va in &tls.callbacks {
            take(va);
        }
    }
    for f in &image.runtime_functions {
        take(f.begin);
    }
    for &(begin, _) in image.unwind_extents() {
        take(begin);
    }
    for va in classes.iter().flat_map(|c| &c.vtables).flat_map(|v| &v.functions) {
        take(*va);
    }
    for call in &facts.calls {
        for &va in &call.targets {
            take(va);
        }
    }
    for &va in &facts.residual {
        take(va);
    }
    // Every word the relocations name, in the code as in the data: `push offset f` and a table of
    // callbacks alike.
    for r in &image.relocations {
        if let Ok(value) = image.pointer(r.va) {
            take(value);
        }
    }
    // Without relocations, or in a 64-bit image whose code names functions by `rip`, an aligned
    // word of the data that holds an entry is a pointer to it.
    if image.is_64() || image.relocations.is_empty() {
        let width = usize::from(image.pointer_size());
        for section in &image.sections {
            if section.is_uninitialized() || !section.is_readable() || (section.is_code() && !image.data_in_code()) {
                continue;
            }
            let start = image.va(section.virtual_address);
            let Ok(bytes) = image.read_to_section_end(start) else { continue };
            for chunk in bytes.chunks_exact(width) {
                take(image.pointer_in(chunk));
            }
        }
    }
    let mask = if image.is_64() { u64::MAX } else { 0xffff_ffff };
    for f in functions.values() {
        for b in &f.blocks {
            for i in &b.instrs {
                for operand in &i.operands {
                    match operand {
                        Operand::Imm { value, bits } if *bits >= 32 => take(*value as u64 & mask),
                        Operand::Mem(m) => {
                            if let Some(va) = m.absolute {
                                take(va);
                            }
                            if !image.is_64() && m.disp > 0 {
                                take(m.disp as u64 & mask);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Terminator::Switch { targets, .. } = &b.terminator {
                for &t in targets {
                    if t != f.entry && !f.contains(t) {
                        take(t);
                    }
                }
            }
        }
    }
    taken
}

/// Whether `i` changes nothing: the fill a linker or a compiler pads with. `int 3`, `nop` in every
/// form, `xchg` of a register with itself, `mov` of a 32-bit register to itself in a 32-bit image
/// (a 64-bit image's clears the upper half), and `lea` of a register to its own value.
pub fn is_fill(i: &ra_asm_arch::Instr, is_64: bool) -> bool {
    let same = |a: &Operand, b: &Operand| matches!((a, b), (Operand::Reg(x), Operand::Reg(y)) if x.id == y.id);
    match i.mnemonic.as_str() {
        "int3" | "nop" | "fnop" => true,
        "xchg" => matches!(i.operands.as_slice(), [a, b] if same(a, b)),
        "mov" => matches!(i.operands.as_slice(), [a @ Operand::Reg(r), b] if same(a, b) && (r.bits == 64 || (r.bits == 32 && !is_64))),
        "lea" => matches!(
            i.operands.as_slice(),
            [Operand::Reg(r), Operand::Mem(m)]
                if m.disp == 0
                    && m.segment.is_none()
                    && m.base.is_some_and(|b| b.id == r.id)
                    && m.index.is_none()
                    && (r.bits == 64 || (r.bits == 32 && !is_64))
        ),
        _ => false,
    }
}

#[cfg(test)]
mod fill_tests {
    use ra_asm_arch::Arch;

    use super::is_fill;

    fn fill(bytes: &[u8], is_64: bool) -> bool {
        let arch = ra_asm_x86::X86::for_pointer_size(if is_64 { 8 } else { 4 });
        let i = arch.decode(bytes, 0x1000).expect("an instruction");
        assert_eq!(usize::from(i.len), bytes.len(), "{bytes:02x?} decodes whole");
        is_fill(&i, is_64)
    }

    #[test]
    fn the_padding_forms_are_fill() {
        for bytes in [
            &[0x90][..],
            &[0xcc],
            &[0x66, 0x90],
            &[0x0f, 0x1f, 0x00],
            &[0x0f, 0x1f, 0x44, 0x00, 0x00],
            &[0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
            &[0x8d, 0x49, 0x00],
            &[0x8d, 0xa4, 0x24, 0x00, 0x00, 0x00, 0x00],
            &[0x8d, 0x9b, 0x00, 0x00, 0x00, 0x00],
            &[0x8b, 0xff],
        ] {
            assert!(fill(bytes, false), "{bytes:02x?} is fill");
        }
    }

    #[test]
    fn code_is_no_fill() {
        for bytes in [&[0x8b, 0xc1][..], &[0xc3], &[0x8d, 0x40, 0x04], &[0x8d, 0x04, 0x09], &[0x87, 0xd1]] {
            assert!(!fill(bytes, false), "{bytes:02x?} is code");
        }
        // A 32-bit `mov` of a register to itself clears the upper half on x64.
        assert!(!fill(&[0x8b, 0xff], true));
        assert!(fill(&[0x48, 0x8b, 0xff], true));
    }
}

/// Judges the functions only the gaps showed, in `functions`: the fill leaves them and is
/// returned, a dead one is marked [`Reach::Dead`], every other one [`Reach::Gap`]. `taken` is
/// [`address_taken`]; the calls, the tail calls and the local calls of every function name the
/// rest.
pub fn classify(image: &Image, functions: &mut BTreeMap<u64, Function>, taken: &BTreeSet<u64>) -> Fill {
    let is_64 = image.is_64();
    // Who names each function from its code: a call, a jump, a switch, a traced call. What the
    // data, the immediates and the image's tables name is in `taken`.
    let mut referrers: HashMap<u64, HashSet<u64>> = HashMap::new();
    for f in functions.values() {
        let mut named: Vec<u64> = f.calls.iter().chain(&f.tail_calls).chain(&f.local_calls).copied().collect();
        named.extend(f.traced_calls.values().flatten().copied());
        for b in &f.blocks {
            match &b.terminator {
                Terminator::NoReturnCall(t) | Terminator::TailCall(t) => named.push(*t),
                Terminator::Switch { targets, .. } => named.extend(targets.iter().copied().filter(|&t| !f.contains(t))),
                _ => {}
            }
        }
        for t in named {
            if t != f.entry {
                referrers.entry(t).or_default().insert(f.entry);
            }
        }
    }
    let only_gaps = |f: &Function| !f.origins.is_empty() && f.origins.iter().all(|o| matches!(o, Origin::Gap(_)));
    let candidates: Vec<u64> = functions
        .values()
        .filter(|f| only_gaps(f) && !taken.contains(&f.entry) && !referrers.contains_key(&f.entry))
        .map(|f| f.entry)
        .collect();
    let mut fill = Vec::new();
    for &va in &candidates {
        let f = &functions[&va];
        if f.blocks.iter().flat_map(|b| &b.instrs).all(|i| is_fill(i, is_64)) {
            fill.push(va);
        }
    }
    for va in &fill {
        functions.remove(va);
    }
    let index = BlocksIndex::of(functions);
    let arch = crate::arch_of(image);
    // Without relocations a pointer in data the scan does not read as one (unaligned, in code not
    // built) could name a function nothing else names: "nothing names it" is evidence only in an
    // image whose relocations list every absolute address.
    let complete = !image.relocations.is_empty();
    let mut dead: HashMap<u64, String> = HashMap::new();
    for &va in &candidates {
        let Some(f) = functions.get(&va) else { continue };
        let evidence =
            padding_evidence(f, &index, is_64).or_else(|| complete.then(|| unreached_evidence(image, arch.as_ref(), f, &index)).flatten());
        if let Some(why) = evidence {
            dead.insert(va, why);
        }
    }
    // A function only dead code names, by its calls and its jumps, is dead too: to a fixpoint.
    if complete {
        loop {
            let newly: Vec<(u64, String)> = functions
                .values()
                .filter(|f| !dead.contains_key(&f.entry) && !taken.contains(&f.entry))
                .filter(|f| f.origins.iter().all(|o| matches!(o, Origin::Gap(_) | Origin::Call(_) | Origin::TailCall(_))))
                .filter_map(|f| {
                    let by = referrers.get(&f.entry)?;
                    by.iter().all(|r| dead.contains_key(r)).then(|| {
                        let first = by.iter().min().copied().unwrap_or(f.entry);
                        (f.entry, format!("only dead code names it ({})", crate::function::default_name(first)))
                    })
                })
                .collect();
            if newly.is_empty() {
                break;
            }
            dead.extend(newly);
        }
    }
    for &va in &candidates {
        if let Some(f) = functions.get_mut(&va) {
            f.reach = Reach::Gap;
        }
    }
    for (va, why) in dead {
        if let Some(f) = functions.get_mut(&va) {
            f.reach = Reach::Dead(why);
        }
    }
    Fill(fill)
}

/// The blocks of every function, by start and by end.
struct BlocksIndex {
    /// start to (end, owner).
    starts: BTreeMap<u64, Vec<(u64, u64)>>,
    /// end to the terminators of the blocks that end there.
    ends: BTreeMap<u64, Vec<Terminator>>,
}

impl BlocksIndex {
    fn of(functions: &BTreeMap<u64, Function>) -> Self {
        let mut starts: BTreeMap<u64, Vec<(u64, u64)>> = BTreeMap::new();
        let mut ends: BTreeMap<u64, Vec<Terminator>> = BTreeMap::new();
        for f in functions.values() {
            for b in &f.blocks {
                starts.entry(b.start).or_default().push((b.end, f.entry));
                ends.entry(b.end).or_default().push(b.terminator.clone());
            }
        }
        Self { starts, ends }
    }

    /// The function other than `not` whose block starts at `va`.
    fn starting_at(&self, va: u64, not: u64) -> Option<u64> {
        self.starts.get(&va)?.iter().find(|&&(_, owner)| owner != not).map(|&(_, owner)| owner)
    }
}

/// Dead by its padding: `f` starts with fill, and the first instruction past the fill starts a
/// block of another function, whose alignment run the candidate was read from.
fn padding_evidence(f: &Function, index: &BlocksIndex, is_64: bool) -> Option<String> {
    let instrs: Vec<&ra_asm_arch::Instr> = f.blocks.iter().flat_map(|b| &b.instrs).collect();
    let leading = instrs.iter().take_while(|i| is_fill(i, is_64)).count();
    if leading == 0 {
        return None;
    }
    let past = instrs.get(leading).map_or_else(|| f.end(), |i| i.address);
    let other = index.starting_at(past, f.entry)?;
    Some(format!("inside the alignment run of {}", crate::function::default_name(other)))
}

/// Dead by what is before it: nothing names `f`, and the code before it ends in a `ret` (or
/// another way out that never falls through), with nothing but fill between, so no path of the
/// program reaches its entry. A call that never returns is a way out only with an `int 3` after
/// it, since the analysis could be wrong about the callee and the fill alone would then stop it.
fn unreached_evidence(image: &Image, arch: &dyn ra_asm_arch::Arch, f: &Function, index: &BlocksIndex) -> Option<String> {
    let is_64 = image.is_64();
    let (&end, terminators) = index.ends.range(..=f.entry).next_back()?;
    // No block runs over the entry from below: the nearest end is the code before it.
    if index.starts.range(end..f.entry).next().is_some() {
        return None;
    }
    let bytes = image.read(end, (f.entry - end) as usize).ok()?;
    let mut at = 0usize;
    let mut first_fill = None;
    while at < bytes.len() {
        let i = arch.decode(&bytes[at..], end + at as u64)?;
        if !is_fill(&i, is_64) {
            return None;
        }
        first_fill.get_or_insert(i.mnemonic.clone());
        at += usize::from(i.len);
    }
    if at != bytes.len() {
        return None;
    }
    let trap_after = first_fill.as_deref() == Some("int3");
    let way_out = |t: &Terminator| match t {
        Terminator::Return => Some("a ret"),
        Terminator::Jump(_) | Terminator::TailCall(_) | Terminator::ImportTailCall(_) => Some("a jump"),
        Terminator::Switch { .. } | Terminator::IndirectJump => Some("an indirect jump"),
        Terminator::NoReturnCall(_) | Terminator::NoReturnImportCall(_) | Terminator::Trap if trap_after => {
            Some("a call that never returns")
        }
        _ => None,
    };
    let ways: Vec<&str> = terminators.iter().map(way_out).collect::<Option<Vec<_>>>()?;
    let what = ways.first()?;
    Some(if bytes.is_empty() {
        format!("unreached: right after {what}")
    } else {
        format!("unreached: after {what} and {} bytes of fill", bytes.len())
    })
}
