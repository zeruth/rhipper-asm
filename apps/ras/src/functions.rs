//! `ras functions` and `ras cfg`: the functions of an image, and the blocks of one.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};
use ra_asm_analysis::{Analysis, Kind, Origin, Reach, Terminator, analyze};
use ra_asm_emit::FunctionInfo;
use ra_asm_pe::Image;
use serde_json::json;

use crate::disasm::note;
use crate::report::hex;

pub(crate) fn origin_name(origin: &Origin) -> String {
    match origin {
        Origin::Entry => "entry".to_owned(),
        Origin::Export(name) => format!("export {name}"),
        Origin::TlsCallback => "tls callback".to_owned(),
        Origin::ExceptionTable => "pdata".to_owned(),
        Origin::Symbol => "symbol".to_owned(),
        Origin::Call(from) => format!("call from {}", hex(*from)),
        Origin::TailCall(from) => format!("jump from {}", hex(*from)),
        Origin::Pointer(slot) => format!("pointer at {}", hex(*slot)),
        Origin::Vtable(vtable) => format!("vtable at {}", hex(*vtable)),
        Origin::Gap(after) => format!("gap after {}", hex(*after)),
        Origin::Trace(from) => format!("seen at run time from {}", hex(*from)),
        Origin::Residual => "run by the residual engine".to_owned(),
    }
}

/// The classes of the analysis as JSON: name, bases (all of them, and the direct ones), and each
/// vtable with its functions by name; a base nothing instantiates has no vtables.
pub(crate) fn classes_json(analysis: &Analysis<'_>) -> serde_json::Value {
    let rows: Vec<_> = analysis
        .classes
        .iter()
        .map(|c| {
            json!({
                "name": c.name, "type_descriptor": hex(c.type_descriptor), "bases": c.bases, "direct_bases": c.direct_bases,
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

/// The shape of one function's control: its back edges (a loop each, or a jump back), its
/// switches, the indirect jumps the analysis did not resolve, and each indirect call with the
/// import it reaches or the functions a traced run saw it reach.
pub(crate) fn shape_json(analysis: &Analysis<'_>, f: &ra_asm_analysis::Function) -> serde_json::Value {
    let back_edges = f.blocks.iter().map(|b| b.successors().iter().filter(|s| **s <= b.start && f.contains(**s)).count()).sum::<usize>();
    let switches = f.blocks.iter().filter(|b| matches!(b.terminator, Terminator::Switch { .. })).count();
    let unresolved: Vec<_> = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::IndirectJump))
        .filter_map(|b| b.instrs.last().map(|i| json!({"addr": hex(i.address), "text": i.text})))
        .collect();
    let indirect: Vec<_> = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| i.flow == ra_asm_arch::Flow::Call(ra_asm_arch::Target::Indirect))
        .map(|i| {
            let traced = f.traced_calls.get(&i.address).map(|targets| targets.iter().map(|t| analysis.name_of(*t)).collect::<Vec<_>>());
            json!({"addr": hex(i.address), "text": i.text, "import": f.register_import_calls.get(&i.address), "traced": traced})
        })
        .collect();
    json!({"back_edges": back_edges, "switches": switches, "unresolved_jumps": unresolved, "indirect_calls": indirect})
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
                "addr": hex(f.entry), "name": f.name, "symbol": f.symbol, "object": f.object, "size": f.size(), "bytes": f.code_bytes(), "blocks": f.blocks.len(),
                "instrs": f.instr_count(), "noreturn": f.noreturn,
                "thunk": match &f.kind { Kind::ImportThunk(i) => Some(i.clone()), _ => None },
                "runtime": match &f.kind { Kind::Runtime(name) => Some(name.clone()), _ => None },
                "calls": f.calls.iter().map(|c| hex(*c)).collect::<Vec<_>>(), "imports": f.import_calls,
                "tail_calls": f.tail_calls.iter().map(|c| hex(*c)).collect::<Vec<_>>(),
                "origins": f.origins.iter().map(origin_name).collect::<Vec<_>>(),
                "strings": strings, "virtuals": virtuals,
                "gap": f.reach != Reach::Named, "dead": match &f.reach { Reach::Dead(why) => Some(why.clone()), _ => None },
                "address_taken": analysis.is_address_taken(f.entry),
            })
        })
        .collect();
    let (dead, gap) = analysis.gap_counts();
    json!({
        "functions": rows, "classes": classes_json(analysis), "warnings": analysis.warnings,
        "gaps": {"fill": analysis.fill.len(), "dead": dead, "gap": gap},
    })
}

/// A `PlayStation 2` program's remote procedure calls, each joined with the server of the IOP
/// modules beside the program (every `.irx` in its directory) that registers its id: the module,
/// and the function the call's number reaches there by its address and name.
fn rpc_json(analysis: &Analysis<'_>) -> serde_json::Value {
    let calls = ra_asm_analysis::rpc::calls(analysis);
    let mut servers = Vec::new();
    // The module and the names of its server's functions, one per server.
    let mut owners: Vec<(String, HashMap<u64, String>)> = Vec::new();
    if !calls.is_empty() {
        for path in modules_beside(analysis.image) {
            let Ok(image) = ra_asm_pe::load(&path) else { continue };
            let module = analyze(&image);
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            for server in ra_asm_analysis::rpc::servers(&module) {
                let reached = std::iter::once(server.dispatch).chain(server.functions.iter().map(|&(_, f)| f));
                owners.push((file.clone(), reached.map(|f| (f, module.name_of(f))).collect()));
                servers.push(server);
            }
        }
    }
    let rows: Vec<_> = calls
        .iter()
        .map(|c| {
            let target = ra_asm_analysis::rpc::resolve(c, &servers);
            let owner = c.server.and_then(|id| servers.iter().position(|s| s.id == id)).map(|i| &owners[i]);
            json!({
                "at": hex(c.at), "caller": hex(c.caller), "client": c.client.map(hex),
                "server": c.server.map(|id| format!("{id:#010x}")), "function": c.function,
                "module": owner.map(|o| o.0.clone()), "target": target.map(hex),
                "name": target.and_then(|t| owner?.1.get(&t).cloned()),
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

/// The IOP modules in the directory of `image`'s file, by name.
fn modules_beside(image: &Image) -> Vec<std::path::PathBuf> {
    let Some(dir) = image.path.as_deref().and_then(Path::parent) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(if dir.as_os_str().is_empty() { Path::new(".") } else { dir }) else { return Vec::new() };
    let mut modules: Vec<_> =
        entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("irx"))).collect();
    modules.sort();
    modules
}

/// Gives each function of a translation's table the evidence the ledger of `gen_dir` holds of the
/// function at its address; a gen directory without a ledger gives none.
pub(crate) fn note_evidence(infos: &mut HashMap<String, FunctionInfo>, gen_dir: &Path) -> Result<()> {
    let ledger = ra_asm_verify::ledger::load(gen_dir)?;
    for info in infos.values_mut() {
        if let Some(function) = ledger.functions.get(&info.address) {
            info.evidence.clone_from(&function.steps);
        }
    }
    Ok(())
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

/// The system calls each lifted function makes by name, keyed by its address as the rows give it:
/// a `PlayStation 2` program's calls on the EE kernel, as the lift of `syscall` names them.
fn kernel_calls(module: &ra_asm_ir::Module) -> HashMap<String, Vec<&'static str>> {
    module
        .functions
        .iter()
        .filter_map(|f| {
            let address = f.address?;
            let calls: Vec<_> = f
                .blocks
                .iter()
                .flat_map(|b| &b.instrs)
                .filter_map(|i| match &i.inst {
                    ra_asm_ir::Inst::Call { callee: ra_asm_ir::Callee::Direct(name), .. } => {
                        ra_asm_lift::raise::prototypes::ee_syscall_named(name)
                    }
                    _ => None,
                })
                .map(|call| call.name)
                .collect();
            (!calls.is_empty()).then(|| (hex(address), calls))
        })
        .collect()
}

pub(crate) fn functions(image: &Image, json: bool) -> Result<()> {
    let analysis = analyze(image);
    let mut out = std::io::stdout().lock();
    if json {
        // The coverage of the machine-level lift rides along, so the workbench reads it where it
        // reads the functions.
        let lifted = ra_asm_lift::lift(image, &analysis);
        let mut value = functions_json(&analysis);
        let syscalls = kernel_calls(&lifted.module);
        if let Some(rows) = value["functions"].as_array_mut() {
            for row in rows {
                let calls = row["addr"].as_str().and_then(|addr| syscalls.get(addr)).cloned().unwrap_or_default();
                row["syscalls"] = json!(calls);
            }
        }
        value["coverage"] = crate::report::coverage_json(&ra_asm_lift::coverage(&analysis, &lifted));
        if image.platform() == Some("ps2") {
            value["rpc"] = rpc_json(&analysis);
        }
        serde_json::to_writer_pretty(&mut out, &value)?;
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
        if let Reach::Dead(why) = &f.reach {
            notes.push(format!("dead: {why}"));
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
    let (dead, gap) = analysis.gap_counts();
    writeln!(
        out,
        "gaps: {} fill (no functions), {dead} dead (not lifted), {gap} gap (lifted); {} address-taken",
        analysis.fill.len(),
        analysis.address_taken.as_ref().map_or(analysis.functions.len(), std::collections::BTreeSet::len)
    )?;
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
