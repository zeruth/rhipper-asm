//! `ras callers` and `ras xrefs`: who refers to a function, and what refers to an address range.

use std::io::Write;

use anyhow::Result;
use ra_asm_analysis::{Analysis, analyze};
use ra_asm_arch::{Flow, Instr, Operand};
use ra_asm_pe::Image;
use serde_json::json;

use crate::report::hex;

/// One reference: where it is, what kind it is, and the text of the instruction when it is one.
struct Reference {
    from: u64,
    kind: &'static str,
    /// The function that holds the instruction; none for a pointer in the data.
    function: Option<(u64, String)>,
    text: String,
}

impl Reference {
    fn json(&self) -> serde_json::Value {
        json!({
            "from": hex(self.from),
            "kind": self.kind,
            "function": self.function.as_ref().map(|(_, name)| name),
            "function_addr": self.function.as_ref().map(|(va, _)| hex(*va)),
            "text": self.text,
        })
    }
}

/// Every instruction of the analysis whose operands or flow fall in `[lo, hi)`, and every
/// relocated pointer in the image that points into it.
fn references(image: &Image, analysis: &Analysis<'_>, lo: u64, hi: u64, code_only: bool) -> Vec<Reference> {
    let within = |va: u64| va >= lo && va < hi;
    let mut out = Vec::new();
    for f in analysis.functions.values() {
        for b in &f.blocks {
            for i in &b.instrs {
                let mut kind = None;
                if i.flow.direct_target().is_some_and(within) {
                    kind = Some(if i.flow.is_call() { "call" } else { "jump" });
                } else if !code_only {
                    for o in &i.operands {
                        kind = match o {
                            Operand::Addr(a) if within(*a) => Some("code"),
                            Operand::Mem(m) if m.absolute.is_some_and(within) => Some(if m.bits == 0 { "address" } else { "memory" }),
                            Operand::Imm { value, .. } if within(*value as u64) => Some("immediate"),
                            _ => continue,
                        };
                        break;
                    }
                }
                if let Some(kind) = kind {
                    out.push(Reference { from: i.address, kind, function: Some((f.entry, f.name.clone())), text: i.text.clone() });
                }
            }
        }
    }
    for r in &image.relocations {
        if let Ok(target) = image.pointer(r.va)
            && within(target)
        {
            let section = image.section_of(r.va).map(|s| s.name.clone()).unwrap_or_default();
            out.push(Reference { from: r.va, kind: "pointer", function: None, text: format!("{section}: pointer to {}", hex(target)) });
        }
    }
    out.sort_by_key(|r| r.from);
    out
}

fn print(image: &Image, analysis: &Analysis<'_>, refs: &[Reference], json: bool, heading: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &json!({"references": refs.iter().map(Reference::json).collect::<Vec<_>>()}))?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(out, "{heading}")?;
    for r in refs {
        let holder =
            r.function.as_ref().map_or_else(|| image.section_of(r.from).map(|s| s.name.clone()).unwrap_or_default(), |(_, n)| n.clone());
        writeln!(out, "{:<18} {:<10} {:<28} {}", hex(r.from), r.kind, holder, r.text)?;
    }
    writeln!(out, "{} references", refs.len())?;
    for w in &analysis.warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
}

/// The callers of `callee` as JSON: what `ras callers --json` prints and `ras serve` answers.
pub(crate) fn callers_json(image: &Image, analysis: &Analysis<'_>, callee: u64) -> serde_json::Value {
    let refs = references(image, analysis, callee, callee + 1, true);
    json!({"references": refs.iter().map(Reference::json).collect::<Vec<_>>()})
}

/// The references into `[lo, hi)` as JSON: what `ras xrefs --json` prints and `ras serve` answers.
pub(crate) fn xrefs_json(image: &Image, analysis: &Analysis<'_>, lo: u64, hi: u64) -> serde_json::Value {
    let refs = references(image, analysis, lo, hi, false);
    json!({"references": refs.iter().map(Reference::json).collect::<Vec<_>>()})
}

/// How an instruction touches the constant address of its memory operand: `write` when it stores
/// there, `read` when it only loads, `address` when it takes the address alone (`lea`). `None`
/// without such an operand, for a call or a jump through one (the stack is what they write), and
/// under `fs` or `gs`, which address the thread's own block.
pub(crate) fn data_access(i: &Instr) -> Option<(u64, &'static str)> {
    if i.flow != Flow::Next {
        return None;
    }
    let (index, m) = i.operands.iter().enumerate().find_map(|(n, o)| match o {
        Operand::Mem(m) => Some((n, m)),
        _ => None,
    })?;
    let at = m.absolute?;
    if m.segment.is_some_and(|s| matches!(s.name, "fs" | "gs")) {
        return None;
    }
    if m.bits == 0 {
        return Some((at, "address"));
    }
    // The destination is the first operand, but a push writes the stack with what it reads; an
    // exchange writes either of its two, and an ARM store names its memory last.
    let stores = i.writes_memory
        && i.mnemonic != "push"
        && (index == 0 || i.mnemonic == "xchg" || (i.mnemonic.starts_with("st") && i.operands.len() == index + 1));
    Some((at, if stores { "write" } else { "read" }))
}

/// The data `function` addresses by a constant: each place once per kind of access, with the
/// section that holds it and the string there when it is one.
pub(crate) fn data_of_json(image: &Image, function: &ra_asm_analysis::Function) -> serde_json::Value {
    let mut seen: Vec<(u64, &'static str)> = Vec::new();
    for i in function.blocks.iter().flat_map(|b| &b.instrs) {
        if let Some(access) = data_access(i)
            && !seen.contains(&access)
        {
            seen.push(access);
        }
    }
    seen.sort_unstable();
    let rows = seen.iter().map(|(at, access)| {
        json!({
            "addr": hex(*at), "access": access, "section": image.section_of(*at).map(|s| s.name.clone()),
            "string": function.strings.iter().find(|(va, _)| va == at).map(|(_, text)| text),
        })
    });
    serde_json::Value::Array(rows.collect())
}

/// What touches the data in `[lo, hi)`: the functions that store there, and every instruction that
/// stores there, loads from there or takes its address, and every relocated pointer into it. What
/// `ras serve` answers to find the function that writes a global.
pub(crate) fn accesses_json(image: &Image, analysis: &Analysis<'_>, lo: u64, hi: u64) -> serde_json::Value {
    let mut writers: Vec<serde_json::Value> = Vec::new();
    let mut rows = Vec::new();
    for f in analysis.functions.values() {
        let mut writes = false;
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            let Some((at, access)) = data_access(i).filter(|(at, _)| *at >= lo && *at < hi) else { continue };
            writes |= access == "write";
            rows.push(json!({
                "from": hex(i.address), "access": access, "target": hex(at), "function": f.name, "function_addr": hex(f.entry), "text": i.text,
            }));
        }
        if writes {
            writers.push(json!({"addr": hex(f.entry), "name": f.name}));
        }
    }
    let pointers: Vec<_> = references(image, analysis, lo, hi, false).iter().filter(|r| r.kind == "pointer").map(Reference::json).collect();
    json!({"writers": writers, "accesses": rows, "pointers": pointers})
}

/// Every call or jump to the function at `callee`, and every pointer to it in the data.
pub(crate) fn callers(image: &Image, callee: u64, json: bool) -> Result<()> {
    let analysis = analyze(image);
    let refs = references(image, &analysis, callee, callee + 1, true);
    print(image, &analysis, &refs, json, &format!("references to {} ({})", analysis.name_of(callee), hex(callee)))
}

/// Every instruction and every data pointer that refers into `[lo, hi)`.
pub(crate) fn xrefs(image: &Image, lo: u64, hi: Option<u64>, json: bool) -> Result<()> {
    let hi = hi.unwrap_or(lo + 1);
    let analysis = analyze(image);
    let refs = references(image, &analysis, lo, hi, false);
    print(image, &analysis, &refs, json, &format!("references into [{}, {})", hex(lo), hex(hi)))
}
