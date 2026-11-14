//! `ras emit --with <dll>`: the program and the DLL it imports port as one. The program's calls
//! of the DLL's exports reach the DLL's lifted functions, its read of the DLL's data export reads
//! the lifted global, and the port loads no DLL for them. The fixture is `linked` over `exports`
//! (`testdata/src/linked.c`, `testdata/src/exports.c`), whose output is `testdata/expected/linked.txt`.

use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_testing::Scratch;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ras(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap();
    assert!(output.status.success(), "ras {}:\n{}", args.join(" "), String::from_utf8_lossy(&output.stderr));
}

/// The port of `linked<bits>.exe` with `exports<bits>.dll`, written into the scratch directory
/// `name`, and its C as one text.
fn port(bits: u32, name: &str) -> (Scratch, String) {
    let dir = Scratch::new("ras-with-image-tests", name);
    let exe = root().join(format!("testdata/bin/linked{bits}.exe"));
    let dll = root().join(format!("testdata/bin/exports{bits}.dll"));
    ras(&["emit", exe.to_str().unwrap(), "--with", dll.to_str().unwrap(), "--out", dir.to_str().unwrap(), "--quiet"]);
    let mut c = String::new();
    for source in sources(&dir) {
        c.push_str(&std::fs::read_to_string(source).unwrap());
    }
    (dir, c)
}

fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> =
        std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "c")).collect();
    sources.sort();
    sources
}

/// The DLL's exports are functions of the port, called directly: no host import, no import
/// library, no wrapper is left for them. Then the port builds and prints what the native program
/// prints, so the binding is proven at run time, not by the text alone.
fn the_program_calls_the_dll_lifted_export(bits: u32) {
    let (dir, c) = port(bits, &format!("calls{bits}"));
    for export in ["ra_add", "ra_twice", "ra_counter"] {
        let declared = c.lines().any(|l| l.starts_with("extern") && l.contains(export));
        assert!(!declared, "{bits}: {export} is still a host import:\n{c}");
        assert!(!c.contains(&format!("imp_{export}")), "{bits}: {export} is still called through an import wrapper:\n{c}");
    }
    assert!(!dir.join(format!("imports/exports{bits}.def")).exists(), "{bits}: an import library of the DLL is still made");
    for function in ["ra_add", "ra_twice"] {
        // A definition starts its line; a call is indented and a prototype ends with `;`.
        let defined =
            c.lines().any(|l| !l.starts_with(char::is_whitespace) && l.contains(&format!(" {function}(")) && !l.trim_end().ends_with(';'));
        assert!(defined, "{bits}: {function} is not a function of the port:\n{c}");
        let called = c.lines().any(|l| l.starts_with(char::is_whitespace) && l.contains(&format!("{function}(")));
        assert!(called, "{bits}: nothing calls {function}:\n{c}");
    }
    // The export by ordinal only (`ra_hidden @20 NONAME`) is a root of the DLL under the name the
    // analysis gives an unnamed export, so the port carries it although the program never calls it.
    assert!(c.contains("ordinal_20("), "{bits}: the DLL's function by ordinal is not in the port:\n{c}");

    let Some(clang) = ra_asm_testing::tool("clang") else { return };
    let exe = dir.join("port.exe");
    let mut build = Command::new(clang);
    build.args(["-O1", "-g", "-gcodeview", "-Wall", "-Wextra", "-Werror", "-std=c23", "-fuse-ld=lld"]);
    if bits == 32 {
        build.arg("-Wl,/largeaddressaware:no");
    }
    build.args(sources(&dir)).arg(root().join("runtime/driver.c"));
    build.arg("-I").arg(dir.path()).arg("-I").arg(root().join("runtime")).arg("-o").arg(&exe).arg("-lkernel32");
    let built = build.output().unwrap();
    assert!(built.status.success(), "{bits}: the port does not build:\n{}", String::from_utf8_lossy(&built.stderr));
    let run = Command::new(&exe).current_dir(dir.path()).output().unwrap();
    let expected = std::fs::read_to_string(root().join("testdata/expected/linked.txt")).unwrap();
    assert_eq!(
        ra_asm_testing::digest::normalize(&String::from_utf8_lossy(&run.stdout)),
        ra_asm_testing::digest::normalize(&expected),
        "{bits}: the port prints something else; stderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.status.code(), Some(0), "{bits}: the port's exit code");
}

#[test]
fn the_64_bit_program_calls_the_dll_lifted_export() {
    the_program_calls_the_dll_lifted_export(64);
}

#[test]
fn the_32_bit_program_calls_the_dll_lifted_export() {
    the_program_calls_the_dll_lifted_export(32);
}

/// `functions.json` lists the functions of both images, each under its image, and the program's
/// entry calls the DLL's functions rather than importing them.
#[test]
fn the_function_table_names_both_images() {
    let (dir, _) = port(64, "table");
    let text = std::fs::read_to_string(dir.join("functions.json")).unwrap();
    let table: serde_json::Value = serde_json::from_str(&text).unwrap();
    let functions = table.as_array().unwrap();
    let of = |image: &str| functions.iter().filter(|f| f["image"] == image).count();
    assert!(of("linked64.exe") > 0 && of("exports64.dll") > 0, "{text}");
    assert_eq!(of("linked64.exe") + of("exports64.dll"), functions.len(), "a function names no image:\n{text}");
    let find = |name: &str| functions.iter().find(|f| f["name"] == name).unwrap_or_else(|| panic!("no {name} in {text}"));
    assert_eq!(find("ra_add")["image"], "exports64.dll");
    assert_eq!(find("ra_add")["status"], "lifted");
    // The program's startup (`mainCRTStartup` in the fixture's map; the port's name takes a `_`,
    // since the host's own runtime owns that entry name) calls the DLL's functions as functions of
    // the port, not as imports.
    let names = |f: &serde_json::Value, key: &str| -> Vec<String> {
        f[key].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    let entry = find("mainCRTStartup_");
    assert_eq!(entry["image"], "linked64.exe");
    let calls = names(entry, "calls");
    assert!(calls.iter().any(|c| c == "ra_add") && calls.iter().any(|c| c == "ra_twice"), "{entry}");
    assert!(!names(entry, "imports").iter().any(|i| i.starts_with("ra_")), "{entry}");
}
