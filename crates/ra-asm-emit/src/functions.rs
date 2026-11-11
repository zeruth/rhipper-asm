//! The function table, `functions.json`: what the workbench reads to list the functions of a
//! port, open each one in its source, and show how far the lift got. One JSON array, one entry per
//! function the analysis found:
//!
//! ```json
//! {"addr": "0x140001330", "name": "fib", "size": 64, "file": "program.c", "line": 131,
//!  "status": "lifted", "traps": 0, "calls": ["fib"], "imports": [], "strings": ["fail\r\n"]}
//! ```
//!
//! `file` is relative to the directory the translation is written to: `program.c`, or
//! `program_NNN.c` when the output is split. `status` is `lifted` when every instruction
//! translated, `partial` when the body calls `ra_trap` for the ones that did not (`traps` counts
//! them), and `not lifted` when the function has no source at all, in which case `file` and `line`
//! are absent. A thunk to an import has status `thunk` and names the import in `imports`.
//! `strings` lists the string literals the function addresses. A virtual function has `class` and
//! `slot`: the class whose vtable holds it first, and the slot in that vtable.

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
}

/// What the analysis knows of every function, by IR name.
pub fn infos(analysis: &ra_asm_analysis::Analysis) -> HashMap<String, FunctionInfo> {
    let name_of = |va: &u64| analysis.functions.get(va).map_or_else(|| ra_asm_lift::function_name(*va), |f| f.name.clone());
    analysis
        .functions
        .values()
        .map(|f| {
            let info = FunctionInfo {
                name: f.name.clone(),
                address: f.entry,
                size: f.size(),
                calls: f.calls.iter().map(name_of).collect(),
                imports: f.import_calls.clone(),
                thunk: f.is_thunk(),
                strings: f.strings.iter().map(|(_, text)| text.clone()).collect(),
                class: f.virtuals.first().map(|v| (v.class.clone(), v.slot)),
            };
            (ra_asm_lift::function_name(f.entry), info)
        })
        .collect()
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
            entry.insert("status".into(), json!(if f.traps == 0 { "lifted" } else { "partial" }));
            entry.insert("traps".into(), json!(f.traps));
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
        entry.insert("status".into(), json!(if info.thunk { "thunk" } else { "not lifted" }));
        entry.insert("traps".into(), json!(0));
        entry.insert("calls".into(), json!(info.calls));
        entry.insert("imports".into(), json!(info.imports));
        facts(&mut entry, info);
        entries.push((info.address, Value::Object(entry)));
    }
    entries.sort_by_key(|(address, _)| *address);
    let values: Vec<Value> = entries.into_iter().map(|(_, v)| v).collect();
    finish(values)
}

/// The strings and the class of a function, in its table entry.
fn facts(entry: &mut Map<String, Value>, info: &FunctionInfo) {
    entry.insert("strings".into(), json!(info.strings));
    if let Some((class, slot)) = &info.class {
        entry.insert("class".into(), json!(class));
        entry.insert("slot".into(), json!(slot));
    }
}

fn finish(values: Vec<Value>) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Array(values)).expect("a JSON array prints");
    out.push('\n');
    out
}
