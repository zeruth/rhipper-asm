//! `ras smoke`: the analysis, the lift and the emit over every executable and library under a
//! directory, so a crash is found by the corpus before a person finds it. Each file runs in a
//! process of its own with a time limit: a panic, a stack overflow or a hang of one file is a line
//! of the report, not the end of the run.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// The extensions taken without `--ext`: programs, libraries, drivers and the other PE images
/// Windows ships under their own names.
const EXTENSIONS: &[&str] = &["exe", "dll", "sys", "ocx", "cpl", "scr", "drv", "efi"];

/// The slowest files the report lists.
const SLOWEST: usize = 20;

/// The end of a crashed child's stderr the report keeps.
const STDERR_TAIL: usize = 2000;

/// What `ras smoke` was asked for.
pub(crate) struct Options {
    /// The time limit of one file.
    pub timeout: Duration,
    /// The files run at once.
    pub jobs: usize,
    /// The time limit of the whole run: the files not started when it is spent are skipped.
    pub budget: Option<Duration>,
    /// How many directory levels under the root to enter; every level with `None`.
    pub depth: Option<usize>,
    /// The extensions to take; [`EXTENSIONS`] when empty.
    pub extensions: Vec<String>,
    /// The `--signatures` of the command, passed to each file's process.
    pub signatures: Vec<PathBuf>,
    /// Where to write the report as JSON, beside what stdout gets.
    pub out: Option<PathBuf>,
    /// No line per file on stderr.
    pub quiet: bool,
    /// The report on stdout as JSON.
    pub json: bool,
    /// The census on stdout instead of the tables: the mnemonics the lift traps on as a pending
    /// list. The JSON report carries the same counts, so `json` wins over it.
    pub census: bool,
}

/// How one file ended.
struct Outcome {
    file: PathBuf,
    /// `ok`, `error` (the image is refused, such as a format the loader does not read), `panic`,
    /// `crash` (the process died without a word, such as a stack overflow), `timeout`, or
    /// `skipped` (not started before the run's budget was spent).
    status: &'static str,
    /// The stage the file was in when it ended.
    stage: String,
    seconds: f64,
    /// The result line of the file's process: its stage times, its coverage, its panic or its
    /// error; null when the process ended without one.
    result: Value,
    /// The exit code of a crash, in hexadecimal.
    exit_code: Option<String>,
    /// The end of a crash's stderr.
    stderr: String,
}

/// Runs every file under `dir` and prints the report; fails when a file panicked or crashed,
/// after the report is out.
pub(crate) fn smoke(dir: &Path, options: &Options) -> Result<()> {
    let extensions: Vec<String> =
        if options.extensions.is_empty() { EXTENSIONS.iter().map(|e| (*e).to_owned()).collect() } else { options.extensions.clone() };
    let mut files = files(dir, options.depth, &extensions);
    // The smallest first, so a budget that runs out leaves the fewest files unrun.
    files.sort_by_cached_key(|f| std::fs::metadata(f).map_or(u64::MAX, |m| m.len()));
    let exe = std::env::current_exe().context("cannot find the ras executable to run the files with")?;
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let outcomes = Mutex::new(Vec::with_capacity(files.len()));
    std::thread::scope(|scope| {
        for _ in 0..options.jobs.max(1) {
            scope.spawn(|| {
                while let Some(file) = files.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let outcome = if options.budget.is_some_and(|b| started.elapsed() >= b) {
                        skipped(file)
                    } else {
                        run_file(&exe, file, &options.signatures, options.timeout)
                    };
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    if !options.quiet {
                        eprintln!("[{n}/{}] {:<7} {:>7.1}s  {}", files.len(), outcome.status, outcome.seconds, file.display());
                    }
                    outcomes.lock().unwrap_or_else(PoisonError::into_inner).push(outcome);
                }
            });
        }
    });
    let mut outcomes = outcomes.into_inner().unwrap_or_else(PoisonError::into_inner);
    outcomes.sort_by(|a, b| a.file.cmp(&b.file));
    let report = report(dir, options.timeout, started.elapsed(), &outcomes);
    if let Some(out) = &options.out {
        std::fs::write(out, serde_json::to_string_pretty(&report)?).with_context(|| format!("cannot write {}", out.display()))?;
    }
    if options.json {
        let mut stdout = std::io::stdout().lock();
        serde_json::to_writer_pretty(&mut stdout, &report)?;
        writeln!(stdout)?;
    } else if options.census {
        print!("{}", census(&report));
    } else {
        print!("{}", text(&report));
    }
    let failed = outcomes.iter().filter(|o| matches!(o.status, "panic" | "crash")).count();
    if failed > 0 {
        bail!("{failed} of {} files panicked or crashed", outcomes.len());
    }
    Ok(())
}

/// The images under `root` (or `root` itself when it is a file) whose extension is one of
/// `extensions`, sorted. A directory that cannot be read is passed over, and a link is not
/// followed, so a junction cannot loop the walk.
fn files(root: &Path, depth: Option<usize>, extensions: &[String]) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, level)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                if depth.is_none_or(|d| level < d) {
                    pending.push((path, level + 1));
                }
            } else if kind.is_file() && path.extension().is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x))) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// A file the run's budget left no time for.
fn skipped(file: &Path) -> Outcome {
    Outcome {
        file: file.to_path_buf(),
        status: "skipped",
        stage: "start".to_owned(),
        seconds: 0.0,
        result: Value::Null,
        exit_code: None,
        stderr: String::new(),
    }
}

/// One file in `ras smoke-file`, stopped at `timeout`.
fn run_file(exe: &Path, file: &Path, signatures: &[PathBuf], timeout: Duration) -> Outcome {
    let started = Instant::now();
    let mut command = Command::new(exe);
    for s in signatures {
        command.arg("--signatures").arg(s);
    }
    command.arg("smoke-file").arg(file).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut outcome = Outcome {
        file: file.to_path_buf(),
        status: "error",
        stage: "start".to_owned(),
        seconds: 0.0,
        result: Value::Null,
        exit_code: None,
        stderr: String::new(),
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            outcome.result = json!({"message": format!("cannot start {}: {e}", exe.display())});
            return outcome;
        }
    };
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break Some(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    outcome.seconds = started.elapsed().as_secs_f64();
    let stdout = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(stage) = value["stage"].as_str() {
            stage.clone_into(&mut outcome.stage);
        }
        if value.get("status").is_some() {
            outcome.result = value;
        }
    }
    match (status, outcome.result["status"].as_str()) {
        (_, Some("ok")) => outcome.status = "ok",
        (_, Some("panic")) => outcome.status = "panic",
        (_, Some(_)) => outcome.status = "error",
        (None, None) => outcome.status = "timeout",
        (Some(status), None) => {
            outcome.status = "crash";
            outcome.exit_code = Some(status.code().map_or_else(|| "none".to_owned(), |c| format!("{:#x}", c as u32)));
            let start = stderr.len().saturating_sub(STDERR_TAIL);
            let start = (start..stderr.len()).find(|&i| stderr.is_char_boundary(i)).unwrap_or(stderr.len());
            stderr[start..].clone_into(&mut outcome.stderr);
        }
    }
    outcome
}

/// Reads a pipe to its end on a thread of its own, so a child that writes much never blocks on
/// a full pipe while the time limit is watched.
fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// The stages of one file, in their order.
const STAGES: [&str; 6] = ["load", "analyze", "lift", "coverage", "raise", "emit"];

/// The stage the file's process is in, for its panic hook.
static STAGE: AtomicUsize = AtomicUsize::new(0);

/// Set by the first panic: the report takes that one, not the panics of the unwinding after it.
static PANICKED: AtomicBool = AtomicBool::new(false);

/// Writes one line of the protocol between `ras smoke-file` and `ras smoke`.
fn line(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

/// The time of each stage of one file.
struct Clock {
    started: Instant,
    times: serde_json::Map<String, Value>,
}

impl Clock {
    fn new() -> Self {
        line(&json!({"stage": STAGES[0]}));
        Clock { started: Instant::now(), times: serde_json::Map::new() }
    }

    /// Ends the current stage and starts `stage`; the line names it before it runs, so the stage
    /// of a process that dies in it is known.
    fn enter(&mut self, stage: usize) {
        self.finish();
        STAGE.store(stage, Ordering::SeqCst);
        line(&json!({"stage": STAGES[stage]}));
    }

    /// Ends the current stage; the times so far.
    fn finish(&mut self) -> Value {
        let current = STAGES[STAGE.load(Ordering::SeqCst)];
        self.times.insert(current.to_owned(), json!(round(self.started.elapsed().as_secs_f64())));
        self.started = Instant::now();
        Value::Object(self.times.clone())
    }
}

/// `ras smoke-file`: the analysis, the lift, the coverage, the raise and the C emit of one image,
/// a line on stdout as each stage starts and a result line at the end. A panic's result line is
/// written by the panic hook, with the backtrace.
pub(crate) fn file(pe: &Path, signatures: &[PathBuf]) {
    std::panic::set_hook(Box::new(|info| {
        if PANICKED.swap(true, Ordering::SeqCst) {
            return;
        }
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic without a message".to_owned());
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let stage = STAGES[STAGE.load(Ordering::SeqCst)];
        line(&json!({"status": "panic", "stage": stage, "message": message, "location": location, "backtrace": backtrace}));
    }));
    let mut clock = Clock::new();
    let image = match crate::load(pe, signatures) {
        Ok(image) => image,
        Err(e) => {
            line(&json!({"status": "error", "stage": STAGES[0], "message": format!("{e:#}"), "times": clock.finish()}));
            return;
        }
    };
    clock.enter(1);
    let analysis = ra_asm_analysis::analyze(&image);
    clock.enter(2);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    clock.enter(3);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    clock.enter(4);
    let signatures = crate::lift::raise(&mut lifted, &analysis, ra_asm_lift::RaiseLevel::Abi);
    clock.enter(5);
    let annotations = BTreeMap::new();
    let options = crate::emit::Options { lang: "c", package: "smoke", trace: false, hand: &[], annotations: &annotations, checked: false };
    let translation = match crate::emit::translate(&image, &analysis, &lifted, &signatures, &options) {
        Ok(translation) => translation,
        Err(e) => {
            line(&json!({"status": "error", "stage": STAGES[5], "message": format!("{e:#}"), "times": clock.finish()}));
            return;
        }
    };
    let header = ra_asm_lift::image_header(&image, &analysis, &lifted, Some(&signatures));
    line(&json!({
        "status": "ok",
        "stage": STAGES[5],
        "times": clock.finish(),
        "functions": lifted.functions.len(),
        "bytes": translation.bytes() + header.len(),
        "coverage": crate::report::coverage_json(&coverage),
    }));
}

fn round(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

/// One mnemonic over the corpus: its traps, the functions and the files it traps in, and the
/// first instance.
#[derive(Default)]
struct Mnemonic {
    count: u64,
    functions: u64,
    files: usize,
    example: Option<(String, String)>,
}

/// The report of a run as JSON: the files by status, the coverage summed over the files that
/// went through, the slowest files, and every panic, crash, timeout and refused image.
fn report(dir: &Path, timeout: Duration, elapsed: Duration, outcomes: &[Outcome]) -> Value {
    let mut statuses: BTreeMap<&str, usize> = ["ok", "error", "panic", "crash", "timeout", "skipped"].into_iter().map(|s| (s, 0)).collect();
    let mut functions = 0;
    let mut traps = 0;
    let mut trapping = 0;
    let mut function_traps = 0;
    let mut unresolved = 0;
    let mut mnemonics: BTreeMap<String, Mnemonic> = BTreeMap::new();
    let mut imports: BTreeMap<String, usize> = BTreeMap::new();
    for o in outcomes {
        *statuses.entry(o.status).or_default() += 1;
        let file = o.file.display().to_string();
        functions += o.result["functions"].as_u64().unwrap_or(0);
        let coverage = &o.result["coverage"];
        traps += coverage["traps"].as_u64().unwrap_or(0);
        trapping += coverage["functions"].as_array().map_or(0, Vec::len);
        function_traps += coverage["functions"].as_array().into_iter().flatten().filter_map(|f| f["traps"].as_u64()).sum::<u64>();
        unresolved += coverage["unresolved_jumps"].as_array().map_or(0, Vec::len);
        for u in coverage["uncovered"].as_array().into_iter().flatten() {
            let Some(name) = u["mnemonic"].as_str() else { continue };
            let m = mnemonics.entry(name.to_owned()).or_default();
            m.count += u["count"].as_u64().unwrap_or(0);
            m.functions += u["functions"].as_u64().unwrap_or(0);
            m.files += 1;
            if m.example.is_none() {
                m.example = Some((file.clone(), u["example"].as_str().unwrap_or_default().to_owned()));
            }
        }
        let names: BTreeSet<&str> = coverage["unknown_imports"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        for name in names {
            *imports.entry(name.to_owned()).or_default() += 1;
        }
    }
    let mut uncovered: Vec<_> = mnemonics.into_iter().collect();
    uncovered.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(&b.0)));
    let mut imports: Vec<_> = imports.into_iter().collect();
    imports.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut slowest: Vec<&Outcome> = outcomes.iter().collect();
    slowest.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));
    slowest.truncate(SLOWEST);
    let with = |status: &'static str| outcomes.iter().filter(move |o| o.status == status);
    json!({
        "directory": dir.display().to_string(),
        "timeout": timeout.as_secs(),
        "seconds": round(elapsed.as_secs_f64()),
        "files": outcomes.len(),
        "status": statuses,
        "coverage": {
            "functions": functions,
            "traps": traps,
            "trapping_functions": trapping,
            "function_traps": function_traps,
            "unresolved_jumps": unresolved,
            "uncovered": uncovered.iter().map(|(name, m)| json!({
                "mnemonic": name,
                "count": m.count,
                "functions": m.functions,
                "files": m.files,
                "example": m.example.as_ref().map(|(file, addr)| json!({"file": file, "addr": addr})),
            })).collect::<Vec<_>>(),
            "unknown_imports": imports.iter().map(|(name, files)| json!({"name": name, "files": files})).collect::<Vec<_>>(),
        },
        "slowest": slowest.iter().map(|o| json!({
            "file": o.file.display().to_string(),
            "seconds": round(o.seconds),
            "status": o.status,
            "stage": o.stage,
            "times": o.result["times"],
        })).collect::<Vec<_>>(),
        "panics": with("panic").map(|o| json!({
            "file": o.file.display().to_string(),
            "stage": o.stage,
            "message": o.result["message"],
            "location": o.result["location"],
            "backtrace": o.result["backtrace"],
        })).collect::<Vec<_>>(),
        "crashes": with("crash").map(|o| json!({
            "file": o.file.display().to_string(),
            "stage": o.stage,
            "exit_code": o.exit_code,
            "stderr": o.stderr,
        })).collect::<Vec<_>>(),
        "timeouts": with("timeout").map(|o| json!({"file": o.file.display().to_string(), "stage": o.stage})).collect::<Vec<_>>(),
        "errors": with("error").map(|o| json!({
            "file": o.file.display().to_string(),
            "stage": o.stage,
            "message": o.result["message"],
        })).collect::<Vec<_>>(),
    })
}

/// The report as tables for a person, each headed `<name> (<count>)`; an empty one says `none`.
fn text(report: &Value) -> String {
    let mut out = String::new();
    let rows = |key: &str| report[key].as_array().cloned().unwrap_or_default();
    let _ = writeln!(
        out,
        "{} files under {} in {}s, {}s each at most",
        report["files"],
        s(&report["directory"]),
        report["seconds"],
        report["timeout"]
    );
    let statuses: Vec<String> = report["status"].as_object().into_iter().flatten().map(|(status, n)| format!("{status} {n}")).collect();
    let _ = writeln!(out, "  {}", statuses.join(", "));
    let c = &report["coverage"];
    let _ = writeln!(
        out,
        "coverage: {} functions, {} traps in {} functions, {} unresolved indirect jumps",
        c["functions"], c["traps"], c["trapping_functions"], c["unresolved_jumps"]
    );
    let uncovered = c["uncovered"].as_array().cloned().unwrap_or_default();
    table(&mut out, "traps by mnemonic", &uncovered, |out, u| {
        let (mnemonic, example) = (s(&u["mnemonic"]), &u["example"]);
        let (addr, file) = (s(&example["addr"]), s(&example["file"]));
        let _ = writeln!(out, "  {mnemonic:<12} {:>8} in {:>5} files  {addr} in {file}", u["count"], u["files"]);
    });
    let imports = c["unknown_imports"].as_array().cloned().unwrap_or_default();
    table(&mut out, "imports without a prototype", &imports, |out, i| {
        let _ = writeln!(out, "  {:<40} {:>5} files", s(&i["name"]), i["files"]);
    });
    table(&mut out, "slowest files", &rows("slowest"), |out, o| {
        let seconds = o["seconds"].as_f64().unwrap_or(0.0);
        let _ = writeln!(out, "  {seconds:>8.1}s  {:<7}  {}  ({})", s(&o["status"]), s(&o["file"]), s(&o["stage"]));
    });
    table(&mut out, "panics", &rows("panics"), |out, p| {
        let _ = writeln!(out, "  {} in {}: {} at {}", s(&p["file"]), s(&p["stage"]), s(&p["message"]), s(&p["location"]));
        for frame in s(&p["backtrace"]).lines() {
            let _ = writeln!(out, "      {frame}");
        }
    });
    table(&mut out, "crashes", &rows("crashes"), |out, p| {
        let _ = writeln!(out, "  {} in {}: exit {}", s(&p["file"]), s(&p["stage"]), s(&p["exit_code"]));
        for line in s(&p["stderr"]).lines() {
            let _ = writeln!(out, "      {line}");
        }
    });
    table(&mut out, "timeouts", &rows("timeouts"), |out, t| {
        let _ = writeln!(out, "  {}  ({})", s(&t["file"]), s(&t["stage"]));
    });
    table(&mut out, "refused", &rows("errors"), |out, e| {
        let _ = writeln!(out, "  {} in {}: {}", s(&e["file"]), s(&e["stage"]), s(&e["message"]));
    });
    out
}

/// The census of a run: a line per mnemonic the lift traps on, the one that affects the most
/// functions first, so the lift's remaining work is ordered by what it frees. The shape is a
/// pending list's: `#` lines of comment, then an entry a line whose first word is its name.
fn census(report: &Value) -> String {
    let c = &report["coverage"];
    let n = |value: &Value| value.as_u64().unwrap_or(0);
    let mut rows = c["uncovered"].as_array().cloned().unwrap_or_default();
    rows.sort_by(|a, b| {
        let by_functions = n(&b["functions"]).cmp(&n(&a["functions"]));
        by_functions.then_with(|| n(&b["count"]).cmp(&n(&a["count"]))).then_with(|| s(&a["mnemonic"]).cmp(&s(&b["mnemonic"])))
    });
    let mut out = String::new();
    let _ = writeln!(out, "# The mnemonics the lift traps on under {}, the most functions affected first.", s(&report["directory"]));
    let _ = writeln!(
        out,
        "# {} in {} of {}, {} lifted.",
        counted(n(&c["traps"]), "trap"),
        counted(n(&c["trapping_functions"]), "function"),
        counted(n(&report["status"]["ok"]), "file"),
        counted(n(&c["functions"]), "function")
    );
    for u in &rows {
        let (functions, traps, files) =
            (counted(n(&u["functions"]), "function"), counted(n(&u["count"]), "trap"), counted(n(&u["files"]), "file"));
        let (addr, file) = (s(&u["example"]["addr"]), s(&u["example"]["file"]));
        let _ = writeln!(out, "{}: {functions}, {traps}, {files}, first {addr} in {file}", s(&u["mnemonic"]));
    }
    out
}

/// A count with its noun, in the plural unless it is one.
fn counted(count: u64, noun: &str) -> String {
    if count == 1 { format!("1 {noun}") } else { format!("{count} {noun}s") }
}

/// One table of the text report: its heading with the row count, `none` or a line per row.
fn table(out: &mut String, name: &str, rows: &[Value], row: impl Fn(&mut String, &Value)) {
    let _ = writeln!(out, "{name} ({})", rows.len());
    if rows.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for r in rows {
        row(out, r);
    }
}

/// A string of the report as itself, anything else as its JSON.
fn s(value: &Value) -> String {
    value.as_str().map_or_else(|| value.to_string(), str::to_owned)
}
