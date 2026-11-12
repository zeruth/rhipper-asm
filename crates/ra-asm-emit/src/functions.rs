//! The function table, `functions.json`: what the workbench reads to list the functions of a
//! port, open each one in its source, and show how far the lift got. One JSON array, one entry per
//! function the analysis found:
//!
//! ```json
//! {"addr": "0x140001330", "name": "fib", "size": 64, "file": "program.c", "line": 131,
//!  "status": "lifted", "traps": 0, "copies": 0, "calls": ["fib"], "imports": [], "strings": ["fail\r\n"]}
//! ```
//!
//! `file` is relative to the directory the translation is written to: `program.c`, the module's
//! own file (`core.c`) when the build's symbols name the object files of two modules or more, or
//! `program_NNN.c` when the output is split by size. `object` names the object file the symbols
//! say holds the function (`core.obj`, `msvcrt.lib:gs_report.obj`), when they say: with `file`,
//! the tree the workbench shows. `status` is `lifted` when every instruction
//! translated, `partial` when the body calls `ra_trap` for the ones that did not (`traps` counts
//! them), and `not lifted` when the function has no source at all, in which case `file` and `line`
//! are absent. `copies` counts the blocks the structuring wrote a second time in place of a
//! `goto`: the first of the readability figures. A thunk to an import has status `thunk` and
//! names the import in `imports`; a function of a C runtime linked into the image, which the
//! port calls in the host's runtime instead of lifting, has status `runtime` and names the
//! host's function in `imports`. A function the hand-written sources of the port own (`hand` in
//! `rhipper.json`) has status `hand`: `file` and `line` point at its declaration in the
//! translation, where the comment says so.
//! `strings` lists the string literals the function addresses. A virtual function has `class` and
//! `slot`: the class whose vtable holds it first, and the slot in that vtable. With the facts of a
//! traced run ([`ra_asm_analysis::facts`]), a function the run entered has `"seen": true`, and
//! one whose indirect calls the run resolved lists the functions they reached in `seen_calls`
//! (they are in `calls` as well): what the workbench shows as seen at run time. A function the
//! person annotated (the project's `rhipper/annotations/`) has `"annotated": true`, its `name`
//! the annotation's, and the annotation's `type` and `comment` when it gives them.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::c::{EmittedFile, EmittedFunction};

/// What the analysis knows of a function, by its IR name.
#[derive(Debug, Clone, Default)]
pub struct FunctionInfo {
    pub name: String,
    pub address: u64,
    pub size: u64,
    /// The functions it calls directly, by source name, in first-call order.
    pub calls: Vec<String>,
    /// The imports it calls or jumps to.
    pub imports: Vec<String>,
    /// Whether the function is a thunk to an import.
    pub thunk: bool,
    /// The string literals it addresses.
    pub strings: Vec<String>,
    /// The class whose vtable holds it, and the slot, for a virtual function.
    pub class: Option<(String, usize)>,
    /// The host's name for a function of a C runtime linked into the image, which is called, not
    /// lifted.
    pub runtime: Option<String>,
    /// Whether a traced run entered the function.
    pub seen: bool,
    /// The functions a traced run saw the function's indirect calls reach, by source name.
    pub seen_calls: Vec<String>,
    /// Whether the person's annotations of the project say something of the function.
    pub annotated: bool,
    /// The function's type and the comment the person's annotation gives it.
    pub ty: Option<String>,
    pub comment: Option<String>,
    /// The object file the build's symbols say holds the function (`core.obj`,
    /// `msvcrt.lib:gs_report.obj`): its module, which the split output follows.
    pub object: Option<String>,
}

/// What the analysis knows of every function, by IR name: the analysis's name, which the lift
/// gives the function ([`ra_asm_lift::Lifted::apply_names`]).
pub fn infos(analysis: &ra_asm_analysis::Analysis) -> HashMap<String, FunctionInfo> {
    let name_of = |va: &u64| analysis.functions.get(va).map_or_else(|| ra_asm_lift::function_name(*va), |f| f.name.clone());
    analysis
        .functions
        .values()
        .map(|f| {
            let runtime = match &f.kind {
                ra_asm_analysis::Kind::Runtime(name) => Some(name.clone()),
                _ => None,
            };
            let mut seen_calls: Vec<String> = Vec::new();
            for target in f.traced_calls.values().flatten() {
                let name = name_of(target);
                if !seen_calls.contains(&name) {
                    seen_calls.push(name);
                }
            }
            let info = FunctionInfo {
                name: f.name.clone(),
                address: f.entry,
                size: f.size(),
                calls: f.calls.iter().map(name_of).collect(),
                imports: f.import_calls.clone(),
                thunk: f.is_thunk(),
                strings: f.strings.iter().map(|(_, text)| text.clone()).collect(),
                class: f.virtuals.first().map(|v| (v.class.clone(), v.slot)),
                runtime,
                seen: analysis.facts.entered.contains(&f.entry),
                seen_calls,
                object: f.object.clone(),
                ..Default::default()
            };
            (f.name.clone(), info)
        })
        .collect()
}

/// The object file of every function the build's symbols place, by IR name: what
/// [`crate::Unit::module_files`] splits the output by.
pub fn objects(infos: &HashMap<String, FunctionInfo>) -> HashMap<String, String> {
    infos.iter().filter_map(|(ir_name, info)| Some((ir_name.clone(), info.object.clone()?))).collect()
}

/// An address as the workbench writes it: `0x` and at least eight hexadecimal digits.
pub fn format_address(address: u64) -> String {
    format!("{address:#010x}")
}

/// The function table of `functions`, written as `files`, as a JSON array ordered by address.
pub fn table(functions: &[EmittedFunction], files: &[EmittedFile], infos: &HashMap<String, FunctionInfo>) -> String {
    let emitted: HashMap<&str, &EmittedFunction> = functions.iter().map(|f| (f.ir_name.as_str(), f)).collect();
    let mut entries: Vec<(u64, Value)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for file in files {
        for (ir_name, line) in &file.functions {
            let Some(f) = emitted.get(ir_name.as_str()) else { continue };
            let Some(address) = f.address else { continue };
            let info = infos.get(&f.ir_name);
            seen.push(&f.ir_name);
            let mut entry = Map::new();
            entry.insert("addr".into(), json!(format_address(address)));
            entry.insert("name".into(), json!(info.map_or(f.ir_name.as_str(), |i| i.name.as_str())));
            entry.insert("size".into(), json!(info.map_or(0, |i| i.size)));
            entry.insert("file".into(), json!(file.name));
            entry.insert("line".into(), json!(line));
            let status = if f.hand {
                "hand"
            } else if f.traps == 0 {
                "lifted"
            } else {
                "partial"
            };
            entry.insert("status".into(), json!(status));
            entry.insert("traps".into(), json!(f.traps));
            entry.insert("copies".into(), json!(f.copies));
            entry.insert("calls".into(), json!(info.map_or(&[][..], |i| &i.calls)));
            entry.insert("imports".into(), json!(info.map_or(&[][..], |i| &i.imports)));
            if let Some(info) = info {
                facts(&mut entry, info);
            }
            entries.push((address, Value::Object(entry)));
        }
    }
    for (ir_name, info) in infos {
        if seen.contains(&ir_name.as_str()) {
            continue;
        }
        let mut entry = Map::new();
        entry.insert("addr".into(), json!(format_address(info.address)));
        entry.insert("name".into(), json!(info.name));
        entry.insert("size".into(), json!(info.size));
        let status = if info.thunk {
            "thunk"
        } else if info.runtime.is_some() {
            "runtime"
        } else {
            "not lifted"
        };
        entry.insert("status".into(), json!(status));
        entry.insert("traps".into(), json!(0));
        entry.insert("copies".into(), json!(0));
        entry.insert("calls".into(), json!(info.calls));
        entry.insert("imports".into(), json!(info.runtime.as_ref().map_or(&info.imports[..], std::slice::from_ref)));
        facts(&mut entry, info);
        entries.push((info.address, Value::Object(entry)));
    }
    entries.sort_by_key(|(address, _)| *address);
    let values: Vec<Value> = entries.into_iter().map(|(_, v)| v).collect();
    finish(values)
}

/// The strings, the class and what a traced run saw of a function, in its table entry.
fn facts(entry: &mut Map<String, Value>, info: &FunctionInfo) {
    entry.insert("strings".into(), json!(info.strings));
    if let Some(object) = &info.object {
        entry.insert("object".into(), json!(object));
    }
    if let Some((class, slot)) = &info.class {
        entry.insert("class".into(), json!(class));
        entry.insert("slot".into(), json!(slot));
    }
    if info.seen {
        entry.insert("seen".into(), json!(true));
    }
    if !info.seen_calls.is_empty() {
        entry.insert("seen_calls".into(), json!(info.seen_calls));
    }
    if info.annotated {
        entry.insert("annotated".into(), json!(true));
    }
    if let Some(ty) = &info.ty {
        entry.insert("type".into(), json!(ty));
    }
    if let Some(comment) = &info.comment {
        entry.insert("comment".into(), json!(comment));
    }
}

fn finish(values: Vec<Value>) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Array(values)).expect("a JSON array prints");
    out.push('\n');
    out
}
