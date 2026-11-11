//! Rhipper Asm: the command line over the disassembly core. Each subcommand prints a table for a
//! person, or JSON with `--json` for the workbench and the agents.

mod report;

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
        Command::Functions { pe, pdata } => report::functions(&ra_asm_pe::load(&pe)?, pdata, json),
    }
}
