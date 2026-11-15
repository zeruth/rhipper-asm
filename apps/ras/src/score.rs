//! `ras score <dir>`: the scoreboard, the progress of the port as a file. A row per program of
//! the directory and a `total` row: the functions at each level (the machine and promoted
//! modules' functions, the ABI level's functions with a signature), the traps and the functions
//! that hold one, the functions whose every recorded vector replays at the machine and at the
//! promoted level, the assumptions of the raise by state (discharged by code, checked by a vector
//! that ran the site, open; see [`ra_asm_verify::ledger`]), and the gotos and casts per thousand
//! lines of the C the emit writes. Under the
//! table, the entries of each pending list of the `testdata` tree the directory lies in, and the
//! tests each oracle ignores, its debugging and maintenance aids aside. Markdown for a person,
//! `--json` for a test.
//!
//! Each program is measured in a process of its own (`ras score-file`) with a time limit, which
//! writes a JSON line as each column is known: a program the limit stops keeps the columns it
//! reached. The vectors come of one interpreted run at the machine level, bounded to
//! [`RUN_LIMIT`] instructions, in a scratch directory so a program that writes files leaves them
//! there; a program the host cannot run has no vector and verifies nothing. The ABI level has no
//! state a vector loads into, so it has no verified column.
//!
//! The JSON (`--json`):
//!
//! ```json
//! { "directory": "testdata/bin", "seconds": 31.2,
//!   "programs": [ { "program": "control64.exe", "status": "ok", "seconds": 2.1,
//!                   "functions": { "machine": 40, "promoted": 40, "abi": 40 },
//!                   "traps": 0, "trapping": 0, "vectors": 52, "recorded": 21,
//!                   "verified": { "machine": 21, "promoted": 21 },
//!                   "assumptions": { "discharged": 30, "checked": 2, "open": 9 }, "stop": null,
//!                   "lines": 900, "gotos": 3, "casts": 120,
//!                   "per_thousand": { "gotos": 3.3, "casts": 133.3 }, "error": null } ],
//!   "total": { "program": "total", "status": "1/1 ok", ... },
//!   "pending": [ { "list": "testdata/vectors/x86/PENDING.txt", "entries": 3 } ],
//!   "ignored": [ { "oracle": "crates/ra-asm-emit/tests/oracle.rs", "tests": 26 } ] }
//! ```
//!
//! A column nobody measured, such as the verified ones of a program that stopped before its run,
//! is null, and a `-` in the table.
//!
//! `ras score --source <dir>`: the lexical half of the grade of any C tree, so the old pipeline's
//! output (`../CrystalCore`) is a baseline the emitter's C is measured against. Over every `.c`
//! and `.h` under the directory (or the one file named) it counts the lines, the `goto`s, the
//! labels and the casts, and gives each of the three per thousand lines. Comments and string
//! literals are blanked before anything is counted, so a `goto` in a comment is none. It reads
//! the tree and writes nothing there.
//!
//! The figures are the text's, not a parse's: a label is a line that is one identifier and a
//! colon (`case`, `default` and the C++ access words aside); a cast is a parenthesised type that
//! does not follow a name or a closing bracket and is followed by an operand, where a type is a
//! C type word, a `struct`, `enum` or `union`, a pointer, a `_t` name or one of the names Ghidra
//! and the runtime write (`uint`, `undefined4`, `ra_v128`, ...). A function pointer type in a
//! cast, and the second cast of a chain like `(uintptr_t)(void *)p`, are missed, which the two
//! sides of a comparison share.
//!
//! The JSON of `--source` (`--json`):
//!
//! ```json
//! { "source": "...", "files": 4, "lines": 36, "gotos": 0, "casts": 0, "labels": 0,
//!   "per_thousand": { "gotos": 0.0, "casts": 0.0, "labels": 0.0 } }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_lift::{LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::{Counts, Replayer, Vector};
use serde_json::{Value, json};

/// The counts over a tree.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Score {
    pub files: usize,
    pub lines: usize,
    pub gotos: usize,
    pub casts: usize,
    pub labels: usize,
}

/// A line that is one of these and a colon is no label.
const NOT_LABELS: [&str; 5] = ["case", "default", "public", "private", "protected"];

/// A word before `(` that leaves the parenthesis free to be a cast; any other name before it
/// makes a call or a declarator.
const BEFORE_CAST: [&str; 7] = ["return", "case", "else", "do", "if", "while", "for"];

/// The words a cast's type begins with.
const TYPE_HEADS: [&str; 16] = [
    "struct", "enum", "union", "const", "volatile", "unsigned", "signed", "int", "char", "short", "long", "float", "double", "void",
    "bool", "_Bool",
];

/// The type names of the old pipeline's output and of this engine's runtime, with no `_t`.
const TYPE_NAMES: [&str; 24] = [
    "uint",
    "ushort",
    "ulong",
    "uchar",
    "byte",
    "sbyte",
    "word",
    "dword",
    "qword",
    "longlong",
    "ulonglong",
    "undefined",
    "undefined1",
    "undefined2",
    "undefined3",
    "undefined4",
    "undefined5",
    "undefined6",
    "undefined7",
    "undefined8",
    "code",
    "float10",
    "int3",
    "ra_u128",
];

impl Score {
    /// The score of the C under `source`: a directory walked for its `.c` and `.h` files, or one
    /// file.
    pub(crate) fn of(source: &Path) -> Result<Self> {
        let mut files = Vec::new();
        if source.is_file() {
            files.push(source.to_path_buf());
        } else if source.is_dir() {
            collect(source, &mut files)?;
        } else {
            bail!("{} is neither a file nor a directory", source.display());
        }
        files.sort();
        let mut score = Self::default();
        for path in &files {
            let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
            score.add(&Self::of_text(&String::from_utf8_lossy(&bytes)));
        }
        Ok(score)
    }

    /// The score of one file's text.
    pub(crate) fn of_text(text: &str) -> Self {
        let code = blank_literals(text);
        Self { files: 1, lines: text.lines().count(), gotos: gotos(&code), casts: casts(&code), labels: labels(&code) }
    }

    fn add(&mut self, other: &Self) {
        self.files += other.files;
        self.lines += other.lines;
        self.gotos += other.gotos;
        self.casts += other.casts;
        self.labels += other.labels;
    }

    /// A count per thousand lines; zero over an empty tree.
    #[allow(clippy::cast_precision_loss)] // Line counts stay far below 2^53.
    fn per_thousand(self, count: usize) -> f64 {
        if self.lines == 0 { 0.0 } else { count as f64 * 1000.0 / self.lines as f64 }
    }

    pub(crate) fn json(self, source: &Path) -> Value {
        json!({
            "source": source.display().to_string(),
            "files": self.files,
            "lines": self.lines,
            "gotos": self.gotos,
            "casts": self.casts,
            "labels": self.labels,
            "per_thousand": {
                "gotos": self.per_thousand(self.gotos),
                "casts": self.per_thousand(self.casts),
                "labels": self.per_thousand(self.labels),
            },
        })
    }
}

/// Every `.c` and `.h` file under `dir`, through its subdirectories; a symbolic link is not followed.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("cannot read {}", dir.display()))?;
        let kind = entry.file_type().with_context(|| format!("cannot read {}", entry.path().display()))?;
        let path = entry.path();
        if kind.is_dir() {
            collect(&path, out)?;
        } else if kind.is_file() && is_c(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether a file is C: a `.c` or a `.h`.
fn is_c(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "c" || e == "h")
}

/// `text` with every comment, string literal and character literal blanked to spaces, the
/// newlines kept, so what is counted afterwards is code and the lines keep their numbers.
fn blank_literals(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(bytes.len(), |e| i + 2 + e + 2);
                while i < end {
                    if bytes[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
            }
            // The quotes stay, so a cast of a literal still has its operand.
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                    let step = if bytes[i] == b'\\' { 2 } else { 1 };
                    out[i..(i + step).min(bytes.len())].fill(b' ');
                    i += step;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    // Only bytes inside ASCII-delimited regions were replaced, every byte of a multibyte character
    // among them, so the result is still UTF-8.
    String::from_utf8(out).unwrap_or_default()
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The identifiers of `code` with the byte each starts at.
fn words(code: &str) -> Vec<(usize, &str)> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_word_byte(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_word_byte(bytes[i]) {
                i += 1;
            }
            out.push((start, &code[start..i]));
        } else {
            i += 1;
        }
    }
    out
}

/// The `goto` statements.
fn gotos(code: &str) -> usize {
    words(code).iter().filter(|(_, w)| *w == "goto").count()
}

/// The lines that are one identifier and a colon, which a `goto` jumps to.
fn labels(code: &str) -> usize {
    code.lines()
        .filter(|line| {
            let line = line.trim_start();
            let name: String = line.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let rest = line[name.len()..].trim_start();
            !name.is_empty()
                && !name.as_bytes()[0].is_ascii_digit()
                && !NOT_LABELS.contains(&name.as_str())
                && rest.starts_with(':')
                && !rest.starts_with("::")
        })
        .count()
}

/// Whether the text between a cast's parentheses names a type.
fn is_type(inner: &str) -> bool {
    // The words and the stars, each a token: `char *const` is `char`, `*`, `const`.
    let mut tokens: Vec<&str> = inner.split_whitespace().flat_map(|piece| piece.split_inclusive('*').flat_map(split_star)).collect();
    let mut pointer = false;
    while let Some(&last) = tokens.last().filter(|t| **t == "*" || **t == "const") {
        pointer |= last == "*";
        tokens.pop();
    }
    let Some((&first, &last)) = tokens.first().zip(tokens.last()) else { return false };
    if !tokens.iter().all(|w| w.bytes().all(is_word_byte) && !w.as_bytes()[0].is_ascii_digit()) {
        return false;
    }
    pointer || TYPE_HEADS.contains(&first) || last.ends_with("_t") || TYPE_NAMES.contains(&last) || last.starts_with("ra_v")
}

/// `foo*` as `foo` and `*`; a piece without a star as itself.
fn split_star(piece: &str) -> Vec<&str> {
    match piece.strip_suffix('*') {
        Some("") => vec!["*"],
        Some(word) => vec![word, "*"],
        None => vec![piece],
    }
}

/// The casts: a parenthesised type that follows no name or closing bracket, with an operand after it.
fn casts(code: &str) -> usize {
    let bytes = code.as_bytes();
    let mut count = 0;
    for (at, _) in code.match_indices('(') {
        let before = code[..at].trim_end();
        let word_before: String = before.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if word_before.is_empty() {
            if before.ends_with([')', ']']) {
                continue;
            }
        } else {
            let word: String = word_before.chars().rev().collect();
            if !BEFORE_CAST.contains(&word.as_str()) {
                continue;
            }
        }
        let Some(end) = code[at + 1..].find([')', '(']).map(|e| at + 1 + e) else { continue };
        if bytes[end] != b')' || !is_type(&code[at + 1..end]) {
            continue;
        }
        let next = code[end + 1..].trim_start().bytes().next();
        if next.is_some_and(|b| is_word_byte(b) || b"(*&-~!\"'".contains(&b)) {
            count += 1;
        }
    }
    count
}

/// `ras score --source`: the score of the C under `source`, as a table or JSON.
pub(crate) fn score(source: &Path, json: bool) -> Result<()> {
    let score = Score::of(source)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&score.json(source))?);
        return Ok(());
    }
    println!("{:<8} {}", "files", score.files);
    println!("{:<8} {}", "lines", score.lines);
    for (name, count) in [("gotos", score.gotos), ("casts", score.casts), ("labels", score.labels)] {
        println!("{name:<8} {count:<8} {:.1} per thousand lines", score.per_thousand(count));
    }
    Ok(())
}

/// The IR instructions the recorded run of one program executes at most: enough for the
/// fixtures to end, and a bound on the time a program that never does costs the board.
pub(crate) const RUN_LIMIT: u64 = 10_000_000;

/// The IR instructions the replay of one vector executes at most.
const REPLAY_LIMIT: u64 = 1_000_000;

/// The first bytes of the images the loaders read: a PE (and an Xbox XBE), an ELF, a PlayStation
/// executable, and a Mach-O of either width and order. A file of the directory without one, such
/// as a link map, is no program.
const MAGICS: [&[u8]; 8] = [
    b"MZ",
    b"XBEH",
    b"\x7fELF",
    b"PS-X EXE",
    &[0xfe, 0xed, 0xfa, 0xce],
    &[0xfe, 0xed, 0xfa, 0xcf],
    &[0xce, 0xfa, 0xed, 0xfe],
    &[0xcf, 0xfa, 0xed, 0xfe],
];

/// The oracles whose ignored tests the board counts, from the repository's root.
const ORACLES: [&str; 2] = ["crates/ra-asm-lift/tests/oracle.rs", "crates/ra-asm-emit/tests/oracle.rs"];

/// The reasons of an ignored test that is a tool run by name, not a program waiting.
const AIDS: [&str; 2] = ["a debugging aid", "a maintenance aid"];

/// The columns of the board's table.
const COLUMNS: &str = "| program | status | machine | promoted | abi | traps | trapping | verified machine | verified promoted | \
                       discharged | checked | open | lines | gotos ‰ | casts ‰ |";

/// What `ras score <dir>` was asked for.
pub(crate) struct Options {
    /// The time limit of one program.
    pub timeout: Duration,
    /// The programs measured at once.
    pub jobs: usize,
    /// The `--signatures` of the command, passed to each program's process.
    pub signatures: Vec<PathBuf>,
    /// The board on stdout as JSON.
    pub json: bool,
}

/// One row of the board: a program, or the total. A column the program's process did not reach
/// is `None`.
struct Row {
    program: String,
    /// `ok`, `error` (the process named a failure), `crash` (it died without one) or `timeout`;
    /// the total's says how many are `ok`.
    status: String,
    seconds: f64,
    /// The functions of the machine and the promoted modules.
    machine: Option<u64>,
    promoted: Option<u64>,
    /// The functions of the ABI level with a signature.
    abi: Option<u64>,
    traps: Option<u64>,
    /// The functions with a trap.
    trapping: Option<u64>,
    /// The vectors the run kept, and the functions they are of.
    vectors: Option<u64>,
    recorded: Option<u64>,
    /// The functions whose every vector replays at the level.
    verified_machine: Option<u64>,
    verified_promoted: Option<u64>,
    /// The assumptions of the raise: discharged by code, checked by a vector through the site,
    /// and open.
    discharged: Option<u64>,
    checked: Option<u64>,
    open: Option<u64>,
    /// The lexical half of the C of the emit.
    lines: Option<u64>,
    gotos: Option<u64>,
    casts: Option<u64>,
    /// What stopped the recorded run, when anything did.
    stop: Option<String>,
    error: Option<String>,
}

impl Row {
    /// The row of `program` from the lines its process wrote, merged.
    fn of(program: String, status: &str, seconds: f64, values: &serde_json::Map<String, Value>) -> Self {
        let n = |key: &str| values.get(key).and_then(Value::as_u64);
        let s = |key: &str| values.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            program,
            status: status.to_owned(),
            seconds,
            machine: n("machine"),
            promoted: n("promoted"),
            abi: n("abi"),
            traps: n("traps"),
            trapping: n("trapping"),
            vectors: n("vectors"),
            recorded: n("recorded"),
            verified_machine: n("verified_machine"),
            verified_promoted: n("verified_promoted"),
            discharged: n("discharged"),
            checked: n("checked"),
            open: n("open"),
            lines: n("lines"),
            gotos: n("gotos"),
            casts: n("casts"),
            stop: s("stop"),
            error: s("error"),
        }
    }

    /// The `total` row: each column summed over the rows that have it, over `seconds` of wall time.
    fn total(rows: &[Row], seconds: f64) -> Self {
        let sum = |column: fn(&Row) -> Option<u64>| rows.iter().filter_map(column).reduce(|a, b| a + b);
        let ok = rows.iter().filter(|r| r.status == "ok").count();
        Self {
            program: "total".to_owned(),
            status: format!("{ok}/{} ok", rows.len()),
            seconds,
            machine: sum(|r| r.machine),
            promoted: sum(|r| r.promoted),
            abi: sum(|r| r.abi),
            traps: sum(|r| r.traps),
            trapping: sum(|r| r.trapping),
            vectors: sum(|r| r.vectors),
            recorded: sum(|r| r.recorded),
            verified_machine: sum(|r| r.verified_machine),
            verified_promoted: sum(|r| r.verified_promoted),
            discharged: sum(|r| r.discharged),
            checked: sum(|r| r.checked),
            open: sum(|r| r.open),
            lines: sum(|r| r.lines),
            gotos: sum(|r| r.gotos),
            casts: sum(|r| r.casts),
            stop: None,
            error: None,
        }
    }

    fn json(&self) -> Value {
        json!({
            "program": self.program,
            "status": self.status,
            "seconds": round(self.seconds),
            "functions": {"machine": self.machine, "promoted": self.promoted, "abi": self.abi},
            "traps": self.traps,
            "trapping": self.trapping,
            "vectors": self.vectors,
            "recorded": self.recorded,
            "verified": {"machine": self.verified_machine, "promoted": self.verified_promoted},
            "assumptions": {"discharged": self.discharged, "checked": self.checked, "open": self.open},
            "stop": self.stop,
            "lines": self.lines,
            "gotos": self.gotos,
            "casts": self.casts,
            "per_thousand": {"gotos": per_thousand(self.gotos, self.lines), "casts": per_thousand(self.casts, self.lines)},
            "error": self.error,
        })
    }

    /// The row as a line of [`COLUMNS`].
    fn cells(&self) -> String {
        let n = |v: Option<u64>| v.map_or_else(|| "-".to_owned(), |v| v.to_string());
        let f = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| format!("{v:.1}"));
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            self.program,
            self.status,
            n(self.machine),
            n(self.promoted),
            n(self.abi),
            n(self.traps),
            n(self.trapping),
            n(self.verified_machine),
            n(self.verified_promoted),
            n(self.discharged),
            n(self.checked),
            n(self.open),
            n(self.lines),
            f(per_thousand(self.gotos, self.lines)),
            f(per_thousand(self.casts, self.lines)),
        )
    }
}

/// A count per thousand lines; zero over no line, `None` when either is unknown.
#[allow(clippy::cast_precision_loss)] // Line counts stay far below 2^53.
fn per_thousand(count: Option<u64>, lines: Option<u64>) -> Option<f64> {
    let (count, lines) = (count?, lines?);
    Some(if lines == 0 { 0.0 } else { count as f64 * 1000.0 / lines as f64 })
}

fn round(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

/// `ras score <dir>`: measures every program of `dir`, each in a process of its own, and prints
/// the board.
pub(crate) fn board(dir: &Path, options: &Options) -> Result<()> {
    let programs = programs(dir)?;
    let exe = std::env::current_exe().context("cannot find the ras executable to measure the programs with")?;
    let scratch = std::env::temp_dir().join(format!("ras-score-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).with_context(|| format!("cannot make {}", scratch.display()))?;
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::with_capacity(programs.len()));
    std::thread::scope(|scope| {
        for _ in 0..options.jobs.max(1) {
            scope.spawn(|| {
                while let Some(program) = programs.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let row = measure(&exe, program, &scratch, options);
                    rows.lock().unwrap_or_else(PoisonError::into_inner).push(row);
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    let mut rows = rows.into_inner().unwrap_or_else(PoisonError::into_inner);
    rows.sort_by(|a, b| a.program.cmp(&b.program));
    let total = Row::total(&rows, started.elapsed().as_secs_f64());
    let tree = testdata(dir);
    let pending = tree.as_deref().map(pending).transpose()?.unwrap_or_default();
    let ignored = tree.as_deref().and_then(Path::parent).map(ignored).unwrap_or_default();
    if options.json {
        let board = json!({
            "directory": dir.display().to_string(),
            "seconds": round(total.seconds),
            "programs": rows.iter().map(Row::json).collect::<Vec<_>>(),
            "total": total.json(),
            "pending": pending.iter().map(|(list, entries)| json!({"list": list, "entries": entries})).collect::<Vec<_>>(),
            "ignored": ignored.iter().map(|(oracle, tests)| json!({"oracle": oracle, "tests": tests})).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&board)?);
    } else {
        print!("{}", markdown(&rows, &total, &pending, &ignored)?);
    }
    Ok(())
}

/// The programs of `dir`, sorted; its subdirectories are not entered.
fn programs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry.with_context(|| format!("cannot read {}", dir.display()))?.path();
        if path.is_file() && is_program(&path) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Whether the file begins as an image the loaders read.
fn is_program(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let n = file.read(&mut head).unwrap_or(0);
    MAGICS.iter().any(|magic| head[..n].starts_with(magic))
}

/// The `testdata` directory `dir` lies in, itself included.
fn testdata(dir: &Path) -> Option<PathBuf> {
    let dir = std::path::absolute(dir).ok()?;
    dir.ancestors().find(|a| a.file_name().is_some_and(|n| n == "testdata")).map(Path::to_path_buf)
}

/// Every pending list under `testdata`, by its path from `testdata` on, with its entries.
fn pending(testdata: &Path) -> Result<Vec<(String, usize)>> {
    let mut lists = Vec::new();
    crate::cards::walk(testdata, &mut lists)?;
    lists
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
            Ok((crate::cards::label(path), crate::cards::entries(&text).len()))
        })
        .collect()
}

/// The tests each of the [`ORACLES`] under `root` ignores, the [`AIDS`] aside; an oracle not
/// there is not listed.
fn ignored(root: &Path) -> Vec<(String, usize)> {
    ORACLES
        .iter()
        .filter_map(|oracle| {
            let text = std::fs::read_to_string(root.join(oracle)).ok()?;
            let tests = text
                .lines()
                .map(str::trim_start)
                .filter(|line| line.starts_with("#[ignore") && !AIDS.iter().any(|aid| line.contains(aid)))
                .count();
            Some(((*oracle).to_owned(), tests))
        })
        .collect()
}

/// The board as Markdown: the table of the programs and the total, the programs' errors, then
/// the pending lists and the ignored tests.
fn markdown(rows: &[Row], total: &Row, pending: &[(String, usize)], ignored: &[(String, usize)]) -> Result<String> {
    let mut out = String::new();
    let columns = COLUMNS.matches('|').count() - 1;
    writeln!(out, "{COLUMNS}")?;
    writeln!(out, "|{}", "---|".repeat(columns))?;
    for row in rows.iter().chain([total]) {
        writeln!(out, "{}", row.cells())?;
    }
    let errors: Vec<&Row> = rows.iter().filter(|r| r.error.is_some()).collect();
    if !errors.is_empty() {
        writeln!(out)?;
        for row in errors {
            writeln!(out, "- {}: {}", row.program, row.error.as_deref().unwrap_or_default())?;
        }
    }
    for (heading, lines) in [("pending list | entries", pending), ("oracle | ignored tests", ignored)] {
        if lines.is_empty() {
            continue;
        }
        writeln!(out, "\n| {heading} |\n|---|---|")?;
        for (name, count) in lines {
            writeln!(out, "| {name} | {count} |")?;
        }
        writeln!(out, "| total | {} |", lines.iter().map(|(_, count)| count).sum::<usize>())?;
    }
    Ok(out)
}

/// One program in `ras score-file`, stopped at the time limit: its row from the lines it wrote.
fn measure(exe: &Path, program: &Path, scratch: &Path, options: &Options) -> Row {
    let name = program.file_name().map_or_else(|| program.display().to_string(), |n| n.to_string_lossy().into_owned());
    let started = Instant::now();
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut command = Command::new(exe);
    for s in &options.signatures {
        command.arg("--signatures").arg(absolute(s));
    }
    command.arg("score-file").arg(absolute(program)).current_dir(scratch).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let values: serde_json::Map<String, Value> =
                [("error".to_owned(), json!(format!("cannot start {}: {e}", exe.display())))].into_iter().collect();
            return Row::of(name, "error", 0.0, &values);
        }
    };
    // Read on a thread of its own, so a process that writes much never blocks on a full pipe
    // while the time limit is watched.
    let stdout = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    });
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break Some(status);
        }
        if started.elapsed() >= options.timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let seconds = started.elapsed().as_secs_f64();
    let bytes = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let mut values = serde_json::Map::new();
    // A line begins where its object does: what the program itself wrote to the console may
    // stand ahead of it.
    for text in String::from_utf8_lossy(&bytes).lines() {
        let Some(at) = text.find('{') else { continue };
        if let Ok(Value::Object(line)) = serde_json::from_str(&text[at..]) {
            values.extend(line);
        }
    }
    let status = match status {
        None => "timeout",
        Some(_) if values.contains_key("error") => "error",
        Some(status) if !status.success() => "crash",
        Some(_) => "ok",
    };
    Row::of(name, status, seconds, &values)
}

/// Writes one line of the protocol between `ras score-file` and `ras score`.
fn line(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

/// `ras score-file`: the columns of one program, a line as each is known, and a line that names
/// the failure that ends it early.
pub(crate) fn program(pe: &Path, signatures: &[PathBuf]) {
    if let Err(e) = columns(pe, signatures) {
        line(&json!({"error": format!("{e:#}")}));
    }
}

fn columns(pe: &Path, files: &[PathBuf]) -> Result<()> {
    let image = crate::load(pe, files)?;
    let analysis = ra_asm_analysis::analyze(&image);

    // The ABI level and its C, as the emit writes the port.
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    line(&json!({"traps": coverage.traps(), "trapping": coverage.functions.len()}));
    let signatures = crate::lift::raise(&mut lifted, &analysis, RaiseLevel::Abi);
    let abi = lifted.module.functions.iter().filter(|f| f.address.is_some() && signatures.contains_key(&f.name)).count();
    line(&json!({"abi": abi}));
    let annotations = BTreeMap::new();
    let options =
        crate::emit::Options { lang: "c", package: "score", trace: false, hand: &[], annotations: &annotations, checked: false, with: &[] };
    let translation = crate::emit::translate(&image, &analysis, &lifted, &signatures, &options)?;
    let mut c = Score::default();
    for file in translation.files.iter().filter(|f| is_c(Path::new(&f.name))) {
        c.add(&Score::of_text(&file.text));
    }
    line(&json!({"lines": c.lines, "gotos": c.gotos, "casts": c.casts}));
    drop((translation, lifted));

    // The two levels over the state, lifted as the image is mapped, which the vectors replay at.
    let machine = mapped(&image, &analysis, RaiseLevel::Machine);
    line(&json!({"machine": stateful(&machine)}));
    let run = ra_asm_lift::run::Options { limit: RUN_LIMIT, ..ra_asm_lift::run::Options::default() };
    // A program the host cannot start, such as a library, has no vector: it verifies nothing.
    let vectors = match ra_asm_verify::record(&image, &machine, &run) {
        Ok((recording, outcome)) => {
            let recorded: BTreeSet<&str> = recording.vectors.iter().map(|v| v.function.as_str()).collect();
            line(&json!({"vectors": recording.vectors.len(), "recorded": recorded.len(), "stop": outcome.stop}));
            recording.vectors
        }
        Err(e) => {
            line(&json!({"vectors": 0, "recorded": 0, "stop": format!("{e:#}")}));
            Vec::new()
        }
    };
    line(&json!({"verified_machine": verified(&image, &machine, RaiseLevel::Machine, &vectors)?}));
    let counts = assumptions(&image, &analysis, &machine, &vectors)?;
    line(&json!({"discharged": counts.discharged, "checked": counts.checked, "open": counts.open}));
    drop(machine);
    let promoted = mapped(&image, &analysis, RaiseLevel::Promoted);
    line(&json!({"promoted": stateful(&promoted)}));
    line(&json!({"verified_promoted": verified(&image, &promoted, RaiseLevel::Promoted, &vectors)?}));
    Ok(())
}

/// `image` lifted as it is mapped and raised to `level`, under the names the recorded run knows.
fn mapped(image: &Image, analysis: &Analysis<'_>, level: RaiseLevel) -> Lifted {
    let mut lifted = ra_asm_lift::lift_with(image, analysis, LiftOptions { standalone: false, x87: X87Unit::Double });
    let options = lifted.raise_options(level);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(analysis, None);
    lifted
}

/// The functions of the image in the module.
fn stateful(lifted: &Lifted) -> usize {
    lifted.module.functions.iter().filter(|f| f.address.is_some()).count()
}

/// The assumptions of the raise over the image by state, as `ras verify --ledger` notes them:
/// checked where a vector that replays equal at the machine level runs the site. Vectors that do
/// not load check nothing.
fn assumptions(image: &Image, analysis: &Analysis<'_>, machine: &Lifted, vectors: &[Vector]) -> Result<Counts> {
    let mut replayer = Replayer::new(image, machine, RaiseLevel::Machine)?;
    replayer.limit = REPLAY_LIMIT;
    let report = if vectors.is_empty() { None } else { replayer.report(vectors).ok() };
    Ok(crate::verify::ledger(image, analysis, machine, report.as_ref().map(|report| (&replayer, report))).counts())
}

/// The functions whose every vector replays against `lifted` without a difference; a vector that
/// cannot be loaded, or runs past [`REPLAY_LIMIT`], fails its function.
fn verified(image: &Image, lifted: &Lifted, level: RaiseLevel, vectors: &[Vector]) -> Result<usize> {
    if vectors.is_empty() {
        return Ok(0);
    }
    let mut replayer = Replayer::new(image, lifted, level)?;
    replayer.limit = REPLAY_LIMIT;
    let mut passed: BTreeMap<&str, bool> = BTreeMap::new();
    for vector in vectors {
        let same = replayer.replay(vector).is_ok_and(|differences| differences.is_empty());
        *passed.entry(vector.function.as_str()).or_insert(true) &= same;
    }
    Ok(passed.values().filter(|same| **same).count())
}
