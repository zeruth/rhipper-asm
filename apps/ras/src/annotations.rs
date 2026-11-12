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
//!   "notes": "recursive in the reference",
//!   "identity": {"callees": [], "callers": ["entry", "string \"A\""], "local": ["recursive"]}
//! }
//! ```
//!
//! The files live under git with the port's sources. One file per function and one key per line
//! make two people's changes merge as text, and a conflict is one function's file, which reads as
//! a warning naming it until it is resolved, the other functions as annotated. `name` renames the
//! function in the IR, the source and the function table (the emit and lift steps, `ras emit
//! --project`, `ras serve`); `hand: true` gives it to the hand-written sources beside the `hand`
//! list of `rhipper.json`; `type`, `comment` and `notes` are the person's, kept for the workbench.
//! `identity` is the engine's: the function apart from its address
//! ([`ra_asm_analysis::identity`]), written with the annotation and kept up with each build, by
//! which a new build of the reference (a patch, another optimization level) finds the function
//! where it moved ([`reattach`]); a file without one is of the function at its address.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use ra_asm_analysis::{Analysis, Identity, Kind};
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
    /// The function apart from its address, as the build the annotation was last applied to has it.
    pub identity: Option<Identity>,
}

impl Annotation {
    /// Whether the annotation says nothing, so its file goes: the identity is no saying.
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
            identity: json.get("identity").and_then(parse_identity),
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
        if let Some(identity) = &self.identity {
            lines.push(format!("  \"identity\": {}", identity_json(identity)));
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

/// An identity as one line of JSON: the `identity` of a sidecar file and of `emitted.json`.
pub(crate) fn identity_json(identity: &Identity) -> Value {
    let mut json = json!({"local": identity.local, "callers": identity.callers, "callees": identity.callees});
    if let Some(symbol) = &identity.symbol {
        json["symbol"] = json!(symbol);
    }
    json
}

/// The identity [`identity_json`] wrote; `None` when the JSON is no identity.
pub(crate) fn parse_identity(json: &Value) -> Option<Identity> {
    let list = |key: &str| -> Option<Vec<String>> {
        json.get(key).map_or(Some(Vec::new()), |v| v.as_array()?.iter().map(|f| f.as_str().map(str::to_owned)).collect())
    };
    json.is_object().then_some(())?;
    Some(Identity {
        symbol: json.get("symbol").and_then(Value::as_str).map(str::to_owned),
        local: list("local")?,
        callers: list("callers")?,
        callees: list("callees")?,
    })
}

/// Finds the function of each annotation in the build `analysis` is of, by the identity its file
/// keeps: a new build of the reference moves the functions, and the person's names, types,
/// comments and takeovers move with them, each file renamed for the new address and its identity
/// brought up to the build. An annotation without an identity is of the function at its address,
/// and takes the function's. One whose function the build no longer has, or has twice alike
/// ([`ra_asm_analysis::identity::find`]), or that would land on a function another annotation
/// holds, stays in its file applied to nothing, with a warning. Returns the annotations by the
/// address of their function in this build; the moves are among the warnings, for the log.
pub(crate) fn reattach(
    root: &Path,
    analysis: &Analysis<'_>,
    annotations: BTreeMap<u64, Annotation>,
    warnings: &mut Vec<String>,
) -> BTreeMap<u64, Annotation> {
    if annotations.is_empty() {
        return annotations;
    }
    let identities = ra_asm_analysis::identity::identities(analysis);
    let index = ra_asm_analysis::identity::Index::new(&identities);
    let mut placed: BTreeMap<u64, Annotation> = BTreeMap::new();
    let mut moving: Vec<(Annotation, Option<u64>)> = Vec::new();
    for annotation in annotations.into_values() {
        let here = identities.get(&annotation.address);
        match &annotation.identity {
            Some(identity) if here == Some(identity) => {
                placed.insert(annotation.address, annotation);
            }
            None => {
                let mut annotation = annotation;
                if let Some(identity) = here {
                    annotation.identity = Some(identity.clone());
                    if let Err(e) = write(root, &annotation) {
                        warnings.push(format!("{e:#}"));
                    }
                }
                placed.insert(annotation.address, annotation);
            }
            Some(identity) => {
                let found = index.find(identity);
                moving.push((annotation, found));
            }
        }
    }
    let label = |a: &Annotation| {
        let what = a.name.as_ref().map_or_else(String::new, |n| format!(" (`{n}`)"));
        format!("the annotation of {}{what}", format_address(a.address))
    };
    let targets: Vec<Option<u64>> = moving.iter().map(|(_, found)| *found).collect();
    let mut accepted: Vec<(Annotation, u64)> = Vec::new();
    for (annotation, found) in moving {
        match found {
            None => warnings
                .push(format!("{} is of no one function of this build: it stays in its file, applied to nothing", label(&annotation))),
            Some(to) if placed.contains_key(&to) || targets.iter().filter(|t| **t == Some(to)).count() > 1 => warnings.push(format!(
                "{} is of the function at {}, which another annotation holds: it stays in its file, applied to nothing",
                label(&annotation),
                format_address(to)
            )),
            Some(to) => accepted.push((annotation, to)),
        }
    }
    // A file at the new address that does not leave (a merge's conflict, an annotation that
    // stays where it was) is the person's: nothing writes over it.
    loop {
        let leaving: Vec<u64> = accepted.iter().filter(|(a, to)| a.address != *to).map(|(a, _)| a.address).collect();
        let (free, held): (Vec<_>, Vec<_>) =
            accepted.into_iter().partition(|(a, to)| a.address == *to || leaving.contains(to) || !file_of(root, *to).exists());
        accepted = free;
        if held.is_empty() {
            break;
        }
        for (annotation, to) in held {
            warnings.push(format!(
                "{} is of the function at {}, whose file is another's: it stays in its file, applied to nothing",
                label(&annotation),
                format_address(to)
            ));
        }
    }
    for (annotation, to) in &accepted {
        if annotation.address != *to {
            let old = file_of(root, annotation.address);
            if let Err(e) = std::fs::remove_file(&old) {
                warnings.push(format!("cannot remove {}: {e}", old.display()));
            }
            warnings.push(format!("{} moves to {} with its function", label(annotation), format_address(*to)));
        }
    }
    for (mut annotation, to) in accepted {
        annotation.address = to;
        annotation.identity = identities.get(&to).cloned();
        if let Err(e) = write(root, &annotation) {
            warnings.push(format!("{e:#}"));
        }
        placed.insert(to, annotation);
    }
    placed
}

/// Gives the functions of `analysis` the names of the project's name-source plugins, then those
/// of its annotations, which win, each found in the build of `analysis` by its identity
/// ([`reattach`]); returns the annotations and what could not be applied.
pub(crate) fn annotate(root: &Path, analysis: &mut Analysis<'_>) -> (BTreeMap<u64, Annotation>, Vec<String>) {
    let mut warnings = Vec::new();
    crate::plugins::apply_names(root, analysis, &mut warnings);
    let loaded = load(root, &mut warnings);
    let annotations = reattach(root, analysis, loaded, &mut warnings);
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
            identity: Some(Identity {
                symbol: None,
                local: vec!["recursive".into()],
                callers: vec!["entry".into(), "string \"A\\r\\n\"".into()],
                callees: Vec::new(),
            }),
        };
        let text = annotation.text();
        assert_eq!(text.lines().count(), 8, "{text}");
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
