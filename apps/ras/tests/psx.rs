//! `ras info` over the PlayStation fixture: a PS-X EXE prints its platform and its header.

use std::path::Path;
use std::process::Command;

fn ras(args: &[&str]) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psx/hello.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(&path).output().unwrap();
    assert!(output.status.success(), "ras {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ras_info_names_the_platform_psx() {
    let info = ras(&["info"]);
    for line in [
        "machine        mips",
        "format         PSX-EXE",
        "platform       psx",
        "text           0x80010000, 0x800 bytes, pc 0x80010094, gp 0x00000000, sp the BIOS's",
        "image base     0x80010000",
        "entry point    0x80010094",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info"])).unwrap();
    assert_eq!((json["platform"].as_str(), json["format"].as_str()), (Some("psx"), Some("psx-exe")));
    assert_eq!(json["psx"]["pc"], "0x80010094");
    assert_eq!(json["psx"]["text_size"], 0x800);
}
