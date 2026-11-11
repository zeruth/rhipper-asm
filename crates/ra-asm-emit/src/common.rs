//! What every printer needs of a function: where each value is defined, how often it is used,
//! which values each block hands to the phis of its successors, and which values can be written
//! inline at their one use.

use std::collections::HashMap;

use ra_asm_ir::{Block, BlockId, Function, Inst, Operand, ValueId};

pub(crate) struct Defs<'a> {
    f: &'a Function,
    /// Where each value is defined: its block and index.
    positions: HashMap<ValueId, (usize, usize)>,
    /// The values each phi takes from each predecessor: pred -> (phi, operand).
    pub phi_moves: HashMap<BlockId, Vec<(ValueId, Operand)>>,
    uses: HashMap<ValueId, usize>,
}

impl<'a> Defs<'a> {
    pub(crate) fn new(f: &'a Function) -> Self {
        let uses = f.use_counts();
        let mut phi_moves: HashMap<BlockId, Vec<(ValueId, Operand)>> = HashMap::new();
        let mut positions = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.instrs.iter().enumerate() {
                if let Some(v) = i.result {
                    positions.insert(v, (bi, ii));
                }
                if let (Some(v), Inst::Phi { incoming, .. }) = (i.result, &i.inst) {
                    for (o, p) in incoming {
                        phi_moves.entry(*p).or_default().push((v, o.clone()));
                    }
                }
            }
        }
        Self { f, positions, phi_moves, uses }
    }

    pub(crate) fn def(&self, v: ValueId) -> Option<(&'a Block, usize, &'a Inst)> {
        let &(bi, ii) = self.positions.get(&v)?;
        let block = &self.f.blocks[bi];
        Some((block, ii, &block.instrs[ii].inst))
    }

    /// Whether `v` can be written inline at its one use: pure, used once, in its own block after
    /// its definition, and when its expression reads memory, with nothing between the two that
    /// could change what it reads (a store or a call); the structuring folds a block of such
    /// values into a condition.
    pub(crate) fn inlinable(&self, v: ValueId) -> bool {
        let Some((block, index, inst)) = self.def(v) else { return false };
        if self.uses.get(&v).copied() != Some(1) || inst.has_side_effects() || matches!(inst, Inst::Phi { .. } | Inst::Alloca { .. }) {
            return false;
        }
        let reads_memory = self.reads_memory(v);
        for n in &block.instrs[index + 1..] {
            if n.inst.uses().contains(&v) {
                return true;
            }
            if reads_memory && n.inst.has_side_effects() {
                return false;
            }
        }
        block.terminator.uses().contains(&v)
    }

    /// Whether the expression of `v` reads memory: a load, or an operand that is written inline
    /// into it (a single-use pure value) and does. `cvtpd2ps xmm0, xmm0` zeroes the register
    /// before the converted value is stored: the conversion of the old value must read it first.
    fn reads_memory(&self, v: ValueId) -> bool {
        let Some((_, _, inst)) = self.def(v) else { return false };
        if matches!(inst, Inst::Load { .. }) {
            return true;
        }
        if inst.has_side_effects() || matches!(inst, Inst::Phi { .. } | Inst::Alloca { .. }) {
            return false;
        }
        inst.uses().into_iter().any(|o| self.uses.get(&o).copied() == Some(1) && self.reads_memory(o))
    }
}

/// `s` without one pair of outer parentheses, when they enclose the whole of it.
pub(crate) fn strip_parens(s: &str) -> String {
    if s.starts_with('(') && s.ends_with(')') {
        let mut depth = 0i32;
        for (i, c) in s.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i != s.len() - 1 {
                        return s.to_owned();
                    }
                }
                _ => {}
            }
        }
        return s[1..s.len() - 1].to_owned();
    }
    s.to_owned()
}

/// `s` in parentheses unless it is one token.
pub(crate) fn parenthesized(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || (s.starts_with('(') && strip_parens(s) != s) {
        s.to_owned()
    } else {
        format!("({s})")
    }
}

/// A value name as an identifier: the IR name with every other character replaced, a digit
/// start prefixed, and a keyword of the language suffixed.
pub(crate) fn identifier(f: &Function, v: ValueId, keywords: &[&str]) -> String {
    let n = f.value_name(v);
    let mut s: String = n.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, 'v');
    }
    if keywords.contains(&s.as_str()) {
        s.push('_');
    }
    s
}

/// The number of `ra_trap` calls in a function: the instructions the lift did not translate.
pub(crate) fn trap_count(f: &Function) -> usize {
    f.blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(&i.inst, Inst::Call { callee: ra_asm_ir::Callee::Direct(n), .. } if n == "ra_trap"))
        .count()
}
