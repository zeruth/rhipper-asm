//! The conformance of the host's Win32 structure adapters: `testdata/conformance/win32_structs.toml`
//! names each structure a 32-bit program hands the 64-bit host, or the host hands it, with the way it
//! crosses the call and whether the adapter converts it yet. Each structure is filled as the 32-bit
//! program lays it out, converted by the adapter and compared with what its layouts at both widths
//! in `ra-asm-protos` say the other side holds. A pending row that converts fails the test, so the
//! table follows the adapter.

use std::path::PathBuf;

use ra_asm_host::adapt;
use ra_asm_protos::{Kind, Place, Record, layout};
use ra_asm_testing::manifest;

/// The structures the ignore reasons of rhtest's misc, sys and win name.
const NAMED: [&str; 10] = [
    "WNDCLASSA",
    "WNDCLASSEXA",
    "CREATESTRUCTA",
    "MSG",
    "OVERLAPPED",
    "MEMORY_BASIC_INFORMATION",
    "SLIST_HEADER",
    "INIT_ONCE",
    "WSADATA",
    "CRITICAL_SECTION",
];
/// The rows still pending: a change that makes one convert marks it passing and lowers this.
const PENDING: usize = 9;

/// The way a structure crosses the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// The program fills it and the host reads it.
    In,
    /// The host fills it and the program reads it.
    Out,
    /// The host reads it and writes it back.
    Both,
}

impl Direction {
    fn widens(self) -> bool {
        matches!(self, Direction::In | Direction::Both)
    }

    fn narrows(self) -> bool {
        matches!(self, Direction::Out | Direction::Both)
    }
}

/// A row of the table.
#[derive(Debug)]
struct Row {
    name: String,
    direction: Direction,
    pending: bool,
    /// The line of its `[[struct]]` header.
    line: usize,
}

fn table() -> PathBuf {
    manifest::workspace().join("testdata/conformance/win32_structs.toml")
}

/// The rows of the table text, in order; `#` starts a comment, and a value holds none.
fn parse(text: &str) -> Result<Vec<Row>, String> {
    struct Partial {
        line: usize,
        name: Option<String>,
        direction: Option<Direction>,
        pending: Option<bool>,
    }
    fn finish(partial: Partial) -> Result<Row, String> {
        let line = partial.line;
        let missing = |key: &str| format!("win32_structs.toml:{line}: the [[struct]] has no `{key}`");
        Ok(Row {
            name: partial.name.ok_or_else(|| missing("name"))?,
            direction: partial.direction.ok_or_else(|| missing("direction"))?,
            pending: partial.pending.ok_or_else(|| missing("status"))?,
            line,
        })
    }
    let mut rows = Vec::new();
    let mut current: Option<Partial> = None;
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let wrong = |what: &str| format!("win32_structs.toml:{number}: {line:?}: {what}");
        if line == "[[struct]]" {
            if let Some(partial) = current.take() {
                rows.push(finish(partial)?);
            }
            current = Some(Partial { line: number, name: None, direction: None, pending: None });
            continue;
        }
        let Some(partial) = current.as_mut() else { return Err(wrong("is outside a [[struct]]")) };
        let Some((key, value)) = line.split_once('=') else { return Err(wrong("is not `key = \"value\"`")) };
        let value = value.trim();
        let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            return Err(wrong("the value is not a string"));
        };
        match key.trim() {
            "name" if partial.name.is_none() => partial.name = Some(value.to_owned()),
            "direction" if partial.direction.is_none() => {
                partial.direction = Some(match value {
                    "in" => Direction::In,
                    "out" => Direction::Out,
                    "both" => Direction::Both,
                    _ => return Err(wrong("the direction is not in, out or both")),
                });
            }
            "status" if partial.pending.is_none() => {
                partial.pending = Some(match value {
                    "pending" => true,
                    "passing" => false,
                    _ => return Err(wrong("the status is not pending or passing")),
                });
            }
            "name" | "direction" | "status" => return Err(wrong("the key repeats")),
            _ => return Err(wrong("the key is not name, direction or status")),
        }
    }
    if let Some(partial) = current {
        rows.push(finish(partial)?);
    }
    Ok(rows)
}

fn rows() -> Vec<Row> {
    let path = table();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    parse(&text).unwrap_or_else(|e| panic!("{e}"))
}

/// The host's conversion of a 32-bit program's structure to its own layout; a structure the host
/// has no adapter for fails here.
fn to_host(record: &Record, guest: &[u8]) -> Result<Vec<u8>, String> {
    adapt::adapter(record.name)?.to_host(guest)
}

/// The host's conversion of its own structure to the 32-bit program's layout.
fn to_guest(record: &Record, host: &[u8]) -> Result<Vec<u8>, String> {
    adapt::adapter(record.name)?.to_guest(host)
}

/// The bytes of a place at an offset from a base.
fn span(place: Place, base: usize) -> std::ops::Range<usize> {
    let start = base + place.offset as usize;
    start..start + place.size as usize
}

/// Fills every field of `record` at `base` in `guest` with bytes that differ along the structure,
/// so that a field moved, cut or extended the wrong way shows; padding stays zero.
fn fill(record: &Record, guest: &mut [u8], base: usize) -> Result<(), String> {
    for field in record.fields {
        if field.kind == Kind::Record {
            let inner = layout(field.record).ok_or_else(|| format!("{}: the layout table does not have {}", record.name, field.record))?;
            for element in 0..field.count as usize {
                fill(inner, guest, base + field.at32.offset as usize + element * inner.size32 as usize)?;
            }
        } else {
            for at in span(field.at32, base) {
                #[allow(clippy::cast_possible_truncation)]
                let byte = (at as u8).wrapping_mul(29) ^ 0x96;
                guest[at] = byte;
            }
        }
    }
    Ok(())
}

/// What the host holds for the 32-bit structure `guest` at `from` once widened, written at `to` in
/// `host`: a pointer and an unsigned integer zero-extended (the port keeps the program's 32-bit
/// addresses), a signed integer sign-extended, a float and a bit field's unit as they are.
fn widen(record: &Record, guest: &[u8], from: usize, host: &mut [u8], to: usize) -> Result<(), String> {
    if record.overlapping {
        return Err(format!("{}: its fields share storage, and their layouts do not say which one it holds", record.name));
    }
    for field in record.fields {
        let unequal = || {
            format!(
                "{}.{}: a {:?} field of {} bytes at 32 bits and {} at 64",
                record.name, field.name, field.kind, field.at32.size, field.at64.size
            )
        };
        match field.kind {
            Kind::Record => {
                let inner =
                    layout(field.record).ok_or_else(|| format!("{}: the layout table does not have {}", record.name, field.record))?;
                for element in 0..field.count as usize {
                    let at32 = from + field.at32.offset as usize + element * inner.size32 as usize;
                    let at64 = to + field.at64.offset as usize + element * inner.size64 as usize;
                    widen(inner, guest, at32, host, at64)?;
                }
            }
            Kind::Float | Kind::Bits => {
                if field.at32.size != field.at64.size {
                    return Err(unequal());
                }
                host[span(field.at64, to)].copy_from_slice(&guest[span(field.at32, from)]);
            }
            Kind::Pointer | Kind::Signed | Kind::Unsigned => {
                let count = field.count.max(1) as usize;
                let (narrow, wide) = (field.at32.size as usize / count, field.at64.size as usize / count);
                if wide < narrow {
                    return Err(unequal());
                }
                for element in 0..count {
                    let source = &guest[from + field.at32.offset as usize + element * narrow..][..narrow];
                    let target = &mut host[to + field.at64.offset as usize + element * wide..][..wide];
                    let negative = field.kind == Kind::Signed && source.last().is_some_and(|top| top & 0x80 != 0);
                    target[..narrow].copy_from_slice(source);
                    target[narrow..].fill(if negative { 0xFF } else { 0 });
                }
            }
        }
    }
    Ok(())
}

/// The first offset where two forms of a structure differ, or their lengths.
fn compare(what: &str, expected: &[u8], actual: &[u8]) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!("{what}: {} bytes, not {}", actual.len(), expected.len()));
    }
    match expected.iter().zip(actual).position(|(e, a)| e != a) {
        Some(at) => Err(format!("{what}: byte {at} is {:#04x}, not {:#04x}", actual[at], expected[at])),
        None => Ok(()),
    }
}

/// Converts the row's structure the way it crosses the call and compares with its layouts.
fn conform(row: &Row) -> Result<(), String> {
    let record = layout(&row.name).ok_or_else(|| format!("{}: the layout table does not have it", row.name))?;
    let mut guest = vec![0; record.size32 as usize];
    fill(record, &mut guest, 0)?;
    let mut host = vec![0; record.size64 as usize];
    widen(record, &guest, 0, &mut host, 0)?;
    if row.direction.widens() {
        compare(&format!("{} widened", row.name), &host, &to_host(record, &guest)?)?;
    }
    if row.direction.narrows() {
        compare(&format!("{} narrowed", row.name), &guest, &to_guest(record, &host)?)?;
    }
    Ok(())
}

#[test]
fn the_table_names_every_structure_of_the_ignore_reasons() {
    let rows = rows();
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, NAMED, "win32_structs.toml names other structures than the ignore reasons");
    let pending = rows.iter().filter(|row| row.pending).count();
    assert_eq!(pending, PENDING, "win32_structs.toml has {pending} rows pending, not {PENDING}");
}

#[test]
fn every_row_converts_as_its_status_says() {
    let mut failures = Vec::new();
    for row in rows() {
        match (conform(&row), row.pending) {
            (Ok(()), true) => failures.push(format!("win32_structs.toml:{}: {} converts: mark it passing", row.line, row.name)),
            (Err(e), false) => failures.push(format!("win32_structs.toml:{}: {} is passing but fails: {e}", row.line, row.name)),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{} rows are not as their status says:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn the_expected_host_form_extends_each_field_by_its_kind() {
    let record = layout("MSG").expect("the layout table has MSG");
    let mut guest = vec![0; record.size32 as usize];
    guest[0..4].copy_from_slice(&0x0040_1000_u32.to_le_bytes());
    guest[8..12].copy_from_slice(&0x8000_0001_u32.to_le_bytes());
    guest[12..16].copy_from_slice(&(-2_i32).to_le_bytes());
    guest[20..24].copy_from_slice(&(-7_i32).to_le_bytes());
    let mut host = vec![0; record.size64 as usize];
    widen(record, &guest, 0, &mut host, 0).expect("MSG widens");
    assert_eq!(host[0..8], 0x0040_1000_u64.to_le_bytes(), "hwnd is zero-extended");
    assert_eq!(host[16..24], 0x8000_0001_u64.to_le_bytes(), "wParam is zero-extended");
    assert_eq!(host[24..32], (-2_i64).to_le_bytes(), "lParam is sign-extended");
    assert_eq!(host[36..40], (-7_i32).to_le_bytes(), "pt.x keeps its width");
}

#[test]
fn the_parser_refuses_a_row_it_cannot_read() {
    let row = "[[struct]]\nname = \"MSG\"\ndirection = \"both\"\nstatus = \"pending\"\n";
    assert_eq!(parse(row).map(|rows| rows.len()), Ok(1));
    assert!(parse("[[struct]]\nname = \"MSG\"\nstatus = \"pending\"\n").is_err(), "a row without a direction");
    assert!(parse("[[struct]]\nname = \"MSG\"\ndirection = \"sideways\"\nstatus = \"pending\"\n").is_err(), "an unknown direction");
    assert!(parse("name = \"MSG\"\n").is_err(), "a key outside a row");
    assert!(parse(&format!("{row}size = \"4\"\n")).is_err(), "an unknown key");
}
