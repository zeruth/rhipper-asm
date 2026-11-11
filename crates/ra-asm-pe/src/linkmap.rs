//! The link map of a build (`link /MAP`, `lld-link /map`): the names of its functions, for a
//! build that kept no PDB.
//!
//! Both linkers write the same text: a preamble with the preferred load address, the segments
//! (`0001:00000000 00002a71H .text$mn CODE`), the publics by value (`0001:000012e0 _rhtest_run
//! 004022e0 f core.obj`), the entry point (`entry point at 0001:00002310`) and the static symbols
//! in the same form. A segment is a one-based index into the image's section table, as in a PDB,
//! so the map of another build shows: a segment longer than the image's section, or an entry point
//! elsewhere. MSVC's `link` flags a function with `f`; lld flags nothing, so there every code
//! symbol counts on x64 and every decorated one (`_name`, `@name@4`, `?name@@...`) on x86, where
//! the labels the assembler-written runtime objects leak (`L1`, `cs10`, `$ehgcr_4_31`) carry no
//! decoration. The import thunks (`kernel32:KERNEL32.dll`) are the linker's, not the program's;
//! the exception funclets (`__catch$f$0`, `?dtor$3@?0?f@4HA`) are pieces of their function.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::image::Image;
use crate::pdb::Symbol;

/// The link map beside the image: `<exe>.map` with the image's own stem.
pub fn map_beside(image: &Image) -> Option<PathBuf> {
    let path = image.path.as_ref()?.with_extension("map");
    path.is_file().then_some(path)
}

/// The functions of `image` that the map at `path` names, in address order, one per address.
/// A map that disagrees with the image is an error that names the map.
pub fn read(image: &Image, path: &Path) -> Result<Vec<Symbol>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse(image, &text).with_context(|| format!("the link map {}", path.display()))
}

fn parse(image: &Image, text: &str) -> Result<Vec<Symbol>> {
    let msvc = text.lines().any(|l| l.split_whitespace().nth(3) == Some("f"));
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut entry: Option<u64> = None;
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let ["entry", "point", "at", at] = parts[..] {
            entry = parse_address(image, at)?;
            continue;
        }
        if parts.len() < 4 || parts[0].len() != 13 || parts[0].as_bytes()[4] != b':' {
            continue;
        }
        if let Some(length) = parts[1].strip_suffix('H') {
            check_segment(image, parts[0], length, parts[2])?;
            continue;
        }
        let Some(va) = parse_address(image, parts[0])? else { continue };
        let symbol = parts[1];
        let object = *parts.last().unwrap();
        let (library, file) = object.split_once(':').map_or((None, object), |(l, f)| (Some(l), f));
        let thunk = Path::new(file).extension().is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        let function = if msvc { parts[3] == "f" } else { image.is_64() || symbol.starts_with(['_', '@', '?']) };
        let funclet = ["__catch$", "__unwindfunclet$", "?catch$", "?dtor$"].iter().any(|p| symbol.starts_with(p));
        if thunk || !function || funclet || !image.is_code(va) {
            continue;
        }
        symbols.push(Symbol { va, name: symbol.to_owned(), library: library.map(|l| format!("{}.lib", l.to_ascii_lowercase())) });
    }
    if entry.is_some() {
        ensure!(
            entry == image.entry_point(),
            "the map is of another build: its entry point is {}, the image's {}",
            describe(entry),
            describe(image.entry_point())
        );
    }
    symbols.sort_by_key(|s| s.va);
    symbols.dedup_by_key(|s| s.va);
    Ok(symbols)
}

/// A map segment (`0001:00000000 000005f0H .text`) must fit the image's section: a longer one is
/// the map of another build.
fn check_segment(image: &Image, start: &str, length: &str, name: &str) -> Result<()> {
    let (segment, offset) = split_address(start)?;
    if segment == 0 {
        return Ok(());
    }
    let length = u32::from_str_radix(length, 16).with_context(|| format!("the length of segment {name}"))?;
    let Some(section) = image.sections.get(usize::from(segment) - 1) else {
        bail!("the map is of another build: it has a segment {segment:04x} ({name}), the image {} sections", image.sections.len());
    };
    let end = u64::from(offset) + u64::from(length);
    ensure!(
        end <= u64::from(section.mapped_size()),
        "the map is of another build: its segment {segment:04x} ({name}) ends at {end:#x}, the image's section {} at {:#x}",
        section.name,
        section.mapped_size()
    );
    Ok(())
}

/// The VA of a `segment:offset`, or `None` for an absolute symbol (segment 0). A segment the image
/// has no section for is the map of another build.
fn parse_address(image: &Image, text: &str) -> Result<Option<u64>> {
    let (segment, offset) = split_address(text)?;
    if segment == 0 {
        return Ok(None);
    }
    let Some(section) = image.sections.get(usize::from(segment) - 1) else {
        bail!("the map is of another build: it names segment {segment:04x}, the image has {} sections", image.sections.len());
    };
    Ok(Some(image.va(section.virtual_address) + u64::from(offset)))
}

fn split_address(text: &str) -> Result<(u16, u32)> {
    let Some((segment, offset)) = text.split_once(':') else { bail!("`{text}` is not a segment:offset") };
    let segment = u16::from_str_radix(segment, 16).with_context(|| format!("the segment of `{text}`"))?;
    let offset = u32::from_str_radix(offset, 16).with_context(|| format!("the offset of `{text}`"))?;
    Ok((segment, offset))
}

fn describe(va: Option<u64>) -> String {
    va.map_or_else(|| "none".to_owned(), |va| format!("{va:#x}"))
}
