//! `ras lift`: the IR of an image, with what the driver needs to run it.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use ra_asm_analysis::Analysis;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{Lifted, RaiseLevel};
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
    /// The x87 unit the lift models.
    pub x87: ra_asm_lift::X87Unit,
}

/// Writes `program.ll`, `image.bin` and `image.h` into `out`, or the IR to stdout without `out`.
/// With `report`, stdout gets the coverage report instead of the IR (JSON with `json`).
pub(crate) fn lift(image: &Image, out: Option<&Path>, options: &Options) -> Result<()> {
    let Options { level, standalone, quiet, report, json, x87 } = *options;
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone, x87 });
    let coverage = report.then(|| ra_asm_lift::coverage(&analysis, &lifted));
    let signatures = raise(&mut lifted, &analysis, level);
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

/// Raises the module of `lifted` to `level` and gives its functions their names: the step between
/// the lift and the IR or the source. The signatures are by the final names.
pub(crate) fn raise(lifted: &mut Lifted, analysis: &Analysis<'_>, level: RaiseLevel) -> HashMap<String, Signature> {
    let options = lifted.raise_options(level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(analysis, Some(&mut signatures));
    signatures
}
