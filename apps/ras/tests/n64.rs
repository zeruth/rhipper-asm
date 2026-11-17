//! `ras info` over the Nintendo 64 fixture: a cartridge ROM prints its platform, its header and how
//! its boot code loads the program.

use std::path::Path;
use std::process::Command;

fn ras(args: &[&str]) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/n64/hello.z64");
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(&path).output().unwrap();
    assert!(output.status.success(), "ras {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ras_info_names_the_platform_n64() {
    let info = ras(&["info"]);
    for line in [
        "machine        mips",
        "format         Z64",
        "platform       n64",
        "title          rhipper hello",
        "boot code      CIC none known, entry 0x80000400, elf at 0x1d00",
        "entry point    0x80000400",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info"])).unwrap();
    assert_eq!((json["platform"].as_str(), json["format"].as_str()), (Some("n64"), Some("z64")));
    assert_eq!(json["n64"]["entry"], "0x80000400");
    assert_eq!(json["n64"]["cic"], serde_json::Value::Null);
}
