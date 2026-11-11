//! `ras emit`: source out of an image.

use std::path::Path;

use anyhow::{Context, Result};
use ra_asm_pe::Image;

/// Writes `program.c`, `image.h` and `functions.json` into `out`, or the C to stdout without `out`.
pub(crate) fn emit(image: &Image, language: &str, out: Option<&Path>, quiet: bool) -> Result<()> {
    anyhow::ensure!(language == "c", "the {language} backend is not written yet; `c` is");
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, ra_asm_lift::RaiseOptions::default());
    let infos = ra_asm_emit::functions::infos(&analysis);
    let names = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let unit = ra_asm_emit::emit_unit(&lifted.module, &ra_asm_emit::COptions { names });
    match out {
        None => print!("{}", unit.text()),
        Some(dir) => {
            let files = unit.files(ra_asm_emit::FUNCTIONS_PER_FILE);
            ra_asm_emit::write_files(dir, &files).with_context(|| format!("cannot write into {}", dir.display()))?;
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures)))?;
            std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit, &files, &infos))?;
            if !quiet {
                let bytes: usize = files.iter().map(|f| f.text.len()).sum();
                eprintln!("{} functions, {} bytes of C in {} files -> {}", unit.functions.len(), bytes, files.len(), dir.display());
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
