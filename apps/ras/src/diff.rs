//! `ras diff`: two builds of a program matched function by function (see
//! [`ra_asm_analysis::matching`]), the changed functions ranked by how much of their code changed,
//! with the diff of their raised IR on request, and the functions one build has alone: a patch
//! reads as the functions it touched. `ras serve` answers the same JSON for the workbench.
//!
//! `ras diff --sessions` compares two sessions of a program instead (the original's and the
//! port's): equal, or the first event that differs with the function that made it on each side.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_analysis::matching::{self, Pair};
use serde_json::{Value, json};

use crate::report::hex;

/// What `ras diff` was asked for.
pub(crate) struct Options {
    /// Match by the builds' symbols first; without, by structure alone.
    pub symbols: bool,
    /// The lines of context of the IR diff of each changed function; no IR without it.
    pub ir: Option<usize>,
    pub json: bool,
}

pub(crate) fn diff(old: &ra_asm_pe::Image, new: &ra_asm_pe::Image, options: &Options) -> Result<()> {
    let (old, new) = (ra_asm_analysis::analyze(old), ra_asm_analysis::analyze(new));
    let value = diff_json(&old, &new, options.symbols, options.ir);
    if options.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    print!("{}", text(&value));
    Ok(())
}

/// The diff of `old` and `new` as JSON: the counts, every pair (the most changed first), the
/// functions of each build alone, and with `ir` the unified diff of the raised IR of each changed
/// pair with that many lines of context.
pub(crate) fn diff_json(old: &Analysis<'_>, new: &Analysis<'_>, symbols: bool, ir: Option<usize>) -> Value {
    let matching = matching::match_functions(old, new, symbols);
    let texts = ir.map(|_| (ir_of(old), ir_of(new)));
    let renames: HashMap<u64, u64> = matching.pairs.iter().map(|p| (p.old, p.new)).collect();
    let pair = |p: &Pair| {
        let mut value = json!({
            "old": hex(p.old),
            "new": hex(p.new),
            "old_name": old.name_of(p.old),
            "new_name": new.name_of(p.new),
            "old_symbol": old.function(p.old).and_then(|f| f.symbol.clone()),
            "new_symbol": new.function(p.new).and_then(|f| f.symbol.clone()),
            "how": p.how,
            "round": p.round,
            "change": p.change,
        });
        if let (Some(context), Some((a, b))) = (ir, &texts)
            && p.change > 0
        {
            let before = a.get(&p.old).map(|t| renamed(t, &renames)).unwrap_or_default();
            let after = b.get(&p.new).cloned().unwrap_or_default();
            value["ir"] = json!(ra_asm_ir::diff::unified(&before, &after, context));
        }
        value
    };
    let alone = |analysis: &Analysis<'_>, va: u64| json!({"address": hex(va), "name": analysis.name_of(va), "symbol": analysis.function(va).and_then(|f| f.symbol.clone())});
    json!({
        "old_functions": old.functions.len(),
        "new_functions": new.functions.len(),
        "matched": matching.pairs.len(),
        "changed": matching.changed().count(),
        "pairs": matching.pairs.iter().map(pair).collect::<Vec<_>>(),
        "removed": matching.removed.iter().map(|&va| alone(old, va)).collect::<Vec<_>>(),
        "added": matching.added.iter().map(|&va| alone(new, va)).collect::<Vec<_>>(),
    })
}

/// The report for a person: the counts, the changed pairs with their IR diffs, and the functions
/// of one build alone.
fn text(value: &Value) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} functions in the old build, {} in the new: {} matched, {} changed, {} removed, {} added",
        value["old_functions"],
        value["new_functions"],
        value["matched"],
        value["changed"],
        value["removed"].as_array().map_or(0, Vec::len),
        value["added"].as_array().map_or(0, Vec::len)
    );
    let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    for p in value["pairs"].as_array().into_iter().flatten().filter(|p| p["change"].as_u64() > Some(0)) {
        let change = p["change"].as_u64().unwrap_or(0);
        let _ = writeln!(
            out,
            "{change:>3}%  {} {} -> {} {}  ({})",
            s(&p["old"]),
            s(&p["old_name"]),
            s(&p["new"]),
            s(&p["new_name"]),
            s(&p["how"])
        );
        for line in p["ir"].as_str().unwrap_or_default().lines() {
            let _ = writeln!(out, "      {line}");
        }
    }
    for (key, label) in [("removed", "only in the old build"), ("added", "only in the new build")] {
        for f in value[key].as_array().into_iter().flatten() {
            let _ = writeln!(out, "{label}: {} {}", s(&f["address"]), s(&f["name"]));
        }
    }
    out
}

/// The raised IR of every lifted function of `analysis`, by entry address, under the lift's
/// address names so that the two builds' texts compare once the old names are moved to the new.
fn ir_of(analysis: &Analysis<'_>) -> BTreeMap<u64, String> {
    let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(analysis.image, analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.functions.iter().filter_map(|(&va, name)| lifted.module.function(name).map(|f| (va, ra_asm_ir::printer::function(f)))).collect()
}

/// `text` with every `F_<address>` of an old function under the address of the new function it
/// matched, so a call to an unchanged callee reads the same in both builds.
fn renamed(text: &str, renames: &HashMap<u64, u64>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("F_") {
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let digits = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        match u64::from_str_radix(&rest[..digits], 16).ok().and_then(|va| renames.get(&va)) {
            Some(new) if digits >= 8 => {
                let _ = write!(out, "{new:08x}");
            }
            _ => out.push_str(&rest[..digits]),
        }
        rest = &rest[digits..];
    }
    out.push_str(rest);
    out
}

/// The first lines of the sessions `ras diff --sessions` reads: the interpreter's and a port's.
const SESSION_HEADERS: [&str; 2] = ["ra-session 1", "ra-port-session 1"];

/// One event of a session: a call, a window procedure called back or its return, with the
/// write lines after it and the function that made it.
struct Event<'a> {
    line: usize,
    text: &'a str,
    writes: Vec<&'a str>,
    /// The program, or the window procedure whose callback the event falls in: a session records
    /// no call site, so the procedure is the nearest function both sides name alike.
    caller: String,
}

impl Event<'_> {
    fn json(&self) -> Value {
        json!({"line": self.line, "event": self.text, "writes": self.writes, "caller": self.caller})
    }
}

/// Compares the sessions at `a` and `b` and prints `equal, N events` or the first event that
/// differs on each side.
pub(crate) fn sessions(a: &Path, b: &Path, json: bool) -> Result<()> {
    let read = |path: &Path| std::fs::read_to_string(path).with_context(|| format!("cannot read the session {}", path.display()));
    let (a_text, b_text) = (read(a)?, read(b)?);
    let ((a_header, a_events), (b_header, b_events)) = (events_of(a, &a_text)?, events_of(b, &b_text)?);
    if a_header != b_header {
        bail!("{} is a `{a_header}` and {} a `{b_header}`: only sessions of one kind compare", a.display(), b.display());
    }
    let value = sessions_json(&a_events, &b_events);
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    print!("{}", sessions_text(&value, (&a.display().to_string(), &b.display().to_string())));
    Ok(())
}

/// The header and the events of a session's `text`, a line ending in CRLF read as one in LF.
fn events_of<'a>(path: &Path, text: &'a str) -> Result<(&'a str, Vec<Event<'a>>)> {
    let mut lines = text.lines().enumerate().map(|(index, line)| (index + 1, line.trim_end_matches('\r')));
    let Some(header) = lines.next().map(|(_, line)| line).filter(|line| SESSION_HEADERS.contains(line)) else {
        bail!("{}: the first line is not `{}`", path.display(), SESSION_HEADERS.join("` or `"));
    };
    let mut events: Vec<Event<'a>> = Vec::new();
    let mut procedures: Vec<&str> = Vec::new();
    for (line, text) in lines {
        if text.is_empty() {
            continue;
        }
        if let Some(write) = text.strip_prefix("  write ") {
            let Some(event) = events.last_mut() else {
                bail!("{}, line {line}: a write before the first event", path.display());
            };
            event.writes.push(write);
            continue;
        }
        let caller = procedures.last().map_or_else(|| "the program".to_owned(), |procedure| format!("the window procedure {procedure}"));
        if let Some(rest) = text.strip_prefix("back ") {
            procedures.push(rest.split(' ').next().unwrap_or_default());
        } else if text == "ret" || text.starts_with("ret ") {
            if procedures.pop().is_none() {
                bail!("{}, line {line}: a return outside any callback", path.display());
            }
        } else if !text.starts_with("call ") {
            bail!("{}, line {line}: `{text}` is neither a call, a write, a callback nor a return", path.display());
        }
        events.push(Event { line, text, writes: Vec::new(), caller });
    }
    Ok((header, events))
}

/// `equal` with the count of events, or the number from one of the first event whose line or
/// writes differ and that event on each side (none past a side's last).
fn sessions_json(a: &[Event<'_>], b: &[Event<'_>]) -> Value {
    let same = |i: usize| match (a.get(i), b.get(i)) {
        (Some(a), Some(b)) => a.text == b.text && a.writes == b.writes,
        _ => false,
    };
    match (0..a.len().max(b.len())).find(|&i| !same(i)) {
        None => json!({"equal": true, "events": a.len()}),
        Some(i) => json!({
            "equal": false,
            "event": i + 1,
            "a": a.get(i).map(Event::json),
            "b": b.get(i).map(Event::json),
            "a_events": a.len(),
            "b_events": b.len(),
        }),
    }
}

/// The comparison for a person, each side under its file's name.
fn sessions_text(value: &Value, names: (&str, &str)) -> String {
    if value["equal"] == true {
        return format!("equal, {} events\n", value["events"]);
    }
    let mut out = String::new();
    let _ = writeln!(out, "event {} differs", value["event"]);
    for (side, name) in [("a", names.0), ("b", names.1)] {
        let event = &value[side];
        if event.is_null() {
            let _ = writeln!(out, "  {name}: past its last event, of {}", value[format!("{side}_events")]);
            continue;
        }
        let s = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let _ = writeln!(out, "  {name}, line {}, in {}: {}", event["line"], s(&event["caller"]), s(&event["event"]));
        for write in event["writes"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "      write {}", s(write));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_name_takes_the_address_of_its_match() {
        let renames = HashMap::from([(0x401000, 0x402000)]);
        assert_eq!(renamed("call @F_00401000(F_00401010) F_", &renames), "call @F_00402000(F_00401010) F_");
    }
}
