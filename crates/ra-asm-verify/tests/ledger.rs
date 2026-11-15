//! The ledger: its file, the merge of runs, and the lines a function loses when its bytes change.

use std::path::PathBuf;

use ra_asm_verify::ledger::{self, Function};
use ra_asm_verify::{Evidence, Ledger, digest, read_ledger, write_ledger};

const BYTES: u64 = 0x9b5f_3c8e_2a1d_4f70;

/// A ledger with every kind of evidence on one function and a second function after it.
fn sample() -> Ledger {
    let mut ledger = Ledger::default();
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "machine", Evidence::Tested(12));
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "promoted", Evidence::Traced);
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "c", Evidence::None);
    ledger.note(0x1_4000_2000, "main", 0x1, "abi", Evidence::Tested(1));
    ledger
}

const SAMPLE: &str = "\
ra-asm-verify ledger 1

function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
  c none
  machine tested 12
  promoted traced

function main at 0x140002000 bytes 0x1
  abi tested 1
";

#[test]
fn write_then_read() {
    let text = write_ledger(&sample()).unwrap();
    assert_eq!(text, SAMPLE);
    assert_eq!(read_ledger(&text).unwrap(), sample());
}

#[test]
fn read_refuses_what_it_cannot_take() {
    assert!(read_ledger("").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\n  machine traced\n").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\n  machine tested many\n").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\n  machine traced\n  machine none\n").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\nfunction g at 0x1 bytes 0x2\n").is_err());
    let comments = "# kept by hand\nra-asm-verify ledger 1\n\nfunction f at 0x1 bytes 0x2\n# none yet\n  machine traced\n";
    assert_eq!(read_ledger(comments).unwrap().evidence(1, "machine"), Evidence::Traced);
}

#[test]
fn evidence_orders_by_strength() {
    assert!(Evidence::None < Evidence::Traced);
    assert!(Evidence::Traced < Evidence::Tested(0));
    assert!(Evidence::Tested(2) < Evidence::Tested(3));
}

#[test]
fn merge_keeps_the_stronger_evidence() {
    let mut held = sample();
    let mut later = Ledger::default();
    later.note(0x1_4000_1000, "F_140001000", BYTES, "machine", Evidence::Traced);
    later.note(0x1_4000_1000, "F_140001000", BYTES, "promoted", Evidence::Tested(3));
    later.note(0x1_4000_1000, "F_140001000", BYTES, "rust", Evidence::Traced);
    later.note(0x1_4000_3000, "F_140003000", 0x7, "machine", Evidence::Tested(2));
    held.merge(later);

    assert_eq!(held.evidence(0x1_4000_1000, "machine"), Evidence::Tested(12));
    assert_eq!(held.evidence(0x1_4000_1000, "promoted"), Evidence::Tested(3));
    assert_eq!(held.evidence(0x1_4000_1000, "c"), Evidence::None);
    assert_eq!(held.evidence(0x1_4000_1000, "rust"), Evidence::Traced);
    assert_eq!(held.evidence(0x1_4000_2000, "abi"), Evidence::Tested(1));
    assert_eq!(held.evidence(0x1_4000_3000, "machine"), Evidence::Tested(2));

    let mut noted = sample();
    noted.note(0x1_4000_1000, "F_140001000", BYTES, "machine", Evidence::Tested(4));
    assert_eq!(noted.evidence(0x1_4000_1000, "machine"), Evidence::Tested(12));
}

#[test]
fn changed_bytes_lose_their_lines() {
    let changed = digest(b"\x48\x89\xc8\xc3");
    assert_ne!(changed, BYTES);

    let mut noted = sample();
    noted.note(0x1_4000_1000, "F_140001000", changed, "c", Evidence::Traced);
    let function = &noted.functions[&0x1_4000_1000];
    assert_eq!(function.bytes, changed);
    assert_eq!(function.steps.len(), 1);
    assert_eq!(noted.evidence(0x1_4000_1000, "machine"), Evidence::None);
    assert_eq!(noted.evidence(0x1_4000_1000, "c"), Evidence::Traced);

    let mut merged = sample();
    let mut later = Ledger::default();
    later.note(0x1_4000_1000, "F_140001000", changed, "promoted", Evidence::None);
    merged.merge(later);
    assert_eq!(merged.evidence(0x1_4000_1000, "machine"), Evidence::None);
    assert_eq!(merged.evidence(0x1_4000_1000, "promoted"), Evidence::None);
    assert_eq!(merged.functions[&0x1_4000_1000].steps.len(), 1);
    assert_eq!(merged.evidence(0x1_4000_2000, "abi"), Evidence::Tested(1));

    let mut forgotten = sample();
    forgotten.forget_changed(|address| (address == 0x1_4000_1000).then_some(changed));
    assert!(forgotten.functions.is_empty());
    let mut kept = sample();
    kept.forget_changed(|address| Some(if address == 0x1_4000_1000 { BYTES } else { changed }));
    assert_eq!(kept.functions.keys().copied().collect::<Vec<_>>(), [0x1_4000_1000]);
}

#[test]
fn digest_is_fnv1a() {
    assert_eq!(digest(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(digest(b"a"), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn the_gen_directory_merges_across_runs() {
    let gen_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ledger-gen");
    let _ = std::fs::remove_dir_all(&gen_dir);
    assert_eq!(ledger::load(&gen_dir).unwrap(), Ledger::default());

    ledger::update(&gen_dir, sample()).unwrap();
    let mut second = Ledger::default();
    second.note(0x1_4000_2000, "main", 0x1, "abi", Evidence::Tested(5));
    let written = ledger::update(&gen_dir, second).unwrap();

    assert_eq!(ledger::load(&gen_dir).unwrap(), written);
    assert_eq!(written.evidence(0x1_4000_2000, "abi"), Evidence::Tested(5));
    assert_eq!(written.evidence(0x1_4000_1000, "machine"), Evidence::Tested(12));
    assert_eq!(std::fs::read_to_string(ledger::path(&gen_dir)).unwrap(), write_ledger(&written).unwrap());
    assert!(matches!(written.functions.get(&0x1_4000_2000), Some(Function { name, .. }) if name == "main"));
}
