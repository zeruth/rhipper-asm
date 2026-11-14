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
//! The JSON (`--json`):
//!
//! ```json
//! { "source": "...", "files": 4, "lines": 36, "gotos": 0, "casts": 0, "labels": 0,
//!   "per_thousand": { "gotos": 0.0, "casts": 0.0, "labels": 0.0 } }
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
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
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "c" || e == "h") {
            out.push(path);
        }
    }
    Ok(())
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
