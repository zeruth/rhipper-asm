//! `ras diff` over the `-Od` and `-O2` builds of the core program: matched by structure alone,
//! every function the two link maps name alike is matched to its namesake. `ras diff --sessions`
//! over two sessions: equal, or the first event that differs with its caller on each side.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ras(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// The symbols one side of the diff names exactly one function by, with the function's address.
fn symbols<'a>(functions: impl Iterator<Item = (&'a Value, &'a Value)>) -> BTreeMap<String, String> {
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (symbol, address) in functions {
        if let (Some(symbol), Some(address)) = (symbol.as_str(), address.as_str()) {
            seen.entry(symbol.to_owned()).or_default().push(address.to_owned());
        }
    }
    seen.into_iter().filter(|(_, a)| a.len() == 1).map(|(s, mut a)| (s, a.remove(0))).collect()
}

#[test]
fn the_debug_and_optimized_builds_of_core_match_by_structure() {
    let (od, o2) = (root().join("testdata/rhtest/core-Od.exe"), root().join("testdata/rhtest/core-O2.exe"));
    let text = ras(&["--json", "diff", od.to_str().unwrap(), o2.to_str().unwrap(), "--structure"]);
    let json: Value = serde_json::from_str(&text).unwrap();
    let pairs = json["pairs"].as_array().unwrap();
    let (removed, added) = (json["removed"].as_array().unwrap(), json["added"].as_array().unwrap());
    let old = symbols(pairs.iter().map(|p| (&p["old_symbol"], &p["old"])).chain(removed.iter().map(|f| (&f["symbol"], &f["address"]))));
    let new = symbols(pairs.iter().map(|p| (&p["new_symbol"], &p["new"])).chain(added.iter().map(|f| (&f["symbol"], &f["address"]))));
    let matched: BTreeSet<(&str, &str)> = pairs.iter().filter_map(|p| Some((p["old"].as_str()?, p["new"].as_str()?))).collect();
    let agreed: Vec<(&String, &String, &String)> = old.iter().filter_map(|(s, a)| new.get(s).map(|b| (s, a, b))).collect();
    assert!(agreed.len() > 40, "the maps agree on {} functions only", agreed.len());
    let wrong: Vec<String> = agreed
        .iter()
        .filter(|(_, a, b)| !matched.contains(&(a.as_str(), b.as_str())))
        .map(|(s, a, b)| {
            let to = pairs
                .iter()
                .find(|p| p["old"] == a.as_str())
                .map_or("nothing".to_owned(), |p| format!("{} by {}", p["new_name"], p["how"]));
            format!("{s} ({a} -> {b}) matched {to}")
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} functions the maps agree on are not matched to their namesake:\n{}",
        wrong.len(),
        agreed.len(),
        wrong.join("\n")
    );
    assert!(pairs.iter().all(|p| p.get("ir").is_none()), "no IR without --ir");
}

/// A port's session, with `answer` the answer of the `GetMessageA` its window procedure makes.
fn port_session(answer: &str) -> String {
    [
        "ra-port-session 1",
        "call GetModuleHandleA 0x0 = i+0x0",
        "back i+0x1180 0x30c1a 0x81 0x0 s",
        &format!("call GetMessageA s 0x0 0x0 0x0 = {answer}"),
        "  write 0+0x0 1ac10300",
        "ret 0x1",
        "call CreateWindowExA 0x0 i+0x2040 i+0x204e 0x80000000 0x0 0x0 0xc8 0x64 0x0 0x0 i+0x0 0x0 = 0x30c1a",
        "call ExitProcess 0x0 ! exit",
        "",
    ]
    .join("\n")
}

fn session_file(name: &str, text: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("diff-sessions");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn sessions_alike_are_equal_with_their_count_of_events() {
    let hello = root().join("testdata/sessions/hello64.session");
    let text = ras(&["diff", "--sessions", hello.to_str().unwrap(), hello.to_str().unwrap()]);
    assert_eq!(text, "equal, 3 events\n");
    let (a, b) = (
        session_file("alike-a.session", &port_session("0x1")),
        session_file("alike-b.session", &port_session("0x1").replace('\n', "\r\n")),
    );
    let text = ras(&["diff", "--sessions", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(text, "equal, 6 events\n", "a CRLF session reads as its LF one");
}

#[test]
fn sessions_that_differ_name_the_first_event_and_its_caller_on_each_side() {
    let (a, b) = (session_file("differ-a.session", &port_session("0x1")), session_file("differ-b.session", &port_session("0x0")));
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    let text = ras(&["diff", "--sessions", a, b]);
    assert!(text.starts_with("event 3 differs\n"), "{text}");
    assert!(text.contains(&format!("  {a}, line 4, in the window procedure i+0x1180: call GetMessageA s 0x0 0x0 0x0 = 0x1\n")), "{text}");
    assert!(text.contains(&format!("  {b}, line 4, in the window procedure i+0x1180: call GetMessageA s 0x0 0x0 0x0 = 0x0\n")), "{text}");
    let json: Value = serde_json::from_str(&ras(&["--json", "diff", "--sessions", a, b])).unwrap();
    assert_eq!(json["equal"], false);
    assert_eq!(json["event"], 3);
    assert_eq!(json["a"]["writes"][0], "0+0x0 1ac10300");

    let short = port_session("0x1").replace("call ExitProcess 0x0 ! exit\n", "");
    let short = session_file("differ-short.session", &short);
    let text = ras(&["diff", "--sessions", a, short.to_str().unwrap()]);
    assert!(text.starts_with("event 6 differs\n"), "{text}");
    assert!(text.contains(&format!("  {a}, line 8, in the program: call ExitProcess 0x0 ! exit\n")), "{text}");
    assert!(text.contains(&format!("  {}: past its last event, of 5\n", short.display())), "{text}");
}
