//! `ras signatures`: what each function takes and returns, as the lift infers it.

use std::io::Write;

use anyhow::Result;
use ra_asm_lift::raise::abi::{Kind, infer};
use ra_asm_pe::Image;
use serde_json::json;

use crate::report::hex;

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
    let signatures = infer(&lifted.module, &lifted.roots, ra_asm_lift::raise::Target { bits });
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
