//! The person's annotations of a project's functions: a name, a type, a comment, a takeover and
//! notes, each function's in a sidecar file of its own under `rhipper/annotations/`, named by the
//! function's address in the reference (`0x140001330.json`), which is its identity across runs of
//! the analysis:
//!
//! ```json
//! {
//!   "addr": "0x140001330",
//!   "name": "fib",
//!   "type": "int fib(int n)",
//!   "comment": "the n-th Fibonacci number",
//!   "hand": true,
//!   "notes": "recursive in the reference"
//! }
//! ```
//!
//! The files live under git with the port's sources. One file per function and one key per line
//! make two people's changes merge as text, and a conflict is one function's file, which reads as
//! a warning naming it until it is resolved, the other functions as annotated. `name` renames the
//! function in the IR, the source and the function table (the emit and lift steps, `ras emit
//! --project`, `ras serve`); `hand: true` gives it to the hand-written sources beside the `hand`
//! list of `rhipper.json`; `type`, `comment` and `notes` are the person's, kept for the workbench.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use ra_asm_analysis::{Analysis, Kind};
use ra_asm_emit::functions::format_address;
use serde_json::{Value, json};

/// Where the sidecar files live, under the project's root.
pub(crate) const ANNOTATIONS_DIR: &str = "rhipper/annotations";

/// What the person says of one function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Annotation {
    pub address: u64,
    /// The function's name in the port, in place of the analysis's.
    pub name: Option<String>,
    /// The function's type, as a C prototype.
    pub ty: Option<String>,
    pub comment: Option<String>,
    /// Whether the hand-written sources own the function.
    pub hand: bool,
    pub notes: Option<String>,
}

impl Annotation {
    /// Whether the annotation says nothing, so its file goes.
    pub(crate) fn is_empty(&self) -> bool {
        self.name.is_none() && self.ty.is_none() && self.comment.is_none() && !self.hand && self.notes.is_none()
    }

    /// The annotation of a sidecar file's JSON; `address` is the file's, when the JSON has none.
    fn parse(json: &Value, address: Option<u64>) -> Option<Self> {
        let text = |key: &str| json.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
        let address = text("addr").as_deref().and_then(crate::project::hand_address).or(address)?;
        Some(Self {
            address,
            name: text("name"),
            ty: text("type"),
            comment: text("comment"),
            hand: json.get("hand").and_then(Value::as_bool).unwrap_or(false),
            notes: text("notes"),
        })
    }

    /// The annotation as the workbench reads it.
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "addr": format_address(self.address), "name": self.name, "type": self.ty,
            "comment": self.comment, "hand": self.hand, "notes": self.notes,
        })
    }

    /// The text of the sidecar file: the keys in one order, one per line, so that changes to two
    /// keys of a function merge.
    fn text(&self) -> String {
        let mut lines = vec![format!("  \"addr\": {}", json!(format_address(self.address)))];
        let mut key = |name: &str, value: Option<&String>| {
            if let Some(value) = value {
                lines.push(format!("  \"{name}\": {}", json!(value)));
            }
        };
        key("name", self.name.as_ref());
        key("type", self.ty.as_ref());
        key("comment", self.comment.as_ref());
        if self.hand {
            lines.push("  \"hand\": true".to_owned());
        }
        if let Some(notes) = &self.notes {
            lines.push(format!("  \"notes\": {}", json!(notes)));
        }
        let mut out = String::from("{\n");
        let _ = writeln!(out, "{}", lines.join(",\n"));
        out.push_str("}\n");
        out
    }
}

/// The sidecar directory of the project at `root`.
pub(crate) fn dir(root: &Path) -> PathBuf {
    root.join(ANNOTATIONS_DIR)
}

/// The sidecar file of the function at `address`.
pub(crate) fn file_of(root: &Path, address: u64) -> PathBuf {
    dir(root).join(format!("{}.json", format_address(address)))
}

/// Every annotation of the project at `root`, by address. A file that is not an annotation (a
/// merge left its conflict in it) is a warning naming it, and its function goes unannotated.
pub(crate) fn load(root: &Path, warnings: &mut Vec<String>) -> BTreeMap<u64, Annotation> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir(root)) else { return out };
    let mut paths: Vec<PathBuf> =
        entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    paths.sort();
    for path in paths {
        let address = path.file_stem().and_then(|s| s.to_str()).and_then(crate::project::hand_address);
        let parsed = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        match parsed.as_ref().and_then(|json| Annotation::parse(json, address)) {
            Some(annotation) => {
                out.insert(annotation.address, annotation);
            }
            None => warnings.push(format!(
                "the annotation {} is no annotation (a merge conflict?): its function is left as the analysis has it",
                path.display()
            )),
        }
    }
    out
}

/// The sidecar files of the project at `root` with the times they changed: what `ras serve`
/// compares to see a checkout or a merge change the annotations under it.
pub(crate) fn stamp(root: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    let Ok(entries) = std::fs::read_dir(dir(root)) else { return Vec::new() };
    let mut out: Vec<(PathBuf, Option<SystemTime>)> =
        entries.filter_map(Result::ok).map(|e| (e.path(), e.metadata().and_then(|m| m.modified()).ok())).collect();
    out.sort();
    out
}

/// Writes `annotation` to its sidecar file, or removes the file when the annotation says nothing.
pub(crate) fn write(root: &Path, annotation: &Annotation) -> Result<PathBuf> {
    let file = file_of(root, annotation.address);
    if annotation.is_empty() {
        if file.is_file() {
            std::fs::remove_file(&file).with_context(|| format!("cannot remove {}", file.display()))?;
        }
        return Ok(file);
    }
    std::fs::create_dir_all(dir(root)).with_context(|| format!("cannot create {}", dir(root).display()))?;
    std::fs::write(&file, annotation.text()).with_context(|| format!("cannot write {}", file.display()))?;
    Ok(file)
}

/// Why `name` cannot name the function at `address`: not a C identifier, a name the port's
/// runtime, the host's C library or a language owns, the lift's own prefixes, or another
/// function's name. `None` when it can.
pub(crate) fn name_fault(analysis: &Analysis<'_>, address: u64, name: &str) -> Option<String> {
    let identifier =
        !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !identifier {
        return Some(format!("`{name}` is no C identifier"));
    }
    if ra_asm_analysis::symbols::is_reserved(name) || name.starts_with("F_") || name.starts_with("U_") {
        return Some(format!("`{name}` is a name the port's runtime, the host's C library or a language owns"));
    }
    analysis
        .functions
        .values()
        .find(|f| f.entry != address && f.name == name)
        .map(|f| format!("`{name}` names the function at {} already", format_address(f.entry)))
}

/// Gives the functions the names `annotations` give them. A name for no function of the image, for
/// a function of the host's C runtime (which the port calls by the host's name) or one the function
/// cannot take ([`name_fault`]) is a warning, and the function keeps the analysis's name.
pub(crate) fn apply_names(analysis: &mut Analysis<'_>, annotations: &BTreeMap<u64, Annotation>, warnings: &mut Vec<String>) {
    for annotation in annotations.values() {
        let Some(name) = &annotation.name else { continue };
        let at = format_address(annotation.address);
        let fault = match analysis.functions.get(&annotation.address) {
            None => Some(format!("it names `{name}`, but no function starts there")),
            Some(f) if matches!(f.kind, Kind::Runtime(_)) => {
                Some(format!("it names `{name}`, but the host's runtime supplies the function"))
            }
            Some(f) if f.name == *name => continue,
            Some(_) => name_fault(analysis, annotation.address, name).map(|fault| format!("{fault}; the function keeps its name")),
        };
        match fault {
            Some(fault) => warnings.push(format!("the annotation of {at}: {fault}")),
            None => {
                if let Some(f) = analysis.functions.get_mut(&annotation.address) {
                    f.name.clone_from(name);
                }
            }
        }
    }
}

/// Reads the annotations of the project at `root` and gives the functions of `analysis` their
/// names; returns the annotations and what could not be applied.
pub(crate) fn annotate(root: &Path, analysis: &mut Analysis<'_>) -> (BTreeMap<u64, Annotation>, Vec<String>) {
    let mut warnings = Vec::new();
    let annotations = load(root, &mut warnings);
    apply_names(analysis, &annotations, &mut warnings);
    (annotations, warnings)
}

/// The functions `annotations` give the hand-written sources, as entries of the `hand` list.
pub(crate) fn takeovers(annotations: &BTreeMap<u64, Annotation>) -> Vec<String> {
    annotations.values().filter(|a| a.hand).map(|a| format_address(a.address)).collect()
}

/// The project's `hand` list with the takeovers of its annotations, each function once.
pub(crate) fn hand_list(hand: &[String], annotations: &BTreeMap<u64, Annotation>) -> Vec<String> {
    let mut out = hand.to_vec();
    for entry in takeovers(annotations) {
        if !out.iter().any(|h| crate::project::same_function(h, &entry)) {
            out.push(entry);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_annotation_reads_back_as_it_was_written() {
        let annotation = Annotation {
            address: 0x1_4000_1330,
            name: Some("fib".into()),
            ty: Some("int fib(int n)".into()),
            comment: None,
            hand: true,
            notes: Some("two lines\nof \"notes\"".into()),
        };
        let text = annotation.text();
        assert_eq!(text.lines().count(), 7, "{text}");
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(Annotation::parse(&json, None), Some(annotation));
    }

    #[test]
    fn the_file_name_is_the_address_when_the_json_has_none() {
        let json = json!({"name": "add"});
        assert_eq!(Annotation::parse(&json, Some(0x401000)).map(|a| a.address), Some(0x401000));
        assert_eq!(Annotation::parse(&json, None), None);
    }
}
