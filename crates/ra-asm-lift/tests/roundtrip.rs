//! The parser's own oracle: every x86 fixture is lifted at the machine, promoted and ABI levels,
//! printed, parsed and printed again, and the two texts are compared. The first print is what the
//! lift writes; the second is what the reader understood of it, so a spelling the reader drops or
//! reads as something else shows as the first line where the texts part, and one it refuses shows
//! as that line of the first text.
//!
//! `crt64.exe` is left out: it carries the whole C runtime, and its three lifts alone would take
//! this test out of the fast tier.

use std::path::{Path, PathBuf};

use ra_asm_lift::RaiseLevel;

/// The x86 programs of `testdata/bin`, 32-bit and 64-bit, the two DLLs among them.
const PROGRAMS: [&str; 33] = [
    "classes32.exe",
    "classes64.exe",
    "computed32.exe",
    "computed64.exe",
    "control32.exe",
    "control64.exe",
    "control64-O1.exe",
    "dispatch32.exe",
    "dispatch64.exe",
    "exports32.dll",
    "exports64.dll",
    "exports_host32.exe",
    "exports_host64.exe",
    "extended32.exe",
    "files32.exe",
    "files64.exe",
    "float32.exe",
    "float64.exe",
    "hello32.exe",
    "hello64.exe",
    "inline32.exe",
    "inline64.exe",
    "linked32.exe",
    "linked64.exe",
    "obfusc32.exe",
    "obfusc64.exe",
    "resources32.exe",
    "resources64.exe",
    "structs32.exe",
    "structs64.exe",
    "throws64.exe",
    "tls32.exe",
    "tls64.exe",
];

const LEVELS: [(RaiseLevel, &str); 3] = [(RaiseLevel::Machine, "machine"), (RaiseLevel::Promoted, "promoted"), (RaiseLevel::Abi, "abi")];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// The text of a fixture lifted and raised to `level` as the oracle builds it: over the mapped
/// image, standalone at the ABI level, with the names of its symbols applied.
fn lifted_text(image: &ra_asm_pe::Image, analysis: &ra_asm_analysis::Analysis<'_>, level: RaiseLevel) -> String {
    let options = ra_asm_lift::LiftOptions { standalone: level == RaiseLevel::Abi, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(image, analysis, options);
    let options = lifted.raise_options(level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(analysis, Some(&mut signatures));
    lifted.module.to_ll()
}

/// What the round trip of `text` lost, in one line that starts with `test`: the line the reader
/// refused, or the first line the second print spells differently.
fn round_trip(test: &str, text: &str) -> Result<(), String> {
    let module = ra_asm_ir::parse::parse(text).map_err(|e| {
        let line = text.lines().nth(e.line.saturating_sub(1)).unwrap_or("<end>");
        format!("{test}: {e}, in {line:?}")
    })?;
    ra_asm_testing::digest::compare(test, text, &module.to_ll())
}

#[test]
fn every_fixture_survives_the_round_trip() {
    let mut programs = 0;
    let mut failures = Vec::new();
    for binary in PROGRAMS {
        let image = ra_asm_pe::load(&fixture(binary)).unwrap_or_else(|e| panic!("{binary}: {e}"));
        let analysis = ra_asm_analysis::analyze(&image);
        for (level, name) in LEVELS {
            let text = lifted_text(&image, &analysis, level);
            assert!(text.contains("\ndefine "), "{binary} ({name}): nothing was lifted");
            if let Err(line) = round_trip(&format!("{binary} ({name}): round trip"), &text) {
                failures.push(line);
            }
        }
        programs += 1;
    }
    assert!(
        failures.is_empty(),
        "{} of {} modules changed in the round trip:\n{}",
        failures.len(),
        programs * LEVELS.len(),
        failures.join("\n")
    );
    assert!(programs >= 20, "{programs} programs went through the round trip, fewer than 20");
}
