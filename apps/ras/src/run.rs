//! `ras run`: a program through the IR interpreter, its output written through as it comes.

use anyhow::Result;
use ra_asm_lift::RaiseLevel;
use ra_asm_pe::Image;

pub(crate) struct Options {
    /// How far to raise before running: the machine IR by default.
    pub level: RaiseLevel,
    pub x87: ra_asm_lift::X87Unit,
    /// Name every function entered on stderr.
    pub trace: bool,
    /// Stop after this many IR instructions; zero for no limit.
    pub limit: u64,
    /// The program's arguments.
    pub args: Vec<String>,
    /// Print no warnings of the lift.
    pub quiet: bool,
}

/// Lifts `image`, runs it, and returns the program's exit code; a fault is reported on stderr as
/// the driver reports a trap, and its exit code is the driver's.
pub(crate) fn run(image: &Image, options: &Options) -> Result<i32> {
    let analysis = ra_asm_analysis::analyze(image);
    let standalone = options.level == RaiseLevel::Abi;
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, ra_asm_lift::LiftOptions { standalone, x87: options.x87 });
    let raise = lifted.raise_options(options.level);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, raise);
    lifted.apply_names(&analysis, Some(&mut signatures));
    if !options.quiet {
        for w in analysis.warnings.iter().chain(&lifted.warnings) {
            eprintln!("warning: {w}");
        }
    }
    let run = ra_asm_lift::run::Options { trace: options.trace, limit: options.limit, args: options.args.clone(), live: true };
    let outcome = ra_asm_lift::run::run(image, &lifted, &signatures, &run)?;
    if let Some(stop) = &outcome.stop {
        eprintln!("ra: {stop}");
    }
    Ok(outcome.exit)
}
