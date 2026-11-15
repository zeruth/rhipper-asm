//! The ledger: what the verification has learned of each function, kept across runs.
//!
//! For every function and every step of the port (a level of the lift, an output of the emit),
//! the ledger keeps the strongest evidence seen so far: `tested` with the count of vectors the
//! step replayed equal, `traced` when the function was only seen entered, `none`. A function's
//! lines hold for the bytes they were learned on: the ledger keeps a digest of them, and a
//! function whose bytes changed starts over.
//!
//! The file is text in the project's gen directory, one item a line:
//!
//! ```text
//! ra-asm-verify ledger 1
//!
//! function F_140001000 at 0x140001000 bytes 0x9b5f3c8e2a1d4f70
//!   machine tested 12
//!   promoted traced
//!   c none
//! ```
//!
//! Functions go by address and steps by name, both in order. A blank line and a line that starts
//! with `#` say nothing; the indentation is for the reader.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::vector::{Error, hex64, word};

/// The line every ledger file starts with; the number is the format's version.
const HEADER: &str = "ra-asm-verify ledger 1";

/// The ledger's file name in the gen directory.
pub const FILE_NAME: &str = "ledger.txt";

/// What a step is known to do with a function, weakest first, so the derived order is the
/// strength: any test is stronger than a trace, and more vectors are stronger than fewer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Evidence {
    /// Nothing is known.
    #[default]
    None,
    /// The function was seen entered, without a vector replayed against it.
    Traced,
    /// This many vectors replayed equal.
    Tested(u32),
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
        self.functions.get(&address).and_then(|function| function.steps.get(step)).copied().unwrap_or_default()
    }

    /// Notes evidence of a step on a function whose bytes have the digest `bytes`. The stronger
    /// of it and what the ledger held stays; the lines of the function's former bytes go.
    pub fn note(&mut self, address: u64, name: &str, bytes: u64, step: &str, evidence: Evidence) {
        let function = self.functions.entry(address).or_default();
        if function.bytes != bytes || function.name != name {
            *function = Function { name: name.to_owned(), bytes, steps: BTreeMap::new() };
        }
        let held = function.steps.entry(step.to_owned()).or_default();
        *held = (*held).max(evidence);
    }

    /// Takes in the ledger of a later run. Where both hold a function on the same bytes, every
    /// step keeps the stronger evidence; where the bytes differ, the later run's lines stand.
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
                            *kept = (*kept).max(evidence);
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

/// Writes a ledger as the text of its file. A name that is not one word is refused, since it
/// would not read back as itself.
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
                Evidence::Tested(count) => writeln!(out, "  {step} tested {count}"),
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
                    ["tested", count] => {
                        Evidence::Tested(count.parse().map_err(|_| Error::at(line, format!("`{count}` is not a count of vectors")))?)
                    }
                    _ => return Err(Error::at(line, format!("expected `{step} none`, `{step} traced` or `{step} tested <count>`"))),
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
