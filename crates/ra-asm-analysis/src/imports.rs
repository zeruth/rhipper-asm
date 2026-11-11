//! Calls through a register that holds an import: `mov ebx, [__imp_WriteFile]` once, then `call
//! ebx` in a loop, as a compiler writes a hot call to an import. A forward dataflow over the
//! blocks tracks which register holds which import slot's value: a load from the slot sets the
//! fact, any other write to the register clears it, a call clears the caller-saved registers, and
//! a block joins what all its predecessors agree on. A `call reg` with a fact is a call to the
//! import, with the import's own convention; a `jmp reg` with a fact is a tail call to it.
//!
//! A data word can hold an import slot's value too: `mov eax, [__imp_WriteFile]; mov [p], eax`
//! stores it, and `call [p]` or `mov eax, [p]; call eax` reaches the import through it. The
//! dataflow reports every store to an absolute word with the fact it carries; the analysis marks
//! the words that only ever receive one import ([`import_words`]) and resolves every function
//! again with them, a load from such a word setting the same fact as a load from the slot.

use std::collections::{BTreeMap, HashMap};

use ra_asm_arch::{Flow, Instr, Operand, Target};
use ra_asm_pe::Image;

use crate::function::{Function, Terminator};

/// What [`resolve`] found in one function.
#[derive(Debug, Default)]
pub struct Resolved {
    /// The import of each resolved call, by the call's address.
    pub calls: HashMap<u64, String>,
    /// The absolute data words the function writes or takes the address of: the word and the
    /// import whose slot value the store carries, `None` for anything else.
    pub stores: Vec<(u64, Option<String>)>,
}

/// The registers a call clobbers in every convention: the accumulator and the two scratch
/// registers, by the full register's name.
fn clobbered_by_call(full: &str) -> bool {
    matches!(full, "rax" | "rcx" | "rdx")
}

fn full_name(reg: ra_asm_arch::Register) -> &'static str {
    ra_asm_x86::registers::full_of(reg.full).name
}

/// The import an absolute address holds: its slot, or a data word marked as holding the slot's value.
fn import_at(image: &Image, words: &HashMap<u64, String>, va: u64) -> Option<String> {
    image.import_at(va).map(ra_asm_pe::Import::display_name).or_else(|| words.get(&va).cloned())
}

/// The import an instruction loads into a register: `mov r, [slot]` or `mov r, [word]`.
fn slot_load(image: &Image, words: &HashMap<u64, String>, i: &Instr) -> Option<(&'static str, String)> {
    if i.mnemonic != "mov" || i.operands.len() != 2 {
        return None;
    }
    let (Operand::Reg(dst), Operand::Mem(m)) = (&i.operands[0], &i.operands[1]) else { return None };
    let import = import_at(image, words, m.absolute?)?;
    Some((full_name(*dst), import))
}

/// The absolute word an instruction writes or takes the address of, with the import fact the
/// stored register carries for a plain `mov [word], reg`.
fn word_store(facts: &BTreeMap<&'static str, String>, i: &Instr) -> Option<(u64, Option<String>)> {
    let (k, m) = i.operands.iter().enumerate().find_map(|(k, o)| match o {
        Operand::Mem(m) if m.absolute.is_some() => Some((k, m)),
        _ => None,
    })?;
    let va = m.absolute?;
    if i.mnemonic == "mov" && k == 0 && i.operands.len() == 2 {
        let fact = match &i.operands[1] {
            Operand::Reg(src) => facts.get(full_name(*src)).cloned(),
            _ => None,
        };
        return Some((va, fact));
    }
    // An address taken (`lea`, a width of zero) may be written through later: no word it names
    // keeps a fact.
    (i.writes_memory || m.bits == 0).then_some((va, None))
}

/// Resolves the register calls and jumps of `f` to imports, and turns a resolved jump into a
/// tail call to the import. `words` are the data words known to hold an import slot's value.
pub fn resolve(image: &Image, words: &HashMap<u64, String>, f: &mut Function) -> Resolved {
    type Facts = BTreeMap<&'static str, String>;
    let n = f.blocks.len();
    let index_of: HashMap<u64, usize> = f.blocks.iter().enumerate().map(|(k, b)| (b.start, k)).collect();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, b) in f.blocks.iter().enumerate() {
        for s in b.successors() {
            if let Some(&t) = index_of.get(&s) {
                preds[t].push(k);
            }
        }
    }
    let transfer = |facts: &mut Facts, i: &Instr| {
        if let Some((reg, import)) = slot_load(image, words, i) {
            facts.insert(reg, import);
            return;
        }
        if matches!(i.flow, Flow::Call(_)) {
            facts.retain(|reg, _| !clobbered_by_call(reg));
        }
        let written: Vec<&'static str> =
            facts.keys().copied().filter(|reg| i.registers.iter().any(|(r, a)| full_name(*r) == *reg && a.writes())).collect();
        for reg in written {
            facts.remove(reg);
        }
    };
    // The facts at each block's entry: unknown until a predecessor is seen, then the agreement.
    let mut entry: Vec<Option<Facts>> = vec![None; n];
    let mut exit: Vec<Option<Facts>> = vec![None; n];
    entry[0] = Some(Facts::new());
    for _round in 0..16 {
        let mut changed = false;
        for k in 0..n {
            if k != 0 {
                let mut joined: Option<Facts> = None;
                for &p in &preds[k] {
                    let Some(out) = &exit[p] else { continue };
                    joined = Some(match joined {
                        None => out.clone(),
                        Some(j) => j.into_iter().filter(|(reg, import)| out.get(reg) == Some(import)).collect(),
                    });
                }
                if joined != entry[k] {
                    entry[k] = joined;
                    changed = true;
                }
            }
            let Some(mut facts) = entry[k].clone() else { continue };
            for i in &f.blocks[k].instrs {
                transfer(&mut facts, i);
            }
            if exit[k].as_ref() != Some(&facts) {
                exit[k] = Some(facts);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // The calls and the jumps through a register or a word that holds an import, and the stores
    // to the words.
    let mut resolved = Resolved::default();
    for (k, at_entry) in entry.iter().enumerate() {
        let Some(mut facts) = at_entry.clone() else { continue };
        let count = f.blocks[k].instrs.len();
        for n in 0..count {
            let i = &f.blocks[k].instrs[n];
            let target = match (&i.flow, i.operands.first()) {
                (Flow::Call(Target::Indirect) | Flow::Jump(Target::Indirect), Some(Operand::Reg(r))) => facts.get(full_name(*r)).cloned(),
                (Flow::Call(Target::Indirect) | Flow::Jump(Target::Indirect), Some(Operand::Mem(m))) => {
                    m.absolute.and_then(|va| words.get(&va).cloned())
                }
                _ => None,
            };
            if let Some(import) = target {
                if matches!(i.flow, Flow::Call(_)) {
                    resolved.calls.insert(i.address, import.clone());
                } else if n + 1 == count && matches!(f.blocks[k].terminator, Terminator::IndirectJump) {
                    f.blocks[k].terminator = Terminator::ImportTailCall(import.clone());
                    if !f.import_calls.contains(&import) {
                        f.import_calls.push(import.clone());
                    }
                }
            }
            let i = &f.blocks[k].instrs[n];
            if let Some(import) = resolved.calls.get(&i.address)
                && !f.import_calls.contains(import)
            {
                f.import_calls.push(import.clone());
            }
            if let Some(store) = word_store(&facts, i) {
                resolved.stores.push(store);
            }
            transfer(&mut facts, &f.blocks[k].instrs[n]);
        }
    }
    resolved
}

/// The data words that hold an import slot's value, over every function: a word every store
/// gives the same import's slot value, and whose bytes nothing else writes or takes the address of.
pub fn import_words(image: &Image, functions: &mut BTreeMap<u64, Function>) -> HashMap<u64, String> {
    let width = u64::from(image.pointer_size());
    let none = HashMap::new();
    let mut stores: Vec<(u64, Option<String>)> = Vec::new();
    for f in functions.values_mut() {
        stores.extend(resolve(image, &none, f).stores);
    }
    let mut words: HashMap<u64, String> = HashMap::new();
    for (va, import) in &stores {
        let Some(import) = import else { continue };
        let agreed = stores.iter().all(|(other, fact)| {
            // Another store that touches the word's bytes, with a different fact or none.
            !(other + width > *va && *other < va + width) || (other == va && fact.as_ref() == Some(import))
        });
        if agreed && image.import_at(*va).is_none() {
            words.insert(*va, import.clone());
        }
    }
    words
}
