//! The backends: source out of the raised IR. The C backend is first; C++ and Rust share its
//! structuring.
//!
//! The IR arrives at the ABI level: functions with parameters and results, locals in allocas, data
//! in globals, conditions as comparisons. The structuring pass turns each function's graph into
//! statements (`if`, `else`, loops, `switch`, with `goto` only where the graph is not reducible),
//! and the printer writes them in the language's syntax, inlining a value into its one use where
//! that keeps the meaning. The function table says where each function landed, for the workbench.

pub mod c;
pub mod functions;
pub mod structure;

pub use c::{Dialect, EmittedFile, EmittedFunction, FUNCTIONS_PER_FILE, Options as COptions, Unit, emit_c, emit_unit};
pub use functions::{FunctionInfo, format_address, table};

/// Writes the files of `unit` into `dir`, after removing the files of an earlier, larger output
/// or another dialect (`program.c`, `program.h`, `data.c`, `program_NNN.c`, and the same as
/// `.cpp`), so a stale part never builds again.
pub fn write_files(dir: &std::path::Path, files: &[EmittedFile]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (stem, extension) = name.rsplit_once('.').unwrap_or((name.as_str(), ""));
            let source = extension == "c" || extension == "cpp";
            let part = stem.strip_prefix("program_").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let ours = name == "program.h" || (source && (stem == "program" || stem == "data" || part));
            if ours && !files.iter().any(|f| f.name == name) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    for f in files {
        std::fs::write(dir.join(&f.name), &f.text)?;
    }
    Ok(())
}
