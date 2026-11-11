//! `ras callers` and `ras xrefs`: who refers to a function, and what refers to an address range.

use std::io::Write;

use anyhow::Result;
use ra_asm_analysis::{Analysis, analyze};
use ra_asm_arch::Operand;
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
