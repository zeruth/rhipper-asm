//! The written sources as their language's formatter writes them: `clang-format` for C and C++,
//! `rustfmt` for Rust, when the tool is on the PATH. `ras emit` and the oracles format the same
//! way, so what the oracles build is what the port is.

use std::collections::HashMap;
use std::io::Write as _;
use std::ops::Range;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::c::{EmittedFile, EmittedFunction};

/// The style of the C and C++ the formatter writes: the backend's indentation and line breaks,
/// the includes in the order the declarations need, and a function's body below its definition
/// line, where the table points.
const CLANG_FORMAT_STYLE: &str = "{BasedOnStyle: LLVM, IndentWidth: 4, ColumnLimit: 0, SortIncludes: false, ReflowComments: false, \
     AllowShortFunctionsOnASingleLine: None, AllowShortBlocksOnASingleLine: Never, AllowShortIfStatementsOnASingleLine: WithoutElse, \
     AllowShortLoopsOnASingleLine: false}";

/// Passes the source files through their formatter and moves each function's line to where its
/// definition went, so the table still opens it. A file the tool refuses, or whose functions
/// cannot be found again by their comments, stays as the backend wrote it.
pub fn format_files(files: &mut [EmittedFile], functions: &[EmittedFunction]) {
    format_each(files, functions, |_| {});
}

/// [`format_files`], handing each file to `done` as soon as its last piece is formatted, on the
/// thread that formatted it, while the pieces of the files after it are still with the formatters:
/// a caller writes a file and drops its text there, so the formatted output is never whole in
/// memory. The files are left as `done` leaves them.
pub(crate) fn format_each<F: Fn(&mut EmittedFile) + Sync>(files: &mut [EmittedFile], functions: &[EmittedFunction], done: F) {
    let above: HashMap<&str, usize> = functions.iter().map(|f| (f.ir_name.as_str(), f.header_lines)).collect();
    // A formatter holds 100 to 250 times its input in memory: a large program's 25 MB header took
    // more than 4 GB of it. So each file goes to the formatter in pieces cut between its top-level
    // declarations, where formatting one piece cannot change another, and the pieces of all the
    // files run side by side, in the files' order.
    let pieces: Vec<Vec<Range<usize>>> = files
        .iter()
        .map(|file| {
            let base = file.text.as_ptr().addr();
            let range = |p: &&str| {
                let start = p.as_ptr().addr() - base;
                start..start + p.len()
            };
            pieces(&file.text).iter().map(range).collect()
        })
        .collect();
    let jobs: Vec<(usize, usize)> = pieces.iter().enumerate().flat_map(|(n, p)| (0..p.len()).map(move |k| (n, k))).collect();
    let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    // Each file's formatted pieces, and how many are still out; a file is finished by the thread
    // that brings its last piece back.
    let pending: Vec<Mutex<(Vec<Option<String>>, usize)>> = pieces.iter().map(|p| Mutex::new((vec![None; p.len()], p.len()))).collect();
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(MAX_FORMATTERS).min(jobs.len().max(1));
    let files: Vec<Mutex<&mut EmittedFile>> = files.iter_mut().map(Mutex::new).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    // The pieces being formatted at once stay within a budget of source bytes: a piece waits until
    // the ones in flight leave it room, and one larger than the budget, a single huge function,
    // runs alone.
    let in_flight = (Mutex::new(0usize), std::sync::Condvar::new());
    let (jobs, pieces, names, pending, files, next, in_flight, above, done) =
        (&jobs, &pieces, &names, &pending, &files, &next, &in_flight, &above, &done);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(move || {
                loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(file, piece)) = jobs.get(n) else { return };
                    let range = pieces[file][piece].clone();
                    let (bytes, room) = in_flight;
                    let mut held = room
                        .wait_while(locked(bytes), |held| *held > 0 && *held + range.len() > FORMAT_BUDGET)
                        .unwrap_or_else(PoisonError::into_inner);
                    *held += range.len();
                    drop(held);
                    let input = locked(&files[file]).text[range.clone()].to_owned();
                    let text = formatted(&names[file], input);
                    *locked(bytes) -= range.len();
                    room.notify_all();
                    let mut state = locked(&pending[file]);
                    state.0[piece] = text;
                    state.1 -= 1;
                    if state.1 > 0 {
                        continue;
                    }
                    let parts = std::mem::take(&mut state.0);
                    drop(state);
                    let mut file = locked(&files[file]);
                    finish(&mut file, parts, above);
                    done(&mut file);
                }
            });
        }
    });
}

/// The lock of `mutex`, whether or not a formatter thread panicked holding it.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Puts the formatted pieces `parts` of `file` in its place, with its functions' lines moved to
/// where their definitions went; the file stays as it was when a piece was refused or a function
/// cannot be found again.
fn finish(file: &mut EmittedFile, parts: Vec<Option<String>>, above: &HashMap<&str, usize>) {
    let Some(parts) = parts.into_iter().collect::<Option<Vec<String>>>() else { return };
    let text = joined(parts);
    let Some(functions) = relocated(&file.text, &text, &file.functions, above) else { return };
    file.text = text;
    file.functions = functions;
}

/// The most formatters at once.
const MAX_FORMATTERS: usize = 8;

/// The source bytes the formatters at once may hold: `clang-format` took up to 250 times its
/// input, so this keeps them near 2 GB.
const FORMAT_BUDGET: usize = 8 << 20;

/// The source bytes a piece of a file holds before it is cut at the next top-level blank line.
const PIECE_BYTES: usize = 1 << 20;

/// `text` cut into pieces of about [`PIECE_BYTES`], each ending before a blank line that lies
/// between top-level declarations: outside every brace, parenthesis, comment, literal and
/// preprocessor conditional. The blank lines between the pieces are left out; [`joined`] puts one
/// back. A text with no such line is one piece.
fn pieces(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let (mut depth, mut conditionals) = (0i64, 0i64);
    let (mut start, mut at, mut line_start) = (0usize, 0usize, true);
    let mut out = Vec::new();
    while at < bytes.len() {
        let b = bytes[at];
        if line_start {
            line_start = false;
            let line_end = text[at..].find('\n').map_or(bytes.len(), |n| at + n);
            let line = text[at..line_end].trim();
            if line.is_empty() {
                if depth == 0 && conditionals == 0 && at - start >= PIECE_BYTES {
                    out.push(&text[start..at]);
                    start = (line_end + 1).min(bytes.len());
                }
                at = line_end;
                continue;
            }
            if let Some(directive) = line.strip_prefix('#') {
                let directive = directive.trim_start();
                if directive.starts_with("if") {
                    conditionals += 1;
                } else if directive.starts_with("endif") {
                    conditionals -= 1;
                }
                // A directive runs to the end of its line, and on past a line ending in `\`.
                let mut end = line_end;
                while end < bytes.len() && text[..end].trim_end_matches('\r').ends_with('\\') {
                    end = text[end + 1..].find('\n').map_or(bytes.len(), |n| end + 1 + n);
                }
                at = end;
                continue;
            }
        }
        match b {
            b'\n' => line_start = true,
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b'"' | b'\'' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b && bytes[at] != b'\n' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                if bytes.get(at) == Some(&b) {
                    at += 1;
                }
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find('\n').map_or(bytes.len(), |n| at + n);
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = text[at + 2..].find("*/").map_or(bytes.len(), |n| at + 2 + n + 2);
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    if start < bytes.len() || out.is_empty() {
        out.push(&text[start.min(bytes.len())..]);
    }
    out
}

/// The formatted pieces of a file as one text, a blank line between each two.
fn joined(parts: Vec<String>) -> String {
    let mut text = String::with_capacity(parts.iter().map(|p| p.len() + 2).sum());
    for part in parts {
        let part = part.trim_matches('\n');
        if part.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(part);
        text.push('\n');
    }
    text
}

/// `text`, the file `name` of a translation, as its language's formatter writes it; `None` when
/// the formatter is not on the PATH, fails, or the file is no source.
fn formatted(name: &str, text: String) -> Option<String> {
    let extension = std::path::Path::new(name).extension().and_then(|x| x.to_str()).unwrap_or_default();
    let is = |x: &str| extension.eq_ignore_ascii_case(x);
    let mut command = if is("rs") {
        let mut command = Command::new("rustfmt");
        command.args(["--edition", "2024"]);
        command
    } else if ["c", "cpp", "h"].into_iter().any(is) {
        let mut command = Command::new("clang-format");
        command.arg(format!("--style={CLANG_FORMAT_STYLE}")).arg(format!("--assume-filename={name}"));
        command
    } else {
        return None;
    };
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    // The input is written from its own thread: a large file fills the output pipe before the
    // formatter has read all of it.
    let writer = std::thread::spawn(move || stdin.write_all(text.as_bytes()));
    let output = child.wait_with_output().ok()?;
    writer.join().ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(if is("rs") { text } else { hooks_on_heads(&text) })
}

/// `text` with each function's entry hook back on its definition line, where the backend writes
/// it: the formatter breaks the line after the brace, and a line of its own in every function is
/// a quarter of a million lines of a large port.
fn hooks_on_heads(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let hook = line.trim();
        // The line before opens a body at the top level: a definition's.
        let head = out.strip_suffix('\n').map_or("", |o| o.rsplit('\n').next().unwrap_or(o));
        if hook.starts_with("RA_ENTER(\"") && hook.ends_with("\");") && head.ends_with(") {") && !head.starts_with(char::is_whitespace) {
            out.pop();
            out.push(' ');
            out.push_str(line.trim_start());
            continue;
        }
        out.push_str(line);
    }
    out
}

/// The definition lines of `functions` in `new`, the formatted `old`: each function is found again
/// by the comment above it, in order. A function without a comment has no address, and the table
/// lists none such; `None` when a comment is gone.
fn relocated(old: &str, new: &str, functions: &[(String, usize)], above: &HashMap<&str, usize>) -> Option<Vec<(String, usize)>> {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let mut from = 0;
    let mut out = Vec::with_capacity(functions.len());
    for (ir_name, line) in functions {
        let above = above.get(ir_name.as_str()).copied().unwrap_or(0);
        if above == 0 {
            continue;
        }
        let comment = old.get(line.checked_sub(above + 1)?)?.trim();
        let at = from + new.get(from..)?.iter().position(|l| l.trim() == comment)?;
        out.push((ir_name.clone(), at + 1 + above));
        from = at + 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_cut_only_between_top_level_declarations() {
        let filler = format!("// {}\n", "x".repeat(PIECE_BYTES));
        let text = format!(
            "{filler}int f(void) {{\n    int a = 1;\n\n    return a;\n}}\n\n#if X\n{filler}\nint g;\n#endif\n\n/* {filler}\n\n */\nconst char *s = \"{{\";\n\nint h;\n"
        );
        let parts = pieces(&text);
        let ends: Vec<&str> = parts.iter().map(|p| &p[p.len().saturating_sub(12)..]).collect();
        assert_eq!(
            ends,
            ["  return a;\n}\n", "nt g;\n#endif\n", "char *s = \"{\";\n", "int h;\n"].map(|e| &e[e.len().saturating_sub(12)..])
        );
        let back = joined(parts.iter().map(|p| (*p).to_owned()).collect());
        assert_eq!(back.replace("\n\n", "\n"), text.replace("\n\n", "\n"));
    }

    #[test]
    fn the_entry_hook_goes_back_on_the_definition_line() {
        let formatted = "// 0x1000\nuint64_t F_1000(uint64_t a) {\n    RA_ENTER(\"F_1000\");\n    if (a) {\n        RA_ENTER(\"x\");\n    }\n    return a;\n}\n";
        assert_eq!(
            hooks_on_heads(formatted),
            "// 0x1000\nuint64_t F_1000(uint64_t a) { RA_ENTER(\"F_1000\");\n    if (a) {\n        RA_ENTER(\"x\");\n    }\n    return a;\n}\n"
        );
    }
}
