//! The C backend against the native programs: each fixture is lifted, raised, written as C,
//! compiled with clang against the driver and the runtime header, run, and compared. The
//! structuring must leave no `goto` in the fixtures.

use std::fmt::Write;
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
    /// The x87 unit the lift models: the exact one for a program built for the x87 alone, whose
    /// bits depend on the extended precision and the precision control.
    x87: ra_asm_lift::X87Unit,
}

fn fixture(binary: &str, expected: &str) -> Fixture {
    Fixture {
        binary: format!("bin/{binary}"),
        expected: format!("expected/{expected}"),
        exit_line: false,
        no_goto: true,
        x87: ra_asm_lift::X87Unit::Double,
    }
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
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: fixture.x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let classes = ra_asm_emit::classes(&analysis, &signatures);
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names, dialect, classes });
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
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: fixture.x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let imports = image.imports.iter().filter_map(|i| i.name.clone().map(|n| (n, i.dll.clone()))).collect();
    let entry = lifted.entry.and_then(|e| lifted.functions.get(&e).cloned());
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

/// The function table names every function, and each `line` is the function's definition, under
/// its name, in its file.
fn check_table(binary: &str, dir: &Path) {
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("functions.json")).unwrap()).unwrap();
    let entries = table.as_array().unwrap();
    assert!(!entries.is_empty(), "{binary}: an empty function table");
    for e in entries {
        let addr = e["addr"].as_str().unwrap();
        assert!(addr.starts_with("0x") && addr.len() >= 10, "{binary}: address {addr}");
        let name = e["name"].as_str().unwrap();
        assert!(!name.is_empty());
        if let Some(line) = e["line"].as_u64() {
            let file = std::fs::read_to_string(dir.join(e["file"].as_str().unwrap())).unwrap();
            let text = file.lines().nth(line as usize - 1).unwrap_or("");
            // A C++ method is defined as `Class::method(`, under the line of record.
            let defines = text.contains(&format!("{name}(")) || (e.get("class").is_some() && text.contains("::"));
            assert!(defines && text.ends_with('{'), "{binary}: line {line} of {addr} is `{text}`");
            assert_eq!(e["status"], if e["traps"] == 0 { "lifted" } else { "partial" });
            assert!(e["copies"].is_u64(), "{binary}: {e}");
        } else {
            assert!(e["status"] == "thunk" || e["status"] == "not lifted" || e["status"] == "runtime", "{binary}: {e}");
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
        // The system libraries the fixtures import from; an unused one costs nothing.
        .args(["-lkernel32", "-luser32", "-lshell32", "-lws2_32", "-lole32", "-loleaut32", "-ladvapi32"])
        .output()
        .unwrap();
    assert!(build.status.success(), "{binary}: clang on {}:\n{}", dir.join("program.c").display(), String::from_utf8_lossy(&build.stderr));
    let run = Command::new(&exe).current_dir(&dir).output().unwrap();
    fixture.compare(&run, &exe);
}

/// An rhtest program, MSVC-built for x86 with its static C runtime, as `<name>-<variant>.exe`;
/// the port prints `<name>.txt` and exits with its last line's code.
/// An rhtest program's variant: its reference is the program's (`<name>.txt`), unless the variant
/// prints its own (`<name>-<variant>.txt`: an x87 build whose extended precision shows in the
/// bits). The `ia32` variants, built for the x87 alone, are lifted with the exact unit.
fn rhtest(name: &str, variant: &str) -> Fixture {
    let own = format!("rhtest/{name}-{variant}.txt");
    let expected = if root().join("testdata").join(&own).is_file() { own } else { format!("rhtest/{name}.txt") };
    let x87 = if variant.starts_with("ia32") { ra_asm_lift::X87Unit::Exact } else { ra_asm_lift::X87Unit::Double };
    Fixture { binary: format!("rhtest/{name}-{variant}.exe"), expected, exit_line: true, no_goto: false, x87 }
}

/// The native output of an rhtest binary in the form `compare` reads: the output, then `exit = N`
/// (a 32-bit Windows program runs on this host as it is).
fn native_output(path: &Path) -> String {
    let run = Command::new(path).current_dir(path.parent().unwrap()).output().unwrap();
    let mut text = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let _ = writeln!(text, "exit = {}", run.status.code().unwrap_or(-1));
    text
}

/// A maintenance aid, run by name: the reference of an rhtest program that has none yet, from the
/// native run of one of its variants, and the reference of a variant whose native output differs
/// from the program's (`<name>-<variant>.txt`), written or removed as the difference stands.
#[test]
#[ignore = "a maintenance aid: writes <name>.txt for every rhtest program without one, and <name>-<variant>.txt where a variant differs, run by name"]
fn rhtest_write_references() {
    let dir = root().join("testdata/rhtest");
    let mut variants = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "exe") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let name = stem.split('-').next().unwrap().to_owned();
        let reference = dir.join(format!("{name}.txt"));
        if !reference.exists() {
            std::fs::write(&reference, native_output(&path)).unwrap();
            eprintln!("{}: written from {stem}", reference.display());
        }
        variants.push((name, stem, path));
    }
    for (name, stem, path) in variants {
        let program = std::fs::read_to_string(dir.join(format!("{name}.txt"))).unwrap().replace("\r\n", "\n");
        let own = dir.join(format!("{stem}.txt"));
        let text = native_output(&path);
        if text.trim_end() == program.trim_end() {
            if own.exists() {
                std::fs::remove_file(&own).unwrap();
                eprintln!("{}: removed, {stem} prints the program's reference", own.display());
            }
        } else if std::fs::read_to_string(&own).ok().is_none_or(|t| t.replace("\r\n", "\n") != text) {
            std::fs::write(&own, text).unwrap();
            eprintln!("{}: written, {stem} differs from the program's reference", own.display());
        }
    }
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

/// Thread-local storage: each thread's own block, from the image's template, and the callbacks.
/// The names the image gives its functions are the names of the C: the classes fixture has no
/// PDB, so the link map beside it names its virtual functions (`Rect::area` as `Rect_area`) and
/// its entry.
#[test]
fn classes64_names_its_functions_in_the_c() {
    let (_, c, _) = emit(&fixture("classes64.exe", "classes.txt"), "names", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    let image = ra_asm_pe::load(&root().join("testdata/bin/classes64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let named: Vec<(&str, u64)> =
        analysis.functions.values().map(|f| (f.name.as_str(), f.entry)).filter(|(n, _)| !n.starts_with("F_")).collect();
    assert!(named.iter().any(|(n, _)| *n == "Rect_area"), "no map name among {named:?}");
    assert!(named.iter().any(|(n, _)| *n == "mainCRTStartup_"), "the entry is not named among {named:?}");
    for (name, address) in named {
        assert!(c.contains(&format!(" {name}(")), "the C does not define {name}");
        assert!(!c.contains(&ra_asm_lift::function_name(address)), "{name} is still named by its address somewhere:\n{c}");
    }
}

/// A program over the static C runtime: the PDB beside it names every function, the runtime's
/// functions are called in the host's runtime rather than lifted, and the port starts at `main`.
#[test]
fn crt64_runs_as_c() {
    let (dir, c, _) = emit(&fixture("crt64.exe", "crt.txt"), "names", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    // A lifted body names itself in its entry hook; a function called in the host's runtime has none.
    let lifted = |name: &str| c.contains(&format!("RA_ENTER(\"{name}\")"));
    assert!(lifted("main_") && lifted("total") && lifted("label"), "the program's functions are not lifted under their names:\n{c}");
    assert!(c.contains("printf(") && !lifted("printf"), "printf is not called in the host's runtime:\n{c}");
    assert!(!lifted("mainCRTStartup") && !lifted("__scrt_common_main_seh"), "the runtime's startup is lifted:\n{c}");
    let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("functions.json")).unwrap()).unwrap();
    let entry = |name: &str| table.as_array().unwrap().iter().find(|e| e["name"] == name).cloned().unwrap_or_else(|| panic!("no {name}"));
    assert_eq!(entry("printf")["status"], "runtime");
    assert_eq!(entry("printf")["imports"], serde_json::json!(["printf"]));
    assert_eq!(entry("main_")["status"], "lifted");
    assert_eq!(entry("mainCRTStartup")["status"], "runtime");
    run_oracle("crt64.exe", "crt.txt");
}

/// The reference of the crt fixture is what the native program prints.
#[test]
fn crt64_expected_is_the_native_output() {
    let run = Command::new(root().join("testdata/bin/crt64.exe")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(root().join("testdata/expected/crt.txt")).unwrap().replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), expected.trim_end());
    assert!(run.status.success());
}

#[test]
fn tls64_runs_as_c() {
    run_oracle("tls64.exe", "tls.txt");
}

#[test]
fn tls32_runs_as_c() {
    run_oracle("tls32.exe", "tls.txt");
}

#[test]
fn tls64_runs_as_rust() {
    run_rust_oracle("tls64.exe", "tls.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn tls32_runs_as_rust() {
    run_rust_oracle("tls32.exe", "tls.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

/// The struct program: a record of mixed fields filled and read through a pointer.
#[test]
fn structs64_runs_as_c() {
    run_oracle("structs64.exe", "structs.txt");
}

#[test]
fn structs32_runs_as_c() {
    run_oracle("structs32.exe", "structs.txt");
}

#[test]
fn structs64_runs_as_rust() {
    run_rust_oracle("structs64.exe", "structs.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn structs32_runs_as_rust() {
    run_rust_oracle("structs32.exe", "structs.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

#[test]
fn structs64_runs_as_cpp() {
    run_oracle_with("structs64.exe", "structs.txt", "cpp", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
}

/// The type recovery in the C: the record's pointer reads as one struct with a field per
/// offset, shared by the function that fills it and the ones that read it, the double field
/// typed by the function that multiplies it, and the accesses as fields, not as casts of
/// pointer arithmetic.
#[test]
fn structs64_reads_as_a_struct() {
    let (_, text, _) = emit(&fixture("structs64.exe", "structs.txt"), "struct", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    let expected = "struct S0 {\n    uint32_t f0;\n    uint8_t f4;\n    uint8_t pad5[1];\n    uint16_t f6;\n    uint64_t f8;\n    double f16;\n    \
                    uint64_t f24;\n    uint32_t f32;\n};\n";
    assert!(text.contains(expected), "the struct is missing or differs:\n{text}");
    assert_eq!(text.matches("struct S").count(), text.matches("struct S0").count(), "one struct for one record:\n{text}");
    for access in ["arg0_s->f0 = 7u;", "arg0_s->f8 = sum;", "* arg0_s->f16;", "((struct S0 *)arg0)->f24;"] {
        assert!(text.contains(access), "`{access}` is missing:\n{text}");
    }
    assert!(!text.contains("(arg0 + "), "an access still goes through pointer arithmetic:\n{text}");
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

/// The classes in C++: a class per RTTI class, derived from its direct bases, its virtual
/// functions as its methods, and the port prints what the program prints, on both targets.
#[test]
fn classes64_runs_as_cpp() {
    run_oracle_with("classes64.exe", "classes.txt", "cpp", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
}

#[test]
fn classes32_runs_as_cpp() {
    run_oracle_with("classes32.exe", "classes.txt", "cpp", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
}

/// The shape of the C++: `Rect` derives from `Shape` and `Named` (its direct bases, not the
/// hierarchy's full list), `Square` from `Shape`; a method that reads the object takes it as
/// `this`, one that never does is static; `Shape::scale`, inherited by `Square`, is declared once;
/// and each virtual function keeps its C name as the vtable's entry, calling the method.
#[test]
fn classes64_reads_as_classes() {
    let (_, text, _) =
        emit(&fixture("classes64.exe", "classes.txt"), "classes", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
    for expected in [
        "class Shape {\npublic:\n    uint64_t scale(uint64_t arg1); // slot 2\n};\n",
        "class Square : public Shape {\npublic:\n    uint64_t area(); // slot 0\n    static uint64_t name(); // slot 1\n};\n",
        "class Named {\n",
        "class Rect : public Shape, public Named {\npublic:\n    uint64_t area(); // slot 0\n",
        "    uint64_t weight(); // slot 1 of the vtable at offset 16\n};\n",
        "uint64_t Rect::area() {\n    RA_ENTER(\"Rect_area\");\n    uint64_t arg0 = (uint64_t)(uintptr_t)this;\n",
        "extern \"C\" uint64_t Rect_area(uint64_t arg0) {\n    return ((Rect *)(uintptr_t)arg0)->area();\n}\n",
        "extern \"C\" uint64_t Rect_name(void) {\n    return Rect::name();\n}\n",
    ] {
        assert!(text.contains(expected), "`{expected}` is missing from the C++:\n{text}");
    }
    assert_eq!(text.matches("\nclass ").count(), 4, "one class per RTTI class:\n{text}");
}

/// The exceptions program: the throws, the catches and the destructors on the way out, as the
/// runtime's exception protocol in C and in Rust.
#[test]
fn throws64_runs_as_c() {
    run_oracle("throws64.exe", "throws.txt");
}

#[test]
fn throws64_runs_as_rust() {
    run_rust_oracle("throws64.exe", "throws.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
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

/// The `static` variant: clang-cl at /O2 /MT, the C runtime linked into the image with no PDB,
/// only the map. The runtime's functions are recognized by the map's libraries and not emitted;
/// the startup is the driver's; the x86 helpers (`_alldiv`, the probe, the cookie check) stay.
#[test]
fn rhtest_core_static_runs_as_c() {
    run_rhtest_as_c("core", "static");
}

/// clang-cl's `insertion_sort` at /MT shares a tail between two paths that the structuring joins
/// with a forward `goto`, which Rust can only write as a labeled block (card 54's domain).
#[test]
#[ignore = "needs the structuring of a shared tail (card 54): a forward goto into insertion_sort the Rust output cannot express"]
fn rhtest_core_static_runs_as_rust() {
    run_rhtest_as_rust("core", "static");
}

#[test]
fn rhtest_int64_static_runs_as_c() {
    run_rhtest_as_c("int64", "static");
}

#[test]
fn rhtest_int64_static_runs_as_rust() {
    run_rhtest_as_rust("int64", "static");
}

#[test]
fn rhtest_core_static_leaves_the_runtime_to_the_host() {
    let (dir, c, _) = emit(&rhtest("core", "static"), "static-names", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    let table = std::fs::read_to_string(dir.join("functions.json")).unwrap();
    assert!(table.contains("\"mainCRTStartup\""), "the startup is in the table:\n{table}");
    assert!(table.contains("\"runtime\""), "the startup has the runtime status:\n{table}");
    for absent in ["mainCRTStartup(", "scrt_common_main", "report_gsfailure("] {
        assert!(!c.contains(&format!(" {absent}")), "`{absent}` is a definition in the C:\n{c}");
    }
    assert!(c.contains("security_check_cookie("), "the cookie check is the program's own:\n{c}");
    assert!(c.contains("main_("), "main is ported:\n{c}");
}

/// The VS2010 `/MT` builds (`<name>-vs2010.exe` with its map, cl 16.00, built elsewhere like the
/// `-O2` and `-Od` ones) are not on every machine: a missing one is skipped with a message.
fn rhtest_vs2010(name: &str) -> Option<Fixture> {
    let fixture = rhtest(name, "vs2010");
    if fixture.binary_path().is_file() {
        return Some(fixture);
    }
    eprintln!("{} is not built here: the test is skipped", fixture.binary);
    None
}

#[test]
fn rhtest_core_vs2010_runs_as_c() {
    if let Some(f) = rhtest_vs2010("core") {
        run_fixture_as_c(&f, "one", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    }
}

#[test]
fn rhtest_core_vs2010_runs_as_rust() {
    if let Some(f) = rhtest_vs2010("core") {
        run_rust_oracle_with(&f, ra_asm_emit::FUNCTIONS_PER_FILE);
    }
}

#[test]
fn rhtest_int64_vs2010_runs_as_c() {
    if let Some(f) = rhtest_vs2010("int64") {
        run_fixture_as_c(&f, "one", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    }
}

#[test]
fn rhtest_int64_vs2010_runs_as_rust() {
    if let Some(f) = rhtest_vs2010("int64") {
        run_rust_oracle_with(&f, ra_asm_emit::FUNCTIONS_PER_FILE);
    }
}

/// The `clang` variant of an rhtest program: clang-cl at /O2 /MD over the same MSVC C runtime,
/// built here (`testdata/rhtest/build.ps1`); the program's own code is clang's, the startup and
/// the helpers it links (`_alldvrm`, `_ftol2`) are MSVC's.
#[test]
fn rhtest_int64_clang_runs_as_c() {
    run_rhtest_as_c("int64", "clang");
}

#[test]
fn rhtest_int64_clang_runs_as_rust() {
    run_rhtest_as_rust("int64", "clang");
}

#[test]
fn rhtest_float_clang_runs_as_c() {
    run_rhtest_as_c("float", "clang");
}

#[test]
fn rhtest_float_clang_runs_as_rust() {
    run_rhtest_as_rust("float", "clang");
}

#[test]
fn rhtest_calls_clang_runs_as_c() {
    run_rhtest_as_c("calls", "clang");
}

#[test]
fn rhtest_calls_clang_runs_as_rust() {
    run_rhtest_as_rust("calls", "clang");
}

#[test]
fn rhtest_fpenv_clang_runs_as_c() {
    run_rhtest_as_c("fpenv", "clang");
}

#[test]
fn rhtest_fpenv_clang_runs_as_rust() {
    run_rhtest_as_rust("fpenv", "clang");
}

/// The floating-point programs at the other levels clang-cl offers (`testdata/rhtest/build.ps1`):
/// `/Od` over SSE2, and `/arch:IA32` at `/O2` and `/Od`, the x87 unit alone, lifted exact so the
/// extended precision of a sum kept in `st(0)` and the precision control a program sets print the
/// bits the native run prints.
#[test]
fn rhtest_float_clang_od_runs_as_c() {
    run_rhtest_as_c("float", "clang-Od");
}

#[test]
fn rhtest_float_clang_od_runs_as_rust() {
    run_rhtest_as_rust("float", "clang-Od");
}

#[test]
fn rhtest_float_ia32_runs_as_c() {
    run_rhtest_as_c("float", "ia32");
}

/// clang's x87 `fsum` at /O2 shares its loop's exit between two paths that the structuring joins
/// with a forward `goto`, which Rust can only write as a labeled block (card 54's domain).
#[test]
#[ignore = "needs the structuring of a shared tail (card 54): a forward goto into fsum the Rust output cannot express"]
fn rhtest_float_ia32_runs_as_rust() {
    run_rhtest_as_rust("float", "ia32");
}

#[test]
fn rhtest_float_ia32_od_runs_as_c() {
    run_rhtest_as_c("float", "ia32-Od");
}

#[test]
fn rhtest_float_ia32_od_runs_as_rust() {
    run_rhtest_as_rust("float", "ia32-Od");
}

#[test]
fn rhtest_fpenv_clang_od_runs_as_c() {
    run_rhtest_as_c("fpenv", "clang-Od");
}

#[test]
fn rhtest_fpenv_clang_od_runs_as_rust() {
    run_rhtest_as_rust("fpenv", "clang-Od");
}

#[test]
fn rhtest_fpenv_ia32_runs_as_c() {
    run_rhtest_as_c("fpenv", "ia32");
}

#[test]
fn rhtest_fpenv_ia32_runs_as_rust() {
    run_rhtest_as_rust("fpenv", "ia32");
}

#[test]
fn rhtest_fpenv_ia32_od_runs_as_c() {
    run_rhtest_as_c("fpenv", "ia32-Od");
}

#[test]
fn rhtest_fpenv_ia32_od_runs_as_rust() {
    run_rhtest_as_rust("fpenv", "ia32-Od");
}

/// The C runtime program: `printf`'s family through the `va_list` adapter, `strto*` with their end
/// pointers, qsort and bsearch calling back, the allocator, `_malloca` through the stack probe.
#[test]
fn rhtest_crt_clang_runs_as_c() {
    run_rhtest_as_c("crt", "clang");
}

/// `malloca_sum`'s two allocation paths each test the count and share the loops after it: the
/// structuring leaves a forward `goto` into the shared region, which Rust can only write as a
/// labeled block (card 54's domain).
#[test]
#[ignore = "needs the structuring of a shared tail after a duplicated test (card 54): a forward goto the Rust output cannot express"]
fn rhtest_crt_clang_runs_as_rust() {
    run_rhtest_as_rust("crt", "clang");
}

#[test]
#[ignore = "a debugging aid: the traced port of crt-clang, run by name"]
fn rhtest_crt_clang_traced() {
    run_fixture_as_c_with(&rhtest("crt", "clang"), "traced", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C, true);
}

/// The Win32 programs wait for the runtime's adapters of the structures laid out apart on x86 and
/// x64 (`WNDCLASSEXA`, `MSG`, `OVERLAPPED`, `SLIST_HEADER`, `WSADATA`, `CRITICAL_SECTION`, a COM
/// object's vtable); `misc` also for setjmp in Rust, which has no non-local jump.
#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I): RegisterClassExA's WNDCLASSEXA and the shell's IMalloc"]
fn rhtest_misc_clang_runs_as_c() {
    run_rhtest_as_c("misc", "clang");
}

#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I), and setjmp, which the Rust output cannot express"]
fn rhtest_misc_clang_runs_as_rust() {
    run_rhtest_as_rust("misc", "clang");
}

#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I): OVERLAPPED, MEMORY_BASIC_INFORMATION, SLIST, INIT_ONCE, MSG"]
fn rhtest_sys_clang_runs_as_c() {
    run_rhtest_as_c("sys", "clang");
}

#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I): OVERLAPPED, MEMORY_BASIC_INFORMATION, SLIST, INIT_ONCE, MSG"]
fn rhtest_sys_clang_runs_as_rust() {
    run_rhtest_as_rust("sys", "clang");
}

#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I): WNDCLASSA, CREATESTRUCT, MSG, WSADATA, CRITICAL_SECTION"]
fn rhtest_win_clang_runs_as_c() {
    run_rhtest_as_c("win", "clang");
}

#[test]
#[ignore = "needs the Win32 structure adapters (a card of milestone I): WNDCLASSA, CREATESTRUCT, MSG, WSADATA, CRITICAL_SECTION"]
fn rhtest_win_clang_runs_as_rust() {
    run_rhtest_as_rust("win", "clang");
}

/// The exception programs wait for milestone J (task 49): structured and C++ exception handling.
#[test]
#[ignore = "needs structured exception handling (`__try`, `_except_handler4`'s scope tables), which task 49 left after the C++ tables"]
fn rhtest_seh_clang_runs_as_c() {
    run_rhtest_as_c("seh", "clang");
}

#[test]
#[ignore = "needs structured exception handling (`__try`, `_except_handler4`'s scope tables), which task 49 left after the C++ tables"]
fn rhtest_seh_clang_runs_as_rust() {
    run_rhtest_as_rust("seh", "clang");
}

#[test]
#[ignore = "needs the x86 frame handler's registration frames, and thrown objects with destructors, which task 49 left after the x64 tables"]
fn rhtest_eh_clang_runs_as_c() {
    run_rhtest_as_c("eh", "clang");
}

#[test]
#[ignore = "needs the x86 frame handler's registration frames, and thrown objects with destructors, which task 49 left after the x64 tables"]
fn rhtest_eh_clang_runs_as_rust() {
    run_rhtest_as_rust("eh", "clang");
}

/// The C++ programs: `stl` and `stlio` import msvcp140 under x86 mangled names the x64 DLL does
/// not export (a thiscall-to-x64 adapter of the C++ runtime is a card of its own); `cpp`, `com`
/// and `engine` build and start but go wrong in their first classes (`cpp`'s `multi`, `engine`'s
/// `stream len`, `com`'s COM vtables and an instruction the lift does not cover).
#[test]
#[ignore = "ports but goes wrong at `multi`: the C++ object model under the x64 runtime, with task 49's exceptions"]
fn rhtest_cpp_clang_runs_as_c() {
    run_rhtest_as_c("cpp", "clang");
}

#[test]
#[ignore = "ports but goes wrong at `multi`: the C++ object model under the x64 runtime, with task 49's exceptions"]
fn rhtest_cpp_clang_runs_as_rust() {
    run_rhtest_as_rust("cpp", "clang");
}

#[test]
#[ignore = "needs COM objects of the x64 runtime behind x86 vtables, and an instruction the lift does not cover (0x401e2e)"]
fn rhtest_com_clang_runs_as_c() {
    run_rhtest_as_c("com", "clang");
}

#[test]
#[ignore = "needs COM objects of the x64 runtime behind x86 vtables, and an instruction the lift does not cover (0x401e2e)"]
fn rhtest_com_clang_runs_as_rust() {
    run_rhtest_as_rust("com", "clang");
}

#[test]
#[ignore = "needs the C++ runtime (msvcp140) under x86 mangled names, which the x64 DLL does not export"]
fn rhtest_stl_clang_runs_as_c() {
    run_rhtest_as_c("stl", "clang");
}

#[test]
#[ignore = "needs the C++ runtime (msvcp140) under x86 mangled names, which the x64 DLL does not export"]
fn rhtest_stl_clang_runs_as_rust() {
    run_rhtest_as_rust("stl", "clang");
}

#[test]
#[ignore = "needs the C++ runtime (msvcp140) under x86 mangled names, which the x64 DLL does not export"]
fn rhtest_stlio_clang_runs_as_c() {
    run_rhtest_as_c("stlio", "clang");
}

#[test]
#[ignore = "needs the C++ runtime (msvcp140) under x86 mangled names, which the x64 DLL does not export"]
fn rhtest_stlio_clang_runs_as_rust() {
    run_rhtest_as_rust("stlio", "clang");
}

#[test]
#[ignore = "ports but goes wrong at `stream len`: the C++ object model under the x64 runtime, with task 49's exceptions"]
fn rhtest_engine_clang_runs_as_c() {
    run_rhtest_as_c("engine", "clang");
}

#[test]
#[ignore = "ports but goes wrong at `stream len`: the C++ object model under the x64 runtime, with task 49's exceptions"]
fn rhtest_engine_clang_runs_as_rust() {
    run_rhtest_as_rust("engine", "clang");
}
