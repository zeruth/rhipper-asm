//! `ras cards --pending`: the pending lists of the checks as roadmap cards. A list names what a
//! check accepts as failing today, one entry a line; its cards are mechanical, so they are
//! written here and not by hand: the title is the line, the acceptance is the list's replay with
//! the line removed, the files are the list and the source the list's kind points at. The output
//! is the outline `rc import` reads, a section per list.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context;

/// The pending lines one card takes at most: a card is one run of a worker.
const LINES_PER_CARD: usize = 10;

/// What a pending list is a backlog of, told from where it lies under `testdata`.
struct Kind {
    /// What a line names, in the plural: `vector forms`.
    what: &'static str,
    /// What is wrong with a listed entry today.
    fault: &'static str,
    /// The cargo command whose test reads the list and passes when exactly its entries fail.
    replay: String,
    /// The source a fix of an entry lands in.
    source: &'static str,
}

impl Kind {
    /// The kind of the list at `label`, a path with `/` between its parts; `None` for a file no
    /// check reads as a pending list.
    fn of(label: &str) -> Option<Self> {
        let mut parts = label.rsplit('/');
        let name = parts.next()?;
        let parent = parts.next().unwrap_or_default();
        let grand = parts.next().unwrap_or_default();
        Some(match (grand, parent, name) {
            (_, _, "TREE_PENDING.txt") => Self {
                what: "trees",
                fault: "differ from their function vectors through the tree interpreter",
                replay: "cargo test -q -p ra-asm-emit --test tree fixtures".to_owned(),
                source: "crates/ra-asm-emit/src/structure.rs",
            },
            (_, "csmith", "EMIT_PENDING.txt") => Self {
                what: "csmith programs",
                fault: "differ from their checksum through the C output",
                replay: "cargo test -q -p ra-asm-emit --features oracle --test csmith".to_owned(),
                source: "crates/ra-asm-emit/src/c.rs",
            },
            (_, "csmith", "PENDING.txt") => Self {
                what: "csmith programs",
                fault: "differ from their checksum in the interpreter",
                replay: "cargo test -q -p ra-asm-host --test csmith".to_owned(),
                source: "crates/ra-asm-lift/src/raise/mod.rs",
            },
            (_, "levels", "PENDING.txt") => Self {
                what: "level violations",
                fault: "violate their level's checker",
                replay: "cargo test -q -p ra-asm-lift --test levels fixtures".to_owned(),
                source: "crates/ra-asm-lift/src/raise/mod.rs",
            },
            ("vectors", machine, "PENDING.txt") => Self {
                what: "vector forms",
                fault: "differ from the processor in the replay",
                replay: format!("cargo test -q -p ra-asm-lift --test vectors {machine}"),
                source: lift_of(machine),
            },
            _ => return None,
        })
    }
}

/// The lift's source for the vectors of `machine`, a directory of `testdata/vectors`.
fn lift_of(machine: &str) -> &'static str {
    match machine {
        "x86_64" | "i386" | "i686" | "x86" => "crates/ra-asm-lift/src/x86.rs",
        "aarch64" | "arm64" => "crates/ra-asm-lift/src/arm64.rs",
        "arm" | "thumb" => "crates/ra-asm-lift/src/thumb.rs",
        _ if machine.starts_with("x87") => "crates/ra-asm-lift/src/x87.rs",
        _ => "crates/ra-asm-lift/src/lib.rs",
    }
}

/// Whether a file name is a pending list's: `PENDING.txt`, or a second list of a directory as
/// `<WHAT>_PENDING.txt`.
fn is_list(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n == "PENDING.txt" || n.ends_with("_PENDING.txt"))
}

/// The pending lists under `dir`, in the order of their paths.
fn walk(dir: &Path, lists: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        entries.push(entry?.path());
    }
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, lists)?;
        } else if is_list(&path) {
            lists.push(path);
        }
    }
    Ok(())
}

/// The path a card names the list by: from `testdata` on when the list lies under one, so the
/// cards read the same wherever the command ran; the path as given otherwise.
fn label(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = text.split('/').collect();
    match parts.iter().rposition(|p| *p == "testdata") {
        Some(start) => parts[start..].join("/"),
        None => text,
    }
}

/// The entries of a list: its lines, without the blank ones and the `#` comments.
fn entries(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

/// What an entry names: its first word; the rest of the line is why it is pending.
fn name(entry: &str) -> &str {
    entry.split_whitespace().next().unwrap_or(entry).trim_end_matches(':')
}

/// The cards of one list as a section of the outline; nothing for a list without entries.
fn section(out: &mut String, label: &str, kind: &Kind, entries: &[&str]) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    writeln!(out, "### The {} pending in {label}\n", kind.what)?;
    for lines in entries.chunks(LINES_PER_CARD) {
        let (count, what, fault, replay, source) = (lines.len(), kind.what, kind.fault, &kind.replay, kind.source);
        match lines {
            [line] => writeln!(out, "- {line} {{c2}}")?,
            [first, ..] => writeln!(out, "- {} and {} more {what} {{c2}}", name(first), count - 1)?,
            [] => continue,
        }
        let these = if count == 1 { "this line" } else { "these lines" };
        writeln!(
            out,
            "  objective: {label} lists {count} of the {what} that {fault} today: {}. Each is fixed at its cause and its line leaves \
             the list; one that is not this card's to fix keeps its line, and the report says why.",
            lines.join("; ")
        )?;
        writeln!(out, "  acceptance: `{replay}` prints `test result: ok` with {these} removed from {label}.")?;
        writeln!(out, "  files: {label} {source}\n")?;
    }
    Ok(())
}

/// The outline of the pending lists at `paths`, each a list or a directory searched for them,
/// under the heading of `milestone` when one is given.
fn outline(paths: &[PathBuf], milestone: Option<&str>) -> anyhow::Result<String> {
    let mut lists = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut lists)?;
        } else {
            lists.push(path.clone());
        }
    }
    let mut out = String::new();
    if let Some(milestone) = milestone {
        writeln!(out, "## {milestone}\n")?;
    }
    for path in &lists {
        let label = label(path);
        let kind = Kind::of(&label).with_context(|| {
            format!(
                "{label} is not a pending list `ras cards` knows: a list is testdata/vectors/<machine>/PENDING.txt, \
                 testdata/vectors/fn/TREE_PENDING.txt, testdata/levels/PENDING.txt, testdata/csmith/PENDING.txt or \
                 testdata/csmith/EMIT_PENDING.txt"
            )
        })?;
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        section(&mut out, &label, &kind, &entries(&text))?;
    }
    Ok(out)
}

/// Prints the cards of the pending lists at `paths`; of `testdata` without any.
pub(crate) fn pending(paths: &[PathBuf], milestone: Option<&str>) -> anyhow::Result<()> {
    let default = [PathBuf::from("testdata")];
    print!("{}", outline(if paths.is_empty() { &default[..] } else { paths }, milestone)?);
    Ok(())
}
