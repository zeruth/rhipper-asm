//! A row's fields in, its word out: the inverse of the decoder, which the cases and the tests build
//! their words with.

use std::fmt;

use crate::table::{Field, Row};

/// Why a row's values make no word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsmError {
    /// The row has `expected` fields and `given` values came.
    Count { row: &'static str, expected: usize, given: usize },
    /// The value does not fit the field: outside `0..2^width` unsigned, `-2^(width-1)..2^(width-1)`
    /// signed.
    Range { row: &'static str, field: &'static str, value: i64 },
    /// The value's bits disagree with the bits the row fixes or with another field over the same bits.
    Conflict { row: &'static str, field: &'static str },
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsmError::Count { row, expected, given } => write!(f, "row `{row}` has {expected} fields, not {given}"),
            AsmError::Range { row, field, value } => write!(f, "{value} does not fit field `{field}` of row `{row}`"),
            AsmError::Conflict { row, field } => {
                write!(f, "the value of field `{field}` of row `{row}` disagrees with the row's fixed bits or another field")
            }
        }
    }
}

impl std::error::Error for AsmError {}

impl Field {
    /// The values the field holds, lowest and highest.
    pub fn range(&self) -> (i64, i64) {
        let width = u32::from(self.width);
        if self.signed { (-(1i64 << (width - 1)), (1i64 << (width - 1)) - 1) } else { (0, (1i64 << width) - 1) }
    }

    /// The field's bits of the word that holds `value` there; `None` when the value does not fit.
    pub fn insert(&self, value: i64) -> Option<u32> {
        let (low, high) = self.range();
        if !(low..=high).contains(&value) {
            return None;
        }
        let raw = (value as u64) & ((1u64 << self.width) - 1);
        Some((raw << self.lo) as u32)
    }
}

/// The word of `row` whose fields hold `values`, in the row's order; the bits of the word that are
/// neither fixed nor a field's are zero.
pub fn assemble(row: &Row, values: &[i64]) -> Result<u32, AsmError> {
    if values.len() != row.fields.len() {
        return Err(AsmError::Count { row: row.name, expected: row.fields.len(), given: values.len() });
    }
    let mut word = row.bits;
    for (field, &value) in row.fields.iter().zip(values) {
        word |= field.insert(value).ok_or(AsmError::Range { row: row.name, field: field.name, value })?;
    }
    // A field over fixed bits or over another field's bits keeps a value only when they agree.
    let fixed = word & row.mask == row.bits;
    for (field, &value) in row.fields.iter().zip(values) {
        if !fixed || field.extract(word) != value {
            return Err(AsmError::Conflict { row: row.name, field: field.name });
        }
    }
    Ok(word)
}
