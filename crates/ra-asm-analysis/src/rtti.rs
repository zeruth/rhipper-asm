//! The classes of a program, from the RTTI the Microsoft C++ ABI leaves in the image.
//!
//! Every vtable of a class with virtual functions is preceded by a pointer to a complete object
//! locator, which names the class through its type descriptor (`.?AVShape@@`) and lists its bases
//! through the class hierarchy descriptor. The locator is found by its shape: on x64 its last field
//! is its own RVA; on x86 its type descriptor pointer leads to a name that starts with `.?A`. The
//! vtable follows the slot that points at the locator, and runs while the slots hold code addresses.
//!
//! Layouts, all fields 32 bits unless said otherwise:
//! - complete object locator: `signature` (1 on x64, 0 on x86), `offset` of this vtable's subobject
//!   in the complete object, `cd_offset`, type descriptor, class hierarchy descriptor, and on x64
//!   the locator's own RVA; the x64 fields are RVAs, the x86 fields virtual addresses.
//! - type descriptor: a pointer to `type_info`'s vtable, a pointer-sized spare, the mangled name.
//! - class hierarchy descriptor: `signature`, `attributes`, the count of bases, the base class array:
//!   the class itself, then its bases depth first, so the direct bases are the entries at the top
//!   level of that walk.
//! - base class descriptor: type descriptor, the count of contained bases (the entries of the
//!   array under this one), the displacement (`mdisp`, `pdisp`, `vdisp`: the subobject's offset,
//!   and the vbtable's offset or -1 when no virtual base lies on the way), `attributes`, class
//!   hierarchy descriptor.

use ra_asm_pe::Image;
use serde::Serialize;

/// A class with virtual functions, or a base of one.
#[derive(Debug, Clone, Serialize)]
pub struct Class {
    /// The class name as the source wrote it: `Shape`, `ns::Inner`.
    pub name: String,
    pub type_descriptor: u64,
    /// The direct and indirect bases, as the hierarchy lists them, without the class itself.
    pub bases: Vec<String>,
    /// The direct bases alone, in declaration order.
    pub direct_bases: Vec<String>,
    /// The direct bases inherited `virtual`, which the object reaches through its vbtable.
    pub virtual_bases: Vec<String>,
    /// The offset of each base's subobject in the object, in the order of `bases`: none for a
    /// base inside a virtual one, whose place the vbtable gives at run time.
    pub base_offsets: Vec<Option<u32>>,
    /// The vtables of the class, by the offset of their subobject: one for a single-inheritance
    /// class, one per base with virtual functions for a multiple-inheritance class. None for a
    /// base nothing instantiates, which the image knows only through the classes derived from it.
    pub vtables: Vec<Vtable>,
}

/// One vtable: the address of its first slot and the functions in its slots.
#[derive(Debug, Clone, Serialize)]
pub struct Vtable {
    pub address: u64,
    /// The offset of the subobject this vtable serves, in the complete object.
    pub offset: u32,
    /// The base class whose interface this vtable implements, when it is not the class itself.
    pub base: Option<String>,
    pub locator: u64,
    pub functions: Vec<u64>,
}

impl Class {
    /// The name of the virtual function in `slot` of the vtable at `offset`, when nothing better
    /// names it: `Shape_vf2`, or `Rect_Named_vf0` for a vtable that implements a base's interface.
    pub fn slot_name(&self, vtable: &Vtable, slot: usize) -> String {
        let mut name = identifier(&self.name);
        if let Some(base) = &vtable.base {
            name.push('_');
            name.push_str(&identifier(base));
        }
        format!("{name}_vf{slot}")
    }
}

/// Every vtable of `classes`, in the order of the classes.
pub fn vtables(classes: &[Class]) -> Vec<Vtable> {
    classes.iter().flat_map(|c| c.vtables.iter().cloned()).collect()
}

/// The functions a call through `slot` can reach: the one in that slot of each vtable of
/// `vtables` that has it, each once, in address order. A call whose receiver's class is unknown
/// reaches any of them; with the receiver's vtable known, pass that one alone.
pub fn slot_functions(vtables: &[Vtable], slot: usize) -> Vec<u64> {
    let mut functions: Vec<u64> = vtables.iter().filter_map(|v| v.functions.get(slot).copied()).collect();
    functions.sort_unstable();
    functions.dedup();
    functions
}

/// A class name as a C identifier: `ns::Inner` is `ns_Inner`.
pub fn identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

/// Every class with a vtable in `image`, and every base of one that has no vtable of its own, in
/// the order of their type descriptors.
pub fn classes(image: &Image) -> Vec<Class> {
    let locators = find_locators(image);
    let mut classes: Vec<Class> = Vec::new();
    let mut hierarchies: Vec<Vec<Base>> = Vec::new();
    for l in locators {
        let Some(name) = type_name(image, l.type_descriptor) else { continue };
        let Some(vtable) = find_vtable(image, l.address) else { continue };
        let base = (l.offset != 0).then(|| base_at_offset(image, l.hierarchy, l.offset)).flatten().filter(|b| *b != name);
        let vtable = Vtable { address: vtable.0, offset: l.offset, base, locator: l.address, functions: vtable.1 };
        if let Some(class) = classes.iter_mut().find(|c| c.type_descriptor == l.type_descriptor) {
            class.vtables.push(vtable);
            continue;
        }
        let hierarchy = hierarchy(image, l.hierarchy);
        let (bases, base_offsets) = bases_of(&hierarchy, 0);
        let (direct_bases, virtual_bases) = direct_bases(&hierarchy, 0);
        classes.push(Class {
            name,
            type_descriptor: l.type_descriptor,
            bases,
            direct_bases,
            virtual_bases,
            base_offsets,
            vtables: vec![vtable],
        });
        hierarchies.push(hierarchy);
    }
    // A base nothing instantiates has no vtable, so no locator names it: the hierarchies of the
    // classes derived from it carry its name, its type descriptor and its own bases.
    for hierarchy in &hierarchies {
        for (index, base) in hierarchy.iter().enumerate().skip(1) {
            if classes.iter().any(|c| c.type_descriptor == base.type_descriptor) {
                continue;
            }
            let (bases, base_offsets) = bases_of(hierarchy, index);
            let (direct_bases, virtual_bases) = direct_bases(hierarchy, index);
            classes.push(Class {
                name: base.name.clone(),
                type_descriptor: base.type_descriptor,
                bases,
                direct_bases,
                virtual_bases,
                base_offsets,
                vtables: Vec::new(),
            });
        }
    }
    for class in &mut classes {
        class.vtables.sort_by_key(|v| v.offset);
    }
    classes.sort_by_key(|c| c.type_descriptor);
    classes
}

/// One entry of a class hierarchy descriptor's base class array.
struct Base {
    name: String,
    type_descriptor: u64,
    /// The entries under this one: its own direct and indirect bases.
    contained: u32,
    /// The offset of the subobject in the complete object (`mdisp`), past the virtual base when
    /// `pdisp` is not -1: then the vbtable at `pdisp` gives where that base lies.
    mdisp: u32,
    pdisp: i32,
}

impl Base {
    /// The subobject's offset in the complete object, when no vbtable decides it.
    fn offset(&self) -> Option<u32> {
        (self.pdisp == -1).then_some(self.mdisp)
    }
}

/// The base class array of the hierarchy at `va`: the class itself first, then its bases depth
/// first, up to the first entry that does not read as a class.
fn hierarchy(image: &Image, va: u64) -> Vec<Base> {
    base_descriptors(image, va)
        .into_iter()
        .map_while(|bcd| {
            let type_descriptor = field_address(image, bcd)?;
            let name = type_name(image, type_descriptor)?;
            let contained = image.u32(bcd + 4).ok()?;
            let mdisp = image.u32(bcd + 8).ok()?;
            let pdisp = image.u32(bcd + 12).ok()? as i32;
            Some(Base { name, type_descriptor, contained, mdisp, pdisp })
        })
        .collect()
}

/// The bases of the entry at `index`, direct and indirect, with the offset of each in the
/// entry's object.
fn bases_of(hierarchy: &[Base], index: usize) -> (Vec<String>, Vec<Option<u32>>) {
    let Some(entry) = hierarchy.get(index) else { return (Vec::new(), Vec::new()) };
    let end = (index + 1 + entry.contained as usize).min(hierarchy.len());
    let under = &hierarchy[index + 1..end];
    let offsets = under.iter().map(|b| b.offset().zip(entry.offset()).and_then(|(b, e)| b.checked_sub(e))).collect();
    (under.iter().map(|b| b.name.clone()).collect(), offsets)
}

/// The direct bases of the entry at `index`: the entries under it, each stepped over with the
/// entries under it in turn; and of those, the ones it inherits `virtual`, which a vbtable
/// reaches from an entry that has a place of its own.
fn direct_bases(hierarchy: &[Base], index: usize) -> (Vec<String>, Vec<String>) {
    let Some(entry) = hierarchy.get(index) else { return (Vec::new(), Vec::new()) };
    let end = (index + 1 + entry.contained as usize).min(hierarchy.len());
    let (mut direct, mut virtual_bases) = (Vec::new(), Vec::new());
    let mut i = index + 1;
    while i < end {
        let base = &hierarchy[i];
        direct.push(base.name.clone());
        if base.pdisp != -1 && entry.pdisp == -1 {
            virtual_bases.push(base.name.clone());
        }
        i += 1 + base.contained as usize;
    }
    (direct, virtual_bases)
}

struct Locator {
    address: u64,
    offset: u32,
    type_descriptor: u64,
    hierarchy: u64,
}

/// A 32-bit field of an RTTI structure as an address: an RVA on x64, a virtual address on x86.
fn field_address(image: &Image, va: u64) -> Option<u64> {
    let raw = image.u32(va).ok()?;
    let address = if image.is_64() { image.va(raw) } else { u64::from(raw) };
    image.is_mapped(address).then_some(address)
}

/// The complete object locators of the image, by their shape.
fn find_locators(image: &Image) -> Vec<Locator> {
    let mut out = Vec::new();
    let expected_signature = u32::from(image.is_64());
    for section in &image.sections {
        if section.is_code() || section.is_executable() || section.is_uninitialized() || !section.is_readable() {
            continue;
        }
        let start = image.va(section.virtual_address);
        let Ok(bytes) = image.read_to_section_end(start) else { continue };
        let size = if image.is_64() { 24 } else { 20 };
        let mut offset = 0usize;
        while offset + size <= bytes.len() {
            let va = start + offset as u64;
            offset += 4;
            let at = |i: usize| u32::from_le_bytes(bytes[offset - 4 + i..offset + i].try_into().unwrap());
            if at(0) != expected_signature {
                continue;
            }
            if image.is_64() && image.rva(va) != Some(at(20)) {
                continue;
            }
            let Some(type_descriptor) = field_address(image, va + 12) else { continue };
            let Some(hierarchy) = field_address(image, va + 16) else { continue };
            if type_name(image, type_descriptor).is_none() {
                continue;
            }
            out.push(Locator { address: va, offset: at(4), type_descriptor, hierarchy });
        }
    }
    out
}

/// The vtable whose locator slot holds `locator`: its first slot and its functions.
fn find_vtable(image: &Image, locator: u64) -> Option<(u64, Vec<u64>)> {
    let ptr = u64::from(image.pointer_size());
    for section in &image.sections {
        if section.is_code() || section.is_executable() || section.is_uninitialized() || !section.is_readable() {
            continue;
        }
        let start = image.va(section.virtual_address);
        let Ok(bytes) = image.read_to_section_end(start) else { continue };
        for (i, chunk) in bytes.chunks_exact(ptr as usize).enumerate() {
            let value = if ptr == 8 {
                u64::from_le_bytes(chunk.try_into().unwrap())
            } else {
                u64::from(u32::from_le_bytes(chunk.try_into().unwrap()))
            };
            if value != locator {
                continue;
            }
            let first = start + (i as u64 + 1) * ptr;
            let mut functions = Vec::new();
            let mut slot = first;
            while let Ok(target) = image.pointer(slot) {
                if !image.is_code(target) {
                    break;
                }
                functions.push(target);
                slot += ptr;
            }
            if !functions.is_empty() {
                return Some((first, functions));
            }
        }
    }
    None
}

/// The class name of the type descriptor at `va`, when its mangled name is a class or struct name.
fn type_name(image: &Image, va: u64) -> Option<String> {
    let name_at = va + 2 * u64::from(image.pointer_size());
    let mangled = image.printable_cstr(name_at, 5, 512)?;
    demangle_type(mangled)
}

/// `.?AVShape@@` is `Shape`; `.?AUInner@Outer@@` is `Outer::Inner`. A name with template arguments
/// or anything else beyond nested names keeps its mangled form after the prefix.
pub fn demangle_type(mangled: &str) -> Option<String> {
    let rest = mangled.strip_prefix(".?AV").or_else(|| mangled.strip_prefix(".?AU"))?;
    let rest = rest.strip_suffix("@@")?;
    if rest.is_empty() || rest.contains('?') {
        return Some(rest.to_owned());
    }
    let parts: Vec<&str> = rest.split('@').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Some(rest.to_owned());
    }
    Some(parts.iter().rev().copied().collect::<Vec<_>>().join("::"))
}

/// The base class descriptors of the hierarchy at `va`: the class itself first, then its bases.
fn base_descriptors(image: &Image, hierarchy: u64) -> Vec<u64> {
    let Ok(count) = image.u32(hierarchy + 8) else { return Vec::new() };
    let Some(array) = field_address(image, hierarchy + 12) else { return Vec::new() };
    (0..count.min(64)).filter_map(|i| field_address(image, array + u64::from(i) * 4)).collect()
}

/// The base whose subobject lies at `offset` in the complete object.
fn base_at_offset(image: &Image, hierarchy: u64, offset: u32) -> Option<String> {
    base_descriptors(image, hierarchy).into_iter().skip(1).find_map(|bcd| {
        let mdisp = image.u32(bcd + 8).ok()?;
        (mdisp == offset).then(|| type_name(image, field_address(image, bcd)?)).flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangled_type_names_read_as_source_names() {
        assert_eq!(demangle_type(".?AVShape@@").as_deref(), Some("Shape"));
        assert_eq!(demangle_type(".?AUInner@Outer@@").as_deref(), Some("Outer::Inner"));
        assert_eq!(demangle_type(".?AVexception@std@@").as_deref(), Some("std::exception"));
        assert_eq!(demangle_type(".?AV?$basic_string@D@std@@").as_deref(), Some("?$basic_string@D@std"));
        assert_eq!(demangle_type(".?AW4Color@@"), None);
        assert_eq!(identifier("std::exception"), "std_exception");
    }
}
