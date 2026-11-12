//! `ras signatures`: what each function takes and returns, as the lift infers it; with `--from`,
//! the library recognition: which functions the signatures of the given libraries name, and the
//! collisions among them. [`apply_files`] is the same recognition for every other command, from
//! `--signatures` or the project's `signatures` key.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ra_asm_analysis::signatures::{Applied, SignatureSet};
use ra_asm_lift::raise::abi::{Kind, infer};
use ra_asm_pe::Image;
use serde_json::json;

use crate::report::hex;

/// Whether a path is a library or an object the signatures are made from, rather than a
/// signature file.
fn is_library(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "lib" | "a" | "obj" | "o"))
}

/// The signatures of `files`: each a signature file, or a library made into one.
pub(crate) fn read_files(files: &[PathBuf]) -> Result<SignatureSet> {
    let mut set = SignatureSet::default();
    for file in files {
        let read = if is_library(file) { SignatureSet::from_library(file) } else { SignatureSet::read(file) };
        set.extend(read.with_context(|| format!("signatures from {}", file.display()))?);
    }
    Ok(set)
}

/// Applies the signatures of `files` to `image`, so the analysis names what they match.
pub(crate) fn apply_files(image: &mut Image, files: &[PathBuf]) -> Result<Applied> {
    let set = read_files(files)?;
    Ok(ra_asm_analysis::signatures::apply(image, &set))
}

/// `ras signatures <pe> --from <files> [--write <file>]`: the matches and the collisions of the
/// signatures over the image; `--write` keeps the signatures made from the libraries.
pub(crate) fn recognize(image: &mut Image, from: &[PathBuf], write: Option<&Path>, json: bool) -> Result<()> {
    let set = read_files(from)?;
    if let Some(path) = write {
        set.write(path)?;
    }
    let applied = ra_asm_analysis::signatures::apply(image, &set);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(
            &mut out,
            &json!({
                "signatures": set.signatures.len(), "functions": applied.functions,
                "matches": applied.matches, "collisions": applied.collisions().count(), "callees": applied.callees,
            }),
        )?;
        writeln!(out)?;
        return Ok(());
    }
    for m in &applied.matches {
        write!(out, "{}  {}  {}:{}", hex(m.va), m.name, m.library, m.object)?;
        if !m.alternatives.is_empty() {
            write!(out, "  collision: {}", m.alternatives.join(" | "))?;
        }
        writeln!(out)?;
    }
    for c in &applied.callees {
        writeln!(
            out,
            "{}  {}  named by a match's reference{}",
            hex(c.va),
            c.name,
            c.library.as_deref().map(|l| format!(" ({l})")).unwrap_or_default()
        )?;
    }
    writeln!(
        out,
        "{} signatures, {} functions, {} matched, {} collisions, {} named by reference",
        set.signatures.len(),
        applied.functions,
        applied.matches.len(),
        applied.collisions().count(),
        applied.callees.len()
    )?;
    Ok(())
}

fn kind_name(kind: Kind, bits: u32) -> &'static str {
    match kind {
        Kind::Int if bits == 32 => "i32",
        Kind::Int | Kind::Wide => "i64",
        Kind::Float => "float",
        Kind::Double => "double",
    }
}

pub(crate) fn signatures(image: &Image, json: bool) -> Result<()> {
    let analysis = ra_asm_analysis::analyze(image);
    let lifted = ra_asm_lift::lift(image, &analysis);
    let bits = lifted.bits;
    let kind_name = |k: Kind| kind_name(k, bits);
    let signatures = infer(&lifted.module, &lifted.roots, ra_asm_lift::raise::Target { bits, isa: lifted.isa }, &lifted.vtables);
    let mut out = std::io::stdout().lock();
    if json {
        let rows: Vec<_> = analysis
            .functions
            .values()
            .filter_map(|f| {
                let s = signatures.get(&ra_asm_lift::function_name(f.entry))?;
                Some(json!({
                    "addr": hex(f.entry), "name": f.name,
                    "params": s.params.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(),
                    "stack_params": s.stack_params.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(),
                    "returns": s.returns.map(kind_name), "noreturn": s.noreturn, "frame_below": s.frame_below, "dynamic_stack": s.dynamic_stack,
                    "pops": s.pops,
                }))
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &rows)?;
        writeln!(out)?;
        return Ok(());
    }
    for f in analysis.functions.values() {
        let Some(s) = signatures.get(&ra_asm_lift::function_name(f.entry)) else { continue };
        let params: Vec<&str> = s.params.iter().chain(&s.stack_params).map(|k| kind_name(*k)).collect();
        let ret = if s.noreturn { "noreturn" } else { s.returns.map_or("void", kind_name) };
        let pops = if s.pops > 0 { format!("  pops {}", s.pops) } else { String::new() };
        writeln!(
            out,
            "{}  {ret} {}({}){pops}{}",
            hex(f.entry),
            f.name,
            params.join(", "),
            if s.dynamic_stack { "  dynamic stack" } else { "" }
        )?;
        if s.frame_below > 0 {
            writeln!(out, "{:18}  frame {} bytes", "", s.frame_below)?;
        }
    }
    Ok(())
}
