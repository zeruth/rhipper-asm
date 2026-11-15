//! The tests every machine described by rows shares, in one call: each row has cases and they
//! decode to it, no row of the same specificity accepts a word of another, and each row's semantics
//! runs against the IR builder and the access recorder without a panic. A new machine gets these
//! by writing its rows and calling [`check`] from a test.

use std::fmt;
use std::panic::{self, AssertUnwindSafe};

use crate::cases::{Plan, cases};
use crate::instr::Machine;
use crate::lift::Code;
use crate::outline::Outlines;
use crate::sem::StateSpec;
use crate::table::Row;

/// The address every case is lifted at.
const ADDRESS: u64 = 0x1000;

/// How many cases of each row have every field random, beside those at the fields' boundaries.
const RANDOM: usize = 16;

/// A row that fails the check, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub row: &'static str,
    pub problem: Problem,
}

/// Why a row fails the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No word the row assembles decodes to it: more specific rows take them all, or its fields
    /// make no word.
    Unreachable,
    /// The row's word `word` decodes to another row, or to its fields' values otherwise than they
    /// were assembled.
    Decode { word: u32 },
    /// The row `other` accepts the row's word `word` too and is not less specific than it.
    Shared { word: u32, other: &'static str },
    /// The row's semantics panicked on `word` in `pass`: the access recorder, the lift or the
    /// lift with outlined rows called.
    Panic { word: u32, pass: &'static str, message: String },
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let row = self.row;
        match &self.problem {
            Problem::Unreachable => write!(f, "row `{row}` has no word that decodes to it"),
            Problem::Decode { word } => write!(f, "{word:#010x} of row `{row}` does not decode to it"),
            Problem::Shared { word, other } => write!(f, "rows `{row}` and `{other}` both accept {word:#010x}"),
            Problem::Panic { word, pass, message } => {
                write!(f, "the semantics of row `{row}` panics on {word:#010x} in the {pass}: {message}")
            }
        }
    }
}

/// Checks every row of `machine` over the state `spec` lays out, on the cases `seed` draws: the
/// failures, at most one per row and check, or `Ok` when there are none.
pub fn check(machine: &Machine, spec: &StateSpec, seed: u64) -> Result<(), Vec<Failure>> {
    let plan = Plan { registers: spec.registers as usize, bits: spec.bits, random: RANDOM };
    let mut failures = Vec::new();
    for row in machine.table.rows() {
        let found = cases(&machine.table, row, plan, seed);
        if found.is_empty() {
            failures.push(Failure { row: row.name, problem: Problem::Unreachable });
            continue;
        }
        let mut problems: Vec<Problem> = Vec::new();
        let mut note = |problem: Problem| {
            if !problems.iter().any(|known| std::mem::discriminant(known) == std::mem::discriminant(&problem)) {
                problems.push(problem);
            }
        };
        for case in &found {
            let word = case.word;
            if !machine.table.decode(word).is_some_and(|decoded| std::ptr::eq(decoded.row, row) && decoded.values == case.values) {
                note(Problem::Decode { word });
            }
            if let Some(other) = machine.table.rows().iter().find(|other| !std::ptr::eq(*other, row) && shares(row, other, word)) {
                note(Problem::Shared { word, other: other.name });
            }
            if let Some((pass, message)) = run(machine, spec, word) {
                note(Problem::Panic { word, pass, message });
            }
        }
        failures.extend(problems.into_iter().map(|problem| Failure { row: row.name, problem }));
    }
    if failures.is_empty() { Ok(()) } else { Err(failures) }
}

/// Whether `other` accepts `word` of `row` without being strictly less specific, so that the word
/// is not `row`'s alone.
fn shares(row: &Row, other: &Row, word: u32) -> bool {
    let wider = other.mask & row.mask == other.mask && other.mask != row.mask;
    word & other.mask == other.bits && !wider
}

/// Runs the semantics of `word` against the access recorder, the lift of the word as a function of
/// its own, delayed rows included, and that lift with outlined rows called: the pass that panicked
/// first and its message, or `None`.
fn run(machine: &Machine, spec: &StateSpec, word: u32) -> Option<(&'static str, String)> {
    let words = [word];
    let code = Code { base: ADDRESS, words: &words };
    let passes: [(&'static str, &dyn Fn()); 3] = [
        ("access recorder", &|| drop(machine.access(word, ADDRESS))),
        ("lift", &|| drop(machine.lift_function(spec, &code, ADDRESS, &[]))),
        ("outlined lift", &|| drop(machine.lift_function_outlined(spec, &code, ADDRESS, &[], &mut Outlines::new()))),
    ];
    passes.into_iter().find_map(|(pass, body)| panic::catch_unwind(AssertUnwindSafe(body)).err().map(|payload| (pass, message(&*payload))))
}

/// The text a panic was raised with.
fn message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}
