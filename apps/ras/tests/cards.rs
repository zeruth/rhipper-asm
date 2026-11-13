//! `ras cards --pending` over pending lists written for the test: the cards a list gives, their
//! fields, and the acceptance of each as a cargo command.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh `testdata` directory of the test `name`, holding the lists of `files`.
fn testdata(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cards").join(name).join("testdata");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn run(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).args(["cards", "--pending"]).arg(dir).output().unwrap()
}

fn cards(dir: &Path) -> String {
    let output = run(dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// A pending list of `count` vector forms, each with its first difference.
fn forms(count: usize) -> String {
    let mut text = String::from("# the forms that differ today, with their first difference\n\n");
    for i in 0..count {
        writeln!(text, "form_{i:02}_r64_m64 rax: 0x{i:x} for 0x{:x}", i + 1).unwrap();
    }
    text
}

/// A list of every kind, with one entry or two.
fn every_kind() -> PathBuf {
    testdata(
        "kinds",
        &[
            ("vectors/x86_64/PENDING.txt", "shld_r64_r64_cl rflags: 0x246 for 0x247\n"),
            ("vectors/i386/PENDING.txt", "aam_imm8 eflags: 0x202 for 0x206\nfprem st0: differs\n"),
            ("vectors/fn/TREE_PENDING.txt", "control.exe F_140001230 returns 3 for 4\n"),
            ("levels/PENDING.txt", "core.exe F_140001000 abi: a read of the state\n"),
            ("csmith/PENDING.txt", "seed07-x64-O2 promoted\n"),
            ("csmith/EMIT_PENDING.txt", "seed11-x86-O0 checksum differs\n"),
        ],
    )
}

/// The cards of an outline, each as its lines: the title and the fields under it.
fn split(outline: &str) -> Vec<Vec<&str>> {
    let mut cards: Vec<Vec<&str>> = Vec::new();
    for line in outline.lines() {
        if line.starts_with("- ") {
            cards.push(vec![line]);
        } else if line.starts_with("  ")
            && let Some(card) = cards.last_mut()
        {
            card.push(line);
        }
    }
    cards
}

#[test]
fn a_pending_list_of_25_forms_gives_3_cards() {
    let dir = testdata("forms", &[("vectors/x86_64/PENDING.txt", forms(25).as_str())]);
    let outline = cards(&dir);
    let cards = split(&outline);
    assert_eq!(cards.len(), 3, "{outline}");
    assert_eq!(outline.lines().filter(|l| l.starts_with("### ")).count(), 1, "the cards of a list are one section\n{outline}");
    assert!(outline.contains("### The vector forms pending in testdata/vectors/x86_64/PENDING.txt"), "{outline}");
    // Ten lines a card at most, every form in exactly one card, in the list's order.
    let held: Vec<usize> = cards.iter().map(|card| card[1].matches("form_").count()).collect();
    assert_eq!(held, [10, 10, 5], "{outline}");
    for i in 0..25 {
        assert_eq!(outline.matches(&format!("form_{i:02}_r64_m64 rax")).count(), 1, "form {i}\n{outline}");
    }
    assert_eq!(cards[0][0], "- form_00_r64_m64 and 9 more vector forms {c2}");
    assert_eq!(cards[2][0], "- form_20_r64_m64 and 4 more vector forms {c2}");
    assert!(!outline.contains("the forms that differ today"), "a comment is not an entry\n{outline}");
}

#[test]
fn each_card_has_the_four_fields() {
    let outline = cards(&every_kind());
    let cards = split(&outline);
    assert_eq!(cards.len(), 6, "{outline}");
    assert_eq!(outline.lines().filter(|l| l.starts_with("### ")).count(), 6, "{outline}");
    for card in &cards {
        assert_eq!(card.len(), 4, "{card:#?}");
        assert!(card[0].len() > "-  {c2}".len() && card[0].ends_with(" {c2}"), "{card:#?}");
        assert!(card[1].starts_with("  objective: "), "{card:#?}");
        assert!(card[2].starts_with("  acceptance: "), "{card:#?}");
        let files: Vec<&str> = card[3].strip_prefix("  files: ").unwrap_or_else(|| panic!("{card:#?}")).split(' ').collect();
        assert_eq!(files.len(), 2, "the list and the source of its kind\n{card:#?}");
        assert!(files[0].starts_with("testdata/") && files[0].ends_with("PENDING.txt"), "{card:#?}");
        assert!(files[1].starts_with("crates/") && files[1].ends_with(".rs"), "{card:#?}");
        assert!(card[1].contains(files[0]) && card[2].contains(files[0]), "{card:#?}");
    }
    // The title of a card of one line is the line.
    assert!(cards.iter().any(|card| card[0] == "- shld_r64_r64_cl rflags: 0x246 for 0x247 {c2}"), "{outline}");
    assert!(cards.iter().any(|card| card[0] == "- aam_imm8 and 1 more vector forms {c2}"), "{outline}");
    // The source is the one the kind points at.
    let files: Vec<&str> = cards.iter().map(|card| card[3]).collect();
    for expected in [
        "  files: testdata/vectors/x86_64/PENDING.txt crates/ra-asm-lift/src/x86.rs",
        "  files: testdata/vectors/fn/TREE_PENDING.txt crates/ra-asm-emit/src/structure.rs",
        "  files: testdata/levels/PENDING.txt crates/ra-asm-lift/src/raise/mod.rs",
        "  files: testdata/csmith/EMIT_PENDING.txt crates/ra-asm-emit/src/c.rs",
    ] {
        assert!(files.contains(&expected), "{expected}\n{outline}");
    }
}

#[test]
fn each_acceptance_is_a_cargo_command_in_backticks() {
    let outline = cards(&every_kind());
    let mut commands = Vec::new();
    for card in split(&outline) {
        let acceptance = card[2].strip_prefix("  acceptance: `").unwrap_or_else(|| panic!("{card:#?}"));
        let (command, rest) = acceptance.split_once('`').unwrap_or_else(|| panic!("{card:#?}"));
        assert!(command.starts_with("cargo test -q -p ra-asm-"), "{card:#?}");
        assert!(command.contains(" --test "), "{card:#?}");
        assert!(
            rest.starts_with(" prints `test result: ok` with this line") || rest.starts_with(" prints `test result: ok` with these lines")
        );
        assert!(!rest.contains("`cargo"), "one command\n{card:#?}");
        commands.push(command);
    }
    commands.sort_unstable();
    assert_eq!(
        commands,
        [
            "cargo test -q -p ra-asm-emit --features oracle --test csmith",
            "cargo test -q -p ra-asm-emit --test tree fixtures",
            "cargo test -q -p ra-asm-host --test csmith",
            "cargo test -q -p ra-asm-lift --test levels fixtures",
            "cargo test -q -p ra-asm-lift --test vectors i386",
            "cargo test -q -p ra-asm-lift --test vectors x86_64",
        ]
    );
}

#[test]
fn a_list_of_no_known_kind_stops_with_its_path() {
    let dir = testdata("unknown", &[("census/PENDING.txt", "fxsave 12\n"), ("levels/PENDING.txt", "# nothing pending\n\n")]);
    let output = run(&dir);
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("testdata/census/PENDING.txt is not a pending list"), "{message}");
    // Without it, the empty list alone gives no section and no card.
    std::fs::remove_file(dir.join("census/PENDING.txt")).unwrap();
    assert_eq!(cards(&dir), "");
}
