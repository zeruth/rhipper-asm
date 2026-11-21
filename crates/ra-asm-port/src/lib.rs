//! The run of a console program's port. `ras port` writes a program's lift as a module for a
//! compiler and a C file that ties what the compiler makes of it to this library
//! ([`ra_asm_lift::port`]); the port's `main` hands its tables to [`ra_port_main`], which runs
//! the program as `ras run` runs it, on the same machine of its console, with the compiled
//! functions in place of their IR.
//!
//! The run lifts the image again as it starts: the lift is what names the functions, says which
//! of them are code the program copies into RAM, and holds the IR of a function the port has no
//! compiled code for.

use std::ffi::{CStr, c_char, c_int};
use std::path::PathBuf;

use ra_asm_ir::interp::native::{self, Code};
use ra_asm_ir::interp::{Stop, Value};
use ra_asm_ir::types::FnType;
use ra_asm_lift::run::{Capture, Console, Native, Options};
use ra_asm_lift::{LiftOptions, RaiseLevel, Whole, X87Unit};

/// A compiled function with the address the program has it at; the table ends at one of no
/// code.
#[repr(C)]
pub struct Function {
    pub address: u64,
    pub code: Option<Code>,
}

/// A global of the module the compiled program has a copy of its own of; the table ends at
/// one of no name.
#[repr(C)]
pub struct Global {
    pub name: *const c_char,
    pub at: *mut u8,
}

/// The run's entries, which the port's code calls: [`ra_port_main`] fills them in.
#[repr(C)]
pub struct Host {
    pub load: Option<unsafe extern "C-unwind" fn(u64, u64) -> u64>,
    pub store: Option<unsafe extern "C-unwind" fn(u64, u64, u64)>,
    pub tick: Option<unsafe extern "C-unwind" fn(*mut u8, u64, u64)>,
    pub call: Option<unsafe extern "C-unwind" fn(u64, u64, u64, u64, u64, u64, u64) -> u64>,
    /// The instructions the program ran since the run last counted them, and how many of them
    /// the run lets go by before it looks at its machine again.
    pub steps: u64,
    pub budget: u64,
    /// The run has something for the program where its next block starts.
    pub boundary: u64,
    /// The loads and the stores the program made in its RAM itself since the run last counted.
    pub loads: u64,
    pub stores: u64,
    /// The console's RAM as the program reads and writes it itself ([`Ram`]).
    pub ram: Ram,
    /// The ranges of the RAM a store goes through the run for, each an offset and a length.
    pub watched: [[u64; 2]; 4],
}

/// The console's RAM for the port's own accesses: where it is on this machine, the segments of
/// the address space that reach it (a bit each, by an address's top three bits), what of an
/// address is its place there, the places the RAM answers at, and what of a place is the byte's
/// offset. No RAM while the limit is zero.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Ram {
    pub at: u64,
    pub segments: u64,
    pub fold: u64,
    pub limit: u64,
    pub wrap: u64,
}

/// What the program counted itself goes to the run, which is then due no look until it says so:
/// a call back that stops the run leaves it so, and the next block asks.
fn arrive(vm: &mut ra_asm_ir::interp::Vm<'_>) {
    let host = HOST.load(std::sync::atomic::Ordering::Relaxed);
    if host.is_null() {
        return;
    }
    // SAFETY: the port's own record, which `ra_port_main` set before the program ran and only
    // the run's thread touches while the program runs.
    let host = unsafe { &mut *host };
    vm.account(std::mem::take(&mut host.steps));
    vm.memory.count(std::mem::take(&mut host.loads), std::mem::take(&mut host.stores));
    host.budget = 0;
    ra_asm_lift::run::phase(1);
}

/// The run says how far the program may go on by itself.
fn leave(vm: &ra_asm_ir::interp::Vm<'_>) {
    let host = HOST.load(std::sync::atomic::Ordering::Relaxed);
    if host.is_null() {
        return;
    }
    // SAFETY: as in `arrive`.
    let host = unsafe { &mut *host };
    host.budget = vm.until_poll();
    host.boundary = u64::from(vm.boundary);
    ra_asm_lift::run::phase(0);
}

/// What a port hands the run.
#[repr(C)]
pub struct Port {
    pub image: *const u8,
    pub image_size: u64,
    pub functions: *const Function,
    /// The names of the runtime's functions the program calls, by the numbers it calls them
    /// under; the table ends at a null.
    pub imports: *const *const c_char,
    pub globals: *const Global,
    pub host: *mut Host,
    /// The programs the console loads that the port has compiled functions of; the table ends
    /// at one of no image.
    pub loaded: *const LoadedProgram,
}

/// A program the console loads into RAM, as the port was made with it: its file's bytes and
/// the entries of its functions a run found.
#[repr(C)]
pub struct LoadedProgram {
    pub image: *const u8,
    pub image_size: u64,
    pub entries: *const u64,
    pub entry_count: u64,
    /// Those of the entries that are where a context the program switched away from goes on.
    pub continuations: *const u64,
    pub continuation_count: u64,
}

/// The runtime's functions the port calls, by number: each one's name and type, from the lift.
/// A process is one port.
static IMPORTS: std::sync::OnceLock<Vec<(String, FnType)>> = std::sync::OnceLock::new();
/// The port's record of the run's entries.
static HOST: std::sync::atomic::AtomicPtr<Host> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C-unwind" fn load(address: u64, bytes: u64) -> u64 {
    native::with(|vm, _| {
        arrive(vm);
        ra_asm_lift::run::phase(5);
        ra_asm_lift::run::phase_detail((address >> 12) as u32);
        let value = match bytes {
            1 => vm.memory.read_u8(address).map(u64::from),
            2 => vm.memory.read_u16(address).map(u64::from),
            4 => vm.memory.read_u32(address).map(u64::from),
            _ => vm.memory.read_u64(address),
        }?;
        leave(vm);
        Ok(value)
    })
}

unsafe extern "C-unwind" fn store(address: u64, bytes: u64, value: u64) {
    native::with(|vm, host| {
        arrive(vm);
        ra_asm_lift::run::phase(6);
        ra_asm_lift::run::phase_detail((address >> 12) as u32);
        match bytes {
            1 => vm.memory.write_u8(address, value as u8),
            2 => vm.memory.write_u16(address, value as u16),
            4 => vm.memory.write_u32(address, value as u32),
            _ => vm.memory.write_u64(address, value),
        }?;
        vm.stored(host)?;
        leave(vm);
        Ok(())
    });
}

unsafe extern "C-unwind" fn tick(state: *mut u8, address: u64, instructions: u64) {
    native::with(|vm, host| {
        arrive(vm);
        vm.tick(host, state as u64, address, instructions)?;
        peek(vm);
        leave(vm);
        Ok(())
    });
}

/// With `RA_PEEK=<address>,<address>...` (hexadecimal), prints the words of the program's
/// memory there every twenty million instructions: what a program that waits is waiting on.
fn peek(vm: &mut ra_asm_ir::interp::Vm<'_>) {
    // With `RA_DUMP=<path>@<instructions>`, the console's RAM is written there once that many
    // instructions have run.
    static DUMP: std::sync::OnceLock<Option<(String, u64)>> = std::sync::OnceLock::new();
    static DUMPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let dump = DUMP.get_or_init(|| {
        let text = std::env::var("RA_DUMP").ok()?;
        let (path, at) = text.rsplit_once('@')?;
        Some((path.to_owned(), at.parse().ok()?))
    });
    if let Some((path, at)) = dump
        && vm.instructions >= *at
        && !DUMPED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        match vm.memory.read_vec(0x8000_0000, 0x20_0000) {
            Ok(ram) => {
                let _ = std::fs::write(path, ram);
                eprintln!("ra: the RAM at {} instructions is in {path}", vm.instructions);
            }
            Err(stop) => eprintln!("ra: no RAM to write: {stop:?}"),
        }
    }
    static ADDRESSES: std::sync::OnceLock<Vec<u64>> = std::sync::OnceLock::new();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let addresses = ADDRESSES.get_or_init(|| {
        std::env::var("RA_PEEK")
            .map(|list| list.split(',').filter_map(|a| u64::from_str_radix(a.trim(), 16).ok()).collect())
            .unwrap_or_default()
    });
    if addresses.is_empty() || vm.instructions < NEXT.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    NEXT.store(vm.instructions + 20_000_000, std::sync::atomic::Ordering::Relaxed);
    let words: Vec<String> = addresses.iter().map(|a| format!("{a:x}={:x}", vm.memory.read_u32(*a).unwrap_or(0xdead_dead))).collect();
    eprintln!("peek @{}: {}", vm.instructions, words.join(" "));
}

unsafe extern "C-unwind" fn call(import: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    let Some((name, ty)) = IMPORTS.get().and_then(|imports| imports.get(import as usize)) else {
        return native::with(|_, _| {
            Err::<u64, _>(Stop::fault(format!("the port calls the runtime's function {import}, which it does not name")))
        });
    };
    // A command of the GTE is arithmetic on the registers the program has itself: it runs on
    // them where they are, and nothing of the run's is touched.
    if name.starts_with("ra_gte_") {
        // SAFETY: the first argument is the program's own global of the sixty-four registers,
        // which only the run's thread touches.
        let registers = unsafe { &mut *(a0 as *mut [i32; 64]) };
        ra_asm_lift::run::gte_command(registers, a1 as u32);
        return 0;
    }
    let args: Vec<Value> = [a0, a1, a2, a3, a4, a5].into_iter().take(ty.params.len()).map(|a| Value::Int(u128::from(a))).collect();
    native::with(|vm, host| {
        arrive(vm);
        ra_asm_lift::run::phase(7);
        ra_asm_lift::run::phase_detail(import as u32);
        let value = vm.host_call(host, name, ty, &args)?;
        leave(vm);
        Ok(if ty.ret == ra_asm_ir::Type::Void { 0 } else { value.as_u64() })
    })
}

/// How the run meets this machine, by the port's arguments.
struct Arguments {
    frames: Option<u64>,
    frame_dir: Option<PathBuf>,
    disc: Option<PathBuf>,
    files: Option<PathBuf>,
    headless: bool,
    stats: bool,
    /// The buttons a headless run holds from a frame on: `<frame>:<buttons>`.
    pads: Vec<String>,
    /// Where a headless run writes its sound.
    wav: Option<PathBuf>,
    /// Where the run writes the entries of the functions it called that it had no lift of.
    entries: Option<PathBuf>,
}

const USAGE: &str = "\
The port of a console's program.

  --disc <PATH>       The disc in the drive: an ISO image, or the directory of its files
  --files <DIR>       The directory of the console's other devices' files (the memory cards')
  --headless          Show no window
  --frames <N>        Stop after this many frames (headless)
  --frame-dir <DIR>   Write each frame there as a PNG (headless)
  --pad <F>:<BUTTONS> Hold these buttons from frame F on (headless): `300:start`, `360:none`
  --wav <FILE>        Write the run's sound there (headless)
  --entries <FILE>    Write the entries of the functions the run called and had no lift of
  --stats             Say how many instructions ran and how fast as the run ends";

fn arguments(args: &[String]) -> anyhow::Result<Arguments> {
    let mut out = Arguments {
        frames: None,
        frame_dir: None,
        disc: None,
        files: None,
        headless: false,
        stats: false,
        pads: Vec::new(),
        wav: None,
        entries: None,
    };
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| anyhow::anyhow!("{arg} takes a value\n\n{USAGE}"));
        match arg.as_str() {
            "--frames" => out.frames = Some(value()?.parse()?),
            "--frame-dir" => out.frame_dir = Some(value()?.into()),
            "--disc" => out.disc = Some(value()?.into()),
            "--files" => out.files = Some(value()?.into()),
            "--pad" => out.pads.push(value()?.clone()),
            "--wav" => out.wav = Some(value()?.into()),
            "--entries" => out.entries = Some(value()?.into()),
            "--headless" => out.headless = true,
            "--stats" => out.stats = true,
            _ => anyhow::bail!("{arg}: no such option\n\n{USAGE}"),
        }
    }
    out.headless |= out.frames.is_some() || out.frame_dir.is_some();
    Ok(out)
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
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend(samples.iter().flatten().flat_map(|s| s.to_le_bytes()));
    out
}

/// Runs the port's program to its end and gives its exit code.
fn run(port: &Port, args: &[String]) -> anyhow::Result<i32> {
    let arguments = arguments(args)?;
    // SAFETY: the port's tables, as its `main` wrote them: the image's bytes, the functions to
    // an entry of no code, the globals and the imports to a null name.
    let (image, functions, globals, imports) = unsafe {
        let image = std::slice::from_raw_parts(port.image, port.image_size as usize).to_vec();
        let mut functions = Vec::new();
        let mut at = port.functions;
        while let Some(code) = (*at).code {
            functions.push(((*at).address, code));
            at = at.add(1);
        }
        let mut globals = Vec::new();
        let mut at = port.globals;
        while !(*at).name.is_null() {
            globals.push((CStr::from_ptr((*at).name).to_string_lossy().into_owned(), (*at).at as u64));
            at = at.add(1);
        }
        let mut imports = Vec::new();
        let mut at = port.imports;
        while !(*at).is_null() {
            imports.push(CStr::from_ptr(*at).to_string_lossy().into_owned());
            at = at.add(1);
        }
        (image, functions, globals, imports)
    };
    // With `RA_NATIVE_SKIP=<from>-<to>` (hexadecimal), the functions in the range run as their
    // IR and not as the port's code: which of the two a fault is in.
    let functions: Vec<(u64, Code)> = match std::env::var("RA_NATIVE_SKIP").ok().and_then(|range| {
        let (from, to) = range.split_once('-')?;
        Some((u64::from_str_radix(from, 16).ok()?, u64::from_str_radix(to, 16).ok()?))
    }) {
        Some((from, to)) => functions.into_iter().filter(|(address, _)| !(from..to).contains(address)).collect(),
        None => functions,
    };
    let mut image = ra_asm_pe::Image::parse(image)?;
    image.path = std::env::current_exe().ok();
    // The programs the console loads that the port has compiled functions of, each with the
    // entries the port was made with: the table ends at one of no image.
    let mut loaded = Vec::new();
    // SAFETY: the port's table, as its `main` wrote it.
    unsafe {
        let mut at = port.loaded;
        while !at.is_null() && !(*at).image.is_null() {
            let bytes = std::slice::from_raw_parts((*at).image, (*at).image_size as usize).to_vec();
            let entries = std::slice::from_raw_parts((*at).entries, (*at).entry_count as usize);
            let continuations = std::slice::from_raw_parts((*at).continuations, (*at).continuation_count as usize);
            loaded.push(ra_asm_lift::Loaded::new(bytes, &format!("loaded-{}.exe", loaded.len()), entries, continuations)?);
            at = at.add(1);
        }
    }
    let whole = Whole::lift_with(&image, &loaded, LiftOptions { standalone: false, x87: X87Unit::Double }, RaiseLevel::Machine)?;
    let declared = |name: &str| whole.lifted.module.declarations.iter().find(|d| d.name == name).map(|d| d.ty.clone());
    let imports: Vec<(String, FnType)> = imports
        .into_iter()
        .map(|name| {
            // A function the run's lift does not declare takes no argument the run reads.
            let ty = declared(&name).unwrap_or_else(|| FnType::new(ra_asm_ir::Type::Void, Vec::new()));
            (name, ty)
        })
        .collect();
    let mut capture = Capture::new().limit(arguments.frames.unwrap_or(0));
    capture.lock().dump = arguments.frame_dir;
    for entry in &arguments.pads {
        let (frame, buttons) = entry.split_once(':').ok_or_else(|| anyhow::anyhow!("--pad {entry}: not `<frame>:<buttons>`"))?;
        let state =
            ra_asm_lift::run::pad(buttons).ok_or_else(|| anyhow::anyhow!("--pad {entry}: `{buttons}` names a button the pad has not"))?;
        capture = capture.press(frame.parse()?, 0, state);
    }
    let headless = arguments.headless || !arguments.pads.is_empty() || arguments.wav.is_some();
    let kept = capture.clone();
    let console = Console {
        capture: headless.then_some(capture),
        disc: arguments.disc,
        files: arguments.files,
        native: Some(Native { functions, globals }),
    };
    let options = Options { limit: 0, live: true, console: Some(console), ..Options::default() };
    // A PlayStation's 2 MiB of RAM are this machine's memory at their `kseg0` address, seen
    // through `kuseg`, `kseg0` and `kseg1` and four times over in each; the code the ROM
    // copied there is the run's to watch.
    if image.platform() == Some("psx") {
        // SAFETY: the port's record of the run's entries, which nothing reads before the run.
        let host = unsafe { &mut *port.host };
        host.ram = Ram { at: 0x8000_0000, segments: 0b0011_0001, fold: 0x1fff_ffff, limit: 0x80_0000, wrap: 0x1f_ffff };
        for (watched, part) in host.watched.iter_mut().zip(&whole.lifted.resident) {
            let (start, len) = part.code();
            *watched = [start & 0x1f_ffff, len];
        }
        if whole.lifted.resident.len() > host.watched.len() {
            host.ram.limit = 0;
        }
    }
    HOST.store(port.host, std::sync::atomic::Ordering::Relaxed);
    let _ = IMPORTS.set(imports);
    if arguments.entries.is_some() {
        // SAFETY: the run's thread is not started yet: no other thread reads the environment.
        unsafe { std::env::set_var("RA_RUN_STATS", "1") };
    }
    ra_asm_lift::run::phase_sampler();
    let started = std::time::Instant::now();
    let outcome = ra_asm_lift::run::run(&image, &whole.lifted, &whole.signatures, &options)?;
    if arguments.stats {
        let seconds = started.elapsed().as_secs_f64();
        eprintln!("ra: {} instructions in {seconds:.2} s, {:.1} million a second", outcome.steps, outcome.steps as f64 / seconds / 1e6);
        // With `RA_RUN_STATS` set, what the run's thread was doing, a sample a millisecond.
        let samples = ra_asm_lift::run::phase_samples();
        let total: u64 = samples.iter().map(|(_, n)| n).sum();
        if total > 0 {
            let parts: Vec<String> = samples.iter().map(|(name, n)| format!("{name} {:.0}%", 100.0 * *n as f64 / total as f64)).collect();
            eprintln!("ra: the run's thread: {}", parts.join(", "));
            for (phase, of, n) in ra_asm_lift::run::phase_details().into_iter().take(14) {
                let what = match phase {
                    7 => IMPORTS.get().and_then(|i| i.get(of as usize)).map_or_else(|| of.to_string(), |(name, _)| name.clone()),
                    5 => format!("loads at {:#x}", u64::from(of) << 12),
                    _ => format!("stores at {:#x}", u64::from(of) << 12),
                };
                eprintln!("ra:   {what} {:.1}%", 100.0 * n as f64 / total as f64);
            }
        }
        // With `RA_RUN_STATS` set, where the time went among the console's devices.
        let [draw, sound, show] = ra_asm_lift::run::device_seconds();
        if draw + sound + show > 0.0 {
            eprintln!(
                "ra: of {seconds:.2} s: {draw:.2} drawing, {sound:.2} mixing sound, {show:.2} showing frames, {:.2} the rest (the program and the run)",
                seconds - draw - sound - show
            );
        }
        // With `RA_RUN_STATS` set, what of them no compiled function ran.
        let (processor, entries) = ra_asm_lift::run::processor_stats::report(24);
        if processor != 0 {
            eprintln!("ra: {processor} instructions were the processor's, from code no lifted function has; the most from:");
            for (entry, count) in entries {
                eprintln!("ra:   {entry:#010x} {count}");
            }
            eprintln!("ra: where it stood the most at the end:");
            for (pc, count) in ra_asm_lift::run::processor_stats::samples(40) {
                eprintln!("ra:   {pc:#010x} {count}");
            }
            eprintln!("ra: its calls and jumps came the most to:");
            for (entry, count) in ra_asm_lift::run::processor_stats::entered(400) {
                eprintln!("ra:   {entry:#010x} {count}");
            }
        }
    }
    if let Some(stop) = &outcome.stop {
        eprintln!("ra: {stop}");
    }
    // With `--entries <file>`, every address a call came to that no lifted function was, with
    // how often: the entries of the functions a program loaded from the disc has, which
    // `ras emit --entries` takes as functions.
    if let Some(path) = &arguments.entries {
        // A line an entry and how often it was called; `jump` after it where a jump through a
        // register came to it in the middle of a function.
        let mut lines: Vec<String> =
            ra_asm_lift::run::processor_stats::called().into_iter().map(|(entry, count)| format!("{entry:08x} {count}\n")).collect();
        lines
            .extend(ra_asm_lift::run::processor_stats::continued().into_iter().map(|(entry, count)| format!("{entry:08x} {count} jump\n")));
        std::fs::write(path, lines.concat())?;
    }
    if let Some(path) = &arguments.wav {
        let kept = kept.lock();
        std::fs::write(path, wav(&kept.samples, kept.rate))?;
    }
    Ok(outcome.exit)
}

/// The entry of a port: runs the program `port` holds, with the arguments of its `main`, and
/// gives the program's exit code.
///
/// # Safety
///
/// `port` is the record a port's `main` builds ([`ra_asm_lift::port::source`]), whose tables end
/// as the fields say, and `argv` holds `argc` strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ra_port_main(port: *const Port, argc: c_int, argv: *const *const c_char) -> c_int {
    // SAFETY: by the function's own contract.
    let (port, args) = unsafe {
        let args = (0..argc.max(0) as usize).map(|n| CStr::from_ptr(*argv.add(n)).to_string_lossy().into_owned()).collect::<Vec<_>>();
        (&*port, args)
    };
    // SAFETY: the port's record of the run's entries, which nothing reads before the run.
    unsafe {
        let host = &mut *port.host;
        host.load = Some(load);
        host.store = Some(store);
        host.tick = Some(tick);
        host.call = Some(call);
    }
    match run(port, &args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ra: {error:#}");
            2
        }
    }
}
