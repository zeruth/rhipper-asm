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
//! - `tools`: the tools the workbench gives a model, each a method here with the JSON schema of
//!   its parameters (`name`, `method`, `description`, `inputSchema`). The questions a model asks
//!   in a port are compositions of the engine's answers, and none calls a model:
//! - `explain` (`query`): what a function does: a `summary` line of its facts, its `callers`,
//!   `calls`, `tail_calls`, `imports`, `strings`, `virtuals`, the `data` it addresses and how
//!   (`read`, `write`, `address`), its `shape` (back edges, switches, unresolved jumps, indirect
//!   calls with the targets a traced run saw), `seen` at run time, its `traps` in the lift, and in
//!   a project its annotation, table entry, source and IR.
//! - `name_hints` (`query`): the facts a name is made from: the function's symbol, strings,
//!   imports, data and classes, and the name, strings and imports of each caller and callee,
//!   `named` when something other than its address names it.
//! - `writers` (`address`, `hi`): the functions that write the data in `[address, hi)`, every
//!   instruction that writes, reads or takes its address, and the pointers to it.
//! - `traps` (`query`): the functions whose lift traps, most first, each trap with its
//!   instruction, and the unresolved jumps; with `query`, that function alone. The lift is made at
//!   the first question and kept until the image changes.
//!
//! A project's server analyzes with the facts of its traced run (`build/facts.json`) when they
//! are of its reference, as the pipeline does.
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

use std::cell::{OnceCell, RefCell};
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
        // The facts of a traced run of the reference, as every step of the pipeline takes them.
        let facts = target
            .project
            .as_ref()
            .map(Project::facts_file)
            .filter(|f| f.is_file())
            .and_then(|f| crate::run::read_facts(&f, &image).ok())
            .unwrap_or_default();
        let mut analysis = ra_asm_analysis::analyze_with(&image, &facts);
        let (annotated, warnings) = match &target.project {
            Some(project) => annotations::annotate(&project.root, &mut analysis),
            None => (BTreeMap::new(), Vec::new()),
        };
        let stamp = target.project.as_ref().map(|p| annotations::stamp(&p.root));
        let session = Session {
            target,
            image: &image,
            analysis: &analysis,
            annotations: annotated,
            warnings,
            stamp,
            lockstep: &lockstep,
            lifted: OnceCell::new(),
            trap_sites: OnceCell::new(),
        };
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
    /// The machine-level lift of the program, made the first time an answer needs its traps.
    lifted: OnceCell<ra_asm_lift::Lifted>,
    /// The addresses the lift traps at, by function, from the lift above.
    trap_sites: OnceCell<BTreeMap<u64, Vec<u64>>>,
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
        let query = || params.get("query").and_then(Value::as_str).ok_or_else(|| anyhow!("the parameter `query` is missing"));
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
            "tools" => Ok(tools()),
            "explain" => Ok(self.explain(self.find(query()?)?)),
            "name_hints" => Ok(self.name_hints(self.find(query()?)?)),
            "writers" => {
                let lo = address("address")?;
                let hi = params.get("hi").and_then(Value::as_str).map(parse_address).transpose()?.unwrap_or(lo + 1);
                Ok(crate::xrefs::accesses_json(self.image, self.analysis, lo, hi))
            }
            "traps" => {
                let only = params.get("query").and_then(Value::as_str).map(|q| self.find(q)).transpose()?;
                Ok(self.traps(only))
            }
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
        self.translation(function, &mut result);
        Ok(result)
    }

    /// Adds to `result` what the project's outputs say of `function`: whether the hand-written
    /// sources own it, its annotation, its function table entry, its source and its raised IR.
    fn translation(&self, function: &ra_asm_analysis::Function, result: &mut Value) {
        let Some(project) = &self.target.project else { return };
        // Whether the hand-written sources own it now, which the table says only after an emit.
        result["hand"] = json!(self.is_hand(function));
        result["annotation"] = self.annotations.get(&function.entry).map_or(Value::Null, Annotation::to_json);
        // The function table entry, and the source of the function in the translation.
        if let Ok(text) = std::fs::read_to_string(project.build_dir().join("functions.json"))
            && let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&text)
            && let Some(entry) = entries.into_iter().find(|e| e.get("addr").and_then(Value::as_str) == Some(hex(function.entry).as_str()))
        {
            if let (Some(file), Some(line)) = (entry.get("file").and_then(Value::as_str), entry.get("line").and_then(Value::as_u64)) {
                let path = project.gen_dir.join(file);
                result["source_file"] = json!(path.display().to_string());
                result["source"] = json!(source_at(&path, line as usize));
            }
            result["table"] = entry;
        }
        // The IR and the source name a function as the analysis does.
        result["ir"] = json!(ir_of(&project.build_dir().join("program.ll"), &function.name));
    }

    /// The machine-level lift of the program, made once per load.
    fn lifted(&self) -> &ra_asm_lift::Lifted {
        self.lifted.get_or_init(|| ra_asm_lift::lift(self.image, self.analysis))
    }

    /// The addresses the lift traps at, by the entry of the function: each `ra_unsupported` call
    /// names the instruction it stands for.
    fn trap_sites(&self) -> &BTreeMap<u64, Vec<u64>> {
        use ra_asm_ir::{Callee, Inst, Operand};
        self.trap_sites.get_or_init(|| {
            let mut sites = BTreeMap::new();
            for f in &self.lifted().module.functions {
                let Some(entry) = f.address else { continue };
                let at: Vec<u64> = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instrs)
                    .filter_map(|i| match &i.inst {
                        Inst::Call { callee: Callee::Direct(name), args, .. } if name == "ra_unsupported" => match args.first() {
                            Some((_, Operand::Int(at))) => u64::try_from(*at).ok(),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect();
                if !at.is_empty() {
                    sites.insert(entry, at);
                }
            }
            sites
        })
    }

    /// The traps of `function` in the lift: each instruction the lift does not cover, by its
    /// address, mnemonic and text.
    fn traps_of(&self, function: &ra_asm_analysis::Function) -> Vec<Value> {
        let Some(sites) = self.trap_sites().get(&function.entry) else { return Vec::new() };
        sites
            .iter()
            .map(|&at| {
                let instr = function.blocks.iter().flat_map(|b| &b.instrs).find(|i| i.address == at);
                json!({"addr": hex(at), "mnemonic": instr.map(|i| &i.mnemonic), "text": instr.map(|i| &i.text)})
            })
            .collect()
    }

    /// What a function does, as the engine knows it: what it calls and what calls it, the imports,
    /// the strings and the data it addresses and how, the shape of its control, the calls a
    /// traced run resolved, its traps, and the project's annotation, source and raised IR.
    fn explain(&self, function: &ra_asm_analysis::Function) -> Value {
        let (strings, virtuals) = crate::functions::facts_json(function);
        let callers: Vec<Value> =
            self.analysis.callers(function.entry).iter().map(|c| json!({"addr": hex(c.entry), "name": c.name})).collect();
        let calls: Vec<Value> = function.calls.iter().map(|c| json!({"addr": hex(*c), "name": self.analysis.name_of(*c)})).collect();
        let data = crate::xrefs::data_of_json(self.image, function);
        let traps = self.traps_of(function);
        let seen = self.analysis.facts.entered.contains(&function.entry);
        let written: Vec<&str> = data
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["access"] == "write")
            .filter_map(|d| d["addr"].as_str())
            .collect();
        // One line of the facts, for a reader that wants the gist before the tables.
        let mut summary =
            vec![format!("{} at {}: {} blocks, {} instructions", function.name, hex(function.entry), function.blocks.len(), function.instr_count())];
        if !function.calls.is_empty() {
            summary.push(format!("calls {}", function.calls.iter().map(|c| self.analysis.name_of(*c)).collect::<Vec<_>>().join(", ")));
        }
        if !function.import_calls.is_empty() {
            summary.push(format!("calls the imports {}", function.import_calls.join(", ")));
        }
        if !function.strings.is_empty() {
            summary.push(format!("addresses the strings {}", function.strings.iter().map(|(_, s)| format!("{s:?}")).collect::<Vec<_>>().join(", ")));
        }
        if !written.is_empty() {
            summary.push(format!("writes the data at {}", written.join(", ")));
        }
        if function.noreturn {
            summary.push("never returns".to_owned());
        }
        if seen {
            summary.push("seen at run time".to_owned());
        }
        summary.push(if traps.is_empty() { "lifts without a trap".to_owned() } else { format!("{} traps in the lift", traps.len()) });
        let mut result = json!({
            "addr": hex(function.entry), "name": function.name, "symbol": function.symbol, "object": function.object,
            "kind": match &function.kind {
                ra_asm_analysis::Kind::Normal => json!("function"),
                ra_asm_analysis::Kind::ImportThunk(import) => json!({"thunk": import}),
                ra_asm_analysis::Kind::Runtime(name) => json!({"runtime": name}),
            },
            "summary": summary.join("; "),
            "size": function.size(), "blocks": function.blocks.len(), "instrs": function.instr_count(), "noreturn": function.noreturn,
            "origins": function.origins.iter().map(crate::functions::origin_name).collect::<Vec<_>>(), "seen": seen,
            "callers": callers, "calls": calls,
            "tail_calls": function.tail_calls.iter().map(|c| json!({"addr": hex(*c), "name": self.analysis.name_of(*c)})).collect::<Vec<_>>(),
            "imports": function.import_calls, "strings": strings, "virtuals": virtuals, "data": data,
            "shape": crate::functions::shape_json(self.analysis, function), "traps": traps,
        });
        self.translation(function, &mut result);
        result
    }

    /// The facts a name is made from: the function's own symbol, strings, imports, data and
    /// vtable slots, and the names and strings of its callers and its callees, each marked
    /// `named` when a symbol, a signature, a plugin or an annotation named it rather than its address.
    fn name_hints(&self, function: &ra_asm_analysis::Function) -> Value {
        let named = |name: &str| !name.starts_with("F_") && !name.starts_with("U_");
        let neighbour = |f: Option<&ra_asm_analysis::Function>, at: u64| {
            let name = f.map_or_else(|| self.analysis.name_of(at), |f| f.name.clone());
            json!({
                "addr": hex(at), "named": named(&name), "name": name,
                "strings": f.map(|f| f.strings.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>()).unwrap_or_default(),
                "imports": f.map(|f| f.import_calls.clone()).unwrap_or_default(),
            })
        };
        let (strings, virtuals) = crate::functions::facts_json(function);
        let mut classes: Vec<&str> = function.virtuals.iter().map(|v| v.class.as_str()).collect();
        classes.sort_unstable();
        classes.dedup();
        json!({
            "addr": hex(function.entry), "name": function.name, "named": named(&function.name), "symbol": function.symbol, "object": function.object,
            "annotation": self.annotations.get(&function.entry).map_or(Value::Null, Annotation::to_json),
            "strings": strings, "imports": function.import_calls, "virtuals": virtuals, "classes": classes,
            "data": crate::xrefs::data_of_json(self.image, function),
            "origins": function.origins.iter().map(crate::functions::origin_name).collect::<Vec<_>>(),
            "callers": self.analysis.callers(function.entry).into_iter().map(|c| neighbour(Some(c), c.entry)).collect::<Vec<_>>(),
            "calls": function.calls.iter().map(|c| neighbour(self.analysis.function(*c), *c)).collect::<Vec<_>>(),
        })
    }

    /// The functions that trap in the lift (of `only` alone when given), most traps first, each
    /// with every trap's instruction and the indirect jumps the analysis did not resolve; and the
    /// coverage of the whole program: the mnemonics not covered and the imports no prototype knows.
    fn traps(&self, only: Option<&ra_asm_analysis::Function>) -> Value {
        let mut rows: Vec<(usize, Value)> = Vec::new();
        for function in self.analysis.functions.values().filter(|f| only.is_none_or(|o| o.entry == f.entry)) {
            let traps = self.traps_of(function);
            let unresolved = crate::functions::shape_json(self.analysis, function)["unresolved_jumps"].take();
            if traps.is_empty() && unresolved.as_array().is_none_or(Vec::is_empty) && only.is_none() {
                continue;
            }
            let count = traps.len();
            rows.push((count, json!({"addr": hex(function.entry), "name": function.name, "count": count, "traps": traps, "unresolved_jumps": unresolved})));
        }
        rows.sort_by_key(|x| std::cmp::Reverse(x.0));
        let coverage = ra_asm_lift::coverage(self.analysis, self.lifted());
        json!({
            "total": coverage.traps(),
            "functions": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
            "uncovered": crate::report::coverage_json(&coverage)["uncovered"].take(),
            "unknown_imports": coverage.unknown_imports,
        })
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

/// The tools the workbench gives a model, each a method of the server with its parameters as a
/// JSON schema, as MCP lists a tool: what a model can ask of the engine, and nothing that calls a
/// model. The workbench reads its tool list from this answer.
fn tools() -> Value {
    const QUERY: (&str, &str, &str, bool) = ("query", "string", "a function: its name, its address, or an address inside it", true);
    const ADDRESS: (&str, &str, &str, bool) = ("address", "string", "an address of the image, hexadecimal with 0x", true);
    let tool = |name: &str, method: &str, description: &str, params: &[(&str, &str, &str, bool)]| {
        let properties: serde_json::Map<String, Value> =
            params.iter().map(|(key, ty, about, _)| ((*key).to_owned(), json!({"type": ty, "description": about}))).collect();
        let required: Vec<&str> = params.iter().filter(|p| p.3).map(|p| p.0).collect();
        json!({
            "name": name, "method": method, "description": description,
            "inputSchema": {"type": "object", "properties": properties, "required": required},
        })
    };
    json!({"tools": [
        tool("list_functions", "functions", "Every function of the program: address, name, size, calls, imports, strings, origins, and the classes of its RTTI.", &[]),
        tool("get_function", "function", "One function: the analysis, its strings and vtable slots, its source in the translation, its raised IR and its instructions.", &[QUERY]),
        tool(
            "explain_function",
            "explain",
            "What a function does, from the engine's facts: a summary line, its callers and callees, imports, strings, the data it reads and writes, its loops and switches, the indirect calls a traced run resolved, its traps, its annotation, source and IR.",
            &[QUERY],
        ),
        tool(
            "name_hints",
            "name_hints",
            "The facts to name a function from: its symbol, strings, imports, data and classes, and the names, strings and imports of its callers and callees, each marked when already named.",
            &[QUERY],
        ),
        tool(
            "find_writers",
            "writers",
            "The functions that write a global (or any address in [address, hi)), with every instruction that writes, reads or takes its address, and the pointers to it in the data.",
            &[ADDRESS, ("hi", "string", "the end of the range, exclusive; one byte when absent", false)],
        ),
        tool(
            "list_traps",
            "traps",
            "The functions whose lift traps, most traps first, with each instruction the lift does not cover and the indirect jumps left unresolved; or one function's.",
            &[("query", "string", "one function alone: its name or an address", false)],
        ),
        tool("find_callers", "callers", "Every call or jump to the function at an address, and every pointer to it in the data.", &[ADDRESS]),
        tool(
            "find_references",
            "xrefs",
            "Every instruction and data pointer that refers into [lo, hi).",
            &[("lo", "string", "the start of the range, hexadecimal", true), ("hi", "string", "the end of the range, exclusive", false)],
        ),
        tool(
            "list_strings",
            "strings",
            "The printable strings of the data sections.",
            &[("min", "integer", "the shortest string, 5 by default", false), ("limit", "integer", "at most this many, 1000 by default", false)],
        ),
        tool("get_cfg", "cfg", "The blocks of the function at or around an address, with their instructions and how each ends.", &[ADDRESS]),
        tool(
            "disassemble",
            "disasm",
            "The instructions from an address.",
            &[ADDRESS, ("length", "string", "the bytes to decode, hexadecimal", false)],
        ),
        tool("list_classes", "classes", "The classes of the RTTI: name, bases, and each vtable with its functions.", &[]),
        tool(
            "replay_trace",
            "trace",
            "The port's run against the reference's, call by call, with the first place they part.",
            &[
                ("function", "string", "the calls of this function alone", false),
                ("from", "integer", "from this call on", false),
                ("limit", "integer", "at most this many calls, 500 by default", false),
            ],
        ),
        tool(
            "annotate_function",
            "annotate",
            "Changes the project's annotation of a function: its name, type, comment or notes; an empty text clears one.",
            &[
                QUERY,
                ("name", "string", "the new name", false),
                ("type", "string", "the C type of the function", false),
                ("comment", "string", "the comment above it in the source", false),
                ("notes", "string", "the notes of the person", false),
            ],
        ),
    ]})
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
