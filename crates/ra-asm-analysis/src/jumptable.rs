//! Jump tables. The compilers write a dense `switch` in a few forms:
//!
//! - clang x64: `lea base, [table]; movsxd v, [base + idx*4]; add v, base; jmp v`, the entries
//!   relative to the table.
//! - MSVC x64: `lea base, [__ImageBase]; mov v, [base + idx*4 + table_rva]; add v, base; jmp v`,
//!   the entries RVAs.
//! - x86: `jmp [table + idx*4]`, the entries absolute addresses.
//! - MSVC x86, two levels: `movzx idx2, byte ptr [idx + cases]; jmp [idx2*4 + table]`, where the
//!   byte table maps a sparse index to a case number. The resolved table is the composition, by
//!   the original index, so a lift switches on it directly.
//!
//! The index is bounded by a `cmp idx, n` and a conditional jump before the table code, in the
//! same block or the one before it. Without a bound, the entries are read while they point at code.

use ra_asm_arch::{Flow, Instr, MemOperand, Operand, Register};
use ra_asm_pe::Image;

/// A resolved table.
#[derive(Debug, Clone)]
pub struct Table {
    /// The address of the first entry.
    pub address: u64,
    pub entry_size: u8,
    /// The target of each entry, in index order.
    pub targets: Vec<u64>,
    /// The index value of each target, as the index register holds it: `0..n` for a table read
    /// from its first entry; the values an `and` leaves whose entries point at code; the negative
    /// ones, wrapped to the register's width, that a `neg` or a `sub` before the read leaves.
    pub indices: Vec<u64>,
    /// The register that holds the index when the entry is read.
    pub index: Register,
    /// The address of the instruction that reads the entry.
    pub load_at: u64,
    /// The address of the compare that bounds the index, when one was found. Without it the
    /// table ends where its entries stop pointing at code, and the index is assumed to stay
    /// inside what was read.
    pub bound: Option<u64>,
    /// The byte or word table of a two-level table, by its address and its length in bytes: the
    /// code reads the case number out of it before the entry, and the number may live on past the
    /// jump (in `eax`, which a function may return), so a port reads it as the program does.
    pub case_table: Option<(u64, u64)>,
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

/// A place the index passes through on its way from the compare to the table read: a register,
/// or a stack slot unoptimized code keeps it in (`cmp [ebp-8], n; ...; mov eax, [ebp-8]`).
#[derive(Clone)]
enum Place {
    Reg(Register),
    Mem(MemOperand),
}

impl Place {
    fn of(operand: &Operand) -> Option<Place> {
        match operand {
            Operand::Reg(r) => Some(Place::Reg(*r)),
            Operand::Mem(m) if m.base.is_some() || m.index.is_some() => Some(Place::Mem(m.clone())),
            _ => None,
        }
    }

    fn is(&self, other: &Place) -> bool {
        let same_reg = |a: Option<Register>, b: Option<Register>| match (a, b) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => true,
            _ => false,
        };
        match (self, other) {
            (Place::Reg(a), Place::Reg(b)) => same(*a, *b),
            (Place::Mem(a), Place::Mem(b)) => {
                same_reg(a.base, b.base) && same_reg(a.index, b.index) && a.scale == b.scale && a.disp == b.disp
            }
            _ => false,
        }
    }
}

/// The upper bound of the index from `cmp idx, n` followed by a conditional jump: the number of
/// entries, with the address of the compare. The index may have moved between registers, or
/// through a stack slot, since the compare.
fn bound_of(blocks: &[&[Instr]], index: Register) -> Option<(usize, u64)> {
    let mut aliases = vec![Place::Reg(index)];
    for block in blocks {
        for i in block.iter().rev() {
            let dst = i.operands.first().and_then(Place::of);
            if i.mnemonic == "cmp" {
                if let (Some(d), Some(Operand::Imm { value, .. })) = (&dst, i.operands.get(1))
                    && aliases.iter().any(|a| a.is(d))
                {
                    // The jump after the compare says which side is the table.
                    let jump = block.last()?;
                    let n = *value;
                    let entries = match jump.mnemonic.as_str() {
                        "ja" | "jbe" | "jg" | "jle" => usize::try_from(n + 1).ok(),
                        "jae" | "jb" | "jge" | "jl" => usize::try_from(n).ok(),
                        _ => None,
                    };
                    return entries.map(|entries| (entries, i.address));
                }
                continue;
            }
            if let (Some(d), Some(src)) = (&dst, i.operands.get(1).and_then(Place::of))
                && matches!(i.mnemonic.as_str(), "mov" | "movsxd" | "movzx" | "movsx")
                && aliases.iter().any(|a| a.is(d))
            {
                aliases.push(src);
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
            let mut index = m.index?;
            let base = m.base.map_or(Some(0), |b| constant_of(block, last, b))?;
            let address = base.wrapping_add(m.disp as u64);
            let mut context: Vec<&[Instr]> = vec![block];
            context.extend_from_slice(predecessors);
            // A two-level table: the index came out of a byte or word table by the real index.
            let mut cases: Option<(u64, u16, u64)> = None;
            if let Some(load) = block[..last].iter().rev().find(|i| i.operands.first().and_then(reg_of).is_some_and(|d| same(d, index)))
                && load.mnemonic == "movzx"
                && let Some(Operand::Mem(cm)) = load.operands.get(1)
                && matches!(cm.bits, 8 | 16)
                && cm.scale <= 1
                && let (Some(real), None) | (None, Some(real)) = (cm.base, cm.index)
            {
                cases = Some((cm.disp as u64, cm.bits, load.address));
                index = real;
            }
            if cases.is_none()
                && m.scale == 4
                && let Some((indices, all)) = signed_indices(block, last, predecessors, index)
            {
                return read_indexed(image, address, 4, &indices, all, index, jump.address);
            }
            let (count, bound) = bound_of(&context, index).unzip();
            let Some((cases_address, case_bits, load_at)) = cases else {
                let table = read_table(image, address, m.scale.max(1), count, Relative::Absolute, index, jump.address)?;
                return Some(Table { bound, ..table });
            };
            // The indices the case table is read at: `0..n` under a compare's bound, else the
            // values an `and idx, mask` before the read leaves (`OPL3_WriteReg`'s `reg & 0xF0`,
            // whose sixteen values are the case table's slots), the mask's submasks.
            let indices: Vec<u64> = match count {
                Some(count) => (0..count.min(MAX_ENTRIES) as u64).collect(),
                None => {
                    let at = block.iter().position(|i| i.address == load_at)?;
                    let mask = mask_of(&block[..at], index)?;
                    (0..=mask).filter(|v| v & !mask == 0).collect()
                }
            };
            // The case of each index, then the target of each case.
            let mut numbers = Vec::with_capacity(indices.len());
            for &n in &indices {
                let slot = cases_address + n * u64::from(case_bits / 8);
                let case = if case_bits == 8 { image.u8(slot).ok().map(u64::from) } else { image.u16(slot).ok().map(u64::from) }?;
                numbers.push(case as usize);
            }
            let entries = numbers.iter().max().copied()? + 1;
            let table = read_table(image, address, m.scale.max(1), Some(entries), Relative::Absolute, index, load_at)?;
            let targets = numbers.iter().map(|&c| table.targets[c]).collect();
            let span = indices.last().map_or(0, |&n| n + 1);
            let case_table = Some((cases_address, span * u64::from(case_bits / 8)));
            Some(Table { targets, indices, bound, case_table, ..table })
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
            let (count, bound) = bound_of(&context, index).unzip();
            let table = read_table(image, address, load.scale.max(1), count, Relative::To(add_base), index, block[load_at].address)?;
            Some(Table { bound, ..table })
        }
        _ => None,
    }
}

/// Resolves the MIPS `jr t` that ends `block`, its delay slot left out, given the blocks that lead
/// into it: `sll s, idx, 2; lui b, %hi(table); addu a, s, b; lw t, %lo(table)(a)`, the entries
/// absolute addresses, the index bounded by `sltiu c, idx, n` and a `beqz c` or `bnez c` to the
/// default before it. The base may come whole from a `lui`/`addiu` pair, from `$gp` or from a
/// `.got` slot, and may be formed in the block before.
pub fn resolve_mips(image: &Image, block: &[Instr], predecessors: &[&[Instr]]) -> Option<Table> {
    let jump = block.last()?;
    let target = reg_of(jump.operands.first()?)?;
    let (load_at, load) = mips_writer(block, block.len() - 1, target)?;
    let Some(Operand::Mem(entry)) = load.operands.get(1) else { return None };
    if load.mnemonic != "lw" {
        return None;
    }
    let (add_at, add) = mips_writer(block, load_at, entry.base?)?;
    let (Some(x), Some(y)) = (add.operands.get(1).and_then(reg_of), add.operands.get(2).and_then(reg_of)) else { return None };
    if add.mnemonic != "addu" {
        return None;
    }
    let mut context: Vec<&[Instr]> = vec![block];
    context.extend_from_slice(predecessors);
    // One addend is the table's address, or its high half, the other the index times four.
    let (base, index) = [(x, y), (y, x)]
        .into_iter()
        .find_map(|(high, scaled)| Some((mips_constant(image, &context, add_at, high)?, mips_scaled_index(block, add_at, scaled)?)))?;
    let address = base.wrapping_add(entry.disp as u64) & 0xffff_ffff;
    let (count, bound) = mips_bound(&context, index).unzip();
    let table = read_table(image, address, 4, count, Relative::Absolute, index, load.address)?;
    Some(Table { bound, ..table })
}

/// Whether a MIPS instruction writes the register it names first, as every form that writes a
/// general register does: not a store, a branch, a multiply or divide into `hi` and `lo`, or a trap.
fn writes_first(i: &Instr) -> bool {
    const READS_FIRST: &[&str] =
        &["sb", "sh", "sw", "swl", "swr", "mult", "multu", "div", "divu", "mthi", "mtlo", "tge", "tgeu", "tlt", "tltu", "teq", "tne"];
    i.flow == Flow::Next && !READS_FIRST.contains(&i.mnemonic.as_str())
}

/// What the search back from an instruction finds of the last write of a MIPS register.
enum Write<'a> {
    At(usize, &'a Instr),
    /// A call or a system call, which clobber the registers, or the delay slot of a call, which
    /// runs before it, comes first.
    Clobbered,
    /// Nothing before it in the block writes the register.
    Unwritten,
}

/// The MIPS global pointer, `$gp`.
const GP: u16 = 28;

fn mips_write(instrs: &[Instr], until: usize, reg: Register) -> Write<'_> {
    for (n, i) in instrs[..until].iter().enumerate().rev() {
        if matches!(i.flow, Flow::Call(_) | Flow::Interrupt) || (n > 0 && matches!(instrs[n - 1].flow, Flow::Call(_))) {
            return Write::Clobbered;
        }
        if writes_first(i) && i.operands.first().and_then(reg_of).is_some_and(|d| same(d, reg)) {
            return Write::At(n, i);
        }
    }
    Write::Unwritten
}

/// The last MIPS instruction before `until` that writes `reg`, with its index.
fn mips_writer(instrs: &[Instr], until: usize, reg: Register) -> Option<(usize, &Instr)> {
    match mips_write(instrs, until, reg) {
        Write::At(n, i) => Some((n, i)),
        Write::Clobbered | Write::Unwritten => None,
    }
}

/// The constant a MIPS register holds before `until` in the first of `blocks`: `$zero`; a `lui`, or
/// an `addiu` or `ori` to a constant, as `%hi`/`%lo` build an address; `$gp` as the start routine
/// sets it, and an `addiu` to it as `%gp_rel` reaches small data; or a `lw` of a `.got` slot, as
/// position-independent code reaches its data through `%got`. When the block does not write the
/// register, the other blocks, the ones that lead into it, must all give it the same value.
fn mips_constant(image: &Image, blocks: &[&[Instr]], until: usize, reg: Register) -> Option<u64> {
    if reg.full == 0 {
        return Some(0);
    }
    let (block, before) = blocks.split_first()?;
    let (at, i) = match mips_write(block, until, reg) {
        Write::At(at, i) => (at, i),
        Write::Clobbered => return None,
        // `$gp` holds the start routine's value everywhere: code that computes its own from `$t9`
        // arrives at the same one.
        Write::Unwritten if before.is_empty() => return if reg.full == GP { image.global_pointer } else { None },
        Write::Unwritten => {
            let mut values = before.iter().map(|b| mips_constant(image, &[*b], b.len(), reg));
            let first = values.next()??;
            return values.all(|v| v == Some(first)).then_some(first);
        }
    };
    let mask = |v: u64| v & 0xffff_ffff;
    match (i.mnemonic.as_str(), i.operands.get(1), i.operands.get(2)) {
        ("lui", Some(Operand::Imm { value, .. }), None) => Some(mask((*value as u64) << 16)),
        ("addiu" | "ori", Some(Operand::Reg(source)), Some(Operand::Imm { value, .. })) => {
            let source = mips_constant(image, blocks, at, *source)?;
            Some(if i.mnemonic == "ori" { source | mask(*value as u64) } else { mask(source.wrapping_add(*value as u64)) })
        }
        ("lw", Some(Operand::Mem(slot)), _) if slot.base.is_some_and(|b| b.full == GP) => {
            let slot = mask(mips_constant(image, blocks, at, slot.base?)?.wrapping_add(slot.disp as u64));
            // Only the GOT holds a constant: small data reached the same way is a variable.
            if image.section_of(slot)?.name != ".got" {
                return None;
            }
            image.u32(slot).ok().map(u64::from)
        }
        _ => None,
    }
}

/// The index a MIPS register holds four times of before `until`: `sll s, idx, 2`.
fn mips_scaled_index(instrs: &[Instr], until: usize, reg: Register) -> Option<Register> {
    let (_, i) = mips_writer(instrs, until, reg)?;
    match (i.mnemonic.as_str(), i.operands.get(1), i.operands.get(2)) {
        ("sll", Some(Operand::Reg(index)), Some(Operand::Imm { value: 2, .. })) => Some(*index),
        _ => None,
    }
}

/// The number of entries `sltiu c, idx, n` bounds a MIPS index to, with the compare's address,
/// when a `beqz c` or `bnez c` after it in its block branches on the result and nothing writes the
/// index between the compare and the table read.
fn mips_bound(blocks: &[&[Instr]], index: Register) -> Option<(usize, u64)> {
    for block in blocks {
        for (n, i) in block.iter().enumerate().rev() {
            let dst = i.operands.first().and_then(reg_of);
            if i.mnemonic == "sltiu"
                && let (Some(c), Some(Operand::Reg(r)), Some(Operand::Imm { value, .. })) = (dst, i.operands.get(1), i.operands.get(2))
                && same(*r, index)
                && block[n + 1..].iter().any(|b| {
                    matches!(b.mnemonic.as_str(), "beqz" | "bnez") && b.operands.first().and_then(reg_of).is_some_and(|t| same(t, c))
                })
            {
                return usize::try_from(*value).ok().map(|entries| (entries, i.address));
            }
            if writes_first(i) && dst.is_some_and(|d| same(d, index)) {
                return None;
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
enum Relative {
    Absolute,
    To(u64),
}

fn read_table(
    image: &Image,
    address: u64,
    entry_size: u8,
    count: Option<usize>,
    relative: Relative,
    index: Register,
    load_at: u64,
) -> Option<Table> {
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
            // A 32-bit entry is an address of a 32-bit space, which a sign never widens.
            Relative::Absolute if entry_size == 4 => raw as u64 & 0xffff_ffff,
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
    let indices = (0..targets.len() as u64).collect();
    (!targets.is_empty()).then_some(Table { address, entry_size, targets, indices, index, load_at, bound: None, case_table: None })
}

/// The index values an x86 `jmp [table + idx*4]` can take that no `cmp` bounds from zero, as the
/// C runtime's `memcpy` writes them: an `and idx, 2^k - 1` before the read, whose entries for the
/// values the code never reaches (the alignment the path already ruled out) need not point at
/// code; a `neg idx` after the compare, which counts the entries down from the table's address;
/// a `sub idx, k` whose borrow jumps to the read, which leaves `-k..-1`. Each with whether every
/// entry must point at code.
fn signed_indices(block: &[Instr], last: usize, predecessors: &[&[Instr]], index: Register) -> Option<(Vec<i64>, bool)> {
    let writer = block[..last].iter().rev().find(|i| i.writes(index));
    match writer {
        Some(i) if i.mnemonic == "and" && i.operands.first().and_then(reg_of).is_some_and(|d| same(d, index)) => {
            let Some(Operand::Imm { value: mask, .. }) = i.operands.get(1) else { return None };
            let mask = *mask;
            ((1..1024).contains(&mask) && (mask + 1) & mask == 0).then(|| ((0..=mask).collect(), false))
        }
        Some(i) if i.mnemonic == "neg" && i.operands.first().and_then(reg_of).is_some_and(|d| same(d, index)) => {
            let mut context: Vec<&[Instr]> = vec![&block[..last]];
            context.extend_from_slice(predecessors);
            let (count, _) = bound_of(&context, index)?;
            Some(((0..count as i64).map(|n| -n).collect(), true))
        }
        Some(_) => None,
        None => {
            // `sub idx, k; jb read`: the read's own block writes nothing of the index.
            let start = block.first()?.address;
            predecessors.iter().find_map(|p| {
                let [.., sub, jump] = p else { return None };
                let borrow = matches!(jump.mnemonic.as_str(), "jb" | "jc" | "jnae") && jump.flow == Flow::CondJump(ra_asm_arch::Target::Direct(start));
                match (sub.mnemonic.as_str(), sub.operands.first().and_then(reg_of), sub.operands.get(1)) {
                    ("sub", Some(d), Some(Operand::Imm { value: k, .. })) if borrow && same(d, index) && (1..1024).contains(k) => {
                        Some(((-*k..0).collect(), true))
                    }
                    _ => None,
                }
            })
        }
    }
}

/// The mask of an `and idx, mask` that last writes `index` in `instrs`, when one does and the mask
/// is small enough to read the table at every value it leaves.
fn mask_of(instrs: &[Instr], index: Register) -> Option<u64> {
    let writer = instrs.iter().rev().find(|i| i.writes(index))?;
    if writer.mnemonic != "and" || !writer.operands.first().and_then(reg_of).is_some_and(|d| same(d, index)) {
        return None;
    }
    let Some(Operand::Imm { value, .. }) = writer.operands.get(1) else { return None };
    let mask = u64::try_from(*value).ok()?;
    (1..MAX_ENTRIES as u64).contains(&mask).then_some(mask)
}

/// The table at `address` read at the entries of `indices`, each index wrapped to the width of
/// `index`: with `all`, a table only when every entry points at code; without, the entries that
/// do, when one does.
fn read_indexed(image: &Image, address: u64, scale: u8, indices: &[i64], all: bool, index: Register, load_at: u64) -> Option<Table> {
    let width_mask = if index.bits >= 64 { u64::MAX } else { (1u64 << index.bits) - 1 };
    let (mut targets, mut kept) = (Vec::new(), Vec::new());
    for &n in indices {
        let slot = address.wrapping_add((n * i64::from(scale)) as u64) & 0xffff_ffff;
        let target = image.u32(slot).ok().map(u64::from).filter(|&t| image.is_code(t));
        match target {
            Some(t) => {
                targets.push(t);
                kept.push(n as u64 & width_mask);
            }
            None if all => return None,
            None => {}
        }
    }
    (!targets.is_empty()).then_some(Table { address, entry_size: 4, targets, indices: kept, index, load_at, bound: None, case_table: None })
}
