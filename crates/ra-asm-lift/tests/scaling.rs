//! The scaling guard of the analysis, the lift and the raise: a function four times as long takes
//! no more than six times as long through them. A pass that grows with the square of a function's
//! size passes every fixture, which are small, and fails here; on a large program one took the
//! emit from 5 minutes to over 40. The emit's own guard is `ra-asm-emit`'s `scaling` test.

#[path = "scaling/program.rs"]
mod program;

use std::time::Instant;

use program::Stage;
use ra_asm_pe::Image;

/// The stages of a raise to the ABI level, standalone as a port is.
fn stages(image: &Image, units: usize) -> Vec<Stage> {
    let start = Instant::now();
    let analysis = ra_asm_analysis::analyze(image);
    let analyzed = start.elapsed();
    let start = Instant::now();
    let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, options);
    let lift = start.elapsed();
    let start = Instant::now();
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let _signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    let raised = start.elapsed();
    // the diamonds kept their blocks, so the dominator tree is as deep as the guard means
    let blocks = lifted.module.functions.iter().map(|f| f.blocks.len()).max().unwrap_or(0);
    assert!(blocks >= 2 * units, "the function of {units} diamonds was raised to {blocks} blocks");
    vec![("analysis", analyzed), ("lift", lift), ("raise", raised)]
}

#[test]
fn a_function_four_times_as_long_takes_at_most_six_times_as_long() {
    program::guard(stages);
}
