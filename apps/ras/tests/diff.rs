//! `ras diff` over the `-Od` and `-O2` builds of the core program: matched by structure alone,
//! every function the two link maps name alike is matched to its namesake.

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
