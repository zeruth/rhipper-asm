//! `typeid` in the standalone port of a 64-bit program: the host's `__RTtypeid` takes the image's
//! base as the locator's address less its own offset and adds the type descriptor's offset to it,
//! which the port's sections, placed apart by its linker, no longer keep. The `ra_rtti` table the
//! lift writes lets the runtime rewrite every such field against the port's module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_asm_ir::Const;
use ra_asm_lift::state;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// The port's module as a linker might lay it out: each data global at a base of its own, a
/// megabyte apart, which no section of the image is from the next.
const MODULE: u64 = 0x7ff6_1000_0000;
const SPACING: u64 = 0x10_0000;

#[test]
fn typeid_of_a_standalone_x64_port_resolves_its_type_descriptor() {
    let image = ra_asm_pe::load(&fixture("classes64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    // classes64 has no C++ runtime: nothing reads its RTTI at run time, and the lift writes no table.
    assert!(lifted.module.globals.iter().all(|g| g.name != state::RTTI));
    assert!(state::rtti_global(&[&image], &lifted).is_none());

    lifted.imports.insert("__RTtypeid".to_owned());
    let table = state::rtti_global(&[&image], &lifted).expect("classes64 carries x64 RTTI");
    let Some(Const::Struct(items)) = &table.init else { panic!("the table is a structure of words: {:?}", table.init) };
    assert_eq!(items[items.len() - 2..], [Const::Int(0), Const::Int(0)], "the table ends with a zero pair");

    let host = |c: &Const| match c {
        Const::Address { global, offset } => {
            let index = lifted.data.iter().position(|d| d.global == *global).unwrap_or_else(|| panic!("{global} is no data global"));
            MODULE + SPACING * (index as u64 + 1) + *offset as u64
        }
        other => panic!("a word of the table is no address of a data global: {other:?}"),
    };
    let host_of = |va: u64| {
        let index = lifted.data.iter().position(|d| va >= d.start && va < d.end).unwrap_or_else(|| panic!("{va:#x} is in no data global"));
        MODULE + SPACING * (index as u64 + 1) + (va - lifted.data[index].start)
    };
    let image_va = |at: u64| {
        let index = ((at - MODULE) / SPACING - 1) as usize;
        lifted.data[index].start + (at - MODULE) % SPACING
    };
    // The runtime's rewrite: each field holds the offset of what it names from the module.
    let mut rewritten: HashMap<u64, u32> = HashMap::new();
    for pair in items[..items.len() - 2].chunks_exact(2) {
        rewritten.insert(host(&pair[0]), u32::try_from(host(&pair[1]) - MODULE).unwrap());
    }

    let mut locators = 0;
    for class in &analysis.classes {
        for vtable in &class.vtables {
            locators += 1;
            let locator = host_of(vtable.locator);
            let field =
                |offset: u64| *rewritten.get(&(locator + offset)).unwrap_or_else(|| panic!("{}: no field at +{offset}", class.name));
            // `__RTtypeid`: the base from the locator's own offset, the descriptor at its offset from it.
            let base = locator - u64::from(field(20));
            let descriptor = image_va(base + u64::from(field(12)));
            let mangled =
                image.printable_cstr(descriptor + 16, 5, 512).unwrap_or_else(|| panic!("{}: no name at {descriptor:#x}", class.name));
            assert_eq!(ra_asm_analysis::rtti::demangle_type(mangled).as_deref(), Some(class.name.as_str()), "{mangled}");
            assert_eq!(descriptor, class.type_descriptor, "{}", class.name);
            // The image's own offsets, read against the port's placement, name another address.
            let own = locator - u64::from(image.u32(vtable.locator + 20).unwrap()) + u64::from(image.u32(vtable.locator + 12).unwrap());
            assert_ne!(own, host_of(class.type_descriptor), "{}: the image's offsets would already hold", class.name);
        }
    }
    // Square, Named, and Rect with a vtable for each of its bases with virtual functions.
    assert_eq!(locators, 4);
}
