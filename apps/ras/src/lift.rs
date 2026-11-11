//! `ras lift`: the IR of an image, with what the driver needs to run it.

use std::path::Path;

use anyhow::{Context, Result};
use ra_asm_lift::RaiseLevel;
use ra_asm_pe::Image;

/// What `ras lift` was asked for.
pub(crate) struct Options {
    /// How far to raise.
    pub level: RaiseLevel,
    /// Lift the data sections into globals, as the oracle and the emit do, so the IR is the one a
    /// port was built from.
    pub standalone: bool,
    /// Print no summary and no warnings.
    pub quiet: bool,
    /// Print the coverage report instead of the IR.
    pub report: bool,
    /// The report as JSON.
    pub json: bool,
}

/// Writes `program.ll`, `image.bin` and `image.h` into `out`, or the IR to stdout without `out`.
/// With `report`, stdout gets the coverage report instead of the IR (JSON with `json`).
pub(crate) fn lift(image: &Image, out: Option<&Path>, options: &Options) -> Result<()> {
    let Options { level, standalone, quiet, report, json } = *options;
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone });
    let coverage = report.then(|| ra_asm_lift::coverage(&analysis, &lifted));
    let options = lifted.raise_options(level);
    let signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let text = lifted.module.to_ll();
    match out {
        None if report => {}
        None => {
            print!("{text}");
        }
        Some(dir) => {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
            std::fs::write(dir.join("program.ll"), &text)?;
            std::fs::write(dir.join("image.bin"), image.mapped())?;
            let abi = (level == RaiseLevel::Abi).then_some(&signatures);
            std::fs::write(dir.join("image.h"), ra_asm_lift::image_header(image, &analysis, &lifted, abi))?;
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
    if let Some(coverage) = &coverage {
        crate::report::coverage(coverage, json)?;
    }
    Ok(())
}
