//! The resource directory: the string tables, dialogs, icons, manifests and raw blocks the
//! resource compiler put in `.rsrc`, as a flat list of leaves (type, name, language). A port
//! reads them through the same Win32 calls as the program, so [`res_file`] writes them back in
//! the `.res` form the linker takes, and [`Resource::file_name`] names the file each is
//! extracted to for reading.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// A resource's type or name: an ordinal, or a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ResourceId {
    Id(u32),
    Name(String),
}

impl ResourceId {
    /// The SDK's name of a well-known resource type (`RT_STRING` is `STRING`), the number else.
    pub fn type_name(&self) -> String {
        let known = match self {
            Self::Id(1) => "CURSOR",
            Self::Id(2) => "BITMAP",
            Self::Id(3) => "ICON",
            Self::Id(4) => "MENU",
            Self::Id(5) => "DIALOG",
            Self::Id(6) => "STRING",
            Self::Id(7) => "FONTDIR",
            Self::Id(8) => "FONT",
            Self::Id(9) => "ACCELERATOR",
            Self::Id(10) => "RCDATA",
            Self::Id(11) => "MESSAGETABLE",
            Self::Id(12) => "GROUP_CURSOR",
            Self::Id(14) => "GROUP_ICON",
            Self::Id(16) => "VERSION",
            Self::Id(17) => "DLGINCLUDE",
            Self::Id(19) => "PLUGPLAY",
            Self::Id(20) => "VXD",
            Self::Id(21) => "ANICURSOR",
            Self::Id(22) => "ANIICON",
            Self::Id(23) => "HTML",
            Self::Id(24) => "MANIFEST",
            Self::Id(n) => return n.to_string(),
            Self::Name(s) => return s.clone(),
        };
        known.to_owned()
    }

    /// The id as a file name part: the number, or the string with what a file name cannot hold
    /// replaced.
    fn file_part(&self) -> String {
        match self {
            Self::Id(n) => n.to_string(),
            Self::Name(s) => s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect(),
        }
    }

    /// The id as the `.res` form writes it: `0xffff` and the ordinal, or the UTF-16 string.
    fn write_res(&self, out: &mut Vec<u8>) {
        match self {
            Self::Id(n) => {
                out.extend_from_slice(&0xffffu16.to_le_bytes());
                out.extend_from_slice(&(*n as u16).to_le_bytes());
            }
            Self::Name(s) => {
                for unit in s.encode_utf16().chain(std::iter::once(0)) {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
            }
        }
    }
}

/// One leaf of the resource tree.
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub kind: ResourceId,
    pub name: ResourceId,
    pub language: u16,
    /// The virtual address of the data.
    pub va: u64,
    pub size: u32,
    pub codepage: u32,
}

impl Resource {
    /// The resource's bytes.
    pub fn data<'a>(&self, image: &'a Image) -> Result<&'a [u8]> {
        image.read(self.va, self.size as usize).with_context(|| format!("resource {}/{}", self.kind.type_name(), self.name.file_part()))
    }

    /// The file the resource is extracted to, under a `resources` directory:
    /// `RCDATA/100.bin`, `MANIFEST/1.xml`; the language joins the name when the same resource
    /// comes in several (`STRING/1-1033.bin`). A string table is written decoded instead, by
    /// [`string_table`].
    pub fn file_name(&self, all: &[Resource]) -> String {
        let extension = match self.kind {
            ResourceId::Id(24) => "xml",
            ResourceId::Id(23) => "html",
            _ => "bin",
        };
        let several = all.iter().filter(|r| r.kind == self.kind && r.name == self.name).count() > 1;
        let language = if several { format!("-{}", self.language) } else { String::new() };
        format!("{}/{}{language}.{extension}", self.kind.type_name(), self.name.file_part())
    }
}

const SUBDIRECTORY: u32 = 0x8000_0000;
const MAX_ENTRIES: u32 = 1 << 16;
/// The entries of the whole tree: directories that share their subdirectories multiply, and three
/// levels of a full directory each would be 2^48 leaves.
const MAX_TREE_ENTRIES: u32 = 1 << 18;

pub(crate) fn parse(image: &Image) -> Result<Vec<Resource>> {
    let Some((dir_va, _)) = image.directory_va(Directory::Resource) else { return Ok(Vec::new()) };
    let mut resources = Vec::new();
    let mut budget = MAX_TREE_ENTRIES;
    walk(image, dir_va, 0, 0, &mut Vec::new(), &mut resources, &mut budget)?;
    Ok(resources)
}

/// One directory of the tree: its entries are types at the root, names below, languages below
/// those, and a language entry names the data. `budget` is what is left of [`MAX_TREE_ENTRIES`].
fn walk(
    image: &Image,
    dir_va: u64,
    offset: u32,
    depth: usize,
    path: &mut Vec<ResourceId>,
    out: &mut Vec<Resource>,
    budget: &mut u32,
) -> Result<()> {
    ensure!(depth < 3, "the resource directory at offset {offset:#x} nests deeper than type, name and language");
    let table = dir_va + u64::from(offset);
    let named = u32::from(image.u16(table + 12).with_context(|| format!("resource directory at offset {offset:#x}"))?);
    let ids = u32::from(image.u16(table + 14)?);
    ensure!(named + ids <= MAX_ENTRIES, "the resource directory at offset {offset:#x} claims {} entries", named + ids);
    let Some(left) = budget.checked_sub(named + ids) else {
        bail!("the resource directory at offset {offset:#x} takes the tree past {MAX_TREE_ENTRIES} entries");
    };
    *budget = left;
    for i in 0..named + ids {
        let entry = table + 16 + u64::from(i) * 8;
        let name = image.u32(entry).with_context(|| format!("resource entry {i} at offset {offset:#x}"))?;
        let data = image.u32(entry + 4)?;
        let id = if name & SUBDIRECTORY != 0 {
            let at = dir_va + u64::from(name & !SUBDIRECTORY);
            let len = usize::from(image.u16(at).with_context(|| format!("resource name at offset {:#x}", name & !SUBDIRECTORY))?);
            let bytes = image.read(at + 2, len * 2)?;
            let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            ResourceId::Name(String::from_utf16_lossy(&units))
        } else {
            ResourceId::Id(name)
        };
        path.push(id);
        if data & SUBDIRECTORY != 0 {
            walk(image, dir_va, data & !SUBDIRECTORY, depth + 1, path, out, budget)?;
        } else if let [kind, name, language] = path.as_slice() {
            let leaf = dir_va + u64::from(data);
            let rva = image.u32(leaf).with_context(|| format!("resource data entry at offset {data:#x}"))?;
            let size = image.u32(leaf + 4)?;
            let codepage = image.u32(leaf + 8)?;
            let language = match language {
                ResourceId::Id(n) => *n as u16,
                ResourceId::Name(_) => 0,
            };
            out.push(Resource { kind: kind.clone(), name: name.clone(), language, va: image.va(rva), size, codepage });
        } else {
            path.pop();
            anyhow::bail!("resource data entry at offset {data:#x} sits at depth {}, not under a type and a name", path.len());
        }
        path.pop();
    }
    Ok(())
}

/// The string table: every string by its id, in id order. The table is stored in blocks of 16
/// (block `b` holds the ids `(b - 1) * 16` to `b * 16 - 1`), each string a length-prefixed
/// UTF-16 run; an empty slot is not a string.
pub fn string_table(image: &Image) -> Vec<(u32, String)> {
    let mut strings = Vec::new();
    for r in &image.resources {
        let (ResourceId::Id(6), ResourceId::Id(block)) = (&r.kind, &r.name) else { continue };
        // A block whose ids pass 2^32 holds no string an id can name.
        let Some(first) = block.saturating_sub(1).checked_mul(16).filter(|first| first.checked_add(15).is_some()) else { continue };
        let Ok(bytes) = r.data(image) else { continue };
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let mut at = 0usize;
        for slot in 0..16u32 {
            let Some(&len) = units.get(at) else { break };
            let len = usize::from(len);
            if len > 0 && at + 1 + len <= units.len() {
                strings.push((first + slot,String::from_utf16_lossy(&units[at + 1..at + 1 + len])));
            }
            at += 1 + len;
        }
    }
    strings.sort_by_key(|(id, _)| *id);
    strings
}

/// The resources as a `.res` file, which the linker folds into the port's own `.rsrc`: the
/// program's `FindResource` and `LoadString` then find them in the port as they did in the
/// image. Empty without resources.
pub fn res_file(image: &Image) -> Vec<u8> {
    if image.resources.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // The empty entry every .res file opens with, which marks the format.
    entry(&mut out, &ResourceId::Id(0), &ResourceId::Id(0), 0, &[]);
    for r in &image.resources {
        let Ok(data) = r.data(image) else { continue };
        entry(&mut out, &r.kind, &r.name, r.language, data);
    }
    out
}

/// One `.res` entry: the data size, the header size, the type and name, the padding to a dword,
/// the version and flags, and the data padded to a dword.
fn entry(out: &mut Vec<u8>, kind: &ResourceId, name: &ResourceId, language: u16, data: &[u8]) {
    let start = out.len();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // the header size, written below
    kind.write_res(out);
    name.write_res(out);
    while (out.len() - start) % 4 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    const MOVEABLE_PURE_DISCARDABLE: u16 = 0x1030;
    out.extend_from_slice(&MOVEABLE_PURE_DISCARDABLE.to_le_bytes());
    out.extend_from_slice(&language.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    let header = (out.len() - start) as u32;
    out[start + 4..start + 8].copy_from_slice(&header.to_le_bytes());
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
