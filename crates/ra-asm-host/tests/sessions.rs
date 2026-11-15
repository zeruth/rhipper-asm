//! The sessions of the Windows fixtures: each program under `testdata/bin` that the manifest lists
//! with a native expected output, lifted, run in the interpreter at the machine level with its host
//! calls kept, is recorded at `testdata/sessions/<name>.session`, and every committed session reads
//! back equal to itself. A fixture without a session is recorded here when its run prints what the
//! native run printed; one whose run does not is named and left without, since its calls are not
//! the program's.
//!
//! And the person's program (`RA_GAME`, never committed): recorded once at the ABI level into the
//! session `RA_GAME_SESSION` names, up to the host call the person chooses, and replayed against
//! every build after, each event compared with the one the session holds.

use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use ra_asm_host::session::{self, Event, Session};
use ra_asm_lift::RaiseLevel;
use ra_asm_testing::manifest::{self, Evidence};

/// The sessions that must read back for the test to pass.
const AT_LEAST: usize = 20;
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

/// The session of the run of `exe`, when the run prints `expected`; why not otherwise.
fn record(exe: &Path, expected: &str) -> Result<Session, String> {
    let image = ra_asm_pe::load(exe).map_err(|e| format!("the image does not load: {e:#}"))?;
    let raised = catch_unwind(AssertUnwindSafe(|| {
        let analysis = ra_asm_analysis::analyze(&image);
        let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        let options = lifted.raise_options(RaiseLevel::Machine);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        (lifted, signatures)
    }));
    let (lifted, signatures) = raised.map_err(|_| "the lift or the raise panicked".to_owned())?;
    let options = ra_asm_host::Options { limit: LIMIT, ..ra_asm_host::Options::default() };
    let (session, outcome) =
        session::record(&image, &lifted, &signatures, &options).map_err(|e| format!("the interpreter failed: {e:#}"))?;
    if let Some(stop) = &outcome.stop {
        return Err(format!("the run stopped: {stop}"));
    }
    let output = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    if output.trim_end() != expected.replace("\r\n", "\n").trim_end() {
        return Err("the run printed other than the native run".to_owned());
    }
    Ok(session)
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
        if path.exists() {
            continue;
        }
        let expected = std::fs::read_to_string(&expected).unwrap_or_else(|e| panic!("{}: {e}", expected.display()));
        match record(&exe, &expected) {
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
