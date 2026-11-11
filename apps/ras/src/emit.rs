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
    let text = unit.text();
    match out {
        None => print!("{text}"),
        Some(dir) => {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
            std::fs::write(dir.join("program.c"), &text)?;
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures)))?;
            std::fs::write(dir.join("functions.json"), ra_asm_emit::table(&unit, "program.c", &infos))?;
            if !quiet {
                eprintln!("{} functions, {} bytes of C -> {}", unit.functions.len(), text.len(), dir.display());
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
