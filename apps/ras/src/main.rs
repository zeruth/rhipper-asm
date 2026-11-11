//! Rhipper Asm: the command line over the disassembly core. Each subcommand prints a table for a
//! person, or JSON with `--json` for the workbench and the agents.

mod disasm;
mod emit;
mod functions;
mod lift;
mod pipeline;
mod project;
mod report;
mod serve;
mod signatures;
mod xrefs;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ras", version, about = "Rhipper Asm: disassembly to the Rhipper IR")]
struct Options {
    /// Print JSON instead of a table.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
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
    /// The functions of the image. With `--pdata`, the raw exception table entries.
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
    /// What each function takes and returns, as the lift infers it.
    Signatures { pe: PathBuf },
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
        /// Keep the machine IR: run no raising pass.
        #[arg(long, conflicts_with = "promoted")]
        machine: bool,
        /// Promote the registers but keep the `%State` interface.
        #[arg(long)]
        promoted: bool,
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
        /// Delete the build outputs before the build.
        #[arg(long)]
        rebuild: bool,
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
}

fn step(text: Option<String>) -> anyhow::Result<Option<pipeline::Step>> {
    text.map(|t| pipeline::Step::parse(&t).ok_or_else(|| anyhow::anyhow!("no step `{t}`: the steps are analyze, lift, emit, build, run")))
        .transpose()
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    let json = options.json;
    match options.command {
        Command::Info { pe } => report::info(&ra_asm_pe::load(&pe)?, json),
        Command::Sections { pe } => report::sections(&ra_asm_pe::load(&pe)?, json),
        Command::Imports { pe } => report::imports(&ra_asm_pe::load(&pe)?, json),
        Command::Exports { pe } => report::exports(&ra_asm_pe::load(&pe)?, json),
        Command::Relocs { pe } => report::relocs(&ra_asm_pe::load(&pe)?, json),
        Command::Debug { pe } => report::debug(&ra_asm_pe::load(&pe)?, json),
        Command::Functions { pe, pdata: true } => report::functions(&ra_asm_pe::load(&pe)?, true, json),
        Command::Functions { pe, pdata: false } => functions::functions(&ra_asm_pe::load(&pe)?, json),
        Command::Cfg { pe, address } => functions::cfg(&ra_asm_pe::load(&pe)?, disasm::parse_address(&address)?, json),
        Command::Lift { pe, out, quiet, machine, promoted } => {
            let level = if machine {
                ra_asm_lift::RaiseLevel::Machine
            } else if promoted {
                ra_asm_lift::RaiseLevel::Promoted
            } else {
                ra_asm_lift::RaiseLevel::Abi
            };
            lift::lift(&ra_asm_pe::load(&pe)?, out.as_deref(), quiet, level)
        }
        Command::Signatures { pe } => signatures::signatures(&ra_asm_pe::load(&pe)?, json),
        Command::Emit { pe, lang, out, quiet } => emit::emit(&ra_asm_pe::load(&pe)?, &lang, out.as_deref(), quiet),
        Command::Disasm { pe, address, length } => {
            let address = disasm::parse_address(&address)?;
            let length = length.as_deref().map(disasm::parse_address).transpose()?;
            disasm::disasm(&ra_asm_pe::load(&pe)?, address, length, json)
        }
        Command::Callers { pe, address } => xrefs::callers(&ra_asm_pe::load(&pe)?, disasm::parse_address(&address)?, json),
        Command::Xrefs { pe, lo, hi } => {
            let lo = disasm::parse_address(&lo)?;
            let hi = hi.as_deref().map(disasm::parse_address).transpose()?;
            xrefs::xrefs(&ra_asm_pe::load(&pe)?, lo, hi, json)
        }
        Command::Pipeline { project, from, only, timeout, config, rebuild } => {
            let options = pipeline::Options { from: step(from)?, only: step(only)?, timeout, config, rebuild };
            pipeline::run(&pipeline::project_dir(project.as_deref()), &options)
        }
        Command::Serve { project, exe } => serve::serve(&serve::Target::of(project.as_deref(), exe.as_deref())?),
    }
}
