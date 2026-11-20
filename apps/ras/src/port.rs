//! `ras port`: a console program as a program of this machine. The image is lifted whole at the
//! machine level (a ROM with the programs it keeps for RAM), the module is written for a
//! compiler (`program.ll`) with the C that ties it to the run (`port.c`) and the image's bytes
//! (`image.bin`), the run's library is put beside them, and clang builds the three into an
//! executable that runs the program on its console's machine, as `ras run` does, with the
//! compiled functions in place of their IR.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use ra_asm_pe::Image;

/// The run's library, as cargo builds it beside `ras`.
const LIBRARY: &str = "ra_asm_port.dll";
const IMPORT_LIBRARY: &str = "ra_asm_port.dll.lib";

pub(crate) struct Options {
    /// The name of the executable, without its extension: the directory's name without one.
    pub name: Option<String>,
    /// Write the sources and build nothing.
    pub no_build: bool,
    pub quiet: bool,
    /// The programs the console loads (a game's executable off its disc) lifted with it.
    pub with: Vec<PathBuf>,
    /// The entries a run found: the functions of those programs nothing in them names.
    pub entries: Option<PathBuf>,
}

/// Where the run's library is: beside this executable, where the same build of the workspace
/// puts it.
fn library_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot tell where ras is")?;
    let dir = exe.parent().context("ras has no directory")?.to_path_buf();
    if !dir.join(LIBRARY).is_file() || !dir.join(IMPORT_LIBRARY).is_file() {
        bail!(
            "{LIBRARY} and {IMPORT_LIBRARY} are not beside ras in {}: build them with `cargo build --release -p ra-asm-port`",
            dir.display()
        );
    }
    Ok(dir)
}

/// Writes the port of `image` into `out` and builds it.
pub(crate) fn port(image: &Image, out: &Path, options: &Options) -> Result<()> {
    if !ra_asm_lift::run::has_machine(image) {
        bail!("the image is no console's program: `ras port` builds a program with its console's machine");
    }
    let level = ra_asm_lift::RaiseLevel::Machine;
    let lift = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    // The programs the console loads that are lifted with it, each with the entries a run found.
    let entries: Vec<u64> = match &options.entries {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("cannot read the entries at {}", path.display()))?;
            text.lines().filter_map(|line| line.split_whitespace().next()).filter_map(|word| u64::from_str_radix(word, 16).ok()).collect()
        }
        None => Vec::new(),
    };
    let mut loaded = Vec::new();
    for (n, path) in options.with.iter().enumerate() {
        let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        loaded.push(ra_asm_lift::Loaded::new(bytes, &format!("loaded-{n}.exe"), &entries)?);
    }
    let mut whole = ra_asm_lift::Whole::lift_with(image, &loaded, lift, level)?;
    if !options.quiet {
        for w in &whole.warnings {
            eprintln!("warning: {w}");
        }
        crate::run::report_images(&whole);
    }
    ra_asm_lift::port::nativize(&mut whole.lifted.module);
    std::fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        std::fs::write(out.join(name), bytes).with_context(|| format!("cannot write {}", out.join(name).display()))
    };
    write("program.ll", whole.lifted.module.to_ll().as_bytes())?;
    write("port.c", ra_asm_lift::port::source(&whole.lifted, &loaded).as_bytes())?;
    write("image.bin", image.file())?;
    for (n, program) in loaded.iter().enumerate() {
        write(&format!("loaded-{n}.bin"), program.image.file())?;
    }
    let library = library_dir()?;
    for name in [LIBRARY, IMPORT_LIBRARY] {
        std::fs::copy(library.join(name), out.join(name)).with_context(|| format!("cannot copy {name} into {}", out.display()))?;
    }
    let name =
        options.name.clone().or_else(|| out.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| "port".to_owned());
    let exe = format!("{name}.exe");
    let arguments = ["-O2", "-std=c23", "-Wno-override-module", "program.ll", "port.c", IMPORT_LIBRARY, "-o", exe.as_str()];
    write("build.cmd", format!("@echo off\r\nclang {}\r\n", arguments.join(" ")).as_bytes())?;
    if options.no_build {
        return Ok(());
    }
    if !options.quiet {
        eprintln!("ra: {} functions; building {}", whole.lifted.functions.len(), out.join(&exe).display());
    }
    let status =
        Command::new("clang").args(arguments).current_dir(out).status().context("cannot run clang: `ras doctor` says what a port needs")?;
    if !status.success() {
        bail!("clang did not build the port: the command is in {}", out.join("build.cmd").display());
    }
    if !options.quiet {
        eprintln!("ra: built {}", out.join(&exe).display());
    }
    Ok(())
}
