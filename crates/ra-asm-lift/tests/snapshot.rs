//! The ABI-level IR of the rhtest `eh-clang.exe` as a tracked text: the raise's output lifted
//! standalone, with the names of its map applied, printed as the oracle writes `program.ll`. The
//! file under `testdata/snapshots` is what the raise gives before clang sees it, so a reader can
//! tell a block the raise merged from one clang merged, and a change of the raise shows in its diff.
//!
//! `RA_SNAPSHOT=write` rewrites the file from the lift; a missing file is written the same way, so
//! the first run of a new snapshot records it.

use std::path::{Path, PathBuf};

use ra_asm_lift::RaiseLevel;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The text of `binary` lifted standalone and raised to the ABI level as the oracle builds it.
fn abi_text(binary: &Path) -> String {
    let image = ra_asm_pe::load(binary).unwrap_or_else(|e| panic!("{}: {e:#}", binary.display()));
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    lifted.module.to_ll()
}

#[test]
fn eh_clang_at_the_abi_level_matches_its_snapshot() {
    let text = abi_text(&root().join("testdata/rhtest/eh-clang.exe"));
    assert!(text.contains("\ndefine "), "eh-clang.exe: nothing was lifted");
    let path = root().join("testdata/snapshots/eh-clang.abi.ll");
    let write = std::env::var_os("RA_SNAPSHOT").is_some_and(|v| v == "write");
    if write || !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        eprintln!("{}: written", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n");
    if let Err(line) = ra_asm_testing::digest::compare("snapshot eh-clang.abi.ll", &committed, &text) {
        panic!("{line}\n{} differs from the lift: rerun with RA_SNAPSHOT=write to rewrite it", path.display());
    }
}
