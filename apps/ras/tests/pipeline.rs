//! `ras pipeline` over a project made of one fixture: every step runs, every output lands where
//! the workbench looks for it, and the port prints what the reference prints.

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
/// keeps it on purpose and is not checked.
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
            "{suite}: left under the system temp since this test was built (or kept by a run with {} set):\n{new:#?}",
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
    let dir =
        project_with("round-trip", r#"{ "name": "control", "exe": "ref/control.exe", "sources": ["src"], "run": { "compare": true } }"#);
    std::fs::copy(root().join("testdata/bin/control64-O1.exe"), dir.join("ref/control.exe")).unwrap();
    let annotations = dir.join("rhipper/annotations");
    std::fs::create_dir_all(&annotations).unwrap();
    std::fs::write(annotations.join("0x1400012b0.json"), "{\n  \"addr\": \"0x1400012b0\",\n  \"name\": \"fib\",\n  \"hand\": true\n}\n")
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
    let entry =
        |name: &str| table.as_array().unwrap().iter().find(|e| e["name"] == name).unwrap_or_else(|| panic!("no {name} in {table}")).clone();
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
    assert!(bin.join("exports64.dll").is_file() || bin.join("Release/exports64.dll").is_file(), "no exports64.dll under {}", bin.display());
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
