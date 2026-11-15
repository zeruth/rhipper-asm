//! A session played back: a run with no host. The replay stands in for the host (see
//! [`Answerer`]): each call the program makes across the platform boundary must be the session's
//! next event, by its name and its arguments, and the host's part of the call is answered with
//! what the event answered and the bytes it wrote. The runtime answers the rest as it does in any
//! run, so the program runs as it ran when it was recorded, on a machine with no Windows DLLs.
//!
//! The first call that is not the session's next event stops the run, and the replay names the
//! event by its number from one, the call the program made instead and the lifted function that
//! made it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use ra_asm_ir::interp::{self, Memory, Observer, Stop};
use ra_asm_lift::Lifted;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_pe::Image;

use crate::session::{self, Answer, Event, Session, Value};
use crate::{Answerer, Answers, Options, Outcome, Watch};

/// The first call of a replay that the session does not hold where the program made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// The event's number in the session, from one.
    pub event: usize,
    /// The lifted function that made the call, when the call came from one.
    pub function: Option<String>,
    pub name: String,
    pub args: Vec<Value>,
    /// What the session holds there; none past its last event.
    pub expected: Option<Event>,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let made = call_text(&self.name, &self.args);
        let by = self.function.as_deref().map_or_else(String::new, |function| format!(" by @{function}"));
        match &self.expected {
            Some(event) => {
                write!(f, "event {}: the call {made}{by}, where the session has {}", self.event, call_text(&event.name, &event.args))
            }
            None => write!(f, "event {}: the call {made}{by}, past the session's last event", self.event),
        }
    }
}

/// What a replay saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The session's events the run made, in order.
    pub replayed: usize,
    /// The session's events.
    pub events: usize,
    /// The first call that differs, when one did.
    pub difference: Option<Difference>,
}

impl Report {
    /// Whether the run made every event of the session and no other call.
    #[must_use]
    pub fn whole(&self) -> bool {
        self.difference.is_none() && self.replayed == self.events
    }
}

/// The stand-in for the host: the session's events, the next one due, and the calls the run has
/// open, innermost last, to name the function a call came from.
struct Replay {
    events: Vec<Event>,
    next: usize,
    open: Vec<String>,
    difference: Option<Difference>,
}

impl Answerer for Replay {
    fn expect(&mut self, name: &str, args: &[interp::Value]) -> Result<usize, Stop> {
        let args: Vec<Value> = args.iter().map(Value::from).collect();
        let index = self.next;
        let expected = self.events.get(index);
        if self.difference.is_none() && expected.is_some_and(|event| event.name == name && event.args == args) {
            self.next += 1;
            return Ok(index);
        }
        let difference = self.difference.get_or_insert_with(|| Difference {
            event: index + 1,
            // The innermost call open is the host's own, entered before it is answered.
            function: self.open.iter().rev().nth(1).cloned(),
            name: name.to_owned(),
            args,
            expected: expected.cloned(),
        });
        Err(Stop::fault(format!("the replay differs at {difference}")))
    }

    fn answer(&mut self, memory: &mut Memory, index: usize) -> Result<interp::Value, Stop> {
        let event = self.events.get(index).ok_or_else(|| Stop::fault(format!("the session has no event {}", index + 1)))?;
        for (address, bytes) in &event.writes {
            memory.write(*address, bytes)?;
        }
        match &event.answer {
            Answer::Value(value) => Ok(value.into()),
            Answer::Exit(code) => Err(Stop::Exit(*code)),
            Answer::Limit(n) => Err(Stop::Limit(*n)),
            Answer::Fault(message) => Err(Stop::Fault(message.clone())),
        }
    }
}

impl Observer for Replay {
    fn enter(&mut self, function: &str, depth: usize) {
        self.open.truncate(depth);
        self.open.push(function.to_owned());
    }

    fn leave(&mut self, _function: &str, depth: usize) {
        self.open.truncate(depth);
    }
}

impl From<&Value> for interp::Value {
    fn from(value: &Value) -> Self {
        match value {
            Value::Int(n) => Self::Int(*n),
            Value::F32(bits) => Self::F32(f32::from_bits(*bits)),
            Value::F64(bits) => Self::F64(f64::from_bits(*bits)),
            Value::Vector(items) => Self::Vector(items.iter().map(Self::from).collect()),
            Value::Aggregate(items) => Self::Aggregate(items.iter().map(Self::from).collect()),
        }
    }
}

/// A call as the session's text writes it, after its `call `.
fn call_text(name: &str, args: &[Value]) -> String {
    let mut out = name.to_owned();
    for arg in args {
        out.push(' ');
        session::put(&mut out, arg);
    }
    out
}

/// Runs `lifted` as [`ra_asm_lift::run::run`] does, with `session` standing in for the host, and
/// gives what the replay saw with the outcome. The options' observer gives way to the replay's,
/// which names the function each call came from.
///
/// # Errors
/// What stops [`ra_asm_lift::run::run`] from running the program at all.
pub fn run(
    image: &Image,
    lifted: &Lifted,
    signatures: &HashMap<String, Signature>,
    session: &Session,
    options: &Options,
) -> anyhow::Result<(Outcome, Report)> {
    let replay = Arc::new(Mutex::new(Replay { events: session.events.clone(), next: 0, open: Vec::new(), difference: None }));
    let options = Options { answers: Some(Answers(replay.clone())), observer: Some(Watch(replay.clone())), ..options.clone() };
    let outcome = ra_asm_lift::run::run(image, lifted, signatures, &options)?;
    let replay = replay.lock().unwrap_or_else(PoisonError::into_inner);
    let report = Report { replayed: replay.next, events: replay.events.len(), difference: replay.difference.clone() };
    Ok((outcome, report))
}

#[cfg(test)]
mod tests;
