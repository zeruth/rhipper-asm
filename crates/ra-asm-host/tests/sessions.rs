//! The sessions of the Windows fixtures: each program under `testdata/bin` that the manifest lists
//! with a native expected output, lifted, run in the interpreter at the machine level with its host
//! calls kept, is recorded at `testdata/sessions/<name>.session`, and every committed session reads
//! back equal to itself. A fixture without a session is recorded here when its run prints what the
//! native run printed; one whose run does not is named and left without, since its calls are not
//! the program's. With `RA_RECORD=write` every fixture is recorded again the same way, its
//! committed session replaced only by a run that prints the native output.
//!
//! And the replays: each fixture's run at the ABI level, recorded the same way at
//! `testdata/sessions/abi/<name>.session`, runs again with its session standing in for the host,
//! makes no call into the host and prints what the native run printed; and a session with one
//! argument changed stops its replay at that event, by its number.
//!
//! And the person's program (`RA_GAME`, never committed): recorded once at the ABI level into the
//! session `RA_GAME_SESSION` names, up to the host call the person chooses, and replayed against
//! every build after, each event compared with the one the session holds.

use std::collections::HashMap;
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use ra_asm_host::Outcome;
use ra_asm_host::replay;
use ra_asm_host::session::{self, Answer, Event, Session, Value};
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_testing::manifest::{self, Evidence};

/// The sessions that must read back for the test to pass, and the programs that must replay.
const AT_LEAST: usize = 20;
/// Where the sessions of the runs at the ABI level are, under `testdata/sessions`: the replays run
/// there, since the machine level's calls take the registers the ABI level's do not have.
const ABI: &str = "abi";
/// The program whose session the replay of a changed argument changes.
const CHANGED: &str = "hello64";
/// IR instructions a recording run may take: a fixture is a short program.
const LIMIT: u64 = 200_000_000;

/// Each Windows fixture with a native expected output, with that output's path.
fn fixtures(root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let path = manifest::path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let entries = manifest::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    entries
        .iter()
        .filter(|entry| entry.evidence == Evidence::Native && entry.path.starts_with("testdata/bin/") && entry.path.ends_with(".exe"))
        .filter_map(|entry| Some((root.join(&entry.path), root.join(entry.expected.as_ref()?))))
        .collect()
}

/// `image` lifted and raised to `level`, standalone at the ABI level as a port is.
fn lift(image: &Image, level: RaiseLevel) -> Result<(Lifted, HashMap<String, Signature>), String> {
    let raised = catch_unwind(AssertUnwindSafe(|| {
        let analysis = ra_asm_analysis::analyze(image);
        let options = ra_asm_lift::LiftOptions { standalone: level == RaiseLevel::Abi, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
        let options = lifted.raise_options(level);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        (lifted, signatures)
    }));
    raised.map_err(|_| "the lift or the raise panicked".to_owned())
}

/// Why the run printed other than `expected` or stopped, if it did.
fn differs(outcome: &Outcome, expected: &str) -> Option<String> {
    if let Some(stop) = &outcome.stop {
        return Some(format!("the run stopped: {stop}"));
    }
    let output = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    (output.trim_end() != expected.replace("\r\n", "\n").trim_end()).then(|| "the run printed other than the native run".to_owned())
}

/// The session of the run of `exe` at `level`, when the run prints `expected`; why not otherwise.
fn record(exe: &Path, expected: &str, level: RaiseLevel) -> Result<Session, String> {
    let image = ra_asm_pe::load(exe).map_err(|e| format!("the image does not load: {e:#}"))?;
    let (lifted, signatures) = lift(&image, level)?;
    let options = ra_asm_host::Options { limit: LIMIT, ..ra_asm_host::Options::default() };
    let (session, outcome) =
        session::record(&image, &lifted, &signatures, &options).map_err(|e| format!("the interpreter failed: {e:#}"))?;
    match differs(&outcome, expected) {
        Some(why) => Err(why),
        None => Ok(session),
    }
}

/// Whether the committed sessions are recorded again rather than read (`RA_RECORD=write`), as a
/// change to what the lift calls makes them stale: on Windows, the one host that records.
fn rewrite() -> bool {
    cfg!(windows) && std::env::var("RA_RECORD").is_ok_and(|value| value == "write")
}

fn stem(exe: &Path) -> String {
    exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n")
}

/// The ABI-level session of `exe`: the committed one, or on Windows the one its run records when
/// the run prints `expected`, written to `path` when `keep` says so; why there is none otherwise.
/// A kept session is recorded again under [`rewrite`], and a run that does not print `expected`
/// leaves the committed one as it is.
fn abi_session(exe: &Path, expected: &str, path: &Path, keep: bool) -> Result<Session, String> {
    if path.exists() && !(keep && rewrite()) {
        return Session::parse(&read(path)).map_err(|e| format!("{}: {e}", path.display()));
    }
    if !cfg!(windows) {
        return Err("no session is committed, and this host has no Windows DLLs to record one".to_owned());
    }
    let session = match record(exe, expected, RaiseLevel::Abi) {
        Ok(session) => session,
        // Recorded again but not the native run: the committed session still replays, so a
        // stale one shows as the replay's difference rather than as a fixture left out.
        Err(why) if path.exists() => {
            eprintln!("not recorded again: {}: {why}; the committed session replays", path.display());
            return Session::parse(&read(path)).map_err(|e| format!("{}: {e}", path.display()));
        }
        Err(why) => return Err(why),
    };
    if keep {
        // Whole or not at all: the other test may read the session as it lands.
        let new = path.with_extension("session.new");
        std::fs::write(&new, session.write()).unwrap_or_else(|e| panic!("{}: {e}", new.display()));
        std::fs::rename(&new, path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
    Ok(session)
}

/// The replay of `session` over `exe` at the ABI level.
fn play_back(exe: &Path, session: &Session) -> Result<(Outcome, replay::Report), String> {
    let image = ra_asm_pe::load(exe).map_err(|e| format!("the image does not load: {e:#}"))?;
    let (lifted, signatures) = lift(&image, RaiseLevel::Abi)?;
    let options = ra_asm_host::Options { limit: LIMIT, ..ra_asm_host::Options::default() };
    replay::run(&image, &lifted, &signatures, session, &options).map_err(|e| format!("the interpreter failed: {e:#}"))
}

#[test]
fn the_fixtures_sessions_read_back_in_their_format() {
    let root = manifest::workspace();
    let dir = root.join("testdata/sessions");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut unrecorded = Vec::new();
    for (exe, expected) in fixtures(&root) {
        let name = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let path = dir.join(format!("{name}.session"));
        if path.exists() && !rewrite() {
            continue;
        }
        let expected = std::fs::read_to_string(&expected).unwrap_or_else(|e| panic!("{}: {e}", expected.display()));
        match record(&exe, &expected, RaiseLevel::Machine) {
            Ok(session) => std::fs::write(&path, session.write()).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            Err(why) => unrecorded.push(format!("{name}: {why}")),
        }
    }
    for line in &unrecorded {
        eprintln!("not recorded: {line}");
    }
    let mut sessions: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|item| item.path())
        .filter(|path| path.extension().is_some_and(|e| e == "session"))
        .collect();
    sessions.sort();
    for path in &sessions {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n");
        let session = Session::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!session.events.is_empty(), "{} holds no host call", path.display());
        let written = session.write();
        assert!(written == text, "{} does not write back as it reads", path.display());
        assert_eq!(Session::parse(&written).as_ref(), Ok(&session), "{} does not read back equal", path.display());
    }
    assert!(
        sessions.len() >= AT_LEAST,
        "{} sessions read back, fewer than {AT_LEAST}; not recorded:\n{}",
        sessions.len(),
        unrecorded.join("\n")
    );
    println!("{} sessions read back equal", sessions.len());
}

#[test]
fn the_fixtures_replay_at_the_abi_level_with_no_host() {
    let root = manifest::workspace();
    let dir = root.join("testdata/sessions").join(ABI);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut replayed = Vec::new();
    let mut not = Vec::new();
    for (exe, expected) in fixtures(&root) {
        let name = stem(&exe);
        let expected = read(&expected);
        let session = match abi_session(&exe, &expected, &dir.join(format!("{name}.session")), true) {
            Ok(session) => session,
            Err(why) => {
                not.push(format!("{name}: {why}"));
                continue;
            }
        };
        let (outcome, report) = match play_back(&exe, &session) {
            Ok(replay) => replay,
            Err(why) => {
                not.push(format!("{name}: {why}"));
                continue;
            }
        };
        assert_eq!(outcome.hosted, 0, "{name}: the replay called into the host");
        if let Some(difference) = &report.difference {
            panic!("{name}: the replay of its committed session differs at {difference}");
        }
        assert!(report.whole(), "{name}: the replay made {} of the session's {} events", report.replayed, report.events);
        if let Some(why) = differs(&outcome, &expected) {
            panic!("{name}: the replay ran to its end, but {why}");
        }
        replayed.push(name);
    }
    for line in &not {
        eprintln!("not replayed: {line}");
    }
    assert!(replayed.len() >= AT_LEAST, "{} programs replayed, fewer than {AT_LEAST}; not replayed:\n{}", replayed.len(), not.join("\n"));
    println!("{} programs replayed at the ABI level with no host call: {}", replayed.len(), replayed.join(" "));
}

#[test]
fn a_replay_names_the_event_where_an_argument_changed() {
    let root = manifest::workspace();
    let (exe, expected) = fixtures(&root)
        .into_iter()
        .find(|(exe, _)| stem(exe) == CHANGED)
        .unwrap_or_else(|| panic!("the manifest lists no {CHANGED}.exe with a native output"));
    let path = root.join("testdata/sessions").join(ABI).join(format!("{CHANGED}.session"));
    let mut session = abi_session(&exe, &read(&expected), &path, false).unwrap_or_else(|why| panic!("{CHANGED}: {why}"));
    // The last call with an integer argument, so the events before it replay as they are.
    let index = session
        .events
        .iter()
        .rposition(|event| matches!(event.args.first(), Some(Value::Int(_))) && !matches!(event.answer, Answer::Limit(_)))
        .unwrap_or_else(|| panic!("{CHANGED}: no call of its session has an integer argument"));
    let Some(Value::Int(n)) = session.events[index].args.first_mut() else { unreachable!("the event was found by its argument") };
    *n ^= 1;
    let name = session.events[index].name.clone();
    let (outcome, report) = play_back(&exe, &session).unwrap_or_else(|why| panic!("{CHANGED}: {why}"));
    assert_eq!(outcome.hosted, 0, "{CHANGED}: the replay called into the host");
    let difference = report.difference.unwrap_or_else(|| panic!("{CHANGED}: the replay took the changed argument of {name}"));
    assert_eq!(difference.event, index + 1, "{CHANGED}: the event named: {difference}");
    assert_eq!(difference.name, name, "{CHANGED}: the call named: {difference}");
    assert_eq!(report.replayed, index, "{CHANGED}: the events before the changed one replay");
    let stop = outcome.stop.unwrap_or_default();
    assert!(stop.contains(&format!("event {}:", index + 1)), "{CHANGED}: the run stops naming the event: {stop}");
    println!("{CHANGED}: {difference}");
}

/// The stack of the thread the program's lift, raise and run take: a large program's analysis
/// and raise nest deeper than a test thread's default stack.
const STACK: usize = 256 << 20;

/// The person's program at the ABI level, with the residual engine behind the run: lifted from the
/// facts of its last run when they are of this image, run with its host calls kept, and the run's
/// facts written back for the next analysis.
struct Program {
    image: ra_asm_pe::Image,
    facts: PathBuf,
}

impl Program {
    fn load(exe: &Path, session: &Path) -> Result<Self, String> {
        let image = ra_asm_pe::load(exe).map_err(|e| format!("{}: the image does not load: {e:#}", exe.display()))?;
        Ok(Self { image, facts: session.with_extension("facts.json") })
    }

    /// The facts of the last run, when they are this image's.
    fn last_facts(&self) -> Option<ra_asm_analysis::Facts> {
        let text = std::fs::read_to_string(&self.facts).ok()?;
        let facts: ra_asm_analysis::Facts =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: the facts do not read: {e}", self.facts.display()));
        facts.input.is_of(&self.image).then_some(facts)
    }

    /// The session of the program's run, and what stopped the run when something did.
    fn run(&self) -> Result<(Session, Option<String>), String> {
        let facts = self.last_facts();
        let analysis = match &facts {
            Some(facts) => ra_asm_analysis::analyze_with(&self.image, facts),
            None => ra_asm_analysis::analyze(&self.image),
        };
        let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&self.image, &analysis, options);
        let options = lifted.raise_options(RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let options = ra_asm_host::Options { session: true, ..ra_asm_host::Options::default() };
        let outcome =
            ra_asm_host::run(&self.image, &lifted, &signatures, &options).map_err(|e| format!("the interpreter failed: {e:#}"))?;
        let json = serde_json::to_string_pretty(&outcome.facts).map_err(|e| format!("the facts do not write: {e}"))?;
        std::fs::write(&self.facts, json).map_err(|e| format!("{}: {e}", self.facts.display()))?;
        Ok((Session::of(&outcome.boundary), outcome.stop))
    }
}

/// An event's call line, without its writes.
fn call_line(event: &Event) -> String {
    let alone = Session { events: vec![Event { writes: Vec::new(), ..event.clone() }] };
    alone.write().lines().nth(1).unwrap_or_default().to_owned()
}

/// The count of the `recorded` events when the `replayed` run made each of them again in its order;
/// the first that differs, with the host function it called, otherwise.
fn compare(recorded: &[Event], replayed: &[Event], stop: Option<&str>) -> Result<usize, String> {
    for (index, event) in recorded.iter().enumerate() {
        let number = index + 1;
        let Some(again) = replayed.get(index) else {
            return Err(format!(
                "the replay ended after {index} of {} events ({}); event {number}, in {}, was `{}`",
                recorded.len(),
                stop.unwrap_or("the program exited"),
                event.name,
                call_line(event)
            ));
        };
        if again == event {
            continue;
        }
        let (was, is) = (call_line(event), call_line(again));
        if was != is {
            return Err(format!("event {number} differs, in {}:\n  recorded `{was}`\n  replayed `{is}`", event.name));
        }
        let first = event.writes.iter().zip(&again.writes).find(|(a, b)| a != b).map(|(a, _)| a.0);
        let first = first.or_else(|| event.writes.get(again.writes.len()).or_else(|| again.writes.get(event.writes.len())).map(|w| w.0));
        let at = first.map_or_else(String::new, |address| format!(", the first that differs at {address:#x}"));
        return Err(format!(
            "event {number} differs, in {}: `{was}` wrote other bytes ({} runs recorded, {} replayed{at})",
            event.name,
            event.writes.len(),
            again.writes.len()
        ));
    }
    Ok(recorded.len())
}

/// The program's session at `session`: recorded there when the file is absent, its first
/// `RA_GAME_CALLS` host calls (all of them without it) the point the person chose; replayed
/// against the current lift otherwise, which prints `equal, N events` or the first difference.
fn the_program_session(exe: &Path, session: &Path, calls: Option<usize>) -> Result<String, String> {
    let program = Program::load(exe, session)?;
    if !session.exists() {
        let (mut recorded, stop) = program.run()?;
        if let Some(calls) = calls {
            recorded.events.truncate(calls);
        }
        if recorded.events.is_empty() {
            return Err(format!("the run made no host call ({})", stop.as_deref().unwrap_or("the program exited")));
        }
        std::fs::write(session, recorded.write()).map_err(|e| format!("{}: {e}", session.display()))?;
        let ended = stop.map_or_else(String::new, |stop| format!("; the run stopped: {stop}"));
        return Ok(format!("recorded {} events to {}{ended}; run again to replay them", recorded.events.len(), session.display()));
    }
    let text = std::fs::read_to_string(session).map_err(|e| format!("{}: {e}", session.display()))?.replace("\r\n", "\n");
    let recorded = Session::parse(&text).map_err(|e| format!("{}: {e}", session.display()))?;
    let (replayed, stop) = program.run()?;
    let n = compare(&recorded.events, &replayed.events, stop.as_deref())?;
    Ok(format!("equal, {n} events"))
}

/// The person's program (`RA_GAME`, never committed) against its session (`RA_GAME_SESSION`).
#[test]
#[ignore = "needs RA_GAME and RA_GAME_SESSION; run with `cargo test -q -p ra-asm-host --test sessions -- --ignored the_program`"]
fn the_program() {
    let exe = PathBuf::from(std::env::var_os("RA_GAME").expect("RA_GAME is not set: it names the program's path"));
    let session = PathBuf::from(std::env::var_os("RA_GAME_SESSION").expect("RA_GAME_SESSION is not set: it names the session's file"));
    assert!(exe.is_file(), "RA_GAME names no file: {}", exe.display());
    let calls =
        std::env::var("RA_GAME_CALLS").ok().map(|n| n.parse::<usize>().unwrap_or_else(|_| panic!("RA_GAME_CALLS `{n}` is no count")));
    let run = std::thread::Builder::new().stack_size(STACK).spawn(move || the_program_session(&exe, &session, calls));
    let line = run.expect("a thread for the program").join().expect("the program's lift or run panicked");
    let mut out = std::io::stdout().lock();
    match line {
        Ok(line) => _ = writeln!(out, "{line}"),
        Err(difference) => {
            _ = writeln!(out, "{difference}");
            panic!("the replay differs from the session: {difference}");
        }
    }
}
