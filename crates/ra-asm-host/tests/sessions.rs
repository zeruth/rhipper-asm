//! The sessions of the Windows fixtures: each program under `testdata/bin` that the manifest lists
//! with a native expected output, lifted, run in the interpreter at the machine level with its host
//! calls kept, is recorded at `testdata/sessions/<name>.session`, and every committed session reads
//! back equal to itself. A fixture without a session is recorded here when its run prints what the
//! native run printed; one whose run does not is named and left without, since its calls are not
//! the program's.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use ra_asm_host::session::{self, Session};
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
