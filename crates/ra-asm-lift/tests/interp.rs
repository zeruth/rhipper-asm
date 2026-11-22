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

/// The Thumb fixture of the processors before Thumb-2 (ARMv4T, the ARM7TDMI's): sixteen-bit
/// instructions, a return by `pop {r1}` and `bx r1`, the divisions and the wide multiply the
/// program's own. It prints what it printed under an emulator of the machine and ends with the
/// code it ended with there.
#[test]
fn thumb1_runs_interpreted() {
    runs_as_the_machine_ran_it("thumb1");
}

/// The same program in ARM code, the other instruction set of those processors: every
/// instruction under its condition, the returns among them, a switch through a table of words.
/// The image is ARM code by its entry, and the decoder, the analysis and the lift follow it.
#[test]
fn arm1_runs_interpreted() {
    let image = ra_asm_pe::load(&root().join("testdata/bin/arm1")).unwrap();
    assert!(image.arm_code() && !image.arm_and_thumb());
    runs_as_the_machine_ran_it("arm1");
}

/// The program in both sets at once, as a cartridge of those processors is: Thumb code entered
/// through ARM code and calling ARM code, each way through the thunk the linker puts there. The
/// image's mapping symbols say which set the code at an address is.
#[test]
fn armthumb_runs_interpreted() {
    let image = ra_asm_pe::load(&root().join("testdata/bin/armthumb")).unwrap();
    assert!(image.arm_and_thumb());
    assert_eq!(ra_asm_analysis::arch_of(&image).name(), "arm+thumb");
    runs_as_the_machine_ran_it("armthumb");
}

/// The same program with its symbols stripped, the mapping symbols among them, as a cartridge
/// has none: the file says only that it starts in ARM code. The decoder tells the sets apart as
/// it follows the code, by the branches that change the set and the low bit of an address a
/// thunk jumps through.
#[test]
fn armthumb_stripped_runs_interpreted() {
    let image = ra_asm_pe::load(&root().join("testdata/bin/armthumb_stripped")).unwrap();
    assert!(image.arm_code() && !image.arm_and_thumb(), "the file names one set, the one it starts in");
    runs_as_the_machine_ran_it("armthumb_stripped");
    let analysis = ra_asm_analysis::analyze(&image);
    assert!(image.arm_and_thumb(), "the analysis found the other");
    assert!(analysis.functions.len() >= 14, "{} functions", analysis.functions.len());
}

/// A fixture of the program `testdata/src/thumb1.c`: lifted without a warning and interpreted,
/// it prints what the program printed under an emulator and ends with the code it ended with.
fn runs_as_the_machine_ran_it(binary: &str) {
    let expected = std::fs::read_to_string(root().join("testdata/expected/thumb1.txt")).unwrap();
    let Raised { image, lifted, signatures } = raise(binary, "run-machine");
    assert!(lifted.warnings.is_empty(), "the lift warned:\n{}", lifted.warnings.join("\n"));
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output(&format!("{binary}: interpreted"), &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!(outcome.exit, 98, "the exit the machine ended with (stop: {:?})", outcome.stop);
}

/// The Cortex-M fixture, firmware for a microcontroller with no system under it: its reset
/// handler copies the data to RAM and calls `main`, which prints by semihosting (`bkpt 0xab`)
/// and ends the program by it.
#[test]
fn cortexm_firmware_runs_interpreted() {
    let expected = std::fs::read_to_string(root().join("testdata/expected/cortexm.txt")).unwrap();
    run_interpreted("cortexm.elf", &expected, "run-machine");
}

/// The same firmware as the flat image a chip is flashed with: no container and no symbol, the
/// vector table at its start. Flash is placed by the reset handler's address, RAM runs to the
/// stack's top, the handlers the table names are functions, and the run prints the same.
#[test]
fn cortexm_flat_image_runs_interpreted() {
    let image = ra_asm_pe::load(&root().join("testdata/bin/cortexm.bin")).unwrap();
    assert!(image.firmware);
    assert_eq!(image.image_base(), 0x0800_0000);
    let handlers: Vec<u64> = image.cortex_m_vectors().unwrap().into_iter().map(|(_, handler)| handler).collect();
    assert_eq!(handlers.len(), 10, "reset, the faults and the tick: {handlers:x?}");
    // The entry is the reset handler, the table's first.
    assert_eq!(image.entry_point(), handlers.first().copied());
    let functions: Vec<u64> = ra_asm_analysis::analyze(&image).functions.keys().copied().collect();
    for handler in handlers {
        assert!(functions.contains(&handler), "the handler at {handler:#x} is no function");
    }
    let expected = std::fs::read_to_string(root().join("testdata/expected/cortexm.txt")).unwrap();
    run_interpreted("cortexm.bin", &expected, "run-machine");
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

/// The PSn00bSDK drawing fixture over the PlayStation's platform runtime, headless: the program
/// draws through the GPU by DMA, waits for the vertical blank's interrupt, which its own handler
/// takes through the BIOS's exit hook, and reads the pad the BIOS's driver polls. Under a script
/// that holds right and down for three frames and then presses the cross button, it prints where
/// the square got to, and its eighth frame is the reference picture, pixel for pixel: the
/// renderer is software, so the same on every host.
#[test]
fn psx_draw_renders_and_reads_the_pad() {
    use ra_asm_lift::run::{Capture, Console, Options};
    let Raised { image, lifted, signatures } = raise("draw_psx.exe", "run-machine");
    assert_eq!(ra_asm_lift::coverage(&ra_asm_analysis::analyze(&image), &lifted).traps(), 0, "psx draw: unsupported instructions");
    let held = |buttons: &str| ra_asm_lift::run::pad(buttons).unwrap();
    let capture = Capture::new().limit(8).keep(&[8]).press(2, 0, held("right+down")).press(5, 0, held("cross")).press(7, 0, held("none"));
    let console = Console { capture: Some(capture.clone()), ..Console::default() };
    let outcome =
        ra_asm_lift::run::run(&image, &lifted, &signatures, &Options { limit: 0, console: Some(console), ..Options::default() }).unwrap();
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    let expected = std::fs::read_to_string(root().join("testdata/expected/draw_psx.txt")).unwrap();
    assert_output("psx draw: interpreted", &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!((outcome.exit, &outcome.stop), (0, &None), "psx draw: the run ends with its last frame");

    let kept = capture.lock();
    let frame = kept.kept.get(&8).expect("the eighth frame was presented");
    let reference = std::fs::read(root().join("testdata/expected/draw_psx.png")).unwrap();
    let reference = ra_asm_platform::png::decode(&reference).expect("the reference picture reads");
    let differing = frame.differing(&reference, 0);
    if differing != 0 {
        let failed = ra_asm_testing::digest::failed_dir();
        std::fs::create_dir_all(&failed).unwrap();
        let path = failed.join("draw_psx.png");
        std::fs::write(&path, ra_asm_platform::png::encode(frame)).unwrap();
        panic!("psx draw: {differing} pixels of the eighth frame differ from the reference; the frame is at {}", path.display());
    }
}

/// The PSP hello, a PRX for the Allegrex: each import stub the program calls lifts to the call on
/// the import its NID names, through the o32 wrapper that leaves the result in `v0`.
#[test]
fn psp_hello_calls_its_imports_by_name() {
    let image = ra_asm_pe::load(&root().join("testdata/psp/hello.prx")).unwrap();
    assert_eq!(image.platform(), Some("psp"), "a PRX");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let ll = lifted.module.to_ll();
    for name in ["sceIoWrite", "sceKernelExitGame"] {
        let wrapper = ra_asm_lift::import_wrapper_name(name, false);
        assert!(ll.lines().any(|l| l.contains("call ") && l.contains(&format!("@{name}("))), "psp hello: no call on {name}");
        assert!(ll.lines().any(|l| l.contains("call ") && l.contains(&format!("@{wrapper}("))), "psp hello: no stub calls {wrapper}");
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

/// The PSP's drawing fixture over the PSP's platform runtime, headless: the program draws into a
/// frame buffer in the video memory, through the uncached mirror, names it to the display, reads
/// the controller and waits for the vertical blank each frame. Under a script that holds right
/// and down for three frames and then presses the cross button, it prints where the square got
/// to, and its eighth frame shows the square there over what the program drew first.
#[test]
fn psp_draw_shows_its_frame_buffer_and_reads_the_pad() {
    use ra_asm_lift::run::{Capture, Console, Options};
    let image = ra_asm_pe::load(&root().join("testdata/psp/draw.prx")).unwrap();
    let expected = std::fs::read_to_string(root().join("testdata/psp/draw.txt")).unwrap().replace("\r\n", "\n");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    assert_eq!(ra_asm_lift::coverage(&analysis, &lifted).traps(), 0, "psp draw: unsupported instructions");
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.entry = image.entry_point();

    let held = |buttons: &str| ra_asm_lift::run::pad(buttons).unwrap();
    let capture = Capture::new().limit(8).keep(&[8]).press(2, 0, held("right+down")).press(5, 0, held("cross")).press(7, 0, held("none"));
    let console = Console { capture: Some(capture.clone()), ..Console::default() };
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &Options { console: Some(console), ..Options::default() }).unwrap();
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output("psp draw: interpreted", &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!((outcome.exit, &outcome.stop), (0, &None), "psp draw: the run ends with its last frame");

    let kept = capture.lock();
    let frame = kept.kept.get(&8).expect("the eighth frame was presented");
    assert_eq!((frame.width, frame.height), (480, 272));
    let at = |x: usize, y: usize| frame.pixels[y * 480 + x];
    assert_eq!((at(10, 10), at(469, 10), at(470, 10)), (0x00ff_ffff, 0x00ff_ffff, 0), "the line across the top");
    assert_eq!((at(16, 200), at(80, 200), at(144, 200)), (0x00ff_0000, 0x0000_ff00, 0x0000_00ff), "the three blocks");
    assert_eq!((at(230, 126), at(261, 157)), (0x00ff_ff00, 0x00ff_ff00), "the square where the pad moved it");
    assert_eq!((at(229, 126), at(224, 120), at(262, 157)), (0x0000_2040, 0x0000_2040, 0), "what the square left is the background's");
}

/// The Nintendo 64's drawing fixture over the console's platform runtime, headless: a libdragon
/// program whose frame buffers the video interface shows. `display_get` waits for the video
/// interface's interrupt, which the run raises as the VR4300's exception into the program's
/// own handler, and the joypad module reads the controller through the PIF by the serial
/// interface's. Under a script that holds right and down for three frames and then presses the
/// cross button (the controller's A), it prints where the square got to, and its eighth frame
/// shows the square there.
#[test]
fn n64_draw_takes_its_interrupts_and_reads_the_controller() {
    use ra_asm_lift::run::{Capture, Console, Options};
    let image = ra_asm_pe::load(&root().join("testdata/n64/draw.z64")).unwrap();
    let expected = std::fs::read_to_string(root().join("testdata/n64/draw.txt")).unwrap().replace("\r\n", "\n");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);

    let held = |buttons: &str| ra_asm_lift::run::pad(buttons).unwrap();
    let capture = Capture::new().limit(8).keep(&[8]).press(2, 0, held("right+down")).press(5, 0, held("cross")).press(7, 0, held("none"));
    let console = Console { capture: Some(capture.clone()), ..Console::default() };
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &Options { console: Some(console), ..Options::default() }).unwrap();
    let ended = format!("exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps);
    assert_output("n64 draw: interpreted", &expected, &String::from_utf8_lossy(&outcome.output), &ended);
    assert_eq!((outcome.exit, &outcome.stop), (0, &None), "n64 draw: the run ends with its last frame");

    let kept = capture.lock();
    let frame = kept.kept.get(&8).expect("the eighth frame was presented");
    assert_eq!(frame.width, 320);
    let at = |x: usize, y: usize| frame.pixels[y * 320 + x];
    assert_eq!((at(10, 10), at(309, 10), at(310, 10)), (0x00ff_ffff, 0x00ff_ffff, 0), "the line across the top");
    assert_eq!((at(150, 110), at(181, 141)), (0x00ff_ff00, 0x00ff_ff00), "the square where the controller moved it");
    assert_eq!((at(149, 110), at(182, 141), at(0, 95)), (0x0000_2142, 0x0000_2142, 0), "the band behind it, and nothing above the band");
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

// The Allegrex's VFPU, form by form: each test overwrites the PSP hello's `module_start` with a
// program that runs one form on lanes it sets, moves the lane each case checks to `$11` and
// compares it with the word the instruction's definition gives; the interpreter runs it at the
// machine level over the mapped module.

/// Where the PSP hello's `module_start` loads, and its length in words: a form's program stays
/// inside it, so no other function's start splits it.
const PSP_START: u64 = 0x0880_40ec;
const PSP_START_WORDS: usize = 59;

/// `$11` holds the word a case checks, `$12` the word its definition gives, `$13` their difference.
const GOT: u32 = 11;
const WANT: u32 = 12;
const DIFF: u32 = 13;
const SP: u32 = 29;

/// The sizes of a VFPU operation: a single, a pair, a triple and a quad.
const S: u32 = 0;
const P: u32 = 0x80;
const T: u32 = 0x8000;
const Q: u32 = 0x8080;

/// The VFPU registers the forms use: the columns of matrix 0, its rows 0 and (of matrix 1) its
/// row 0, matrix 0, its transpose and matrix 1. Lane `k` of a column `c` is the single `c + 32 k`.
const C000: u32 = 0;
const C010: u32 = 1;
const C020: u32 = 2;
const C030: u32 = 3;
const R000: u32 = 0x20;
const R100: u32 = 0x24;
const M000: u32 = 0;
const E000: u32 = 0x20;
const M100: u32 = 4;

const VADD: u32 = 0x6000_0000;
const VSUB: u32 = 0x6080_0000;
const VDIV: u32 = 0x6380_0000;
const VMUL: u32 = 0x6400_0000;
const VSCL: u32 = 0x6500_0000;
const VCRS: u32 = 0x6680_0000;
const VCMP: u32 = 0x6c00_0000;
const VMIN: u32 = 0x6d00_0000;
const VMAX: u32 = 0x6d80_0000;
const VSCMP: u32 = 0x6e80_0000;
const VSGE: u32 = 0x6f00_0000;
const VSLT: u32 = 0x6f80_0000;
const VMOV: u32 = 0xd000_0000;
const VABS: u32 = 0xd001_0000;
const VNEG: u32 = 0xd002_0000;
const VIDT: u32 = 0xd003_0000;
const VSAT0: u32 = 0xd004_0000;
const VSAT1: u32 = 0xd005_0000;
const VZERO: u32 = 0xd006_0000;
const VONE: u32 = 0xd007_0000;
const VBFY1: u32 = 0xd042_0000;
const VBFY2: u32 = 0xd043_0000;
const VCST: u32 = 0xd060_0000;
const VF2IN: u32 = 0xd200_0000;
const VF2IZ: u32 = 0xd220_0000;
const VF2IU: u32 = 0xd240_0000;
const VF2ID: u32 = 0xd260_0000;
const VI2F: u32 = 0xd280_0000;
const VCMOVT: u32 = 0xd2a0_0000;
const VCMOVF: u32 = 0xd2a8_0000;
const VMSCL: u32 = 0xf200_0000;
const VMMOV: u32 = 0xf380_0000;
const VMIDT: u32 = 0xf383_0000;
const VMZERO: u32 = 0xf386_0000;
const VMONE: u32 = 0xf387_0000;

/// A case: its name, its words ending with the checked word in `$11`, and that word's value.
type Case = (&'static str, Vec<u32>, u32);

fn r_type(rs: u32, rt: u32, rd: u32, sa: u32, funct: u32) -> u32 {
    (rs << 21) | (rt << 16) | (rd << 11) | (sa << 6) | funct
}

fn i_type(op: u32, rs: u32, rt: u32, imm: u16) -> u32 {
    (op << 26) | (rs << 21) | (rt << 16) | u32::from(imm)
}

/// The word `v` in `$12`: `lui` and `ori`, less the one a half of zero leaves nothing to do.
fn want(v: u32) -> Vec<u32> {
    match ((v >> 16) as u16, v as u16) {
        (0, low) => vec![i_type(0x0d, 0, WANT, low)],
        (high, 0) => vec![i_type(0x0f, 0, WANT, high)],
        (high, low) => vec![i_type(0x0f, 0, WANT, high), i_type(0x0d, WANT, WANT, low)],
    }
}

/// `lw $11, offset($sp)`.
fn lw(offset: i16) -> u32 {
    i_type(0x23, SP, GOT, offset as u16)
}

/// `lv.s`, `sv.s`, `lv.q` or `sv.q` (`op` 50, 58, 54 or 62) of `reg` at `offset($sp)`: the
/// register's bits past its low five in the offset's low two, of a quad its row bit alone.
fn vmem(op: u32, reg: u32, offset: i16) -> u32 {
    let high = if matches!(op, 50 | 58) { (reg >> 5) & 3 } else { (reg >> 5) & 1 };
    i_type(op, SP, reg & 0x1f, offset as u16) | high
}

/// The operation `base` of `size` on `vd`, `vs` and `vt`; a form that takes an immediate (a
/// constant's index, a scale, a condition bit) has it in `vt`, and `vcmp` its condition in `vd`.
fn vop(base: u32, size: u32, vd: u32, vs: u32, vt: u32) -> u32 {
    base | size | (vt << 16) | (vs << 8) | vd
}

/// `vpfxs`, `vpfxt` or `vpfxd` (`k` 0, 1 or 2) with its 24 bits.
fn vpfx(k: u32, data: u32) -> u32 {
    0xdc00_0000 | (k << 24) | data
}

fn viim(reg: u32, v: i16) -> u32 {
    0xdf00_0000 | (reg << 16) | u32::from(v as u16)
}

fn vfim(reg: u32, half: u16) -> u32 {
    0xdf80_0000 | (reg << 16) | u32::from(half)
}

/// `mfv $11, reg`.
fn mfv(reg: u32) -> u32 {
    0x4860_0000 | (GOT << 16) | reg
}

/// `mtv $12, reg`.
fn mtv(reg: u32) -> u32 {
    0x48e0_0000 | (WANT << 16) | reg
}

fn lane(v: u32, k: u32) -> u32 {
    v + 32 * k
}

/// The column `v` set to the integers `values` by `viim.s`.
fn column(v: u32, values: [i16; 4]) -> Vec<u32> {
    (0..4).zip(values).map(|(k, x)| viim(lane(v, k), x)).collect()
}

/// `C000` set to 1 to 4 and `C010` to 10 to 40.
fn quads() -> Vec<u32> {
    let mut words = column(C000, [1, 2, 3, 4]);
    words.extend(column(C010, [10, 20, 30, 40]));
    words
}

fn bits(v: f32) -> u32 {
    v.to_bits()
}

/// A word conversion's result, which saturates as Rust's does.
fn word(v: f64) -> u32 {
    v as i32 as u32
}

/// The program: `v0` cleared, `setup`, then each case's words and the comparison of `$11` with
/// its word, which sets the case's bit of `v0` when they differ; `v0` is the run's exit code.
fn vfpu_program(setup: &[u32], cases: &[Case]) -> Vec<u32> {
    let mut code = vec![r_type(0, 0, 2, 0, 0x25)];
    code.extend(setup);
    for (bit, (_, words, expected)) in (0..).zip(cases) {
        code.extend(words);
        code.extend(want(*expected));
        // xor, sltu, sll, or: the case's bit, set when the words differ.
        code.extend([
            r_type(GOT, WANT, DIFF, 0, 0x26),
            r_type(0, DIFF, DIFF, 0, 0x2b),
            r_type(0, DIFF, DIFF, bit, 0),
            r_type(2, DIFF, 2, 0, 0x25),
        ]);
    }
    code.extend([0x03e0_0008, 0]);
    code
}

/// Runs the program of `setup` and `cases` in place of the PSP hello's `module_start`: it must lift
/// without a trap and stop nowhere, and each case's lane must be the definition's.
fn run_vfpu(form: &str, setup: &[u32], cases: &[Case]) {
    let program = vfpu_program(setup, cases);
    assert!(program.len() <= PSP_START_WORDS && cases.len() < 32, "{form}: the program outgrows module_start ({} words)", program.len());
    let mut image = ra_asm_pe::load(&root().join("testdata/psp/hello.prx")).unwrap();
    assert_eq!((image.platform(), image.entry_point()), (Some("psp"), Some(PSP_START)), "the PSP hello");
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_le_bytes()).collect();
    image.write(PSP_START, &bytes).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    let trapped: Vec<_> = coverage.functions.iter().filter(|(entry, ..)| *entry == PSP_START).collect();
    assert!(trapped.is_empty(), "{form}: the program traps: {trapped:?}\n{:?}", lifted.warnings);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.entry = Some(PSP_START);
    let outcome = ra_asm_lift::run::run(&image, &lifted, &signatures, &ra_asm_lift::run::Options::default()).unwrap();
    assert_eq!(outcome.stop, None, "{form}: the run stopped ({} steps)", outcome.steps);
    let wrong: Vec<_> = (0..).zip(cases).filter(|(bit, _)| outcome.exit as u32 & (1 << bit) != 0).map(|(_, (name, ..))| *name).collect();
    assert!(wrong.is_empty(), "{form}: the lift differs from the definition on {wrong:?}");
}

/// `mtv` and `mfv` move a word's bits unchanged.
#[test]
fn vfpu_mtv_mfv() {
    let mut single = want(bits(2.5));
    single.extend([mtv(lane(C010, 1)), mfv(lane(C010, 1))]);
    let mut raw = want(0x1234_5678);
    raw.extend([mtv(98), mfv(98)]);
    let cases = [("a single", single, bits(2.5)), ("any word", raw, 0x1234_5678), ("viim.s", vec![viim(C030, 7), mfv(C030)], bits(7.0))];
    run_vfpu("mtv and mfv", &[], &cases);
}

/// `sv.s` stores a single's word and `lv.s` loads it, the register's row in the offset.
#[test]
fn vfpu_lv_sv_single() {
    let cases = [("sv.s", vec![vmem(58, C000, -16), lw(-16)], bits(1.0)), ("lv.s to S023", vec![vmem(50, 98, -16), mfv(98)], bits(1.0))];
    run_vfpu("lv.s and sv.s", &[viim(C000, 1)], &cases);
}

/// `sv.q` and `lv.q` move four lanes, a column's or, with the offset's bit 0, a row's.
#[test]
fn vfpu_lv_sv_quad() {
    let cases = [
        ("sv.q of a column", vec![vmem(62, C010, -32), lw(-24)], bits(30.0)),
        ("lv.q of a row", vec![vmem(54, R100, -32), mfv(6)], bits(30.0)),
        ("lv.q of a row, lane 3", vec![mfv(7)], bits(40.0)),
        ("lv.q of a column", vec![vmem(54, C020, -32), mfv(lane(C020, 1))], bits(20.0)),
        ("sv.q of a row", vec![vmem(62, R000, -48), lw(-44)], bits(10.0)),
    ];
    run_vfpu("lv.q and sv.q", &quads(), &cases);
}

/// `vadd`: `d = s + t` lane by lane, at every size.
#[test]
fn vfpu_vadd() {
    let cases = [
        ("vadd.q", vec![vop(VADD, Q, C020, C000, C010), mfv(C020)], bits(1.0 + 10.0)),
        ("vadd.q, lane 3", vec![mfv(lane(C020, 3))], bits(4.0 + 40.0)),
        ("vadd.s", vec![vop(VADD, S, C030, C000, C010), mfv(C030)], bits(1.0 + 10.0)),
        ("vadd.p", vec![vop(VADD, P, C030, C000, C010), mfv(lane(C030, 1))], bits(2.0 + 20.0)),
        ("vadd.t", vec![vop(VADD, T, C030, C000, C010), mfv(lane(C030, 2))], bits(3.0 + 30.0)),
    ];
    run_vfpu("vadd", &quads(), &cases);
}

/// `vsub`: `d = s - t`.
#[test]
fn vfpu_vsub() {
    let cases = [
        ("vsub.q", vec![vop(VSUB, Q, C020, C010, C000), mfv(C020)], bits(10.0 - 1.0)),
        ("vsub.q, lane 3", vec![mfv(lane(C020, 3))], bits(40.0 - 4.0)),
        ("vsub.s", vec![vop(VSUB, S, C030, C000, C010), mfv(C030)], bits(1.0 - 10.0)),
    ];
    run_vfpu("vsub", &quads(), &cases);
}

/// `vmul`: `d = s * t`.
#[test]
fn vfpu_vmul() {
    let cases = [
        ("vmul.q", vec![vop(VMUL, Q, C020, C000, C010), mfv(lane(C020, 1))], bits(2.0 * 20.0)),
        ("vmul.q, lane 3", vec![mfv(lane(C020, 3))], bits(4.0 * 40.0)),
        ("vmul.t", vec![vop(VMUL, T, C030, C000, C010), mfv(lane(C030, 2))], bits(3.0 * 30.0)),
    ];
    run_vfpu("vmul", &quads(), &cases);
}

/// `vdiv`: `d = s / t`, rounded as IEEE's division.
#[test]
fn vfpu_vdiv() {
    let cases = [
        ("vdiv.q", vec![vop(VDIV, Q, C020, C000, C010), mfv(C020)], bits(1.0 / 10.0)),
        ("vdiv.q, lane 2", vec![mfv(lane(C020, 2))], bits(3.0 / 30.0)),
        ("vdiv.s", vec![vop(VDIV, S, C030, C010, C000), mfv(C030)], bits(10.0 / 1.0)),
    ];
    run_vfpu("vdiv", &quads(), &cases);
}

/// `vscl`: each lane of `s` times the single `t`.
#[test]
fn vfpu_vscl() {
    let cases = [
        ("vscl.q", vec![vop(VSCL, Q, C020, C000, C010), mfv(C020)], bits(1.0 * 10.0)),
        ("vscl.q, lane 2", vec![mfv(lane(C020, 2))], bits(3.0 * 10.0)),
        ("vscl.p", vec![vop(VSCL, P, C030, C000, C010), mfv(lane(C030, 1))], bits(2.0 * 10.0)),
    ];
    run_vfpu("vscl", &quads(), &cases);
}

/// `vcrs.t`: the products a cross product pairs, `(s.y t.z, s.z t.x, s.x t.y)`.
#[test]
fn vfpu_vcrs() {
    let cases = [
        ("vcrs.t, x", vec![vop(VCRS, T, C020, C000, C010), mfv(C020)], bits(2.0 * 30.0)),
        ("vcrs.t, y", vec![mfv(lane(C020, 1))], bits(3.0 * 10.0)),
        ("vcrs.t, z", vec![mfv(lane(C020, 2))], bits(1.0 * 20.0)),
    ];
    run_vfpu("vcrs", &quads(), &cases);
}

/// `vcmp` sets a condition bit per lane, its "any" bit 4 and its "all" bit 5; `vcmovt` and
/// `vcmovf` move a lane where the bit they name is set or clear, with 6 each lane by its own.
#[test]
fn vfpu_vcmp_vcmov() {
    let mut setup = quads();
    setup.extend(column(C030, [1, 5, 3, -4]));
    // (1, 2, 3, 4) < (1, 5, 3, -4) on lane 1 alone; == on lanes 0 and 2.
    let less = vop(VCMP, Q, 2, C000, C030);
    let equal = vop(VCMP, Q, 1, C000, C030);
    let cases = [
        ("vcmp lt, vcmovt by lane", vec![vop(VZERO, Q, C020, 0, 0), less, vop(VCMOVT, Q, C020, C010, 6), mfv(lane(C020, 1))], bits(20.0)),
        ("vcmovt by lane, a false lane", vec![mfv(C020)], bits(0.0)),
        ("vcmovt on any", vec![vop(VCMOVT, S, C020, C000, 4), mfv(C020)], bits(1.0)),
        ("vcmovf on all", vec![vop(VCMOVF, S, C020, C010, 5), mfv(C020)], bits(10.0)),
        ("vcmp eq, vcmovf by lane", vec![equal, vop(VCMOVF, Q, C020, C010, 6), mfv(lane(C020, 3))], bits(40.0)),
    ];
    run_vfpu("vcmp and vcmov", &setup, &cases);
}

/// `C000` (1, 2, 3, 4) and `C030` (1, 5, 3, -4), the operands of the orders.
fn ordered() -> Vec<u32> {
    let mut setup = column(C000, [1, 2, 3, 4]);
    setup.extend(column(C030, [1, 5, 3, -4]));
    setup
}

/// `vmin`: the lesser of each lane.
#[test]
fn vfpu_vmin() {
    let cases = [
        ("vmin.q", vec![vop(VMIN, Q, C020, C000, C030), mfv(lane(C020, 1))], bits(2.0f32.min(5.0))),
        ("vmin.q, lane 3", vec![mfv(lane(C020, 3))], bits(4.0f32.min(-4.0))),
    ];
    run_vfpu("vmin", &ordered(), &cases);
}

/// `vmax`: the greater of each lane.
#[test]
fn vfpu_vmax() {
    let cases = [
        ("vmax.q", vec![vop(VMAX, Q, C020, C000, C030), mfv(lane(C020, 1))], bits(2.0f32.max(5.0))),
        ("vmax.q, lane 3", vec![mfv(lane(C020, 3))], bits(4.0f32.max(-4.0))),
    ];
    run_vfpu("vmax", &ordered(), &cases);
}

/// `vscmp`: the sign of `s - t` as a single, `0` where they are equal.
#[test]
fn vfpu_vscmp() {
    let cases = [
        ("vscmp.q, equal", vec![vop(VSCMP, Q, C020, C000, C030), mfv(C020)], bits(0.0)),
        ("vscmp.q, less", vec![mfv(lane(C020, 1))], bits(-1.0)),
        ("vscmp.q, greater", vec![mfv(lane(C020, 3))], bits(1.0)),
    ];
    run_vfpu("vscmp", &ordered(), &cases);
}

/// `vsge`: `1` where `s >= t`, else `0`.
#[test]
fn vfpu_vsge() {
    let cases = [
        ("vsge.q, less", vec![vop(VSGE, Q, C020, C000, C030), mfv(lane(C020, 1))], bits(0.0)),
        ("vsge.q, equal", vec![mfv(lane(C020, 2))], bits(1.0)),
        ("vsge.q, greater", vec![mfv(lane(C020, 3))], bits(1.0)),
    ];
    run_vfpu("vsge", &ordered(), &cases);
}

/// `vslt`: `1` where `s < t`, else `0`.
#[test]
fn vfpu_vslt() {
    let cases = [
        ("vslt.q, less", vec![vop(VSLT, Q, C020, C000, C030), mfv(lane(C020, 1))], bits(1.0)),
        ("vslt.q, equal", vec![mfv(lane(C020, 2))], bits(0.0)),
        ("vslt.q, greater", vec![mfv(lane(C020, 3))], bits(0.0)),
    ];
    run_vfpu("vslt", &ordered(), &cases);
}

/// `vmov`: `s` as it is, a row's lanes along its columns.
#[test]
fn vfpu_vmov() {
    let cases = [
        ("vmov.q", vec![vop(VMOV, Q, C020, C010, 0), mfv(lane(C020, 2))], bits(30.0)),
        ("vmov.s", vec![vop(VMOV, S, C030, lane(C000, 1), 0), mfv(C030)], bits(2.0)),
        ("vmov.t of a row", vec![vop(VMOV, T, C030, R000, 0), mfv(lane(C030, 1))], bits(10.0)),
    ];
    run_vfpu("vmov", &quads(), &cases);
}

/// `vabs`: the magnitude, `-0` to `0`.
#[test]
fn vfpu_vabs() {
    let setup = [viim(C030, -1), viim(lane(C030, 1), 5), viim(lane(C030, 2), -3), vfim(lane(C030, 3), 0x8000)];
    let cases = [
        ("vabs.q", vec![vop(VABS, Q, C020, C030, 0), mfv(C020)], bits(1.0)),
        ("vabs.q, lane 2", vec![mfv(lane(C020, 2))], bits(3.0)),
        ("vabs.q of -0", vec![mfv(lane(C020, 3))], bits(0.0)),
    ];
    run_vfpu("vabs", &setup, &cases);
}

/// `vneg`: the sign flipped, `0` to `-0`.
#[test]
fn vfpu_vneg() {
    let cases = [
        ("vneg.q", vec![vop(VNEG, Q, C020, C000, 0), mfv(C020)], bits(-1.0)),
        ("vneg.q, lane 3", vec![mfv(lane(C020, 3))], bits(-4.0)),
        ("vneg.s of 0", vec![vop(VZERO, S, C030, 0, 0), vop(VNEG, S, C020, C030, 0), mfv(C020)], bits(-0.0)),
    ];
    run_vfpu("vneg", &column(C000, [1, 2, 3, 4]), &cases);
}

/// The lanes of the saturations, 0.5, 5, -0.5 and -4.
fn saturated() -> [u32; 4] {
    [vfim(C030, 0x3800), viim(lane(C030, 1), 5), vfim(lane(C030, 2), 0xb800), viim(lane(C030, 3), -4)]
}

/// `vsat0`: each lane clamped to `[0, 1]`, a negative one to `0`.
#[test]
fn vfpu_vsat0() {
    let cases = [
        ("vsat0.q, inside", vec![vop(VSAT0, Q, C020, C030, 0), mfv(C020)], bits(0.5)),
        ("vsat0.q, over", vec![mfv(lane(C020, 1))], bits(1.0)),
        ("vsat0.q, under", vec![mfv(lane(C020, 2))], bits(0.0)),
        ("vsat0.q, under 1", vec![mfv(lane(C020, 3))], bits(0.0)),
    ];
    run_vfpu("vsat0", &saturated(), &cases);
}

/// `vsat1`: each lane clamped to `[-1, 1]`.
#[test]
fn vfpu_vsat1() {
    let cases = [
        ("vsat1.q, inside", vec![vop(VSAT1, Q, C020, C030, 0), mfv(C020)], bits(0.5)),
        ("vsat1.q, over", vec![mfv(lane(C020, 1))], bits(1.0)),
        ("vsat1.q, inside below 0", vec![mfv(lane(C020, 2))], bits(-0.5)),
        ("vsat1.q, under", vec![mfv(lane(C020, 3))], bits(-1.0)),
    ];
    run_vfpu("vsat1", &saturated(), &cases);
}

/// `vidt`: the row of an identity matrix the register's column names.
#[test]
fn vfpu_vidt() {
    let cases = [
        ("vidt.q, its one", vec![vop(VIDT, Q, C020, 0, 0), mfv(lane(C020, 2))], bits(1.0)),
        ("vidt.q, a zero", vec![mfv(C020)], bits(0.0)),
        ("vidt.p, its one", vec![vop(VIDT, P, C010, 0, 0), mfv(lane(C010, 1))], bits(1.0)),
        ("vidt.p, its zero", vec![mfv(C010)], bits(0.0)),
    ];
    run_vfpu("vidt", &[], &cases);
}

/// `vzero`: its lanes `0`, the others as they are.
#[test]
fn vfpu_vzero() {
    let cases = [
        ("vzero.t", vec![vop(VONE, Q, C020, 0, 0), vop(VZERO, T, C020, 0, 0), mfv(C020)], bits(0.0)),
        ("vzero.t, past its lanes", vec![mfv(lane(C020, 3))], bits(1.0)),
    ];
    run_vfpu("vzero", &[], &cases);
}

/// `vone`: its lanes `1`, the others as they are.
#[test]
fn vfpu_vone() {
    let cases = [
        ("vone.p", vec![vop(VZERO, Q, C020, 0, 0), vop(VONE, P, C020, 0, 0), mfv(lane(C020, 1))], bits(1.0)),
        ("vone.p, past its lanes", vec![mfv(lane(C020, 2))], bits(0.0)),
    ];
    run_vfpu("vone", &[], &cases);
}

/// `vcst`: the constant of its index in every lane.
#[test]
fn vfpu_vcst() {
    let cases = [
        ("vcst.s pi", vec![vop(VCST, S, C020, 0, 9), mfv(C020)], bits(std::f32::consts::PI)),
        ("vcst.q e", vec![vop(VCST, Q, C030, 0, 10), mfv(lane(C030, 3))], bits(std::f32::consts::E)),
        ("vcst.s the largest single", vec![vop(VCST, S, C020, 0, 1), mfv(C020)], bits(f32::MAX)),
    ];
    run_vfpu("vcst", &[], &cases);
}

/// `vbfy1`: `(s.x + s.y, s.x - s.y, s.z + s.w, s.z - s.w)`.
#[test]
fn vfpu_vbfy1() {
    let cases = [
        ("vbfy1.q, x", vec![vop(VBFY1, Q, C020, C000, 0), mfv(C020)], bits(1.0 + 2.0)),
        ("vbfy1.q, y", vec![mfv(lane(C020, 1))], bits(1.0 - 2.0)),
        ("vbfy1.q, z", vec![mfv(lane(C020, 2))], bits(3.0 + 4.0)),
        ("vbfy1.q, w", vec![mfv(lane(C020, 3))], bits(3.0 - 4.0)),
        ("vbfy1.p, y", vec![vop(VBFY1, P, C030, C000, 0), mfv(lane(C030, 1))], bits(1.0 - 2.0)),
    ];
    run_vfpu("vbfy1", &column(C000, [1, 2, 3, 4]), &cases);
}

/// `vbfy2`: `(s.x + s.z, s.y + s.w, s.x - s.z, s.y - s.w)`.
#[test]
fn vfpu_vbfy2() {
    let cases = [
        ("vbfy2.q, x", vec![vop(VBFY2, Q, C020, C000, 0), mfv(C020)], bits(1.0 + 3.0)),
        ("vbfy2.q, y", vec![mfv(lane(C020, 1))], bits(2.0 + 4.0)),
        ("vbfy2.q, z", vec![mfv(lane(C020, 2))], bits(1.0 - 3.0)),
        ("vbfy2.q, w", vec![mfv(lane(C020, 3))], bits(2.0 - 4.0)),
    ];
    run_vfpu("vbfy2", &column(C000, [1, 2, 3, 4]), &cases);
}

/// `S030` 2.5, `S031` -2.5 and `S032` 3.5, the operands of the conversions to words.
fn halves() -> [u32; 3] {
    [vfim(C030, 0x4100), vfim(lane(C030, 1), 0xc100), vfim(lane(C030, 2), 0x4300)]
}

/// `vf2in`: `s * 2^scale` to the nearest word, a tie to the even one, saturating.
#[test]
fn vfpu_vf2in() {
    let cases = [
        ("vf2in.s 2.5", vec![vop(VF2IN, S, C020, C030, 0), mfv(C020)], word(2.5f64.round_ties_even())),
        ("vf2in.s 3.5", vec![vop(VF2IN, S, C020, lane(C030, 2), 0), mfv(C020)], word(3.5f64.round_ties_even())),
        ("vf2in.s -2.5 by 2^31", vec![vop(VF2IN, S, C020, lane(C030, 1), 31), mfv(C020)], word((-2.5 * 2f64.powi(31)).round_ties_even())),
    ];
    run_vfpu("vf2in", &halves(), &cases);
}

/// `vf2iz`: `s * 2^scale` toward zero, saturating.
#[test]
fn vfpu_vf2iz() {
    let cases = [
        ("vf2iz.s -2.5", vec![vop(VF2IZ, S, C020, lane(C030, 1), 0), mfv(C020)], word((-2.5f64).trunc())),
        ("vf2iz.s 2.5 by 2^4", vec![vop(VF2IZ, S, C020, C030, 4), mfv(C020)], word((2.5 * 2f64.powi(4)).trunc())),
        ("vf2iz.s 2.5 by 2^31", vec![vop(VF2IZ, S, C020, C030, 31), mfv(C020)], word((2.5 * 2f64.powi(31)).trunc())),
    ];
    run_vfpu("vf2iz", &halves(), &cases);
}

/// `vf2iu`: `s * 2^scale` up.
#[test]
fn vfpu_vf2iu() {
    let cases = [
        ("vf2iu.s 2.5", vec![vop(VF2IU, S, C020, C030, 0), mfv(C020)], word(2.5f64.ceil())),
        ("vf2iu.s -2.5", vec![vop(VF2IU, S, C020, lane(C030, 1), 0), mfv(C020)], word((-2.5f64).ceil())),
    ];
    run_vfpu("vf2iu", &halves(), &cases);
}

/// `vf2id`: `s * 2^scale` down.
#[test]
fn vfpu_vf2id() {
    let cases = [
        ("vf2id.s 2.5", vec![vop(VF2ID, S, C020, C030, 0), mfv(C020)], word(2.5f64.floor())),
        ("vf2id.s -2.5", vec![vop(VF2ID, S, C020, lane(C030, 1), 0), mfv(C020)], word((-2.5f64).floor())),
    ];
    run_vfpu("vf2id", &halves(), &cases);
}

/// `vi2f`: each word as a single, times `2^-scale`.
#[test]
fn vfpu_vi2f() {
    let mut setup = want(6);
    setup.extend([mtv(C030), i_type(0x09, 0, WANT, (-8i16) as u16), mtv(lane(C030, 1))]);
    let cases = [
        ("vi2f.s by 2^-1", vec![vop(VI2F, S, C020, C030, 1), mfv(C020)], bits(6.0 / 2.0)),
        ("vi2f.s of a negative", vec![vop(VI2F, S, C020, lane(C030, 1), 0), mfv(C020)], bits(-8.0)),
        ("vi2f.p by 2^-2", vec![vop(VI2F, P, C020, C030, 2), mfv(lane(C020, 1))], bits(-8.0 / 4.0)),
    ];
    run_vfpu("vi2f", &setup, &cases);
}

/// `vpfxs`: each source lane one of the vector's or a constant, then its magnitude, then negated.
#[test]
fn vfpu_vpfxs() {
    let cases = [
        // Lane 0 is lane 1 negated.
        ("swizzle and negate", vec![vpfx(0, 0x01_00e5), vop(VMOV, Q, C020, C000, 0), mfv(C020)], bits(-2.0)),
        ("the lanes it leaves", vec![mfv(lane(C020, 2))], bits(3.0)),
        ("magnitude", vec![vop(VNEG, Q, C030, C000, 0), vpfx(0, 0x02e4), vop(VMOV, Q, C020, C030, 0), mfv(lane(C020, 1))], bits(2.0)),
        // Lane 3 is the constant of selector 3 with the magnitude bit.
        ("constant", vec![vpfx(0, 0x88e4), vop(VMOV, Q, C020, C000, 0), mfv(lane(C020, 3))], bits(1.0 / 6.0)),
    ];
    run_vfpu("vpfxs", &column(C000, [1, 2, 3, 4]), &cases);
}

/// `vpfxt`: the same on the second operand.
#[test]
fn vfpu_vpfxt() {
    let cases = [
        // Lane 0 is the constant 3.
        ("constant", vec![vpfx(1, 0x11e4), vop(VADD, Q, C020, C000, C010), mfv(C020)], bits(1.0 + 3.0)),
        // Lane 0 is lane 3.
        ("swizzle", vec![vpfx(1, 0xe7), vop(VSUB, Q, C020, C010, C000), mfv(C020)], bits(10.0 - 4.0)),
        ("negate", vec![vpfx(1, 0x02_00e4), vop(VADD, Q, C020, C000, C010), mfv(lane(C020, 1))], bits(2.0 - 20.0)),
    ];
    run_vfpu("vpfxt", &quads(), &cases);
}

/// `vpfxd`: a masked lane keeps its value, a saturated one is clamped.
#[test]
fn vfpu_vpfxd() {
    let cases = [
        ("mask", vec![vop(VONE, Q, C020, 0, 0), vpfx(2, 0x200), vop(VMOV, Q, C020, C010, 0), mfv(lane(C020, 1))], bits(1.0)),
        ("the lanes it does not mask", vec![mfv(C020)], bits(10.0)),
        ("saturate to [-1, 1]", vec![vpfx(2, 0x30), vop(VMOV, Q, C020, C010, 0), mfv(lane(C020, 2))], bits(1.0)),
        ("saturate to [0, 1]", vec![vop(VNEG, Q, C030, C000, 0), vpfx(2, 0x01), vop(VMOV, Q, C020, C030, 0), mfv(C020)], bits(0.0)),
    ];
    run_vfpu("vpfxd", &quads(), &cases);
}

/// `viim.s`: a single of a 16-bit integer.
#[test]
fn vfpu_viim() {
    let cases = [
        ("a negative", vec![viim(C020, -5), mfv(C020)], bits(-5.0)),
        ("the largest", vec![viim(C020, i16::MAX), mfv(C020)], bits(f32::from(i16::MAX))),
    ];
    run_vfpu("viim", &[], &cases);
}

/// `vfim.s`: a single of a half, exactly.
#[test]
fn vfpu_vfim() {
    let cases = [
        ("1.5", vec![vfim(C020, 0x3e00), mfv(C020)], bits(1.5)),
        ("-2", vec![vfim(C020, 0xc000), mfv(C020)], bits(-2.0)),
        ("an infinity", vec![vfim(C020, 0x7c00), mfv(C020)], bits(f32::INFINITY)),
        ("the least subnormal", vec![vfim(C020, 0x0001), mfv(C020)], bits(2f32.powi(-24))),
    ];
    run_vfpu("vfim", &[], &cases);
}

/// `vmscl`: each element of `s` times the single `t`. `M100`'s element is the one of `M000` four
/// singles before it.
#[test]
fn vfpu_vmscl() {
    let cases = [
        ("vmscl.q", vec![vop(VMSCL, Q, M100, M000, C010), mfv(36)], bits(2.0 * 10.0)),
        ("vmscl.q, row 0", vec![mfv(5)], bits(10.0 * 10.0)),
        ("vmscl.q, the last", vec![mfv(101)], bits(40.0 * 10.0)),
    ];
    run_vfpu("vmscl", &quads(), &cases);
}

/// `vmmov`: `s` as it is, or its transpose.
#[test]
fn vfpu_vmmov() {
    let cases = [
        ("vmmov.q", vec![vop(VMMOV, Q, M100, M000, 0), mfv(69)], bits(30.0)),
        ("vmmov.q of a transpose", vec![vop(VMMOV, Q, M100, E000, 0), mfv(5)], bits(2.0)),
        ("vmmov.q of a transpose, its mirror", vec![mfv(36)], bits(10.0)),
    ];
    run_vfpu("vmmov", &quads(), &cases);
}

/// `vmidt`: an identity matrix of its size, the elements past it as they are.
#[test]
fn vfpu_vmidt() {
    let cases = [
        ("vmidt.t, the diagonal", vec![vop(VMONE, Q, M100, 0, 0), vop(VMIDT, T, M100, 0, 0), mfv(4)], bits(1.0)),
        ("vmidt.t, off it", vec![mfv(5)], bits(0.0)),
        ("vmidt.t, row 1", vec![mfv(37)], bits(1.0)),
        ("vmidt.t, past its size", vec![mfv(7)], bits(1.0)),
    ];
    run_vfpu("vmidt", &[], &cases);
}

/// `vmzero`: a matrix of zeros of its size.
#[test]
fn vfpu_vmzero() {
    let cases = [
        ("vmzero.p", vec![vop(VMONE, Q, M100, 0, 0), vop(VMZERO, P, M100, 0, 0), mfv(37)], bits(0.0)),
        ("vmzero.p, past its size", vec![mfv(70)], bits(1.0)),
    ];
    run_vfpu("vmzero", &[], &cases);
}

/// `vmone`: a matrix of ones of its size.
#[test]
fn vfpu_vmone() {
    let cases = [
        ("vmone.t", vec![vop(VMZERO, Q, M100, 0, 0), vop(VMONE, T, M100, 0, 0), mfv(70)], bits(1.0)),
        ("vmone.t, past its size", vec![mfv(103)], bits(0.0)),
    ];
    run_vfpu("vmone", &[], &cases);
}

/// `vnop`, `vsync` and `vflush` change nothing.
#[test]
fn vfpu_nop_sync_flush() {
    let cases = [("vnop, vsync, vflush", vec![0xffff_0000, 0xffff_0320, 0xffff_040d, mfv(C010)], bits(10.0))];
    run_vfpu("vnop, vsync and vflush", &[viim(C010, 10)], &cases);
}
