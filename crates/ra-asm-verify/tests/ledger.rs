//! The ledger: its file, the merge of runs, the lines a function loses when its bytes change, the
//! `assumptions` of a function and their state, and the `coverage` of `control64.exe`: the blocks
//! its recorded vectors ran when they replay.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ra_asm_lift::raise::assume::{Assumption, Kind, Step};
use ra_asm_lift::{Lifted, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::ledger::{self, Function};
use ra_asm_verify::record::record;
use ra_asm_verify::{Assumed, Counts, Evidence, Ledger, Replayer, Report, State, Tested, Vector, digest, read_ledger, write_ledger};

const BYTES: u64 = 0x9b5f_3c8e_2a1d_4f70;

/// A test of `vectors` vectors over `blocks` blocks, `never` the ones no vector ran.
fn tested(vectors: u32, blocks: u32, never: &[&str]) -> Evidence {
    Evidence::Tested(Tested { vectors, blocks, never: never.iter().map(|name| (*name).to_owned()).collect() })
}

/// A ledger with every kind of evidence on one function and a second function after it.
fn sample() -> Ledger {
    let mut ledger = Ledger::default();
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "machine", tested(12, 5, &["b_140001020", "b_14000103a"]));
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "promoted", Evidence::Traced);
    ledger.note(0x1_4000_1000, "F_140001000", BYTES, "c", Evidence::None);
    ledger.note(0x1_4000_2000, "main", 0x1, "abi", tested(1, 1, &[]));
    ledger
}

const SAMPLE: &str = "\
ra-asm-verify ledger 1

function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
  c none
  machine tested 12 vectors, 3 of 5 blocks, never b_140001020 b_14000103a
  promoted traced

function main at 0x140002000 bytes 0x1
  abi tested 1 vector, 1 of 1 block
";

#[test]
fn write_then_read() {
    let text = write_ledger(&sample()).unwrap();
    assert_eq!(text, SAMPLE);
    assert_eq!(read_ledger(&text).unwrap(), sample());
}

#[test]
fn read_refuses_what_it_cannot_take() {
    let function = "ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\n";
    assert!(read_ledger("").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\n  machine traced\n").is_err());
    assert!(read_ledger(&format!("{function}  machine tested many vectors, 1 of 1 block\n")).is_err());
    assert!(read_ledger(&format!("{function}  machine traced\n  machine none\n")).is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\nfunction g at 0x1 bytes 0x2\n").is_err());
    let comments = "# kept by hand\nra-asm-verify ledger 1\n\nfunction f at 0x1 bytes 0x2\n# none yet\n  machine traced\n";
    assert_eq!(read_ledger(comments).unwrap().evidence(1, "machine"), Evidence::Traced);
}

#[test]
fn coverage_lines_that_do_not_add_up_are_refused() {
    let function = "ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\n";
    let read = |line: &str| read_ledger(&format!("{function}  machine tested {line}\n"));
    assert_eq!(read("2 vectors, 1 of 2 blocks, never b_2").unwrap().evidence(1, "machine"), tested(2, 2, &["b_2"]));
    // The blocks run and the blocks never run make the blocks of the function.
    assert!(read("2 vectors, 2 of 2 blocks, never b_2").is_err());
    assert!(read("2 vectors, 1 of 2 blocks").is_err());
    assert!(read("2 vectors, 0 of 1 block, never b_1 b_2").is_err());
    assert!(read("2 vectors, 0 of 2 blocks, never b_1 b_1").is_err());
    assert!(read("2 vectors, 1 of 2 blocks, never").is_err());
    assert!(read("2").is_err());
}

#[test]
fn evidence_orders_by_strength() {
    assert!(Evidence::None < Evidence::Traced);
    assert!(Evidence::Traced < tested(0, 0, &[]));
    assert!(tested(2, 3, &[]) < tested(3, 3, &[]));
    // Forty vectors through one path are one test: the blocks come first.
    assert!(tested(40, 3, &["b_2"]) < tested(2, 3, &[]));
}

#[test]
fn merge_keeps_the_stronger_evidence() {
    let mut held = sample();
    let mut later = Ledger::default();
    later.note(0x1_4000_1000, "F_140001000", BYTES, "machine", Evidence::Traced);
    later.note(0x1_4000_1000, "F_140001000", BYTES, "promoted", tested(3, 5, &[]));
    later.note(0x1_4000_1000, "F_140001000", BYTES, "rust", Evidence::Traced);
    later.note(0x1_4000_3000, "F_140003000", 0x7, "machine", tested(2, 1, &[]));
    held.merge(later);

    assert_eq!(held.evidence(0x1_4000_1000, "machine"), tested(12, 5, &["b_140001020", "b_14000103a"]));
    assert_eq!(held.evidence(0x1_4000_1000, "promoted"), tested(3, 5, &[]));
    assert_eq!(held.evidence(0x1_4000_1000, "c"), Evidence::None);
    assert_eq!(held.evidence(0x1_4000_1000, "rust"), Evidence::Traced);
    assert_eq!(held.evidence(0x1_4000_2000, "abi"), tested(1, 1, &[]));
    assert_eq!(held.evidence(0x1_4000_3000, "machine"), tested(2, 1, &[]));

    let mut noted = sample();
    noted.note(0x1_4000_1000, "F_140001000", BYTES, "machine", tested(4, 5, &["b_140001020", "b_14000103a"]));
    assert_eq!(noted.evidence(0x1_4000_1000, "machine"), tested(12, 5, &["b_140001020", "b_14000103a"]));
}

#[test]
fn coverage_of_two_runs_is_every_block_either_ran() {
    let mut held = sample();
    held.note(0x1_4000_1000, "F_140001000", BYTES, "machine", tested(2, 5, &["b_14000103a", "b_140001050"]));
    assert_eq!(held.evidence(0x1_4000_1000, "machine"), tested(12, 5, &["b_14000103a"]));
    // Another count of blocks is another lift of the bytes: the stronger test stands.
    held.note(0x1_4000_1000, "F_140001000", BYTES, "machine", tested(1, 6, &[]));
    assert_eq!(held.evidence(0x1_4000_1000, "machine"), tested(1, 6, &[]));
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
    assert_eq!(merged.evidence(0x1_4000_2000, "abi"), tested(1, 1, &[]));

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
    second.note(0x1_4000_2000, "main", 0x1, "abi", tested(5, 1, &[]));
    let written = ledger::update(&gen_dir, second).unwrap();

    assert_eq!(ledger::load(&gen_dir).unwrap(), written);
    assert_eq!(written.evidence(0x1_4000_2000, "abi"), tested(5, 1, &[]));
    assert_eq!(written.evidence(0x1_4000_1000, "machine"), tested(12, 5, &["b_140001020", "b_14000103a"]));
    assert_eq!(std::fs::read_to_string(ledger::path(&gen_dir)).unwrap(), write_ledger(&written).unwrap());
    assert!(matches!(written.functions.get(&0x1_4000_2000), Some(Function { name, .. }) if name == "main"));
}

/// An assumption of `F_140001000` as the raise lists it.
fn raised(step: Step, kind: Kind, site: Option<u64>, discharged: Option<u64>) -> Assumption {
    Assumption { function: "F_140001000".to_owned(), step, kind, site, discharged }
}

/// The sample with one assumption in each state, and one without a site.
fn assuming() -> Ledger {
    let mut ledger = sample();
    for (assumption, ran) in [
        (raised(Step::Abi, Kind::CalleeSaved, Some(0x1_4000_1010), Some(0x1_4000_2000)), false),
        (raised(Step::JumpTable, Kind::TableIndex, Some(0x1_4000_1024), None), true),
        (raised(Step::Devirtualize, Kind::CompleteCandidates, Some(0x1_4000_1040), None), true),
        (raised(Step::Stack, Kind::BalancedStack, None, None), false),
    ] {
        let (assumed, state) = ledger::assumption(&assumption, ran);
        ledger.assume(0x1_4000_1000, "F_140001000", BYTES, assumed, state);
    }
    ledger
}

const ASSUMING: &str = "\
ra-asm-verify ledger 1

function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
  c none
  machine tested 12 vectors, 3 of 5 blocks, never b_140001020 b_14000103a
  promoted traced
  assumes balanced-stack by stack open
  assumes callee-saved by abi at 0x140001010 discharged by 0x140002000
  assumes table-index by jump-table at 0x140001024 checked
  assumes complete-candidates by devirtualize at 0x140001040 open

function main at 0x140002000 bytes 0x1
  abi tested 1 vector, 1 of 1 block
";

fn assumed(kind: &str, step: &str, site: Option<u64>) -> Assumed {
    Assumed { site, kind: kind.to_owned(), step: step.to_owned() }
}

#[test]
fn assumptions_write_then_read() {
    let text = write_ledger(&assuming()).unwrap();
    assert_eq!(text, ASSUMING);
    assert_eq!(read_ledger(&text).unwrap(), assuming());
}

#[test]
fn assumptions_are_discharged_checked_or_open() {
    let ledger = assuming();
    let function = &ledger.functions[&0x1_4000_1000];
    assert_eq!(function.assumptions[&assumed("callee-saved", "abi", Some(0x1_4000_1010))], State::Discharged(0x1_4000_2000));
    assert_eq!(function.assumptions[&assumed("table-index", "jump-table", Some(0x1_4000_1024))], State::Checked);
    // The lifted code does not stop where a vtable slot holds another class's function: a run
    // through the site shows nothing.
    assert_eq!(function.assumptions[&assumed("complete-candidates", "devirtualize", Some(0x1_4000_1040))], State::Open);
    // A table no vector ran through is open, and a site the lift did not give can be run by none.
    let (_, state) = ledger::assumption(&raised(Step::JumpTable, Kind::TableIndex, Some(0x1_4000_1024), None), false);
    assert_eq!(state, State::Open);
    let (_, state) = ledger::assumption(&raised(Step::JumpTable, Kind::TableIndex, None, None), true);
    assert_eq!(state, State::Open);

    assert_eq!(function.counts(), Counts { discharged: 1, checked: 1, open: 2 });
    assert_eq!(ledger.counts(), function.counts());
    assert_eq!(
        function.proved_under(),
        [
            "table-index at 0x140001024 by jump-table, checked",
            "balanced-stack by stack, open",
            "complete-candidates at 0x140001040 by devirtualize, open",
        ]
    );
    assert!(ledger.functions[&0x1_4000_2000].proved_under().is_empty());
}

#[test]
fn assumptions_keep_the_stronger_state_and_go_with_changed_bytes() {
    let table = assumed("table-index", "jump-table", Some(0x1_4000_1024));
    let candidates = assumed("complete-candidates", "devirtualize", Some(0x1_4000_1040));
    let mut held = assuming();
    held.assume(0x1_4000_1000, "F_140001000", BYTES, table.clone(), State::Open);
    assert_eq!(held.functions[&0x1_4000_1000].assumptions[&table], State::Checked, "a run that did not reach the site takes nothing away");

    let mut later = Ledger::default();
    later.assume(0x1_4000_1000, "F_140001000", BYTES, candidates.clone(), State::Checked);
    later.assume(0x1_4000_1000, "F_140001000", BYTES, table.clone(), State::Open);
    held.merge(later);
    let function = &held.functions[&0x1_4000_1000];
    assert_eq!(function.assumptions[&candidates], State::Checked);
    assert_eq!(function.assumptions[&table], State::Checked);
    assert_eq!(function.counts(), Counts { discharged: 1, checked: 2, open: 1 });
    assert_eq!(held.evidence(0x1_4000_1000, "machine"), tested(12, 5, &["b_140001020", "b_14000103a"]), "the evidence stands");

    let changed = digest(b"\xc3");
    let mut noted = assuming();
    noted.assume(0x1_4000_1000, "F_140001000", changed, table.clone(), State::Open);
    assert_eq!(noted.functions[&0x1_4000_1000].assumptions.len(), 1);
    assert_eq!(noted.functions[&0x1_4000_1000].assumptions[&table], State::Open);
    assert_eq!(noted.evidence(0x1_4000_1000, "machine"), Evidence::None);
}

#[test]
fn assumptions_that_do_not_read_are_refused() {
    let function = "ra-asm-verify ledger 1\nfunction f at 0x1 bytes 0x2\n";
    let read = |line: &str| read_ledger(&format!("{function}  {line}\n"));
    assert_eq!(read("assumes table-index by jump-table at 0x10 checked").unwrap().functions[&1].counts().checked, 1);
    assert_eq!(read("assumes frame-pointer by stack discharged by 0x20").unwrap().functions[&1].counts().discharged, 1);
    assert!(read("assumes table-index by jump-table at 0x10 held").is_err());
    assert!(read("assumes table-index by jump-table at ten open").is_err());
    assert!(read("assumes table-index by jump-table discharged").is_err());
    assert!(read("assumes table-index jump-table open").is_err());
    assert!(read("assumes table-index by jump-table open\n  assumes table-index by jump-table open").is_err());
    assert!(read_ledger("ra-asm-verify ledger 1\n  assumes table-index by jump-table open\n").is_err());
    // A step of that name would read back as an assumption.
    let mut ledger = Ledger::default();
    ledger.note(1, "f", 2, "assumes", Evidence::Traced);
    assert!(write_ledger(&ledger).is_err());
}

/// `op_div` of the fixture: `if (b == 0) fail(3)`, entered once with a divisor of six, so the
/// block at `0x1400016a6` that calls `fail` never runs.
const OP_DIV: u64 = 0x1_4000_1690;
const FAIL_BLOCK: &str = "1400016a6";

/// `print_int` of the fixture, entered for every number the program prints.
const PRINT_INT: u64 = 0x1_4000_1330;

fn image() -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe");
    ra_asm_pe::load(&path).expect("the fixture loads")
}

/// The fixture lifted and raised to `level`, under the names the recorded run knew.
fn lifted(image: &Image, level: RaiseLevel) -> Lifted {
    let analysis = ra_asm_analysis::analyze(image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
    let options = lifted.raise_options(level);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, None);
    lifted
}

/// The vectors of one recorded run of the fixture at the machine level, made once; none on a
/// host without the Windows DLLs the program imports, where the tests are skipped.
fn vectors() -> Option<&'static [Vector]> {
    static RUN: OnceLock<Option<Vec<Vector>>> = OnceLock::new();
    RUN.get_or_init(|| {
        let image = image();
        let lifted = lifted(&image, RaiseLevel::Machine);
        let (recording, outcome) = record(&image, &lifted, &ra_asm_lift::run::Options::default()).expect("the run starts");
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: control64.exe: {stop}; this host has no Windows DLLs: the coverage is skipped");
            return None;
        }
        assert_eq!(outcome.exit, 0, "the recorded run ends as the program does (stop: {:?})", outcome.stop);
        Some(recording.vectors)
    })
    .as_deref()
}

/// Every vector replays equal at `level`, and what the vectors of each function ran of it is
/// whole: the blocks are the function's, the entry ran, and the blocks never run are among them.
/// Gives the report and the ledger it makes, its step named after the level.
fn covered(level: RaiseLevel) -> Option<(Report, Ledger)> {
    let vectors = vectors()?;
    let image = image();
    let lifted = lifted(&image, level);
    let replayer = Replayer::new(&image, &lifted, level).expect("the level replays");
    let report = replayer.report(vectors).expect("the vectors load");
    let lines: Vec<String> = report.failures.iter().map(ToString::to_string).collect();
    assert!(report.failures.is_empty(), "every vector replays at the {} level:\n{}", level.name(), lines.join("\n"));
    let functions: BTreeSet<u64> = vectors.iter().map(|vector| vector.address).collect();
    assert_eq!(report.covered.keys().copied().collect::<BTreeSet<_>>(), functions, "every function with a vector is covered");

    let mut ledger = Ledger::default();
    for (address, covered) in &report.covered {
        let tested = &covered.tested;
        let blocks = replayer.blocks(&covered.function).expect("the function is in the module");
        assert_eq!(tested.blocks as usize, blocks.len(), "{}: the blocks of the function", covered.function);
        assert!(tested.vectors >= 1 && tested.covered() >= 1, "{}: {tested}", covered.function);
        assert!(tested.never.iter().all(|name| blocks.contains(&name.as_str())), "{}: {tested}", covered.function);
        assert!(!tested.never.contains(blocks[0]), "{}: the entry block ran: {tested}", covered.function);
        // A stand-in for the digest of the function's bytes, which the replay does not need.
        ledger.note(*address, &covered.function, digest(&address.to_le_bytes()), level.name(), Evidence::Tested(tested.clone()));
    }

    // The branch to `fail` is in the function at every level, and the one vector never takes it.
    let op_div = &report.covered[&OP_DIV].tested;
    assert_eq!(op_div.vectors, 1, "op_div: {op_div}");
    assert!(op_div.covered() < op_div.blocks, "op_div: {op_div}");
    let print_int = &report.covered[&PRINT_INT].tested;
    assert!(print_int.vectors >= 2, "print_int: {print_int}");
    Some((report, ledger))
}

#[test]
fn coverage_of_control64_at_the_machine_level_names_the_blocks_no_vector_ran() {
    let Some((report, ledger)) = covered(RaiseLevel::Machine) else { return };
    let op_div = &report.covered[&OP_DIV].tested;
    assert!(op_div.never.iter().any(|name| name.contains(FAIL_BLOCK)), "op_div: the call of fail never runs: {op_div}");
    let text = write_ledger(&ledger).expect("the ledger writes");
    let op_div = text
        .lines()
        .find(|line| line.starts_with("  machine tested 1 vector, ") && line.contains(FAIL_BLOCK))
        .unwrap_or_else(|| panic!("a line says the test of op_div:\n{text}"));
    assert!(op_div.contains(" blocks, never "), "{op_div}");
    assert_eq!(read_ledger(&text).expect("the ledger reads back"), ledger);
}

#[test]
fn coverage_of_control64_at_the_promoted_level_reads_back_and_merges_with_the_machine_level() {
    let Some((_, promoted)) = covered(RaiseLevel::Promoted) else { return };
    let Some((_, mut ledger)) = covered(RaiseLevel::Machine) else { return };
    ledger.merge(promoted.clone());
    let text = write_ledger(&ledger).expect("the ledger writes");
    let read = read_ledger(&text).expect("the ledger reads back");
    assert_eq!(read, ledger);
    for (address, function) in &promoted.functions {
        assert_eq!(read.evidence(*address, "promoted"), function.steps["promoted"], "{}", function.name);
        assert!(matches!(read.evidence(*address, "machine"), Evidence::Tested(_)), "{}", function.name);
    }
}
