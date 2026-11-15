//! The ledger: what the verification has learned of each function, kept across runs.
//!
//! For every function and every step of the port (a level of the lift, an output of the emit),
//! the ledger keeps the strongest evidence seen so far: `tested` with the count of vectors the
//! step replayed equal and the blocks of the function they ran, `traced` when the function was
//! only seen entered, `none`. Forty vectors through one path are one test: a test is as strong as
//! the blocks it ran first, and the blocks no vector ran are named, for the vectors still to make.
//! A function's lines hold for the bytes they were learned on: the ledger keeps a digest of them,
//! and a function whose bytes changed starts over.
//!
//! Beside the evidence, the ledger keeps what the raise assumed of each function (see
//! [`ra_asm_lift::raise::assume`]) and how far each assumption is known to hold, weakest first:
//! `open`, nothing shows it; `checked`, a recorded vector ran its site through code that stops
//! where it does not hold, as the checked build's `ra_assume` does, and it held; `discharged`, the
//! code the step found proves it, by that code's address. What a function is proved under is its
//! assumptions that are not discharged ([`Function::proved_under`]).
//!
//! The file is text in the project's gen directory, one item a line:
//!
//! ```text
//! ra-asm-verify ledger 1
//!
//! function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
//!   machine tested 12 vectors, 3 of 5 blocks, never b_140001020 b_14000103a
//!   promoted traced
//!   c none
//!   assumes callee-saved by abi at 0x140001010 discharged by 0x140002000
//!   assumes table-index by jump-table at 0x140001024 checked
//!   assumes complete-candidates by devirtualize at 0x140001040 open
//! ```
//!
//! Functions go by address, steps and blocks by name, assumptions by site, kind and step, all in
//! order; an assumption the lift gave no site has no `at`. A blank line and a line that starts with
//! `#` say nothing; the indentation is for the reader.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use ra_asm_lift::raise::assume::{Assumption, Kind};

use crate::vector::{Error, hex64, word};

/// The line every ledger file starts with; the number is the format's version.
const HEADER: &str = "ra-asm-verify ledger 1";

/// The first word of an assumption's line, which no step is called.
const ASSUMES: &str = "assumes";

/// The kinds of assumption the lifted code stops at where they do not hold, so a vector that
/// replays equal through the site shows it held there: a jump table's index outside the table
/// goes to the trap of the switch, as the checked build's `ra_assume` stops.
const WATCHED: [Kind; 1] = [Kind::TableIndex];

/// The ledger's file name in the gen directory.
pub const FILE_NAME: &str = "ledger.txt";

/// What a step is known to do with a function, ordered by strength: any test is stronger than a
/// trace, and a trace than nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Evidence {
    /// Nothing is known.
    #[default]
    None,
    /// The function was seen entered, without a vector replayed against it.
    Traced,
    /// Vectors replayed equal.
    Tested(Tested),
}

impl Evidence {
    /// The kind of the evidence, weakest first.
    fn rank(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Traced => 1,
            Self::Tested(_) => 2,
        }
    }

    /// What two runs show together. Two tests of the same number of blocks ran every block
    /// either ran; otherwise the stronger evidence stands.
    #[must_use]
    pub fn join(self, other: Evidence) -> Evidence {
        match (self, other) {
            (Self::Tested(a), Self::Tested(b)) if a.blocks == b.blocks => Self::Tested(Tested {
                vectors: a.vectors.max(b.vectors),
                blocks: a.blocks,
                never: a.never.intersection(&b.never).cloned().collect(),
            }),
            (a, b) => a.max(b),
        }
    }
}

impl Ord for Evidence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Tested(a), Self::Tested(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

impl PartialOrd for Evidence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// What the vectors of a step that replayed equal ran of a function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tested {
    /// The vectors that replayed equal.
    pub vectors: u32,
    /// The blocks of the function at the step.
    pub blocks: u32,
    /// The blocks no vector ran, by name, one word each.
    pub never: BTreeSet<String>,
}

impl Tested {
    /// The blocks some vector ran.
    #[must_use]
    pub fn covered(&self) -> u32 {
        self.blocks.saturating_sub(u32::try_from(self.never.len()).unwrap_or(u32::MAX))
    }
}

/// A test is as strong as the blocks it ran, then as the vectors that ran them; the rest of the
/// order only keeps it whole.
impl Ord for Tested {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.covered(), self.vectors, self.blocks, &self.never).cmp(&(other.covered(), other.vectors, other.blocks, &other.never))
    }
}

impl PartialOrd for Tested {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The test as its line in the file says it after `tested`: `5 vectors, 11 of 14 blocks`, and
/// `, never` with the blocks no vector ran when there are any.
impl fmt::Display for Tested {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vectors = if self.vectors == 1 { "vector" } else { "vectors" };
        let blocks = if self.blocks == 1 { "block" } else { "blocks" };
        write!(f, "{} {vectors}, {} of {} {blocks}", self.vectors, self.covered(), self.blocks)?;
        if !self.never.is_empty() {
            f.write_str(", never")?;
            for name in &self.never {
                write!(f, " {name}")?;
            }
        }
        Ok(())
    }
}

/// One thing the raise assumed of a function: where, what, and the step that relied on it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Assumed {
    /// The address of the instruction it is made at, when the lift gave it one.
    pub site: Option<u64>,
    /// The kind, by its report name ([`Kind::name`]), one word.
    pub kind: String,
    /// The step, by its report name, one word.
    pub step: String,
}

/// `table-index at 0x140001024 by jump-table`, as a person reads it.
impl fmt::Display for Assumed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.kind)?;
        if let Some(site) = self.site {
            write!(f, " at {site:#x}")?;
        }
        write!(f, " by {}", self.step)
    }
}

/// How far an assumption is known to hold, ordered by strength.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// Nothing shows it holds.
    #[default]
    Open,
    /// A recorded vector ran the site through code that stops where it does not hold, and the
    /// vector replayed equal.
    Checked,
    /// The code at this address proves it: the check the step saw, or the callee whose summary
    /// keeps what the call assumed.
    Discharged(u64),
}

/// The state as its line says it: `open`, `checked`, `discharged by 0x140002000`.
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => f.write_str("open"),
            Self::Checked => f.write_str("checked"),
            Self::Discharged(by) => write!(f, "discharged by {by:#x}"),
        }
    }
}

/// The assumption `assumption` as the ledger keeps it, and its state: discharged when the step
/// found the code that proves it, checked when it is of a [`WATCHED`] kind and `ran` says a
/// vector that replayed equal ran its site, open otherwise.
#[must_use]
pub fn assumption(assumption: &Assumption, ran: bool) -> (Assumed, State) {
    let assumed = Assumed { site: assumption.site, kind: assumption.kind.name().to_owned(), step: assumption.step.name().to_owned() };
    let state = match assumption.discharged {
        Some(by) => State::Discharged(by),
        None if ran && assumption.site.is_some() && WATCHED.contains(&assumption.kind) => State::Checked,
        None => State::Open,
    };
    (assumed, state)
}

/// The assumptions of a function or a program, by state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub discharged: usize,
    pub checked: usize,
    pub open: usize,
}

impl Counts {
    fn add(&mut self, state: State) {
        match state {
            State::Open => self.open += 1,
            State::Checked => self.checked += 1,
            State::Discharged(_) => self.discharged += 1,
        }
    }
}

/// One function's lines: the bytes they hold for, the evidence of each step and the assumptions
/// of the raise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Function {
    /// The function's name, one word.
    pub name: String,
    /// The digest of the function's bytes ([`digest`]) the evidence was learned on.
    pub bytes: u64,
    /// The evidence of each step, by the step's name, one word.
    pub steps: BTreeMap<String, Evidence>,
    /// What the raise assumed of the function, with how far each is known to hold.
    pub assumptions: BTreeMap<Assumed, State>,
}

impl Function {
    /// A function of no line yet.
    fn new(name: &str, bytes: u64) -> Self {
        Self { name: name.to_owned(), bytes, ..Self::default() }
    }

    /// The assumptions by state.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for state in self.assumptions.values() {
            counts.add(*state);
        }
        counts
    }

    /// What the function is proved under: a line for each assumption no code discharges, the
    /// checked ones first, each as `table-index at 0x140001024 by jump-table, checked`.
    #[must_use]
    pub fn proved_under(&self) -> Vec<String> {
        let mut open: Vec<(&Assumed, State)> =
            self.assumptions.iter().filter(|(_, state)| !matches!(state, State::Discharged(_))).map(|(a, s)| (a, *s)).collect();
        open.sort_by_key(|(_, state)| std::cmp::Reverse(*state));
        open.into_iter().map(|(assumed, state)| format!("{assumed}, {state}")).collect()
    }

    /// Joins what two runs show of the function's lines on the same bytes.
    fn join(&mut self, other: Function) {
        for (step, evidence) in other.steps {
            let kept = self.steps.entry(step).or_default();
            *kept = std::mem::take(kept).join(evidence);
        }
        for (assumed, state) in other.assumptions {
            let kept = self.assumptions.entry(assumed).or_default();
            *kept = (*kept).max(state);
        }
    }
}

/// The evidence of a program's functions, by address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    pub functions: BTreeMap<u64, Function>,
}

impl Ledger {
    /// The evidence of a step on the function at `address`, `none` when the ledger has no line.
    #[must_use]
    pub fn evidence(&self, address: u64, step: &str) -> Evidence {
        self.functions.get(&address).and_then(|function| function.steps.get(step)).cloned().unwrap_or_default()
    }

    /// Notes evidence of a step on a function whose bytes have the digest `bytes`. It joins
    /// what the ledger held ([`Evidence::join`]); the lines of the function's former bytes go.
    pub fn note(&mut self, address: u64, name: &str, bytes: u64, step: &str, evidence: Evidence) {
        let held = self.on(address, name, bytes).steps.entry(step.to_owned()).or_default();
        *held = std::mem::take(held).join(evidence);
    }

    /// Notes an assumption of the function at `address`, on bytes of the digest `bytes`, in the
    /// state `state`: the stronger of it and what the ledger held stands; the lines of the
    /// function's former bytes go.
    pub fn assume(&mut self, address: u64, name: &str, bytes: u64, assumed: Assumed, state: State) {
        let held = self.on(address, name, bytes).assumptions.entry(assumed).or_default();
        *held = (*held).max(state);
    }

    /// The lines of the function at `address` on the bytes `bytes`, those of other bytes or
    /// another name dropped.
    fn on(&mut self, address: u64, name: &str, bytes: u64) -> &mut Function {
        let function = self.functions.entry(address).or_default();
        if function.bytes != bytes || function.name != name {
            *function = Function::new(name, bytes);
        }
        function
    }

    /// The assumptions of every function by state.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for state in self.functions.values().flat_map(|function| function.assumptions.values()) {
            counts.add(*state);
        }
        counts
    }

    /// Takes in the ledger of a later run. Where both hold a function on the same bytes, every
    /// step joins the evidence of both and every assumption keeps the stronger state; where the
    /// bytes differ, the later run's lines stand.
    pub fn merge(&mut self, later: Ledger) {
        for (address, function) in later.functions {
            match self.functions.entry(address) {
                Entry::Vacant(slot) => {
                    slot.insert(function);
                }
                Entry::Occupied(mut slot) => {
                    let held = slot.get_mut();
                    if held.bytes == function.bytes && held.name == function.name {
                        held.join(function);
                    } else {
                        *held = function;
                    }
                }
            }
        }
    }

    /// Drops the lines of every function whose bytes are no longer those the evidence was
    /// learned on: `current` gives the digest of the bytes at an address today, none when the
    /// program has no function there any more.
    pub fn forget_changed(&mut self, current: impl Fn(u64) -> Option<u64>) {
        self.functions.retain(|&address, function| current(address) == Some(function.bytes));
    }
}

/// The digest of a function's bytes the ledger keeps: 64-bit FNV-1a, stable across builds and
/// machines, which a ledger file outlives.
#[must_use]
pub fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// Writes a ledger as the text of its file. A name that is not one word, of a function, a step
/// or a block, is refused, since it would not read back as itself.
pub fn write_ledger(ledger: &Ledger) -> Result<String, Error> {
    let mut out = String::from(HEADER);
    out.push('\n');
    for (address, function) in &ledger.functions {
        let name = word(&function.name, "function")?;
        let _ = write!(out, "\nfunction {name} at {address:#x} bytes {:#x}\n", function.bytes);
        for (step, evidence) in &function.steps {
            let step = word(step, "step")?;
            if step.starts_with('#') || step == ASSUMES {
                return Err(Error::unwritable(format!("the step name {step:?} would read as another line")));
            }
            let _ = match evidence {
                Evidence::None => writeln!(out, "  {step} none"),
                Evidence::Traced => writeln!(out, "  {step} traced"),
                Evidence::Tested(tested) => {
                    for block in &tested.never {
                        word(block, "block")?;
                    }
                    writeln!(out, "  {step} tested {tested}")
                }
            };
        }
        for (assumed, state) in &function.assumptions {
            let kind = word(&assumed.kind, "assumption kind")?;
            let step = word(&assumed.step, "assumption step")?;
            let _ = write!(out, "  {ASSUMES} {kind} by {step}");
            if let Some(site) = assumed.site {
                let _ = write!(out, " at {site:#x}");
            }
            let _ = writeln!(out, " {state}");
        }
    }
    Ok(out)
}

/// Reads the text of a ledger file.
pub fn read_ledger(text: &str) -> Result<Ledger, Error> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));
    match lines.next() {
        Some((_, HEADER)) => {}
        Some((line, other)) => return Err(Error::at(line, format!("expected `{HEADER}`, found `{other}`"))),
        None => return Err(Error::at(1, format!("expected `{HEADER}`, found an empty file"))),
    }
    let mut ledger = Ledger::default();
    let mut current: Option<(u64, Function)> = None;
    for (line, text) in lines {
        let words: Vec<&str> = text.split_whitespace().collect();
        match words.as_slice() {
            ["function", name, "at", address, "bytes", bytes] => {
                let address = hex64(line, address)?;
                let function = Function::new(name, hex64(line, bytes)?);
                if ledger.functions.contains_key(&address) || current.as_ref().is_some_and(|(held, _)| *held == address) {
                    return Err(Error::at(line, format!("the function at {address:#x} is in the ledger twice")));
                }
                if let Some((held, done)) = current.replace((address, function)) {
                    ledger.functions.insert(held, done);
                }
            }
            ["function", ..] => return Err(Error::at(line, "expected `function <name> at <address> bytes <digest>`")),
            [ASSUMES, kind, "by", step, rest @ ..] => {
                let Some((address, function)) = current.as_mut() else {
                    return Err(Error::at(line, format!("the assumption `{kind}` comes before any function")));
                };
                let (site, state) = match rest {
                    ["at", site, state @ ..] => (Some(hex64(line, site)?), state),
                    state => (None, state),
                };
                let state = match state {
                    ["open"] => State::Open,
                    ["checked"] => State::Checked,
                    ["discharged", "by", by] => State::Discharged(hex64(line, by)?),
                    _ => return Err(Error::at(line, "expected `open`, `checked` or `discharged by <address>` after the assumption")),
                };
                let assumed = Assumed { site, kind: (*kind).to_owned(), step: (*step).to_owned() };
                if function.assumptions.insert(assumed, state).is_some() {
                    return Err(Error::at(line, format!("the assumption `{kind}` of the function at {address:#x} is in the ledger twice")));
                }
            }
            [ASSUMES, ..] => return Err(Error::at(line, format!("expected `{ASSUMES} <kind> by <step> [at <site>] <state>`"))),
            [step, rest @ ..] => {
                let Some((address, function)) = current.as_mut() else {
                    return Err(Error::at(line, format!("the step `{step}` comes before any function")));
                };
                let evidence = match rest {
                    ["none"] => Evidence::None,
                    ["traced"] => Evidence::Traced,
                    ["tested", test @ ..] => Evidence::Tested(tested(line, test)?),
                    _ => return Err(Error::at(line, format!("expected `{step} none`, `{step} traced` or `{step} tested ...`"))),
                };
                if function.steps.insert((*step).to_owned(), evidence).is_some() {
                    return Err(Error::at(line, format!("the step `{step}` of the function at {address:#x} is in the ledger twice")));
                }
            }
            [] => {}
        }
    }
    if let Some((address, function)) = current {
        ledger.functions.insert(address, function);
    }
    Ok(ledger)
}

/// Reads the words of a test after `tested`, as [`Tested`] writes them.
fn tested(line: usize, words: &[&str]) -> Result<Tested, Error> {
    let every_block_ran: &[&str] = &[];
    let (vectors, covered, blocks, never) = match words {
        [vectors, "vector," | "vectors,", covered, "of", blocks, "block" | "blocks"] => (vectors, covered, blocks, every_block_ran),
        [vectors, "vector," | "vectors,", covered, "of", blocks, "block," | "blocks,", "never", never @ ..] if !never.is_empty() => {
            (vectors, covered, blocks, never)
        }
        _ => return Err(Error::at(line, "expected `tested <count> vectors, <count> of <count> blocks`, then `, never` and the blocks")),
    };
    let mut tested = Tested { vectors: count(line, vectors)?, blocks: count(line, blocks)?, never: BTreeSet::new() };
    for block in never {
        if !tested.never.insert((*block).to_owned()) {
            return Err(Error::at(line, format!("the block `{block}` is never run twice")));
        }
    }
    let ran = count(line, covered)?;
    if u32::try_from(never.len()).ok().and_then(|never| tested.blocks.checked_sub(never)) != Some(ran) {
        return Err(Error::at(line, format!("{ran} blocks run and {} never run are not the {} blocks", never.len(), tested.blocks)));
    }
    Ok(tested)
}

/// A count of the line `line`.
fn count(line: usize, text: &str) -> Result<u32, Error> {
    text.parse().map_err(|_| Error::at(line, format!("`{text}` is not a count")))
}

/// The path of the ledger file in a gen directory.
#[must_use]
pub fn path(gen_dir: &Path) -> PathBuf {
    gen_dir.join(FILE_NAME)
}

/// Reads the ledger of a gen directory; an empty ledger when it has none yet.
pub fn load(gen_dir: &Path) -> Result<Ledger> {
    let path = path(gen_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Ledger::default()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    read_ledger(&text).with_context(|| format!("reading {}", path.display()))
}

/// Writes a ledger as the file of a gen directory, the directory made when it is missing.
pub fn store(gen_dir: &Path, ledger: &Ledger) -> Result<()> {
    let path = path(gen_dir);
    let text = write_ledger(ledger)?;
    std::fs::create_dir_all(gen_dir).with_context(|| format!("making {}", gen_dir.display()))?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// Merges what a run learned into the ledger of a gen directory and writes it back; the ledger
/// as written.
pub fn update(gen_dir: &Path, run: Ledger) -> Result<Ledger> {
    let mut ledger = load(gen_dir)?;
    ledger.merge(run);
    store(gen_dir, &ledger)?;
    Ok(ledger)
}
