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
//! - `hand` (`query`, `owned`): gives a function of a project to the hand-written sources or
//!   takes it back, in the `hand` list of `rhipper.json` (by the function's name), and answers
//!   with the list; the next emit step writes the function as its declaration.
//! - `annotate` (`query`, and any of `name`, `type`, `comment`, `notes`, `hand`): changes the
//!   person's annotation of a function in its sidecar file under `rhipper/annotations/` (see
//!   [`crate::annotations`]): a text key sets the text and `null` or `""` clears it, `hand` sets
//!   the takeover; a name another function has, or the port's runtime owns, is refused. Answers
//!   with the annotation; every later answer, and the next emit, takes the name.
//! - `annotations`: every annotation of the project, and what of them the analysis could not take
//!   (with what the project's name-source plugins said that it could not take).
//! - `plugins`: the plugins of the project and the user's (see [`crate::plugins`]): `abi`, the
//!   `user` directory, and each plugin's `name`, `origin`, `dir`, `kinds` and `error`.
//!
//! The answers name the functions as the plugins and then the annotations do; a change to the sidecar files on disk (a
//! checkout, a merge) reloads them before the next answer.
//! - `trace` (`function`, `from`, `limit`): the replay of the port's trace against the
//!   reference's (the two the run step recorded with `run.trace`), call by call: each call of the
//!   port with its arguments and its result beside the reference's at the same place, the calls
//!   of `function` alone when given, from the `from`th and at most `limit`; the verdict; and
//!   `parts`, the first divergence, with the function, the call and the two values.
//! - `lockstep.start`: starts the port (a build with `run.trace`) and the reference through the
//!   interpreter side by side, each stopped at its first function entry (see
//!   [`crate::lockstep`]), and answers with the state: `entry`, the place of each side (the
//!   `function` and its `args` with their kinds, and `address` on an integer the reference holds
//!   as an address of its memory, or `ended` with the `exit`), `equal`, and
//!   `differs` when the two part. A session started again replaces the one before.
//! - `lockstep.step` (`function`, `count`, `divergence`): lets both sides go on `count` entries
//!   (1 by default; of `function` alone when given), or to the first entry where they part when
//!   `divergence`; never past the end of either side. Answers with the state.
//! - `lockstep.state`: the state where the two stand.
//! - `lockstep.read` (`address`, `size`): the `size` bytes at `address` (at most 4096) in each
//!   side's memory, as hexadecimal `bytes`, or the `error` that kept them.
//! - `lockstep.stop`: ends the session; the port is stopped.
//! - `shutdown`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use ra_asm_analysis::Analysis;
use ra_asm_pe::Image;
use serde_json::{Value, json};

use crate::annotations::{self, Annotation};
use crate::disasm::parse_address;
use crate::lockstep::Lockstep;
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
    let mut pending: Option<String> = None;
    // The two processes of a lockstep outlive a reload of the image or of the annotations.
    let lockstep = RefCell::new(None);
    loop {
        let image = match &target.project {
            Some(project) => project.load_image()?,
            None => ra_asm_pe::load(&target.exe)?,
        };
        let loaded = modified(&target.exe);
        let mut analysis = ra_asm_analysis::analyze(&image);
        let (annotated, warnings) = match &target.project {
            Some(project) => annotations::annotate(&project.root, &mut analysis),
            None => (BTreeMap::new(), Vec::new()),
        };
        let stamp = target.project.as_ref().map(|p| annotations::stamp(&p.root));
        let session = Session { target, image: &image, analysis: &analysis, annotations: annotated, warnings, stamp, lockstep: &lockstep };
        let mut replay = pending.take().map(Ok::<String, std::io::Error>).into_iter().chain(&mut lines);
        match session.run(&mut replay, loaded)? {
            Outcome::Exit => return Ok(()),
            Outcome::Reload(line) => pending = line,
        }
    }
}

enum Outcome {
    /// Load the image and the annotations again, then answer the request held, when there is one.
    Reload(Option<String>),
    Exit,
}

struct Session<'a> {
    target: &'a Target,
    image: &'a Image,
    analysis: &'a Analysis<'a>,
    /// The project's annotations as the analysis took them, by address.
    annotations: BTreeMap<u64, Annotation>,
    /// What the annotations said that the analysis could not take.
    warnings: Vec<String>,
    /// The sidecar files as they were when the session loaded them; `None` without a project.
    stamp: Option<Vec<(PathBuf, Option<SystemTime>)>>,
    /// The lockstep session, when one was started.
    lockstep: &'a RefCell<Option<Lockstep>>,
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
                return Ok(Outcome::Reload(None));
            }
            // A checkout or a merge changed the annotations: the answer takes the new names.
            if let (Some(project), Some(stamp)) = (&self.target.project, &self.stamp)
                && *stamp != annotations::stamp(&project.root)
            {
                return Ok(Outcome::Reload(Some(line)));
            }
            let (response, changed) = match self.dispatch(method, &params) {
                Ok(result) => (json!({"jsonrpc": "2.0", "id": id, "result": result}), method == "annotate"),
                Err(e) => {
                    let code = if e.to_string().starts_with("no method") { -32601 } else { -32602 };
                    (json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": format!("{e:#}")}}), false)
                }
            };
            write_response(&mut out, &response)?;
            // A rename changes the analysis every later answer reads.
            if changed {
                return Ok(Outcome::Reload(None));
            }
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
            "hand" => {
                let Some(project) = &self.target.project else { bail!("no project: the hand-written functions are a project's") };
                let query = params.get("query").and_then(Value::as_str).ok_or_else(|| anyhow!("the parameter `query` is missing"))?;
                let owned = params.get("owned").and_then(Value::as_bool).ok_or_else(|| anyhow!("the parameter `owned` is missing"))?;
                let function = self.find(query)?;
                let hand = project.set_hand(&function.name, owned)?;
                Ok(json!({"name": function.name, "owned": owned, "hand": hand}))
            }
            "annotate" => {
                let query = params.get("query").and_then(Value::as_str).ok_or_else(|| anyhow!("the parameter `query` is missing"))?;
                self.annotate(self.find(query)?, params)
            }
            "annotations" => Ok(json!({
                "dir": self.target.project.as_ref().map(|p| annotations::dir(&p.root).display().to_string()),
                "annotations": self.annotations.values().map(Annotation::to_json).collect::<Vec<_>>(),
                "warnings": self.warnings,
            })),
            "plugins" => Ok(crate::plugins::list_json(self.target.project.as_ref().map(|p| p.root.as_path()))),
            "trace" => {
                let Some(project) = &self.target.project else { bail!("no project: the traces are a project's") };
                let function = params.get("function").and_then(Value::as_str);
                let from = params.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
                let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                Ok(trace_json(project, function, from, limit))
            }
            "lockstep.start" => {
                let Some(project) = &self.target.project else { bail!("no project: the lockstep runs a project's port") };
                // The session before goes first: the interpreter runs one program at a time.
                self.lockstep.borrow_mut().take();
                let session = Lockstep::start(project, self.image)?;
                let state = session.state();
                *self.lockstep.borrow_mut() = Some(session);
                Ok(state)
            }
            "lockstep.step" => {
                let function = params.get("function").and_then(Value::as_str);
                let count = params.get("count").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
                let divergence = params.get("divergence").and_then(Value::as_bool).unwrap_or(false);
                self.with_lockstep(|session| session.step(function, count, divergence))
            }
            "lockstep.state" => self.with_lockstep(|session| Ok(session.state())),
            "lockstep.read" => {
                let at = address("address")?;
                let size = params.get("size").and_then(Value::as_u64).unwrap_or(64);
                if size > crate::lockstep::MAX_READ {
                    bail!("a read is at most {} bytes", crate::lockstep::MAX_READ);
                }
                self.with_lockstep(|session| Ok(session.read(at, size)))
            }
            "lockstep.stop" => Ok(json!({"stopped": self.lockstep.borrow_mut().take().is_some()})),
            other => Err(anyhow!("no method `{other}`")),
        }
    }

    /// Answers from the lockstep session, or refuses when none was started.
    fn with_lockstep(&self, answer: impl FnOnce(&mut Lockstep) -> Result<Value>) -> Result<Value> {
        let mut session = self.lockstep.borrow_mut();
        let Some(session) = session.as_mut() else { bail!("no lockstep session: start one with `lockstep.start`") };
        answer(session)
    }

    /// The `hand` list of the project as `rhipper.json` has it now, which the `hand` method
    /// changes after the project was loaded.
    fn hand_list(&self) -> Vec<String> {
        let Some(project) = &self.target.project else { return Vec::new() };
        Project::load(&project.root).map_or_else(|_| project.hand.clone(), |p| p.hand)
    }

    /// Whether the hand-written sources own `function`: the list names it or its address, or its
    /// annotation takes it over.
    fn is_hand(&self, function: &ra_asm_analysis::Function) -> bool {
        let address = crate::report::hex(function.entry);
        self.annotations.get(&function.entry).is_some_and(|a| a.hand)
            || self.hand_list().iter().any(|h| h == &function.name || crate::project::same_function(h, &address))
    }

    /// Changes the annotation of `function` by the keys `params` has: `name`, `type`, `comment`
    /// and `notes` set a text and clear it with `null` or `""`, `hand` sets the takeover. Writes
    /// the sidecar file, which the next emit reads, and answers with the annotation.
    fn annotate(&self, function: &ra_asm_analysis::Function, params: &Value) -> Result<Value> {
        let Some(project) = &self.target.project else { bail!("no project: the annotations are a project's") };
        let mut annotation = self
            .annotations
            .get(&function.entry)
            .cloned()
            .unwrap_or_else(|| Annotation { address: function.entry, ..Annotation::default() });
        let text = |key: &str| -> Result<Option<Option<String>>> {
            match params.get(key) {
                None => Ok(None),
                Some(Value::Null) => Ok(Some(None)),
                Some(Value::String(s)) => Ok(Some(Some(s.clone()).filter(|s| !s.is_empty()))),
                Some(_) => bail!("the parameter `{key}` is no string"),
            }
        };
        if let Some(name) = text("name")? {
            if let Some(name) = &name {
                if matches!(function.kind, ra_asm_analysis::Kind::Runtime(_)) {
                    bail!("the host's runtime supplies {}: the port calls it by the host's name", function.name);
                }
                if let Some(fault) = annotations::name_fault(self.analysis, function.entry, name) {
                    bail!("{fault}");
                }
            }
            annotation.name = name;
        }
        if let Some(ty) = text("type")? {
            annotation.ty = ty;
        }
        if let Some(comment) = text("comment")? {
            annotation.comment = comment;
        }
        if let Some(notes) = text("notes")? {
            annotation.notes = notes;
        }
        if let Some(hand) = params.get("hand") {
            annotation.hand = hand.as_bool().ok_or_else(|| anyhow!("the parameter `hand` is no boolean"))?;
        }
        // The function apart from its address, which the next build of the reference finds it by.
        annotation.identity = ra_asm_analysis::identity::identities(self.analysis).remove(&function.entry);
        let file = annotations::write(&project.root, &annotation)?;
        Ok(json!({"file": file.display().to_string(), "written": !annotation.is_empty(), "annotation": annotation.to_json()}))
    }

    /// One function by name, address or an address inside it.
    fn find(&self, query: &str) -> Result<&ra_asm_analysis::Function> {
        self.analysis
            .functions
            .values()
            .find(|f| f.name == query)
            .or_else(|| {
                let a = parse_address(query).ok()?;
                self.analysis.function(a).or_else(|| self.analysis.containing(a))
            })
            .ok_or_else(|| anyhow!("no function matches `{query}`"))
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
            "sources": project.sources.iter().map(|s| s.display().to_string()).collect::<Vec<_>>(),
            "hand": self.hand_list(),
            "annotations": {
                "dir": annotations::dir(&project.root).display().to_string(),
                "count": self.annotations.len(), "warnings": self.warnings,
            },
            "build": {
                "dir": project.build.dir.display().to_string(), "config": project.build.config, "target": project.build.target,
                "generator": project.build.generator.id(),
            },
            "port": project.port_binary(self.image.headers.coff.is_dll()).display().to_string(),
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
        let function = self.find(query)?;
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
            // Whether the hand-written sources own it now, which the table says only after an emit.
            result["hand"] = json!(self.is_hand(function));
            result["annotation"] = self.annotations.get(&function.entry).map_or(Value::Null, Annotation::to_json);
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

/// The replay of the project's two traces (see [`crate::trace::replay`]): the calls of `function`
/// (every function without one) from the `from`th, at most `limit`, each with the port's side and
/// the reference's; the functions called, in order of first call, with their counts; the verdict;
/// and `parts`, the call at fault with the two values, when the traces part.
fn trace_json(project: &Project, function: Option<&str>, from: usize, limit: usize) -> Value {
    use crate::trace::{Call, Side, Verdict};
    let log_dir = project.log_dir();
    let (port, reference) = (log_dir.join("port.trace"), log_dir.join("reference.trace"));
    let replay = crate::trace::replay(&port, &reference);
    let side = |s: &Side| json!({"args": s.args, "result": s.result});
    let call_json = |index: usize, c: &Call| {
        json!({
            "index": index, "function": c.function, "depth": c.depth, "enter": c.enter, "return": c.ret,
            "port": side(&c.port), "reference": c.reference.as_ref().map(side), "fault": replay.fault == Some(index),
        })
    };
    let selected: Vec<(usize, &Call)> = replay.calls.iter().enumerate().filter(|(_, c)| function.is_none_or(|f| c.function == f)).collect();
    let total = selected.len();
    let calls: Vec<Value> = selected.into_iter().skip(from).take(limit).map(|(i, c)| call_json(i, c)).collect();
    let mut functions: Vec<(&str, usize)> = Vec::new();
    for c in &replay.calls {
        match functions.iter_mut().find(|(name, _)| *name == c.function) {
            Some(entry) => entry.1 += 1,
            None => functions.push((&c.function, 1)),
        }
    }
    let parts = match &replay.verdict {
        Verdict::Parts { event, function, detail, values } => json!({
            "event": event, "function": function, "detail": detail,
            "call": replay.fault.map(|i| call_json(i, &replay.calls[i])),
            "port": values.as_ref().map(|v| &v.0), "reference": values.as_ref().map(|v| &v.1),
        }),
        Verdict::Same(_) | Verdict::NotTraced(_) => Value::Null,
    };
    // The last run's record says how the reference's trace was recorded.
    let tracer = std::fs::read_to_string(log_dir.join("runs.jsonl"))
        .ok()
        .and_then(|runs| runs.lines().last().and_then(|l| serde_json::from_str::<Value>(l).ok()))
        .and_then(|record| record.get("tracer").cloned())
        .unwrap_or(Value::Null);
    json!({
        "port": port.display().to_string(), "reference": reference.display().to_string(), "tracer": tracer,
        "verdict": replay.verdict.to_string(), "parts": parts,
        "functions": functions.iter().map(|(name, calls)| json!({"name": name, "calls": calls})).collect::<Vec<_>>(),
        "total": total, "calls": calls,
    })
}

fn write_response(out: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *out, response).context("writing the response")?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
