//! The instruction vector file every machine's instruction tests take: one case a line, in
//! hexadecimal text, with no knowledge of any machine. A file holds the cases of one mnemonic
//! group; a line is
//!
//! ```text
//! <bytes> <in>... -> <out>...
//! ```
//!
//! where `<bytes>` is the instruction's encoding as contiguous hex, each `<in>` and `<out>` is a
//! register `name=hex` or a memory cell `[address]=hex`, and `<out>` may instead be the one word
//! `fault=<kind>` when the instruction is expected to fault. A register not listed in `<in>` is
//! zero; a register not listed in `<out>` is unchanged, so the writer drops a zero register from
//! the input and [`changed`] computes an output from the states before and after. A value keeps
//! its width as a number of bytes, so a 128-bit register is 32 digits. A blank line or one that
//! starts with `#` is a comment, as is the rest of a line after a `#` token; anything else is
//! refused with its line number.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

/// A register or memory value: its bytes, the most significant first, as many as the register or
/// cell is wide.
pub type Value = Vec<u8>;

/// The registers and memory cells of one side of a case; what is absent is zero on the input side
/// and unchanged on the output side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub registers: BTreeMap<String, Value>,
    pub memory: BTreeMap<u64, Value>,
}

/// What the instruction is expected to do: leave a state, or fault with a kind the machine names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    State(State),
    Fault(String),
}

/// One line of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub bytes: Vec<u8>,
    pub input: State,
    pub outcome: Outcome,
}

/// What stops a file from being read: the line and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

impl State {
    /// Whether the state names nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registers.is_empty() && self.memory.is_empty()
    }
}

/// The output side of a case from the states before and after the instruction: the registers and
/// cells of `after` whose value differs from `before`'s, where a register `before` lacks is zero.
#[must_use]
pub fn changed(before: &State, after: &State) -> State {
    let mut state = State::default();
    for (name, value) in &after.registers {
        let same = before.registers.get(name).map_or_else(|| value.iter().all(|&b| b == 0), |old| old == value);
        if !same {
            state.registers.insert(name.clone(), value.clone());
        }
    }
    for (address, value) in &after.memory {
        if before.memory.get(address) != Some(value) {
            state.memory.insert(*address, value.clone());
        }
    }
    state
}

/// The cases of the file text, in order.
///
/// # Errors
/// The first line that is not a case, a comment or blank: no `->`, bytes or a value that are not
/// hex, a register name that is not an identifier, an address wider than 64 bits, a register or
/// cell given twice on one side, or a `fault=` beside other outputs.
pub fn parse(text: &str) -> Result<Vec<Case>, Error> {
    let mut cases = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        cases.push(case(index + 1, line)?);
    }
    Ok(cases)
}

/// The file text of `cases`, one a line, as [`parse`] reads it back: registers in name order,
/// cells in address order, a zero register left out of the input.
#[must_use]
pub fn write(cases: &[Case]) -> String {
    let mut text = String::new();
    for case in cases {
        text.push_str(&hex(&case.bytes));
        let input = State {
            registers: case
                .input
                .registers
                .iter()
                .filter(|(_, v)| v.iter().any(|&b| b != 0))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            memory: case.input.memory.clone(),
        };
        push_state(&mut text, &input);
        text.push_str(" ->");
        match &case.outcome {
            Outcome::State(state) => push_state(&mut text, state),
            Outcome::Fault(kind) => {
                text.push_str(" fault=");
                text.push_str(kind);
            }
        }
        text.push('\n');
    }
    text
}

fn push_state(text: &mut String, state: &State) {
    for (name, value) in &state.registers {
        let _ = write!(text, " {name}={}", hex(value));
    }
    for (address, value) in &state.memory {
        let _ = write!(text, " [{address:x}]={}", hex(value));
    }
}

/// `bytes` as lowercase hex, two digits a byte.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The bytes of a hex text, the most significant first; an odd digit count gets a leading zero.
/// `None` when the text is empty or has a character that is not a hex digit.
#[must_use]
pub fn unhex(digits: &str) -> Option<Value> {
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let padded = if digits.len() % 2 == 1 { format!("0{digits}") } else { digits.to_owned() };
    padded.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()).collect()
}

fn case(number: usize, line: &str) -> Result<Case, Error> {
    let error = |message: String| Error { line: number, message };
    let mut tokens = line.split_whitespace().take_while(|token| *token != "#");
    let bytes = tokens.next().ok_or_else(|| error("a case without bytes".to_owned()))?;
    if bytes.len() % 2 == 1 {
        return Err(error(format!("the bytes `{bytes}` have an odd number of digits")));
    }
    let bytes = unhex(bytes).ok_or_else(|| error(format!("the bytes `{bytes}` are not hex")))?;
    let mut input = State::default();
    let mut arrow = false;
    for token in tokens.by_ref() {
        if token == "->" {
            arrow = true;
            break;
        }
        assign(number, &mut input, token)?;
    }
    if !arrow {
        return Err(error("no `->` between the input and the output".to_owned()));
    }
    let mut output = State::default();
    let mut fault = None;
    for token in tokens {
        if let Some(kind) = token.strip_prefix("fault=") {
            if kind.is_empty() {
                return Err(error("`fault=` without a kind".to_owned()));
            }
            if fault.replace(kind.to_owned()).is_some() {
                return Err(error("two faults in one case".to_owned()));
            }
            continue;
        }
        assign(number, &mut output, token)?;
    }
    let outcome = match fault {
        Some(kind) if output.is_empty() => Outcome::Fault(kind),
        Some(kind) => return Err(error(format!("`fault={kind}` beside other outputs"))),
        None => Outcome::State(output),
    };
    Ok(Case { bytes, input, outcome })
}

/// `name=hex` or `[address]=hex` into `state`.
fn assign(number: usize, state: &mut State, token: &str) -> Result<(), Error> {
    let error = |message: String| Error { line: number, message };
    let (target, digits) = token.split_once('=').ok_or_else(|| error(format!("`{token}` is not a `name=hex` or `[address]=hex`")))?;
    let value = unhex(digits).ok_or_else(|| error(format!("the value of `{target}` is not hex: `{digits}`")))?;
    if let Some(address) = target.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        let address = (!address.is_empty() && address.len() <= 16)
            .then(|| u64::from_str_radix(address, 16).ok())
            .flatten()
            .ok_or_else(|| error(format!("`{address}` is not an address of up to 16 hex digits")))?;
        if state.memory.insert(address, value).is_some() {
            return Err(error(format!("the cell at {address:x} is given twice")));
        }
        return Ok(());
    }
    let mut chars = target.chars();
    let identifier = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if !identifier {
        return Err(error(format!("`{target}` is not a register name")));
    }
    if state.registers.insert(target.to_owned(), value).is_some() {
        return Err(error(format!("`{target}` is given twice")));
    }
    Ok(())
}
