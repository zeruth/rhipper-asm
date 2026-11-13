//! `ras pipeline`: the steps from the reference binary to a running port, in a project with a
//! `rhipper.json`. The steps are `analyze`, `lift`, `emit`, `build` and `run`; each writes its log to
//! `build/pipeline/<step>.log`, and the pipeline prints one progress line per step, `[  12s] emit`,
//! which the workbench reads to show the step in work. A failed step prints
//! `[  12s] build failed: stopping` and the pipeline ends; the last line of a whole run is `done`.
//! Every run of the port appends a record to `build/pipeline/runs.jsonl`; with `run.compare` on,
//! the reference runs after the port (its output in `reference.log`) and the record and the log
//! carry a verdict: `same`, `differs at line N` or `port crashed`. With `run.trace` on, the port
//! is emitted with the trace hooks and the run step records its trace (`port.trace`) and the
//! reference's (`reference.trace`), under the debugger or through the IR interpreter as
//! `run.tracer` says, and the record and the log carry where the two part and in which function
//! (`trace`, `trace_function`, `trace_event`, `tracer`). A DLL is built as a shared
//! library and run through `run.exe`, the program that loads it, which finds the port first; the
//! comparison runs the same program finding the reference first. With `run.facts` on, the analyze
//! step runs the reference through the IR interpreter first and writes what it saw to
//! `build/facts.json`; every step analyzes with the facts there when they are of the reference.
//! The analyze, lift and emit steps stamp what they read and what they wrote in
//! `build/pipeline/<step>.stamp`: a step whose inputs and outputs are as the stamp says prints its
//! last summary with `(unchanged)` and does no work, and the emit rewrites only the files whose
//! text changed, so the build recompiles only those. In one run the lift takes over the analyze
//! step's analysis and the emit the lift's program. The build step writes the project of the
//! build system `build.generator` names when the root has none (`CMakeLists.txt`, a Visual Studio
//! solution or `meson.build`) and drives it, or compiles the port with the host's clang alone; a
//! Rust port is built by cargo. `--rebuild` runs every step again. With `--stats`, every step that
//! works prints its wall time and the peak working set of the process when it ended,
//! `analyze took 0.012 s, peak 23.4 MB`, and `build/pipeline/status.json` keeps the last numbers
//! of each step with their totals; a step its stamp skips keeps the numbers of the run that did it.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::annotations::annotate;
use crate::doctor::{self, Port};
use crate::project::{Generator, Project};
use crate::trace::{self, Tracer};

const CMAKE_LISTS: &str = include_str!("../templates/CMakeLists.txt");
const VCXPROJ: &str = include_str!("../templates/port.vcxproj");
const SOLUTION: &str = include_str!("../templates/port.sln");
const MESON_BUILD: &str = include_str!("../templates/meson.build");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Step {
    Analyze,
    Lift,
    Emit,
    Build,
    Run,
}

impl Step {
    pub(crate) const ALL: [Step; 5] = [Step::Analyze, Step::Lift, Step::Emit, Step::Build, Step::Run];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Step::Analyze => "analyze",
            Step::Lift => "lift",
            Step::Emit => "emit",
            Step::Build => "build",
            Step::Run => "run",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.id() == text)
    }
}

/// Which steps run, and how long the port may run.
#[derive(Debug, Clone, Default)]
pub(crate) struct Options {
    pub from: Option<Step>,
    pub only: Option<Step>,
    /// Stops the port after this many seconds and counts the run as alive; over `run.timeout`.
    pub timeout: Option<u64>,
    /// The build configuration, such as `Debug`, over `build.config` of the project.
    pub config: Option<String>,
    /// Runs every step whatever its stamp says, and deletes the build outputs before the build.
    pub rebuild: bool,
    /// Prints the wall time and the peak working set of every step and keeps them in the status file.
    pub stats: bool,
}

/// Runs the pipeline of the project at `root`.
pub(crate) fn run(root: &Path, options: &Options) -> Result<()> {
    let mut project = Project::load(root)?;
    if let Some(config) = &options.config {
        project.build.config.clone_from(config);
    }
    let steps: Vec<Step> = match (options.only, options.from) {
        (Some(step), _) => vec![step],
        (None, Some(first)) => Step::ALL.into_iter().filter(|s| *s >= first).collect(),
        (None, None) => Step::ALL.to_vec(),
    };
    fs::create_dir_all(project.log_dir())?;
    let progress = Progress { started: Instant::now() };
    let mut stats = options.stats.then(|| Stats::load(project.status_file()));
    let image = OnceCell::new();
    let mut pipeline =
        Pipeline { project, progress, image: &image, analysis: None, ported: None, timeout: options.timeout, rebuild: options.rebuild };
    for step in steps {
        pipeline.progress.line(step.id());
        let log = pipeline.project.log_dir().join(format!("{}.log", step.id()));
        let stamp = pipeline.project.log_dir().join(format!("{}.stamp", step.id()));
        // `--rebuild` still stamps what it does, so the next run can skip it.
        let key = inputs_key(&pipeline.project, step);
        if !options.rebuild
            && let Some(key) = &key
            && let Some(summary) = unchanged(&stamp, key)
        {
            let mut log = fs::OpenOptions::new().append(true).create(true).open(&log)?;
            writeln!(log, "unchanged since the last run: {summary}")?;
            println!("{summary} (unchanged)");
            continue;
        }
        // A step that fails part way leaves outputs its stamp no longer vouches for.
        let _ = fs::remove_file(&stamp);
        let mut log = File::create(&log).with_context(|| format!("cannot write {}", log.display()))?;
        let started = Instant::now();
        let result = match step {
            Step::Analyze => pipeline.analyze(&mut log).map(Some),
            Step::Lift => pipeline.lift(&mut log).map(Some),
            Step::Emit => pipeline.emit(&mut log).map(Some),
            Step::Build => pipeline.build(&mut log).map(|()| None),
            Step::Run => pipeline.run_port(&mut log).map(|()| None),
        };
        // A step that fails is measured too: what it cost before it stopped is what a person asks.
        if let Some(stats) = &mut stats {
            let line = stats.record(step, started.elapsed());
            let _ = writeln!(log, "{line}");
            println!("{line}");
            stats.write()?;
        }
        match result {
            Ok(done) => {
                if let (Some(key), Some(done)) = (key, done) {
                    fs::write(&stamp, done.stamp(&key)).with_context(|| format!("cannot write {}", stamp.display()))?;
                }
            }
            Err(e) => {
                let _ = writeln!(log, "error: {e:#}");
                eprintln!("error: {e:#}");
                pipeline.progress.line(&format!("{} failed: stopping", step.id()));
                bail!("{} failed", step.id());
            }
        }
    }
    pipeline.progress.line("done");
    Ok(())
}

struct Progress {
    started: Instant,
}

impl Progress {
    fn line(&self, text: &str) {
        println!("[{:>4}s] {text}", self.started.elapsed().as_secs());
        let _ = std::io::stdout().flush();
    }
}

/// The measures of `--stats`: per step, the wall time of its last run and the peak working set of
/// the process when it ended, as the status file holds them. The peak is the process's, so a
/// step's is never under that of the step before it in one run.
struct Stats {
    file: PathBuf,
    steps: BTreeMap<String, serde_json::Value>,
}

impl Stats {
    /// The measures the status file at `file` holds, which this run's steps replace one by one.
    fn load(file: PathBuf) -> Stats {
        let held = fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        let steps =
            held.as_ref().and_then(|v| v["steps"].as_object()).map(|steps| steps.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        Stats { file, steps: steps.unwrap_or_default() }
    }

    /// Takes the measure of `step`, which ran for `elapsed`, and gives the line that says it.
    fn record(&mut self, step: Step, elapsed: Duration) -> String {
        let peak = peak_working_set();
        self.steps.insert(step.id().to_owned(), serde_json::json!({ "seconds": elapsed.as_secs_f64(), "peak_bytes": peak }));
        let peak = peak.map_or_else(
            || "peak unknown on this host".to_owned(),
            |bytes| format!("peak {}.{} MB", bytes >> 20, ((bytes & 0xf_ffff) * 10) >> 20),
        );
        format!("{} took {:.3} s, {peak}", step.id(), elapsed.as_secs_f64())
    }

    /// Writes the status file: the steps, the sum of their times and the highest of their peaks.
    fn write(&self) -> Result<()> {
        let seconds: f64 = self.steps.values().filter_map(|s| s["seconds"].as_f64()).sum();
        let peak = self.steps.values().filter_map(|s| s["peak_bytes"].as_u64()).max();
        let status = serde_json::json!({ "steps": self.steps, "seconds": seconds, "peak_bytes": peak });
        let text = serde_json::to_string_pretty(&status)?;
        fs::write(&self.file, text + "\n").with_context(|| format!("cannot write {}", self.file.display()))
    }
}

/// The peak working set of this process in bytes, where the host tells it.
#[cfg(windows)]
fn peak_working_set() -> Option<u64> {
    use std::ffi::c_void;

    /// `PROCESS_MEMORY_COUNTERS`: the layout is the system's, and only the peak is read.
    #[repr(C)]
    #[allow(dead_code)]
    struct Counters {
        size: u32,
        page_faults: u32,
        peak_working_set: usize,
        rest: [usize; 7],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Counters, size: u32) -> i32;
    }

    let size = size_of::<Counters>() as u32;
    let mut counters = Counters { size, page_faults: 0, peak_working_set: 0, rest: [0; 7] };
    // SAFETY: the handle is the process's own, and `counters` is as large as the size says and
    // outlives the call.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) };
    (ok != 0).then_some(counters.peak_working_set as u64)
}

/// The peak resident set of this process in bytes, where `/proc` tells it.
#[cfg(not(windows))]
fn peak_working_set() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kilobytes = status.lines().find_map(|line| line.strip_prefix("VmHWM:"))?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()?;
    Some(kilobytes * 1024)
}

/// What a step that stamps its work did: its summary line, and every file it wrote with the
/// digest of what it wrote there.
struct Done {
    summary: String,
    outputs: Vec<(PathBuf, u64)>,
}

impl Done {
    /// The text of the step's stamp: the key of its inputs, the summary, then a line per output.
    fn stamp(&self, key: &str) -> String {
        use std::fmt::Write;
        let mut text = format!("{key}\n{}\n", self.summary);
        for (path, digest) in &self.outputs {
            let _ = writeln!(text, "{digest:016x} {}", path.display());
        }
        text
    }
}

/// The digest of an output's bytes, which its stamp holds.
fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// The program as the lift writes it and the emit translates it: the analysis with the person's
/// annotations, lifted standalone, raised to the ABI level and named. A run makes it once.
struct Ported<'a> {
    analysis: ra_asm_analysis::Analysis<'a>,
    annotations: BTreeMap<u64, crate::annotations::Annotation>,
    /// What the annotations could not apply.
    annotated: Vec<String>,
    lifted: ra_asm_lift::Lifted,
    signatures: HashMap<String, ra_asm_lift::raise::abi::Signature>,
}

struct Pipeline<'a> {
    project: Project,
    progress: Progress,
    /// The reference image, loaded by the first step that needs it.
    image: &'a OnceCell<ra_asm_pe::Image>,
    /// The analyze step's analysis, which the lift takes over rather than analyzing again.
    analysis: Option<ra_asm_analysis::Analysis<'a>>,
    /// The lift step's program, which the emit translates rather than lifting again.
    ported: Option<Ported<'a>>,
    timeout: Option<u64>,
    rebuild: bool,
}

impl<'a> Pipeline<'a> {
    fn image(&self) -> Result<&'a ra_asm_pe::Image> {
        let cell: &'a OnceCell<ra_asm_pe::Image> = self.image;
        if let Some(image) = cell.get() {
            return Ok(image);
        }
        let image = self.project.load_image()?;
        Ok(cell.get_or_init(|| image))
    }

    /// The program the lift and the emit share, made by the first of them that runs.
    fn ported(&mut self, log: &mut File) -> Result<&Ported<'a>> {
        if self.ported.is_none() {
            let image = self.image()?;
            let mut analysis = match self.analysis.take() {
                Some(analysis) => analysis,
                None => ra_asm_analysis::analyze_with(image, &self.facts(log)?),
            };
            let (annotations, annotated) = annotate(&self.project.root, &mut analysis);
            let options = ra_asm_lift::LiftOptions { standalone: true, x87: self.project.x87 };
            let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
            let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
            let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
            lifted.apply_names(&analysis, Some(&mut signatures));
            self.ported = Some(Ported { analysis, annotations, annotated, lifted, signatures });
        }
        Ok(self.ported.as_ref().expect("ported"))
    }

    /// The facts of a traced run of the reference, from `build/facts.json` when it is of the
    /// reference; empty otherwise, with the reason in the log.
    fn facts(&mut self, log: &mut File) -> Result<ra_asm_analysis::Facts> {
        let file = self.project.facts_file();
        if !file.is_file() {
            return Ok(ra_asm_analysis::Facts::default());
        }
        match crate::run::read_facts(&file, self.image()?) {
            Ok(facts) => {
                writeln!(
                    log,
                    "facts of a run of {} with {:?}: {} calls resolved, {} functions entered",
                    facts.input.exe,
                    facts.input.args,
                    facts.calls.len(),
                    facts.entered.len()
                )?;
                Ok(facts)
            }
            Err(reason) => {
                writeln!(log, "{reason}")?;
                Ok(ra_asm_analysis::Facts::default())
            }
        }
    }

    /// Runs the reference through the IR interpreter with the project's arguments and writes
    /// what the run saw to `build/facts.json`. The run is at the ABI level, the port's own shape,
    /// which runs a 32-bit program over its C runtime too.
    fn trace_facts(&mut self, log: &mut File) -> Result<()> {
        let (args, x87, file) = (self.project.run.args.clone(), self.project.x87, self.project.facts_file());
        let image = self.image()?;
        let analysis = ra_asm_analysis::analyze(image);
        let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let run = ra_asm_lift::run::Options { args, ..Default::default() };
        let outcome = ra_asm_lift::run::run(image, &lifted, &signatures, &run)?;
        let stop = outcome.stop.as_deref().unwrap_or("ran to its end");
        let line =
            format!("interpreted: exit {:#x}, {} steps, {stop}; {} calls resolved", outcome.exit, outcome.steps, outcome.facts.calls.len());
        writeln!(log, "{line}")?;
        println!("{line}");
        crate::run::write_facts(&file, &outcome.facts)
    }

    fn analyze(&mut self, log: &mut File) -> Result<Done> {
        let mut outputs = Vec::new();
        if self.project.run.facts {
            self.trace_facts(log)?;
            let file = self.project.facts_file();
            let bytes = fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
            outputs.push((file, digest(&bytes)));
        }
        let facts = self.facts(log)?;
        let image = self.image()?;
        let analysis = ra_asm_analysis::analyze_with(image, &facts);
        let warnings = log_warnings(log, image.warnings.iter().chain(&analysis.warnings));
        let summary = format!("{} functions, {} warnings", analysis.functions.len(), warnings);
        writeln!(log, "{summary}")?;
        println!("{summary}");
        self.analysis = Some(analysis);
        Ok(Done { summary, outputs })
    }

    fn lift(&mut self, log: &mut File) -> Result<Done> {
        let file = self.project.build_dir().join("program.ll");
        let Ported { analysis, annotated, lifted, .. } = self.ported(log)?;
        let text = lifted.module.to_ll();
        ra_asm_emit::write_if_changed(&file, text.as_bytes())?;
        let warnings = log_warnings(log, analysis.warnings.iter().chain(annotated).chain(&lifted.warnings));
        let summary = format!(
            "{} functions, {} imports, {} bytes of IR, {} warnings",
            lifted.functions.len(),
            lifted.imports.len(),
            text.len(),
            warnings
        );
        writeln!(log, "{summary}")?;
        println!("{summary}");
        Ok(Done { summary, outputs: vec![(file, digest(text.as_bytes()))] })
    }

    fn emit(&mut self, log: &mut File) -> Result<Done> {
        let gen_dir = self.project.gen_dir.clone();
        let build_dir = self.project.build_dir();
        let (lang, package) = (self.project.lang.clone(), self.project.build.target.clone());
        let (trace, hand) = (self.project.run.trace, self.project.hand.clone());
        let image = self.image()?;
        let Ported { analysis, annotations, annotated, lifted, signatures } = self.ported(log)?;
        let hand = crate::annotations::hand_list(&hand, annotations);
        let options = crate::emit::Options { lang: &lang, package: &package, trace, hand: &hand, annotations, checked: false };
        let translation = crate::emit::translate(image, analysis, lifted, signatures, &options)?;
        fs::create_dir_all(&build_dir)?;
        translation.write(&gen_dir)?;
        let header = ra_asm_lift::image_header(image, analysis, lifted, Some(signatures));
        ra_asm_emit::write_if_changed(&gen_dir.join("image.h"), header.as_bytes())?;
        let table = translation.table();
        ra_asm_emit::write_if_changed(&build_dir.join("functions.json"), table.as_bytes())?;
        // Every file the translation wrote, so a deleted or edited one runs the emit again.
        let written = translation
            .files
            .iter()
            .map(|f| (f.name.as_str(), f.text.as_bytes()))
            .chain(translation.runtime.iter().map(|(p, t)| (*p, t.as_bytes())))
            .chain(translation.resources.iter().map(|(p, b)| (p.as_str(), b.as_slice())))
            .chain(translation.exports.iter().map(|d| ("exports.def", d.as_bytes())))
            .chain([("image.h", header.as_bytes())]);
        let mut outputs: Vec<(PathBuf, u64)> = written.map(|(path, bytes)| (gen_dir.join(path), digest(bytes))).collect();
        outputs.push((build_dir.join("functions.json"), digest(table.as_bytes())));
        let regenerated = crate::emit::regenerate(&build_dir, analysis, &translation)?;
        let warnings = log_warnings(log, analysis.warnings.iter().chain(annotated).chain(&lifted.warnings).chain(&translation.warnings));
        if let Some(line) = regenerated {
            writeln!(log, "{line}")?;
            println!("{line}");
        }
        let summary = format!(
            "{} functions, {} bytes of {lang} in {} files, {} instructions not lifted, {} warnings -> {}",
            translation.functions.len(),
            translation.bytes(),
            translation.files.len(),
            translation.instructions_not_lifted(),
            warnings,
            gen_dir.display()
        );
        writeln!(log, "{summary}")?;
        println!("{summary}");
        Ok(Done { summary, outputs })
    }

    fn build(&mut self, log: &mut File) -> Result<()> {
        // The program the emit translated is not needed again, and the compiler wants the memory.
        self.analysis = None;
        self.ported = None;
        let project = &self.project;
        // The tools first: a missing one fails with its name, not with the build system's error.
        if project.is_rust() || project.build.generator == Generator::CMake {
            let port = if project.is_rust() { Port::Rust } else { Port::C };
            if let Some(failure) = doctor::check(port).failure() {
                bail!("{failure}");
            }
        }
        if project.is_rust() {
            // A Rust translation is a cargo package under gen; its outputs go under build/cargo.
            let mut cargo = Command::new("cargo");
            cargo
                .arg("build")
                .arg("--manifest-path")
                .arg(project.gen_dir.join("Cargo.toml"))
                .arg("--target-dir")
                .arg(project.build_dir().join("cargo"));
            if !project.build.config.eq_ignore_ascii_case("debug") {
                cargo.arg("--release");
            }
            let outcome = run_logged(&mut cargo, log, None, true)?;
            if outcome.code != Some(0) {
                bail!("cargo build exited with {}", outcome.exit());
            }
            return Ok(());
        }
        let image = self.image()?;
        let (bits, dll) = (u32::from(image.pointer_size()) * 8, image.headers.coff.is_dll());
        let project = &self.project;
        match project.build.generator {
            Generator::CMake => build_cmake(project, bits, dll, self.rebuild, log),
            Generator::VisualStudio => build_solution(project, bits, dll, self.rebuild, log),
            Generator::Meson => build_meson(project, bits, dll, self.rebuild, log),
            Generator::Clang => build_clang(project, bits, dll, log),
        }
    }

    fn run_port(&mut self, log: &mut File) -> Result<()> {
        let facts = self.facts(log)?;
        let image = self.image()?;
        let (dll, bits, image_base) = (image.headers.coff.is_dll(), u32::from(image.pointer_size()) * 8, image.image_base());
        let project = &self.project;
        let port = project.port_binary(dll);
        if !port.is_file() {
            bail!("no port at {}: build it first", port.display());
        }
        // A DLL runs in the program that loads it, `run.exe`, which finds the port in its own
        // directory before any other; a program is its own.
        let exe = if dll {
            if let Some(host) = &project.run.exe {
                host.clone()
            } else {
                let line = format!("the port is a DLL, {}, and run.exe names no program that loads it: nothing to run", port.display());
                writeln!(log, "{line}")?;
                println!("{line}");
                return Ok(());
            }
        } else {
            port.clone()
        };
        let timeout = self.timeout.or(project.run.timeout).map(Duration::from_secs);
        let log_dir = project.log_dir();
        let mut side: Vec<Child> = Vec::new();
        for command in &project.run.with {
            let args: Vec<String> = command.iter().map(|a| a.replace("{out}", &log_dir.to_string_lossy())).collect();
            let child = Command::new(&args[0])
                .args(&args[1..])
                .current_dir(&project.root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .with_context(|| format!("cannot start {}", args[0]))?;
            side.push(child);
        }
        let mut command = Command::new(&exe);
        command.args(&project.run.args).current_dir(&project.run.cwd);
        if dll && let Some(dir) = port.parent() {
            lead_dll_search(&mut command, dir);
        }
        // A traced port writes its trace where the environment says; a stale one from an earlier
        // run would pass for this run's if the port never started.
        let (port_trace, reference_trace) = (log_dir.join("port.trace"), log_dir.join("reference.trace"));
        if project.run.trace {
            let _ = fs::remove_file(&port_trace);
            let _ = fs::remove_file(&reference_trace);
            command.env("RHIPPER_TRACE", &port_trace);
        }
        let outcome = run_logged(&mut command, log, timeout, true);
        for mut child in side {
            let _ = child.kill();
            let _ = child.wait();
        }
        let outcome = outcome?;
        let mut record = serde_json::json!({
            "when": now_iso(),
            "commit": git(&project.root, &["rev-parse", "--short", "HEAD"]).unwrap_or_default(),
            "dirty_gen": git(&project.root, &["status", "--porcelain", "--", &project.gen_dir.to_string_lossy()]).is_some_and(|s| !s.is_empty()),
            "secs": outcome.secs,
            "alive": outcome.alive,
            "exit": outcome.exit(),
            "fault": outcome.fault(),
        });
        let mut summary =
            format!("exit {}{}, {} s", outcome.exit(), outcome.fault().map(|f| format!(" ({f})")).unwrap_or_default(), outcome.secs);
        if project.run.compare || project.run.trace {
            let reference_log = log_dir.join("reference.log");
            let mut reference_log = File::create(&reference_log).with_context(|| format!("cannot write {}", reference_log.display()))?;
            // The reference of a DLL is the same program loading the reference DLL instead.
            let mut reference = Command::new(if dll { &exe } else { &project.exe });
            reference.args(&project.run.args).current_dir(&project.run.cwd);
            if dll && let Some(dir) = project.exe.parent() {
                lead_dll_search(&mut reference, dir);
            }
            // The reference's trace comes from the debugger at the program's own functions or
            // from the interpreter over its IR; a DLL's run in its host is beyond both for now,
            // and a 32-bit program is beyond the debugger.
            let (tracer, not_traced) = if !project.run.trace {
                (None, None)
            } else if dll {
                (None, Some("the reference is a DLL in its host".to_owned()))
            } else {
                match project.run.tracer.unwrap_or(if bits == 32 { Tracer::Interpreter } else { Tracer::Debugger }) {
                    Tracer::Debugger if bits == 32 => {
                        (None, Some("the reference is a 32-bit program, which the debugger does not trace".to_owned()))
                    }
                    tracer => (Some(tracer), None),
                }
            };
            let reference = match tracer {
                Some(Tracer::Debugger) => {
                    let plan = trace::plan(&project.build_dir().join("functions.json"))?;
                    trace::run_reference(&mut reference, &plan, image_base, &reference_trace, &mut reference_log, timeout)?
                }
                Some(Tracer::Interpreter) => interpret_reference(image, &facts, project, &reference_trace, &mut reference_log)?,
                None => run_logged(&mut reference, &mut reference_log, timeout, false)?,
            };
            let verdict = Verdict::of(&outcome, &reference);
            record["verdict"] = serde_json::Value::String(verdict.to_string());
            record["differs_at"] = verdict.line().into();
            record["reference_exit"] = serde_json::Value::String(reference.exit());
            summary = format!("{summary}; reference exit {}, {} s; verdict: {verdict}", reference.exit(), reference.secs);
            if project.run.trace {
                let traces = match not_traced {
                    Some(reason) => trace::Verdict::NotTraced(reason),
                    None => trace::compare(&port_trace, &reference_trace),
                };
                record["trace"] = serde_json::Value::String(traces.to_string());
                record["trace_function"] = traces.function().into();
                record["trace_event"] = traces.event().into();
                record["tracer"] = tracer.map(Tracer::name).into();
                summary = format!("{summary}; trace: {traces}");
                if let Some(tracer) = tracer {
                    summary = format!("{summary} ({})", tracer.name());
                }
            }
        }
        let mut runs = fs::OpenOptions::new().create(true).append(true).open(log_dir.join("runs.jsonl"))?;
        writeln!(runs, "{record}")?;
        writeln!(log, "{summary}")?;
        println!("{summary}");
        if !outcome.alive && outcome.code != Some(0) {
            bail!("the port exited with {}", outcome.exit());
        }
        Ok(())
    }
}

/// How the port's run compares with the reference's: the standard outputs line by line, after
/// the port ended on its own or at the timeout. The error outputs are not compared: the runtime's
/// own messages go there.
enum Verdict {
    Same,
    /// The first line (from 1) where the outputs part; one past the shorter output when it is a prefix.
    DiffersAt(usize),
    PortCrashed,
}

impl Verdict {
    fn of(port: &Outcome, reference: &Outcome) -> Verdict {
        if port.fault().is_some() {
            return Verdict::PortCrashed;
        }
        let first = port.stdout.iter().zip(&reference.stdout).position(|(p, r)| p != r);
        match first {
            Some(i) => Verdict::DiffersAt(i + 1),
            None if port.stdout.len() == reference.stdout.len() => Verdict::Same,
            None => Verdict::DiffersAt(port.stdout.len().min(reference.stdout.len()) + 1),
        }
    }

    fn line(&self) -> Option<usize> {
        match self {
            Verdict::DiffersAt(n) => Some(*n),
            Verdict::Same | Verdict::PortCrashed => None,
        }
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Same => f.write_str("same"),
            Verdict::DiffersAt(n) => write!(f, "differs at line {n}"),
            Verdict::PortCrashed => f.write_str("port crashed"),
        }
    }
}

/// Runs the reference through the IR interpreter, lifted as the emit step lifts it so its
/// functions carry the port's names, with the record of the run written to `trace` in the shape
/// of the port's; the output goes to `log` as a run's, and the outcome reads as a process's.
fn interpret_reference(
    image: &ra_asm_pe::Image,
    facts: &ra_asm_analysis::Facts,
    project: &Project,
    trace: &Path,
    log: &mut File,
) -> Result<Outcome> {
    writeln!(log, "$ ras run --level abi --record {} {}", trace.display(), project.exe.display())?;
    let started = Instant::now();
    let run = ra_asm_lift::run::Options { args: project.run.args.clone(), record: Some(trace.to_path_buf()), ..Default::default() };
    let outcome = interpret(image, facts, project, &run)?;
    let text = String::from_utf8_lossy(&outcome.output).replace("\r\n", "\n");
    log.write_all(text.as_bytes())?;
    if let Some(stop) = &outcome.stop {
        writeln!(log, "ra: {stop}")?;
    }
    let stdout = text.lines().map(str::to_owned).collect();
    Ok(Outcome { code: Some(outcome.exit), alive: false, secs: started.elapsed().as_secs(), stdout })
}

/// Runs the reference's IR through the interpreter with `run`'s options: lifted as the emit step
/// lifts it, raised to the ABI level, with the port's names, so its functions are the port's.
pub(crate) fn interpret(
    image: &ra_asm_pe::Image,
    facts: &ra_asm_analysis::Facts,
    project: &Project,
    run: &ra_asm_lift::run::Options,
) -> Result<ra_asm_lift::run::Outcome> {
    let analysis = ra_asm_analysis::analyze_with(image, facts);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87: project.x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    ra_asm_lift::run::run(image, &lifted, &signatures, run)
}

/// Puts `dir` first on the `PATH` of `command`, so a DLL the program loads by name comes from
/// there when its own directory has none.
fn lead_dll_search(command: &mut Command, dir: &Path) {
    let mut dirs = vec![dir.to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Ok(path) = std::env::join_paths(dirs) {
        command.env("PATH", path);
    }
}

/// The key of what a step that reads the reference depends on: the project's `rhipper.json`, the
/// reference, its map and library signatures, the facts of its traced run, the annotations under
/// `rhipper/annotations` and the files of the plugins (the lift and the emit), and this `ras` itself
/// (its version and its binary's size and time, so a rebuilt engine runs the steps again).
/// `None` for the build, which `CMake` or cargo keeps incremental itself, and for the run, which
/// is the point of running.
fn inputs_key(project: &Project, step: Step) -> Option<String> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    if !matches!(step, Step::Analyze | Step::Lift | Step::Emit) {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    step.id().hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    let engine = std::env::current_exe().and_then(fs::metadata).ok();
    engine.as_ref().map(|m| (m.len(), m.modified().ok())).hash(&mut hasher);
    let mut files = vec![project.root.join(crate::project::RHIPPER_JSON), project.exe.clone()];
    files.extend(project.map.iter().cloned());
    files.extend(project.signatures.iter().cloned());
    files.extend(crate::signatures::store_files(&project.signature_stores()));
    // The analyze step with `run.facts` on writes the facts rather than reads them.
    if !(step == Step::Analyze && project.run.facts) {
        files.push(project.facts_file());
    }
    // The lift and the emit read the person's annotations, a file per function.
    if step != Step::Analyze
        && let Ok(entries) = fs::read_dir(crate::annotations::dir(&project.root))
    {
        let mut annotations: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        annotations.sort();
        files.extend(annotations);
    }
    // The plugins name the functions the lift and the emit write, from their files.
    if step != Step::Analyze {
        for plugin in crate::plugins::discover(Some(&project.root)) {
            if let Ok(entries) = fs::read_dir(&plugin.dir) {
                let mut own: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
                own.sort();
                files.extend(own);
            }
        }
    }
    for file in files {
        file.hash(&mut hasher);
        fs::read(&file).ok().hash(&mut hasher);
    }
    Some(format!("{:016x}", hasher.finish()))
}

/// The summary of the last run of a step when its stamp holds `key` and every output it names
/// still holds what the step wrote: the step would write the same again.
fn unchanged(stamp: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(stamp).ok()?;
    let mut lines = text.lines();
    if lines.next()? != key {
        return None;
    }
    let summary = lines.next()?.to_owned();
    for line in lines {
        let (stamped, path) = line.split_once(' ')?;
        let bytes = fs::read(path).ok()?;
        if format!("{:016x}", digest(&bytes)) != stamped {
            return None;
        }
    }
    Some(summary)
}

/// Writes the warnings to the log and returns how many there were.
fn log_warnings<'a>(log: &mut File, warnings: impl Iterator<Item = &'a String>) -> usize {
    let mut count = 0;
    for w in warnings {
        let _ = writeln!(log, "warning: {w}");
        count += 1;
    }
    count
}

/// Runs a step of a build and fails with its exit when it fails.
fn run_checked(command: &mut Command, log: &mut File, what: &str) -> Result<()> {
    let outcome = run_logged(command, log, None, true)?;
    if outcome.code != Some(0) {
        bail!("{what} exited with {}", outcome.exit());
    }
    Ok(())
}

/// Fails with the name and the use of every tool of `tools` the PATH does not have, as the
/// doctor does for a `CMake` build.
fn require(tools: &[(&str, &str)]) -> Result<()> {
    let missing: Vec<String> = tools.iter().filter(|(tool, _)| !on_path(tool)).map(|(tool, why)| format!("{tool} ({why})")).collect();
    if !missing.is_empty() {
        bail!("missing: {}", missing.join(", "));
    }
    Ok(())
}

/// Builds a C or C++ port through its `CMakeLists.txt`: configures a new build directory, or one
/// of a single-configuration generator for another configuration, and builds the target.
fn build_cmake(project: &Project, bits: u32, dll: bool, rebuild: bool, log: &mut File) -> Result<()> {
    write_cmake_lists(project, bits, dll)?;
    let cmake = "cmake";
    let dir = &project.build.dir;
    let cache = dir.join("CMakeCache.txt");
    if !cache.is_file() {
        let mut configure = Command::new(cmake);
        configure.arg("-S").arg(&project.root).arg("-B").arg(dir).args(generator_args(&project.build.config));
        run_checked(&mut configure, log, "cmake configure")?;
    } else if let Some(current) = single_config_type(&cache)
        && current != project.build.config
    {
        // A single-configuration generator (Ninja) builds the configuration it was configured
        // with: configure again for another.
        let mut configure = Command::new(cmake);
        configure.arg("-B").arg(dir).arg(format!("-DCMAKE_BUILD_TYPE={}", project.build.config));
        run_checked(&mut configure, log, "cmake configure")?;
    }
    let mut build = Command::new(cmake);
    build.arg("--build").arg(dir).args(["--config", &project.build.config, "--parallel", "--target", &project.build.target]);
    if rebuild {
        build.arg("--clean-first");
    }
    run_checked(&mut build, log, "cmake build")
}

/// Builds a C or C++ port through its Visual Studio solution, with the `msbuild` of the PATH or
/// of the newest Visual Studio. The `ClangCL` toolset compiles it with the LLVM of the PATH when
/// there is one, the clang the oracles use, rather than the one Visual Studio bundles.
fn build_solution(project: &Project, bits: u32, dll: bool, rebuild: bool, log: &mut File) -> Result<()> {
    let Some(msbuild) = msbuild() else {
        bail!("missing: msbuild (the build of a Visual Studio solution: Visual Studio with the C++ tools and the ClangCL toolset)");
    };
    let solution = write_solution(project, bits, dll)?;
    let mut build = Command::new(msbuild);
    build
        .arg(&solution)
        .args(["-nologo", "-verbosity:minimal", "-maxCpuCount", "-nodeReuse:false", "-p:Platform=x64"])
        .arg(format!("-p:Configuration={}", project.build.config))
        // A worker node left alive would hold the log's pipes open after the build.
        .env("MSBUILDDISABLENODEREUSE", "1");
    if let Some((dir, version)) = llvm_install() {
        build.arg(format!("-p:LLVMInstallDir={}", dir.display()));
        if let Some(version) = version {
            build.arg(format!("-p:LLVMToolsVersion={version}"));
        }
    }
    if rebuild {
        build.arg("-t:Rebuild");
    }
    run_checked(&mut build, log, "msbuild")
}

/// The `msbuild` of the PATH, else of the newest Visual Studio that has one, through `vswhere`.
fn msbuild() -> Option<PathBuf> {
    if on_path("msbuild") {
        return Some(PathBuf::from("msbuild"));
    }
    let installer = std::env::var_os("ProgramFiles(x86)").or_else(|| std::env::var_os("ProgramFiles"))?;
    let vswhere = Path::new(&installer).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let output = Command::new(vswhere)
        .args(["-latest", "-products", "*", "-requires", "Microsoft.Component.MSBuild", "-find", r"MSBuild\**\Bin\MSBuild.exe"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.is_file())
}

/// The LLVM installation of the `clang-cl` on the PATH, with its tools version (its directory
/// under `lib/clang`), for the `ClangCL` toolset.
fn llvm_install() -> Option<(PathBuf, Option<String>)> {
    let path = std::env::var_os("PATH")?;
    let bin = std::env::split_paths(&path).find(|dir| dir.join("clang-cl.exe").is_file())?;
    let root = bin.parent()?.to_path_buf();
    let version = fs::read_dir(root.join("lib").join("clang"))
        .ok()
        .and_then(|entries| entries.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned())).max());
    Some((root, version))
}

/// Writes the Visual Studio solution and project of a project that has none, `<target>.sln` and
/// `<target>.vcxproj`, and returns the solution. Their configurations are Debug, Release and the
/// project's; `bits` and `dll` are as for [`write_cmake_lists`].
fn write_solution(project: &Project, bits: u32, dll: bool) -> Result<PathBuf> {
    let target = &project.build.target;
    let solution = project.root.join(format!("{target}.sln"));
    let vcxproj = project.root.join(format!("{target}.vcxproj"));
    let guid = guid(target);
    let mut configs = vec!["Debug".to_owned(), "Release".to_owned()];
    if !configs.contains(&project.build.config) {
        configs.push(project.build.config.clone());
    }
    if !vcxproj.exists() {
        let in_project = |dir: &Path| match dir.strip_prefix(&project.root) {
            Ok(relative) if relative.as_os_str().is_empty() => "$(MSBuildProjectDirectory)".to_owned(),
            Ok(relative) => format!("$(MSBuildProjectDirectory)\\{}", xml(&relative.to_string_lossy().replace('/', "\\"))),
            Err(_) => xml(&dir.to_string_lossy().replace('/', "\\")),
        };
        let configurations: Vec<String> = configs
            .iter()
            .map(|c| {
                let c = xml(c);
                format!(
                    "    <ProjectConfiguration Include=\"{c}|x64\">\n      <Configuration>{c}</Configuration>\n      \
                     <Platform>x64</Platform>\n    </ProjectConfiguration>"
                )
            })
            .collect();
        // A hand-written file's object goes under its own directory, so it cannot take the name of
        // the translation's file of the same name.
        let hand: Vec<String> = project
            .sources
            .iter()
            .map(|dir| {
                let dir = in_project(dir);
                format!(
                    "    <ClCompile Include=\"{dir}\\**\\*.c;{dir}\\**\\*.cpp\" Exclude=\"$(RaGenDir)\\**\">\n      \
                     <ObjectFileName>$(IntDir)hand\\%(RecursiveDir)</ObjectFileName>\n    </ClCompile>"
                )
            })
            .collect();
        let text = VCXPROJ
            .replace("@NAME@", &xml(&project.name))
            .replace("@TARGET@", &xml(target))
            .replace("@GUID@", &guid)
            .replace("@CONFIGURATIONS@", &configurations.join("\n"))
            .replace("@GEN_DIR@", &in_project(&project.gen_dir))
            .replace("@OUT_DIR@", &in_project(&project.build.dir))
            .replace("@HAND_SOURCES@", &hand.join("\n"))
            .replace("@POINTER_BITS@", &bits.to_string())
            .replace("@DLL@", if dll { "true" } else { "false" });
        fs::write(&vcxproj, text).with_context(|| format!("cannot write {}", vcxproj.display()))?;
    }
    if !solution.exists() {
        let solution_configs: Vec<String> = configs.iter().map(|c| format!("\t\t{c}|x64 = {c}|x64")).collect();
        let project_configs: Vec<String> = configs
            .iter()
            .map(|c| format!("\t\t{{{guid}}}.{c}|x64.ActiveCfg = {c}|x64\n\t\t{{{guid}}}.{c}|x64.Build.0 = {c}|x64"))
            .collect();
        let text = SOLUTION
            .replace("@TARGET@", target)
            .replace("@GUID@", &guid)
            .replace("@SOLUTION_CONFIGURATIONS@", &solution_configs.join("\n"))
            .replace("@PROJECT_CONFIGURATIONS@", &project_configs.join("\n"));
        fs::write(&solution, text.replace("\r\n", "\n").replace('\n', "\r\n"))
            .with_context(|| format!("cannot write {}", solution.display()))?;
    }
    Ok(solution)
}

/// The project GUID of a target: the same for the same name, so a solution written again names
/// the project it named before.
fn guid(target: &str) -> String {
    let hex = format!("{:016X}{:016X}", digest(target.as_bytes()), digest(format!("{target}.vcxproj").as_bytes()));
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Builds a C or C++ port through its `meson.build` with clang: lists the sources for it,
/// configures a new build directory or one whose list changed, and compiles.
fn build_meson(project: &Project, bits: u32, dll: bool, rebuild: bool, log: &mut File) -> Result<()> {
    require(&[
        ("meson", "the build of a Meson project"),
        ("ninja", "the backend of Meson"),
        ("clang", "the C compiler of a C or C++ port"),
    ])?;
    let list = project.build_dir().join("meson-sources.txt");
    write_meson_build(project, &list, bits, dll)?;
    let text: String = port_sources(project)?.iter().fold(String::new(), |mut s, p| {
        s.push_str(&in_root(project, p));
        s.push('\n');
        s
    });
    let listed = fs::read_to_string(&list).is_ok_and(|old| old == text);
    if !listed {
        fs::create_dir_all(project.build_dir())?;
        fs::write(&list, &text).with_context(|| format!("cannot write {}", list.display()))?;
    }
    let dir = &project.build.dir;
    let buildtype = if project.build.config.eq_ignore_ascii_case("debug") { "debug" } else { "release" };
    let configured = dir.join("meson-private").is_dir();
    if !configured || !listed {
        let mut setup = Command::new("meson");
        setup.arg("setup");
        if configured {
            setup.arg("--reconfigure");
        }
        setup.arg(dir).arg(&project.root).arg(format!("--buildtype={buildtype}")).env("CC", "clang").env("CXX", "clang++");
        run_checked(&mut setup, log, "meson setup")?;
    } else {
        let mut configure = Command::new("meson");
        configure.arg("configure").arg(dir).arg(format!("-Dbuildtype={buildtype}"));
        run_checked(&mut configure, log, "meson configure")?;
    }
    if rebuild {
        let mut clean = Command::new("meson");
        clean.args(["compile", "--clean", "-C"]).arg(dir);
        run_checked(&mut clean, log, "meson compile --clean")?;
    }
    let mut compile = Command::new("meson");
    compile.args(["compile", "-C"]).arg(dir);
    run_checked(&mut compile, log, "meson compile")
}

/// Writes the `meson.build` of a project that has none, over the list of sources at `list`.
fn write_meson_build(project: &Project, list: &Path, bits: u32, dll: bool) -> Result<()> {
    let file = project.root.join("meson.build");
    if file.exists() {
        return Ok(());
    }
    let quoted = |text: &str| text.replace('\\', "\\\\").replace('\'', "\\'");
    let text = MESON_BUILD
        .replace("@NAME@", &project.name)
        .replace("@TARGET@", &quoted(&project.build.target))
        .replace("@GEN_DIR@", &quoted(&in_root(project, &project.gen_dir)))
        .replace("@SOURCE_LIST@", &quoted(&in_root(project, list)))
        .replace("@POINTER_BITS@", &bits.to_string())
        .replace("@DLL@", if dll { "true" } else { "false" });
    fs::write(&file, text).with_context(|| format!("cannot write {}", file.display()))
}

/// Builds a C or C++ port with the host's clang and no build system: one command compiles the
/// translation, its runtime and the hand-written sources and links the port in the build
/// directory, on Windows with lld and elsewhere against the C library the driver binds the
/// program's imports from.
fn build_clang(project: &Project, bits: u32, dll: bool, log: &mut File) -> Result<()> {
    let mut tools = vec![("clang", "the C compiler of a C or C++ port")];
    if cfg!(windows) {
        tools.push(("lld-link", "the linker of a C or C++ port"));
    }
    require(&tools)?;
    let sources = port_sources(project)?;
    let gen_dir = &project.gen_dir;
    let dir = &project.build.dir;
    fs::create_dir_all(dir).with_context(|| format!("cannot write {}", dir.display()))?;
    let name =
        if dll { format!("{}.dll", project.build.target) } else { format!("{}{}", project.build.target, std::env::consts::EXE_SUFFIX) };
    let mut clang = Command::new("clang");
    clang.args(["-O1", "-Wall", "-Wno-unused-variable", "-Wno-unused-but-set-variable", "-Wno-parentheses"]);
    if project.build.config.eq_ignore_ascii_case("debug") {
        clang.arg("-g");
    }
    clang.arg("-I").arg(gen_dir).arg("-I").arg(gen_dir.join("runtime")).args(&sources);
    if cfg!(windows) {
        clang.args(["-fuse-ld=lld", "-lkernel32", "-luser32"]);
        // A 32-bit program keeps its 32-bit pointers in the port, which is not large-address-aware.
        if bits == 32 {
            clang.arg("-Wl,/largeaddressaware:no");
        }
        let resources = gen_dir.join("resources.res");
        if resources.is_file() {
            clang.arg(resources);
        }
        if dll {
            clang.arg("-shared").arg(format!("-Wl,/def:{}", gen_dir.join("exports.def").display()));
        }
    } else {
        if dll {
            clang.args(["-shared", "-fPIC"]);
        }
        clang.args(["-ldl", "-lm"]);
        if sources.iter().any(|s| s.extension().is_some_and(|e| e == "cpp")) {
            clang.arg(if cfg!(target_os = "macos") { "-lc++" } else { "-lstdc++" });
        }
    }
    clang.arg("-o").arg(dir.join(name));
    run_checked(&mut clang, log, "clang")
}

/// The sources of a C or C++ port: the translation's files under `gen` and its driver, then the
/// hand-written ones under `sources` but those under `gen`, each in name order.
fn port_sources(project: &Project) -> Result<Vec<PathBuf>> {
    fn c_or_cpp(path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "c" || e == "cpp")
    }
    fn walk(dir: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.starts_with(skip) {
                continue;
            }
            if path.is_dir() {
                walk(&path, skip, out);
            } else if c_or_cpp(&path) {
                out.push(path);
            }
        }
    }
    let gen_dir = &project.gen_dir;
    let entries = fs::read_dir(gen_dir).with_context(|| format!("no translation at {}: run the emit first", gen_dir.display()))?;
    let mut sources: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file() && c_or_cpp(p)).collect();
    sources.sort();
    sources.push(gen_dir.join("runtime").join("driver.c"));
    let mut hand = Vec::new();
    for dir in &project.sources {
        walk(dir, gen_dir, &mut hand);
    }
    hand.sort();
    sources.extend(hand);
    Ok(sources)
}

/// A path as a build file at the project's root names it: relative to the root when under it,
/// else whole, with forward slashes.
fn in_root(project: &Project, path: &Path) -> String {
    match path.strip_prefix(&project.root) {
        Ok(relative) => slashes(relative),
        Err(_) => slashes(path),
    }
}

/// Writes the `CMakeLists.txt` of a project that has none. `bits` is the pointer width of the
/// program, which sets how the port is linked, and `dll` whether it is a shared library.
fn write_cmake_lists(project: &Project, bits: u32, dll: bool) -> Result<()> {
    let file = project.root.join("CMakeLists.txt");
    if file.exists() {
        return Ok(());
    }
    let in_project = |dir: &Path| match dir.strip_prefix(&project.root) {
        Ok(relative) => format!("${{CMAKE_CURRENT_SOURCE_DIR}}/{}", slashes(relative)),
        Err(_) => slashes(dir),
    };
    let sources: Vec<String> = project.sources.iter().map(|s| format!("\"{}\"", in_project(s))).collect();
    let text = CMAKE_LISTS
        .replace("@NAME@", &project.name)
        .replace("@TARGET@", &project.build.target)
        .replace("@GEN_DIR@", &in_project(&project.gen_dir))
        .replace("@SOURCES@", &sources.join(" "))
        .replace("@POINTER_BITS@", &bits.to_string())
        .replace("@DLL@", if dll { "ON" } else { "OFF" });
    fs::write(&file, text).with_context(|| format!("cannot write {}", file.display()))
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The `CMAKE_BUILD_TYPE` of a build directory whose generator builds one configuration (Ninja,
/// Makefiles); `None` for a multi-configuration generator such as Visual Studio.
fn single_config_type(cache: &Path) -> Option<String> {
    let text = fs::read_to_string(cache).ok()?;
    let generator = text.lines().find_map(|l| l.strip_prefix("CMAKE_GENERATOR:INTERNAL="))?;
    if generator.starts_with("Visual Studio") || generator == "Xcode" || generator.contains("Multi-Config") {
        return None;
    }
    Some(text.lines().find_map(|l| l.strip_prefix("CMAKE_BUILD_TYPE:STRING=")).unwrap_or("").to_owned())
}

/// The generator and the compiler of a new build directory: the environment's `CMAKE_GENERATOR`
/// when set; else Ninja with clang when both are on the PATH, else Visual Studio with clang-cl,
/// else the default generator. The translation needs a clang front end.
fn generator_args(config: &str) -> Vec<String> {
    if std::env::var_os("CMAKE_GENERATOR").is_some() {
        return Vec::new();
    }
    if on_path("ninja") && on_path("clang") {
        return vec![
            "-G".into(),
            "Ninja".into(),
            "-DCMAKE_C_COMPILER=clang".into(),
            "-DCMAKE_CXX_COMPILER=clang++".into(),
            format!("-DCMAKE_BUILD_TYPE={config}"),
        ];
    }
    if on_path("clang-cl") {
        return vec!["-A".into(), "x64".into(), "-T".into(), "ClangCL".into()];
    }
    vec!["-A".into(), "x64".into()]
}

fn on_path(tool: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| dir.join(format!("{tool}.exe")).is_file() || dir.join(tool).is_file())
}

/// The end of a process the pipeline ran.
pub(crate) struct Outcome {
    /// The exit code; `None` when the pipeline stopped the process.
    pub(crate) code: Option<i32>,
    /// Whether the process ran until the pipeline stopped it at the timeout.
    pub(crate) alive: bool,
    pub(crate) secs: u64,
    /// The lines of the standard output, for the comparison with the reference.
    pub(crate) stdout: Vec<String>,
}

impl Outcome {
    /// The exit code as text: decimal, or `0x` and eight digits for a status such as `0xc0000005`.
    fn exit(&self) -> String {
        match self.code {
            None => "stopped".to_owned(),
            Some(code) if !(0..=0xFFFF).contains(&code) => format!("0x{:08x}", code as u32),
            Some(code) => code.to_string(),
        }
    }

    /// What ended the process, when the exit code is a Windows status or a runtime stop.
    fn fault(&self) -> Option<String> {
        let code = self.code? as u32;
        let name = match code {
            0xC000_0005 => "access violation",
            0xC000_001D => "illegal instruction",
            0xC000_0094 => "integer divide by zero",
            0xC000_00FD => "stack overflow",
            0xC000_0409 => "stack buffer overrun",
            0xC000_0374 => "heap corruption",
            0xE000_0001 => "trap",
            0xE000_0002 => "an instruction the lift does not cover",
            0xE000_0003 => "an indirect call to no lifted function",
            0xE000_0004 => "an import that did not bind",
            0xC000_0000..=0xFFFF_FFFF => "exception",
            _ => return None,
        };
        Some(format!("{code:08x} {name}"))
    }
}

/// The threads reading a child's output, and the standard output lines they keep.
pub(crate) struct Readers {
    threads: Vec<std::thread::JoinHandle<()>>,
    stdout: Arc<Mutex<Vec<String>>>,
}

/// Runs `command` with its output written to `log`, and with `echo` to the pipeline's own output
/// as it comes, stopping it after `timeout`.
fn run_logged(command: &mut Command, log: &mut File, timeout: Option<Duration>, echo: bool) -> Result<Outcome> {
    let started = Instant::now();
    let (mut child, readers) = spawn_logged(command, log, echo)?;
    let mut alive = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            // A program with a window gets the close its user would give it, so its message loop
            // ends and it leaves with its own code; one that does not leave on that is killed and
            // reported alive.
            close_windows(child.id());
            let grace = Instant::now();
            let closed = loop {
                if let Some(status) = child.try_wait()? {
                    break Some(status);
                }
                if grace.elapsed() >= Duration::from_secs(2) {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            if let Some(status) = closed {
                break status;
            }
            alive = true;
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(collect(status, alive, started, readers))
}

/// Starts `command` with its output read into `log` (and echoed when `echo`) by threads; the
/// caller waits for the child and then [`collect`]s the outcome.
pub(crate) fn spawn_logged(command: &mut Command, log: &mut File, echo: bool) -> Result<(Child, Readers)> {
    let program = command.get_program().to_string_lossy().into_owned();
    let line =
        std::iter::once(program.clone()).chain(command.get_args().map(|a| a.to_string_lossy().into_owned())).collect::<Vec<_>>().join(" ");
    writeln!(log, "$ {line}")?;
    if echo {
        println!("$ {line}");
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;
    let sink = Arc::new(Mutex::new(log.try_clone()?));
    let stdout = Arc::new(Mutex::new(Vec::new()));
    let readers: Vec<_> = [
        child.stdout.take().map(|s| (Box::new(s) as Box<dyn std::io::Read + Send>, Some(Arc::clone(&stdout)))),
        child.stderr.take().map(|s| (Box::new(s) as Box<dyn std::io::Read + Send>, None)),
    ]
    .into_iter()
    .flatten()
    .map(|(stream, kept)| {
        let sink = Arc::clone(&sink);
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if echo {
                    println!("{line}");
                }
                if let Ok(mut log) = sink.lock() {
                    let _ = writeln!(log, "{line}");
                }
                if let Some(kept) = &kept
                    && let Ok(mut kept) = kept.lock()
                {
                    kept.push(line);
                }
            }
        })
    })
    .collect();
    Ok((child, Readers { threads: readers, stdout }))
}

/// The outcome of a child that ended with `status`, once its readers are done.
pub(crate) fn collect(status: std::process::ExitStatus, alive: bool, started: Instant, readers: Readers) -> Outcome {
    for reader in readers.threads {
        let _ = reader.join();
    }
    let _ = std::io::stdout().flush();
    let stdout = readers.stdout.lock().map(|lines| lines.clone()).unwrap_or_default();
    Outcome { code: if alive { None } else { status.code() }, alive, secs: started.elapsed().as_secs(), stdout }
}

/// Posts `WM_CLOSE` to every top-level window of the process `pid`, as closing it by hand would.
#[cfg(windows)]
fn close_windows(pid: u32) {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(callback: unsafe extern "system" fn(*mut c_void, isize) -> i32, lparam: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn PostMessageW(hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize) -> i32;
    }

    unsafe extern "system" fn close(hwnd: *mut c_void, pid: isize) -> i32 {
        const WM_CLOSE: u32 = 0x10;
        let mut owner = 0u32;
        // SAFETY: user32 hands a window handle to the callback, and `owner` outlives the call.
        unsafe {
            GetWindowThreadProcessId(hwnd, &raw mut owner);
            if owner == pid as u32 {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
        1
    }

    // SAFETY: the callback matches the signature user32 calls it with and touches nothing beyond
    // its arguments.
    unsafe {
        EnumWindows(close, pid as isize);
    }
}

#[cfg(not(windows))]
fn close_windows(_: u32) {}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(root).args(args).stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The time now as `2026-11-10T12:34:56Z`.
#[allow(clippy::many_single_char_names)]
fn now_iso() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// The project directory of the pipeline: `--project`, else the current directory.
pub(crate) fn project_dir(given: Option<&Path>) -> PathBuf {
    given.map_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), Path::to_path_buf)
}
