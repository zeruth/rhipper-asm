//! Calls through a register that holds an import: `mov ebx, [__imp_WriteFile]` once, then `call
//! ebx` in a loop, as a compiler writes a hot call to an import. A forward dataflow over the
//! blocks tracks which register holds which import slot's value: a load from the slot sets the
//! fact, any other write to the register clears it, a call clears the caller-saved registers, and
//! a block joins what all its predecessors agree on. A `call reg` with a fact is a call to the
//! import, with the import's own convention; a `jmp reg` with a fact is a tail call to it.

use std::collections::{BTreeMap, HashMap};

use ra_asm_arch::{Flow, Instr, Operand, Target};
use ra_asm_pe::Image;

use crate::function::{Function, Terminator};

/// The registers a call clobbers in every convention: the accumulator and the two scratch
/// registers, by the full register's name.
fn clobbered_by_call(full: &str) -> bool {
    matches!(full, "rax" | "rcx" | "rdx")
}

fn full_name(reg: ra_asm_arch::Register) -> &'static str {
    ra_asm_x86::registers::full_of(reg.full).name
}

/// The import slot an instruction loads into a register: `mov r, [slot]`.
fn slot_load(image: &Image, i: &Instr) -> Option<(&'static str, String)> {
    if i.mnemonic != "mov" || i.operands.len() != 2 {
        return None;
    }
    let (Operand::Reg(dst), Operand::Mem(m)) = (&i.operands[0], &i.operands[1]) else { return None };
    let va = m.absolute?;
    let import = image.import_at(va)?;
    Some((full_name(*dst), import.display_name()))
}

/// Resolves the register calls and jumps of `f` to imports, and turns a resolved jump into a
/// tail call to the import. Returns the import of each resolved call, by the instruction's address.
pub fn resolve(image: &Image, f: &mut Function) -> HashMap<u64, String> {
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
        if let Some((reg, import)) = slot_load(image, i) {
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
    // The calls and the jumps through a register that holds an import.
    let mut resolved = HashMap::new();
    for (k, at_entry) in entry.iter().enumerate() {
        let Some(mut facts) = at_entry.clone() else { continue };
        let count = f.blocks[k].instrs.len();
        for n in 0..count {
            let i = &f.blocks[k].instrs[n];
            let reg_target = match (&i.flow, i.operands.first()) {
                (Flow::Call(Target::Indirect) | Flow::Jump(Target::Indirect), Some(Operand::Reg(r))) => Some(full_name(*r)),
                _ => None,
            };
            if let Some(reg) = reg_target
                && let Some(import) = facts.get(reg).cloned()
            {
                if matches!(i.flow, Flow::Call(_)) {
                    resolved.insert(i.address, import.clone());
                } else if n + 1 == count && matches!(f.blocks[k].terminator, Terminator::IndirectJump) {
                    f.blocks[k].terminator = Terminator::ImportTailCall(import.clone());
                    if !f.import_calls.contains(&import) {
                        f.import_calls.push(import.clone());
                    }
                }
            }
            let i = &f.blocks[k].instrs[n];
            if let Some(import) = resolved.get(&i.address)
                && !f.import_calls.contains(import)
            {
                f.import_calls.push(import.clone());
            }
            transfer(&mut facts, &f.blocks[k].instrs[n]);
        }
    }
    resolved
}
