//! `ras` over the macOS fixtures, x86-64 bound by `LC_DYLD_INFO` and arm64 by chained fixups:
//! `info`, `sections`, `symbols`, `imports`, `exports` and `functions` print a Mach-O file as they
//! print a PE.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

fn ras(name: &str, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(fixture(name)).output().unwrap();
    assert!(output.status.success(), "ras {args:?} {name}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// What both fixtures print alike; `functions` are the names `ras functions` must list.
fn check(name: &str, machine: &str, binding: &str, functions: &[&str]) {
    let info = ras(name, &["info"]);
    for line in [
        format!("machine        {machine}"),
        "format         MACHO64".to_owned(),
        "type           executable".to_owned(),
        "dylinker       /usr/lib/dyld".to_owned(),
        "dylibs         /usr/lib/libSystem.B.dylib".to_owned(),
        format!("binding        {binding}"),
    ] {
        assert!(info.contains(&line), "{name}: no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(name, &["--json", "info"])).unwrap();
    assert_eq!(json["format"], "macho64", "{name}");
    assert_eq!(json["macho"]["type"], "executable", "{name}");

    let sections = ras(name, &["sections"]);
    for section in ["__TEXT,__text", "__TEXT,__stubs", "__TEXT,__unwind_info", "__DATA_CONST,__got"] {
        assert!(sections.lines().any(|l| l.starts_with(section)), "{name}: no {section} in:\n{sections}");
    }

    // A C name reads as the source wrote it, without the underscore the compiler prefixed.
    let symbols = ras(name, &["symbols"]);
    for symbol in ["square", "main"] {
        assert!(symbols.lines().any(|l| l.ends_with(&format!("  {symbol}"))), "{name}: no {symbol} in:\n{symbols}");
    }

    let imports = ras(name, &["imports"]);
    assert!(imports.starts_with("/usr/lib/libSystem.B.dylib\n"), "{name}: {imports}");
    assert!(imports.contains("  write (hint 0)") && imports.contains("  exit (hint 0)"), "{name}: {imports}");
    let exports = ras(name, &["exports"]);
    assert!(exports.contains("square") && exports.contains("main"), "{name}: {exports}");

    // `__unwind_info` merges `square` and `main` when their encodings agree; the function starts
    // cut the range back into two.
    let extents = ras(name, &["functions", "--pdata"]);
    assert!(extents.contains("2 __unwind_info functions"), "{name}: {extents}");
    let listed = ras(name, &["functions"]);
    for function in functions {
        assert!(listed.lines().any(|l| l.contains(&format!("  {function}"))), "{name}: no {function} in:\n{listed}");
    }
}

#[test]
fn ras_prints_the_macos_x64_hello() {
    // Each x86-64 stub is one `jmp [slot]` through a lazily bound pointer, so it reads as a thunk.
    check("hello_macos_x64", "x86_64", "dyld info", &["square", "main", "thunk_write", "thunk_exit"]);
}

#[test]
fn ras_prints_the_macos_arm64_hello() {
    check("hello_macos_arm64", "arm64", "chained fixups", &["square", "main"]);
}
