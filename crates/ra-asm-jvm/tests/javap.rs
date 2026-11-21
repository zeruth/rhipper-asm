//! The reader and the decoder against `javap`: the fixture class compiled for each release
//! (`testdata/java/<release>/Opcodes.class`) decodes to the instructions `javap -c -p` lists for
//! it (`Opcodes.txt` beside it), method by method, offset by offset.

use std::path::PathBuf;

use ra_asm_jvm::{ClassFile, decode, version};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java")
}

/// A line with its runs of white space made one space each, and no comment.
fn plain(line: &str) -> String {
    let line = line.find(" //").map_or(line, |at| &line[..at]);
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The lines of a listing that are instructions: `<offset>: <name> ...`, and the lines of a
/// switch (a key and its target each, the default, the closing brace).
fn listed(listing: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_switch = false;
    for line in listing.lines().map(plain) {
        let first = line.split(' ').next().unwrap_or_default();
        let numbered =
            first.strip_suffix(':').is_some_and(|n| !n.is_empty() && n.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit()));
        if numbered || (in_switch && (line == "}" || first == "default:")) {
            in_switch = if line == "}" { false } else { in_switch || line.ends_with('{') };
            out.push(line);
        }
    }
    out
}

/// The releases with a fixture, each with its directory.
fn releases() -> Vec<(u16, PathBuf)> {
    let mut found: Vec<(u16, PathBuf)> = std::fs::read_dir(fixtures())
        .expect("testdata/java")
        .filter_map(Result::ok)
        .filter_map(|entry| Some((entry.file_name().to_str()?.parse().ok()?, entry.path())))
        .collect();
    found.sort();
    found
}

#[test]
fn every_release_has_its_fixture() {
    let found: Vec<u16> = releases().into_iter().map(|(release, _)| release).collect();
    assert_eq!(found, (8..=25).collect::<Vec<u16>>(), "the releases javac targets");
}

#[test]
fn a_class_is_of_the_release_it_was_compiled_for() {
    for (release, dir) in releases() {
        let file =
            ClassFile::parse(&std::fs::read(dir.join("Opcodes.class")).unwrap()).unwrap_or_else(|e| panic!("release {release}: {e}"));
        assert_eq!(version::release(file.major), Some(release));
        assert_eq!(file.name(), Some("Opcodes"));
        assert_eq!(file.class_name(file.super_class), Some("java/lang/Object"));
        let names: Vec<&str> = file.methods.iter().filter_map(|m| file.utf8(m.name)).collect();
        for expected in
            ["<init>", "arithmetic", "wide", "real", "convert", "dense", "sparse", "words", "arrays", "guarded", "calls", "fields", "main"]
        {
            assert!(names.contains(&expected), "release {release} has no method {expected}: {names:?}");
        }
        assert_eq!(file.fields.len(), 3);
    }
}

#[test]
fn the_decoder_agrees_with_javap() {
    for (release, dir) in releases() {
        let file = ClassFile::parse(&std::fs::read(dir.join("Opcodes.class")).unwrap()).unwrap();
        let mut decoded = Vec::new();
        for method in &file.methods {
            let Some(code) = file.code(method).unwrap() else { continue };
            let instructions = decode(&code.code, file.major).unwrap_or_else(|e| panic!("release {release}: {e}"));
            assert_eq!(instructions.iter().map(|i| i.length as usize).sum::<usize>(), code.code.len());
            decoded.extend(instructions.iter().flat_map(|i| i.to_string().lines().map(str::to_owned).collect::<Vec<_>>()));
        }
        let expected = listed(&std::fs::read_to_string(dir.join("Opcodes.txt")).unwrap());
        assert!(expected.len() > 400, "release {release}: javap lists {} instructions", expected.len());
        for (n, (ours, theirs)) in decoded.iter().zip(&expected).enumerate() {
            assert_eq!(ours, theirs, "release {release}, line {n} of the instructions");
        }
        assert_eq!(decoded.len(), expected.len(), "release {release}");
    }
}

#[test]
fn the_fixture_uses_most_of_the_machine() {
    let (_, dir) = releases().pop().expect("a fixture");
    let file = ClassFile::parse(&std::fs::read(dir.join("Opcodes.class")).unwrap()).unwrap();
    let mut used = std::collections::BTreeSet::new();
    for method in &file.methods {
        if let Some(code) = file.code(method).unwrap() {
            used.extend(decode(&code.code, file.major).unwrap().into_iter().map(|i| i.name));
        }
    }
    assert!(used.len() >= 135, "the fixture uses {} instructions: {used:?}", used.len());
    for name in
        ["tableswitch", "lookupswitch", "invokedynamic", "invokeinterface", "multianewarray", "monitorenter", "athrow", "iinc_w", "ldc2_w"]
    {
        assert!(used.contains(name), "the fixture has no {name}");
    }
}
