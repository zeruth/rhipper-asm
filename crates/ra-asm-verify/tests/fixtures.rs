//! The committed vectors of the fixtures, replayed: `testdata/vectors/fn/<name>.fv` holds the
//! vectors of one interpreted run of `testdata/bin/<name>.exe` at the machine level, as
//! `ras verify <program> --write testdata/vectors/fn` writes them, and every one of them replays
//! at the machine, the promoted and the ABI level with no run of the program. A change to the
//! raise is checked against every function the fixtures' runs entered, and each difference is a
//! line that names the program, the function, the level and the vector.
//!
//! What the replay does not decide yet (a global of the lift's, the pops of an x86 callee, a
//! callee the ABI raise put in place of the record's) is a function and level of
//! `testdata/vectors/fn/known-differences.txt`: any other that differs fails the test, and so
//! does one of the file that replays now, so the file only shrinks.
//!
//! A program whose file is not there yet, or was kept otherwise than the recorder keeps now, is
//! recorded once and its file written, to be committed; that run of the test says so, writes the
//! known differences anew and does not hold itself to the time a replay alone takes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::{Replayer, Vector};

/// The programs of `testdata/bin` that run to their end through the interpreter on their own:
/// no argument, no file of their own, no window and no second image to load.
const PROGRAMS: &[&str] = &[
    "hello32",
    "hello64",
    "control32",
    "control64",
    "control64-O1",
    "float32",
    "float64",
    "obfusc32",
    "obfusc64",
    "extended32",
    "classes32",
    "classes64",
    "throws64",
    "inline32",
    "inline64",
    "tls32",
    "tls64",
    "crt64",
    "resources32",
    "resources64",
    "structs32",
    "structs64",
    "dispatch32",
    "dispatch64",
    "computed32",
    "computed64",
];

const PROGRAMS_AT_LEAST: usize = 20;
const VECTORS_AT_LEAST: usize = 400;
const REPLAY_WITHIN: Duration = Duration::from_secs(60);

/// The functions and levels whose vectors differ for a reason the replay does not decide yet
/// (see the module of the replayer), one line each as the test prints it.
const KNOWN: &str = "testdata/vectors/fn/known-differences.txt";

const KNOWN_HEADER: &str = "\
# The functions of the fixtures whose committed vectors differ at a level, one line each as the
# fixtures test of ra-asm-verify prints it: what the replay does not decide yet. The test fails
# on a function and level that differs and is not here, and on one here that replays now; the
# test writes the file anew when it records the vectors again.
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What one program's vectors did.
#[derive(Default)]
struct Replayed {
    vectors: usize,
    /// One line for each function and level that differed: its first difference there.
    differences: Vec<String>,
}

#[test]
fn every_committed_vector_of_the_fixtures_replays_at_each_level() {
    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(&str, Replayed)>> = Mutex::new(Vec::new());
    let seeded: Mutex<Vec<&str>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(4);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|n| {
                // The interpreter recurses with the program's calls, past a test thread's stack.
                std::thread::Builder::new()
                    .name(format!("fixtures-{n}"))
                    .stack_size(256 << 20)
                    .spawn_scoped(scope, || {
                        while let Some(name) = PROGRAMS.get(next.fetch_add(1, Ordering::Relaxed)).copied() {
                            let Some((vectors, recorded)) = vectors_of(name) else { continue };
                            if recorded {
                                seeded.lock().unwrap().push(name);
                            }
                            let replayed = replay(name, &vectors);
                            results.lock().unwrap().push((name, replayed));
                        }
                    })
                    .expect("a worker starts")
            })
            .collect();
        for handle in handles {
            handle.join().expect("a worker ends");
        }
    });
    let elapsed = start.elapsed();

    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(name, _)| PROGRAMS.iter().position(|program| program == name));
    let seeded = seeded.into_inner().unwrap();
    let programs = results.len();
    let vectors: usize = results.iter().map(|(_, replayed)| replayed.vectors).sum();
    let differences: Vec<&str> = results.iter().flat_map(|(_, replayed)| replayed.differences.iter().map(String::as_str)).collect();
    println!("replayed {vectors} vectors of {programs} programs at three levels in {:.1} s", elapsed.as_secs_f64());
    if !seeded.is_empty() {
        println!("recorded and wrote the vectors of {}: commit them under testdata/vectors/fn", seeded.join(", "));
    }

    let file = root().join(KNOWN);
    // Vectors recorded again are other vectors: what they leave undecided is written anew.
    match known(&file).filter(|_| seeded.is_empty()) {
        None => {
            let mut text = String::from(KNOWN_HEADER);
            for line in &differences {
                text.push_str(line);
                text.push('\n');
            }
            std::fs::write(&file, text).unwrap_or_else(|error| panic!("writing {}: {error}", file.display()));
            println!("wrote the {} known differences to {KNOWN}: commit it\n{}", differences.len(), differences.join("\n"));
        }
        Some(known) => {
            let new: Vec<&str> = differences.iter().copied().filter(|line| !known.contains_key(key(line))).collect();
            let keys: HashSet<&str> = differences.iter().copied().map(key).collect();
            let replays: Vec<&str> =
                known.iter().filter(|(held, _)| !keys.contains(held.as_str())).map(|(_, line)| line.as_str()).collect();
            println!("{} functions and levels differ as {KNOWN} holds", differences.len() - new.len());
            assert!(new.is_empty(), "{} functions and levels differ that {KNOWN} does not hold:\n{}", new.len(), new.join("\n"));
            assert!(
                replays.is_empty(),
                "{} functions and levels of {KNOWN} replay now: take them out of it\n{}",
                replays.len(),
                replays.join("\n")
            );
        }
    }
    assert!(programs >= PROGRAMS_AT_LEAST, "the vectors of {programs} programs replayed, not {PROGRAMS_AT_LEAST}");
    assert!(vectors >= VECTORS_AT_LEAST, "{vectors} vectors replayed, not {VECTORS_AT_LEAST}");
    if seeded.is_empty() {
        assert!(elapsed < REPLAY_WITHIN, "the replay took {:.1} s, past {} s", elapsed.as_secs_f64(), REPLAY_WITHIN.as_secs());
    }
}

/// The functions and levels `file` holds as differing, each line by its [`key`]; none when there
/// is no such file.
fn known(file: &Path) -> Option<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(file).ok()?;
    let lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    Some(lines.map(|line| (key(line).to_owned(), line.to_owned())).collect())
}

/// What a line of a difference is of: the program, the function and the level, its first three
/// words. The difference itself may read otherwise once the raise changes, and is still known.
fn key(line: &str) -> &str {
    line.match_indices(' ').nth(2).map_or(line, |(at, _)| &line[..at])
}

/// The committed vectors of `name`, or those of one run recorded now and written where they are
/// committed, with whether they were recorded; none when the program keeps none on this host. A
/// committed file the recorder kept otherwise than it keeps now is recorded again.
fn vectors_of(name: &str) -> Option<(Vec<Vector>, bool)> {
    let file = root().join("testdata/vectors/fn").join(format!("{name}.fv"));
    if file.exists() {
        let text = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("reading {}: {error}", file.display()));
        let keeping = ra_asm_verify::keeping();
        if text.lines().any(|line| line == keeping) {
            let vectors = ra_asm_verify::read(&text).unwrap_or_else(|error| panic!("reading the vectors of {}: {error}", file.display()));
            return Some((vectors, false));
        }
    }
    let image = image(name);
    let machine = lifted(&image, RaiseLevel::Machine);
    let (recording, outcome) =
        ra_asm_verify::record(&image, &machine, &ra_asm_lift::run::Options::default()).unwrap_or_else(|error| panic!("{name}: {error:#}"));
    if let Some(stop) = &outcome.stop {
        if stop.starts_with("no binding for ") && !cfg!(windows) {
            eprintln!("SKIPPED: {name}: {stop}; this host has no Windows DLLs to record a run with");
            return None;
        }
        eprintln!("{name}: the run stopped: {stop}; the vectors it kept are written");
    }
    if recording.vectors.is_empty() {
        eprintln!("SKIPPED: {name}: the run kept no vector");
        return None;
    }
    std::fs::create_dir_all(file.parent().expect("the file is in a directory")).expect("the directory of the vectors is made");
    let text = ra_asm_verify::record::file(&recording.vectors).unwrap_or_else(|error| panic!("{name}: writing its vectors: {error}"));
    std::fs::write(&file, text).unwrap_or_else(|error| panic!("writing {}: {error}", file.display()));
    Some((recording.vectors, true))
}

/// Replays the vectors of `name` at the machine, the promoted and the ABI level, the vectors of
/// each function in the order the run kept them, up to each level's first difference.
fn replay(name: &str, vectors: &[Vector]) -> Replayed {
    let image = image(name);
    let mut differences = Vec::new();
    for level in [RaiseLevel::Machine, RaiseLevel::Promoted] {
        let lifted = lifted(&image, level);
        let replayer = Replayer::new(&image, &lifted, level).unwrap_or_else(|error| panic!("{name}: {error:#}"));
        differences.extend(first_differences(name, level, &replayer, vectors));
    }
    let (abi, signatures) = signed(&image);
    let replayer = Replayer::with_signatures(&image, &abi, &signatures).unwrap_or_else(|error| panic!("{name}: {error:#}"));
    differences.extend(first_differences(name, RaiseLevel::Abi, &replayer, vectors));
    Replayed { vectors: vectors.len(), differences }
}

/// The line of each function whose vectors differ at `level`: the first of them that does.
fn first_differences(name: &str, level: RaiseLevel, replayer: &Replayer<'_>, vectors: &[Vector]) -> Vec<String> {
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut lines: Vec<(u64, String)> = Vec::new();
    for vector in vectors {
        let n = seen.entry(vector.address).or_default();
        *n += 1;
        if lines.iter().any(|(address, _)| *address == vector.address) {
            continue;
        }
        let what = match replayer.replay(vector) {
            Ok(differences) => differences.first().map(ToString::to_string),
            Err(error) => Some(format!("does not load: {error:#}")),
        };
        if let Some(what) = what {
            lines.push((vector.address, format!("{name} {} {} vector {}: {what}", vector.function, level.name(), *n)));
        }
    }
    lines.into_iter().map(|(_, line)| line).collect()
}

fn image(name: &str) -> Image {
    let path = root().join("testdata/bin").join(format!("{name}.exe"));
    ra_asm_pe::load(&path).unwrap_or_else(|error| panic!("{}: {error:#}", path.display()))
}

/// `image` lifted as it is mapped and raised to `level`, under the names the run knows.
fn lifted(image: &Image, level: RaiseLevel) -> Lifted {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: false, x87: X87Unit::Double });
    let options = lifted.raise_options(level);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    lifted
}

/// `image` lifted standing alone and raised to the ABI level, with the signatures of its
/// functions under the names the run knows.
fn signed(image: &Image) -> (Lifted, HashMap<String, Signature>) {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    (lifted, signatures)
}
