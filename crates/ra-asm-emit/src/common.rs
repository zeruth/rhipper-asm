//! What every printer needs of a function: where each value is defined, how often it is used,
//! which values each block hands to the phis of its successors, and which values can be written
//! inline at their one use.

use std::collections::{HashMap, HashSet};

use ra_asm_ir::{Block, BlockId, Function, Inst, Operand, Type, ValueId};

/// A field of a named struct a `getelementptr` names: the struct's name, the field's type and
/// its byte offset. `None` for any other `getelementptr`.
pub(crate) struct Field<'a> {
    pub name: &'a str,
    pub ty: &'a Type,
    pub offset: u64,
}

/// The field a `getelementptr %S, ptr %p, i64 0, i32 k` addresses, when `%S` is one of `types`.
pub(crate) fn field_of<'a>(types: &'a [(String, Vec<Type>)], base_ty: &Type, indices: &[(Type, Operand)]) -> Option<Field<'a>> {
    let Type::Named(name) = base_ty else { return None };
    let [(_, Operand::Int(0)), (_, Operand::Int(k))] = indices else { return None };
    let (name, fields) = types.iter().find(|(n, _)| n == name)?;
    let k = usize::try_from(*k).ok()?;
    let (offsets, _) = ra_asm_ir::types::layout(fields, 8)?;
    Some(Field { name, ty: fields.get(k)?, offset: *offsets.get(k)? })
}

/// The byte offset of the field a `getelementptr` of a named struct addresses.
pub(crate) fn field_offset(types: &[(String, Vec<Type>)], base_ty: &Type, indices: &[(Type, Operand)]) -> Option<u64> {
    field_of(types, base_ty, indices).map(|f| f.offset)
}

/// How deep an inlined expression may nest: past this, a value gets a name. An unrolled loop
/// (a 64-step hash) chains hundreds of single-use values, which no reader follows and which
/// the printers' recursion cannot hold.
const MAX_DEPTH: usize = 24;

pub(crate) struct Defs<'a> {
    f: &'a Function,
    /// Where each value is defined: its block and index.
    positions: HashMap<ValueId, (usize, usize)>,
    /// The values each phi takes from each predecessor: pred -> (phi, operand).
    pub phi_moves: HashMap<BlockId, Vec<(ValueId, Operand)>>,
    uses: HashMap<ValueId, usize>,
    /// The values written inline at their one use.
    inline: HashSet<ValueId>,
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
                    // A `switch` with many cases to one block lists the edge once per case; the
                    // move is one.
                    for (o, p) in incoming {
                        let moves = phi_moves.entry(*p).or_default();
                        if !moves.contains(&(v, o.clone())) {
                            moves.push((v, o.clone()));
                        }
                    }
                }
            }
        }
        let mut defs = Self { f, positions, phi_moves, uses, inline: HashSet::new() };
        // An inlined operand is defined earlier in the same block, so its depth is known by the
        // time its user is reached.
        let mut depth: HashMap<ValueId, usize> = HashMap::new();
        for b in &f.blocks {
            for i in &b.instrs {
                let Some(v) = i.result else { continue };
                if !defs.inlinable_at_use(v) {
                    continue;
                }
                let nested = i.inst.uses().into_iter().filter_map(|o| depth.get(&o)).max().copied().unwrap_or(0);
                if nested < MAX_DEPTH {
                    depth.insert(v, nested + 1);
                    defs.inline.insert(v);
                }
            }
        }
        defs
    }

    pub(crate) fn def(&self, v: ValueId) -> Option<(&'a Block, usize, &'a Inst)> {
        let &(bi, ii) = self.positions.get(&v)?;
        let block = &self.f.blocks[bi];
        Some((block, ii, &block.instrs[ii].inst))
    }

    /// Whether `v` is written inline at its one use: pure, used once, in its own block after its
    /// definition, when its expression reads memory with nothing between the two that could
    /// change what it reads (a store or a call), and not nested too deep; the structuring folds
    /// a block of such values into a condition.
    pub(crate) fn inlinable(&self, v: ValueId) -> bool {
        self.inline.contains(&v)
    }

    /// The rule of `inlinable`, before the depth.
    fn inlinable_at_use(&self, v: ValueId) -> bool {
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

    /// Whether the expression of `v` still computes the same at the end of its block: pure, and
    /// when it reads memory, nothing after it in the block writes or calls. A `for` step moves
    /// such a value from the latch to the loop's head.
    pub(crate) fn holds_to_block_end(&self, v: ValueId) -> bool {
        let Some((block, index, inst)) = self.def(v) else { return false };
        if inst.has_side_effects() || matches!(inst, Inst::Phi { .. } | Inst::Alloca { .. }) {
            return false;
        }
        !self.reads_memory(v) || block.instrs[index + 1..].iter().all(|n| !n.inst.has_side_effects())
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
/// The DLL and the ordinal of an import by ordinal, from the symbol the lift gives one:
/// `ra_ordinal_<dll with its dot an underscore>__<ordinal>`.
pub(crate) fn ordinal_import(symbol: &str) -> Option<(String, u32)> {
    let rest = symbol.strip_prefix("ra_ordinal_")?;
    let (dll, ordinal) = rest.rsplit_once("__")?;
    let (stem, extension) = dll.rsplit_once('_')?;
    Some((format!("{stem}.{extension}"), ordinal.parse().ok()?))
}

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
