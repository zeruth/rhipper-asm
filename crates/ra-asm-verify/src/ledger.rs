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
//! The file is text in the project's gen directory, one item a line:
//!
//! ```text
//! ra-asm-verify ledger 1
//!
//! function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
//!   machine tested 12 vectors, 3 of 5 blocks, never b_140001020 b_14000103a
//!   promoted traced
//!   c none
//! ```
//!
//! Functions go by address, steps and blocks by name, all in order. A blank line and a line that
//! starts with `#` say nothing; the indentation is for the reader.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::vector::{Error, hex64, word};

/// The line every ledger file starts with; the number is the format's version.
const HEADER: &str = "ra-asm-verify ledger 1";

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

/// One function's lines: the bytes they hold for and the evidence of each step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Function {
    /// The function's name, one word.
    pub name: String,
    /// The digest of the function's bytes ([`digest`]) the evidence was learned on.
    pub bytes: u64,
    /// The evidence of each step, by the step's name, one word.
    pub steps: BTreeMap<String, Evidence>,
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
        let function = self.functions.entry(address).or_default();
        if function.bytes != bytes || function.name != name {
            *function = Function { name: name.to_owned(), bytes, steps: BTreeMap::new() };
        }
        let held = function.steps.entry(step.to_owned()).or_default();
        *held = std::mem::take(held).join(evidence);
    }

    /// Takes in the ledger of a later run. Where both hold a function on the same bytes, every
    /// step joins the evidence of both; where the bytes differ, the later run's lines stand.
    pub fn merge(&mut self, later: Ledger) {
        for (address, function) in later.functions {
            match self.functions.entry(address) {
                Entry::Vacant(slot) => {
                    slot.insert(function);
                }
                Entry::Occupied(mut slot) => {
                    let held = slot.get_mut();
                    if held.bytes == function.bytes && held.name == function.name {
                        for (step, evidence) in function.steps {
                            let kept = held.steps.entry(step).or_default();
                            *kept = std::mem::take(kept).join(evidence);
                        }
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
            if step.starts_with('#') {
                return Err(Error::unwritable(format!("the step name {step:?} would read as a comment")));
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
                let function = Function { name: (*name).to_owned(), bytes: hex64(line, bytes)?, steps: BTreeMap::new() };
                if ledger.functions.contains_key(&address) || current.as_ref().is_some_and(|(held, _)| *held == address) {
                    return Err(Error::at(line, format!("the function at {address:#x} is in the ledger twice")));
                }
                if let Some((held, done)) = current.replace((address, function)) {
                    ledger.functions.insert(held, done);
                }
            }
            ["function", ..] => return Err(Error::at(line, "expected `function <name> at <address> bytes <digest>`")),
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
