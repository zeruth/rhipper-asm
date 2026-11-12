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

/// A test's own directory under the system temp, removed when the guard drops; `RA_KEEP_SCRATCH`
/// keeps it for a person to read.
fn scratch(name: &str) -> ra_asm_testing::Scratch {
    ra_asm_testing::Scratch::new("ra-asm-emit-tests", name)
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
    /// The image without the map or PDB beside it, its C runtime named by the signatures of the
    /// installed toolset's static runtime instead.
    by_signatures: bool,
    /// The facts of a traced run the analysis takes; none by default.
    facts: ra_asm_analysis::Facts,
}

fn fixture(binary: &str, expected: &str) -> Fixture {
    Fixture {
        binary: format!("bin/{binary}"),
        expected: format!("expected/{expected}"),
        exit_line: false,
        no_goto: true,
        x87: ra_asm_lift::X87Unit::Double,
        by_signatures: false,
        facts: ra_asm_analysis::Facts::default(),
    }
}

impl Fixture {
    fn name(&self) -> &str {
        Path::new(&self.binary).file_name().unwrap().to_str().unwrap()
    }

    fn binary_path(&self) -> PathBuf {
        root().join("testdata").join(&self.binary)
    }

    /// The image as the port reads it: with the names beside it, or, `by_signatures`, bare and
    /// with the installed runtime's signatures applied.
    fn image(&self) -> ra_asm_pe::Image {
        if !self.by_signatures {
            return ra_asm_pe::load(&self.binary_path()).unwrap();
        }
        let mut image = ra_asm_pe::Image::parse(std::fs::read(self.binary_path()).unwrap()).unwrap();
        let set = installed_runtime_signatures().expect("the toolset was checked before");
        let applied = ra_asm_analysis::signatures::apply(&mut image, &set);
        eprintln!("{}: {} of {} functions matched by signature", self.name(), applied.matches.len(), applied.functions);
        image
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
fn emit(fixture: &Fixture, tag: &str, functions_per_file: usize, dialect: ra_asm_emit::Dialect) -> (ra_asm_testing::Scratch, String, u32) {
    let binary = fixture.name();
    let image = fixture.image();
    let analysis = ra_asm_analysis::analyze_with(&image, &fixture.facts);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: fixture.x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let classes = ra_asm_emit::classes(&analysis, &signatures);
    let options = ra_asm_emit::COptions { names, dialect, classes, ..Default::default() };
    let unit = ra_asm_emit::emit_unit(&lifted.module, &options);
    let files = unit.files(functions_per_file);
    let dir = scratch(&format!("{binary}-{tag}"));
    ra_asm_emit::write_files(&dir, &files).unwrap();
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
    std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit.functions, &files, &infos)).unwrap();
    write_resources(&dir, &image);
    (dir, unit.text(), u32::from(image.pointer_size()) * 8)
}

/// The program's resources as `resources.res` in the port's directory, as `ras emit` writes them,
/// for the link to fold into the port.
fn write_resources(dir: &Path, image: &ra_asm_pe::Image) {
    if !image.resources.is_empty() {
        std::fs::write(dir.join("resources.res"), ra_asm_pe::resources::res_file(image)).unwrap();
    }
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
    let resources = !image.resources.is_empty();
    let options = ra_asm_emit::RustOptions {
        names,
        imports,
        entry,
        package: package.clone(),
        functions_per_file,
        pointer_bits,
        resources,
        ..Default::default()
    };
    let output = ra_asm_emit::emit_rust(&lifted.module, &options);
    let dir = scratch(&format!("{binary}-rust-{functions_per_file}"));
    ra_asm_emit::write_files(&dir, &output.files).unwrap();
    write_resources(&dir, &image);
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
    if dir.join("resources.res").is_file() {
        build.arg(dir.join("resources.res"));
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
    Fixture {
        binary: format!("rhtest/{name}-{variant}.exe"),
        expected,
        exit_line: true,
        no_goto: false,
        x87,
        by_signatures: false,
        facts: ra_asm_analysis::Facts::default(),
    }
}

/// The signatures of the installed toolset's static x86 C runtime (`libcmt.lib`,
/// `libvcruntime.lib`, `libucrt.lib`), or `None` without a toolset.
fn installed_runtime_signatures() -> Option<ra_asm_analysis::SignatureSet> {
    let paths = ra_asm_analysis::signatures::installed_msvc_runtime("x86")?;
    let mut set = ra_asm_analysis::SignatureSet::default();
    for path in &paths {
        set.extend(ra_asm_analysis::SignatureSet::from_library(path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display())));
    }
    Some(set)
}

/// The `static` variant of an rhtest program read without its map: the installed toolset's
/// runtime signatures name the C runtime instead, and the port runs the same.
fn run_rhtest_static_by_signatures_as_c(name: &str) {
    if installed_runtime_signatures().is_none() {
        eprintln!("no MSVC toolset with libcmt.lib: the signature oracle is skipped");
        return;
    }
    let fixture = Fixture { by_signatures: true, ..rhtest(name, "static") };
    run_fixture_as_c(&fixture, "sig", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
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

/// The compiler's output read back as its source: `collatz`'s rotated loop as a `for` over its
/// two induction variables with the `cmov` of its ternary absorbed into the step, `print_int`'s
/// digit loop as the `do ... while` it is written as, the division and the remainder by ten as
/// the operators behind the magic multiply, and the walk to a string's end as `strlen`.
#[test]
fn control_reads_as_source() {
    for binary in ["control64.exe", "control32.exe"] {
        let (_, c, _) = emit(&fixture(binary, "control.txt"), "source", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        for expected in [
            "rcx_phi = 27u; rcx_phi != 1u; rax_phi++, rcx_phi = (",
            "(rcx_phi * 3u) + 1u)",
            "_phi > 9u);",
            " / 10u;",
            " % 10u)",
            "ra_strlen((const char *)(uintptr_t)(arg0))",
            // `classify`'s switch reads over an enum of its cases (task 115).
            "enum classify_case { CLASSIFY_0 = 0, CLASSIFY_1 = 1, ",
            "switch ((enum classify_case)",
            "case CLASSIFY_8:",
        ] {
            assert!(c.contains(expected), "{binary}: no `{expected}` in the C:\n{c}");
        }
        assert!(!c.contains("3435973837"), "{binary}: the magic multiply is still in the C:\n{c}");
    }
}

#[test]
fn inline64_runs_as_c() {
    run_oracle("inline64.exe", "inline.txt");
}

#[test]
fn inline32_runs_as_c() {
    run_oracle("inline32.exe", "inline.txt");
}

#[test]
fn inline64_runs_as_rust() {
    run_rust_oracle("inline64.exe", "inline.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

/// The inlining undone: `score`, inlined into `show_low`, `show_high` and `show_from`, reads as
/// one function `H_<address>` of the C, its prototype, its definition and a call in each of the
/// three callers; no second helper is made of it.
#[test]
#[ignore = "#116: the outlined helper is not yet called from each of the three callers"]
fn inline_reads_as_source() {
    for binary in ["inline64.exe", "inline32.exe"] {
        let (_, c, _) = emit(&fixture(binary, "inline.txt"), "source", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
        let helpers: std::collections::BTreeSet<&str> = c
            .match_indices("H_")
            .filter(|(at, _)| !c[..*at].ends_with(word))
            .map(|(at, _)| c[at..].split(|ch: char| !word(ch)).next().unwrap())
            .collect();
        assert_eq!(helpers.len(), 1, "{binary}: one outlined helper in the C, not {helpers:?}:\n{c}");
        let helper = helpers.first().unwrap();
        let calls = c.matches(&format!("{helper}(")).count();
        assert!(calls >= 5, "{binary}: {helper} is declared, defined and called three times, not {calls} mentions:\n{c}");
        for caller in ["show_low", "show_high", "show_from"] {
            // The definition: the mention whose line opens a body.
            let definition = c
                .match_indices(&format!(" {caller}("))
                .map(|(at, _)| &c[at..])
                .find(|rest| rest.lines().next().is_some_and(|line| line.trim_end().ends_with('{')))
                .unwrap_or_else(|| panic!("{binary}: no definition of {caller}:\n{c}"));
            let body = definition.split("\n}").next().unwrap();
            assert!(body.contains(&format!("{helper}(")), "{binary}: {caller} calls the helper:\n{body}");
            assert!(!body.contains("while (") && !body.contains("for ("), "{binary}: {caller} keeps no loop:\n{body}");
        }
    }
}

#[test]
fn obfusc64_runs_as_c() {
    run_oracle("obfusc64.exe", "obfusc.txt");
}

#[test]
fn obfusc32_runs_as_c() {
    run_oracle("obfusc32.exe", "obfusc.txt");
}

/// The obfuscation undone: the flattened function reads as its loop, with no dispatcher switch
/// and none of its state numbers (0x3a1 and 0x1f2 of the source) left; the encrypted strings are
/// literals; the bogus branch behind the opaque predicate, which would have decrypted "never",
/// is gone.
#[test]
fn obfusc_reads_as_source() {
    for binary in ["obfusc64.exe", "obfusc32.exe"] {
        let (_, c, _) = emit(&fixture(binary, "obfusc.txt"), "source", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        for expected in ["\"Collatz steps\\r\\n\"", "\"steps \"", "memcpy("] {
            assert!(c.contains(expected), "{binary}: no `{expected}` in the C:\n{c}");
        }
        assert!(c.contains("while (") || c.contains("for ("), "{binary}: no loop in the C:\n{c}");
        for absent in ["switch (", "929", "498", "never"] {
            assert!(!c.contains(absent), "{binary}: `{absent}` is still in the C:\n{c}");
        }
    }
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

/// The virtual calls of the classes fixture read as direct calls: `describe` takes a `Shape` and
/// `describe_named` a `Named` whose class the call does not know, so each call tests the slot's
/// code address against the functions the vtables fill it with and calls the one it holds by
/// name; no call through a pointer is left.
#[test]
fn classes64_calls_its_virtual_functions_by_name() {
    let (_, c, _) =
        emit(&fixture("classes64.exe", "classes.txt"), "devirtualized", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    let image = ra_asm_pe::load(&root().join("testdata/bin/classes64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let name = |address: u64| analysis.function(address).unwrap_or_else(|| panic!("a function at {address:#x}")).name.clone();
    let body = |address: u64| {
        let name = name(address);
        let start = c.find(&format!("RA_ENTER(\"{name}\")")).unwrap_or_else(|| panic!("the C does not define {name}:\n{c}"));
        let end = c[start..].find("\n}\n").map_or(c.len(), |k| start + k);
        (name, c[start..end].to_owned())
    };
    // `describe` and `describe_named`, by their addresses in `classes64.map`, with the functions
    // their calls reach: `Square::area`, `Rect::area`, `Square::name`, `Rect::name`,
    // `Shape::scale`, `Rect::scale`; `Rect::label`, `Named::label`, `Rect::weight`, `Named::weight`.
    for (caller, callees) in [
        (0x1_4000_1070, &[0x1_4000_11d0, 0x1_4000_1200, 0x1_4000_11e0, 0x1_4000_1210, 0x1_4000_11f0, 0x1_4000_1220][..]),
        (0x1_4000_10c0, &[0x1_4000_1230, 0x1_4000_1250, 0x1_4000_1240, 0x1_4000_1260][..]),
    ] {
        let (caller, text) = body(caller);
        assert!(!text.contains("(*)("), "{caller} still calls through a pointer:\n{text}");
        for &callee in callees {
            let callee = name(callee);
            assert!(text.contains(&format!("{callee}(")), "{caller} does not call {callee} by name:\n{text}");
        }
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

/// The window program: user32 calls the program's window procedure back (on x86 through its
/// universal entry point, with the four arguments in registers), the message loop runs to the
/// quit message, and the 32-bit port's WNDCLASSA, MSG and PAINTSTRUCT go through the runtime's
/// adapters.
#[test]
fn window64_runs_as_c() {
    run_oracle("window64.exe", "window.txt");
}

#[test]
fn window32_runs_as_c() {
    run_oracle("window32.exe", "window.txt");
}

/// The files program: a file created, written, read back, asked for its attributes and removed
/// through kernel32, with the SDK's named constants in every call.
#[test]
fn files64_runs_as_c() {
    run_oracle("files64.exe", "files.txt");
}

#[test]
fn files32_runs_as_c() {
    run_oracle("files32.exe", "files.txt");
}

#[test]
fn files64_runs_as_rust() {
    run_rust_oracle("files64.exe", "files.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

/// The reference of the files fixture is what the native program prints.
#[test]
fn files64_expected_is_the_native_output() {
    let run = Command::new(root().join("testdata/bin/files64.exe")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(root().join("testdata/expected/files.txt")).unwrap().replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), expected.trim_end());
    assert!(run.status.success());
}

/// The constants of the Windows API calls read as the SDK names them, from the prototype table's
/// enumerations: the access mask and the share mode of `CreateFileA` in both ports, and in the
/// 32-bit port, whose pushed arguments the raise forwards into the call, the disposition and the
/// attributes too; the handle compared with `INVALID_HANDLE_VALUE`, the attribute bit tested
/// with its name, and each name defined once at the top with the value it stands for.
#[test]
fn files_read_with_the_sdk_names() {
    for binary in ["files64.exe", "files32.exe"] {
        let (_, c, _) = emit(&fixture(binary, "files.txt"), "names", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        let mut expected = vec![
            "GetStdHandle(STD_OUTPUT_HANDLE)",
            "GENERIC_READ | GENERIC_WRITE, 0ull, 0ull, ",
            "GENERIC_READ, FILE_SHARE_READ, 0ull, ",
            " INVALID_HANDLE_VALUE)",
            " & FILE_ATTRIBUTE_DIRECTORY)",
            "#define GENERIC_READ 0x80000000ull\n",
            "#define FILE_ATTRIBUTE_DIRECTORY ((uint8_t)0x10)\n",
        ];
        if binary == "files32.exe" {
            expected.extend([
                "GENERIC_READ | GENERIC_WRITE, 0ull, 0ull, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0ull)",
                "OPEN_EXISTING, 0ull, 0ull)",
            ]);
        }
        for text in expected {
            assert!(c.contains(text), "{binary}: no `{text}` in the C:\n{c}");
        }
        assert_eq!(c.matches("#define INVALID_HANDLE_VALUE ").count(), 1, "{binary}:\n{c}");
        // The mask may still sit inside a packed stack constant; the call itself names it.
        for call in c.lines().filter(|l| l.contains("CreateFileA(")) {
            assert!(!call.contains("3221225472"), "{binary}: the access mask is still a number:\n{call}\n{c}");
        }
    }
}

/// The resources program: a string table and an RCDATA block read through user32 and kernel32,
/// which the port finds in its own resource directory, and tables of integers, strings and
/// doubles read by index.
#[test]
fn resources64_runs_as_c() {
    run_oracle("resources64.exe", "resources.txt");
}

#[test]
fn resources32_runs_as_c() {
    run_oracle("resources32.exe", "resources.txt");
}

#[test]
fn resources64_runs_as_cpp() {
    run_oracle_with("resources64.exe", "resources.txt", "cpp", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::Cpp);
}

#[test]
fn resources64_runs_as_rust() {
    run_rust_oracle("resources64.exe", "resources.txt", ra_asm_emit::FUNCTIONS_PER_FILE);
}

/// The typed data in the C: the table of squares reads as an array of `uint32_t`, the table of
/// names as an array of address words, the halves as doubles and the counters as an array of
/// `uint16_t` in the writable section; the strings the program passes are string literals; and
/// each read by index is an element, not a cast of pointer arithmetic.
#[test]
fn resources64_data_reads_as_arrays() {
    let (_, text, _) =
        emit(&fixture("resources64.exe", "resources.txt"), "arrays", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    for expected in [
        "uint32_t f0[8]; uint64_t f32[4]; double f64[4]; double f96; ",
        "\n    0u, 1u, 4u, 9u, 16u, 25u, 36u, 49u,\n",
        "0.5, 1.5, 2.5, 3.5,",
        "\"zero\"",
        "\"three\"",
        "\"\\r\\n\"",
        "uint16_t f16[4]; } RA_PACKED_END;",
        "sec_rdata.f0[",
        "sec_rdata.f32[",
        "sec_rdata.f64[",
        "* sec_rdata.f96",
        "sec_data.f16[",
    ] {
        assert!(text.contains(expected), "`{expected}` is missing from:\n{text}");
    }
    assert!(!text.contains("(uint8_t *)&sec_rdata + ((uint64_t)"), "an indexed read is still pointer arithmetic:\n{text}");
}

/// The reference of the resources fixture is what the native program prints.
#[test]
fn resources64_expected_is_the_native_output() {
    let run = Command::new(root().join("testdata/bin/resources64.exe")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(root().join("testdata/expected/resources.txt")).unwrap().replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), expected.trim_end());
    assert!(run.status.success());
}

/// The struct program: a record of mixed fields filled and read through a pointer.
#[test]
fn structs64_runs_as_c() {
    run_oracle("structs64.exe", "structs.txt");
}

/// The dispatch program without facts: its calls through the table stay indirect.
#[test]
fn dispatch64_runs_as_c() {
    run_oracle("dispatch64.exe", "dispatch.txt");
}

#[test]
fn dispatch32_runs_as_c() {
    run_oracle("dispatch32.exe", "dispatch.txt");
}

/// The facts of a traced run: the interpreter runs the ABI lift and sees the four handlers behind
/// the call through the table, the analysis of the next pass takes them, and the port calls the
/// handlers directly (the indirect call stays for a target the run did not see), with the
/// function table marking what the run saw. The port runs the same.
#[test]
fn a_traced_run_makes_the_dispatch_direct() {
    for binary in ["dispatch64.exe", "dispatch32.exe"] {
        let mut fixture = fixture(binary, "dispatch.txt");
        let image = fixture.image();
        let analysis = ra_asm_analysis::analyze(&image);
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: fixture.x87 });
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
        assert_eq!(outcome.exit, 0, "{binary}: the traced run (stop: {:?})", outcome.stop);
        assert_eq!(outcome.facts.calls.len(), 1, "{binary}: the one call through the table: {:?}", outcome.facts.calls);
        fixture.facts = outcome.facts;
        let (dir, c, _) = emit(&fixture, "facts", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        // The definition of dispatch: the unindented line that opens it, to the closing brace.
        let lines: Vec<&str> = c.lines().collect();
        let opens = lines.iter().position(|l| !l.starts_with(' ') && l.contains(" dispatch(") && l.ends_with('{')).unwrap();
        let body = lines[opens..].iter().take_while(|l| **l != "}").copied().collect::<Vec<_>>().join("\n");
        for handler in ["h_add(", "h_sub(", "h_mul(", "h_xor("] {
            assert!(body.contains(handler), "{binary}: dispatch calls {handler}) directly:\n{body}");
        }
        let table = std::fs::read_to_string(dir.join("functions.json")).unwrap();
        assert!(table.contains("\"seen\": true"), "{binary}: the table marks the functions the run entered:\n{table}");
        assert!(table.contains("\"seen_calls\""), "{binary}: the table lists the resolved calls:\n{table}");
        run_fixture_as_c(&fixture, "facts", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
    }
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

/// The Rust package of a fixture as text, the modules joined, without building it.
fn rust_text(fixture: &Fixture) -> String {
    rust_text_with(fixture, true)
}

/// The functions of a fixture's Rust port, with or without the proofs of safety.
fn rust_text_with(fixture: &Fixture, proofs: bool) -> String {
    let image = ra_asm_pe::load(&fixture.binary_path()).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: fixture.x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let options = ra_asm_emit::RustOptions { names, proofs, ..ra_asm_emit::RustOptions::default() };
    ra_asm_emit::emit_rust(&lifted.module, &options).functions.into_iter().map(|f| f.text).collect()
}

/// The Rust port is safe where the engine can prove it (task 120): the functions are safe
/// `extern "C" fn`s, a local the frame never shares is a value, an access of a data section at a
/// known offset is a checked one, and what stays raw sits in an `unsafe` block that names why.
/// The control port keeps under half the `unsafe` blocks of the raw port, where every raw
/// operation stands in its own.
#[test]
fn control64_rust_is_safe_where_provable() {
    let fixture = fixture("control64.exe", "control.txt");
    let proven = rust_text_with(&fixture, true);
    let raw = rust_text_with(&fixture, false);
    let blocks = |text: &str| text.matches("unsafe {").count();
    assert!(blocks(&proven) * 2 < blocks(&raw), "{} unsafe blocks against {} raw:\n{proven}", blocks(&proven), blocks(&raw));
    assert!(!proven.contains("pub unsafe extern \"C\" fn"), "an unsafe function:\n{proven}");
    // `sink`, `out` and the sorted `table` are accesses of the data section, constant and indexed.
    assert!(proven.contains("sec_data.get::<u32>(64)"), "{proven}");
    assert!(proven.contains("sec_data.set::<u32>(("), "{proven}");
    // The `n` of WriteFile is a local the function keeps as a value; the frame's address escapes
    // to WriteFile and stays raw, named as such.
    assert!(proven.contains("let mut local_m18: u64 = 0u64;"), "{proven}");
    assert!(proven.contains(&format!("// {}", "memory at a computed address")), "{proven}");
    assert!(proven.contains("unsafe { /* an import */") || proven.contains("// an import"), "{proven}");
}

/// The text of the function whose definition starts with `head`, up to its closing brace.
fn function_text<'a>(text: &'a str, head: &str) -> &'a str {
    let start = text.find(head).unwrap_or_else(|| panic!("no `{head}` in:\n{text}"));
    let end = text[start..].find("\n}\n").map_or(text.len(), |e| start + e + 3);
    &text[start..end]
}

/// The switches of core as their source wrote them: `fallthrough` with its cases that run into
/// the next (`case 0: r += 1;` into `case 1: r *= 3; break;`, `case 4` into `case 5` into
/// `case 6`) as C fall-throughs, and in Rust as one arm per chain whose steps test the value;
/// `vowels`, whose every case breaks (`case 'a': … case 'u': n += 1; break;` is one case of five
/// values, not a fall-through), with no fall-through, no copied case tail and no `goto`.
#[test]
fn rhtest_core_switches_read_as_source() {
    for variant in ["O2", "Od"] {
        let fixture = rhtest("core", variant);
        let (_, c, _) = emit(&fixture, "switches", ra_asm_emit::FUNCTIONS_PER_FILE, ra_asm_emit::Dialect::C);
        let f = function_text(&c, "uint32_t fallthrough(uint32_t arg0) {");
        assert_eq!(f.matches("/* falls through */").count(), 3, "core-{variant}:\n{f}");
        // The cases read over the switch's own enum (task 115).
        for (from, into) in [
            ("case FALLTHROUGH_0:", "case FALLTHROUGH_1:"),
            ("case FALLTHROUGH_4:", "case FALLTHROUGH_5:"),
            ("case FALLTHROUGH_5:", "case FALLTHROUGH_6:"),
        ] {
            let (a, b) = (f.find(from).unwrap(), f.find(into).unwrap());
            assert!(a < b && f[a..b].contains("/* falls through */"), "core-{variant}: `{from}` does not fall into `{into}`:\n{f}");
        }
        assert_eq!(f.matches("case FALLTHROUGH_1:").count(), 1, "core-{variant}:\n{f}");
        assert_eq!(f.matches("1000u").count(), 1, "core-{variant}: the tail of case 5 is copied:\n{f}");
        assert!(!f.contains("goto "), "core-{variant}:\n{f}");
        let v = function_text(&c, "uint32_t vowels(uint32_t arg0) {");
        assert!(!v.contains("falls through") && !v.contains("goto "), "core-{variant}:\n{v}");
        for tail in ["65536u", "256u"] {
            assert_eq!(v.matches(tail).count(), 1, "core-{variant}: a case tail of vowels is copied:\n{v}");
        }

        let r = rust_text(&fixture);
        let f = function_text(&r, "fn fallthrough(arg0: u32) -> u32 {");
        for arm in ["0u32 | 1u32 => {", "4u32 | 5u32 | 6u32 => {", "if matches!(", "4u32 | 5u32) {"] {
            assert!(f.contains(arm), "core-{variant}: no `{arm}` in:\n{f}");
        }
        assert_eq!(f.matches("1000u32").count(), 1, "core-{variant}: the tail of case 5 is copied:\n{f}");
        let v = function_text(&r, "fn vowels(arg0: u32) -> u32 {");
        assert!(!v.contains("matches!(") && !v.contains("ra_goto"), "core-{variant}:\n{v}");
        for tail in ["65536u32", "256u32"] {
            assert_eq!(v.matches(tail).count(), 1, "core-{variant}: a case tail of vowels is copied:\n{v}");
        }
    }
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

/// The same two static builds with no map at all: the signatures made from the installed
/// toolset's `libcmt.lib` (and the ucrt and vcruntime beside it) name the C runtime before the
/// lift, its bodies stay out of the port, and the port prints the reference.
#[test]
fn rhtest_core_static_runs_as_c_by_signatures() {
    run_rhtest_static_by_signatures_as_c("core");
}

#[test]
fn rhtest_int64_static_runs_as_c_by_signatures() {
    run_rhtest_static_by_signatures_as_c("int64");
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
