//! Rows of fixed 32-bit instruction words, and the table that decodes a word to one.

use std::fmt;

use crate::sem::Sem;

/// One instruction of the machine: the word `w` is this row when `w & mask == bits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    /// The bits the encoding fixes.
    pub mask: u32,
    /// Their values; a bit outside `mask` is zero.
    pub bits: u32,
    /// The operands the word carries, in the order [`Decoded::values`] gives them.
    pub fields: &'static [Field],
    pub flow: Flow,
    /// What the instruction does, over a [`crate::sem::Builder`]; a row without it lifts to a trap.
    pub sem: Option<Sem>,
}

/// An operand of a row: `width` bits of the word from bit `lo` up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub lo: u8,
    pub width: u8,
    /// Whether the top bit of the field is its sign: an offset or an immediate that may be negative.
    pub signed: bool,
    /// What the value is an operand of.
    pub kind: Kind,
}

/// What a field's value means, which is all the neutral record and the text need of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The number of a register of the machine's file.
    Reg,
    /// A constant.
    Imm,
    /// A code address relative to the instruction: `address + from + (value << shift)`.
    Rel { shift: u8, from: i8 },
    /// The displacement of a memory access of `bits` bits based on the register field named `base`,
    /// which the access takes in and which is then no operand of its own.
    Mem { base: &'static str, bits: u16 },
}

impl Field {
    /// The field's value in `word`, sign-extended when the field is signed.
    pub fn extract(&self, word: u32) -> i64 {
        let width = u32::from(self.width);
        let raw = (u64::from(word) >> self.lo) & ((1u64 << width) - 1);
        if self.signed && (raw >> (width - 1)) & 1 == 1 { raw as i64 - (1i64 << width) } else { raw as i64 }
    }
}

/// Where control goes after a row's instruction; the target, when there is one, is in its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// The next instruction.
    Next,
    /// A jump that always goes to its target.
    Jump,
    /// A jump that goes to its target or falls through.
    Branch,
    /// A call: control returns to the next instruction unless the callee never returns.
    Call,
    Return,
    /// Execution does not continue here.
    Trap,
    /// A transfer that takes effect after the next instruction, its delay slot, has run: the
    /// condition and the target are what they are before the slot runs, and control goes on past
    /// the slot. A likely one runs its slot only when it is taken.
    Delayed {
        transfer: Transfer,
        likely: bool,
    },
}

/// How a delayed row transfers control, as the flow of the same name does at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Jump,
    Branch,
    Call,
    Return,
}

impl Flow {
    /// Whether the row is a delayed transfer, which has a delay slot.
    pub fn delayed(self) -> bool {
        matches!(self, Flow::Delayed { .. })
    }

    /// Whether control may go elsewhere than to the next instruction, or come back to it from a
    /// callee: what a delay slot may not hold.
    pub fn transfers(self) -> bool {
        !matches!(self, Flow::Next | Flow::Trap)
    }
}

/// Why a set of rows is not a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Both rows accept `word` and neither is strictly more specific, so no row wins it.
    Overlap { first: &'static str, second: &'static str, word: u32 },
    /// The row sets a bit outside its mask, so it accepts no word.
    Bits { row: &'static str },
    /// The field is empty or runs past bit 31.
    Field { row: &'static str, field: &'static str },
    /// The memory field's base names no register field of its row.
    Base { row: &'static str, field: &'static str },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Overlap { first, second, word } => {
                write!(f, "rows `{first}` and `{second}` both accept {word:#010x} and neither is more specific")
            }
            Error::Bits { row } => write!(f, "row `{row}` sets bits outside its mask"),
            Error::Field { row, field } => write!(f, "field `{field}` of row `{row}` is not inside the 32-bit word"),
            Error::Base { row, field } => write!(f, "the base of field `{field}` of row `{row}` is not one of its register fields"),
        }
    }
}

impl std::error::Error for Error {}

/// A word decoded: the row that won it and the values of that row's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded<'t> {
    pub row: &'t Row,
    /// One value per field of the row, in the row's order.
    pub values: Vec<i64>,
}

impl Decoded<'_> {
    /// The value of the field named `name`.
    pub fn field(&self, name: &str) -> Option<i64> {
        self.row.fields.iter().position(|field| field.name == name).map(|at| self.values[at])
    }
}

/// The rows of a machine and the tree that decodes a word to one.
#[derive(Debug, Clone)]
pub struct Table {
    rows: Vec<Row>,
    root: Node,
}

/// A node of the decode tree: a switch on some bits of the word, or the row its path settles on.
#[derive(Debug, Clone)]
enum Node {
    /// The most specific row that accepts every word reaching here, or none.
    Leaf(Option<usize>),
    /// The arm whose key is `word & mask`, sorted by key; a word with no arm is no row.
    Switch { mask: u32, arms: Vec<(u32, Node)> },
}

impl Table {
    /// The table of `rows`, refused when a word would be claimed by two rows of which neither is
    /// strictly more specific.
    pub fn new(rows: &[Row]) -> Result<Table, Error> {
        for row in rows {
            if row.bits & !row.mask != 0 {
                return Err(Error::Bits { row: row.name });
            }
            if let Some(field) = row.fields.iter().find(|field| field.width == 0 || u32::from(field.lo) + u32::from(field.width) > 32) {
                return Err(Error::Field { row: row.name, field: field.name });
            }
            let names_register = |name: &str| row.fields.iter().any(|field| field.name == name && field.kind == Kind::Reg);
            if let Some(field) = row.fields.iter().find(|field| matches!(field.kind, Kind::Mem { base, .. } if !names_register(base))) {
                return Err(Error::Base { row: row.name, field: field.name });
            }
        }
        for (at, first) in rows.iter().enumerate() {
            for second in &rows[at + 1..] {
                let both = first.mask & second.mask;
                if (first.bits ^ second.bits) & both != 0 {
                    continue;
                }
                // A row whose mask holds the other's accepts a subset of its words; equal masks are the same set.
                let nested = (both == first.mask) != (both == second.mask);
                if !nested {
                    return Err(Error::Overlap { first: first.name, second: second.name, word: first.bits | second.bits });
                }
            }
        }
        let all: Vec<usize> = (0..rows.len()).collect();
        let root = build(rows, &all, 0);
        Ok(Table { rows: rows.to_vec(), root })
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The most specific row that accepts `word`, and its field values; `None` when no row does.
    pub fn decode(&self, word: u32) -> Option<Decoded<'_>> {
        let mut node = &self.root;
        let at = loop {
            match node {
                Node::Leaf(at) => break (*at)?,
                Node::Switch { mask, arms } => {
                    let arm = arms.binary_search_by_key(&(word & mask), |(key, _)| *key).ok()?;
                    node = &arms[arm].1;
                }
            }
        };
        let row = &self.rows[at];
        Some(Decoded { row, values: row.fields.iter().map(|field| field.extract(word)).collect() })
    }
}

/// The tree for the words that agree with the path on the `tested` bits, over the rows of `set`
/// that the path has not ruled out.
fn build(rows: &[Row], set: &[usize], tested: u32) -> Node {
    let left = |at: usize| rows[at].mask & !tested;
    let common = set.iter().fold(u32::MAX, |common, &at| common & left(at));
    if !set.is_empty() && common != 0 {
        // Every row fixes these bits, so a word whose bits no row has is no row.
        let mut keys: Vec<u32> = set.iter().map(|&at| rows[at].bits & common).collect();
        keys.sort_unstable();
        keys.dedup();
        let arms = keys
            .into_iter()
            .map(|key| {
                let under: Vec<usize> = set.iter().copied().filter(|&at| rows[at].bits & common == key).collect();
                (key, build(rows, &under, tested | common))
            })
            .collect();
        return Node::Switch { mask: common, arms };
    }
    let open = set.iter().fold(0, |open, &at| open | left(at));
    if open == 0 {
        // Each row here has had all its fixed bits matched; the checked overlaps make their masks a
        // chain, so the widest mask is the one strictly more specific than the rest.
        return Node::Leaf(set.iter().copied().max_by_key(|&at| rows[at].mask.count_ones()));
    }
    // The rows disagree on which bits they fix: test the bit most of them fix, and send the rows
    // that leave it open down both arms.
    let bit = (0..32)
        .map(|n| 1u32 << n)
        .filter(|bit| open & bit != 0)
        .max_by_key(|bit| (set.iter().filter(|&&at| left(at) & bit != 0).count(), !bit))
        .unwrap_or(0);
    let arms = [0, bit]
        .into_iter()
        .map(|key| {
            let under: Vec<usize> = set.iter().copied().filter(|&at| rows[at].mask & bit == 0 || rows[at].bits & bit == key).collect();
            (key, build(rows, &under, tested | bit))
        })
        .collect();
    Node::Switch { mask: bit, arms }
}
