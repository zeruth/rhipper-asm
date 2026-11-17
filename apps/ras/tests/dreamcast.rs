//! `ras info` over the Dreamcast fixture: a boot program prints its platform and where it maps.

use std::path::Path;
use std::process::Command;

fn ras(args: &[&str]) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/dc/1ST_READ.BIN");
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(&path).output().unwrap();
    assert!(output.status.success(), "ras {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ras_info_names_the_platform_dreamcast() {
    let info = ras(&["info"]);
    for line in [
        "machine        sh4",
        "format         BIN",
        "platform       dreamcast",
        "boot program   plain",
        "image base     0x8c010000",
        "entry point    0x8c010000",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info"])).unwrap();
    assert_eq!((json["platform"].as_str(), json["format"].as_str()), (Some("dreamcast"), Some("bin")));
    assert_eq!(json["dreamcast"]["scrambled"], false);
}
