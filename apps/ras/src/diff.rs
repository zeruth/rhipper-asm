//! `ras diff`: two builds of a program matched function by function (see
//! [`ra_asm_analysis::matching`]), the changed functions ranked by how much of their code changed,
//! with the diff of their raised IR on request, and the functions one build has alone: a patch
//! reads as the functions it touched. `ras serve` answers the same JSON for the workbench.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use anyhow::Result;
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
    let alone = |analysis: &Analysis<'_>, va: u64| {
        json!({"address": hex(va), "name": analysis.name_of(va), "symbol": analysis.function(va).and_then(|f| f.symbol.clone())})
    };
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
        value["old_functions"], value["new_functions"], value["matched"], value["changed"], value["removed"].as_array().map_or(0, Vec::len),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_name_takes_the_address_of_its_match() {
        let renames = HashMap::from([(0x401000, 0x402000)]);
        assert_eq!(renamed("call @F_00401000(F_00401010) F_", &renames), "call @F_00402000(F_00401010) F_");
    }
}
