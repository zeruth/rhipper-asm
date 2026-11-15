//! The csmith programs under `testdata/csmith/bin` through the C output: each one lifted, raised
//! to the ABI level, written as C, compiled with clang against the driver and the runtime header,
//! run, and its line compared with the checksum its native run printed. A program whose port
//! differs is listed in `testdata/csmith/EMIT_PENDING.txt` with the stage at which it does (the
//! emit, the build or the run), and the list must be exactly those: a program fixed leaves it, a
//! program broken joins it, and either way the test says which lines to write.
//!
//! The committed programs are the regression set. `RA_CSMITH_BIN` names another directory of
//! programs built by `testdata/csmith/build.ps1` (the nightly's fresh seeds): there the pending
//! list and the count do not apply, and any program that differs fails the test.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const PROGRAMS: usize = 128;
/// The stages a port goes through, in order; a program is listed at the first that fails.
const STAGES: [&str; 3] = ["emit", "build", "run"];
/// The variable that names a directory of fresh programs instead of the committed ones.
const FRESH: &str = "RA_CSMITH_BIN";
/// A build of a port has this long, and its run `RUN_LIMIT`: the native program ends within two
/// seconds (`build.ps1` drops a seed that does not), so a port past a minute loops.
const BUILD_LIMIT: Duration = Duration::from_secs(600);
const RUN_LIMIT: Duration = Duration::from_secs(60);
/// The stack of a worker: the lift and the raise recurse over a program's expressions, and a
/// csmith program nests them deeper than the default stack holds.
const STACK: usize = 256 << 20;
/// The longest note a line of the pending list keeps.
const NOTE: usize = 160;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `text` on one line of at most [`NOTE`] characters.
fn note(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= NOTE {
        return line;
    }
    let mut clipped: String = line.chars().take(NOTE - 1).collect();
    clipped.push('…');
    clipped
}

/// The port of the program at `path` written as C into a scratch directory of its own, and the
/// program's pointer width.
fn emit(path: &Path) -> Result<(ra_asm_testing::Scratch, u32), String> {
    let image = ra_asm_pe::load(path).map_err(|e| format!("the image does not load: {e:#}"))?;
    let stem = path.file_stem().unwrap().to_string_lossy();
    let emitted = catch_unwind(AssertUnwindSafe(|| {
        let analysis = ra_asm_analysis::analyze(&image);
        let lift_options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, lift_options);
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let infos = ra_asm_emit::functions::infos(&analysis);
        let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
        let classes = ra_asm_emit::classes(&analysis, &signatures);
        let options = ra_asm_emit::COptions { names, dialect: ra_asm_emit::Dialect::C, classes, ..Default::default() };
        let unit = ra_asm_emit::emit_unit(&lifted.module, &options);
        let files = unit.files(ra_asm_emit::FUNCTIONS_PER_FILE);
        let dir = ra_asm_testing::Scratch::new("ra-asm-emit-csmith", &stem);
        ra_asm_emit::write_files(&dir, &files).unwrap();
        for (name, bytes) in &unit.blobs {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures))).unwrap();
        dir
    }));
    let dir = emitted.map_err(|e| {
        let message = e.downcast_ref::<String>().map(String::as_str).or_else(|| e.downcast_ref::<&str>().copied()).unwrap_or("");
        format!("the lift, the raise or the emit panicked: {message}")
    })?;
    Ok((dir, u32::from(image.pointer_size()) * 8))
}

/// Reads a pipe of the child to its end on a thread of its own.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// The output of `command`, run for at most `limit`: past it the process is killed and its stderr
/// ends with a line that says how long it had.
fn output_within(command: &mut Command, limit: Duration) -> std::process::Output {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = Instant::now();
    let mut killed = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            killed = true;
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().unwrap();
    let mut stderr = stderr.join().unwrap();
    if killed {
        stderr.extend_from_slice(format!("\nthe process had not ended after {} s and was killed\n", limit.as_secs()).as_bytes());
    }
    std::process::Output { status, stdout, stderr }
}

/// The last line of `bytes` that is not blank, or `nothing`.
fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().rfind(|line| !line.trim().is_empty()).unwrap_or("nothing").trim().to_owned()
}

/// The first stage at which the port of the program at `path` differs from the native run, and
/// how, or `None` when it prints the native line and exits with 0.
fn differs(path: &Path) -> Option<(&'static str, String)> {
    let expected = std::fs::read_to_string(path.with_extension("txt")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (dir, bits) = match emit(path) {
        Ok(port) => port,
        Err(how) => return Some(("emit", how)),
    };
    let sources: Vec<PathBuf> =
        std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "c")).collect();
    let exe = dir.join("program.exe");
    // As the emit oracle builds a port: warning-free under the strict flags, the 32-bit program's
    // port without large-address awareness so that it keeps its 32-bit pointers.
    let mut build = Command::new("clang");
    build.args(["-O1", "-Wall", "-Wextra", "-Werror", "-std=c23", "-fuse-ld=lld", "-DRA_TRACE_LAST"]);
    if bits == 32 {
        build.arg("-Wl,/largeaddressaware:no");
    }
    build
        .args(&sources)
        .arg(root().join("runtime/driver.c"))
        .arg("-I")
        .arg(dir.path())
        .arg("-I")
        .arg(root().join("runtime"))
        .arg("-o")
        .arg(&exe);
    build.args(["-lkernel32", "-luser32", "-lshell32", "-lws2_32", "-lole32", "-loleaut32", "-ladvapi32"]);
    let built = output_within(&mut build, BUILD_LIMIT);
    if !built.status.success() {
        let stderr = String::from_utf8_lossy(&built.stderr);
        let lines = || stderr.lines().map(str::trim);
        let first = lines().find(|l| l.contains("error")).or_else(|| lines().find(|l| !l.is_empty())).unwrap_or("no message on stderr");
        let first = first.replace(&format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR), "");
        return Some(("build", format!("clang: {first}")));
    }
    let run = output_within(Command::new(&exe).current_dir(dir.path()), RUN_LIMIT);
    let printed = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    if printed.trim_end() == expected.trim_end() && run.status.success() {
        return None;
    }
    let line = printed.trim_end().lines().last().unwrap_or("nothing").to_owned();
    Some(("run", format!("printed {line:?}, {}, stderr: {}", run.status, last_line(&run.stderr))))
}

/// The program and the stage of each line of the pending list; `#` starts a comment and what
/// follows the two words on a line is a note.
fn pending(text: &str) -> BTreeMap<String, String> {
    let mut listed = BTreeMap::new();
    for line in text.lines().map(|line| line.split('#').next().unwrap_or("").trim()).filter(|line| !line.is_empty()) {
        let mut words = line.split_whitespace();
        let (Some(program), Some(stage)) = (words.next(), words.next()) else {
            panic!("EMIT_PENDING.txt: {line:?} is not `<program> <stage>`");
        };
        assert!(STAGES.contains(&stage), "EMIT_PENDING.txt: {line:?}: {stage} is not a stage");
        assert!(listed.insert(program.to_owned(), stage.to_owned()).is_none(), "EMIT_PENDING.txt lists {program} twice");
    }
    listed
}

#[test]
fn the_csmith_programs_run_as_c() {
    if !cfg!(windows) {
        eprintln!("SKIPPED: the port of a Windows program runs on a Windows host: the csmith ports are skipped");
        return;
    }
    if ra_asm_testing::tool("clang").is_none() {
        return;
    }
    let fresh = std::env::var_os(FRESH).map(PathBuf::from);
    let bin = fresh.clone().unwrap_or_else(|| root().join("testdata/csmith/bin"));
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&bin)
        .unwrap_or_else(|e| panic!("{}: {e} (testdata/csmith/build.ps1 writes it)", bin.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "exe"))
        .collect();
    programs.sort();
    if fresh.is_none() {
        assert_eq!(programs.len(), PROGRAMS, "programs under {}", bin.display());
    }
    for path in &programs {
        assert!(path.with_extension("txt").is_file(), "{}: no expected output beside it", path.display());
    }

    // Each worker emits, builds and runs one program at a time; clang's memory, not the cores,
    // bounds how many run side by side on this machine.
    let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Option<(&'static str, String)>>>> = programs.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(STACK)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = programs.get(i) else { break };
                        let result = differs(path);
                        match &result {
                            None => eprintln!("{}: equal", path.display()),
                            Some((stage, how)) => eprintln!("{} ({stage}): {how}", path.display()),
                        }
                        *slots[i].lock().unwrap() = Some(result);
                    }
                })
                .expect("a worker thread");
        }
    });

    // program -> (the first stage that differs, how)
    let mut differing: BTreeMap<String, (&str, String)> = BTreeMap::new();
    let mut ran = 0;
    for (path, slot) in programs.iter().zip(slots) {
        let result = slot.into_inner().unwrap().expect("every program went through");
        ran += 1;
        if let Some((stage, how)) = result {
            differing.insert(path.file_stem().unwrap().to_str().unwrap().to_owned(), (stage, how));
        }
    }
    let count = |stage: &str| differing.values().filter(|(s, _)| *s == stage).count();
    eprintln!(
        "csmith as C: {} of {ran} programs equal; {} differ at the emit, {} at the build, {} at the run",
        ran - differing.len(),
        count("emit"),
        count("build"),
        count("run")
    );
    let lines: String = differing.iter().map(|(program, (stage, how))| format!("{program} {stage} # {}\n", note(how))).collect();

    if fresh.is_some() {
        assert!(differing.is_empty(), "the ports of these programs under {} differ:\n{lines}", bin.display());
        return;
    }
    assert_eq!(ran, PROGRAMS, "the csmith programs built and run");
    let path = root().join("testdata/csmith/EMIT_PENDING.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let listed = pending(&text);
    let found: BTreeMap<String, String> = differing.iter().map(|(program, (stage, _))| (program.clone(), (*stage).to_owned())).collect();
    assert!(
        listed == found,
        "{} lists {listed:?}, but these differ (the list's lines, to write under its header):\n{lines}",
        path.display()
    );
}
