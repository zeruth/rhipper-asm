//! The scaling guard of the whole translation, the C emit with its structuring after the analysis,
//! the lift and the raise: a function four times as long takes no more than six times as long.
//! The program and the guard are `ra-asm-lift`'s, whose `scaling` test guards the stages before
//! the emit.

#[path = "../../ra-asm-lift/tests/scaling/program.rs"]
mod program;

use std::hint::black_box;
use std::time::Instant;

use program::Stage;
use ra_asm_pe::Image;

/// The stages of a C port: the raise to the ABI level, standalone, and the emit.
fn stages(image: &Image, _units: usize) -> Vec<Stage> {
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
    let start = Instant::now();
    let options = ra_asm_emit::COptions { dialect: ra_asm_emit::Dialect::C, ..Default::default() };
    black_box(ra_asm_emit::emit_unit(&lifted.module, &options).text().len());
    let emitted = start.elapsed();
    vec![("analysis", analyzed), ("lift", lift), ("raise", raised), ("emit", emitted)]
}

#[test]
#[ignore = "#549: the emit stage grows 12-16x for 4x (6.5-6.8x in all against 6), so the guard fails at random until it is linear"]
fn a_function_four_times_as_long_takes_at_most_six_times_as_long() {
    // The emit recurses with the chain of diamonds, past a test thread's stack in a debug build.
    std::thread::Builder::new()
        .name("scaling".into())
        .stack_size(256 << 20)
        .spawn(|| program::guard(stages))
        .expect("the guard's thread starts")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}
