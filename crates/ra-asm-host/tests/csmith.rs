//! The csmith programs under `testdata/csmith/bin` run in the interpreter: each one lifted, raised
//! to the machine, promoted and ABI levels in turn and interpreted, its output compared with the
//! checksum its native run printed. A program that differs is listed in `testdata/csmith/PENDING.txt`
//! with the first level at which it differs, and the list must be exactly those: a program fixed
//! leaves it, a program broken joins it, and either way the test says which lines to write.
//!
//! Each program runs in a child process of the test, since a run can end its process (an
//! overflow of the host's stack) and one program must not hide the others.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_lift::RaiseLevel;

const PROGRAMS: usize = 128;
const LEVELS: [(&str, RaiseLevel); 3] = [("machine", RaiseLevel::Machine), ("promoted", RaiseLevel::Promoted), ("abi", RaiseLevel::Abi)];
const TEST: &str = "the_csmith_programs_run_interpreted_at_three_levels";
/// The variable that makes the test the child run of the one program it names.
const ONE: &str = "RA_CSMITH_PROGRAM";
/// The lines a child run prints: the level it starts, and how the program differs there.
const STARTS: &str = "csmith-level: ";
const DIFFERS: &str = "csmith-differs: ";
/// IR instructions a run may take: a csmith program is a loop over its globals, and one that
/// runs past this is listed rather than holding the fast tier.
const LIMIT: u64 = 10_000_000;

fn csmith() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csmith")
}

/// How the run of `path` at `level` differs from the native one, if it does.
fn differs(path: &Path, level: RaiseLevel, expected: &str) -> Option<String> {
    let image = match ra_asm_pe::load(path) {
        Ok(image) => image,
        Err(e) => return Some(format!("the image does not load: {e:#}")),
    };
    let raised = catch_unwind(AssertUnwindSafe(|| {
        let analysis = ra_asm_analysis::analyze(&image);
        let options = ra_asm_lift::LiftOptions { standalone: level == RaiseLevel::Abi, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        let options = lifted.raise_options(level);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        (lifted, signatures)
    }));
    let Ok((lifted, signatures)) = raised else {
        return Some("the lift or the raise panicked".to_owned());
    };
    let options = ra_asm_host::Options { limit: LIMIT, ..ra_asm_host::Options::default() };
    let outcome = match ra_asm_host::run(&image, &lifted, &signatures, &options) {
        Ok(outcome) => outcome,
        Err(e) => return Some(format!("the interpreter failed: {e:#}")),
    };
    let output = String::from_utf8_lossy(&outcome.output);
    if output.trim_end() == expected.trim_end() && outcome.exit == 0 {
        return None;
    }
    let printed = output.trim_end().lines().last().unwrap_or("nothing").to_owned();
    Some(format!("printed {printed:?}, exit {:#x}, stop: {:?}, {} steps", outcome.exit, outcome.stop, outcome.steps))
}

/// The child run: the program at `path` at each level in turn, up to the first that differs. The
/// level is printed before its run, so a run that ends the process leaves it behind.
fn run_one(path: &Path) {
    let expected = std::fs::read_to_string(path.with_extension("txt")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for &(name, level) in &LEVELS {
        println!("{STARTS}{name}");
        if let Some(how) = differs(path, level, &expected) {
            println!("{DIFFERS}{name} {how}");
            return;
        }
    }
}

fn level_named(name: &str) -> Option<&'static str> {
    LEVELS.iter().map(|&(level, _)| level).find(|&level| level == name)
}

/// The first level at which the program at `path` differs and how, from a child run of its own.
fn first_difference(path: &Path) -> Option<(&'static str, String)> {
    let output = Command::new(std::env::current_exe().expect("the test's own path"))
        .args([TEST, "--exact", "--nocapture", "--test-threads=1"])
        .env(ONE, path)
        .output()
        .unwrap_or_else(|e| panic!("cannot start the run of {}: {e}", path.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut started = None;
    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix(STARTS) {
            started = level_named(name);
        } else if let Some(rest) = line.strip_prefix(DIFFERS) {
            let (name, how) = rest.split_once(' ').unwrap_or((rest, ""));
            let level = level_named(name).unwrap_or_else(|| panic!("the run of {} names no level: {line:?}", path.display()));
            return Some((level, how.to_owned()));
        }
    }
    if !output.status.success() {
        let level = started.unwrap_or(LEVELS[0].0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr.lines().rfind(|line| !line.trim().is_empty()).unwrap_or("nothing").trim();
        return Some((level, format!("the run ended its process ({}): {last}", output.status)));
    }
    assert_eq!(started, Some(LEVELS[LEVELS.len() - 1].0), "the run of {} did not reach the last level:\n{stdout}", path.display());
    None
}

/// The program and the level of each line of the pending list; `#` starts a comment and what
/// follows the two words on a line is a note.
fn pending(text: &str) -> BTreeMap<String, String> {
    let mut listed = BTreeMap::new();
    for line in text.lines().map(|line| line.split('#').next().unwrap_or("").trim()).filter(|line| !line.is_empty()) {
        let mut words = line.split_whitespace();
        let (Some(program), Some(level)) = (words.next(), words.next()) else {
            panic!("PENDING.txt: {line:?} is not `<program> <level>`");
        };
        assert!(LEVELS.iter().any(|&(name, _)| name == level), "PENDING.txt: {line:?}: {level} is not a level");
        assert!(listed.insert(program.to_owned(), level.to_owned()).is_none(), "PENDING.txt lists {program} twice");
    }
    listed
}

#[test]
fn the_csmith_programs_run_interpreted_at_three_levels() {
    if let Some(path) = std::env::var_os(ONE) {
        run_one(Path::new(&path));
        return;
    }
    let bin = csmith().join("bin");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&bin)
        .unwrap_or_else(|e| panic!("{}: {e} (testdata/csmith/build.ps1 writes it)", bin.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "exe"))
        .collect();
    programs.sort();
    assert_eq!(programs.len(), PROGRAMS, "programs under {}", bin.display());

    for path in &programs {
        assert!(path.with_extension("txt").is_file(), "{}: no expected output beside it", path.display());
    }
    // Each program runs in a child process of its own, so the children run side by side: one at
    // a time took 36 minutes, past the check's half hour.
    let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(2, 6));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Option<(&'static str, String)>>>> =
        programs.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(path) = programs.get(i) else { break };
                    *slots[i].lock().unwrap() = Some(first_difference(path));
                }
            });
        }
    });
    let results = slots.into_iter().map(|slot| slot.into_inner().unwrap().expect("every program ran"));

    // program -> (the first level that differs, how)
    let mut differing: BTreeMap<String, (&str, String)> = BTreeMap::new();
    let mut runs = 0;
    for (path, result) in programs.iter().zip(results) {
        let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let Some((level, how)) = result else {
            runs += LEVELS.len();
            continue;
        };
        runs += LEVELS.iter().position(|&(name, _)| name == level).map_or(LEVELS.len(), |i| i + 1);
        if !cfg!(windows) && how.contains("no binding for ") {
            eprintln!("SKIPPED: {stem}: {how}; this host has no Windows DLLs: the interpreted runs are skipped");
            return;
        }
        eprintln!("{stem} ({level}): {how}");
        differing.insert(stem, (level, how));
    }
    let passed = PROGRAMS - differing.len();
    eprintln!("csmith: {passed} of {PROGRAMS} programs equal at the three levels, {runs} runs");

    let path = csmith().join("PENDING.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let listed = pending(&text);
    let found: BTreeMap<String, String> = differing.iter().map(|(program, (level, _))| (program.clone(), (*level).to_owned())).collect();
    if listed != found {
        let lines: String = differing.iter().map(|(program, (level, how))| format!("{program} {level} # {how}\n")).collect();
        panic!("{} lists {:?}, but these differ:\n{lines}", path.display(), listed);
    }
}
