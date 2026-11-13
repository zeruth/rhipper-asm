//! Rhipper Asm: the command line over the disassembly core. Each subcommand prints a table for a
//! person, or JSON with `--json` for the workbench and the agents.

mod annotations;
mod cards;
mod diff;
mod disasm;
mod doctor;
mod emit;
mod functions;
mod lift;
mod lockstep;
mod pipeline;
mod plugins;
mod project;
mod report;
mod run;
mod serve;
mod signatures;
mod smoke;
mod trace;
mod xrefs;

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ras", version, about = "Rhipper Asm: disassembly to the Rhipper IR")]
struct Options {
    /// Print JSON instead of a table.
    #[arg(long, global = true)]
    json: bool,
    /// Library signatures applied to the image before anything reads it: a signature file, or a
    /// `.lib`, `.a` or `.obj` the signatures are made from on the spot. A match names the
    /// function and keeps its body out of the port. Repeatable.
    #[arg(long, global = true, value_name = "FILE")]
    signatures: Vec<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

/// Loads `pe` and applies the `--signatures` to it.
fn load(pe: &Path, signatures: &[PathBuf]) -> anyhow::Result<ra_asm_pe::Image> {
    let mut image = ra_asm_pe::load(pe)?;
    if !signatures.is_empty() {
        signatures::apply_files(&mut image, signatures)?;
    }
    Ok(image)
}

#[derive(Subcommand)]
enum Command {
    /// The headers of a PE image: machine, subsystem, base, entry point, and the directories present.
    Info { pe: PathBuf },
    /// The section table.
    Sections { pe: PathBuf },
    /// Every import, with its IAT slot.
    Imports { pe: PathBuf },
    /// Every export.
    Exports { pe: PathBuf },
    /// Every base relocation.
    Relocs { pe: PathBuf },
    /// The debug directory and the PDB record.
    Debug { pe: PathBuf },
    /// The named functions: from the PDB or the link map of a PE, from the symbol table of an ELF.
    Symbols { pe: PathBuf },
    /// The functions of the image. With `--pdata`, the raw exception table entries, or the function
    /// extents of an ELF file's `.eh_frame`.
    Functions {
        pe: PathBuf,
        /// List the exception table entries instead of the functions.
        #[arg(long)]
        pdata: bool,
    },
    /// The blocks of the function at an address, with the edges between them.
    Cfg {
        pe: PathBuf,
        /// `0x140001000`, `140001000` or `F_140001000`.
        address: String,
    },
    /// What each function takes and returns, as the lift infers it. With `--from`, the library
    /// recognition instead: the matches of the signatures of those files (signature files, or
    /// `.lib`/`.a`/`.obj` files to make them from) over the image's functions, and the collisions.
    /// With `--build`, writes the signature store; with `--list`, says what the stores hold and
    /// which files the image's fingerprint picks.
    Signatures {
        /// The image; with `--build` or `--list`, the one whose fingerprint keys the store.
        pe: Option<PathBuf>,
        /// Write the signatures of these libraries into the store, under the toolset of
        /// `--toolset` or of the image's fingerprint, a file per library and machine. Put the
        /// image before the flag: the paths after it are the libraries.
        #[arg(long, num_args = 0.., value_name = "LIB")]
        build: Option<Vec<PathBuf>>,
        /// With `--build`, also the installed MSVC static runtime of every machine, under its
        /// version's toolset key unless `--toolset` gives one.
        #[arg(long, requires = "build")]
        installed: bool,
        /// The toolset key the built files go under: `msvc-14.44`, `mingw`.
        #[arg(long, value_name = "KEY", requires = "build")]
        toolset: Option<String>,
        /// The store to build or list instead of the user's (`RHIPPER_SIGNATURES`, else
        /// `~/.rhipper/signatures`).
        #[arg(long, value_name = "DIR")]
        store: Option<PathBuf>,
        /// The files of the stores, and for the image (or the project's reference) the toolset
        /// key, the files it picks, and the key the installed MSVC runtime would be built under.
        #[arg(long, conflicts_with = "build")]
        list: bool,
        /// With `--list`, the project whose stores (its own and the user's) and reference to list.
        #[arg(long, value_name = "DIR", requires = "list")]
        project: Option<PathBuf>,
        /// A signature file or a library to make signatures from; repeatable.
        #[arg(long, value_name = "FILE")]
        from: Vec<PathBuf>,
        /// Recognize with the C runtime libraries the image's toolset picks (the installed MSVC
        /// runtime for a 14.x one), beside `--from`.
        #[arg(long)]
        runtime: bool,
        /// Write the signatures made from `--from` to this file, in the `ra signatures` text format.
        #[arg(long, value_name = "FILE")]
        write: Option<PathBuf>,
    },
    /// Source out of the image. With `--out`, writes `program.c`, `image.h` and `functions.json` there.
    Emit {
        pe: PathBuf,
        /// The language: `c`; `cpp` for the same source with C++ linkage and `.cpp` files; `rust` for a cargo package.
        #[arg(long, default_value = "c")]
        lang: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
        /// The x87 unit: `double` (eight doubles) or `exact` (the 80-bit unit through the runtime's `f80`).
        #[arg(long, default_value = "double")]
        x87: ra_asm_lift::X87Unit,
        /// A function the hand-written sources of the port own, by name or address: written as its declaration, not its body.
        #[arg(long, value_name = "FUNCTION")]
        hand: Vec<String>,
        /// The port project whose annotations (`rhipper/annotations/*.json`) name the functions and give them to the hand-written sources.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Write the open assumptions as checks: a `ra_assume` at each jump table whose index nothing bounds, which stops the port
        /// with the function, what was assumed and the site when it does not hold. A C or C++ translation only.
        #[arg(long)]
        checked: bool,
    },
    /// The IR of every function. With `--out`, writes `program.ll`, `image.bin` and `image.h` there.
    Lift {
        pe: PathBuf,
        /// The directory to write into; without it, the IR goes to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Print no summary and no warnings.
        #[arg(long)]
        quiet: bool,
        /// How far to raise: `machine` keeps the lifted IR over `%State`, `promoted` turns the
        /// registers and flags into SSA values but keeps the `%State` interface, `abi` applies the
        /// inferred signatures.
        #[arg(long, default_value = "abi")]
        level: ra_asm_lift::RaiseLevel,
        /// Write the data sections as globals and resolve the relocated references to them, so the
        /// IR stands without the image; the default at the `abi` level, as the oracle and the emit
        /// have it. The machine and promoted IR address the mapped image without it.
        #[arg(long)]
        standalone: bool,
        /// Print the coverage report instead of the IR: the instructions the lift does not cover,
        /// the functions by trap count, the imports without a prototype, the unresolved jumps.
        #[arg(long)]
        report: bool,
        /// The x87 unit: `double` (eight doubles) or `exact` (the 80-bit unit through the runtime's `f80`).
        #[arg(long, default_value = "double")]
        x87: ra_asm_lift::X87Unit,
    },
    /// The program through the IR interpreter: what it prints goes to stdout, its exit code is
    /// the command's. The lift traps and the faults of the machine stop the run with a message
    /// that names the address.
    Run {
        pe: PathBuf,
        /// The program's arguments.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        /// How far to raise before running: `machine`, `promoted` or `abi`.
        #[arg(long, default_value = "machine")]
        level: ra_asm_lift::RaiseLevel,
        /// The x87 unit: `double` (eight doubles) or `exact` (the 80-bit unit).
        #[arg(long, default_value = "double")]
        x87: ra_asm_lift::X87Unit,
        /// Name every function entered on stderr.
        #[arg(long)]
        trace: bool,
        /// Stop after this many IR instructions; 0 for no limit.
        #[arg(long, default_value_t = 2_000_000_000)]
        limit: u64,
        /// Print no warnings of the lift.
        #[arg(long)]
        quiet: bool,
        /// Write the facts of the run (the targets of its indirect calls, the functions entered)
        /// as JSON here, for the analysis of the next pass: `build/facts.json` in a project.
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Write the record of the run here: the calls and returns of the program's functions with
        /// their values, as a port emitted with `run.trace` writes its trace, for `ras serve`'s
        /// replay against the port's.
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// The instructions at an address: its `.pdata` extent, one run of code, or `length` bytes.
    Disasm {
        pe: PathBuf,
        /// `0x140001000`, `140001000` or `F_140001000`.
        address: String,
        /// The number of bytes, in hexadecimal.
        length: Option<String>,
    },
    /// Every call or jump to a function, and every pointer to it in the data.
    Callers {
        pe: PathBuf,
        /// `0x140001000`, `140001000` or `F_140001000`.
        address: String,
    },
    /// Every instruction and data pointer that refers into an address range `[lo, hi)`.
    Xrefs {
        pe: PathBuf,
        lo: String,
        /// The first address after the range; `lo + 1` without it.
        hi: Option<String>,
    },
    /// Two builds of a program matched function by function: the changed functions, the most
    /// changed first, and the functions one build has alone.
    Diff {
        old: PathBuf,
        new: PathBuf,
        /// Match by structure alone, the builds' symbols aside.
        #[arg(long)]
        structure: bool,
        /// Print the diff of the raised IR of each changed function.
        #[arg(long)]
        ir: bool,
        /// The lines of context around each change of the IR diff.
        #[arg(long, default_value_t = 3)]
        context: usize,
    },
    /// The steps from the reference binary to a running port: analyze, lift, emit, build and run,
    /// in the project whose `rhipper.json` is in `--project` (the current directory without it).
    Pipeline {
        #[arg(long)]
        project: Option<PathBuf>,
        /// The first step to run; every step after it runs too.
        #[arg(long, conflicts_with = "only")]
        from: Option<String>,
        /// The one step to run.
        #[arg(long)]
        only: Option<String>,
        /// Stop the port after this many seconds and count the run as alive.
        #[arg(long)]
        timeout: Option<u64>,
        /// The build configuration, such as `Debug`, instead of the project's.
        #[arg(long)]
        config: Option<String>,
        /// Run the analyze, lift and emit steps whatever their stamps say, and delete the build
        /// outputs before the build.
        #[arg(long)]
        rebuild: bool,
        /// Print the wall time and the peak working set of every step, and keep them in
        /// `build/pipeline/status.json`.
        #[arg(long)]
        stats: bool,
    },
    /// The tools a port needs on this machine, each with its version or `missing` and what it is
    /// needed for; fails when a required one is missing. With `--host <exe>`, the imports of a
    /// Windows program instead, each with what answers it off Windows, and fails when one is
    /// missing from the runtime's Win32 layer.
    Doctor {
        /// The language of the port the tools are required for: `c` (also `cpp`) or `rust`.
        #[arg(long, default_value = "c")]
        lang: String,
        /// A Windows program whose imports are checked against the Win32 layer of the runtime.
        #[arg(long)]
        host: Option<PathBuf>,
    },
    /// The plugins `ras` loads: the project's (`rhipper/plugins/<name>/<name>.wasm` under
    /// `--project`) and the user's (`RHIPPER_PLUGINS`), each with what it adds.
    Plugins {
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// The engine as a service: JSON-RPC over stdio, one request and one response per line, with
    /// the image and its analysis kept in memory. Serves the project of `--project` (the current
    /// directory without it), or a bare image with `--exe`.
    Serve {
        #[arg(long, conflicts_with = "exe")]
        project: Option<PathBuf>,
        #[arg(long)]
        exe: Option<PathBuf>,
    },
    /// The analysis, the lift and the emit over every executable and library under a directory,
    /// each file in a process of its own with a time limit: the coverage summed, the traps by
    /// mnemonic, the slowest files, and the panics with their backtraces. Fails when a file
    /// panicked or crashed, after the report.
    Smoke {
        dir: PathBuf,
        /// The time limit of one file, in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// The files run at once.
        #[arg(long, default_value_t = 1)]
        jobs: usize,
        /// The time limit of the whole run, in seconds: a file not started when it is spent is
        /// reported as skipped. No limit without it.
        #[arg(long)]
        budget: Option<u64>,
        /// How many directory levels under `dir` to enter: 0 for its own files only; all without it.
        #[arg(long)]
        depth: Option<usize>,
        /// An extension to take, repeatable; without it, exe, dll, sys, ocx, cpl, scr, drv and efi.
        #[arg(long = "ext", value_name = "EXT")]
        extensions: Vec<String>,
        /// Write the report as JSON to this file too.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Print no line per file on stderr.
        #[arg(long)]
        quiet: bool,
    },
    /// The backlog as roadmap cards, in the outline `rc import` reads. With `--pending`, a card
    /// per ten lines of each pending list (`PENDING.txt`, `*_PENDING.txt`) at or under the
    /// paths, `testdata` without any: the title is the line, the acceptance the list's replay
    /// with the lines removed, the files the list and the source its kind points at.
    Cards {
        /// Write the cards of the pending lists.
        #[arg(long)]
        pending: bool,
        /// A pending list, or a directory searched for them.
        paths: Vec<PathBuf>,
        /// The milestone the sections go under: printed as the outline's first heading.
        #[arg(long, value_name = "TITLE")]
        milestone: Option<String>,
    },
    /// One file of `ras smoke`, in the process it starts for the file.
    #[command(hide = true)]
    SmokeFile { pe: PathBuf },
}

fn step(text: Option<String>) -> anyhow::Result<Option<pipeline::Step>> {
    text.map(|t| pipeline::Step::parse(&t).ok_or_else(|| anyhow::anyhow!("no step `{t}`: the steps are analyze, lift, emit, build, run")))
        .transpose()
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    let json = options.json;
    let sigs = &options.signatures;
    match options.command {
        Command::Info { pe } => report::info(&load(&pe, sigs)?, json),
        Command::Sections { pe } => report::sections(&load(&pe, sigs)?, json),
        Command::Imports { pe } => report::imports(&load(&pe, sigs)?, json),
        Command::Exports { pe } => report::exports(&load(&pe, sigs)?, json),
        Command::Relocs { pe } => report::relocs(&load(&pe, sigs)?, json),
        Command::Debug { pe } => report::debug(&load(&pe, sigs)?, json),
        Command::Symbols { pe } => report::symbols(&load(&pe, sigs)?, json),
        Command::Functions { pe, pdata: true } => report::functions(&load(&pe, sigs)?, true, json),
        Command::Functions { pe, pdata: false } => functions::functions(&load(&pe, sigs)?, json),
        Command::Cfg { pe, address } => functions::cfg(&load(&pe, sigs)?, disasm::parse_address(&address)?, json),
        Command::Lift { pe, out, quiet, level, standalone, report, x87 } => {
            let standalone = standalone || level == ra_asm_lift::RaiseLevel::Abi;
            lift::lift(&load(&pe, sigs)?, out.as_deref(), &lift::Options { level, standalone, quiet, report, json, x87 })
        }
        Command::Signatures { pe, build: Some(libraries), installed, toolset, store, .. } => {
            let image = pe.as_deref().map(ra_asm_pe::load).transpose()?;
            signatures::build(store.as_deref(), &libraries, installed, toolset.as_deref(), image.as_ref(), json)
        }
        Command::Signatures { pe, list: true, store, project, .. } => {
            let (stores, exe) = match project {
                Some(dir) => {
                    let project = project::Project::load(&dir)?;
                    let stores = store.map_or_else(|| project.signature_stores(), |s| vec![s]);
                    (stores, pe.or(Some(project.exe)))
                }
                None => (store.or_else(signatures::user_store).into_iter().collect(), pe),
            };
            let image = exe.as_deref().map(ra_asm_pe::load).transpose()?;
            signatures::list(&stores, image.as_ref(), json)
        }
        Command::Signatures { pe, mut from, runtime, write, .. } if runtime || !from.is_empty() || write.is_some() => {
            let pe = pe.context("a PE image is required")?;
            let mut image = load(&pe, sigs)?;
            if runtime {
                let libraries = ra_asm_analysis::signatures::runtime_for(&image)
                    .with_context(|| format!("no installed runtime libraries fit the toolset {}", ra_asm_pe::Toolset::of(&image).name))?;
                from.extend(libraries);
            }
            signatures::recognize(&mut image, &from, write.as_deref(), json)
        }
        Command::Signatures { pe, .. } => signatures::signatures(&load(&pe.context("a PE image is required")?, sigs)?, json),
        Command::Emit { pe, lang, out, quiet, x87, hand, project, checked } => {
            emit::emit(&load(&pe, sigs)?, &lang, out.as_deref(), quiet, x87, &hand, project.as_deref(), checked)
        }
        Command::Run { pe, args, level, x87, trace, limit, quiet, facts, record } => {
            let code = run::run(&load(&pe, sigs)?, &run::Options { level, x87, trace, limit, args, quiet, facts, record })?;
            std::process::exit(code)
        }
        Command::Disasm { pe, address, length } => {
            let address = disasm::parse_address(&address)?;
            let length = length.as_deref().map(disasm::parse_address).transpose()?;
            disasm::disasm(&load(&pe, sigs)?, address, length, json)
        }
        Command::Callers { pe, address } => xrefs::callers(&load(&pe, sigs)?, disasm::parse_address(&address)?, json),
        Command::Xrefs { pe, lo, hi } => {
            let lo = disasm::parse_address(&lo)?;
            let hi = hi.as_deref().map(disasm::parse_address).transpose()?;
            xrefs::xrefs(&load(&pe, sigs)?, lo, hi, json)
        }
        Command::Diff { old, new, structure, ir, context } => {
            let options = diff::Options { symbols: !structure, ir: ir.then_some(context), json };
            diff::diff(&load(&old, sigs)?, &load(&new, sigs)?, &options)
        }
        Command::Pipeline { project, from, only, timeout, config, rebuild, stats } => {
            let options = pipeline::Options { from: step(from)?, only: step(only)?, timeout, config, rebuild, stats };
            pipeline::run(&pipeline::project_dir(project.as_deref()), &options)
        }
        Command::Plugins { project } => plugins::list(project.as_deref(), json),
        Command::Doctor { lang: _, host: Some(exe) } => doctor::host(&exe, json),
        Command::Doctor { lang, host: None } => doctor::doctor(doctor::Port::of(&lang), json),
        Command::Serve { project, exe } => serve::serve(&serve::Target::of(project.as_deref(), exe.as_deref())?),
        Command::Smoke { dir, timeout, jobs, budget, depth, extensions, out, quiet } => {
            let timeout = std::time::Duration::from_secs(timeout);
            let budget = budget.map(std::time::Duration::from_secs);
            let options = smoke::Options { timeout, jobs, budget, depth, extensions, signatures: sigs.clone(), out, quiet, json };
            smoke::smoke(&dir, &options)
        }
        Command::Cards { pending, paths, milestone } => {
            anyhow::ensure!(pending, "`ras cards` writes the cards of the pending lists: pass --pending");
            cards::pending(&paths, milestone.as_deref())
        }
        Command::SmokeFile { pe } => {
            smoke::file(&pe, sigs);
            Ok(())
        }
    }
}
