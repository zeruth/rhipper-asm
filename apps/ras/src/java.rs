//! `ras java`: the classes of a Java program, read, lifted and run.
//!
//! The command takes class files and directories of them. By itself it says what each class is:
//! the release that compiled it, its methods and the instructions of each. `--ll` writes the IR
//! the lift makes of them all, and `--run` runs the `main` of a class over the interpreter with
//! the machine's runtime, printing what the program prints. `--c` writes the classes as C, and
//! `--port` a directory of that source with the C runtime beside it, built into an executable
//! that runs the `main` of the class `--main` names.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ra_asm_jvm::flow::flow;
use ra_asm_jvm::{ClassFile, version};

/// The runtime a port's source is compiled with.
const JVM_C: &str = include_str!("../../../runtime/jvm.c");

pub(crate) struct Options {
    pub(crate) ll: Option<PathBuf>,
    pub(crate) c: Option<PathBuf>,
    pub(crate) main: Option<String>,
    pub(crate) port: Option<PathBuf>,
    pub(crate) no_build: bool,
    pub(crate) run: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) limit: u64,
    pub(crate) json: bool,
}

/// The class files `paths` name: each file, and every `.class` under each directory, in order.
fn class_files(paths: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    fn walk(directory: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .with_context(|| format!("reading {}", directory.display()))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                walk(&entry, out)?;
            } else if entry.extension().is_some_and(|e| e == "class") {
                out.push(entry);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut out)?;
        } else {
            out.push(path.clone());
        }
    }
    anyhow::ensure!(!out.is_empty(), "no class file among the paths given");
    Ok(out)
}

pub(crate) fn java(paths: &[PathBuf], options: &Options) -> anyhow::Result<()> {
    let mut classes = Vec::new();
    for path in class_files(paths)? {
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        classes.push(ClassFile::parse(&bytes).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?);
    }
    if options.ll.is_none() && options.run.is_none() && options.c.is_none() && options.port.is_none() {
        return list(&classes, options.json);
    }
    let module = ra_asm_jvm::lift::lift(&classes).map_err(|e| anyhow::anyhow!("the lift: {e}"))?;
    // The source names what the module does by C's identifiers, and with `--main` it has the
    // function a C program starts at, which runs that class's `main`.
    let source = |main: Option<&String>| -> anyhow::Result<String> {
        let mut named = module.clone();
        if let Some(main) = main {
            ra_asm_jvm::lift::entry(&mut named, &main.replace('.', "/")).map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        ra_asm_jvm::names::c_names(&mut named);
        Ok(ra_asm_emit::c::emit_c(&named, &ra_asm_emit::c::Options::default()))
    };
    if let Some(path) = &options.c {
        std::fs::write(path, source(options.main.as_ref())?).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("{}: {} functions of {} classes", path.display(), module.functions.len(), classes.len());
    }
    if let Some(directory) = &options.port {
        let main = options.main.as_ref().context("a port runs a class's `main`: name the class with --main")?;
        port(directory, main, &source(Some(main))?, options.no_build)?;
    }
    if let Some(path) = &options.ll {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).with_context(|| format!("writing {}", path.display()))?);
        module.write_ll(&mut file)?;
        file.flush()?;
        eprintln!("{}: {} functions of {} classes", path.display(), module.functions.len(), classes.len());
    }
    if let Some(main) = &options.run {
        let main = main.replace('.', "/");
        let arguments: Vec<&str> = options.args.iter().map(String::as_str).collect();
        let outcome =
            ra_asm_jvm::run::run(&module, &main, &arguments, options.limit).map_err(|stop| anyhow::anyhow!("the run stopped: {stop:?}"))?;
        std::io::stdout().write_all(&outcome.out)?;
        std::io::stderr().write_all(&outcome.err)?;
        if outcome.thrown.is_some() {
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Writes the port of a program into `directory`: its classes as `program.c`, the runtime they
/// are compiled with (`jvm.c`, `rhipper.h`) and `build.cmd`, the command that builds them; then
/// builds the executable, named after the class, unless `no_build` or there is no clang.
fn port(directory: &Path, main: &str, source: &str, no_build: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory).with_context(|| format!("making {}", directory.display()))?;
    for (name, text) in [("program.c", source), ("jvm.c", JVM_C), ("rhipper.h", crate::emit::RHIPPER_H)] {
        ra_asm_emit::write_if_changed(&directory.join(name), text.as_bytes()).with_context(|| format!("writing {name}"))?;
    }
    let name = main.rsplit(['.', '/']).next().unwrap_or(main).to_lowercase();
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let arguments = ["-O2", "-I", ".", "program.c", "jvm.c", "-o", executable.as_str()];
    std::fs::write(directory.join("build.cmd"), format!("clang {}\n", arguments.join(" ")))?;
    if no_build {
        eprintln!("{}: the sources are written; build.cmd builds {executable}", directory.display());
        return Ok(());
    }
    match std::process::Command::new("clang").args(arguments).current_dir(directory).output() {
        Ok(output) if output.status.success() => eprintln!("{}", directory.join(&executable).display()),
        Ok(output) => anyhow::bail!("clang did not build the port:\n{}", String::from_utf8_lossy(&output.stderr)),
        Err(_) => eprintln!(
            "{}: the sources are written; clang is not on the PATH to build them (build.cmd has the command)",
            directory.display()
        ),
    }
    Ok(())
}

/// What each class is: its release, and its methods with the instructions and the stack of each.
fn list(classes: &[ClassFile], json: bool) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for file in classes {
        let name = file.name().unwrap_or("?");
        let mut methods = Vec::new();
        for method in &file.methods {
            let (method_name, descriptor) = (file.utf8(method.name).unwrap_or("?"), file.utf8(method.descriptor).unwrap_or("?"));
            let code = file.code(method).map_err(|e| anyhow::anyhow!("{name}.{method_name}: {e}"))?;
            let walked =
                code.as_ref().map(|code| flow(file, code)).transpose().map_err(|e| anyhow::anyhow!("{name}.{method_name}: {e}"))?;
            methods.push((format!("{method_name}{descriptor}"), walked.map(|w| (w.instructions.len(), w.max_stack))));
        }
        rows.push((name, version::name(file.major).unwrap_or_default(), file.major, file.minor, methods));
    }
    if json {
        let escape = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
        let mut out = String::from("[");
        for (n, (name, release, major, minor, methods)) in rows.iter().enumerate() {
            let methods: Vec<String> = methods
                .iter()
                .map(|(method, code)| match code {
                    Some((instructions, stack)) => {
                        format!("{{\"name\":\"{}\",\"instructions\":{instructions},\"stack\":{stack}}}", escape(method))
                    }
                    None => format!("{{\"name\":\"{}\"}}", escape(method)),
                })
                .collect();
            out.push_str(&format!(
                "{}{{\"class\":\"{}\",\"release\":\"{}\",\"major\":{major},\"minor\":{minor},\"methods\":[{}]}}",
                if n > 0 { "," } else { "" },
                escape(name),
                escape(release),
                methods.join(",")
            ));
        }
        println!("{out}]");
        return Ok(());
    }
    for (name, release, major, minor, methods) in rows {
        println!("{name}: {release} (class file {major}.{minor}), {} methods", methods.len());
        for (method, code) in methods {
            match code {
                Some((instructions, stack)) => println!("  {method}: {instructions} instructions, a stack of {stack}"),
                None => println!("  {method}: no code"),
            }
        }
    }
    Ok(())
}
