//! `ras functions` and `ras cfg`: the functions of an image, and the blocks of one.

use std::io::Write;

use anyhow::{Result, bail};
use ra_asm_analysis::{Analysis, Kind, Origin, Terminator, analyze};
use ra_asm_pe::Image;
use serde_json::json;

use crate::disasm::note;
use crate::report::hex;

fn origin_name(origin: &Origin) -> String {
    match origin {
        Origin::Entry => "entry".to_owned(),
        Origin::Export(name) => format!("export {name}"),
        Origin::TlsCallback => "tls callback".to_owned(),
        Origin::ExceptionTable => "pdata".to_owned(),
        Origin::Call(from) => format!("call from {}", hex(*from)),
        Origin::TailCall(from) => format!("jump from {}", hex(*from)),
        Origin::Pointer(slot) => format!("pointer at {}", hex(*slot)),
        Origin::Vtable(vtable) => format!("vtable at {}", hex(*vtable)),
    }
}

/// The classes of the analysis as JSON: name, bases, and each vtable with its functions by name.
pub(crate) fn classes_json(analysis: &Analysis<'_>) -> serde_json::Value {
    let rows: Vec<_> = analysis
        .classes
        .iter()
        .map(|c| {
            json!({
                "name": c.name, "type_descriptor": hex(c.type_descriptor), "bases": c.bases,
                "vtables": c.vtables.iter().map(|v| json!({
                    "addr": hex(v.address), "offset": v.offset, "base": v.base,
                    "functions": v.functions.iter().map(|f| json!({"addr": hex(*f), "name": analysis.name_of(*f)})).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

/// The facts of one function the table and the server share: its strings and its vtable slots.
pub(crate) fn facts_json(f: &ra_asm_analysis::Function) -> (serde_json::Value, serde_json::Value) {
    let strings = f.strings.iter().map(|(va, text)| json!({"addr": hex(*va), "text": text})).collect::<Vec<_>>();
    let virtuals = f.virtuals.iter().map(|v| json!({"class": v.class, "offset": v.offset, "slot": v.slot})).collect::<Vec<_>>();
    (serde_json::Value::Array(strings), serde_json::Value::Array(virtuals))
}

fn terminator_json(t: &Terminator) -> serde_json::Value {
    match t {
        Terminator::FallThrough(to) => json!({"kind": "fall_through", "to": hex(*to)}),
        Terminator::Jump(to) => json!({"kind": "jump", "to": hex(*to)}),
        Terminator::CondJump { taken, fallthrough } => json!({"kind": "cond_jump", "taken": hex(*taken), "fallthrough": hex(*fallthrough)}),
        Terminator::Switch { table, targets, index, cases, .. } => json!({
            "kind": "switch", "table": hex(*table), "index": index.name, "targets": targets.iter().map(|t| hex(*t)).collect::<Vec<_>>(),
            "cases": cases.iter().map(|(i, t)| json!({"index": i, "target": hex(*t)})).collect::<Vec<_>>(),
        }),
        Terminator::IndirectJump => json!({"kind": "indirect_jump"}),
        Terminator::TailCall(to) => json!({"kind": "tail_call", "to": hex(*to)}),
        Terminator::ImportTailCall(import) => json!({"kind": "import_tail_call", "import": import}),
        Terminator::NoReturnCall(to) => json!({"kind": "noreturn_call", "to": hex(*to)}),
        Terminator::NoReturnImportCall(import) => json!({"kind": "noreturn_import_call", "import": import}),
        Terminator::Return => json!({"kind": "return"}),
        Terminator::Trap => json!({"kind": "trap"}),
        Terminator::Halt => json!({"kind": "halt"}),
        Terminator::Invalid => json!({"kind": "invalid"}),
    }
}

fn terminator_text(analysis: &Analysis<'_>, t: &Terminator) -> String {
    match t {
        Terminator::FallThrough(to) => format!("falls through to {}", hex(*to)),
        Terminator::Jump(to) => format!("jumps to {}", hex(*to)),
        Terminator::CondJump { taken, fallthrough } => format!("branches to {} or {}", hex(*taken), hex(*fallthrough)),
        Terminator::Switch { table, targets, index, .. } => {
            format!(
                "switch on {} over the table at {} to {}",
                index.name,
                hex(*table),
                targets.iter().map(|t| hex(*t)).collect::<Vec<_>>().join(", ")
            )
        }
        Terminator::IndirectJump => "jumps indirectly (unresolved)".to_owned(),
        Terminator::TailCall(to) => format!("tail-calls {}", analysis.name_of(*to)),
        Terminator::ImportTailCall(import) => format!("tail-calls import {import}"),
        Terminator::NoReturnCall(to) => format!("calls {}, which never returns", analysis.name_of(*to)),
        Terminator::NoReturnImportCall(import) => format!("calls import {import}, which never returns"),
        Terminator::Return => "returns".to_owned(),
        Terminator::Trap => "traps".to_owned(),
        Terminator::Halt => "halts".to_owned(),
        Terminator::Invalid => "ends in bytes that are no instruction".to_owned(),
    }
}

/// Every function of the analysis as JSON: what `ras functions --json` prints and `ras serve` answers.
pub(crate) fn functions_json(analysis: &Analysis<'_>) -> serde_json::Value {
    let rows: Vec<_> = analysis
        .functions
        .values()
        .map(|f| {
            let (strings, virtuals) = facts_json(f);
            json!({
                "addr": hex(f.entry), "name": f.name, "size": f.size(), "bytes": f.code_bytes(), "blocks": f.blocks.len(),
                "instrs": f.instr_count(), "noreturn": f.noreturn,
                "thunk": match &f.kind { Kind::ImportThunk(i) => Some(i.clone()), Kind::Normal => None },
                "calls": f.calls.iter().map(|c| hex(*c)).collect::<Vec<_>>(), "imports": f.import_calls,
                "tail_calls": f.tail_calls.iter().map(|c| hex(*c)).collect::<Vec<_>>(),
                "origins": f.origins.iter().map(origin_name).collect::<Vec<_>>(),
                "strings": strings, "virtuals": virtuals,
            })
        })
        .collect();
    json!({"functions": rows, "classes": classes_json(analysis), "warnings": analysis.warnings})
}

/// The blocks of `function` as JSON: what `ras cfg --json` prints and `ras serve` answers.
pub(crate) fn cfg_json(image: &Image, function: &ra_asm_analysis::Function) -> serde_json::Value {
    let blocks: Vec<_> = function
        .blocks
        .iter()
        .map(|b| {
            json!({
                "start": hex(b.start), "end": hex(b.end),
                "instrs": b.instrs.iter().map(|i| json!({"address": hex(i.address), "text": i.text, "note": note(image, i)})).collect::<Vec<_>>(),
                "terminator": terminator_json(&b.terminator), "successors": b.successors().iter().map(|s| hex(*s)).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({"addr": hex(function.entry), "name": function.name, "size": function.size(), "noreturn": function.noreturn, "blocks": blocks})
}

pub(crate) fn functions(image: &Image, json: bool) -> Result<()> {
    let analysis = analyze(image);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &functions_json(&analysis))?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(out, "{:<18} {:>6} {:>6} {:>6}  name", "address", "size", "blocks", "calls")?;
    for f in analysis.functions.values() {
        let mut notes = Vec::new();
        if f.noreturn {
            notes.push("never returns".to_owned());
        }
        if let Some(origin) = f.origins.first() {
            notes.push(origin_name(origin));
        }
        let notes = if notes.is_empty() { String::new() } else { format!("  ({})", notes.join(", ")) };
        writeln!(
            out,
            "{:<18} {:>6} {:>6} {:>6}  {}{notes}",
            hex(f.entry),
            f.size(),
            f.blocks.len(),
            f.calls.len() + f.import_calls.len(),
            f.name
        )?;
    }
    writeln!(out, "{} functions", analysis.functions.len())?;
    for warning in &analysis.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

pub(crate) fn cfg(image: &Image, address: u64, json: bool) -> Result<()> {
    let analysis = analyze(image);
    let Some(function) = analysis.function(address).or_else(|| analysis.containing(address)) else {
        bail!("no function holds {}", hex(address));
    };
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &cfg_json(image, function))?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(
        out,
        "{} {} ({} bytes, {} blocks){}",
        function.name,
        hex(function.entry),
        function.size(),
        function.blocks.len(),
        if function.noreturn { ", never returns" } else { "" }
    )?;
    for b in &function.blocks {
        let predecessors: Vec<String> =
            function.blocks.iter().filter(|p| p.successors().contains(&b.start)).map(|p| hex(p.start)).collect();
        writeln!(out)?;
        writeln!(
            out,
            "block {}{}",
            hex(b.start),
            if predecessors.is_empty() { String::new() } else { format!("  <- {}", predecessors.join(", ")) }
        )?;
        for i in &b.instrs {
            let note = note(image, i).filter(|n| !(n.starts_with("0x") && !n.contains(' ') && i.text.contains(n.as_str())));
            let note = note.map(|n| format!("   ; {n}")).unwrap_or_default();
            writeln!(out, "  {}  {}{note}", hex(i.address), i.text)?;
        }
        writeln!(out, "  -> {}", terminator_text(&analysis, &b.terminator))?;
    }
    Ok(())
}
