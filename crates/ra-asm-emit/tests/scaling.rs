//! The scaling guard of the whole translation, the C emit with its structuring after the analysis,
//! the lift and the raise: a function four times as long takes no more than six times as long.
//! The program and the guard are `ra-asm-lift`'s, whose `scaling` test guards the stages before
//! the emit.

#[path = "../../ra-asm-lift/tests/scaling/program.rs"]
mod program;

use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

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

/// Each emit pass's time over the run of [`emit_pass_times`], summed over its reports.
static PASS_TIMES: Mutex<BTreeMap<&'static str, Duration>> = Mutex::new(BTreeMap::new());

fn record_pass(pass: &'static str, time: Duration) {
    *PASS_TIMES.lock().unwrap_or_else(PoisonError::into_inner).entry(pass).or_default() += time;
}

/// The time of each emit pass over the larger function of the guard, one `pass <name> <ms>` line
/// each: which pass grows past linear, without the guard's two sizes and CI. Run by hand with
/// `--ignored --nocapture`.
#[test]
#[ignore = "prints timings for a person to read"]
fn emit_pass_times() {
    std::thread::Builder::new()
        .name("pass times".into())
        .stack_size(256 << 20)
        .spawn(|| {
            let image = program::image(4 * program::UNITS);
            let analysis = ra_asm_analysis::analyze(&image);
            let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
            let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
            let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
            let _signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
            ra_asm_emit::time_passes(record_pass);
            let options = ra_asm_emit::COptions { dialect: ra_asm_emit::Dialect::C, ..Default::default() };
            black_box(ra_asm_emit::emit_unit(&lifted.module, &options).text().len());
            let times = PASS_TIMES.lock().unwrap_or_else(PoisonError::into_inner);
            for pass in ["structure", "coalesce", "tidy", "print"] {
                let time = times.get(pass).copied().unwrap_or_default();
                println!("pass {pass} {:.1}", time.as_secs_f64() * 1000.0);
            }
            assert!(times.contains_key("print"), "the emit reported no pass: the timing hook is not reached");
        })
        .expect("the timer's thread starts")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}
