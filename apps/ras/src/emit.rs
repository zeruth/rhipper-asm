//! `ras emit`: source out of an image, in C, C++ or Rust, with the function table beside it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_emit::{EmittedFile, EmittedFunction, FunctionInfo};
use ra_asm_lift::Lifted;
use ra_asm_pe::Image;

use crate::annotations::Annotation;

/// The C runtime, written beside a C or C++ translation; a Rust translation carries its own.
const RHIPPER_H: &str = include_str!("../../../runtime/rhipper.h");
const DRIVER_C: &str = include_str!("../../../runtime/driver.c");

/// A translation: the files to write, the functions in them, and what the analysis knows.
pub(crate) struct Translation {
    pub files: Vec<EmittedFile>,
    pub functions: Vec<EmittedFunction>,
    pub infos: HashMap<String, FunctionInfo>,
    /// The runtime files, by path under the output directory.
    pub runtime: Vec<(&'static str, &'static str)>,
    /// The image's resources, by path under the output directory: `resources.res` for the link,
    /// and each one as a file under `resources/` for reading.
    pub resources: Vec<(String, Vec<u8>)>,
    /// The module definition file of a DLL's port, `exports.def`: the image's exports as the
    /// port's. `None` for a program.
    pub exports: Option<String>,
    /// What the translation could not carry over, such as an export at no symbol of the port.
    pub warnings: Vec<String>,
}

impl Translation {
    /// The whole translation as text, for the standard output.
    pub(crate) fn text(&self) -> String {
        if self.files.len() == 1 {
            return self.files[0].text.clone();
        }
        let mut out = String::new();
        for f in &self.files {
            out.push_str("// ==== ");
            out.push_str(&f.name);
            out.push('\n');
            out.push_str(&f.text);
            out.push('\n');
        }
        out
    }

    pub(crate) fn instructions_not_lifted(&self) -> usize {
        self.functions.iter().map(|f| f.traps).sum()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.files.iter().map(|f| f.text.len()).sum()
    }

    /// Writes the files, the runtime and the function table into `dir`.
    pub(crate) fn write(&self, dir: &Path) -> Result<()> {
        ra_asm_emit::write_files(dir, &self.files).with_context(|| format!("cannot write into {}", dir.display()))?;
        let exports = self.exports.iter().map(|d| ("exports.def", d.as_bytes()));
        let files =
            self.runtime.iter().map(|(p, t)| (*p, t.as_bytes())).chain(self.resources.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
        for (path, bytes) in files.chain(exports) {
            let path = dir.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        Ok(())
    }

    pub(crate) fn table(&self) -> String {
        ra_asm_emit::table(&self.functions, &self.files, &self.infos)
    }
}

/// What a translation is asked for, beyond the image and its lift.
pub(crate) struct Options<'a> {
    /// The language: `c`, `cpp` or `rust`.
    pub lang: &'a str,
    /// The name of a Rust package and its binary.
    pub package: &'a str,
    /// Instruments a C or C++ translation with the runtime's trace hooks.
    pub trace: bool,
    /// The functions the hand-written sources own, by source name or address, which a C or C++
    /// translation declares instead of defining.
    pub hand: &'a [String],
    /// The person's annotations of the project, by address, which the function table carries.
    pub annotations: &'a BTreeMap<u64, Annotation>,
}

/// Translates the raised module of `lifted` as `options` ask. `signatures` (by the final function
/// names) tell the C++ output which virtual functions read their object.
pub(crate) fn translate(
    image: &Image,
    analysis: &Analysis<'_>,
    lifted: &Lifted,
    signatures: &HashMap<String, ra_asm_lift::raise::abi::Signature>,
    options: &Options<'_>,
) -> Result<Translation> {
    let Options { lang, package, trace, hand, annotations } = *options;
    let mut infos = ra_asm_emit::functions::infos(analysis);
    for info in infos.values_mut() {
        if let Some(annotation) = annotations.get(&info.address) {
            info.annotated = true;
            info.ty.clone_from(&annotation.ty);
            info.comment.clone_from(&annotation.comment);
        }
    }
    let names: HashMap<String, String> = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let resources = resource_files(image);
    let mut warnings = Vec::new();
    let hand = hand_functions(&infos, hand, &mut warnings);
    let exports = image.headers.coff.is_dll().then(|| exports_def(image, lifted, &mut warnings));
    if lang.eq_ignore_ascii_case("rust") || lang.eq_ignore_ascii_case("rs") {
        if exports.is_some() {
            bail!("a DLL ports as C or C++: the Rust translation is a program");
        }
        if !hand.is_empty() {
            warnings.push(format!("{} hand-written functions are named, which a Rust port does not take: they are translated", hand.len()));
        }
        let imports = image.imports.iter().filter_map(|i| i.name.clone().map(|n| (n, i.dll.clone()))).collect();
        let entry = lifted.entry.and_then(|e| lifted.functions.get(&e).cloned());
        let options = ra_asm_emit::RustOptions {
            names,
            imports,
            entry,
            package: package.to_owned(),
            pointer_bits: u32::from(image.pointer_size()) * 8,
            resources: !image.resources.is_empty(),
            ..Default::default()
        };
        let output = ra_asm_emit::emit_rust(&lifted.module, &options);
        return Ok(Translation {
            files: output.files,
            functions: output.functions,
            infos,
            runtime: Vec::new(),
            resources,
            exports,
            warnings,
        });
    }
    let Some(dialect) = ra_asm_emit::Dialect::parse(lang) else {
        bail!("the language `{lang}` is not `c`, `cpp` or `rust`");
    };
    let classes = ra_asm_emit::classes(analysis, signatures);
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names, dialect, classes, trace, hand });
    let files = unit.files(ra_asm_emit::FUNCTIONS_PER_FILE);
    Ok(Translation {
        files,
        functions: unit.functions,
        infos,
        runtime: vec![("runtime/rhipper.h", RHIPPER_H), ("runtime/driver.c", DRIVER_C)],
        resources,
        exports,
        warnings,
    })
}

/// The IR names of the functions `hand` names, each by its source name, its IR name or its
/// address. An entry that names no function of the image is a warning: the port would still
/// build, with the lift's body where the person meant theirs.
fn hand_functions(infos: &HashMap<String, FunctionInfo>, hand: &[String], warnings: &mut Vec<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    for entry in hand {
        let address = crate::project::hand_address(entry);
        let found = infos.iter().find(|(ir, i)| i.name == *entry || **ir == *entry || address.is_some_and(|a| a == i.address));
        match found {
            Some((ir, _)) => {
                out.insert(ir.clone());
            }
            None => warnings.push(format!("hand-written function `{entry}` is no function of the image: nothing is left to the sources")),
        }
    }
    out
}

/// What an emit keeps of the functions it wrote, in the build directory, for the next emit to
/// compare with: one entry per function, its `addr`, `name`, `identity` and `text`.
pub(crate) const EMITTED_JSON: &str = "emitted.json";
/// The comparison of an emit with the last, for the workbench: `functions` (how many this emit
/// wrote), the counts `moved`, `changed`, `new` and `gone`, and `entries`, one per function that
/// is not as it was: its `name`, `addr` and `change` (`moved` to another address, `changed` in its
/// text at the same one, `new`, `gone`), `was` and `was_name` for the last emit's address and
/// name, and `diff`, the unified diff of its text when the text changed.
pub(crate) const REGENERATED_JSON: &str = "regenerated.json";
/// The same comparison as one unified diff, function by function, to read.
pub(crate) const REGENERATED_DIFF: &str = "regenerated.diff";

/// The context lines around a change in a diff.
const DIFF_CONTEXT: usize = 3;
/// The most cells of the table a diff of two texts fills; two texts longer than that differ as
/// wholes.
const DIFF_CELLS: usize = 1 << 22;
/// The most identity lookups times functions a regeneration spends pairing the functions that
/// moved; past that the rest pair by an exact identity and by name only.
const PAIRING_BUDGET: usize = 1 << 24;

/// One function of a translation, as one emit compares it with another's.
struct Written {
    address: u64,
    name: String,
    identity: Option<Identity>,
    text: String,
}

impl Written {
    fn to_json(&self) -> Value {
        let identity = self.identity.as_ref().map_or(Value::Null, crate::annotations::identity_json);
        json!({"addr": format_address(self.address), "name": self.name, "identity": identity, "text": self.text})
    }

    fn parse(json: &Value) -> Option<Self> {
        Some(Self {
            address: crate::project::hand_address(json.get("addr")?.as_str()?)?,
            name: json.get("name")?.as_str()?.to_owned(),
            identity: json.get("identity").and_then(crate::annotations::parse_identity),
            text: json.get("text")?.as_str()?.to_owned(),
        })
    }
}

/// Compares `translation`, of the build `analysis` is of, with the translation the last emit
/// wrote (its `emitted.json` in `dir`), and writes the comparison beside it: `regenerated.json`
/// and `regenerated.diff`. A function of the last emit is this emit's function of the same
/// identity ([`ra_asm_analysis::identity`]) or, failing that, of the same name when the name is
/// not the lift's own (a person's names follow their functions, [`crate::annotations::reattach`]);
/// one this emit's function two of the last pair with is neither's. Keeps this translation in
/// `emitted.json` for the next. Returns the line that sums the comparison up, `None` on the first
/// emit.
pub(crate) fn regenerate(dir: &Path, analysis: &Analysis<'_>, translation: &Translation) -> Result<Option<String>> {
    let identities = ra_asm_analysis::identity::identities(analysis);
    let now: Vec<Written> = translation
        .functions
        .iter()
        .filter_map(|f| {
            let address = f.address?;
            let name = translation.infos.get(&f.ir_name).map_or_else(|| f.ir_name.clone(), |i| i.name.clone());
            Some(Written { address, name, identity: identities.get(&address).cloned(), text: f.text.clone() })
        })
        .collect();
    let file = dir.join(EMITTED_JSON);
    let last: Option<Vec<Written>> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|json| Some(json.as_array()?.iter().filter_map(Written::parse).collect()));
    std::fs::create_dir_all(dir)?;
    let kept = Value::Array(now.iter().map(Written::to_json).collect());
    std::fs::write(&file, serde_json::to_string(&kept)?).with_context(|| format!("cannot write {}", file.display()))?;
    let Some(last) = last else { return Ok(None) };

    let by_address: HashMap<u64, usize> = now.iter().enumerate().map(|(n, w)| (w.address, n)).collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_identity: HashMap<&Identity, Vec<usize>> = HashMap::new();
    for (n, w) in now.iter().enumerate() {
        by_name.entry(w.name.as_str()).or_default().push(n);
        if let Some(identity) = &w.identity {
            by_identity.entry(identity).or_default().push(n);
        }
    }
    let index = ra_asm_analysis::identity::Index::new(&identities);
    let mut lookups = 0usize;
    let found: Vec<Option<usize>> = last
        .iter()
        .map(|old| {
            let identity = old.identity.as_ref()?;
            if let Some([one]) = by_identity.get(identity).map(Vec::as_slice) {
                return Some(*one);
            }
            lookups += 1;
            if lookups.saturating_mul(now.len()) > PAIRING_BUDGET {
                return None;
            }
            index.find(identity).and_then(|a| by_address.get(&a).copied())
        })
        .zip(&last)
        .map(|(found, old)| {
            found.or_else(|| match by_name.get(old.name.as_str()).map(Vec::as_slice) {
                Some([one]) if !old.name.starts_with("F_") => Some(*one),
                _ => None,
            })
        })
        .collect();
    let mut claims: HashMap<usize, usize> = HashMap::new();
    for n in found.iter().flatten() {
        *claims.entry(*n).or_default() += 1;
    }
    let found: Vec<Option<usize>> = found.into_iter().map(|f| f.filter(|n| claims[n] == 1)).collect();

    let (mut moved, mut changed, mut new, mut gone) = (0usize, 0usize, 0usize, 0usize);
    let mut entries = Vec::new();
    let mut diff = String::new();
    let mut paired = vec![false; now.len()];
    let mut entry = |change: &str, old: Option<&Written>, w: Option<&Written>, diff: &mut String| {
        let (before, after) = (old.map_or("", |o| o.text.as_str()), w.map_or("", |w| w.text.as_str()));
        let hunks = if before == after { String::new() } else { unified_diff(before, after) };
        let at = |w: Option<&Written>| w.map_or_else(|| "nothing".to_owned(), |w| format!("{} at {}", w.name, format_address(w.address)));
        if !hunks.is_empty() {
            let _ = write!(diff, "--- {}\n+++ {}\n{hunks}", at(old), at(w));
        }
        let mut e = json!({"name": w.or(old).map(|w| w.name.as_str()), "addr": w.map(|w| format_address(w.address)), "change": change});
        if let Some(old) = old {
            e["was"] = json!(format_address(old.address));
            if w.is_some_and(|w| w.name != old.name) {
                e["was_name"] = json!(old.name);
            }
        }
        if !hunks.is_empty() {
            e["diff"] = json!(hunks);
        }
        entries.push(e);
    };
    for (old, found) in last.iter().zip(&found) {
        match found {
            Some(n) => {
                paired[*n] = true;
                let w = &now[*n];
                if w.address != old.address {
                    moved += 1;
                    entry("moved", Some(old), Some(w), &mut diff);
                } else if w.text != old.text {
                    changed += 1;
                    entry("changed", Some(old), Some(w), &mut diff);
                }
            }
            None => {
                gone += 1;
                entry("gone", Some(old), None, &mut diff);
            }
        }
    }
    for (w, _) in now.iter().zip(&paired).filter(|(_, p)| !**p) {
        new += 1;
        entry("new", None, Some(w), &mut diff);
    }
    let summary = json!({
        "functions": now.len(), "moved": moved, "changed": changed, "new": new, "gone": gone, "entries": entries,
    });
    std::fs::write(dir.join(REGENERATED_JSON), serde_json::to_string_pretty(&summary)?)?;
    std::fs::write(dir.join(REGENERATED_DIFF), &diff)?;
    let line = if moved + changed + new + gone == 0 {
        format!("regenerated: the {} functions are as the last emit wrote them", now.len())
    } else {
        format!(
            "regenerated: {moved} functions moved, {changed} changed, {new} new and {gone} gone of {} -> {}",
            now.len(),
            dir.join(REGENERATED_DIFF).display()
        )
    };
    Ok(Some(line))
}

/// The unified diff of two texts, line by line: the hunks, without the file header.
fn unified_diff(old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let ops = line_ops(&a, &b);
    let changes: Vec<usize> = ops.iter().enumerate().filter(|(_, (kind, _))| *kind != ' ').map(|(n, _)| n).collect();
    let mut out = String::new();
    let mut k = 0;
    while k < changes.len() {
        let first = changes[k];
        let mut last = first;
        while k + 1 < changes.len() && changes[k + 1] - last <= 2 * DIFF_CONTEXT + 1 {
            k += 1;
            last = changes[k];
        }
        k += 1;
        let (lo, hi) = (first.saturating_sub(DIFF_CONTEXT), (last + DIFF_CONTEXT + 1).min(ops.len()));
        let count = |ops: &[(char, &str)], skip: char| ops.iter().filter(|(kind, _)| *kind != skip).count();
        let (old_start, new_start) = (count(&ops[..lo], '+'), count(&ops[..lo], '-'));
        let (old_len, new_len) = (count(&ops[lo..hi], '+'), count(&ops[lo..hi], '-'));
        let _ = writeln!(out, "@@ -{},{old_len} +{},{new_len} @@", old_start + usize::from(old_len > 0), new_start + usize::from(new_len > 0));
        for (kind, line) in &ops[lo..hi] {
            let _ = writeln!(out, "{kind}{line}");
        }
    }
    out
}

/// The lines of `a` and `b` as a diff's operations: ` ` kept, `-` only in `a`, `+` only in `b`,
/// by the longest common subsequence of the lines between the common head and tail.
fn line_ops<'t>(a: &[&'t str], b: &[&'t str]) -> Vec<(char, &'t str)> {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (x, y) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let mut ops: Vec<(char, &str)> = a[..head].iter().map(|l| (' ', *l)).collect();
    let (n, m) = (x.len(), y.len());
    if n.saturating_mul(m) > DIFF_CELLS {
        ops.extend(x.iter().map(|l| ('-', *l)));
        ops.extend(y.iter().map(|l| ('+', *l)));
    } else {
        let at = |i: usize, j: usize| i * (m + 1) + j;
        let mut common = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                common[at(i, j)] = if x[i] == y[j] { common[at(i + 1, j + 1)] + 1 } else { common[at(i + 1, j)].max(common[at(i, j + 1)]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && x[i] == y[j] {
                ops.push((' ', x[i]));
                i += 1;
                j += 1;
            } else if i < n && (j == m || common[at(i + 1, j)] >= common[at(i, j + 1)]) {
                ops.push(('-', x[i]));
                i += 1;
            } else {
                ops.push(('+', y[j]));
                j += 1;
            }
        }
    }
    ops.extend(a[a.len() - tail..].iter().map(|l| (' ', *l)));
    ops
}

/// The module definition file of a DLL's port: every export of the image under its name and
/// ordinal, a function as the lifted function at its address, a forwarder as it was, and a data
/// export as the global that starts at its address. An export at no symbol of the port (a
/// function the analysis did not find, data inside a section's global) is left out with a warning.
fn exports_def(image: &Image, lifted: &Lifted, warnings: &mut Vec<String>) -> String {
    let mut out = String::from("; The exports of the port, as the image had them. Generated by ras emit.\nEXPORTS\n");
    let Some(table) = &image.exports else { return out };
    for export in &table.exports {
        let name = export.name.clone().unwrap_or_else(|| format!("ordinal_{}", export.ordinal));
        let noname = if export.name.is_none() { " NONAME" } else { "" };
        if let Some(target) = &export.forwarder {
            let _ = writeln!(out, "    {name} = {target} @{}{noname}", export.ordinal);
            continue;
        }
        let Some(va) = export.va else { continue };
        if let Some(function) = lifted.functions.get(&va) {
            let internal = if *function == name { String::new() } else { format!(" = {function}") };
            let _ = writeln!(out, "    {name}{internal} @{}{noname}", export.ordinal);
        } else if let Some(global) = lifted.module.globals.iter().find(|g| g.address == Some(va)) {
            let _ = writeln!(out, "    {name} = {} @{}{noname} DATA", global.name, export.ordinal);
        } else {
            let place = lifted
                .data
                .iter()
                .find(|s| va >= s.start && va < s.end)
                .map_or_else(String::new, |s| format!(" ({} bytes into {})", va - s.start, s.global));
            warnings
                .push(format!("export {name} (#{}) at 0x{va:x}{place} is no function or global of the port: not exported", export.ordinal));
        }
    }
    out
}

/// The image's resources as files: `resources.res`, which the port's link folds into its own
/// resource directory, and under `resources/` the string table decoded (`strings.txt`, one
/// `id<tab>text` line each) and every other resource as its bytes. Empty without resources.
fn resource_files(image: &Image) -> Vec<(String, Vec<u8>)> {
    if image.resources.is_empty() {
        return Vec::new();
    }
    let mut files = vec![("resources.res".to_owned(), ra_asm_pe::resources::res_file(image))];
    let strings = ra_asm_pe::resources::string_table(image);
    if !strings.is_empty() {
        let mut text = String::new();
        for (id, s) in &strings {
            text.push_str(&id.to_string());
            text.push('\t');
            text.push_str(&s.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n"));
            text.push('\n');
        }
        files.push(("resources/strings.txt".to_owned(), text.into_bytes()));
    }
    for r in &image.resources {
        if r.kind == ra_asm_pe::ResourceId::Id(6) {
            continue; // the string table, decoded above
        }
        if let Ok(bytes) = r.data(image) {
            files.push((format!("resources/{}", r.file_name(&image.resources)), bytes.to_vec()));
        }
    }
    files
}

/// Writes the translation, the runtime and `functions.json` into `out`, or the translation to
/// stdout without `out`. `hand` names the functions left to hand-written sources; the annotations
/// of the project at `project` rename functions and leave more to the sources.
pub(crate) fn emit(
    image: &Image,
    language: &str,
    out: Option<&Path>,
    quiet: bool,
    x87: ra_asm_lift::X87Unit,
    hand: &[String],
    project: Option<&Path>,
) -> Result<()> {
    let mut analysis = ra_asm_analysis::analyze(image);
    let (annotations, warnings) = project.map(|root| crate::annotations::annotate(root, &mut analysis)).unwrap_or_default();
    let hand = crate::annotations::hand_list(hand, &annotations);
    let hand = hand.as_slice();
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let package = image.path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "port".to_owned(), |s| s.to_string_lossy().into_owned());
    let options = Options { lang: language, package: &package, trace: false, hand, annotations: &annotations };
    let translation = translate(image, &analysis, &lifted, &signatures, &options)?;
    match out {
        None => print!("{}", translation.text()),
        Some(dir) => {
            translation.write(dir)?;
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures)))?;
            std::fs::write(dir.join("functions.json"), translation.table())?;
            if !quiet {
                eprintln!(
                    "{} functions, {} bytes of {language} in {} files -> {}",
                    translation.functions.len(),
                    translation.bytes(),
                    translation.files.len(),
                    dir.display()
                );
            }
        }
    }
    if !quiet {
        for w in analysis.warnings.iter().chain(&warnings).chain(&lifted.warnings).chain(&translation.warnings) {
            eprintln!("warning: {w}");
        }
    }
    Ok(())
}
