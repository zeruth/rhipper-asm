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

#[test]
fn a_fixture_without_a_pdb_has_no_symbols() {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    assert!(image.debug.iter().any(|e| e.codeview.is_some()));
    assert!(ra_asm_pe::debug::pdb_beside(&image).is_none());
    assert!(image.symbols.is_empty());
}
