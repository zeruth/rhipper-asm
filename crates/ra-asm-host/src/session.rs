//! A session: the events of a run at the platform boundary, in the order the program made them.
//! Each is one host call (an import, or a `ra_*` function that answers for the system) with its
//! arguments, its answer and the bytes it left in the program's memory (see [`HostCall`]).
//!
//! The text has one call to a line, its writes on the lines after it:
//!
//! ```text
//! ra-session 1
//! call GetStdHandle 0xfffffff5 = 0x54
//! call WriteFile 0x54 0x14000a000 0xd 0x14ff20 0x0 = 0x1
//!   write 0x14ff20 0d000000
//! call ExitProcess 0x0 ! exit 0
//! ```
//!
//! A value is an integer in hex, `f32:` or `f64:` with the float's bits in hex, a vector `<..>` or
//! an aggregate `{..}` of values with commas between them. A call that stopped the run answers
//! with `!` and the stop: `exit <code>`, `limit <instructions>` or `fault <message>`, the message
//! with `\\`, `\n` and `\r` escaped. A write takes a line per 64 bytes, and adjacent lines read
//! back as one run.

use std::collections::HashMap;
use std::fmt::Write as _;

use ra_asm_ir::interp::{self, Stop};
use ra_asm_lift::Lifted;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_pe::Image;

use crate::{HostCall, Options, Outcome};

/// The first line of a session's text.
const HEADER: &str = "ra-session 1";
/// The bytes of a write line.
const LINE_BYTES: usize = 64;

/// A value at the boundary, a float by its bits so that a session equals itself read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(u128),
    F32(u32),
    F64(u64),
    Vector(Vec<Value>),
    Aggregate(Vec<Value>),
}

impl From<&interp::Value> for Value {
    fn from(value: &interp::Value) -> Self {
        match value {
            interp::Value::Int(n) => Self::Int(*n),
            interp::Value::F32(f) => Self::F32(f.to_bits()),
            interp::Value::F64(f) => Self::F64(f.to_bits()),
            interp::Value::Vector(items) => Self::Vector(items.iter().map(Self::from).collect()),
            interp::Value::Aggregate(items) => Self::Aggregate(items.iter().map(Self::from).collect()),
        }
    }
}

/// What a call answered: a value, or the stop it ended the run with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Value(Value),
    Exit(i32),
    Limit(u64),
    Fault(String),
}

impl From<&Result<interp::Value, Stop>> for Answer {
    fn from(result: &Result<interp::Value, Stop>) -> Self {
        match result {
            Ok(value) => Self::Value(value.into()),
            Err(Stop::Exit(code)) => Self::Exit(*code),
            Err(Stop::Limit(n)) => Self::Limit(*n),
            Err(Stop::Fault(message)) => Self::Fault(message.clone()),
        }
    }
}

/// One host call of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub args: Vec<Value>,
    pub answer: Answer,
    /// The bytes the call wrote, as runs of adjacent addresses in address order.
    pub writes: Vec<(u64, Vec<u8>)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    pub events: Vec<Event>,
}

/// A line of a session's text that does not read, by its number from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

impl Session {
    /// The session of the calls a run kept (its outcome's `boundary`).
    #[must_use]
    pub fn of(calls: &[HostCall]) -> Self {
        let events = calls
            .iter()
            .map(|call| Event {
                name: call.name.clone(),
                args: call.args.iter().map(Value::from).collect(),
                answer: (&call.result).into(),
                writes: call.writes.clone(),
            })
            .collect();
        Self { events }
    }

    /// The session's text.
    #[must_use]
    pub fn write(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        for event in &self.events {
            out.push_str("call ");
            out.push_str(&event.name);
            for arg in &event.args {
                out.push(' ');
                put(&mut out, arg);
            }
            match &event.answer {
                Answer::Value(value) => {
                    out.push_str(" = ");
                    put(&mut out, value);
                }
                Answer::Exit(code) => _ = write!(out, " ! exit {code}"),
                Answer::Limit(n) => _ = write!(out, " ! limit {n}"),
                Answer::Fault(message) => _ = write!(out, " ! fault {}", escape(message)),
            }
            out.push('\n');
            for (start, run) in &event.writes {
                for (address, chunk) in (0..).step_by(LINE_BYTES).map(|at: u64| start + at).zip(run.chunks(LINE_BYTES)) {
                    _ = write!(out, "  write {address:#x} ");
                    for byte in chunk {
                        _ = write!(out, "{byte:02x}");
                    }
                    out.push('\n');
                }
            }
        }
        out
    }

    /// The session of `text`, as [`Session::write`] makes it; a line ending in CRLF reads as one
    /// ending in LF.
    ///
    /// # Errors
    /// The first line that is not the header, a call or a write after a call.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut lines = text.lines().enumerate().map(|(index, line)| (index + 1, line.trim_end_matches('\r')));
        match lines.next() {
            Some((_, HEADER)) => {}
            Some((line, other)) => return Err(Error { line, message: format!("`{other}` is not the header `{HEADER}`") }),
            None => return Err(Error { line: 1, message: "an empty session".to_owned() }),
        }
        let mut events: Vec<Event> = Vec::new();
        for (line, text) in lines {
            let error = |message: String| Error { line, message };
            if text.is_empty() {
                continue;
            }
            if let Some(rest) = text.strip_prefix("  write ") {
                let event = events.last_mut().ok_or_else(|| error("a write before the first call".to_owned()))?;
                let (address, bytes) = rest.split_once(' ').ok_or_else(|| error(format!("`{rest}` is not `<address> <bytes>`")))?;
                let address = address
                    .strip_prefix("0x")
                    .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                    .ok_or_else(|| error(format!("`{address}` is no address")))?;
                let bytes = unhex(bytes).ok_or_else(|| error(format!("`{bytes}` is not bytes in hex")))?;
                match event.writes.last_mut() {
                    Some((start, run)) if start.checked_add(run.len() as u64) == Some(address) => run.extend(bytes),
                    _ => event.writes.push((address, bytes)),
                }
            } else if let Some(rest) = text.strip_prefix("call ") {
                events.push(call(rest).map_err(error)?);
            } else {
                return Err(error(format!("`{text}` is neither a call nor a write")));
            }
        }
        Ok(Self { events })
    }
}

/// A call line after its `call `.
fn call(text: &str) -> Result<Event, String> {
    let at = [" = ", " ! "].into_iter().filter_map(|separator| text.find(separator)).min();
    let at = at.ok_or_else(|| format!("`{text}` has no answer"))?;
    let (head, tail) = (&text[..at], &text[at + 3..]);
    let mut words = head.split(' ');
    let name = words.next().filter(|name| !name.is_empty()).ok_or_else(|| format!("`{text}` names no function"))?;
    let args = words.map(|word| whole(word).ok_or_else(|| format!("`{word}` is no value"))).collect::<Result<Vec<_>, _>>()?;
    let answer = if text[at..].starts_with(" = ") {
        Answer::Value(whole(tail).ok_or_else(|| format!("`{tail}` is no value"))?)
    } else if let Some(code) = tail.strip_prefix("exit ") {
        Answer::Exit(code.parse().map_err(|_| format!("`{code}` is no exit code"))?)
    } else if let Some(n) = tail.strip_prefix("limit ") {
        Answer::Limit(n.parse().map_err(|_| format!("`{n}` is no instruction count"))?)
    } else if let Some(message) = tail.strip_prefix("fault ") {
        Answer::Fault(unescape(message).ok_or_else(|| format!("`{message}` has an escape that is not `\\\\`, `\\n` or `\\r`"))?)
    } else {
        return Err(format!("`{tail}` is no stop"));
    };
    Ok(Event { name: name.to_owned(), args, answer, writes: Vec::new() })
}

fn put(out: &mut String, value: &Value) {
    match value {
        Value::Int(n) => _ = write!(out, "{n:#x}"),
        Value::F32(bits) => _ = write!(out, "f32:{bits:x}"),
        Value::F64(bits) => _ = write!(out, "f64:{bits:x}"),
        Value::Vector(items) | Value::Aggregate(items) => {
            let (open, close) = if matches!(value, Value::Vector(_)) { ('<', '>') } else { ('{', '}') };
            out.push(open);
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                put(out, item);
            }
            out.push(close);
        }
    }
}

/// The value that is the whole of `text`.
fn whole(text: &str) -> Option<Value> {
    value(text).and_then(|(value, rest)| rest.is_empty().then_some(value))
}

/// The value `text` starts with, and the text after it.
fn value(text: &str) -> Option<(Value, &str)> {
    type ValueConstructor = (char, char, fn(Vec<Value>) -> Value);
    let lists: [ValueConstructor; 2] = [('<', '>', Value::Vector), ('{', '}', Value::Aggregate)];
    for (open, close, make) in lists {
        let Some(mut rest) = text.strip_prefix(open) else {
            continue;
        };
        let mut items = Vec::new();
        if let Some(after) = rest.strip_prefix(close) {
            return Some((make(items), after));
        }
        loop {
            let (item, after) = value(rest)?;
            items.push(item);
            if let Some(after) = after.strip_prefix(',') {
                rest = after;
            } else {
                return after.strip_prefix(close).map(|after| (make(items), after));
            }
        }
    }
    let digits = |rest: &str| rest.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(rest.len());
    if let Some(rest) = text.strip_prefix("f32:") {
        let n = digits(rest);
        return u32::from_str_radix(&rest[..n], 16).ok().map(|bits| (Value::F32(bits), &rest[n..]));
    }
    if let Some(rest) = text.strip_prefix("f64:") {
        let n = digits(rest);
        return u64::from_str_radix(&rest[..n], 16).ok().map(|bits| (Value::F64(bits), &rest[n..]));
    }
    let rest = text.strip_prefix("0x")?;
    let n = digits(rest);
    u128::from_str_radix(&rest[..n], 16).ok().map(|v| (Value::Int(v), &rest[n..]))
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok()).collect()
}

fn escape(message: &str) -> String {
    message.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r")
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    Some(out)
}

/// Runs `lifted` as [`crate::run`] does, keeping every host call of the run, and gives the
/// session with the outcome.
///
/// # Errors
/// What stops [`crate::run`] from running the program at all.
pub fn record(
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    options: &Options,
) -> anyhow::Result<(Session, Outcome)> {
    let options = Options { session: true, ..options.clone() };
    let outcome = crate::run(image, lifted, signatures, &options)?;
    Ok((Session::of(&outcome.boundary), outcome))
}

#[cfg(test)]
mod tests;
