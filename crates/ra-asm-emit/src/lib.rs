//! The backends: source out of the raised IR. The C backend is first; C++ and Rust share its
//! structuring.
//!
//! The IR arrives at the ABI level: functions with parameters and results, locals in allocas, data
//! in globals, conditions as comparisons. The structuring pass turns each function's graph into
//! statements (`if`, `else`, loops, `switch`, with `goto` only where the graph is not reducible),
//! and the printer writes them in the language's syntax, inlining a value into its one use where
//! that keeps the meaning. The function table says where each function landed, for the workbench.
//! [`tree`] runs a function's statements on the IR's interpreter, beside a run of its graph.
//! A 32-bit program's imports that take a structure of pointers are called through the wrappers
//! of [`adapters`], which convert it between the program's layout and the host's.

pub mod adapters;
pub mod c;
mod coalesce;
mod common;
pub mod cpp;
mod format;
pub mod functions;
mod layout;
pub mod rust;
mod safety;
pub mod structure;
mod tidy;
pub mod tree;

pub use c::{
    ADAPTERS_FILE, Check, Dialect, EmittedFile, EmittedFunction, FUNCTIONS_PER_FILE, MODULE_MARK, Options as COptions, Unit, Watch, emit_c,
    emit_unit, emit_unit_dropping,
};
pub use cpp::{Class, Method, classes};
pub use format::format_files;
pub use functions::{FunctionInfo, Grade, format_address, table};
pub use rust::{Options as RustOptions, Output as RustOutput, emit_rust};

/// Writes the files of `unit` into `dir`, after removing the files of an earlier, larger output
/// or another dialect (`program.c`, `program.h`, `data.c`, `program_NNN.c`, a module's source and
/// header, which start with [`c::MODULE_MARK`], and the same as `.cpp`) and the import wrappers
/// of an output that had them ([`ADAPTERS_FILE`]), so a stale part never builds again.
pub fn write_files(dir: &std::path::Path, files: &[EmittedFile]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (stem, extension) = name.rsplit_once('.').unwrap_or((name.as_str(), ""));
            let source = extension == "c" || extension == "cpp";
            let part = stem.strip_prefix("program_").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let module =
                (source || extension == "h") && std::fs::read_to_string(entry.path()).is_ok_and(|text| text.starts_with(c::MODULE_MARK));
            let ours = name == "program.h" || name == ADAPTERS_FILE || module || (source && (stem == "program" || stem == "data" || part));
            if ours && !files.iter().any(|f| f.name == name) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    // The Rust modules of an earlier output, under src/.
    if let Ok(entries) = std::fs::read_dir(dir.join("src")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".rs").unwrap_or("");
            let part = stem.strip_prefix("program_").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let ours = stem == "program" || stem == "data" || stem == "main" || stem == "rhipper" || part;
            if ours && !files.iter().any(|f| f.name == format!("src/{name}")) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    for f in files {
        write_if_changed(&dir.join(&f.name), f.text.as_bytes())?;
    }
    Ok(())
}

/// Writes `bytes` to `path` unless the file holds them already, creating its directory: a file
/// the emit did not change keeps its time, so the build recompiles only the parts that changed.
pub fn write_if_changed(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if holds(path, bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

/// Moves the file at `new` over `path` unless `path` holds the same bytes, in which case `new`
/// goes and `path` keeps its time: [`write_if_changed`] for an output written a piece at a time,
/// which is never whole in memory. Returns whether `path` changed.
pub fn replace_if_changed(new: &std::path::Path, path: &std::path::Path) -> std::io::Result<bool> {
    if same_files(new, path) {
        std::fs::remove_file(new)?;
        return Ok(false);
    }
    std::fs::rename(new, path)?;
    Ok(true)
}

/// Whether the files at `a` and `b` hold the same bytes, read a piece at a time.
fn same_files(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::io::Read;
    let (Ok(mut a), Ok(mut b)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    if !matches!((a.metadata(), b.metadata()), (Ok(x), Ok(y)) if x.len() == y.len()) {
        return false;
    }
    let (mut piece, mut other) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let n = match a.read(&mut piece) {
            Ok(0) => return b.read(&mut other).is_ok_and(|n| n == 0),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        };
        if b.read_exact(&mut other[..n]).is_err() || piece[..n] != other[..n] {
            return false;
        }
    }
}

/// Whether the file at `path` holds exactly `bytes`, read a piece at a time: the old text of a
/// large output is never in memory beside the new.
fn holds(path: &std::path::Path, bytes: &[u8]) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    if !file.metadata().is_ok_and(|m| m.len() == bytes.len() as u64) {
        return false;
    }
    let mut piece = vec![0u8; 1 << 16];
    let mut rest = bytes;
    loop {
        let n = match file.read(&mut piece) {
            Ok(0) => return rest.is_empty(),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        };
        if n > rest.len() || piece[..n] != rest[..n] {
            return false;
        }
        rest = &rest[n..];
    }
}
