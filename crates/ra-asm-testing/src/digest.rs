//! The comparison of an oracle's output with its expectation, which fails in one line: the first
//! differing line with the three lines before it, in fewer than [`LIMIT`] characters, so that a
//! check's cut log still shows where. Both whole sides go to `out/failed/<test>/expected.txt` and
//! `actual.txt`, and the line itself is appended to `out/failed/digest.txt`, one per failed
//! comparison. Line endings are normalized first: a CRLF text equals its LF twin.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The bound of a digest line, in characters: the line is always shorter.
pub const LIMIT: usize = 200;

/// The lines before the differing one that a digest line carries.
const CONTEXT: usize = 3;

/// The first line where two texts part, numbered from 1. A side that ended before it is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub line: usize,
    pub expected: Option<String>,
    pub actual: Option<String>,
    /// Up to three lines before `line`, which both sides share, the earliest first.
    pub context: Vec<String>,
}

/// The text with every CRLF and lone CR made LF.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Where `expected` and `actual` first differ once their line endings are normalized, or `None`
/// when they are equal. A missing final newline counts as a difference on the line after the last.
#[must_use]
pub fn first_difference(expected: &str, actual: &str) -> Option<Difference> {
    let (expected, actual) = (normalize(expected), normalize(actual));
    if expected == actual {
        return None;
    }
    let left: Vec<&str> = expected.split('\n').collect();
    let right: Vec<&str> = actual.split('\n').collect();
    let index = (0..left.len().max(right.len())).find(|&i| left.get(i) != right.get(i))?;
    Some(Difference {
        line: index + 1,
        expected: left.get(index).map(|s| (*s).to_owned()),
        actual: right.get(index).map(|s| (*s).to_owned()),
        context: left[index.saturating_sub(CONTEXT)..index].iter().map(|s| (*s).to_owned()).collect(),
    })
}

/// Compares `actual` with `expected` for `test` and, on a difference, writes both sides under the
/// workspace's `out/failed/<test>/`, appends the digest line to `out/failed/digest.txt` and returns
/// that line. An equal comparison removes what an earlier failure of the test left.
///
/// # Errors
/// The digest line of the first difference.
pub fn compare(test: &str, expected: &str, actual: &str) -> Result<(), String> {
    compare_in(&failed_dir(), test, expected, actual)
}

/// [`compare`] with `root` in place of `out/failed`.
///
/// # Errors
/// The digest line of the first difference.
pub fn compare_in(root: &Path, test: &str, expected: &str, actual: &str) -> Result<(), String> {
    let dir = root.join(file_name(test));
    let Some(difference) = first_difference(expected, actual) else {
        let _ = std::fs::remove_dir_all(&dir);
        return Ok(());
    };
    let line = digest_line(test, &difference);
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("expected.txt"), expected))
        .and_then(|()| std::fs::write(dir.join("actual.txt"), actual))
        .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(root.join("digest.txt")))
        .and_then(|mut digest| digest.write_all(format!("{line}\n").as_bytes()));
    if let Err(e) = written {
        eprintln!("{}: not written: {e}", dir.display());
    }
    Err(line)
}

/// The workspace's `out/failed`.
#[must_use]
pub fn failed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/failed")
}

/// The one line that names `test`, the differing line and the lines before it, clipped to fewer
/// than [`LIMIT`] characters: the two sides of the line keep their room before the context does.
#[must_use]
pub fn digest_line(test: &str, difference: &Difference) -> String {
    let side = |s: Option<&str>| s.map_or_else(|| "<end>".to_owned(), |s| format!("{s:?}"));
    let head = format!("{test}: line {}: ", difference.line);
    let room = (LIMIT - 1).saturating_sub(head.chars().count() + "expected  got ".len() + " after ".len());
    let expected = clip(&side(difference.expected.as_deref()), room / 3);
    let actual = clip(&side(difference.actual.as_deref()), room / 3);
    let mut line = format!("{head}expected {expected} got {actual}");
    if !difference.context.is_empty() {
        let context: Vec<String> = difference.context.iter().map(|s| format!("{s:?}")).collect();
        line.push_str(" after ");
        line.push_str(&context.join(" | "));
    }
    clip(&line, LIMIT - 1)
}

/// `text` in at most `width` characters, its tail replaced by an ellipsis when it is longer.
fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(width.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

/// A test's name as a directory name: a character a path does not take becomes `_`.
fn file_name(test: &str) -> String {
    test.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect()
}
