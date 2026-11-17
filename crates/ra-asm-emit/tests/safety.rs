//! The safety of the Rust port, read from its text without building it: the fast tier, since it
//! needs no tool but cargo and the committed `control64.exe`.

use std::path::PathBuf;

/// The functions of `control64.exe`'s Rust port, with or without the proofs of safety.
fn control64_rust(proofs: bool) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    let image = ra_asm_pe::load(&path).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted =
        ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let options = ra_asm_emit::RustOptions { names, proofs, ..ra_asm_emit::RustOptions::default() };
    ra_asm_emit::emit_rust(&lifted.module, &options).functions.into_iter().map(|f| f.text).collect()
}

/// The Rust port is safe where the engine can prove it (task 120): the functions are safe
/// `extern "C" fn`s, a local the frame never shares is a value, an access of a data section at a
/// known offset is a checked one, and what stays raw sits in an `unsafe` block that names why.
/// The control port keeps under half the `unsafe` blocks of the raw port, where every raw
/// operation stands in its own.
#[test]
fn control64_rust_is_safe_where_provable() {
    let proven = control64_rust(true);
    let raw = control64_rust(false);
    let blocks = |text: &str| text.matches("unsafe {").count();
    assert!(blocks(&proven) * 2 < blocks(&raw), "{} unsafe blocks against {} raw:\n{proven}", blocks(&proven), blocks(&raw));
    assert!(!proven.contains("pub unsafe extern \"C\" fn"), "an unsafe function:\n{proven}");
    // `sink`, `out` and the sorted `table` are accesses of the data section, constant and indexed:
    // an index scaled by the element's size is an element of the array.
    assert!(proven.contains("sec_data.get::<u32>(64)"), "{proven}");
    assert!(proven.contains("sec_data.put::<u32>(") || proven.contains("sec_data.set::<u32>(("), "{proven}");
    // Every `unsafe` block that stays names why.
    for line in proven.lines().filter(|l| l.contains("unsafe {")) {
        assert!(line.contains("unsafe { /* ") || line.contains("} // "), "an unsafe block that names no reason: {line}");
    }
    // The vector registers of `sum_table` are a local whose address never escapes: the proven
    // port keeps them as checked bytes, where the raw port gives them storage behind a pointer.
    // The frame's address escapes to WriteFile and stays raw, named as such.
    assert!(raw.contains("let mut xmm_mem = rhipper::Aligned("), "{raw}");
    assert!(!proven.contains("xmm_mem") && proven.contains("let xmm = rhipper::Section::new("), "{proven}");
    assert!(proven.contains(&format!("// {}", "memory at a computed address")), "{proven}");
    assert!(proven.contains("unsafe { /* an import */") || proven.contains("// an import"), "{proven}");
}
