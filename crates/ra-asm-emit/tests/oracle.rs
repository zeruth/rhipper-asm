//! The C backend against the native programs: each fixture is lifted, raised, written as C,
//! compiled with clang against the driver and the runtime header, run, and compared. The
//! structuring must leave no `goto` in the fixtures.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn clang_available() -> bool {
    Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ra-asm-emit-tests").join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The C of a fixture, with the image header and the function table beside it. With a
/// `functions_per_file`, the output splits into a header, a data file and several function files.
fn emit(binary: &str, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect) -> (PathBuf, String) {
    let image = ra_asm_pe::load(&root().join("testdata/bin").join(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, ra_asm_lift::RaiseOptions::default());
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names, dialect });
    let files = unit.files(functions_per_file);
    let dir = scratch(&format!("{binary}-{tag}"));
    ra_asm_emit::write_files(&dir, &files).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
    std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit, &files, &infos)).unwrap();
    (dir, unit.text())
}

/// The function table names every function, and each `line` is the function's definition in its file.
fn check_table(binary: &str, dir: &Path) {
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("functions.json")).unwrap()).unwrap();
    let entries = table.as_array().unwrap();
    assert!(!entries.is_empty(), "{binary}: an empty function table");
    for e in entries {
        let addr = e["addr"].as_str().unwrap();
        assert!(addr.starts_with("0x") && addr.len() >= 10, "{binary}: address {addr}");
        assert!(!e["name"].as_str().unwrap().is_empty());
        if let Some(line) = e["line"].as_u64() {
            let file = std::fs::read_to_string(dir.join(e["file"].as_str().unwrap())).unwrap();
            let text = file.lines().nth(line as usize - 1).unwrap_or("");
            let ir_name = format!("F_{}(", addr.trim_start_matches("0x").trim_start_matches('0'));
            assert!(text.contains(&ir_name) && text.ends_with('{'), "{binary}: line {line} of {addr} is `{text}`");
            assert_eq!(e["status"], if e["traps"] == 0 { "lifted" } else { "partial" });
        } else {
            assert!(e["status"] == "thunk" || e["status"] == "not lifted", "{binary}: {e}");
        }
    }
}

fn run_oracle(binary: &str, expected_name: &str) {
    run_oracle_with(binary, expected_name, "one", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
}

fn run_oracle_with(binary: &str, expected_name: &str, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect) {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let (dir, c) = emit(binary, tag, functions_per_file, dialect);
    check_table(binary, &dir);
    assert!(!c.contains("goto "), "{binary}: the C has a goto:\n{c}");
    let sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "c" || x == "cpp"))
        .collect();
    let exe = dir.join("program.exe");
    // clang compiles each file in its own language by its extension; the driver stays C.
    let build = Command::new("clang")
        .args(["-O1", "-Wall", "-Wno-unused-variable", "-Wno-unused-but-set-variable", "-Wno-parentheses", "-Werror", "-fuse-ld=lld"])
        .args(&sources)
        .arg(root().join("runtime/driver.c"))
        .arg("-I")
        .arg(&dir)
        .arg("-I")
        .arg(root().join("runtime"))
        .arg("-o")
        .arg(&exe)
        .arg("-lkernel32")
        .output()
        .unwrap();
    assert!(build.status.success(), "{binary}: clang on {}:\n{}", dir.join("program.c").display(), String::from_utf8_lossy(&build.stderr));
    let run = Command::new(&exe).current_dir(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(root().join("testdata/expected").join(expected_name)).unwrap();
    assert_eq!(
        stdout.trim_end(),
        expected.trim_end(),
        "{binary}: the output (exit {:?}, stderr {})",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn hello64_runs_as_c() {
    run_oracle("hello64.exe", "hello.txt");
}

#[test]
fn control64_runs_as_c() {
    run_oracle("control64.exe", "control.txt");
}

#[test]
fn float64_runs_as_c() {
    run_oracle("float64.exe", "float.txt");
}

/// The split output: a header, a data file and several function files build and run together.
#[test]
fn control64_runs_as_split_c() {
    run_oracle_with("control64.exe", "control.txt", "split", 4, ra_asm_emit::Dialect::C);
}

/// The C++ dialect: the same source with C linkage, built by clang++ beside the C driver.
#[test]
fn control64_runs_as_cpp() {
    run_oracle_with("control64.exe", "control.txt", "cpp", 4, ra_asm_emit::Dialect::Cpp);
}

#[test]
fn float64_runs_as_cpp() {
    run_oracle_with("float64.exe", "float.txt", "cpp", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
}
