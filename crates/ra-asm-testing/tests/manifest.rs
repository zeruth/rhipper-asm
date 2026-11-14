//! The manifest of testdata holds: every binary, vector file and expected output of the tree is
//! listed with the hash the manifest gives and the record of how it was made, an unlisted file
//! and a wrong hash are failures, and the freestanding fixtures of the foreign machines are listed
//! with the evidence the card that made them claims.

use ra_asm_testing::Scratch;
use ra_asm_testing::manifest::{self, Entry, Evidence};

fn entries() -> Vec<Entry> {
    let path = manifest::path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    manifest::parse(&text).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_tree_is_what_the_manifest_lists() {
    let entries = entries();
    let failures = manifest::check(&manifest::workspace(), &entries);
    assert!(failures.is_empty(), "{} files are not as testdata/MANIFEST.toml says:\n{}", failures.len(), failures.join("\n"));
    assert!(entries.len() >= 150, "the manifest lists {} files, fewer than the 150 the tree held when it was written", entries.len());
}

#[test]
fn every_entry_says_how_it_was_made() {
    let mut bare = Vec::new();
    for entry in entries() {
        for (key, value) in [("command", &entry.command), ("tools", &entry.tools)] {
            if value.as_deref().is_none_or(|text| text.trim().is_empty()) {
                bare.push(format!("{} has no `{key}`", entry.path));
            }
        }
    }
    assert!(bare.is_empty(), "{}", bare.join("\n"));
}

/// A tree of four files, two of them fixtures: the listed tree passes, with a text file checked
/// out as CRLF under the hash of its LF form, and each way of leaving the manifest is named.
#[test]
fn an_unlisted_file_and_a_wrong_hash_are_failures() {
    let scratch = Scratch::new("ra-asm-testing-manifest", "tree");
    let write = |path: &str, bytes: &[u8]| {
        let file = scratch.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    };
    let entry = |path: &str, bytes: &[u8]| Entry {
        path: path.to_owned(),
        sha256: manifest::sha256(bytes),
        evidence: Evidence::Native,
        expected: None,
        source: None,
        command: None,
        tools: None,
    };
    write("testdata/bin/a.exe", b"MZ\0\r\n");
    write("testdata/expected/a.txt", b"1\r\n2\r\n");
    write("testdata/src/a.c", b"int main(void) { return 0; }\n");
    write("testdata/vectors/x86/PENDING.txt", b"# nothing pending\n");
    let mut entries = vec![entry("testdata/bin/a.exe", b"MZ\0\r\n"), entry("testdata/expected/a.txt", b"1\n2\n")];
    assert_eq!(manifest::fixtures(&scratch), ["testdata/bin/a.exe", "testdata/expected/a.txt"]);
    assert_eq!(manifest::check(&scratch, &entries), Vec::<String>::new());

    write("testdata/bin/a.map", b"a map\n");
    write("testdata/vectors/x86/arith.vec", b"90 -> \n");
    let failures = manifest::check(&scratch, &entries);
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(failures[0].starts_with("testdata/bin/a.map is not in the manifest"), "{failures:#?}");
    assert!(failures[0].contains(&manifest::sha256(b"a map\n")), "{failures:#?}");
    assert!(failures[1].starts_with("testdata/vectors/x86/arith.vec is not in the manifest"), "{failures:#?}");
    entries.push(entry("testdata/bin/a.map", b"a map\n"));
    entries.push(entry("testdata/vectors/x86/arith.vec", b"90 -> \n"));
    assert_eq!(manifest::check(&scratch, &entries), Vec::<String>::new());

    // A binary is its bytes: the CRLF of one is not a line ending to look past.
    write("testdata/bin/a.exe", b"MZ\0\n");
    let failures = manifest::check(&scratch, &entries);
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("testdata/bin/a.exe: the manifest says"), "{failures:#?}");
    assert!(failures[0].ends_with(&manifest::sha256(b"MZ\0\n")), "{failures:#?}");
    write("testdata/bin/a.exe", b"MZ\0\r\n");

    entries[1].expected = Some("testdata/expected/b.txt".to_owned());
    entries.push(entry("testdata/bin/gone.exe", b""));
    entries.push(entry("testdata/bin/a.map", b"a map\n"));
    let failures = manifest::check(&scratch, &entries);
    assert_eq!(failures.len(), 3, "{failures:#?}");
    assert!(failures[0].contains("its expected output testdata/expected/b.txt is not there"), "{failures:#?}");
    assert!(failures[1].starts_with("testdata/bin/gone.exe is listed but not there"), "{failures:#?}");
    assert_eq!(failures[2], "testdata/bin/a.map is listed twice");
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
