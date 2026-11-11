//! `ras emit`: source out of an image.

use std::path::Path;

use anyhow::{Context, Result};
use ra_asm_pe::Image;

/// Writes `program.c` and `image.h` into `out`, or the C to stdout without `out`.
pub(crate) fn emit(image: &Image, language: &str, out: Option<&Path>, quiet: bool) -> Result<()> {
    anyhow::ensure!(language == "c", "the {language} backend is not written yet; `c` is");
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone: true });
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, ra_asm_lift::RaiseOptions::default());
    let names = analysis.functions.values().map(|f| (ra_asm_lift::function_name(f.entry), f.name.clone())).collect();
    let text = ra_asm_emit::emit_c(&lifted.module, &ra_asm_emit::COptions { names });
    match out {
        None => print!("{text}"),
        Some(dir) => {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
            std::fs::write(dir.join("program.c"), &text)?;
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures)))?;
            if !quiet {
                eprintln!("{} functions, {} bytes of C -> {}", lifted.functions.len(), text.len(), dir.display());
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
