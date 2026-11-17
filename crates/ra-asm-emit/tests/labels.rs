//! The labels of the C a fixture's lift writes, read from its text without building it: the fast
//! tier, since it needs no tool but cargo and the committed `testdata/bin` programs.

use std::path::PathBuf;

/// The C of `binary` lifted standing alone and raised to the ABI level, as one unit.
fn c_of(binary: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(binary);
    let image = ra_asm_pe::load(&path).unwrap_or_else(|error| panic!("{}: {error:#}", path.display()));
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted =
        ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    ra_asm_emit::emit_c(&lifted.module, &ra_asm_emit::COptions::default())
}

/// `scan` reaches one loop from both arms of a branch, and the loop's head tests a line break in
/// two compares, the second of which post-dominates the head and folds into its condition: the
/// way past the shared head is out of the loop, so every `goto` names a label the C writes.
#[test]
fn shared_loop_head_jumps_only_to_written_labels() {
    for binary in ["scan64.exe", "scan32.exe"] {
        let c = c_of(binary);
        let gotos: Vec<&str> = c.lines().filter_map(|l| l.trim().strip_prefix("goto ")).map(|l| l.trim_end_matches(';')).collect();
        for label in gotos {
            assert!(c.contains(&format!("{label}:;")), "{binary}: `goto {label}` names no label:\n{c}");
        }
    }
}
