//! `ras lift` over a fixture: the IR the CLI prints is the one the oracle lifts, by `--level`
//! and `--standalone`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn control64() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(control64()).output().unwrap()
}

fn ras(args: &[&str]) -> String {
    let output = run(args);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// The ABI level is standalone by default, as the oracle and the emit have it: the data sections
/// are globals of the module, and no function keeps the `%State` interface.
#[test]
fn the_abi_lift_is_standalone() {
    let text = ras(&["lift", "--level", "abi", "--quiet"]);
    assert!(text.contains("@sec_rdata = "), "no data global in:\n{}", &text[..text.len().min(4000)]);
    assert!(text.contains("define "), "no function in the IR");
    assert!(!text.contains("ptr %state)"), "a `%State` interface survived the ABI raise");
    assert_eq!(text, ras(&["lift", "--quiet"]), "the default level is not the ABI level");
}

/// The machine level addresses the mapped image unless `--standalone` asks for the globals.
#[test]
fn the_machine_lift_folds_the_data_only_when_asked() {
    let machine = ras(&["lift", "--level", "machine", "--quiet"]);
    assert!(machine.contains("ptr %state)"), "the machine IR lost its `%State` interface");
    assert!(!machine.contains("@sec_rdata = "), "the machine IR folded the data sections unasked");
    let standalone = ras(&["lift", "--level", "machine", "--standalone", "--quiet"]);
    assert!(standalone.contains("@sec_rdata = "), "`--standalone` wrote no data global");
    assert!(standalone.contains("ptr %state)"), "`--standalone` changed the level");
}

#[test]
fn an_unknown_level_is_refused_by_name() {
    let output = run(&["lift", "--level", "source", "--quiet"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("machine, promoted, abi"), "{stderr}");
}
