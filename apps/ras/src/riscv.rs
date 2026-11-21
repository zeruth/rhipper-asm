//! `ras riscv`: a RISC-V program, read, lifted and run.
//!
//! The command takes a program's ELF file. By itself it lists the instructions of its code, each
//! at its address, a compressed one as the instruction it stands for. `--ll` writes the IR of
//! the lift, `--run` runs the lifted program over the interpreter with the system calls a
//! program without a C library makes, and `--port` writes a directory of the program as C with
//! the C runtime beside it, built into an executable of this machine.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ra_asm_riscv::run::Program;

/// The runtime a port's source is compiled with.
const RISCV_C: &str = include_str!("../../../runtime/riscv.c");

pub(crate) struct Options {
    pub(crate) ll: Option<PathBuf>,
    pub(crate) port: Option<PathBuf>,
    pub(crate) no_build: bool,
    pub(crate) run: bool,
    pub(crate) limit: u64,
}

pub(crate) fn riscv(path: &Path, options: &Options) -> anyhow::Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let program = Program::parse(&bytes).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    if options.ll.is_none() && options.port.is_none() && !options.run {
        return list(&program);
    }
    let module = program.lift().map_err(|e| anyhow::anyhow!("the lift: {e}"))?;
    if let Some(path) = &options.ll {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).with_context(|| format!("writing {}", path.display()))?);
        module.write_ll(&mut file)?;
        file.flush()?;
        eprintln!("{}: {} blocks", path.display(), module.functions.iter().map(|f| f.blocks.len()).sum::<usize>());
    }
    if let Some(directory) = &options.port {
        let source = ra_asm_emit::c::emit_c(&module, &ra_asm_emit::c::Options::default());
        let name = path.file_stem().map_or_else(|| "program".to_owned(), |stem| stem.to_string_lossy().into_owned());
        port(directory, &name, &source, options.no_build)?;
    }
    if options.run {
        let outcome = ra_asm_riscv::run::run(&program, options.limit).map_err(|e| anyhow::anyhow!("{e}"))?;
        std::io::stdout().write_all(&outcome.out)?;
        std::io::stdout().flush()?;
        std::process::exit(outcome.exit.unwrap_or(0));
    }
    Ok(())
}

/// The instructions of the program's code, each at its address.
fn list(program: &Program) -> anyhow::Result<()> {
    let code = program.segments.iter().find(|s| s.executable).context("the program has no code")?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut offset = 0;
    while offset + 2 <= code.bytes.len() {
        let address = code.address + offset as u64;
        let (instruction, length) = ra_asm_riscv::decode_at(&code.bytes[offset..], address, program.width);
        match instruction {
            Some(instruction) => writeln!(out, "{address:#010x}  {instruction}")?,
            None => writeln!(out, "{address:#010x}  unknown")?,
        }
        offset += usize::from(length);
    }
    Ok(())
}

/// Writes the port of a program into `directory`: its lift as `program.c`, the runtime it is
/// compiled with (`riscv.c`, `rhipper.h`) and `build.cmd`, the command that builds them; then
/// builds the executable unless `no_build` or there is no clang.
fn port(directory: &Path, name: &str, source: &str, no_build: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory).with_context(|| format!("making {}", directory.display()))?;
    for (file, text) in [("program.c", source), ("riscv.c", RISCV_C), ("rhipper.h", crate::emit::RHIPPER_H)] {
        ra_asm_emit::write_if_changed(&directory.join(file), text.as_bytes()).with_context(|| format!("writing {file}"))?;
    }
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let arguments = ["-O2", "-I", ".", "program.c", "riscv.c", "-o", executable.as_str()];
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
