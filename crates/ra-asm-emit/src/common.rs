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
    /// Whether a value whose one use is the phi move at its block's end is written inline there.
    fold_moves: bool,
}

/// Whether `inst` only reads what the runtime holds, as a load reads memory: the exception check,
/// which a call may change and nothing else does.
fn runtime_read(inst: &Inst) -> bool {
    matches!(inst, Inst::Call { callee: ra_asm_ir::Callee::Direct(n), args, .. } if args.is_empty() && n == "ra_eh_pending")
}

/// Whether `inst` does something besides computing its value, a runtime read aside.
fn acts(inst: &Inst) -> bool {
    inst.has_side_effects() && !runtime_read(inst)
}

impl<'a> Defs<'a> {
    pub(crate) fn new(f: &'a Function) -> Self {
        Self::with(f, false)
    }

    /// [`Self::new`] that also writes inline a value whose one use is a phi move at its block's
    /// end, and the exception check at its one use (a backend whose moves are parallel copies).
    pub(crate) fn with_moves(f: &'a Function) -> Self {
        Self::with(f, true)
    }

    fn with(f: &'a Function, fold_moves: bool) -> Self {
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
        let mut defs = Self { f, positions, phi_moves, uses, inline: HashSet::new(), fold_moves };
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

    /// How many times `v` is read.
    pub(crate) fn uses(&self, v: ValueId) -> usize {
        self.uses.get(&v).copied().unwrap_or(0)
    }

    /// The rule of `inlinable`, before the depth.
    fn inlinable_at_use(&self, v: ValueId) -> bool {
        let Some((block, index, inst)) = self.def(v) else { return false };
        if self.uses.get(&v).copied() != Some(1) || matches!(inst, Inst::Phi { .. } | Inst::Alloca { .. }) {
            return false;
        }
        // A call, or a value that writes one inline, moves to its use only past pure arithmetic:
        // a load or a call between would run before it, and C orders neither against it within
        // the expression of the use. Never into a phi move, which a `for` writes elsewhere.
        if self.calls(v) {
            for n in &block.instrs[index + 1..] {
                if n.inst.uses().contains(&v) {
                    // Not where the printer may write an operand twice or not at all: a field
                    // pointer at each of its uses, a select's arms, an unordered float compare.
                    return !matches!(n.inst, Inst::Gep { .. } | Inst::Select { .. } | Inst::FCmp { .. } | Inst::Phi { .. });
                }
                if n.inst.has_side_effects() || matches!(n.inst, Inst::Load { .. }) {
                    return false;
                }
            }
            return block.terminator.uses().contains(&v);
        }
        if self.acts(inst) {
            return false;
        }
        let reads_memory = self.reads_memory(v);
        for n in &block.instrs[index + 1..] {
            if n.inst.uses().contains(&v) {
                return true;
            }
            // The words a call through a pointer hands on past the six are stored in their own
            // global, which a read of the stack does not see.
            if reads_memory && n.inst.has_side_effects() && !(self.fold_moves && self.stores_args(&n.inst) && !self.reads_args(v, 0)) {
                return false;
            }
        }
        if block.terminator.uses().contains(&v) {
            return true;
        }
        // The moves at the block's end, after every instruction of it, before its branch.
        self.fold_moves && self.phi_moves.get(&block.id).is_some_and(|moves| moves.iter().any(|(_, o)| *o == Operand::Local(v)))
    }

    /// Whether `inst` stores into the global of the words a call through a pointer hands on past
    /// the universal six: at its address, or a pointer made of it by offsets and casts.
    fn stores_args(&self, inst: &Inst) -> bool {
        matches!(inst, Inst::Store { ptr, volatile: false, .. } if self.points_stores_args(ptr))
    }

    /// Whether the address `ptr` is the global of the extra call words or an offset from it, as
    /// its arithmetic says: a pointer that comes from memory, a call or a phi is the stack's.
    fn points_stores_args(&self, ptr: &Operand) -> bool {
        let mut at = ptr.clone();
        for _ in 0..16 {
            match &at {
                Operand::Global(g) | Operand::Address { global: g, .. } => return g == ra_asm_lift::state::ARGS,
                Operand::Local(v) => match self.def(*v) {
                    Some((_, _, Inst::Gep { ptr, indices, .. })) if indices.iter().all(|(_, i)| matches!(i, Operand::Int(_))) => {
                        at = ptr.clone();
                    }
                    Some((_, _, Inst::Cast { value, .. })) => at = value.clone(),
                    Some((_, _, Inst::Bin { op: ra_asm_ir::BinOp::Add | ra_asm_ir::BinOp::Sub, lhs, rhs, .. })) => {
                        // The pointer side of a sum: the operand that is not a constant.
                        at = if matches!(rhs, Operand::Int(_)) {
                            lhs.clone()
                        } else {
                            return self.points_stores_args(lhs) || self.points_stores_args(rhs);
                        };
                    }
                    _ => return false,
                },
                _ => return false,
            }
        }
        true
    }

    /// Whether the expression of `v`, as it is written inline, may load from the global of the
    /// extra call words.
    fn reads_args(&self, v: ValueId, depth: usize) -> bool {
        let Some((_, _, inst)) = self.def(v) else { return false };
        if depth > 32 {
            return true;
        }
        if let Inst::Load { ptr, .. } = inst
            && self.points_stores_args(ptr)
        {
            return true;
        }
        if matches!(inst, Inst::Phi { .. } | Inst::Call { .. } | Inst::Alloca { .. }) {
            return false;
        }
        inst.uses().into_iter().any(|u| self.uses.get(&u).copied() == Some(1) && self.reads_args(u, depth + 1))
    }

    /// Whether `inst` does something besides computing its value: the exception check only reads
    /// the runtime where the moves fold.
    fn acts(&self, inst: &Inst) -> bool {
        if self.fold_moves { acts(inst) } else { inst.has_side_effects() }
    }

    /// Whether the expression of `v`, as it would be written inline, makes a call (where the moves
    /// fold): `v` is a call with a result, or a pure value over a single-use operand that does.
    pub(crate) fn calls(&self, v: ValueId) -> bool {
        if !self.fold_moves {
            return false;
        }
        let Some((_, _, inst)) = self.def(v) else { return false };
        match inst {
            Inst::Call { .. } => !runtime_read(inst),
            Inst::Phi { .. } | Inst::Alloca { .. } => false,
            _ if inst.has_side_effects() => false,
            _ => inst.uses().into_iter().any(|o| self.uses.get(&o).copied() == Some(1) && self.calls(o)),
        }
    }

    /// Whether the expression of `v` reads memory: a load, or an operand that is written inline
    /// into it (a single-use pure value) and does. `cvtpd2ps xmm0, xmm0` zeroes the register
    /// before the converted value is stored: the conversion of the old value must read it first.
    fn reads_memory(&self, v: ValueId) -> bool {
        let Some((_, _, inst)) = self.def(v) else { return false };
        if matches!(inst, Inst::Load { .. }) || (self.fold_moves && runtime_read(inst)) {
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
        if self.acts(inst) || self.calls(v) || matches!(inst, Inst::Phi { .. } | Inst::Alloca { .. }) {
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

/// The identifiers of C or Rust source outside its comments and string literals, in order, each
/// with whether it opens its line as the target of a plain assignment (`x = ...;`). A field after
/// `.` or `->` is no identifier of its own.
pub(crate) fn identifiers(text: &str) -> Vec<(&str, bool)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // Only blanks since the line began.
    let mut line_start = true;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line_start = true;
                i += 1;
            }
            b' ' | b'\t' | b'\r' => i += 1,
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                line_start = false;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..].find("*/").map_or(bytes.len(), |end| i + 2 + end + 2);
                line_start = false;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let field = text[..start].ends_with('.') || text[..start].ends_with("->");
                if !field {
                    out.push((&text[start..i], line_start && text[i..].starts_with(" = ")));
                }
                line_start = false;
            }
            c if c.is_ascii_digit() => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.') {
                    i += 1;
                }
                line_start = false;
            }
            _ => {
                i += 1;
                line_start = false;
            }
        }
    }
    out
}

/// The number of `ra_trap` calls in a function: the instructions the lift did not translate.
pub(crate) fn trap_count(f: &Function) -> usize {
    f.blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(&i.inst, Inst::Call { callee: ra_asm_ir::Callee::Direct(n), .. } if n == "ra_trap"))
        .count()
}
