//! `ras pipeline` over a project made of one fixture: every step runs, every output lands where
//! the workbench looks for it, and the port prints what the reference prints. Those are the slow
//! tier's, behind the `oracle` feature; the large program's run is ignored and needs no feature,
//! and the link of a C++ port from the template needs none either.

use std::path::PathBuf;
use std::process::Command;

/// The large program of [`a_large_program_runs_the_pipeline_and_gives_its_peak`]: Windows' own
/// `mshtml.dll`, about 100k functions, which this machine has; `RA_LARGE_BINARY` names another.
const LARGE: &str = r"C:\Windows\System32\mshtml.dll";

/// The variable that names a large program in [`LARGE`]'s place.
const LARGE_VARIABLE: &str = "RA_LARGE_BINARY";

/// The variable that sets the seconds [`a_large_program_runs_the_pipeline_and_gives_its_peak`]
/// spends, [`BUDGET`] when unset.
const BUDGET_VARIABLE: &str = "RA_PIPELINE_BUDGET";

/// The seconds the large program's steps run for by default: the check that runs the test gives
/// it 1800, its build included, and mshtml's lift alone takes longer than that.
const BUDGET: u64 = 1200;

/// The most memory a step of the large program's pipeline may hold, in bytes.
const BOUND: u64 = 4 << 30;

/// A large program runs the analyze, lift and emit steps, each in a process of its own, and the
/// peak memory of each goes to the test's standard error as the step ends, past the harness's
/// capture, and stays under [`BOUND`]. The emit's process analyzes, lifts and raises the program
/// again before it translates it, so its peak is that of the whole pipeline up to the source.
/// Once the budget of [`BUDGET_VARIABLE`] has passed, the step running is stopped with its peak so
/// far, which stays under the bound too, and the test passes with the peaks it has, naming the
/// steps it did not finish. The build and the run of such a port are the person's to start.
#[test]
#[ignore = "runs the pipeline over mshtml.dll, about 100k functions"]
fn a_large_program_runs_the_pipeline_and_gives_its_peak() {
    use std::io::Write as _;
    use std::time::{Duration, Instant};
    let exe = std::env::var_os(LARGE_VARIABLE).map_or_else(|| PathBuf::from(LARGE), PathBuf::from);
    if !exe.is_file() {
        let _ = writeln!(std::io::stderr(), "{} is not on this host: the large program's pipeline is skipped", exe.display());
        return;
    }
    let budget = std::env::var(BUDGET_VARIABLE).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(BUDGET);
    let deadline = Instant::now() + Duration::from_secs(budget);
    let exe = std::path::absolute(&exe).unwrap();
    let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "large");
    let json = serde_json::json!({ "name": "large", "exe": exe.to_string_lossy() });
    std::fs::write(dir.join("rhipper.json"), json.to_string()).unwrap();
    let steps = ["analyze", "lift", "emit"];
    for (i, step) in steps.iter().enumerate() {
        if Instant::now() >= deadline {
            let _ = writeln!(std::io::stderr(), "the budget of {budget}s has passed: not reached: {}", steps[i..].join(", "));
            return;
        }
        let started = Instant::now();
        let mut child = Command::new(env!("CARGO_BIN_EXE_ras"))
            .args(["pipeline", "--project"])
            .arg(&*dir)
            .args(["--only", step])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let log = std::fs::read_to_string(dir.join(format!("build/pipeline/{step}.log"))).unwrap_or_default();
        let Some(status) = status else {
            // The process is gone but its handle is open: its peak until the stop is still there.
            let measured = peak_memory(&child);
            let peak = measured.map_or_else(|| "not measured".to_owned(), |b| format!("{} MB", b >> 20));
            let _ = writeln!(
                std::io::stderr(),
                "{step} peak memory: {peak} until it was stopped unfinished after {}s, the budget of {budget}s spent; not reached: {}",
                started.elapsed().as_secs(),
                if i + 1 < steps.len() { steps[i + 1..].join(", ") } else { "none".to_owned() }
            );
            if let Some(bytes) = measured {
                assert!(
                    bytes < BOUND,
                    "the {step} step held {} MB before it was stopped, past the bound of {} MB",
                    bytes >> 20,
                    BOUND >> 20
                );
            }
            return;
        };
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        assert!(status.success(), "the {step} step failed; the end of its log, last line first:\n{}", tail.join("\n"));
        // Off Windows the step's own measure, which its log ends with.
        let logged = log.lines().rev().find_map(|l| l.strip_prefix("peak memory: ")?.strip_suffix(" MB")?.parse::<u64>().ok());
        // A step whose peak is not known has not given the baseline: it fails rather than passes.
        let peak = peak_memory(&child)
            .or(logged.map(|mb| mb << 20))
            .unwrap_or_else(|| panic!("the {step} step's peak memory was not measured on this host"));
        let summary = log.lines().rev().find(|l| l.contains("functions,")).unwrap_or("");
        // Straight to the standard error, which the harness does not capture of a passing test.
        let _ = writeln!(std::io::stderr(), "{step} peak memory: {} MB in {}s; {summary}", peak >> 20, started.elapsed().as_secs());
        assert!(peak < BOUND, "the {step} step held {} MB, past the bound of {} MB", peak >> 20, BOUND >> 20);
    }
    assert!(dir.join("build/program.ll").is_file() && dir.join("build/functions.json").is_file());
}

/// The most memory the exited `child` held: the larger of its peak working set and its peak
/// commit, so a run that swapped counts what it asked for.
#[cfg(windows)]
fn peak_memory(child: &std::process::Child) -> Option<u64> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    /// `PROCESS_MEMORY_COUNTERS`: the sizes are the peak working set first and the peak commit
    /// (`PeakPagefileUsage`) last.
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        _faults: u32,
        sizes: [usize; 8],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Counters, cb: u32) -> i32;
    }

    let mut counters = Counters { cb: size_of::<Counters>() as u32, ..Counters::default() };
    // SAFETY: the child's handle stays open until `child` drops, and `counters` is the structure
    // of the size its `cb` names.
    let ok = unsafe { K32GetProcessMemoryInfo(child.as_raw_handle(), &raw mut counters, counters.cb) };
    (ok != 0).then_some(counters.sizes[0].max(counters.sizes[7]) as u64)
}

#[cfg(not(windows))]
fn peak_memory(_: &std::process::Child) -> Option<u64> {
    None
}

/// The program as the pipeline lifts and raises it (frozen, standalone, to the ABI level) holds
/// one function at a time on each thread through the raise, the whole-module steps included: they
/// read the other functions through the table the raise writes as it freezes each function and
/// the steps keep, never building it again, and only a step that reads several functions at once
/// thaws those, never the whole program. The module stays frozen from one step
/// to the next, and the raise still ends with every function thawed for the emit.
#[test]
fn per_function_raise_holds_one_function() {
    let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin");
    let mut fewest = usize::MAX;
    for fixture in ["control64.exe", "control32.exe", "classes32.exe"] {
        let image = ra_asm_pe::load(&bin.join(fixture)).unwrap();
        let analysis = ra_asm_analysis::analyze(&image);
        let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_frozen_with(&image, &analysis, &[], options);
        assert!(!lifted.module.frozen.is_empty(), "{fixture}: the lift froze nothing");
        let raise_options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, raise_options);
        assert!(!signatures.is_empty(), "{fixture}: no signature");
        assert!(lifted.module.frozen.is_empty(), "{fixture}: the raise left functions frozen");
        fewest = fewest.min(lifted.module.functions.len());
    }
    let (most, together) = (ra_asm_lift::raise::held::most_thawed(), ra_asm_lift::raise::held::most_together());
    let between = ra_asm_lift::raise::held::thawed_between();
    let rebuilt = ra_asm_lift::raise::held::tables_rebuilt();
    eprintln!("the raise held at most {most} function(s) thawed on a thread, and thawed {together} together");
    assert_eq!(most, 1, "a thread held {most} functions thawed at once");
    assert_eq!(rebuilt, 0, "a module step found the table of the functions out of step {rebuilt} time(s) and thawed them all for it");
    assert_eq!(between, 0, "a module step began or ended with {between} functions thawed beside the frozen ones");
    assert!(together < fewest, "a module step thawed {together} functions together, the whole of a program of {fewest}");
}

/// The C++ port of a program (`classes64`) builds from the `CMakeLists.txt` the pipeline writes
/// and links, with msvcprt.lib, which gives a C++ port msvcp140's imports and the static part of
/// its import library (`std::_Facet_Register`, the `__std_*` vector algorithms). Without
/// Windows, cmake or clang the build is skipped and says why.
#[test]
fn a_cpp_port_links_msvcprt_through_the_template() {
    let available = |tool: &str| Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success());
    if !cfg!(windows) {
        eprintln!("the port of a Windows program links on a Windows host: the C++ build is skipped");
        return;
    }
    if !(available("cmake") && available("clang")) {
        eprintln!("cmake or clang is not on the PATH: the C++ build is skipped");
        return;
    }
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin");
    let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "classes-cpp");
    std::fs::create_dir_all(dir.join("ref")).unwrap();
    std::fs::copy(fixtures.join("classes64.exe"), dir.join("ref/classes64.exe")).unwrap();
    let json = r#"{ "name": "classes", "exe": "ref/classes64.exe", "lang": "cpp", "build": { "generator": "cmake" } }"#;
    std::fs::write(dir.join("rhipper.json"), json).unwrap();
    for step in ["emit", "build"] {
        let output =
            Command::new(env!("CARGO_BIN_EXE_ras")).args(["pipeline", "--project"]).arg(&*dir).args(["--only", step]).output().unwrap();
        let log = std::fs::read_to_string(dir.join(format!("build/pipeline/{step}.log"))).unwrap_or_default();
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        assert!(
            output.status.success(),
            "the {step} step failed: {}{}\nthe end of its log, last line first:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            tail.join("\n")
        );
    }
    let gen_dir = dir.join("src/gen");
    let cpp = std::fs::read_dir(&gen_dir).unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "cpp"));
    assert!(cpp, "the translation under {} has no .cpp file", gen_dir.display());
    let lists = std::fs::read_to_string(dir.join("CMakeLists.txt")).unwrap();
    assert!(lists.contains("msvcprt"), "the CMakeLists.txt links no msvcprt:\n{lists}");
    let bin = dir.join("build/cmake/bin");
    assert!(
        bin.join("classes.exe").is_file() || bin.join("Release/classes.exe").is_file(),
        "the build linked no classes.exe under {}",
        bin.display()
    );
}

/// `scripts/build-port.ps1` builds the port `ras emit --out` wrote of `hello64` into one
/// executable that prints what the native run prints, and says so in one line; a port that does
/// not compile gives one line naming clang's first error. Without Windows or clang the build is
/// skipped and says why.
#[test]
fn the_build_port_script_builds_an_emitted_port_in_one_line() {
    let available = |tool: &str| Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success());
    if !cfg!(windows) {
        eprintln!("the port of a Windows program builds on a Windows host: the script is skipped");
        return;
    }
    if !available("clang") {
        eprintln!("clang is not on the PATH: the script is skipped");
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("scripts/build-port.ps1");
    let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "build-port");
    let emitted = Command::new(env!("CARGO_BIN_EXE_ras"))
        .arg("emit")
        .arg(root.join("testdata/bin/hello64.exe"))
        .arg("--out")
        .arg(dir.join("hello"))
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(emitted.status.success(), "the emit failed: {}", String::from_utf8_lossy(&emitted.stderr));
    let build = |name: &str| {
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg(name)
            .arg("-Out")
            .arg(&*dir)
            .arg("-DRA_BUILD_PORT_TEST")
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let (ok, line, stderr) = build("hello");
    let exe = dir.join("hello/hello.exe");
    assert_eq!(line, format!("built {}", exe.display()).replace('\\', "/"), "{stderr}");
    assert!(ok && exe.is_file(), "{line}\n{stderr}");
    let run = Command::new(&exe).output().unwrap();
    let expected = std::fs::read_to_string(root.join("testdata/expected/hello.txt")).unwrap();
    let lines = |text: &str| text.lines().map(str::trim_end).filter(|l| !l.is_empty()).map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(lines(&String::from_utf8_lossy(&run.stdout)), lines(&expected), "the port printed otherwise than the native run");

    std::fs::create_dir_all(dir.join("broken")).unwrap();
    std::fs::write(dir.join("broken/broken.c"), "int main(void) { return undeclared; }\n").unwrap();
    let (ok, line, stderr) = build("broken");
    assert!(!ok, "a port that does not compile built: {line}");
    assert!(line.starts_with("failed: ") && line.contains("undeclared") && !line.contains('\n'), "{line}\n{stderr}");
    // clang's own line, not the text Windows PowerShell wraps a native command's stderr in.
    assert!(line.contains("broken.c") && line.contains("error:") && !line.contains("NativeCommandError"), "{line}\n{stderr}");
}

/// The pipeline over the fixtures: the slow tier, which builds and runs the ports.
#[cfg(feature = "oracle")]
mod fixtures {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn available(tool: &str) -> bool {
        Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
    }

    /// The port of a Windows fixture builds with cmake and clang and runs as a Windows process: on
    /// another host, or without the tools, the build and the run skip and say why.
    fn port_builds_here() -> bool {
        if !cfg!(windows) {
            eprintln!("the port of a Windows program runs on a Windows host: the build and the run are skipped");
            return false;
        }
        if !(available("cmake") && available("clang")) {
            eprintln!("cmake or clang is not on the PATH: the build and the run are skipped");
            return false;
        }
        true
    }

    /// A project directory with the fixture as its reference binary, whose run step compares the
    /// port with it. The directory goes when the guard drops; `RA_KEEP_SCRATCH` keeps it for a
    /// person to read.
    fn project(name: &str) -> ra_asm_testing::Scratch {
        project_with(name, r#"{ "name": "control", "exe": "ref/control64.exe", "run": { "compare": true } }"#)
    }

    fn project_with(name: &str, rhipper_json: &str) -> ra_asm_testing::Scratch {
        let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", name);
        std::fs::create_dir_all(dir.join("ref")).unwrap();
        std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control64.exe")).unwrap();
        std::fs::write(dir.join("rhipper.json"), rhipper_json).unwrap();
        dir
    }

    fn ras(dir: &Path, args: &[&str]) -> (bool, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg("pipeline").arg("--project").arg(dir).args(args).output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        (output.status.success(), text)
    }

    /// The tests leave nothing new under the system temp: the scratch directories of this build that
    /// may exist while this one runs are this process's own and those a live guard holds; the runs of
    /// other checkouts, under way at the same time, are not looked at. A directory of another process
    /// written since this binary was built is a leftover of a test that wrote without a guard; an
    /// older one is from the time before the guards, and is only named. A run that keeps its scratch
    /// keeps it on purpose and is not checked; a failed check of this run keeps its own and is named
    /// here as well.
    #[test]
    fn the_tests_leave_nothing_under_temp() {
        if std::env::var_os(ra_asm_testing::KEEP).is_some() {
            eprintln!("{} is set: the scratch directories are kept, and not checked", ra_asm_testing::KEEP);
            return;
        }
        let built = std::fs::metadata(std::env::current_exe().unwrap()).unwrap().modified().unwrap();
        for suite in ["ra-asm-emit-tests", "ra-asm-lift-tests", "ras-pipeline-tests"] {
            let foreign = ra_asm_testing::Scratch::foreign(suite);
            let (new, old): (Vec<_>, Vec<_>) =
                foreign.into_iter().partition(|p| std::fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t > built));
            if !old.is_empty() {
                eprintln!("{suite}: {} directories left by earlier runs, to remove by hand", old.len());
            }
            assert!(
                new.is_empty(),
                "{suite}: left under the system temp since this test was built (or kept by a failed check or a run with {} set):\n{new:#?}",
                ra_asm_testing::KEEP
            );
        }
    }

    /// `ras doctor --json` lists the tools of this machine; clang is among them, with a version when
    /// it is on the PATH.
    #[test]
    fn doctor_lists_the_tools_as_json() {
        let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(["doctor", "--json"]).output().unwrap();
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let report: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
        let tools = report["tools"].as_array().unwrap();
        let clang = tools.iter().find(|t| t["name"] == "clang").unwrap_or_else(|| panic!("no clang in {text}"));
        assert_eq!(clang["for"], "the C compiler of a C or C++ port");
        assert_eq!(clang["required"], true);
        if available("clang") {
            assert!(clang["version"].as_str().is_some_and(|v| v.contains('.')), "{clang}");
        } else {
            assert!(clang["version"].is_null(), "{clang}");
        }
        for name in ["clang++", "lld", "cmake", "ninja", "visual studio", "cargo", "llvm-symbolizer"] {
            assert!(tools.iter().any(|t| t["name"] == name), "no {name} in {text}");
        }
        assert_eq!(report["ok"].as_bool().unwrap(), output.status.success());
    }

    /// With no tool on the PATH, the build step fails with the doctor's message, not `CMake`'s.
    #[test]
    fn the_build_step_names_the_missing_tools() {
        let dir = project("doctor");
        let output = Command::new(env!("CARGO_BIN_EXE_ras"))
            .args(["pipeline", "--project"])
            .arg(&dir)
            .args(["--only", "build"])
            .env("PATH", "")
            .env_remove("ProgramFiles(x86)")
            .env_remove("ProgramFiles")
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("missing: clang (") && text.contains("cmake (") && text.contains("] build failed: stopping"), "{text}");
    }

    #[test]
    fn emit_writes_the_translation_and_the_table() {
        let dir = project("emit");
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        assert!(text.contains("] emit\n") && text.contains("] done\n"), "{text}");
        for file in [
            "src/gen/program.c",
            "src/gen/image.h",
            "src/gen/runtime/rhipper.h",
            "src/gen/runtime/driver.c",
            "build/functions.json",
            "build/pipeline/emit.log",
        ] {
            assert!(dir.join(file).is_file(), "{file} is missing");
        }
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["name"] == "entry" && e["status"] == "lifted")), "{table}");
    }

    /// A second run over unchanged inputs skips the steps, each in under a second, and leaves the
    /// translation as it was; a deleted output, or a change to `rhipper.json`, runs them again.
    #[test]
    fn an_unchanged_program_is_not_lifted_again() {
        let dir = project("incremental");
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        assert!(!text.contains("(unchanged)"), "{text}");
        let program = dir.join("src/gen/program.c");
        let written = std::fs::metadata(&program).unwrap().modified().unwrap();
        for step in ["analyze", "lift", "emit"] {
            let (ok, text) = ras(&dir, &["--only", step]);
            assert!(ok, "{text}");
            if step == "emit" {
                assert!(text.contains("(unchanged)"), "{text}");
            }
        }
        for step in ["analyze", "lift", "emit"] {
            let (ok, text) = ras(&dir, &["--only", step]);
            assert!(ok && text.contains("(unchanged)"), "{text}");
            assert!(text.contains("[   0s] done"), "an unchanged {step} took a second or more:\n{text}");
        }
        assert_eq!(std::fs::metadata(&program).unwrap().modified().unwrap(), written, "the emit rewrote an unchanged file");
        std::fs::remove_file(dir.join("src/gen/runtime/driver.c")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok && !text.contains("(unchanged)"), "the emit missed a deleted output:\n{text}");
        assert!(dir.join("src/gen/runtime/driver.c").is_file(), "the deleted output was not written again");
        std::fs::write(dir.join("rhipper.json"), r#"{ "name": "control", "exe": "ref/control64.exe", "x87": "double" }"#).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok && !text.contains("(unchanged)"), "{text}");
        assert_eq!(std::fs::metadata(&program).unwrap().modified().unwrap(), written, "the same translation was written again");
    }

    /// A `map` in `rhipper.json` names the reference's functions; the map of another build is refused
    /// with its path.
    #[test]
    fn the_project_map_names_the_functions() {
        let dir = project_with("map", r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/control.map" }"#);
        std::fs::copy(root().join("testdata/bin/control64.map"), dir.join("ref/control.map")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["name"] == "mainCRTStartup_")), "{table}");

        let dir = project_with("wrong-map", r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/core.map" }"#);
        std::fs::copy(root().join("testdata/rhtest/core-O2.map"), dir.join("ref/core.map")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "analyze"]);
        assert!(!ok && text.contains("core.map"), "{text}");
    }

    #[test]
    fn the_whole_pipeline_builds_and_runs_the_port() {
        if !port_builds_here() {
            return;
        }
        let dir = project("all");
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        for step in ["analyze", "lift", "emit", "build", "run"] {
            assert!(text.contains(&format!("] {step}\n")), "no progress line for {step}:\n{text}");
            assert!(dir.join(format!("build/pipeline/{step}.log")).is_file(), "no {step}.log");
        }
        assert!(text.ends_with("] done\n"), "{text}");
        assert!(dir.join("CMakeLists.txt").is_file() && dir.join("build/program.ll").is_file());
        let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert_eq!(record["exit"], "0", "{record}");
        assert_eq!(record["alive"], false);
        assert!(record["when"].as_str().is_some_and(|w| w.ends_with('Z')));
        // The reference ran beside the port and both printed the same.
        assert_eq!(record["verdict"], "same", "{record}");
        assert_eq!(record["reference_exit"], "0", "{record}");
        assert!(record["differs_at"].is_null(), "{record}");
        assert!(text.contains("verdict: same"), "{text}");
        let reference_log = std::fs::read_to_string(dir.join("build/pipeline/reference.log")).unwrap().replace("\r\n", "\n");
        assert!(reference_log.contains(expected.trim_end()), "the reference log:\n{reference_log}");
        // A second run reuses the configured build directory.
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
    }

    /// Runs the whole pipeline over control with `build.generator` set to `generator`, and checks
    /// that the build wrote `files` at the root and the port printed what the reference prints.
    fn port_through(generator: &str, files: &[&str]) {
        let dir = project_with(
            &format!("generator-{generator}"),
            &format!(
                r#"{{ "name": "control", "exe": "ref/control64.exe", "build": {{ "generator": "{generator}" }}, "run": {{ "compare": true }} }}"#
            ),
        );
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        for file in files {
            assert!(dir.join(file).is_file(), "the {generator} build wrote no {file}:\n{text}");
        }
        assert!(dir.join("build").join(generator).is_dir(), "no build/{generator}:\n{text}");
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert_eq!(record["exit"], "0", "{record}");
        assert_eq!(record["verdict"], "same", "{record}");
        // A second build reuses what the first wrote and configured.
        let (ok, text) = ras(&dir, &["--only", "build"]);
        assert!(ok, "{text}");
    }

    /// The port of control built through the generator `rhipper.json` names: a Visual Studio solution
    /// built by `msbuild` when it is on the PATH, else the `CMake` project.
    #[test]
    fn the_pipeline_builds_through_the_generator_of_the_project() {
        let msbuild = Command::new("msbuild").args(["-version", "-nologo"]).output().is_ok_and(|o| o.status.success());
        if msbuild {
            port_through("vs", &["control.sln", "control.vcxproj"]);
        } else if available("cmake") && available("clang") {
            eprintln!("msbuild is not on the PATH: the port is built through CMake");
            port_through("cmake", &["CMakeLists.txt"]);
        } else {
            eprintln!("neither msbuild nor cmake and clang are on the PATH: the build and the run are skipped");
        }
    }

    /// The host's clang alone builds the port, with no build system, as on a Linux or macOS machine
    /// that has nothing else.
    #[test]
    fn the_host_clang_builds_the_port_without_a_build_system() {
        if !available("clang") {
            eprintln!("clang is not on the PATH: the build and the run are skipped");
            return;
        }
        port_through("clang", &[]);
    }

    /// A Meson project, when Meson is on the PATH.
    #[test]
    fn meson_builds_the_port() {
        if !(available("meson") && available("ninja") && available("clang")) {
            eprintln!("meson, ninja or clang is not on the PATH: the build and the run are skipped");
            return;
        }
        port_through("meson", &["meson.build", "build/meson-sources.txt"]);
    }

    /// Sends `requests` to a server over the project at `dir` and returns the responses, in order.
    fn serve(dir: &Path, requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
        use std::io::{BufRead, BufReader, Write};
        let mut child = Command::new(env!("CARGO_BIN_EXE_ras"))
            .args(["serve", "--project"])
            .arg(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
        writeln!(stdin, r#"{{"jsonrpc": "2.0", "id": 999, "method": "shutdown"}}"#).unwrap();
        drop(stdin);
        let responses = BufReader::new(stdout).lines().map(|l| serde_json::from_str(&l.unwrap()).unwrap()).collect();
        assert!(child.wait().unwrap().success());
        responses
    }

    /// With `run.trace`, the port records its calls and returns with their values and the reference's
    /// are recorded under the debugger at the same functions; the two compare the same. A port with
    /// one wrong function (`fib` returning one more) is found by the comparison in one run, which
    /// names the function; the `trace` method of `ras serve` replays the two call by call and names
    /// the function, the call and the two values; and the reference recorded the other way, by the
    /// interpreter over its IR, replays over the same port to the same function and value.
    #[test]
    fn the_trace_beside_the_port_names_the_wrong_function() {
        if !port_builds_here() {
            return;
        }
        let dir = project_with(
            "trace",
            r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/control.map", "run": { "compare": true, "trace": true } }"#,
        );
        std::fs::copy(root().join("testdata/bin/control64.map"), dir.join("ref/control.map")).unwrap();
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        assert!(text.contains("verdict: same; trace: same, "), "{text}");
        let port_trace = std::fs::read_to_string(dir.join("build/pipeline/port.trace")).unwrap();
        let reference_trace = std::fs::read_to_string(dir.join("build/pipeline/reference.trace")).unwrap();
        for trace in [&port_trace, &reference_trace] {
            assert!(trace.contains("> fib(0xf") && trace.contains("< fib = 0x262") && trace.contains("> classify("), "{trace}");
        }
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert!(record["trace"].as_str().is_some_and(|t| t.starts_with("same, ")), "{record}");
        assert!(record["trace_function"].is_null() && record["trace_event"].is_null(), "{record}");

        // One wrong function: fib's result off by one, in the port's C, then the build and the run.
        let program = dir.join("src/gen/program.c");
        let c = std::fs::read_to_string(&program).unwrap();
        let patched = c.replacen("ra_trace_ret(\"fib\", (uint64_t)(", "ra_trace_ret(\"fib\", 1 + (uint64_t)(", 1);
        assert_ne!(c, patched, "no traced return of fib in:\n{c}");
        std::fs::write(&program, patched).unwrap();
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        assert!(text.contains("; trace: parts at event ") && text.contains(" in fib: the port returned 0x"), "{text}");
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert_eq!(record["trace_function"], "fib", "{record}");
        assert!(record["trace_event"].as_u64().is_some_and(|e| e > 0), "{record}");
        assert_eq!(record["tracer"], "debugger", "{record}");
        assert!(record["verdict"].as_str().is_some_and(|v| v.starts_with("differs at line")), "{record}");

        // The replay in the workbench: the call at fault is a `fib` whose port result is one more
        // than the reference's, and the calls of `fib` alone step side by side with their arguments.
        let responses = serve(
            &dir,
            &[
                serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "trace"}),
                serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "trace", "params": {"function": "fib", "limit": 3}}),
            ],
        );
        let replay = &responses[0]["result"];
        assert_eq!(replay["tracer"], "debugger", "{replay}");
        assert!(replay["verdict"].as_str().is_some_and(|v| v.starts_with("parts at event")), "{replay}");
        let parts = &replay["parts"];
        assert_eq!(parts["function"], "fib", "{parts}");
        let (port, reference) = (parts["port"].as_str().unwrap(), parts["reference"].as_str().unwrap());
        let value = |text: &str| u64::from_str_radix(text.trim_start_matches("0x").split(':').next().unwrap(), 16).unwrap();
        assert_eq!(value(port), value(reference) + 1, "{parts}");
        let call = &parts["call"];
        assert_eq!(call["function"], "fib", "{call}");
        assert_eq!(call["fault"], true, "{call}");
        assert!(call["port"]["args"].as_str().is_some_and(|a| a.starts_with("0x")), "{call}");
        assert_eq!(call["port"]["result"], port, "{call}");
        assert!(call["reference"]["args"].as_str().is_some_and(|a| a.starts_with("0x")), "{call}");
        assert!(
            replay["functions"].as_array().is_some_and(|f| f.iter().any(|e| e["name"] == "fib" && e["calls"].as_u64() > Some(1))),
            "{replay}"
        );
        let fibs = &responses[1]["result"];
        assert_eq!(fibs["calls"].as_array().map(Vec::len), Some(3), "{fibs}");
        assert!(
            fibs["calls"].as_array().unwrap().iter().all(|c| c["function"] == "fib" && c["enter"].as_u64() < c["return"].as_u64()),
            "{fibs}"
        );
        assert!(fibs["total"].as_u64() > Some(3), "{fibs}");

        // The reference recorded the other way, by the interpreter over its IR, replayed over the
        // same port: the same function and the same value.
        std::fs::write(
        dir.join("rhipper.json"),
        r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/control.map", "run": { "compare": true, "trace": true, "tracer": "interpreter" } }"#,
    )
    .unwrap();
        let (ok, text) = ras(&dir, &["--only", "run"]);
        assert!(ok, "{text}");
        assert!(text.contains(" in fib: the port returned 0x") && text.contains(" (interpreter)"), "{text}");
        let reference_trace = std::fs::read_to_string(dir.join("build/pipeline/reference.trace")).unwrap();
        assert!(reference_trace.contains("> fib(0xf)") && reference_trace.contains("< fib = 0x262:32"), "{reference_trace}");
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert_eq!(record["tracer"], "interpreter", "{record}");
        assert_eq!(record["trace_function"], "fib", "{record}");
        let responses = serve(&dir, &[serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "trace"})]);
        let parts = &responses[0]["result"]["parts"];
        assert_eq!(responses[0]["result"]["tracer"], "interpreter", "{}", responses[0]);
        assert_eq!(parts["function"], "fib", "{parts}");
        assert_eq!(parts["port"], port, "{parts}");
        assert_eq!(parts["reference"], reference, "{parts}");
    }

    /// The lockstep of `ras serve`: the port of control64 (a build with `run.trace`) and the reference
    /// through the interpreter, each stopped at its first function entry, then let go to the first
    /// entry of `fib`, where both stand with equal states; a read answers from each side's memory; and
    /// a run to the first divergence takes both to their end with the same exit.
    #[test]
    fn the_lockstep_stops_the_port_and_the_reference_at_the_same_entry() {
        if !port_builds_here() {
            return;
        }
        let dir = project_with(
            "lockstep",
            r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/control.map", "run": { "trace": true } }"#,
        );
        std::fs::copy(root().join("testdata/bin/control64.map"), dir.join("ref/control.map")).unwrap();
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        let responses = serve(
            &dir,
            &[
                serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "lockstep.state"}),
                serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "lockstep.start"}),
                serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "lockstep.step", "params": {"function": "fib"}}),
                serde_json::json!({"jsonrpc": "2.0", "id": 4, "method": "lockstep.read", "params": {"address": "0x0", "size": 8}}),
                serde_json::json!({"jsonrpc": "2.0", "id": 5, "method": "lockstep.step", "params": {"divergence": true}}),
                serde_json::json!({"jsonrpc": "2.0", "id": 6, "method": "lockstep.stop"}),
            ],
        );
        assert!(responses[0]["error"]["message"].as_str().is_some_and(|m| m.contains("no lockstep session")), "{}", responses[0]);
        let started = &responses[1]["result"];
        assert_eq!(started["entry"], 1, "{}", responses[1]);
        assert_eq!(started["equal"], true, "{started}");
        assert!(started["port"]["function"].is_string() && started["port"]["function"] == started["reference"]["function"], "{started}");

        let at_fib = &responses[2]["result"];
        assert_eq!(at_fib["equal"], true, "{}", responses[2]);
        for side in ["port", "reference"] {
            assert_eq!(at_fib[side]["function"], "fib", "{at_fib}");
            assert_eq!(at_fib[side]["args"][0]["kind"], "u", "{at_fib}");
            assert_eq!(at_fib[side]["args"][0]["value"], "0xf", "{at_fib}");
        }
        assert!(at_fib["entry"].as_u64() > Some(1), "{at_fib}");

        // Nothing is mapped at 0 on either side: each says why.
        let read = &responses[3]["result"];
        for side in ["port", "reference"] {
            assert!(read[side]["error"].is_string(), "{}", responses[3]);
        }

        let end = &responses[4]["result"];
        assert_eq!(end["equal"], true, "{}", responses[4]);
        for side in ["port", "reference"] {
            assert_eq!(end[side]["ended"], true, "{end}");
            assert_eq!(end[side]["exit"], "0x0", "{end}");
        }
        assert_eq!(responses[5]["result"]["stopped"], true, "{}", responses[5]);
    }

    /// A function taken over by hand: `hand` in `rhipper.json` names `fib`, the emit step writes its
    /// declaration in place of its body and marks it in the table, the build compiles the hand-written
    /// `fib` from `sources` beside the translation, and the port prints what the reference prints.
    #[test]
    fn the_pipeline_leaves_a_hand_written_function_to_the_sources() {
        let dir = project_with(
            "hand",
            r#"{ "name": "control", "exe": "ref/control64.exe", "map": "ref/control.map", "sources": ["src"], "hand": ["fib", "nobody"],
             "run": { "compare": true } }"#,
        );
        std::fs::copy(root().join("testdata/bin/control64.map"), dir.join("ref/control.map")).unwrap();
        std::fs::create_dir_all(dir.join("src/hand")).unwrap();
        std::fs::write(
            dir.join("src/hand/fib.c"),
            "#include <stdint.h>\n\nuint32_t fib(uint32_t n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }\n",
        )
        .unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let c = std::fs::read_to_string(dir.join("src/gen/program.c")).unwrap();
        assert!(c.contains("// fib at 0x1400013f0: hand-written in the port's sources\nuint32_t fib(uint32_t arg0);\n"), "{c}");
        assert!(!c.contains("uint32_t fib(uint32_t arg0) {"), "{c}");
        let log = std::fs::read_to_string(dir.join("build/pipeline/emit.log")).unwrap();
        assert!(log.contains("hand-written function `nobody` is no function of the image"), "{log}");
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        let fib = table.as_array().unwrap().iter().find(|e| e["name"] == "fib").unwrap_or_else(|| panic!("no fib in {table}"));
        assert_eq!(fib["status"], "hand", "{fib}");
        assert_eq!(fib["file"], "program.c", "{fib}");
        let line = fib["line"].as_u64().unwrap() as usize;
        assert_eq!(c.lines().nth(line - 1), Some("uint32_t fib(uint32_t arg0);"), "{fib}");
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        assert!(text.contains("verdict: same"), "{text}");
        let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
    }

    /// The round trip: a port of the `-O1` build of control, with `fib` taken over by hand and named
    /// and `classify` named and commented in the sidecar files, regenerated from the `-O2` build put
    /// in its place. Every function moves; the annotations find theirs by identity, not by address,
    /// so the regenerated port keeps the hand-written `fib` and both names, the emit step shows what
    /// moved, and the port prints what the new reference prints.
    #[test]
    fn a_regeneration_from_a_new_build_keeps_the_hand_written_function_and_the_names() {
        let dir = project_with(
            "round-trip",
            r#"{ "name": "control", "exe": "ref/control.exe", "sources": ["src"], "run": { "compare": true } }"#,
        );
        std::fs::copy(root().join("testdata/bin/control64-O1.exe"), dir.join("ref/control.exe")).unwrap();
        let annotations = dir.join("rhipper/annotations");
        std::fs::create_dir_all(&annotations).unwrap();
        std::fs::write(
            annotations.join("0x1400012b0.json"),
            "{\n  \"addr\": \"0x1400012b0\",\n  \"name\": \"fib\",\n  \"hand\": true\n}\n",
        )
        .unwrap();
        std::fs::write(
            annotations.join("0x1400012e0.json"),
            "{\n  \"addr\": \"0x1400012e0\",\n  \"name\": \"classify\",\n  \"comment\": \"the dense switch\"\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src/hand")).unwrap();
        std::fs::write(
            dir.join("src/hand/fib.c"),
            "#include <stdint.h>\n\nuint32_t fib(uint32_t n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }\n",
        )
        .unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        assert!(!text.contains("regenerated:"), "the first emit has nothing to compare with:\n{text}");
        let c = std::fs::read_to_string(dir.join("src/gen/program.c")).unwrap();
        assert!(c.contains("// fib at 0x1400012b0: hand-written in the port's sources\n"), "{c}");
        let fib = std::fs::read_to_string(annotations.join("0x1400012b0.json")).unwrap();
        assert!(fib.contains("\"identity\": {"), "the first emit keeps the function's identity in its file:\n{fib}");

        // The new build of the reference, at the same path.
        std::fs::copy(root().join("testdata/bin/control64.exe"), dir.join("ref/control.exe")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        assert!(text.contains("regenerated: "), "{text}");
        for (old, new, name) in [("0x1400012b0", "0x1400013f0", "fib"), ("0x1400012e0", "0x140001430", "classify")] {
            assert!(!annotations.join(format!("{old}.json")).exists(), "the annotation of {name} stayed at {old}");
            let moved = std::fs::read_to_string(annotations.join(format!("{new}.json"))).unwrap_or_else(|e| panic!("{name} at {new}: {e}"));
            assert!(moved.contains(&format!("\"addr\": \"{new}\"")) && moved.contains(&format!("\"name\": \"{name}\"")), "{moved}");
        }
        let log = std::fs::read_to_string(dir.join("build/pipeline/emit.log")).unwrap();
        assert!(log.contains("moves to 0x1400013f0 with its function") && log.contains("moves to 0x140001430 with its function"), "{log}");
        assert!(!log.contains("applied to nothing"), "{log}");
        let c = std::fs::read_to_string(dir.join("src/gen/program.c")).unwrap();
        assert!(c.contains("// fib at 0x1400013f0: hand-written in the port's sources\nuint32_t fib(uint32_t arg0);\n"), "{c}");
        assert!(c.contains("classify("), "{c}");
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        let entry = |name: &str| {
            table.as_array().unwrap().iter().find(|e| e["name"] == name).unwrap_or_else(|| panic!("no {name} in {table}")).clone()
        };
        assert_eq!(entry("fib")["status"], "hand", "{table}");
        let classify = entry("classify");
        assert_eq!(classify["annotated"], true, "{classify}");
        assert_eq!(classify["comment"], "the dense switch", "{classify}");

        // What moved, for the person to read.
        let regenerated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("build/regenerated.json")).unwrap()).unwrap();
        assert!(regenerated["moved"].as_u64().is_some_and(|n| n >= 2), "{regenerated}");
        let fib = regenerated["entries"].as_array().unwrap().iter().find(|e| e["name"] == "fib").unwrap_or_else(|| panic!("{regenerated}"));
        assert_eq!(fib["change"], "moved", "{fib}");
        assert_eq!(fib["was"], "0x1400012b0", "{fib}");
        assert_eq!(fib["addr"], "0x1400013f0", "{fib}");
        assert!(dir.join("build/regenerated.diff").is_file());

        if !(available("cmake") && available("clang")) {
            eprintln!("cmake or clang is not on the PATH: the build and the run are skipped");
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        assert!(text.contains("verdict: same"), "{text}");
    }

    /// A program with resources: the emit step writes them beside the port (`resources.res` for the
    /// link, the string table decoded, the other resources as files), and the port CMake builds
    /// finds them through the calls the program made.
    #[test]
    fn the_pipeline_ports_a_program_with_resources() {
        let dir = project_with("resources", r#"{ "name": "resources", "exe": "ref/resources64.exe", "run": { "compare": true } }"#);
        std::fs::copy(root().join("testdata/bin/resources64.exe"), dir.join("ref/resources64.exe")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let strings = std::fs::read_to_string(dir.join("src/gen/resources/strings.txt")).unwrap();
        assert_eq!(strings, "1\thello from the string table\n2\ta second string\n3\tthe third, with a tab\\tand quotes \"here\"\n");
        assert_eq!(std::fs::read(dir.join("src/gen/resources/RCDATA/100.bin")).unwrap(), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(dir.join("src/gen/resources.res").is_file());
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        assert!(text.contains("verdict: same"), "{text}");
    }

    /// A program whose resources hold a manifest, as most of a game's do: the port CMake builds
    /// links the program's and adds none of its own, which would be a second `MANIFEST` 1 and
    /// fail the link ("duplicate resource").
    #[test]
    fn the_port_of_a_program_with_a_manifest_links_its_own_alone() {
        let dir = project_with("manifest", r#"{ "name": "manifest", "exe": "ref/resources64.exe", "run": { "compare": true } }"#);
        std::fs::copy(root().join("testdata/bin/resources64.exe"), dir.join("ref/resources64.exe")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        if !port_builds_here() {
            return;
        }
        // The fixture's resources with a manifest added: type 24, name 1, US English.
        let manifest = br#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"></assembly>"#;
        let res = dir.join("src/gen/resources.res");
        let mut bytes = std::fs::read(&res).unwrap();
        bytes.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 0xff, 24, 0, 0xff, 0xff, 1, 0]);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0x1030u16.to_le_bytes());
        bytes.extend_from_slice(&0x409u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(manifest);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        std::fs::write(&res, bytes).unwrap();
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        assert!(text.contains("verdict: same"), "{text}");
    }

    /// A DLL ports as a DLL: the emit step writes the image's exports (by name and ordinal, the data
    /// export, the forwarder) as the definition file of the port and marks `image.h` with `RA_DLL`,
    /// the build makes a shared library, and the run step starts `run.exe`, the host that loads it,
    /// with the port's directory first on its DLL search and the reference's for the comparison; the
    /// host prints the same over both.
    #[test]
    fn the_pipeline_ports_a_dll() {
        let dir = project_with(
            "dll",
            r#"{ "name": "exports", "exe": "ref/exports64.dll", "build": { "target": "exports64" },
             "run": { "exe": "exports_host64.exe", "args": ["exports64.dll"], "compare": true } }"#,
        );
        std::fs::copy(root().join("testdata/bin/exports64.dll"), dir.join("ref/exports64.dll")).unwrap();
        std::fs::copy(root().join("testdata/bin/exports_host64.exe"), dir.join("exports_host64.exe")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let def = std::fs::read_to_string(dir.join("src/gen/exports.def")).unwrap();
        let (_, exports) = def.split_once("EXPORTS\n").unwrap_or_else(|| panic!("no EXPORTS in:\n{def}"));
        assert_eq!(
            exports,
            "    ordinal_20 @20 NONAME\n    ra_add @21\n    ra_counter = sec_data @22 DATA\n    ra_tick = KERNEL32.GetTickCount @23\n    ra_twice @24\n"
        );
        assert!(std::fs::read_to_string(dir.join("src/gen/image.h")).unwrap().contains("#define RA_DLL 1\n"));
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        let bin = dir.join("build/cmake/bin");
        assert!(
            bin.join("exports64.dll").is_file() || bin.join("Release/exports64.dll").is_file(),
            "no exports64.dll under {}",
            bin.display()
        );
        assert!(text.contains("verdict: same"), "{text}");
        let expected = std::fs::read_to_string(root().join("testdata/expected/exports.txt")).unwrap();
        for log in ["run.log", "reference.log"] {
            let text = std::fs::read_to_string(dir.join("build/pipeline").join(log)).unwrap().replace("\r\n", "\n");
            assert!(text.contains(expected.trim_end()), "{log}:\n{text}");
        }
    }

    /// The 32-bit DLL ports as a 64-bit DLL that keeps the program's 32-bit pointers: the 64-bit host
    /// built without large-address awareness loads it and calls its exports as a 64-bit program
    /// would. The reference needs a 32-bit host, so the run is compared with the native output only.
    #[test]
    fn the_pipeline_ports_a_32_bit_dll() {
        let dir = project_with(
            "dll32",
            r#"{ "name": "exports", "exe": "ref/exports32.dll", "build": { "target": "exports32" },
             "run": { "exe": "exports_host64.exe", "args": ["exports32.dll"] } }"#,
        );
        std::fs::copy(root().join("testdata/bin/exports32.dll"), dir.join("ref/exports32.dll")).unwrap();
        std::fs::copy(root().join("testdata/bin/exports_host64.exe"), dir.join("exports_host64.exe")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let def = std::fs::read_to_string(dir.join("src/gen/exports.def")).unwrap();
        assert!(def.contains("    ra_add @21\n") && def.contains("    ra_counter = sec_data @22 DATA\n"), "{def}");
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        let expected = std::fs::read_to_string(root().join("testdata/expected/exports.txt")).unwrap();
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
    }

    /// The static mipsel control program, an o32 Linux executable whose `write` and `exit` are
    /// `syscall`s, ports end to end: the port keeps its 32-bit pointers, the runtime's `ra_syscall`
    /// answers the two calls, and the run prints what the source prints. The reference is a MIPS
    /// program this host cannot run, so the run is compared with the source's output only.
    #[test]
    fn the_pipeline_ports_a_mips_program() {
        let dir = project_with("mips", r#"{ "name": "control", "exe": "ref/control" }"#);
        std::fs::copy(root().join("testdata/mips/control"), dir.join("ref/control")).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        let image_h = std::fs::read_to_string(dir.join("src/gen/image.h")).unwrap();
        assert!(image_h.contains("#define RA_POINTER_SIZE 4\n") && !image_h.contains("RA_WIN32"), "{image_h}");
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "build"]);
        assert!(ok, "{text}");
        let expected = std::fs::read_to_string(root().join("testdata/mips/control.txt")).unwrap();
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
        let runs = std::fs::read_to_string(dir.join("build/pipeline/runs.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(runs.lines().last().unwrap()).unwrap();
        assert_eq!(record["exit"], "0", "{record}");
    }

    /// A fixture packed by UPX ports through the unpack step: the pipeline unpacks it first into
    /// `build/unpacked`, the steps after read what the stub unpacked, and the port prints what the
    /// unpacked twin, `<name>.exe`, prints when it runs natively. A second run finds the unpack done.
    fn port_packed(name: &str) {
        let exe = format!("{name}-upx.exe");
        let dir =
            project_with(&format!("upx-{name}"), &format!(r#"{{ "name": "{name}", "exe": "ref/{exe}", "run": {{ "compare": true }} }}"#));
        std::fs::copy(root().join("testdata/bin").join(&exe), dir.join("ref").join(&exe)).unwrap();
        let (ok, text) = ras(&dir, &["--only", "emit"]);
        assert!(ok, "{text}");
        assert!(text.contains("] unpack\n") && text.contains("unpacked by its UPX stub"), "no unpack step:\n{text}");
        assert!(dir.join("build/unpacked").join(&exe).is_file(), "no unpacked file:\n{text}");
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["status"] == "lifted")), "{table}");
        if !port_builds_here() {
            return;
        }
        let (ok, text) = ras(&dir, &["--from", "unpack"]);
        assert!(ok, "{text}");
        assert!(
            text.lines().any(|l| l.starts_with("unpacked by its UPX stub") && l.ends_with("(unchanged)")),
            "the unpack ran again over the same reference:\n{text}"
        );
        assert!(text.contains("verdict: same"), "{text}");
        let twin = Command::new(root().join("testdata/bin").join(format!("{name}.exe"))).output().unwrap();
        let expected = String::from_utf8_lossy(&twin.stdout).replace("\r\n", "\n");
        assert!(!expected.trim().is_empty(), "{name}.exe printed nothing");
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log beside {name}.exe's output:\n{run_log}\n---\n{expected}");
    }

    #[test]
    fn the_pipeline_ports_hello32_upx() {
        port_packed("hello32");
    }

    #[test]
    fn the_pipeline_ports_hello64_upx() {
        port_packed("hello64");
    }

    #[test]
    fn the_pipeline_ports_control32_upx() {
        port_packed("control32");
    }

    /// A Rust project: the emit step writes a cargo package, the build step runs cargo, and the port
    /// prints what the reference prints.
    #[test]
    fn the_pipeline_builds_a_rust_port() {
        if !cfg!(windows) {
            eprintln!("the port of a Windows program runs on a Windows host: the Rust pipeline is skipped");
            return;
        }
        if !available("cargo") {
            eprintln!("cargo is not on the PATH: the Rust pipeline is skipped");
            return;
        }
        let dir = project_with("rust", r#"{ "name": "control", "exe": "ref/control64.exe", "lang": "rust" }"#);
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        for file in [
            "src/gen/Cargo.toml",
            "src/gen/src/main.rs",
            "src/gen/src/rhipper.rs",
            "src/gen/src/data.rs",
            "src/gen/src/program.rs",
            "build/functions.json",
        ] {
            assert!(dir.join(file).is_file(), "{file} is missing");
        }
        let table: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/functions.json")).unwrap()).unwrap();
        assert!(table.as_array().is_some_and(|a| a.iter().any(|e| e["file"] == "src/program.rs")), "{table}");
        assert!(dir.join("build/cargo/release/control.exe").is_file(), "no port under build/cargo");
        let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(run_log.contains(expected.trim_end()), "the run log:\n{run_log}");
    }

    /// The residual engine's static library of this build: cargo builds it beside the rlib that
    /// `ras` links, under `deps` with the build's hash, and uplifts it when it builds the crate itself.
    fn residual_library() -> Option<PathBuf> {
        let bin = Path::new(env!("CARGO_BIN_EXE_ras")).parent()?.to_path_buf();
        let uplifted = bin.join("ra_asm_residual.lib");
        if uplifted.is_file() {
            return Some(uplifted);
        }
        std::fs::read_dir(bin.join("deps"))
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("ra_asm_residual-"))
                    && p.extension().is_some_and(|x| x == "lib")
            })
            .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
    }

    /// computed's port with the residual engine linked (`build.residual`): the three functions the
    /// program reaches only through addresses it computes have no lifted body, and the port's calls
    /// of them run in the engine over the reference's bytes; the port prints what the reference
    /// prints, says on stderr that the engine entered three entries, and lists them in
    /// `build/residual.json`.
    #[test]
    fn computed_runs_with_residue() {
        if !port_builds_here() {
            return;
        }
        let library = residual_library().expect("no static library of ra-asm-residual beside the ras of this build");
        let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "residual");
        std::fs::create_dir_all(dir.join("ref")).unwrap();
        std::fs::copy(root().join("testdata/bin/computed64.exe"), dir.join("ref/computed64.exe")).unwrap();
        let json = serde_json::json!({
            "name": "computed",
            "exe": "ref/computed64.exe",
            "build": { "residual": library.to_string_lossy() },
        });
        std::fs::write(dir.join("rhipper.json"), json.to_string()).unwrap();
        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        let expected = std::fs::read_to_string(root().join("testdata/expected/computed.txt")).unwrap().replace("\r\n", "\n");
        let run_log = std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");
        assert!(without_residual(&run_log).contains(expected.trim_end()), "the run log:\n{run_log}");
        assert!(run_log.contains("residual: 3 entered"), "the run log:\n{run_log}");
        let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/residual.json")).unwrap()).unwrap();
        assert_eq!(facts["residual"].as_array().map(Vec::len), Some(3), "{facts}");
    }

    /// The loop closes over computed: the first run leaves the three entries to the residual engine,
    /// the pipeline takes them into `build/facts.json`, and the second run's analysis seeds them as
    /// functions, so the rebuilt port lifts them and enters the engine for none.
    #[test]
    fn computed_converges() {
        if !port_builds_here() {
            return;
        }
        let library = residual_library().expect("no static library of ra-asm-residual beside the ras of this build");
        let dir = ra_asm_testing::Scratch::new("ras-pipeline-tests", "converges");
        std::fs::create_dir_all(dir.join("ref")).unwrap();
        std::fs::copy(root().join("testdata/bin/computed64.exe"), dir.join("ref/computed64.exe")).unwrap();
        let json = serde_json::json!({
            "name": "computed",
            "exe": "ref/computed64.exe",
            "build": { "residual": library.to_string_lossy() },
        });
        std::fs::write(dir.join("rhipper.json"), json.to_string()).unwrap();
        let expected = std::fs::read_to_string(root().join("testdata/expected/computed.txt")).unwrap().replace("\r\n", "\n");
        let run_log = || std::fs::read_to_string(dir.join("build/pipeline/run.log")).unwrap().replace("\r\n", "\n");

        let (ok, first_text) = ras(&dir, &[]);
        assert!(ok, "{first_text}");
        let first = run_log();
        assert!(first.contains("residual: 3 entered"), "the first run log:\n{first}");
        let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/facts.json")).unwrap()).unwrap();
        assert_eq!(facts["residual"].as_array().map(Vec::len), Some(3), "{facts}");

        let (ok, text) = ras(&dir, &[]);
        assert!(ok, "{text}");
        assert!(!text.contains("(unchanged)"), "the second run did not analyze the facts again:\n{text}");
        let second = run_log();
        assert!(without_residual(&second).contains(expected.trim_end()), "the second run log:\n{second}");
        assert!(second.contains("residual: 0 entered"), "the second run log:\n{second}");
        // The facts keep the entries the second run no longer left.
        let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("build/facts.json")).unwrap()).unwrap();
        assert_eq!(facts["residual"].as_array().map(Vec::len), Some(3), "{facts}");
        assert_eq!(analyzed(&text), analyzed(&first_text) + 3, "the three entries are not functions of the second analysis:\n{text}");
    }

    /// A run log without the residual engine's line: it goes to the port's stderr, which the log
    /// reads on its own thread, so it can land between any two lines of the standard output.
    fn without_residual(log: &str) -> String {
        log.lines().filter(|l| !l.starts_with("residual: ")).collect::<Vec<_>>().join("\n")
    }

    /// The count of functions the analyze step of a pipeline's output found.
    fn analyzed(text: &str) -> usize {
        let summary =
            text.split("] analyze\n").nth(1).and_then(|rest| rest.lines().next()).unwrap_or_else(|| panic!("no analyze step:\n{text}"));
        summary.split(' ').next().and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no count of functions in `{summary}`"))
    }
}
