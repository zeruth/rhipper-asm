//! `ras run`: a program through the IR interpreter, its output written through as it comes. With
//! `--facts`, what the run saw (see [`ra_asm_analysis::facts`]) is written there as JSON, for the
//! analysis of the next pass; with `--record`, the calls and returns of the run with their values
//! are written there in the shape of a port's trace (see [`crate::trace`]).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
    /// Where to write the facts of the run, when asked.
    pub facts: Option<PathBuf>,
    /// Where to write the record of the run (the calls and returns with their values, as a
    /// port's trace), when asked.
    pub record: Option<PathBuf>,
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
    let run = ra_asm_lift::run::Options {
        trace: options.trace,
        limit: options.limit,
        args: options.args.clone(),
        live: true,
        record: options.record.clone(),
        pause: None,
    };
    let outcome = ra_asm_lift::run::run(image, &lifted, &signatures, &run)?;
    if let Some(stop) = &outcome.stop {
        eprintln!("ra: {stop}");
    }
    if let Some(path) = &options.facts {
        write_facts(path, &outcome.facts)?;
    }
    Ok(outcome.exit)
}

/// Writes `facts` as JSON at `path`, making its directory.
pub(crate) fn write_facts(path: &Path, facts: &ra_asm_analysis::Facts) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot make {}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(facts)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}

/// The facts at `path`, when there are any and they are of `image`; `None` with the reason
/// otherwise, which the caller logs.
pub(crate) fn read_facts(path: &Path, image: &Image) -> Result<ra_asm_analysis::Facts, String> {
    let text = std::fs::read_to_string(path).map_err(|_| format!("no facts at {}", path.display()))?;
    let facts: ra_asm_analysis::Facts = serde_json::from_str(&text).map_err(|e| format!("{} is not a facts file: {e}", path.display()))?;
    if !facts.input.is_of(image) {
        return Err(format!("{} is of {} ({} bytes), not of this image: ignored", path.display(), facts.input.exe, facts.input.size));
    }
    Ok(facts)
}
