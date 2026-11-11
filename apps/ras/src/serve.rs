//! `ras serve`: the engine as a service for the workbench. JSON-RPC 2.0 over stdio, one request
//! and one response per line, with the image and its analysis held in memory between requests,
//! so a question about a large program costs no new analysis. The image is loaded again when its
//! file changes.
//!
//! The methods, with their parameters:
//! - `project`: the project and the outputs that exist.
//! - `info`: the headers of the image.
//! - `functions`: every function of the analysis, and the classes of the RTTI.
//! - `classes`: the classes alone: name, bases, and each vtable with its functions.
//! - `function` (`query`): one function: the analysis, the strings it addresses, the vtable slots
//!   that hold it, the function table entry, its source in the translation, its IR, and its
//!   instructions.
//! - `disasm` (`address`, `length`), `cfg` (`address`), `callers` (`address`), `xrefs` (`lo`, `hi`).
//! - `strings` (`min`, `limit`): the printable strings of the data sections.
//! - `shutdown`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow};
use ra_asm_analysis::Analysis;
use ra_asm_pe::Image;
use serde_json::{Value, json};

use crate::disasm::parse_address;
use crate::project::Project;
use crate::report::hex;

/// What the server serves: a project with a `rhipper.json`, or a bare image.
pub(crate) struct Target {
    pub project: Option<Project>,
    pub exe: PathBuf,
}

impl Target {
    pub(crate) fn of(project_dir: Option<&Path>, exe: Option<&Path>) -> Result<Self> {
        if let Some(exe) = exe {
            return Ok(Self { project: None, exe: exe.to_path_buf() });
        }
        let project = Project::load(&crate::pipeline::project_dir(project_dir))?;
        let exe = project.exe.clone();
        Ok(Self { project: Some(project), exe })
    }
}

/// Runs the server on stdin and stdout until `shutdown` or the end of the input.
pub(crate) fn serve(target: &Target) -> Result<()> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        let image = match &target.project {
            Some(project) => project.load_image()?,
            None => ra_asm_pe::load(&target.exe)?,
        };
        let loaded = modified(&target.exe);
        let analysis = ra_asm_analysis::analyze(&image);
        let session = Session { target, image: &image, analysis: &analysis };
        if let Outcome::Exit = session.run(&mut lines, loaded)? {
            return Ok(());
        }
    }
}

enum Outcome {
    Reload,
    Exit,
}

struct Session<'a> {
    target: &'a Target,
    image: &'a Image,
    analysis: &'a Analysis<'a>,
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Session<'_> {
    fn run(&self, lines: &mut impl Iterator<Item = std::io::Result<String>>, loaded: Option<SystemTime>) -> Result<Outcome> {
        let mut out = std::io::stdout().lock();
        for line in lines {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    write_response(&mut out, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}))?;
                    continue;
                }
            };
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let method = request.get("method").and_then(Value::as_str).unwrap_or("");
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            if method == "shutdown" {
                write_response(&mut out, &json!({"jsonrpc": "2.0", "id": id, "result": null}))?;
                return Ok(Outcome::Exit);
            }
            // The image changed on disk: answer after a reload, with the request replayed.
            if modified(&self.target.exe) != loaded {
                write_response(
                    &mut out,
                    &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "the image changed; the server reloads it, ask again"}}),
                )?;
                return Ok(Outcome::Reload);
            }
            let response = match self.dispatch(method, &params) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(e) => {
                    let code = if e.to_string().starts_with("no method") { -32601 } else { -32602 };
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": format!("{e:#}")}})
                }
            };
            write_response(&mut out, &response)?;
        }
        Ok(Outcome::Exit)
    }

    fn dispatch(&self, method: &str, params: &Value) -> Result<Value> {
        let address = |key: &str| -> Result<u64> {
            let text = params.get(key).and_then(Value::as_str).ok_or_else(|| anyhow!("the parameter `{key}` is missing"))?;
            parse_address(text)
        };
        match method {
            "project" => Ok(self.project()),
            "info" => Ok(crate::report::info_json(self.image)),
            "functions" => Ok(crate::functions::functions_json(self.analysis)),
            "classes" => Ok(json!({"classes": crate::functions::classes_json(self.analysis)})),
            "function" => {
                let query = params.get("query").and_then(Value::as_str).ok_or_else(|| anyhow!("the parameter `query` is missing"))?;
                self.function(query)
            }
            "disasm" => {
                let length = params.get("length").and_then(Value::as_str).map(parse_address).transpose()?;
                let decoded = crate::disasm::decode(self.image, address("address")?, length)?;
                Ok(
                    json!({"text": crate::disasm::disasm_text(self.image, &decoded), "instrs": crate::disasm::disasm_json(self.image, &decoded)}),
                )
            }
            "cfg" => {
                let a = address("address")?;
                let function = self
                    .analysis
                    .function(a)
                    .or_else(|| self.analysis.containing(a))
                    .ok_or_else(|| anyhow!("no function holds {}", hex(a)))?;
                Ok(crate::functions::cfg_json(self.image, function))
            }
            "callers" => Ok(crate::xrefs::callers_json(self.image, self.analysis, address("address")?)),
            "xrefs" => {
                let lo = address("lo")?;
                let hi = params.get("hi").and_then(Value::as_str).map(parse_address).transpose()?.unwrap_or(lo + 1);
                Ok(crate::xrefs::xrefs_json(self.image, self.analysis, lo, hi))
            }
            "strings" => {
                let min = params.get("min").and_then(Value::as_u64).unwrap_or(5) as usize;
                let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(1000) as usize;
                Ok(self.strings(min, limit))
            }
            other => Err(anyhow!("no method `{other}`")),
        }
    }

    fn project(&self) -> Value {
        let Some(project) = &self.target.project else {
            return json!({"exe": self.target.exe.display().to_string()});
        };
        let exists = |p: &Path| p.is_file().then(|| p.display().to_string());
        json!({
            "root": project.root.display().to_string(),
            "name": project.name,
            "exe": project.exe.display().to_string(),
            "map": project.map.as_ref().map(|m| m.display().to_string()),
            "gen": project.gen_dir.display().to_string(),
            "lang": project.lang,
            "build": {"dir": project.build.dir.display().to_string(), "config": project.build.config, "target": project.build.target},
            "port": project.port_exe().display().to_string(),
            "outputs": {
                "functions": exists(&project.build_dir().join("functions.json")),
                "ir": exists(&project.build_dir().join("program.ll")),
                "logs": project.log_dir().display().to_string(),
            },
            "machine": self.image.machine().name(),
            "entry": self.image.entry_point().map(hex),
            "functions": self.analysis.functions.len(),
        })
    }

    /// One function by name, address or an address inside it: the analysis, the table entry, the
    /// source, the IR and the instructions.
    fn function(&self, query: &str) -> Result<Value> {
        let function = self
            .analysis
            .functions
            .values()
            .find(|f| f.name == query)
            .or_else(|| {
                let a = parse_address(query).ok()?;
                self.analysis.function(a).or_else(|| self.analysis.containing(a))
            })
            .ok_or_else(|| anyhow!("no function matches `{query}`"))?;
        // The IR and the source name a function as the analysis does.
        let ir_name = function.name.clone();
        let decoded = crate::disasm::decode(self.image, function.entry, Some(function.size()))?;
        let (strings, virtuals) = crate::functions::facts_json(function);
        let mut result = json!({
            "addr": hex(function.entry), "name": function.name, "ir_name": ir_name, "size": function.size(),
            "blocks": function.blocks.len(), "instrs": function.instr_count(), "noreturn": function.noreturn,
            "calls": function.calls.iter().map(|c| self.analysis.name_of(*c)).collect::<Vec<_>>(),
            "imports": function.import_calls,
            "strings": strings, "virtuals": virtuals,
            "disasm": crate::disasm::disasm_text(self.image, &decoded),
        });
        if let Some(project) = &self.target.project {
            // The function table entry, and the source of the function in the translation.
            if let Ok(text) = std::fs::read_to_string(project.build_dir().join("functions.json"))
                && let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&text)
                && let Some(entry) =
                    entries.into_iter().find(|e| e.get("addr").and_then(Value::as_str) == Some(hex(function.entry).as_str()))
            {
                if let (Some(file), Some(line)) = (entry.get("file").and_then(Value::as_str), entry.get("line").and_then(Value::as_u64)) {
                    let path = project.gen_dir.join(file);
                    result["source_file"] = json!(path.display().to_string());
                    result["source"] = json!(source_at(&path, line as usize));
                }
                result["table"] = entry;
            }
            result["ir"] = json!(ir_of(&project.build_dir().join("program.ll"), &ir_name));
        }
        Ok(result)
    }

    /// The printable strings of the data sections: runs of `min` printable bytes ended by a zero.
    fn strings(&self, min: usize, limit: usize) -> Value {
        let mut rows = Vec::new();
        for section in &self.image.sections {
            if section.is_code() || section.virtual_size == 0 {
                continue;
            }
            let start = self.image.va(section.virtual_address);
            let Ok(bytes) = self.image.read_to_section_end(start) else { continue };
            let mut run = 0usize;
            for (i, b) in bytes.iter().enumerate() {
                if (0x20..0x7f).contains(b) || *b == b'\t' {
                    run += 1;
                } else {
                    if *b == 0 && run >= min {
                        let text = String::from_utf8_lossy(&bytes[i - run..i]).into_owned();
                        rows.push(json!({"addr": hex(start + (i - run) as u64), "section": section.name, "text": text}));
                        if rows.len() >= limit {
                            return json!({"strings": rows, "truncated": true});
                        }
                    }
                    run = 0;
                }
            }
        }
        json!({"strings": rows, "truncated": false})
    }
}

/// The text of a function in a source file: from `line` (1-based) to the closing brace in column 0,
/// with the comment line above it.
fn source_at(path: &Path, line: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    let start = if line >= 2 && lines[line - 2].starts_with("//") { line - 2 } else { line - 1 };
    let mut out = String::new();
    for l in &lines[start..] {
        out.push_str(l);
        out.push('\n');
        if *l == "}" {
            break;
        }
    }
    Some(out)
}

/// The definition of `@name` in an `.ll` file, up to its closing brace.
fn ir_of(path: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let needle = format!("@{name}(");
    let mut out = String::new();
    let mut inside = false;
    for l in text.lines() {
        if !inside && l.starts_with("define ") && l.contains(&needle) {
            inside = true;
        }
        if inside {
            out.push_str(l);
            out.push('\n');
            if l == "}" {
                return Some(out);
            }
        }
    }
    None
}

fn write_response(out: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *out, response).context("writing the response")?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
