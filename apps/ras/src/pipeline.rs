//! `ras pipeline`: the steps from the reference binary to a running port, in a project with a
//! `rhipper.json`. The steps are `analyze`, `lift`, `emit`, `build` and `run`; each writes its log to
//! `build/pipeline/<step>.log`, and the pipeline prints one progress line per step, `[  12s] emit`,
//! which the workbench reads to show the step in work. A failed step prints
//! `[  12s] build failed: stopping` and the pipeline ends; the last line of a whole run is `done`.
//! Every run of the port appends a record to `build/pipeline/runs.jsonl`; with `run.compare` on,
//! the reference runs after the port (its output in `reference.log`) and the record and the log
//! carry a verdict: `same`, `differs at line N` or `port crashed`. With `run.trace` on, the port
//! is emitted with the trace hooks and the run step records its trace (`port.trace`) and the
//! reference's under the debugger (`reference.trace`), and the record and the log carry where
//! the two part and in which function (`trace`, `trace_function`, `trace_event`). A DLL is built as a shared
//! library and run through `run.exe`, the program that loads it, which finds the port first; the
//! comparison runs the same program finding the reference first.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::doctor::{self, Port};
use crate::project::Project;
use crate::trace;

const CMAKE_LISTS: &str = include_str!("../templates/CMakeLists.txt");

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
    /// Deletes the build outputs before the build.
    pub rebuild: bool,
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
    let mut pipeline = Pipeline { project, progress, image: None, timeout: options.timeout, rebuild: options.rebuild };
    for step in steps {
        pipeline.progress.line(step.id());
        let log = pipeline.project.log_dir().join(format!("{}.log", step.id()));
        let mut log = File::create(&log).with_context(|| format!("cannot write {}", log.display()))?;
        let result = match step {
            Step::Analyze => pipeline.analyze(&mut log),
            Step::Lift => pipeline.lift(&mut log),
            Step::Emit => pipeline.emit(&mut log),
            Step::Build => pipeline.build(&mut log),
            Step::Run => pipeline.run_port(&mut log),
        };
        if let Err(e) = result {
            let _ = writeln!(log, "error: {e:#}");
            eprintln!("error: {e:#}");
            pipeline.progress.line(&format!("{} failed: stopping", step.id()));
            bail!("{} failed", step.id());
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

struct Pipeline {
    project: Project,
    progress: Progress,
    /// The reference image, loaded by the first step that needs it.
    image: Option<ra_asm_pe::Image>,
    timeout: Option<u64>,
    rebuild: bool,
}

impl Pipeline {
    fn image(&mut self) -> Result<&ra_asm_pe::Image> {
        if self.image.is_none() {
            self.image = Some(self.project.load_image()?);
        }
        Ok(self.image.as_ref().expect("loaded"))
    }

    fn analyze(&mut self, log: &mut File) -> Result<()> {
        let image = self.image()?;
        let analysis = ra_asm_analysis::analyze(image);
        let warnings = log_warnings(log, image.warnings.iter().chain(&analysis.warnings));
        let summary = format!("{} functions, {} warnings", analysis.functions.len(), warnings);
        writeln!(log, "{summary}")?;
        println!("{summary}");
        Ok(())
    }

    fn lift(&mut self, log: &mut File) -> Result<()> {
        let build_dir = self.project.build_dir();
        let x87 = self.project.x87;
        let image = self.image()?;
        let analysis = ra_asm_analysis::analyze(image);
        let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, None);
        let text = lifted.module.to_ll();
        fs::create_dir_all(&build_dir)?;
        fs::write(build_dir.join("program.ll"), &text)?;
        let warnings = log_warnings(log, analysis.warnings.iter().chain(&lifted.warnings));
        let summary = format!(
            "{} functions, {} imports, {} bytes of IR, {} warnings",
            lifted.functions.len(),
            lifted.imports.len(),
            text.len(),
            warnings
        );
        writeln!(log, "{summary}")?;
        println!("{summary}");
        Ok(())
    }

    fn emit(&mut self, log: &mut File) -> Result<()> {
        let gen_dir = self.project.gen_dir.clone();
        let build_dir = self.project.build_dir();
        let (lang, package) = (self.project.lang.clone(), self.project.build.target.clone());
        let (x87, trace) = (self.project.x87, self.project.run.trace);
        let image = self.image()?;
        let analysis = ra_asm_analysis::analyze(image);
        let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
        let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let translation = crate::emit::translate(image, &analysis, &lifted, &signatures, &lang, &package, trace)?;
        fs::create_dir_all(&build_dir)?;
        translation.write(&gen_dir)?;
        fs::write(gen_dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures)))?;
        fs::write(build_dir.join("functions.json"), translation.table())?;
        let warnings = log_warnings(log, analysis.warnings.iter().chain(&lifted.warnings).chain(&translation.warnings));
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
        Ok(())
    }

    fn build(&mut self, log: &mut File) -> Result<()> {
        let project = &self.project;
        // The tools first: a missing one fails with its name, not with the build system's error.
        let port = if project.is_rust() { Port::Rust } else { Port::C };
        if let Some(failure) = doctor::check(port).failure() {
            bail!("{failure}");
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
        write_cmake_lists(project, bits, dll)?;
        let cmake = "cmake";
        let dir = &project.build.dir;
        let cache = dir.join("CMakeCache.txt");
        if !cache.is_file() {
            let mut configure = Command::new(cmake);
            configure.arg("-S").arg(&project.root).arg("-B").arg(dir).args(generator_args(&project.build.config));
            let outcome = run_logged(&mut configure, log, None, true)?;
            if outcome.code != Some(0) {
                bail!("cmake configure exited with {}", outcome.exit());
            }
        } else if let Some(current) = single_config_type(&cache)
            && current != project.build.config
        {
            // A single-configuration generator (Ninja) builds the configuration it was configured
            // with: configure again for another.
            let mut configure = Command::new(cmake);
            configure.arg("-B").arg(dir).arg(format!("-DCMAKE_BUILD_TYPE={}", project.build.config));
            let outcome = run_logged(&mut configure, log, None, true)?;
            if outcome.code != Some(0) {
                bail!("cmake configure exited with {}", outcome.exit());
            }
        }
        let mut build = Command::new(cmake);
        build.arg("--build").arg(dir).args(["--config", &project.build.config, "--parallel", "--target", &project.build.target]);
        if self.rebuild {
            build.arg("--clean-first");
        }
        let outcome = run_logged(&mut build, log, None, true)?;
        if outcome.code != Some(0) {
            bail!("cmake build exited with {}", outcome.exit());
        }
        Ok(())
    }

    fn run_port(&mut self, log: &mut File) -> Result<()> {
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
            // The reference's trace needs the debugger at the program's own functions: a DLL's
            // run in its host and a 32-bit program's are beyond it for now.
            let not_traced = if !project.run.trace {
                None
            } else if dll {
                Some("the reference is a DLL in its host".to_owned())
            } else if bits == 32 {
                Some("the reference is a 32-bit program".to_owned())
            } else {
                None
            };
            let reference = if project.run.trace && not_traced.is_none() {
                let plan = trace::plan(&project.build_dir().join("functions.json"))?;
                trace::run_reference(&mut reference, &plan, image_base, &reference_trace, &mut reference_log, timeout)?
            } else {
                run_logged(&mut reference, &mut reference_log, timeout, false)?
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
                summary = format!("{summary}; trace: {traces}");
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

/// Writes the warnings to the log and returns how many there were.
fn log_warnings<'a>(log: &mut File, warnings: impl Iterator<Item = &'a String>) -> usize {
    let mut count = 0;
    for w in warnings {
        let _ = writeln!(log, "warning: {w}");
        count += 1;
    }
    count
}

/// Writes the `CMakeLists.txt` of a project that has none. `bits` is the pointer width of the
/// program, which sets how the port is linked, and `dll` whether it is a shared library.
fn write_cmake_lists(project: &Project, bits: u32, dll: bool) -> Result<()> {
    let file = project.root.join("CMakeLists.txt");
    if file.exists() {
        return Ok(());
    }
    let gen_dir = match project.gen_dir.strip_prefix(&project.root) {
        Ok(relative) => format!("${{CMAKE_CURRENT_SOURCE_DIR}}/{}", slashes(relative)),
        Err(_) => slashes(&project.gen_dir),
    };
    let text = CMAKE_LISTS
        .replace("@NAME@", &project.name)
        .replace("@TARGET@", &project.build.target)
        .replace("@GEN_DIR@", &gen_dir)
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
