//! The PDB beside a fixture: the names of its functions and the libraries they came from.

use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

#[test]
fn the_crt_fixture_names_its_functions_from_its_pdb() {
    let image = ra_asm_pe::load(&fixture("crt64.exe")).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    assert!(ra_asm_pe::debug::pdb_beside(&image).is_some(), "no crt64.pdb beside the fixture");
    let symbol = |name: &str| image.symbols.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no symbol {name}"));
    // The program's own functions, the static ones included, come from the procedure records.
    assert_eq!(symbol("main").library, None);
    assert_eq!(symbol("total").library, None);
    assert_eq!(symbol("label").library, None);
    // The runtime's functions name the static library they were linked from, and lie in its ranges.
    assert_eq!(symbol("printf").library.as_deref(), Some("libucrt.lib"));
    assert_eq!(symbol("mainCRTStartup").library.as_deref(), Some("libcmt.lib"));
    assert_eq!(image.entry_point(), Some(symbol("mainCRTStartup").va));
    let range_of = |va: u64| image.libraries.iter().find(|r| va >= r.va && va < r.va + u64::from(r.size));
    assert_eq!(range_of(symbol("printf").va).map(|r| r.library.as_str()), Some("libucrt.lib"));
    assert!(range_of(symbol("main").va).is_none(), "main lies in a library's range");
    assert!(image.libraries.windows(2).all(|w| w[0].va <= w[1].va));
    // One name per address, in address order.
    assert!(image.symbols.windows(2).all(|w| w[0].va < w[1].va));
    assert!(image.symbols.iter().all(|s| image.is_code(s.va)), "a symbol outside the code");
}

/// Without a PDB, the link map beside the fixture names its functions.
#[test]
fn a_fixture_without_a_pdb_names_its_functions_from_its_map() {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    assert!(image.debug.iter().any(|e| e.codeview.is_some()));
    assert!(ra_asm_pe::debug::pdb_beside(&image).is_none());
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let entry = image.symbols.iter().find(|s| Some(s.va) == image.entry_point()).expect("the entry is named");
    assert_eq!(entry.name, "mainCRTStartup");
    assert_eq!(entry.library, None);
    assert!(image.symbols.windows(2).all(|w| w[0].va < w[1].va));
    assert!(image.symbols.iter().all(|s| image.is_code(s.va)), "a symbol outside the code");
}

/// The map of another build is refused with its path: one whose entry point lies elsewhere, one
/// whose segments outgrow the image's sections.
#[test]
fn the_map_of_another_build_is_refused() {
    let rhtest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest");
    let error = ra_asm_pe::load_with_map(&rhtest.join("core-O2.exe"), &fixture("hello32.map")).expect_err("refused");
    let text = format!("{error:#}");
    assert!(text.contains("hello32.map") && text.contains("entry point"), "{text}");
    let error = ra_asm_pe::load_with_map(&fixture("control64.exe"), &rhtest.join("core-O2.map")).expect_err("refused");
    let text = format!("{error:#}");
    assert!(text.contains("core-O2.map") && text.contains("another build"), "{text}");
}
