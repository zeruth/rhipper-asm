//! The execution oracle: a fixture is lifted, run through the IR interpreter, then compiled with
//! clang against the driver and run again, and each output is compared with the native program's.
//! The IR gate (`clang -c -x ir`) runs on every fixture; the clang run needs the C runtime and
//! skips without it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("testdata/bin").join(name)
}

fn clang_available() -> bool {
    Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ra-asm-lift-tests").join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fixture lifted and raised to the level the tag names.
struct Raised {
    image: ra_asm_pe::Image,
    lifted: ra_asm_lift::Lifted,
    signatures: std::collections::HashMap<String, ra_asm_lift::raise::abi::Signature>,
    level: ra_asm_lift::RaiseLevel,
}

/// Lifts `binary` to the level and the x87 unit the tag names. The machine IR, the promoted IR
/// and the ABI IR must all run; the tag picks one. The ABI IR stands alone: its data are globals
/// and it never maps the image. The exact x87 unit when the tag asks for it.
fn raise(binary: &str, tag: &str) -> Raised {
    let image = ra_asm_pe::load(&fixture(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let level = if tag.contains("machine") {
        ra_asm_lift::RaiseLevel::Machine
    } else if tag.contains("promoted") {
        ra_asm_lift::RaiseLevel::Promoted
    } else {
        ra_asm_lift::RaiseLevel::Abi
    };
    let x87 = if tag.contains("exact") { ra_asm_lift::X87Unit::Exact } else { ra_asm_lift::X87Unit::Double };
    let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87 };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    Raised { image, lifted, signatures, level }
}

/// Lifts `binary` into a directory with `program.ll`, `image.bin` and `image.h`.
fn lift_to(binary: &str, tag: &str) -> (PathBuf, ra_asm_lift::Lifted) {
    let Raised { image, lifted, signatures, level } = raise(binary, tag);
    let analysis = ra_asm_analysis::analyze(&image);
    // The tests run in parallel: each one writes its own directory.
    let dir = scratch(&format!("{tag}-{binary}"));
    std::fs::write(dir.join("program.ll"), lifted.module.to_ll()).unwrap();
    std::fs::write(dir.join("image.bin"), image.mapped()).unwrap();
    // The program's resources, linked into the port as the C port links them.
    if !image.resources.is_empty() {
        std::fs::write(dir.join("resources.res"), ra_asm_pe::resources::res_file(&image)).unwrap();
    }
    let abi = (level == ra_asm_lift::RaiseLevel::Abi).then_some(&signatures);
    std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, abi)).unwrap();
    (dir, lifted)
}

/// Runs `binary` at the tag's level through the IR interpreter and compares its output with the
/// native program's: the same proof as the clang run, without clang.
fn run_interpreted(binary: &str, expected: &str, tag: &str) {
    let Raised { image, lifted, signatures, .. } = raise(binary, tag);
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let options = ra_asm_lift::run::Options::default();
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &options).unwrap();
    let stdout = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    assert_eq!(
        stdout.trim_end(),
        expected.trim_end(),
        "{binary} ({tag}): the interpreted program's output (exit {:#x}, stop: {:?}, {} steps)",
        outcome.exit,
        outcome.stop,
        outcome.steps
    );
    assert_eq!(outcome.exit, 0, "{binary} ({tag}): the interpreted program's exit (stop: {:?})", outcome.stop);
}

#[test]
fn the_lifted_fixtures_pass_the_ir_gate() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the IR gate is skipped");
        return;
    }
    for (binary, tag) in [
        ("hello64.exe", "gate-machine"),
        ("hello64.exe", "gate-abi"),
        ("control64.exe", "gate-abi"),
        ("float64.exe", "gate-abi"),
        ("exports64.dll", "gate-machine"),
        ("exports64.dll", "gate-abi"),
        ("tls64.exe", "gate-machine"),
        ("tls32.exe", "gate-machine"),
        ("crt64.exe", "gate-abi"),
        ("throws64.exe", "gate-machine"),
    ] {
        let (dir, lifted) = lift_to(binary, tag);
        assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
        let check =
            Command::new("clang").args(["-c", "-x", "ir", "-o"]).arg(dir.join("program.o")).arg(dir.join("program.ll")).output().unwrap();
        assert!(
            check.status.success(),
            "{binary}: clang -c on {}:\n{}",
            dir.join("program.ll").display(),
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

/// The ARM64 build of hello lifts without a trap, over the mapped image and standalone, and its IR
/// passes the gate. The run needs an ARM64 host or the IR interpreter (task 107): neither is here.
#[test]
fn the_arm64_fixture_lifts_and_passes_the_ir_gate() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the IR gate is skipped");
        return;
    }
    let image = ra_asm_pe::load(&fixture("helloa64.exe")).unwrap();
    assert_eq!(image.machine(), ra_asm_pe::Machine::Arm64);
    let analysis = ra_asm_analysis::analyze(&image);
    for standalone in [false, true] {
        let options = ra_asm_lift::LiftOptions { standalone, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        assert_eq!(lifted.isa, ra_asm_lift::Isa::Arm64);
        assert!(lifted.warnings.is_empty(), "the lift warned:\n{}", lifted.warnings.join("\n"));
        let coverage = ra_asm_lift::coverage(&analysis, &lifted);
        assert_eq!(coverage.traps(), 0, "unsupported instructions: {:?}", coverage.uncovered);
        lifted.apply_names(&analysis, None);
        let ll = lifted.module.to_ll();
        assert!(ll.contains("target triple = \"aarch64-pc-windows-msvc\""));
        // The three imports are called through their wrappers, found through the adrp/ldr pair.
        for import in ["GetStdHandle", "WriteFile", "ExitProcess"] {
            assert!(lifted.imports.contains(import), "{import} is called: {:?}", lifted.imports);
        }
        let dir = scratch(&format!("gate-arm64-{standalone}-helloa64"));
        std::fs::write(dir.join("program.ll"), &ll).unwrap();
        let check =
            Command::new("clang").args(["-c", "-x", "ir", "-o"]).arg(dir.join("program.o")).arg(dir.join("program.ll")).output().unwrap();
        assert!(check.status.success(), "clang -c on {}:\n{}", dir.join("program.ll").display(), String::from_utf8_lossy(&check.stderr));
    }
    eprintln!("helloa64: the host is x64 and the IR interpreter (task 107) is not here: the run is skipped");
}

fn run_oracle(binary: &str, expected: &str) {
    run_oracle_as(binary, expected, "run-machine");
    run_oracle_as(binary, expected, "run-promoted");
    run_oracle_as(binary, expected, "run-abi");
}

fn run_oracle_as(binary: &str, expected: &str, tag: &str) {
    run_interpreted(binary, expected, tag);
    let (dir, lifted) = lift_to(binary, tag);
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let exe = dir.join("lifted.exe");
    let mut build = Command::new("clang");
    build.args(["-O1", "-g", "-gcodeview", "-fuse-ld=lld", "-Wno-override-module"]);
    // A port of a 32-bit program keeps its 32-bit pointers: every address of the process must
    // fit in them, which the linker arranges for an image that is not large-address-aware. The
    // levels that map the image at its base cannot have that: the loader of such a process puts
    // its own heap and environment block at the low addresses, so they run as a plain process
    // with the driver's stack placed low.
    if lifted.bits == 32 && lifted.standalone {
        build.arg("-Wl,/largeaddressaware:no");
    }
    if dir.join("resources.res").is_file() {
        build.arg(dir.join("resources.res"));
    }
    let build = build
        .arg(dir.join("program.ll"))
        .arg(root().join("runtime/driver.c"))
        .arg("-I")
        .arg(&dir)
        .arg("-o")
        .arg(&exe)
        .args(["-lkernel32", "-luser32"])
        .output()
        .unwrap();
    if !build.status.success() {
        let text = String::from_utf8_lossy(&build.stderr);
        if text.contains("cannot open input file") || text.contains("libcmt") || text.contains("msvcrt") {
            eprintln!("the C runtime is not installed: the run is skipped\n{text}");
            return;
        }
        panic!("{binary}: clang link:\n{text}");
    }
    let run = Command::new(&exe).arg(dir.join("image.bin")).current_dir(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(
        stdout.trim_end(),
        expected.trim_end(),
        "{binary}: the lifted program's output (exit {:?}, stderr: {stderr})",
        run.status.code()
    );
    assert!(run.status.success(), "{binary}: exit {:?}, stderr: {stderr}", run.status.code());
}

#[test]
fn hello64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    run_oracle("hello64.exe", &expected);
}

#[test]
fn float64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    run_oracle("float64.exe", &expected);
}

/// The 32-bit fixtures at every level.
fn run_oracle_32(binary: &str, expected: &str) {
    run_oracle(binary, expected);
}

/// The homebrew XBE: a 32-bit x86 program whose imports are the kernel's, by ordinal, named from
/// the loader's table; the interpreter runs it at every level and answers the kernel calls. The
/// clang run waits for the kernel's platform runtime.
#[test]
fn xbe32_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/xbe.txt")).unwrap();
    let raised = raise("xbe32.xbe", "run-abi");
    for name in ["DbgPrint", "ExAllocatePool", "ExFreePool", "HalReturnToFirmware"] {
        assert!(raised.lifted.imports.contains(name), "the lift names the kernel import {name}: {:?}", raised.lifted.imports);
    }
    for tag in ["run-machine", "run-promoted", "run-abi"] {
        run_interpreted("xbe32.xbe", &expected, tag);
    }
}

#[test]
fn hello32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    run_oracle_32("hello32.exe", &expected);
}

#[test]
fn control32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    run_oracle_32("control32.exe", &expected);
}

#[test]
fn float32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    run_oracle_32("float32.exe", &expected);
}

/// The x87 program under the exact unit: the series summed at 64 bits, the square root, the
/// tbyte round trip, all to 17 digits.
#[test]
fn extended32_runs_lifted_exact() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/extended.txt")).unwrap();
    run_oracle_as("extended32.exe", &expected, "run-machine-exact");
    run_oracle_as("extended32.exe", &expected, "run-promoted-exact");
    run_oracle_as("extended32.exe", &expected, "run-abi-exact");
}

#[test]
fn classes32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/classes.txt")).unwrap();
    run_oracle_32("classes32.exe", &expected);
}

#[test]
fn classes64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/classes.txt")).unwrap();
    run_oracle("classes64.exe", &expected);
}

/// The exceptions program: a throw caught by value and one by reference to a base, through a
/// function without a handler, a rethrow into a catch-all, and a local destroyed on the way out,
/// all from the unwind data and the frame handler's tables.
#[test]
fn throws64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/throws.txt")).unwrap();
    run_oracle("throws64.exe", &expected);
}

#[test]
fn control64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/control.txt")).unwrap();
    run_oracle("control64.exe", &expected);
}

/// The files program: a file created, written, read back and removed through kernel32, whose
/// pushed and vector-stored arguments the raise forwards into the calls.
#[test]
fn files64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/files.txt")).unwrap();
    run_oracle("files64.exe", &expected);
}

/// The thread-local storage program: a thread-local counter stepped from two threads, each with
/// its own block, and the TLS callbacks counted. A thread kernel32 starts enters the lifted code
/// through its native entry, which only the ABI level has, so the run is at that level alone;
/// the gate takes the machine IR.
#[test]
fn tls64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/tls.txt")).unwrap();
    run_oracle_as("tls64.exe", &expected, "run-abi");
}

/// A program over the static C runtime starts at `main` with the driver's arguments and calls
/// the host's runtime for the functions the image links in: the ABI level is the one that has
/// a `main` to call and a host runtime to call into.
#[test]
fn crt64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/crt.txt")).unwrap();
    run_oracle_as("crt64.exe", &expected, "run-abi");
}

#[test]
fn tls32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/tls.txt")).unwrap();
    run_oracle_as("tls32.exe", &expected, "run-abi");
}

/// The resources program: a string table and an RCDATA block read through user32 and kernel32,
/// which the port finds in its own resource directory (the interpreter in the image's), and
/// tables of integers, strings and doubles read by index.
#[test]
fn resources64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/resources.txt")).unwrap();
    run_oracle("resources64.exe", &expected);
}

/// The struct program: a record of mixed fields filled and read through a pointer, which the
/// type recovery turns into a named struct at the ABI level.
#[test]
fn structs64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/structs.txt")).unwrap();
    run_oracle("structs64.exe", &expected);
}

#[test]
fn structs32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/structs.txt")).unwrap();
    run_oracle_32("structs32.exe", &expected);
}

/// The dispatch program without facts: every call through its table stays an indirect call,
/// which runs at every level.
#[test]
fn dispatch64_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/dispatch.txt")).unwrap();
    run_oracle("dispatch64.exe", &expected);
}

#[test]
fn dispatch32_runs_lifted() {
    if !clang_available() {
        eprintln!("clang is not on the PATH: the oracle is skipped");
        return;
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/dispatch.txt")).unwrap();
    run_oracle_32("dispatch32.exe", &expected);
}

/// The facts of a traced run resolve the dispatch: the interpreter sees the four handlers behind
/// the one call through the table (a tail jump through a register on x64, a call through the
/// table on x86), the analysis of the next pass takes them, and the lift calls the handlers
/// directly, with the indirect call kept for a target the run did not see. The resolved program
/// runs the same, through the interpreter and (at the ABI level, as the port is) through clang.
#[test]
fn a_traced_run_resolves_the_dispatch() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/dispatch.txt")).unwrap();
    for (binary, level) in [("dispatch64.exe", "machine"), ("dispatch64.exe", "abi"), ("dispatch32.exe", "abi")] {
        let Raised { image, lifted, signatures, .. } = raise(binary, level);
        let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
        assert_eq!(outcome.exit, 0, "{binary} ({level}): the first run (stop: {:?})", outcome.stop);
        let facts = outcome.facts;
        assert!(facts.input.is_of(&image), "{binary}: the facts name their input: {:?}", facts.input);
        let analysis = ra_asm_analysis::analyze(&image);
        let handlers: Vec<u64> = ["h_add", "h_sub", "h_mul", "h_xor"]
            .iter()
            .map(|n| analysis.functions.values().find(|f| f.name == *n).unwrap().entry)
            .collect();
        let dispatch = analysis.functions.values().find(|f| f.name == "dispatch").unwrap();
        let [call] = facts.calls.as_slice() else { panic!("{binary} ({level}): one call resolved, not {:?}", facts.calls) };
        assert!(dispatch.contains(call.site), "{binary} ({level}): the call at {:#x} is in dispatch", call.site);
        assert_eq!(call.targets, handlers, "{binary} ({level}): the targets are the handlers");
        assert!(facts.entered.contains(&dispatch.entry), "{binary} ({level}): dispatch was entered");

        // The next pass: the analysis knows the calls, and the ABI lift makes them direct.
        let analysis = ra_asm_analysis::analyze_with(&image, &facts);
        let dispatch = analysis.functions.values().find(|f| f.name == "dispatch").unwrap();
        assert_eq!(dispatch.traced_calls.get(&call.site), Some(&handlers), "{binary}: dispatch knows its traced call");
        assert!(handlers.iter().all(|h| dispatch.calls.contains(h)), "{binary}: the handlers are among dispatch's calls");
        let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        assert_eq!(lifted.traced_calls.get(&call.site), Some(&handlers));
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let f = lifted.module.functions.iter().find(|f| f.name == "dispatch").unwrap();
        let direct: Vec<&str> = f
            .blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .filter_map(|i| match &i.inst {
                ra_asm_ir::Inst::Call { callee: ra_asm_ir::Callee::Direct(name), .. } if name.starts_with("h_") => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            direct,
            ["h_add", "h_sub", "h_mul", "h_xor"],
            "{binary}: dispatch calls the handlers directly:\n{}",
            lifted.module.to_ll()
        );
        let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
        let stdout = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
        assert_eq!(stdout.trim_end(), expected.trim_end(), "{binary}: the resolved program's output (stop: {:?})", outcome.stop);
        assert_eq!(outcome.exit, 0, "{binary}: the resolved program's exit (stop: {:?})", outcome.stop);
    }
}
