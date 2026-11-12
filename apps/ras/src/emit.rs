//! `ras emit`: source out of an image, in C, C++ or Rust, with the function table beside it.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_emit::{EmittedFile, EmittedFunction, FunctionInfo};
use ra_asm_lift::Lifted;
use ra_asm_pe::Image;

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
            ra_asm_emit::write_if_changed(&dir.join(path), bytes)?;
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
    let Options { lang, package, trace, hand } = *options;
    let infos = ra_asm_emit::functions::infos(analysis);
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
/// stdout without `out`. `hand` names the functions left to hand-written sources.
pub(crate) fn emit(
    image: &Image,
    language: &str,
    out: Option<&Path>,
    quiet: bool,
    x87: ra_asm_lift::X87Unit,
    hand: &[String],
) -> Result<()> {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    let package = image.path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "port".to_owned(), |s| s.to_string_lossy().into_owned());
    let translation =
        translate(image, &analysis, &lifted, &signatures, &Options { lang: language, package: &package, trace: false, hand })?;
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
        for w in analysis.warnings.iter().chain(&lifted.warnings).chain(&translation.warnings) {
            eprintln!("warning: {w}");
        }
    }
    Ok(())
}
