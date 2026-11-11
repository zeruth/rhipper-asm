//! `ras emit`: source out of an image, in C, C++ or Rust, with the function table beside it.

use std::collections::HashMap;
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
        for (path, text) in &self.runtime {
            let path = dir.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, text)?;
        }
        Ok(())
    }

    pub(crate) fn table(&self) -> String {
        ra_asm_emit::table(&self.functions, &self.files, &self.infos)
    }
}

/// Translates the raised module of `lifted` to `lang`: `c`, `cpp` or `rust`. `package` names a
/// Rust package and its binary.
pub(crate) fn translate(image: &Image, analysis: &Analysis<'_>, lifted: &Lifted, lang: &str, package: &str) -> Result<Translation> {
    let infos = ra_asm_emit::functions::infos(analysis);
    let names: HashMap<String, String> = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    if lang.eq_ignore_ascii_case("rust") || lang.eq_ignore_ascii_case("rs") {
        let imports = image.imports.iter().filter_map(|i| i.name.clone().map(|n| (n, i.dll.clone()))).collect();
        let entry = image.entry_point().map(ra_asm_lift::function_name);
        let options = ra_asm_emit::RustOptions {
            names,
            imports,
            entry,
            package: package.to_owned(),
            pointer_bits: u32::from(image.pointer_size()) * 8,
            ..Default::default()
        };
        let output = ra_asm_emit::emit_rust(&lifted.module, &options);
        return Ok(Translation { files: output.files, functions: output.functions, infos, runtime: Vec::new() });
    }
    let Some(dialect) = ra_asm_emit::Dialect::parse(lang) else {
        bail!("the language `{lang}` is not `c`, `cpp` or `rust`");
    };
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names, dialect });
    let files = unit.files(ra_asm_emit::FUNCTIONS_PER_FILE);
    Ok(Translation {
        files,
        functions: unit.functions,
        infos,
        runtime: vec![("runtime/rhipper.h", RHIPPER_H), ("runtime/driver.c", DRIVER_C)],
    })
}

/// Writes the translation, the runtime and `functions.json` into `out`, or the translation to
/// stdout without `out`.
pub(crate) fn emit(image: &Image, language: &str, out: Option<&Path>, quiet: bool, x87: ra_asm_lift::X87Unit) -> Result<()> {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true, x87 });
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let package = image.path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "port".to_owned(), |s| s.to_string_lossy().into_owned());
    let translation = translate(image, &analysis, &lifted, language, &package)?;
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
        for w in analysis.warnings.iter().chain(&lifted.warnings) {
            eprintln!("warning: {w}");
        }
    }
    Ok(())
}
