//! `ras` over the Linux fixture: `info`, `sections`, `symbols`, `imports`, `exports` and `functions`
//! print an ELF file as they print a PE.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello_linux64")
}

fn ras(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(fixture()).output().unwrap();
    assert!(output.status.success(), "ras {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ras_prints_the_linux_hello() {
    let info = ras(&["info"]);
    for line in ["machine        x86_64", "format         ELF64", "interpreter    /lib64/ld-linux-x86-64.so.2", "needed         libc.so.6"]
    {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info"])).unwrap();
    assert_eq!(json["format"], "elf64");
    assert_eq!(json["elf"]["type"], "executable");
    assert_eq!(json["imports"], 2);

    let sections = ras(&["sections"]);
    for name in [".text", ".plt", ".got.plt", ".eh_frame"] {
        assert!(sections.lines().any(|l| l.starts_with(name)), "no {name} in:\n{sections}");
    }

    let symbols = ras(&["symbols"]);
    for name in ["square", "_start"] {
        assert!(symbols.lines().any(|l| l.ends_with(&format!("  {name}"))), "no {name} in:\n{symbols}");
    }

    let imports = ras(&["imports"]);
    assert!(imports.starts_with("libc.so.6\n"), "{imports}");
    assert!(imports.contains("  write (hint 0)") && imports.contains("  exit (hint 0)"), "{imports}");
    assert_eq!(ras(&["exports"]).trim(), "no exports");

    let extents = ras(&["functions", "--pdata"]);
    assert!(extents.contains("2 .eh_frame functions"), "{extents}");
    // The functions by their identifiers: `_start` reads `start`, a PLT stub `thunk_<import>`.
    let functions = ras(&["functions"]);
    for name in ["square", "start", "thunk_write", "thunk_exit"] {
        assert!(functions.lines().any(|l| l.contains(&format!("  {name}"))), "no {name} in:\n{functions}");
    }
}
