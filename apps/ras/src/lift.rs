//! `ras lift`: the IR of an image, with what the driver needs to run it.

use std::path::Path;

use anyhow::{Context, Result};
use ra_asm_pe::Image;

/// Writes `program.ll`, `image.bin` and `image.h` into `out`, or the IR to stdout without `out`.
pub(crate) fn lift(image: &Image, out: Option<&Path>, quiet: bool) -> Result<()> {
    let analysis = ra_asm_analysis::analyze(image);
    let lifted = ra_asm_lift::lift(image, &analysis);
    let text = lifted.module.to_ll();
    match out {
        None => {
            print!("{text}");
        }
        Some(dir) => {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
            std::fs::write(dir.join("program.ll"), &text)?;
            std::fs::write(dir.join("image.bin"), image.mapped())?;
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted))?;
            if !quiet {
                eprintln!(
                    "{} functions, {} imports, {} bytes of IR -> {}",
                    lifted.functions.len(),
                    lifted.imports.len(),
                    text.len(),
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
