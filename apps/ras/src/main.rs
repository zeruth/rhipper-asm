//! Rhipper Asm: the command line over the disassembly core. The first milestone adds the loader
//! and the decoder; the subcommands grow with them.

use clap::Parser;

#[derive(Parser)]
#[command(name = "ras", version, about = "Rhipper Asm: disassembly to the Rhipper IR")]
struct Options {}

fn main() -> anyhow::Result<()> {
    let _options = Options::parse();
    anyhow::bail!("no subcommands yet; the loader and the decoder land first")
}
