//! A machine's committed vectors replayed by `ra_asm_machine::replay`: the toy machine's of
//! `testdata/vectors/toy` on any host, and the pending-list rule over vectors written into a
//! scratch directory: an unlisted difference, a listed one, a listed form that passes and a fault.

mod toy;

use std::fs;
use std::path::{Path, PathBuf};

use ra_asm_machine::{Replayed, replay};
use toy::{TOY_SPEC, toy_machine};

/// The header every pending list of these tests starts with.
const HEADER: &str = "# the forms that differ today\n";

/// `add r3, r1, r2` over 0x7fffffff and 1, with the output `r3` of the case.
fn add_case(r3: &str) -> String {
    format!("00611000 pc=00001000 r1=7fffffff r2=00000001 -> pc=00001004 r3={r3}\n")
}

/// The replay of a scratch directory holding `alu.vec` with `alu` and a pending list of `pending`.
fn replay_scratch(name: &str, alu: &str, pending: &str) -> Replayed {
    let scratch = ra_asm_testing::Scratch::new("ra-asm-machine-tests", name);
    fs::write(scratch.join("alu.vec"), alu).unwrap();
    fs::write(scratch.join("PENDING.txt"), format!("{HEADER}{pending}")).unwrap();
    replay(&toy_machine(), &TOY_SPEC, &scratch).unwrap_or_else(|e| panic!("{e}"))
}

fn vectors(machine: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors").join(machine)
}

/// Every committed toy vector replays through the decoder, the lift and the interpreter, and
/// exactly the forms of its pending list differ.
#[test]
fn toy_replays() {
    let replayed = replay(&toy_machine(), &TOY_SPEC, &vectors("toy")).unwrap_or_else(|e| panic!("{e}"));
    println!("{replayed}");
    assert!(replayed.passed() && replayed.forms == 6, "{replayed}");
}

/// A form that differs and is not listed fails with its first difference, as a line to paste.
#[test]
fn an_unlisted_difference_fails() {
    let replayed = replay_scratch("replay-unlisted", &add_case("80000001"), "");
    assert_eq!(replayed.failures, ["alu add: case 1 (00611000): r3 is 0x80000000, the machine gives 0x80000001"], "{replayed}");
}

/// A form that differs and is listed is a finding, not a failure.
#[test]
fn a_listed_difference_passes() {
    let replayed = replay_scratch("replay-listed", &add_case("80000001"), "alu add: r3 is 0x80000000, the machine gives 0x80000001\n");
    assert!(replayed.passed(), "{replayed}");
    assert_eq!((replayed.cases, replayed.differing, replayed.pending), (1, 1, 1), "{replayed}");
}

/// A listed form that passes and a listed form without vectors fail, so the list stays exact.
#[test]
fn a_listed_form_that_passes_fails() {
    let replayed = replay_scratch("replay-passes", &add_case("80000000"), "alu add: once wrong\nalu mix: never replayed\n");
    assert_eq!(replayed.failures, ["remove from PENDING.txt: alu add", "PENDING.txt names a form without vectors: alu mix"], "{replayed}");
}

/// A load from where the case maps nothing faults as the machine does, and a case that says it
/// does not fault differs.
#[test]
fn a_fault_is_compared() {
    let faulting = "40a60004 pc=00001000 r6=00009000 -> fault=segv\n";
    assert!(replay_scratch("replay-fault", faulting, "").passed());
    let loading = "40a60004 pc=00001000 r6=00009000 -> pc=00001004 r5=00000000\n";
    let replayed = replay_scratch("replay-no-fault", loading, "");
    assert_eq!(replayed.failures.len(), 1, "{replayed}");
    assert!(replayed.failures[0].starts_with("alu ld: case 1 (40a60004): faulted with segv"), "{replayed}");
}
