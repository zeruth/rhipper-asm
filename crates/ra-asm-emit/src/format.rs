//! The written sources as their language's formatter writes them: `clang-format` for C and C++,
//! `rustfmt` for Rust, when the tool is on the PATH. `ras emit` and the oracles format the same
//! way, so what the oracles build is what the port is.

use std::collections::HashMap;
use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::c::{EmittedFile, EmittedFunction};

/// The style of the C and C++ the formatter writes: the backend's indentation and line breaks,
/// the includes in the order the declarations need, and a function's body below its definition
/// line, where the table points.
const CLANG_FORMAT_STYLE: &str = "{BasedOnStyle: LLVM, IndentWidth: 4, ColumnLimit: 0, SortIncludes: false, ReflowComments: false, \
     AllowShortFunctionsOnASingleLine: None, AllowShortBlocksOnASingleLine: Never, AllowShortIfStatementsOnASingleLine: Never, \
     AllowShortLoopsOnASingleLine: false}";

/// Passes the source files through their formatter and moves each function's line to where its
/// definition went, so the table still opens it. A file the tool refuses, or whose functions
/// cannot be found again by their comments, stays as the backend wrote it.
pub fn format_files(files: &mut [EmittedFile], functions: &[EmittedFunction]) {
    let above: HashMap<&str, usize> = functions.iter().map(|f| (f.ir_name.as_str(), f.header_lines)).collect();
    for file in files {
        let Some(text) = formatted(&file.name, &file.text) else { continue };
        let Some(functions) = relocated(&file.text, &text, &file.functions, &above) else { continue };
        file.text = text;
        file.functions = functions;
    }
}

/// `text`, the file `name` of a translation, as its language's formatter writes it; `None` when
/// the formatter is not on the PATH, fails, or the file is no source.
fn formatted(name: &str, text: &str) -> Option<String> {
    let mut command = if name.ends_with(".rs") {
        let mut command = Command::new("rustfmt");
        command.args(["--edition", "2024"]);
        command
    } else if [".c", ".cpp", ".h"].iter().any(|x| name.ends_with(x)) {
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
    let input = text.to_owned();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child.wait_with_output().ok()?;
    writer.join().ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
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
