//! The manifest of testdata holds: every listed file is there with the hash the manifest gives,
//! its expected output is there, and the freestanding fixtures of the foreign machines are listed
//! with the evidence the card that made them claims.

use std::collections::HashSet;

use ra_asm_testing::manifest::{self, Entry, Evidence};

fn entries() -> Vec<Entry> {
    let path = manifest::path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    manifest::parse(&text).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn every_listed_file_has_its_hash() {
    let root = manifest::workspace();
    let mut seen = HashSet::new();
    let mut wrong = Vec::new();
    for entry in entries() {
        assert!(seen.insert(entry.path.clone()), "{} is listed twice", entry.path);
        let file = root.join(&entry.path);
        let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("{} is listed but not there: {e}", entry.path));
        let actual = manifest::sha256(&bytes);
        if actual != entry.sha256 {
            wrong.push(format!("{}: the manifest says {} and the file hashes to {actual}", entry.path, entry.sha256));
        }
        if let Some(expected) = &entry.expected {
            assert!(root.join(expected).is_file(), "{}: its expected output {expected} is not there", entry.path);
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_mipsel_fixtures_ran_in_the_emulator() {
    let entries = entries();
    for (program, expected) in [("hello", "testdata/expected/hello.txt"), ("control", "testdata/expected/control.txt")] {
        let path = format!("testdata/bin/{program}_mipsel");
        let entry = entries.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("{path} is not in the manifest"));
        assert_eq!(entry.evidence, Evidence::Emulator, "{path}");
        assert_eq!(entry.expected.as_deref(), Some(expected), "{path}");
        assert_eq!(entry.source.as_deref(), Some(format!("testdata/src/{program}.c").as_str()), "{path}");
        assert!(entry.command.is_some() && entry.tools.is_some(), "{path}: no record of how it was made");
    }
}

#[test]
fn the_psx_fixtures_rest_on_their_source() {
    let entries = entries();
    for program in ["hello", "gte"] {
        let path = format!("testdata/bin/{program}_psx.exe");
        let entry = entries.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("{path} is not in the manifest"));
        assert_eq!(entry.evidence, Evidence::Source, "{path}");
        assert_eq!(entry.expected.as_deref(), Some(format!("testdata/expected/{program}_psx.txt").as_str()), "{path}");
        assert_eq!(entry.source.as_deref(), Some(format!("testdata/src/psx/{program}.c").as_str()), "{path}");
        assert!(entry.command.is_some() && entry.tools.is_some(), "{path}: no record of how it was made");
    }
}
