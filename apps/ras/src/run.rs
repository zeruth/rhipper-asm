//! `ras run`: a program through the IR interpreter, its output written through as it comes. With
//! `--facts`, what the run saw (see [`ra_asm_analysis::facts`]) is written there as JSON, for the
//! analysis of the next pass; with `--record`, the calls and returns of the run with their values
//! are written there in the shape of a port's trace (see [`crate::trace`]), or, to a path ending in
//! `.session`, the run's session (see [`ra_asm_host::session`]).

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
    /// port's trace, or the session to a `.session` path), when asked.
    pub record: Option<PathBuf>,
    /// How a console program's machine meets this one: its window or its headless capture, its
    /// disc and its files.
    pub console: Option<ra_asm_lift::run::Console>,
    /// Where to write the sound of a console program's headless run, as a WAV file.
    pub wav: Option<PathBuf>,
}

/// How the machine of `image`'s console meets this one, by the options of `ras run`: a window, or
/// headless with the frames to run, where to write them and the buttons to hold; `None` for a
/// program of an operating system. A `--pad` entry is `<frame>:<buttons>` or
/// `<frame>:<port>:<buttons>`.
pub(crate) fn console(
    image: &Image,
    headless: bool,
    frames: Option<u64>,
    frame_dir: Option<PathBuf>,
    pads: &[String],
    disc: Option<PathBuf>,
    files: Option<PathBuf>,
) -> Result<Option<ra_asm_lift::run::Console>> {
    if !ra_asm_lift::run::has_machine(image) {
        return Ok(None);
    }
    let mut capture = ra_asm_lift::run::Capture::new().limit(frames.unwrap_or(0));
    capture.lock().dump = frame_dir;
    for entry in pads {
        let fields: Vec<&str> = entry.split(':').collect();
        let (frame, port, buttons) = match fields[..] {
            [frame, buttons] => (frame, "0", buttons),
            [frame, port, buttons] => (frame, port, buttons),
            _ => anyhow::bail!("--pad {entry}: not `<frame>:<buttons>` or `<frame>:<port>:<buttons>`"),
        };
        let frame: u64 = frame.parse().with_context(|| format!("--pad {entry}: the frame `{frame}` is no number"))?;
        let port: usize = port.parse().with_context(|| format!("--pad {entry}: the port `{port}` is no number"))?;
        let state = ra_asm_lift::run::pad(buttons).with_context(|| format!("--pad {entry}: `{buttons}` names a button the pad has not"))?;
        capture = capture.press(frame, port, state);
    }
    Ok(Some(ra_asm_lift::run::Console { capture: headless.then_some(capture), disc, files, native: None }))
}

/// Lifts `image`, runs it, and returns the program's exit code; a fault is reported on stderr as
/// the driver reports a trap, and its exit code is the driver's.
pub(crate) fn run(image: &Image, options: &Options) -> Result<i32> {
    let standalone = options.level == RaiseLevel::Abi;
    // The programs a console's ROM keeps for RAM lift with it, as second images.
    let whole = ra_asm_lift::Whole::lift(image, ra_asm_lift::LiftOptions { standalone, x87: options.x87 }, options.level)?;
    if !options.quiet {
        for w in &whole.warnings {
            eprintln!("warning: {w}");
        }
        report_images(&whole);
    }
    let ra_asm_lift::Whole { lifted, signatures, .. } = whole;
    let session = options.record.as_ref().filter(|path| path.extension().is_some_and(|e| e == "session"));
    let run = ra_asm_lift::run::Options {
        trace: options.trace,
        limit: options.limit,
        args: options.args.clone(),
        live: true,
        record: if session.is_some() { None } else { options.record.clone() },
        pause: None,
        observer: None,
        residual: None,
        session: session.is_some(),
        unpack: false,
        answers: None,
        console: options.console.clone(),
    };
    let started = std::time::Instant::now();
    let outcome = ra_asm_lift::run::run(image, &lifted, &signatures, &run)?;
    if options.trace || std::env::var_os("RA_RUN_STATS").is_some() {
        let seconds = started.elapsed().as_secs_f64();
        eprintln!("ra: {} IR instructions in {seconds:.2} s, {:.1} million a second", outcome.steps, outcome.steps as f64 / seconds / 1e6);
        let (processor, entries) = ra_asm_lift::run::processor_stats::report(40);
        if processor != 0 {
            eprintln!("ra: {processor} instructions were the processor's, from code no lifted function has; the most from:");
            for (entry, count) in entries {
                eprintln!("ra:   {entry:#010x} {count}");
            }
            eprintln!("ra: its calls and jumps came the most to:");
            for (entry, count) in ra_asm_lift::run::processor_stats::entered(40) {
                eprintln!("ra:   {entry:#010x} {count}");
            }
        }
        for line in ra_asm_ir::interp::block::profile::report() {
            eprintln!("ra: {line}");
        }
    }
    if let Some(stop) = &outcome.stop {
        eprintln!("ra: {stop}");
    }
    if let Some(path) = session {
        let text = ra_asm_host::session::Session::of(&outcome.boundary).write();
        std::fs::write(path, text).with_context(|| format!("cannot write the session at {}", path.display()))?;
    }
    if let Some(path) = &options.facts {
        write_facts(path, &outcome.facts)?;
    }
    if let (Some(path), Some(capture)) = (&options.wav, options.console.as_ref().and_then(|c| c.capture.as_ref())) {
        let kept = capture.lock();
        std::fs::write(path, wav(&kept.samples, kept.rate)).with_context(|| format!("cannot write the sound at {}", path.display()))?;
    }
    Ok(outcome.exit)
}

/// Says, of a ROM lifted with the programs it keeps for RAM, how many functions each gave.
pub(crate) fn report_images(whole: &ra_asm_lift::Whole) {
    let [(_, _, functions), parts @ ..] = whole.images.as_slice() else { return };
    if parts.is_empty() {
        return;
    }
    eprintln!("ra: {functions} functions of the ROM");
    for (base, size, functions) in parts {
        eprintln!("ra: {functions} functions of its program for {base:#x} ({size:#x} bytes)");
    }
}

/// The sound a headless run kept as a WAV file's bytes: sixteen bits, two channels, at `rate`.
fn wav(samples: &[[i16; 2]], rate: u32) -> Vec<u8> {
    let rate = if rate == 0 { 44_100 } else { rate };
    let data = (samples.len() * 4) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, two channels, the rate, the bytes a second, the bytes a frame, the bits a sample.
    for field in [1u16, 2] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    for field in [4u16, 16] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend(samples.iter().flatten().flat_map(|s| s.to_le_bytes()));
    out
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
