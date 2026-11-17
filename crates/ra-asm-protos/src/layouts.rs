//! The layouts of the structures the Windows API's prototypes point to, at 32 and at 64 bits: a
//! 32-bit program hands the 64-bit host a `WNDCLASSEXA` laid out for 4-byte pointers, and the
//! adapter that widens it reads where each field sits at both widths here. The table is in
//! `win32_layouts`, generated from the SDK headers by clang's record layouts
//! (`tests/win32_layouts.rs`); a structure the headers' layouts do not cover is not in it.

use crate::sdl2_layouts;
use crate::win32_layouts;

/// One generated table: its structures, the typedef names of each, the parameters of each
/// function that point to one, and the functions whose result points to one, every list sorted
/// by name.
struct Table {
    records: &'static [Record],
    aliases: &'static [(&'static str, &'static str)],
    pointees: &'static [(&'static str, &'static [(u8, &'static str)])],
    results: &'static [(&'static str, &'static str)],
}

/// The tables, the Windows API's first: SDL2's (`sdl2_layouts`, its mixer's and network
/// library's beside it) lays out the structures a program's SDL calls point to.
fn tables() -> [Table; 2] {
    [
        Table {
            records: win32_layouts::RECORDS,
            aliases: win32_layouts::ALIASES,
            pointees: win32_layouts::POINTEES,
            results: win32_layouts::RESULTS,
        },
        Table {
            records: sdl2_layouts::RECORDS,
            aliases: sdl2_layouts::ALIASES,
            pointees: sdl2_layouts::POINTEES,
            results: sdl2_layouts::RESULTS,
        },
    ]
}

/// The record of a `void **` parameter, through which the callee writes a pointer back
/// (`SDL_LockTexture`'s pixels): one pointer, 4 bytes in the program and 8 in the host.
pub const POINTER_CELL: &str = "ra_pointer";

static POINTER_CELL_RECORD: Record = Record {
    name: POINTER_CELL,
    overlapping: false,
    same: false,
    size32: 4,
    align32: 4,
    size64: 8,
    align64: 8,
    fields: &[Field { name: "pointer", kind: Kind::Pointer, count: 1, at32: Place { offset: 0, size: 4 }, at64: Place { offset: 0, size: 8 }, record: "" }],
};

/// The structures the host makes and keeps, and hands the program a pointer to: the program reads
/// their fields, and hands the same pointer back. Each crosses as an object, a copy laid out for
/// the program that stands for the host's, and not as a structure converted around one call. Bound
/// by hand: the headers do not say who owns a structure.
const OBJECTS: &[&str] = &["SDL_Palette", "SDL_PixelFormat", "SDL_Surface"];

/// The pointer fields of an object that point to another, by the record, the field and the
/// record pointed to. Bound by hand: the layouts say a field is a pointer, not to what.
const OBJECT_FIELDS: &[(&str, &str, &str)] =
    &[("SDL_PixelFormat", "next", "SDL_PixelFormat"), ("SDL_PixelFormat", "palette", "SDL_Palette"), ("SDL_Surface", "format", "SDL_PixelFormat")];

/// The functions after which the object they take may be gone: its copy is not read again from
/// the host's.
const OBJECT_FREES: &[&str] = &["SDL_FreeFormat", "SDL_FreePalette", "SDL_FreeSurface"];

/// Whether `function` may free the object it takes ([`OBJECT_FREES`]).
pub fn frees_object(function: &str) -> bool {
    OBJECT_FREES.contains(&function)
}

/// The structures the program makes and hands the host, which the host keeps a pointer to past
/// the call: SDL_mixer plays a chunk after `Mix_PlayChannel` returns. Each crosses as a lasting copy
/// of the host's per structure of the program's, written anew at every call that hands it over.
const KEPT: &[&str] = &["Mix_Chunk"];

/// Whether the host keeps a pointer to the program's `record` past the call ([`KEPT`]).
pub fn is_kept(record: &str) -> bool {
    KEPT.contains(&record)
}

/// Whether the structure `record` is an object the host keeps ([`OBJECTS`]).
pub fn is_object(record: &str) -> bool {
    OBJECTS.contains(&record)
}

/// The object a pointer field of `record` points to, by its record's name.
pub fn object_field(record: &str, field: &str) -> Option<&'static str> {
    OBJECT_FIELDS.iter().find(|&&(r, f, _)| r == record && f == field).map(|&(_, _, to)| to)
}

/// The structure a function's result points to, when the tables have one.
pub fn result(function: &str) -> Option<&'static Record> {
    tables().into_iter().find_map(|t| {
        let i = t.results.binary_search_by_key(&function, |(name, _)| name).ok()?;
        layout(t.results[i].1)
    })
}

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
    if name == POINTER_CELL {
        return Some(&POINTER_CELL_RECORD);
    }
    tables().into_iter().find_map(|t| {
        let by_name = |name: &str| t.records.binary_search_by_key(&name, |r| r.name).ok().map(|i| &t.records[i]);
        by_name(name).or_else(|| {
            let i = t.aliases.binary_search_by_key(&name, |(alias, _)| alias).ok()?;
            by_name(t.aliases[i].1)
        })
    })
}

/// The parameters of a Windows API or SDL2 function that point to a structure the tables have:
/// the parameter's index, from 0, and the structure, in order; empty for a function with none.
pub fn pointees(function: &str) -> Vec<(usize, &'static Record)> {
    for t in tables() {
        if let Ok(i) = t.pointees.binary_search_by_key(&function, |(name, _)| name) {
            return t.pointees[i].1.iter().filter_map(|&(param, record)| Some((usize::from(param), layout(record)?))).collect();
        }
    }
    Vec::new()
}

/// Every structure of the Windows API's table, by name.
pub fn records() -> &'static [Record] {
    win32_layouts::RECORDS
}

/// Every structure of SDL2's table, by name.
pub fn sdl2_records() -> &'static [Record] {
    sdl2_layouts::RECORDS
}
