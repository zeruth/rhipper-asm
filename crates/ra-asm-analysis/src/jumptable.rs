//! Jump tables. The compilers write a dense `switch` in a few forms:
//!
//! - clang x64: `lea base, [table]; movsxd v, [base + idx*4]; add v, base; jmp v`, the entries
//!   relative to the table.
//! - MSVC x64: `lea base, [__ImageBase]; mov v, [base + idx*4 + table_rva]; add v, base; jmp v`,
//!   the entries RVAs.
//! - x86: `jmp [table + idx*4]`, the entries absolute addresses.
//!
//! The index is bounded by a `cmp idx, n` and a conditional jump before the table code, in the
//! same block or the one before it. Without a bound, the entries are read while they point at code.

use ra_asm_arch::{Instr, Operand, Register};
use ra_asm_pe::Image;

/// A resolved table.
#[derive(Debug, Clone)]
pub struct Table {
    /// The address of the first entry.
    pub address: u64,
    pub entry_size: u8,
    pub targets: Vec<u64>,
}

/// The most entries a table is read for when no compare bounds it.
const MAX_ENTRIES: usize = 1024;

fn reg_of(operand: &Operand) -> Option<Register> {
    match operand {
        Operand::Reg(r) => Some(*r),
        _ => None,
    }
}

fn same(a: Register, b: Register) -> bool {
    a.full == b.full
}

/// The constant a register holds before `until`, from a `lea r, [absolute]` or `mov r, imm`.
fn constant_of(instrs: &[Instr], until: usize, reg: Register) -> Option<u64> {
    for i in instrs[..until].iter().rev() {
        let Some(dst) = i.operands.first().and_then(reg_of) else { continue };
        if !same(dst, reg) {
            if i.writes(reg) {
                return None;
            }
            continue;
        }
        return match (i.mnemonic.as_str(), i.operands.get(1)) {
            ("lea", Some(Operand::Mem(m))) => m.absolute,
            ("mov", Some(Operand::Imm { value, .. })) => Some(*value as u64),
            _ => None,
        };
    }
    None
}

/// The upper bound of the index from `cmp idx, n` followed by a conditional jump: the number of
/// entries. The index may have moved between registers since the compare.
fn bound_of(blocks: &[&[Instr]], index: Register) -> Option<usize> {
    let mut aliases = vec![index];
    for block in blocks {
        for i in block.iter().rev() {
            let dst = i.operands.first().and_then(reg_of);
            if i.mnemonic == "cmp" {
                if let (Some(r), Some(Operand::Imm { value, .. })) = (dst, i.operands.get(1))
                    && aliases.iter().any(|a| same(*a, r))
                {
                    // The jump after the compare says which side is the table.
                    let jump = block.last()?;
                    let n = *value;
                    return match jump.mnemonic.as_str() {
                        "ja" | "jbe" | "jg" | "jle" => usize::try_from(n + 1).ok(),
                        "jae" | "jb" | "jge" | "jl" => usize::try_from(n).ok(),
                        _ => None,
                    };
                }
                continue;
            }
            if let (Some(d), Some(Operand::Reg(src))) = (dst, i.operands.get(1))
                && matches!(i.mnemonic.as_str(), "mov" | "movsxd" | "movzx" | "movsx")
                && aliases.iter().any(|a| same(*a, d))
            {
                aliases.push(*src);
            }
        }
    }
    None
}

/// Resolves the indirect jump that ends `block`, given the blocks that lead into it.
pub fn resolve(image: &Image, block: &[Instr], predecessors: &[&[Instr]]) -> Option<Table> {
    let jump = block.last()?;
    let last = block.len() - 1;
    match jump.operands.first()? {
        // x86: jmp [table + idx*4]
        Operand::Mem(m) => {
            let index = m.index?;
            let base = m.base.map_or(Some(0), |b| constant_of(block, last, b))?;
            let address = base.wrapping_add(m.disp as u64);
            let mut context: Vec<&[Instr]> = vec![block];
            context.extend_from_slice(predecessors);
            let count = bound_of(&context, index);
            read_table(image, address, m.scale.max(1), count, Relative::Absolute)
        }
        // x64: jmp v, where v = base + entry
        Operand::Reg(value) => {
            let value = *value;
            // add v, base  (or add v, base with the operands the other way around)
            let (add_at, base) = block[..last].iter().enumerate().rev().find_map(|(n, i)| {
                if i.mnemonic != "add" {
                    return None;
                }
                let d = reg_of(i.operands.first()?)?;
                let s = reg_of(i.operands.get(1)?)?;
                if same(d, value) { Some((n, s)) } else { None }
            })?;
            // The load of the entry into v or into the base's partner.
            let (load_at, load) = block[..add_at].iter().enumerate().rev().find_map(|(n, i)| {
                let d = reg_of(i.operands.first()?)?;
                match i.operands.get(1)? {
                    Operand::Mem(m) if matches!(i.mnemonic.as_str(), "mov" | "movsxd" | "movzx") && (same(d, value) || same(d, base)) => {
                        Some((n, m.clone()))
                    }
                    _ => None,
                }
            })?;
            let index = load.index?;
            // The register the add used that is not the loaded value is the base; the load's own
            // base register holds the table (clang) or the image base (MSVC).
            let table_base_reg = load.base?;
            let table_base = constant_of(block, load_at, table_base_reg)?;
            let add_base = constant_of(block, add_at, base).or_else(|| constant_of(block, add_at, value))?;
            let address = table_base.wrapping_add(load.disp as u64);
            let mut context: Vec<&[Instr]> = vec![block];
            context.extend_from_slice(predecessors);
            let count = bound_of(&context, index);
            read_table(image, address, load.scale.max(1), count, Relative::To(add_base))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum Relative {
    Absolute,
    To(u64),
}

fn read_table(image: &Image, address: u64, entry_size: u8, count: Option<usize>, relative: Relative) -> Option<Table> {
    let limit = count.unwrap_or(MAX_ENTRIES).min(MAX_ENTRIES);
    let mut targets = Vec::new();
    for n in 0..limit {
        let slot = address + n as u64 * u64::from(entry_size);
        let raw = match entry_size {
            4 => image.u32(slot).ok().map(|v| i64::from(v as i32)),
            8 => image.u64(slot).ok().map(|v| v as i64),
            _ => None,
        };
        let Some(raw) = raw else { break };
        let target = match relative {
            Relative::Absolute => raw as u64,
            Relative::To(base) => base.wrapping_add(raw as u64),
        };
        if !image.is_code(target) {
            if count.is_some() {
                return None; // a bounded table with a bad entry is not a table
            }
            break;
        }
        targets.push(target);
    }
    (!targets.is_empty()).then_some(Table { address, entry_size, targets })
}
