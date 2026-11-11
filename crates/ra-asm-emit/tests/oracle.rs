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

/// A program and what it prints, as paths under `testdata`: the clang fixtures sit in `bin/` with
/// their output in `expected/`, the old pipeline's MSVC test programs in `rhtest/` beside theirs.
/// An rhtest reference ends with `exit = N`, the native run's exit code, and the port's own exit
/// code is compared with it.
struct Fixture {
    binary: String,
    expected: String,
    exit_line: bool,
    /// The structuring must leave no `goto` in the clang fixtures; MSVC's optimized control flow
    /// and its C runtime keep a few, and an rhtest program is held to its output alone.
    no_goto: bool,
}

fn fixture(binary: &str, expected: &str) -> Fixture {
    Fixture { binary: format!("bin/{binary}"), expected: format!("expected/{expected}"), exit_line: false, no_goto: true }
}

impl Fixture {
    fn name(&self) -> &str {
        Path::new(&self.binary).file_name().unwrap().to_str().unwrap()
    }

    fn binary_path(&self) -> PathBuf {
        root().join("testdata").join(&self.binary)
    }

    /// The port's output against the reference: a mismatch names the run's exit code and stderr,
    /// and a crash the function and line of the host address, when `exe` carries debug
    /// information and llvm-symbolizer is on the PATH.
    fn compare(&self, run: &std::process::Output, exe: &Path) {
        let mut stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
        if self.exit_line {
            stdout.push_str("exit = ");
            stdout.push_str(&run.status.code().unwrap_or(-1).to_string());
            stdout.push('\n');
        }
        let expected = std::fs::read_to_string(root().join("testdata").join(&self.expected)).unwrap().replace("\r\n", "\n");
        let stderr = String::from_utf8_lossy(&run.stderr);
        if stdout.trim_end() != expected.trim_end() {
            let (got, want): (Vec<&str>, Vec<&str>) = (stdout.lines().collect(), expected.lines().collect());
            let first = (0..got.len().max(want.len())).find(|&n| got.get(n) != want.get(n)).unwrap_or(0);
            panic!(
                "{}: the output differs at line {} (`{}` for `{}`; exit {:?}, stderr {}{})",
                self.binary,
                first + 1,
                got.get(first).unwrap_or(&"<end>"),
                want.get(first).unwrap_or(&"<end>"),
                run.status.code(),
                stderr,
                symbolize(&stderr, exe).unwrap_or_default()
            );
        }
    }
}

/// The crash site named in a driver's `module+<offset>` report, as llvm-symbolizer reads it from
/// the port's debug information.
fn symbolize(stderr: &str, exe: &Path) -> Option<String> {
    let offset = stderr.split("module+").nth(1)?.split(')').next()?;
    let address = 0x1_4000_0000u64 + u64::from_str_radix(offset.trim_start_matches("0x"), 16).ok()?;
    let out = Command::new("llvm-symbolizer").arg(format!("--obj={}", exe.display())).arg(format!("{address:#x}")).output().ok()?;
    Some(format!("\nat {}", String::from_utf8_lossy(&out.stdout).trim()))
}

/// The C of a fixture, with the image header and the function table beside it, and the pointer
/// width of the program. With a `functions_per_file`, the output splits into a header, a data
/// file and several function files.
fn emit(fixture: &Fixture, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect) -> (PathBuf, String, u32) {
    let binary = fixture.name();
    let image = ra_asm_pe::load(&fixture.binary_path()).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names, dialect });
    let files = unit.files(functions_per_file);
    let dir = scratch(&format!("{binary}-{tag}"));
    ra_asm_emit::write_files(&dir, &files).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
    std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit.functions, &files, &infos)).unwrap();
    (dir, unit.text(), u32::from(image.pointer_size()) * 8)
}

fn cargo_available() -> bool {
    Command::new("cargo").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The Rust package of a fixture, built by cargo and run; its output must match the reference's.
fn run_rust_oracle(binary: &str, expected_name: &str, functions_per_file: usize) {
    run_rust_oracle_with(&fixture(binary, expected_name), functions_per_file);
}

fn run_rust_oracle_with(fixture: &Fixture, functions_per_file: usize) {
    if !cargo_available() {
        eprintln!("cargo is not on the PATH: the Rust oracle is skipped");
        return;
    }
    let binary = fixture.name();
    let image = ra_asm_pe::load(&fixture.binary_path()).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let imports = image.imports.iter().filter_map(|i| i.name.clone().map(|n| (n, i.dll.clone()))).collect();
    let entry = image.entry_point().map(ra_asm_lift::function_name);
    let package = binary.trim_end_matches(".exe").to_owned();
    let pointer_bits = u32::from(image.pointer_size()) * 8;
    let options = ra_asm_emit::RustOptions { names, imports, entry, package: package.clone(), functions_per_file, pointer_bits };
    let output = ra_asm_emit::emit_rust(&lifted.module, &options);
    let dir = scratch(&format!("{binary}-rust-{functions_per_file}"));
    ra_asm_emit::write_files(&dir, &output.files).unwrap();
    std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&output.functions, &output.files, &infos)).unwrap();
    check_table(binary, &dir);
    let target = dir.join("target");
    let build = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .output()
        .unwrap();
    assert!(build.status.success(), "{binary}: cargo build in {}:\n{}", dir.display(), String::from_utf8_lossy(&build.stderr));
    let exe = target.join("release").join(format!("{package}.exe"));
    let run = Command::new(&exe).current_dir(&dir).output().unwrap();
    fixture.compare(&run, &exe);
}

#[test]
fn hello64_runs_as_rust() {
    run_rust_oracle("hello64.exe", "hello.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn control64_runs_as_rust() {
    run_rust_oracle("control64.exe", "control.txt", 4);
}

#[test]
fn float64_runs_as_rust() {
    run_rust_oracle("float64.exe", "float.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
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
            let ir_name = format!("F_{:08x}(", u64::from_str_radix(addr.trim_start_matches("0x"), 16).unwrap());
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
    run_fixture_as_c(&fixture(binary, expected_name), tag, functions_per_file, dialect);
}

fn run_fixture_as_c(fixture: &Fixture, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect) {
    run_fixture_as_c_with(fixture, tag, functions_per_file, dialect, false);
}

/// With `trace`, the port names every lifted function it enters on stderr (`RA_TRACE`): the
/// ignored `_traced` tests, for a port that crashes in the clang-less shell of a worker.
fn run_fixture_as_c_with(fixture: &Fixture, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect, trace: bool) {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let binary = fixture.name();
    let (dir, c, bits) = emit(fixture, tag, functions_per_file, dialect);
    check_table(binary, &dir);
    assert!(!fixture.no_goto || !c.contains("goto "), "{binary}: the C has a goto:\n{c}");
    let sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "c" || x == "cpp"))
        .collect();
    let exe = dir.join("program.exe");
    // clang compiles each file in its own language by its extension; the driver stays C. A
    // 32-bit program's port keeps its 32-bit pointers: the binary is not large-address-aware.
    // The debug information lets a crash report name its function and line.
    let mut build = Command::new("clang");
    build.args(["-O1", "-g", "-gcodeview", "-Wall", "-Wno-unused-variable", "-Wno-unused-but-set-variable", "-Wno-parentheses", "-Werror"]);
    build.arg("-fuse-ld=lld");
    if bits == 32 {
        build.arg("-Wl,/largeaddressaware:no");
    }
    if trace {
        build.arg("-DRA_TRACE");
    }
    let build = build
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
    fixture.compare(&run, &exe);
}

/// An rhtest program, MSVC-built for x86 with its static C runtime, as `<name>-<variant>.exe`;
/// the port prints `<name>.txt` and exits with its last line's code.
fn rhtest(name: &str, variant: &str) -> Fixture {
    Fixture { binary: format!("rhtest/{name}-{variant}.exe"), expected: format!("rhtest/{name}.txt"), exit_line: true, no_goto: false }
}

fn run_rhtest_as_c(name: &str, variant: &str) {
    run_fixture_as_c(&rhtest(name, variant), "one", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
}

fn run_rhtest_as_rust(name: &str, variant: &str) {
    run_rust_oracle_with(&rhtest(name, variant), ra_asm_emit::FUNCTIONS_PER_FILE);
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
fn classes64_runs_as_c() {
    run_oracle("classes64.exe", "classes.txt");
}

#[test]
fn classes64_runs_as_rust() {
    run_rust_oracle("classes64.exe", "classes.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

/// The 32-bit fixtures, as C and as Rust: the port runs as a 64-bit program that keeps the
/// program's 32-bit pointers.
#[test]
fn hello32_runs_as_c() {
    run_oracle("hello32.exe", "hello.txt");
}

#[test]
fn control32_runs_as_c() {
    run_oracle("control32.exe", "control.txt");
}

#[test]
fn float32_runs_as_c() {
    run_oracle("float32.exe", "float.txt");
}

#[test]
fn classes32_runs_as_c() {
    run_oracle("classes32.exe", "classes.txt");
}

#[test]
fn control32_runs_as_rust() {
    run_rust_oracle("control32.exe", "control.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn float32_runs_as_rust() {
    run_rust_oracle("float32.exe", "float.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn classes32_runs_as_rust() {
    run_rust_oracle("classes32.exe", "classes.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

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

/// The rhtest programs: the MSVC C runtime, statically linked, runs lifted around each one, at
/// /O2 and at /Od.
#[test]
fn rhtest_core_o2_runs_as_c() {
    run_rhtest_as_c("core", "O2");
}

#[test]
fn rhtest_core_od_runs_as_c() {
    run_rhtest_as_c("core", "Od");
}

#[test]
#[ignore = "a debugging aid: the traced port of core-O2, run by name"]
fn rhtest_core_o2_traced() {
    run_fixture_as_c_with(&rhtest("core", "O2"), "traced", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C, true);
}

#[test]
#[ignore = "a debugging aid: the traced port of core-Od, run by name"]
fn rhtest_core_od_traced() {
    run_fixture_as_c_with(&rhtest("core", "Od"), "traced", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C, true);
}

#[test]
fn rhtest_core_o2_runs_as_rust() {
    run_rhtest_as_rust("core", "O2");
}

#[test]
fn rhtest_core_od_runs_as_rust() {
    run_rhtest_as_rust("core", "Od");
}
