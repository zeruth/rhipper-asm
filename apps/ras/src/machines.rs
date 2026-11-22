//! `ras riscv`, `ras ppc`, `ras sh4`: a program of a machine whose lift stands beside the
//! pipeline, read, lifted and run.
//!
//! The command takes a program's ELF file and tells the machine by it: RISC-V of either width,
//! 32-bit big-endian PowerPC, little-endian SH-4. By itself it lists the instructions of the
//! program's code, each at its address. `--ll` writes the IR of the lift, `--run` runs the lifted
//! program over the interpreter with the system calls a program without a C library makes, and
//! `--port` writes a directory of the program as C with the machine's C runtime beside it, built
//! into an executable of this machine.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ra_asm_ir::Module;

pub(crate) struct Options {
    pub(crate) ll: Option<PathBuf>,
    pub(crate) port: Option<PathBuf>,
    pub(crate) no_build: bool,
    pub(crate) run: bool,
    pub(crate) limit: u64,
}

/// A program of one of the machines.
enum Program {
    Riscv(ra_asm_riscv::run::Program),
    Ppc(ra_asm_ppc::run::Program),
    Sh4(ra_asm_sh::run::Program),
}

impl Program {
    /// Reads an ELF file as a program of the machine its header names.
    fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let big = bytes.get(5) == Some(&2);
        let machine = bytes.get(18..20).map(|m| if big { u16::from_be_bytes([m[0], m[1]]) } else { u16::from_le_bytes([m[0], m[1]]) });
        let failed = |e: String| anyhow::anyhow!("{e}");
        // A DOL file, the GameCube's executable, has no mark: the PowerPC reader knows one by
        // its header's sections.
        if bytes.get(..4) != Some(b"\x7fELF")
            && let Ok(program) = ra_asm_ppc::run::Program::parse(bytes)
        {
            return Ok(Self::Ppc(program));
        }
        match machine {
            Some(243) => Ok(Self::Riscv(ra_asm_riscv::run::Program::parse(bytes).map_err(|e| failed(e.0))?)),
            Some(20 | 21) => Ok(Self::Ppc(ra_asm_ppc::run::Program::parse(bytes).map_err(|e| failed(e.0))?)),
            Some(42) => Ok(Self::Sh4(ra_asm_sh::run::Program::parse(bytes).map_err(|e| failed(e.0))?)),
            Some(other) => anyhow::bail!("an ELF file of machine {other}, which is none of RISC-V, PowerPC and SH-4"),
            None => anyhow::bail!("no ELF file"),
        }
    }

    /// The runtime in C a port of the program is compiled with: its file's name and its text.
    fn runtime(&self) -> (&'static str, &'static str) {
        match self {
            Self::Riscv(_) => ("riscv.c", include_str!("../../../runtime/riscv.c")),
            Self::Ppc(_) => ("ppc.c", include_str!("../../../runtime/ppc.c")),
            Self::Sh4(_) => ("sh4.c", include_str!("../../../runtime/sh4.c")),
        }
    }

    fn lift(&self) -> anyhow::Result<Module> {
        match self {
            // A RISC-V program lifts as its functions, a function of the IR for each.
            Self::Riscv(program) => program.lift_functions().map_err(|e| anyhow::anyhow!("the lift: {e}")),
            Self::Ppc(program) => program.lift().map_err(|e| anyhow::anyhow!("the lift: {e}")),
            Self::Sh4(program) => program.lift().map_err(|e| anyhow::anyhow!("the lift: {e}")),
        }
    }

    /// Runs the lifted program: what it printed and the code it ended with.
    fn run(&self, limit: u64) -> anyhow::Result<(Vec<u8>, Option<i32>)> {
        Ok(match self {
            Self::Riscv(program) => {
                let outcome = ra_asm_riscv::run::run_functions(program, limit).map_err(|e| anyhow::anyhow!("{e}"))?;
                (outcome.out, outcome.exit)
            }
            Self::Ppc(program) => {
                let outcome = ra_asm_ppc::run::run(program, limit).map_err(|e| anyhow::anyhow!("{e}"))?;
                (outcome.out, outcome.exit)
            }
            Self::Sh4(program) => {
                let outcome = ra_asm_sh::run::run(program, limit).map_err(|e| anyhow::anyhow!("{e}"))?;
                (outcome.out, outcome.exit)
            }
        })
    }

    /// The instructions of the program's code, each at its address; a word that is none is
    /// `unknown` (a constant the code keeps between its functions, a header the file maps).
    fn list(&self) -> anyhow::Result<()> {
        let mut out = std::io::BufWriter::new(std::io::stdout().lock());
        match self {
            Self::Riscv(program) => {
                let code = program.segments.iter().find(|s| s.executable).context("the program has no code")?;
                let mut offset = 0;
                while offset + 2 <= code.bytes.len() {
                    let address = code.address + offset as u64;
                    let (instruction, length) = ra_asm_riscv::decode_at(&code.bytes[offset..], address, program.width);
                    let text = instruction.map_or_else(|| "unknown".to_owned(), |i| i.to_string());
                    writeln!(out, "{address:#010x}  {text}")?;
                    offset += usize::from(length);
                }
            }
            Self::Ppc(program) => {
                let code = program.segments.iter().find(|s| s.executable).context("the program has no code")?;
                for (n, word) in code.bytes.chunks_exact(4).enumerate() {
                    let address = code.address + 4 * n as u64;
                    let word = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
                    let text = ra_asm_ppc::disasm::disassemble(word, address).unwrap_or_else(|| "unknown".to_owned());
                    writeln!(out, "{address:#010x}  {text}")?;
                }
            }
            Self::Sh4(program) => {
                let code = program.segments.iter().find(|s| s.executable).context("the program has no code")?;
                let decoder = ra_asm_sh::Sh4 { big_endian: program.big_endian, ..ra_asm_sh::Sh4::default() };
                for (n, word) in code.bytes.chunks_exact(2).enumerate() {
                    let address = code.address + 2 * n as u64;
                    let word =
                        if program.big_endian { u16::from_be_bytes([word[0], word[1]]) } else { u16::from_le_bytes([word[0], word[1]]) };
                    let decoded = decoder.decode_word(word, address);
                    let text = decoded.map_or_else(|| "unknown".to_owned(), |i| i.text);
                    writeln!(out, "{address:#010x}  {text}")?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn machine(path: &Path, options: &Options) -> anyhow::Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let program = Program::parse(&bytes).with_context(|| path.display().to_string())?;
    if options.ll.is_none() && options.port.is_none() && !options.run {
        return program.list();
    }
    let module = program.lift()?;
    if let Some(path) = &options.ll {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).with_context(|| format!("writing {}", path.display()))?);
        module.write_ll(&mut file)?;
        file.flush()?;
        eprintln!("{}: {} blocks", path.display(), module.functions.iter().map(|f| f.blocks.len()).sum::<usize>());
    }
    if let Some(directory) = &options.port {
        let source = ra_asm_emit::c::emit_c(&module, &ra_asm_emit::c::Options::default());
        let name = path.file_stem().map_or_else(|| "program".to_owned(), |stem| stem.to_string_lossy().into_owned());
        port(directory, &name, &source, program.runtime(), options.no_build)?;
    }
    if options.run {
        let (printed, exit) = program.run(options.limit)?;
        std::io::stdout().write_all(&printed)?;
        std::io::stdout().flush()?;
        std::process::exit(exit.unwrap_or(0));
    }
    Ok(())
}

/// Writes the port of a program into `directory`: its lift as `program.c`, the runtime it is
/// compiled with and `rhipper.h`, and `build.cmd`, the command that builds them; then builds the
/// executable unless `no_build` or there is no clang.
fn port(directory: &Path, name: &str, source: &str, runtime: (&str, &str), no_build: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory).with_context(|| format!("making {}", directory.display()))?;
    for (file, text) in [("program.c", source), runtime, ("rhipper.h", crate::emit::RHIPPER_H)] {
        ra_asm_emit::write_if_changed(&directory.join(file), text.as_bytes()).with_context(|| format!("writing {file}"))?;
    }
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    // The lift's products and sums are the program's own: the compiler is not to fuse them.
    let arguments = ["-O2", "-ffp-contract=off", "-I", ".", "program.c", runtime.0, "-o", executable.as_str()];
    std::fs::write(directory.join("build.cmd"), format!("clang {}\n", arguments.join(" ")))?;
    if no_build {
        eprintln!("{}: the sources are written; build.cmd builds {executable}", directory.display());
        return Ok(());
    }
    match std::process::Command::new("clang").args(arguments).current_dir(directory).output() {
        Ok(output) if output.status.success() => eprintln!("{}", directory.join(&executable).display()),
        Ok(output) => anyhow::bail!("clang did not build the port:\n{}", String::from_utf8_lossy(&output.stderr)),
        Err(_) => {
            eprintln!(
                "{}: the sources are written; clang is not on the PATH to build them (build.cmd has the command)",
                directory.display()
            );
        }
    }
    Ok(())
}
