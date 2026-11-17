//! The interpreted oracle: a fixture is lifted, run through the IR interpreter, and its output is
//! compared with the native program's. It needs no clang and starts no process, so it is the fast
//! tier; the IR gate and the clang runs of the same fixtures are in `oracle.rs`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("testdata/bin").join(name)
}

/// The one skip a missing tool does not explain: a program of another system, which this host
/// cannot run. On a host that is not `native` it says on stderr why `what` is skipped, and answers
/// whether it is.
fn foreign_host(native: bool, program: &str, why: &str, what: &str) -> bool {
    if !native {
        eprintln!("SKIPPED: {program}: {why}: {what} is skipped");
    }
    !native
}

/// Fails `test` with the digest's one line when its output differs from the expected one; how
/// the run ended goes to stderr before it, and both whole sides are under `out/failed`. The blank
/// end of either side does not count.
fn assert_output(test: &str, expected: &str, actual: &str, ended: &str) {
    let failed = ra_asm_testing::digest::failed_dir();
    if let Err(line) = ra_asm_testing::digest::compare_in(&failed, test, expected.trim_end(), actual.trim_end()) {
        eprintln!("{test}: {ended}");
        panic!("{line}");
    }
}

/// A fixture lifted and raised to the level the tag names.
struct Raised {
    image: ra_asm_pe::Image,
    lifted: ra_asm_lift::Lifted,
    signatures: std::collections::HashMap<String, ra_asm_lift::raise::abi::Signature>,
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
    Raised { image, lifted, signatures }
}

/// A host without the Windows DLLs answers only the imports the interpreter runs itself: a run
/// stopped at another one says so and is skipped there.
fn stopped_for_dlls(binary: &str, outcome: &ra_asm_lift::run::Outcome) -> bool {
    match &outcome.stop {
        Some(stop) if stop.starts_with("no binding for ") => {
            foreign_host(cfg!(windows), binary, &format!("{stop}; this host has no Windows DLLs"), "the interpreted run")
        }
        _ => false,
    }
}

/// Runs `binary` at the tag's level through the IR interpreter and compares its output with the
/// native program's: the same proof as the clang run, without clang.
fn run_interpreted(binary: &str, expected: &str, tag: &str) {
    let Raised { image, lifted, signatures } = raise(binary, tag);
    assert!(lifted.warnings.is_empty(), "{binary}: the lift warned:\n{}", lifted.warnings.join("\n"));
    let options = ra_asm_lift::run::Options::default();
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &options).unwrap();
    if stopped_for_dlls(binary, &outcome) {
        return;
    }
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output(&format!("{binary} ({tag}): interpreted"), expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!(outcome.exit, 0, "{binary} ({tag}): the interpreted program's exit (stop: {:?})", outcome.stop);
}

/// Runs `binary` through the interpreter at each of `tags`, against `testdata/expected/<expected>`.
fn run_levels(binary: &str, expected: &str, tags: &[&str]) {
    let expected = std::fs::read_to_string(root().join("testdata/expected").join(expected)).unwrap();
    for tag in tags {
        run_interpreted(binary, &expected, tag);
    }
}

/// The machine, promoted and ABI levels, which an x86 fixture runs at unless it says otherwise.
const LEVELS: [&str; 3] = ["run-machine", "run-promoted", "run-abi"];

/// The ARM64 build of hello runs lifted through the IR interpreter, over the mapped image and
/// standalone: the host is x64, so the interpreter is its ARM64 machine. The standalone run is
/// the ABI level, raised by AAPCS64.
#[test]
fn helloa64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("helloa64.exe", &expected, tag);
    }
}

/// The ARM64 build of the float fixture runs through the interpreter with the native program's
/// output.
#[test]
fn floata64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/float.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("floata64.exe", &expected, tag);
    }
}

/// The Thumb-2 fixture, a static 32-bit ARM Linux program, runs through the interpreter over the
/// mapped image with the output its source prints.
#[test]
fn thumb32_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/thumb.txt")).unwrap();
    run_interpreted("thumb32", &expected, "run-machine");
}

/// The ARM64 Linux hello runs through the interpreter with the x64 build's native output.
#[test]
fn hello_linux_a64_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/hello_linux.txt")).unwrap();
    for tag in ["run-machine", "run-abi"] {
        run_interpreted("hello_linux_a64", &expected, tag);
    }
}

/// The static mipsel control program, an o32 Linux executable whose `write` and `exit` are
/// `syscall`s, runs through the interpreter at every level with the output its source prints;
/// the lift covers every instruction, and each level's IR passes `clang -c` when clang is here.
#[test]
fn mips_control_runs_interpreted() {
    run_mips_control("control", false);
}

/// The same program built big-endian (`--target=mips-linux-gnu`, the flags of its source
/// otherwise): its words read big-endian and its loads and stores swap, so it prints the same.
#[test]
fn mips_control_big_endian_runs_interpreted() {
    run_mips_control("control.be", true);
}

/// Runs `testdata/mips/<file>` at every level through the interpreter against `control.txt`,
/// with zero traps and each level's IR through `clang -c` when clang is here.
fn run_mips_control(file: &str, big_endian: bool) {
    let path = root().join("testdata/mips").join(file);
    let expected = std::fs::read_to_string(root().join("testdata/mips/control.txt")).unwrap();
    for tag in LEVELS {
        let image = ra_asm_pe::load(&path).unwrap();
        assert_eq!(image.machine(), ra_asm_pe::Machine::Mips, "a MIPS image");
        assert_eq!(image.big_endian(), big_endian, "{file}: the byte order");
        let analysis = ra_asm_analysis::analyze(&image);
        let level = if tag.contains("machine") {
            ra_asm_lift::RaiseLevel::Machine
        } else if tag.contains("promoted") {
            ra_asm_lift::RaiseLevel::Promoted
        } else {
            ra_asm_lift::RaiseLevel::Abi
        };
        let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        assert!(lifted.warnings.is_empty(), "mips {file} ({tag}): the lift warned:\n{}", lifted.warnings.join("\n"));
        let coverage = ra_asm_lift::coverage(&analysis, &lifted);
        assert_eq!(coverage.traps(), 0, "mips {file} ({tag}): unsupported instructions: {:?}", coverage.uncovered);
        let options = lifted.raise_options(level);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));

        let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
        let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
        assert_output(&format!("mips {file} ({tag}): interpreted"), &expected, &String::from_utf8_lossy(&outcome.output), &ended);
        assert_eq!(outcome.exit, 0, "mips {file} ({tag}): the interpreted program's exit (stop: {:?})", outcome.stop);

        if ra_asm_testing::tool("clang").is_none() {
            eprintln!("SKIPPED: mips {file} ({tag}): clang is not on the PATH: the IR gate is skipped");
            continue;
        }
        let dir = root().join("out/interp").join(format!("mips-{file}-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("program.ll"), lifted.module.to_ll()).unwrap();
        let check = std::process::Command::new("clang")
            .args(["-c", "-x", "ir", "-o"])
            .arg(dir.join("program.o"))
            .arg(dir.join("program.ll"))
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "mips {file} ({tag}): clang -c on {}:\n{}",
            dir.join("program.ll").display(),
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

/// The PSn00bSDK hello, a PS-X EXE for the R3000 whose `printf` is the BIOS's `A0` function
/// `0x3f`: the lift covers every instruction and names the call, and the interpreter answers it
/// at every level with the console output its source prints; each level's IR passes `clang -c`
/// when clang is here.
#[test]
fn psx_hello_runs_interpreted() {
    let path = root().join("testdata/psx/hello.exe");
    let expected = std::fs::read_to_string(root().join("testdata/psx/hello.txt")).unwrap();
    for tag in LEVELS {
        let image = ra_asm_pe::load(&path).unwrap();
        assert_eq!(image.platform(), Some("psx"), "a PS-X EXE");
        let analysis = ra_asm_analysis::analyze(&image);
        let level = if tag.contains("machine") {
            ra_asm_lift::RaiseLevel::Machine
        } else if tag.contains("promoted") {
            ra_asm_lift::RaiseLevel::Promoted
        } else {
            ra_asm_lift::RaiseLevel::Abi
        };
        let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        assert!(lifted.warnings.is_empty(), "psx hello ({tag}): the lift warned:\n{}", lifted.warnings.join("\n"));
        let coverage = ra_asm_lift::coverage(&analysis, &lifted);
        assert_eq!(coverage.traps(), 0, "psx hello ({tag}): unsupported instructions: {:?}", coverage.uncovered);
        let options = lifted.raise_options(level);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let ll = lifted.module.to_ll();
        assert!(
            ll.lines().any(|l| l.contains("call ") && l.contains("@ra_bios_printf(")),
            "psx hello ({tag}): the BIOS printf is not called by its name"
        );

        let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
        let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
        assert_output(&format!("psx hello ({tag}): interpreted"), &expected, &String::from_utf8_lossy(&outcome.output), &ended);
        assert_eq!(outcome.exit, 0, "psx hello ({tag}): the interpreted program's exit (stop: {:?})", outcome.stop);

        if ra_asm_testing::tool("clang").is_none() {
            eprintln!("SKIPPED: psx hello ({tag}): clang is not on the PATH: the IR gate is skipped");
            continue;
        }
        let dir = root().join("out/interp").join(format!("psx-hello-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("program.ll"), &ll).unwrap();
        let check = std::process::Command::new("clang")
            .args(["-c", "-x", "ir", "-o"])
            .arg(dir.join("program.o"))
            .arg(dir.join("program.ll"))
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "psx hello ({tag}): clang -c on {}:\n{}",
            dir.join("program.ll").display(),
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

/// The ps2sdk hello, an R5900 ELF: its own code (`main` and the `sio_puts` it calls, down to
/// `sio_putc`) lifts with no trap, and run from `main` over the mapped image it prints through
/// the EE's serial port the console output its source writes. The C library's start, which waits
/// on the IOP, does not run.
#[test]
fn ps2_hello_runs_interpreted() {
    let path = root().join("testdata/ps2/hello.elf");
    let expected = std::fs::read_to_string(root().join("testdata/ps2/hello.txt")).unwrap();
    let image = ra_asm_pe::load(&path).unwrap();
    assert_eq!(image.platform(), Some("ps2"), "an R5900 ELF");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let symbol = |name: &str| image.symbols.iter().find(|s| s.name == name).map(|s| s.va).unwrap_or_else(|| panic!("no symbol {name}"));
    let main = symbol("main");
    let root = lifted.functions.get(&main).cloned().expect("main is lifted");

    let own = reached_from(&lifted.module, &root);
    for name in ["sio_puts", "sio_putsn", "sio_putc"] {
        assert!(own.contains(&symbol(name)), "ps2 hello: main does not reach {name}: {own:x?}");
    }
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    let trapped: Vec<_> = coverage.functions.iter().filter(|(entry, ..)| own.contains(entry)).collect();
    assert!(trapped.is_empty(), "ps2 hello: the program's own code traps: {trapped:?}");

    lifted.entry = Some(main);
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    let head = &outcome.output[..outcome.output.len().min(8)];
    let ended = format!("exit {:#x}, stop: {:?}, {} steps, the output starting {head:02x?}", outcome.exit, outcome.stop, outcome.steps);
    assert_output("ps2 hello: interpreted", &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!(outcome.exit, 0, "ps2 hello: main's result (stop: {:?})", outcome.stop);
}

/// The pspdev hello, an Allegrex PRX: every function of the module, its C library with it, lifts
/// with no trap, and run from its `module_start` at the machine level over the mapped module it
/// prints, through `sceIoWrite`, the console output its source writes, then ends in
/// `sceKernelExitGame` with 0.
#[test]
fn psp_hello_runs_interpreted() {
    let path = root().join("testdata/psp/hello.prx");
    let expected = std::fs::read_to_string(root().join("testdata/psp/hello.txt")).unwrap().replace("\r\n", "\n");
    let image = ra_asm_pe::load(&path).unwrap();
    assert_eq!(image.platform(), Some("psp"), "an Allegrex PRX");
    let start = image.entry_point().expect("the PRX's module_start");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    assert_eq!(coverage.traps(), 0, "psp hello: unsupported instructions: {:?}", coverage.uncovered);
    assert!(coverage.functions.is_empty(), "psp hello: the module's own code traps: {:?}", coverage.functions);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);

    lifted.entry = Some(start);
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output("psp hello: interpreted", &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!(outcome.stop, None, "psp hello: the run stopped ({} steps)", outcome.steps);
    assert_eq!(outcome.exit, 0, "psp hello: the interpreted program's exit");
}

/// The image addresses of the functions of `module` that `root` reaches by its direct calls,
/// its own among them.
fn reached_from(module: &ra_asm_ir::Module, root: &str) -> std::collections::BTreeSet<u64> {
    let by_name: std::collections::HashMap<&str, &ra_asm_ir::Function> = module.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut names = std::collections::BTreeSet::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut work = vec![root.to_owned()];
    while let Some(name) = work.pop() {
        let Some(f) = by_name.get(name.as_str()) else { continue };
        if !names.insert(name) {
            continue;
        }
        seen.extend(f.address);
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            if let ra_asm_ir::Inst::Call { callee: ra_asm_ir::Callee::Direct(callee), .. } = &i.inst {
                work.push(callee.clone());
            }
        }
    }
    seen
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
fn hello64_runs_interpreted() {
    run_levels("hello64.exe", "hello.txt", &LEVELS);
}

#[test]
fn hello32_runs_interpreted() {
    run_levels("hello32.exe", "hello.txt", &LEVELS);
}

#[test]
fn float64_runs_interpreted() {
    run_levels("float64.exe", "float.txt", &LEVELS);
}

#[test]
fn float32_runs_interpreted() {
    run_levels("float32.exe", "float.txt", &LEVELS);
}

/// The x87 program under the exact unit.
#[test]
fn extended32_runs_interpreted_exact() {
    run_levels("extended32.exe", "extended.txt", &["run-machine-exact", "run-promoted-exact", "run-abi-exact"]);
}

#[test]
fn control64_runs_interpreted() {
    run_levels("control64.exe", "control.txt", &LEVELS);
}

#[test]
fn control32_runs_interpreted() {
    run_levels("control32.exe", "control.txt", &LEVELS);
}

#[test]
fn obfusc64_runs_interpreted() {
    run_levels("obfusc64.exe", "obfusc.txt", &LEVELS);
}

#[test]
fn obfusc32_runs_interpreted() {
    run_levels("obfusc32.exe", "obfusc.txt", &LEVELS);
}

#[test]
fn classes64_runs_interpreted() {
    run_levels("classes64.exe", "classes.txt", &LEVELS);
}

#[test]
fn classes32_runs_interpreted() {
    run_levels("classes32.exe", "classes.txt", &LEVELS);
}

#[test]
fn throws64_runs_interpreted() {
    run_levels("throws64.exe", "throws.txt", &LEVELS);
}

#[test]
fn inline64_runs_interpreted() {
    run_levels("inline64.exe", "inline.txt", &LEVELS);
}

#[test]
fn inline32_runs_interpreted() {
    run_levels("inline32.exe", "inline.txt", &LEVELS);
}

#[test]
fn files64_runs_interpreted() {
    run_levels("files64.exe", "files.txt", &LEVELS);
}

/// A thread kernel32 starts enters the lifted code through its native entry, which only the ABI
/// level has, so the run is at that level alone.
#[test]
fn tls64_runs_interpreted() {
    run_levels("tls64.exe", "tls.txt", &["run-abi"]);
}

#[test]
fn tls32_runs_interpreted() {
    run_levels("tls32.exe", "tls.txt", &["run-abi"]);
}

/// A program over the static C runtime starts at `main`, which the ABI level alone has.
#[test]
fn crt64_runs_interpreted() {
    run_levels("crt64.exe", "crt.txt", &["run-abi"]);
}

/// The resources are read in the image's own resource directory.
#[test]
fn resources64_runs_interpreted() {
    run_levels("resources64.exe", "resources.txt", &LEVELS);
}

#[test]
fn structs64_runs_interpreted() {
    run_levels("structs64.exe", "structs.txt", &LEVELS);
}

#[test]
fn structs32_runs_interpreted() {
    run_levels("structs32.exe", "structs.txt", &LEVELS);
}

/// The dispatch program without facts: every call through its table stays an indirect call.
#[test]
fn dispatch64_runs_interpreted() {
    run_levels("dispatch64.exe", "dispatch.txt", &LEVELS);
}

#[test]
fn dispatch32_runs_interpreted() {
    run_levels("dispatch32.exe", "dispatch.txt", &LEVELS);
}
