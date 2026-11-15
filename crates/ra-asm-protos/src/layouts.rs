//! The layouts of the structures the Windows API's prototypes point to, at 32 and at 64 bits: a
//! 32-bit program hands the 64-bit host a `WNDCLASSEXA` laid out for 4-byte pointers, and the
//! adapter that widens it reads where each field sits at both widths here. The table is in
//! `win32_layouts`, generated from the SDK headers by clang's record layouts
//! (`tests/win32_layouts.rs`); a structure the headers' layouts do not cover is not in it.

use crate::win32_layouts::{ALIASES, POINTEES, RECORDS};

/// What a field holds, the same at both widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A data or code pointer: 4 bytes at 32 bits, 8 at 64.
    Pointer,
    /// A signed integer (`char` is signed in the Windows ABI), or an enumeration.
    Signed,
    /// An unsigned integer.
    Unsigned,
    /// A `float` or a `double`.
    Float,
    /// A bit field: the place is its storage unit, which the bit fields around it may share.
    Bits,
    /// An array of structures (or an empty one), laid out by the record the field names.
    Record,
}

/// Where a field sits at one width: its offset from the structure's start and its size in
/// bytes, the whole array's for an array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub offset: u32,
    pub size: u32,
}

/// A field of a structure, a member of a nested structure being one field each (`pt.x`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    /// The member's path from the structure, through its named nested members.
    pub name: &'static str,
    pub kind: Kind,
    /// The elements of an array, 1 for a field that is not one.
    pub count: u32,
    pub at32: Place,
    pub at64: Place,
    /// The record of a [`Kind::Record`] field's elements, empty for another kind.
    pub record: &'static str,
}

/// A structure or union at both widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Record {
    /// The tag (`tagMSG`), or for a structure without one the first of its typedef names.
    pub name: &'static str,
    /// Some fields share storage: a union, or a structure that holds one.
    pub overlapping: bool,
    /// The two layouts are the same (no field changes with the address width), so the
    /// structure passes to the host as it is.
    pub same: bool,
    /// The size and alignment at 32 bits.
    pub size32: u32,
    pub align32: u32,
    /// The size and alignment at 64 bits.
    pub size64: u32,
    pub align64: u32,
    pub fields: &'static [Field],
}

impl Record {
    /// The field named `name`, a nested member by its path.
    pub fn field(&self, name: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// A field of the generated table, its kind as one letter: `p` pointer, `i` signed, `u`
/// unsigned, `f` floating, `b` bit field, `r` record.
#[allow(clippy::too_many_arguments)]
pub(crate) const fn field(
    name: &'static str,
    kind: char,
    count: u32,
    offset32: u32,
    size32: u32,
    offset64: u32,
    size64: u32,
    record: &'static str,
) -> Field {
    let kind = match kind {
        'p' => Kind::Pointer,
        'i' => Kind::Signed,
        'u' => Kind::Unsigned,
        'f' => Kind::Float,
        'b' => Kind::Bits,
        'r' => Kind::Record,
        _ => panic!("a field kind the table does not know"),
    };
    Field { name, kind, count, at32: Place { offset: offset32, size: size32 }, at64: Place { offset: offset64, size: size64 }, record }
}

/// A record of the generated table: name, overlapping, same, size and alignment at 32 then at
/// 64 bits, fields.
#[allow(clippy::too_many_arguments)]
pub(crate) const fn record(
    name: &'static str,
    overlapping: bool,
    same: bool,
    size32: u32,
    align32: u32,
    size64: u32,
    align64: u32,
    fields: &'static [Field],
) -> Record {
    Record { name, overlapping, same, size32, align32, size64, align64, fields }
}

/// The layout of a structure, by its tag (`tagWNDCLASSEXA`) or one of its typedef names
/// (`WNDCLASSEXA`, `WNDCLASSEX`); `None` for one the table does not have.
pub fn layout(name: &str) -> Option<&'static Record> {
    let by_name = |name: &str| RECORDS.binary_search_by_key(&name, |r| r.name).ok().map(|i| &RECORDS[i]);
    by_name(name).or_else(|| {
        let i = ALIASES.binary_search_by_key(&name, |(alias, _)| alias).ok()?;
        by_name(ALIASES[i].1)
    })
}

/// The parameters of a Windows API function that point to a structure the table has: the
/// parameter's index, from 0, and the structure, in order; empty for a function with none.
pub fn pointees(function: &str) -> Vec<(usize, &'static Record)> {
    let Ok(i) = POINTEES.binary_search_by_key(&function, |(name, _)| name) else { return Vec::new() };
    POINTEES[i].1.iter().filter_map(|&(param, record)| Some((usize::from(param), layout(record)?))).collect()
}

/// Every structure of the table, by name.
pub fn records() -> &'static [Record] {
    RECORDS
}
