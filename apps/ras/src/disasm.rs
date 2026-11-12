//! `ras disasm`: the instructions at an address, with a note for each operand that points into the
//! image: an import slot, a string, or a data address.

use std::io::Write;

use anyhow::{Context, Result, bail};
use ra_asm_arch::{Flow, Instr, Operand, Target};
use ra_asm_pe::Image;
use serde_json::json;

use crate::report::hex;

/// Reads `0x401000`, `401000` or `F_00401000` as an address.
pub(crate) fn parse_address(text: &str) -> Result<u64> {
    let t = text.trim();
    let digits = t.strip_prefix("F_").unwrap_or(t);
    let digits = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")).unwrap_or(digits);
    u64::from_str_radix(digits, 16).with_context(|| format!("{text} is not an address"))
}

/// The byte range to disassemble: `length` when given, else the `.pdata` extent that holds the
/// address, else one straight run of code up to the first return, jump or trap.
fn range(image: &Image, address: u64, length: Option<u64>) -> (u64, Option<u64>) {
    if let Some(length) = length {
        return (address, Some(length));
    }
    if let Some(f) = image.runtime_functions.iter().find(|f| f.contains(address)) {
        return (address, Some(f.end - address));
    }
    (address, None)
}

/// The instructions decoded at `address`: the run, with its start and its bytes.
pub(crate) struct Decoded<'a> {
    pub start: u64,
    pub bytes: &'a [u8],
    pub instrs: Vec<Instr>,
}

/// Decodes the instructions at `address`: `length` bytes, the `.pdata` extent, or one run of code.
pub(crate) fn decode(image: &Image, address: u64, length: Option<u64>) -> Result<Decoded<'_>> {
    if !image.is_code(address) {
        bail!("{} lies in no code section", hex(address));
    }
    let arch = ra_asm_analysis::arch_of(image);
    let (start, len) = range(image, address, length);
    let bytes = match len {
        Some(len) => image.read(start, len as usize).with_context(|| format!("the {len:#x} bytes at {}", hex(start)))?,
        None => image.read_to_section_end(start)?,
    };
    let mut instrs = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let Some(mut i) = arch.decode(&bytes[offset..], start + offset as u64) else {
            eprintln!("warning: no instruction at {}", hex(start + offset as u64));
            break;
        };
        arch.link(&instrs, &mut i);
        offset += i.len as usize;
        let stop = len.is_none() && matches!(i.flow, Flow::Return | Flow::Jump(_) | Flow::Trap | Flow::Halt);
        instrs.push(i);
        if stop || offset >= 0x1000 && len.is_none() {
            break;
        }
    }
    Ok(Decoded { start, bytes, instrs })
}

/// The instructions as JSON rows: what `ras disasm --json` prints and `ras serve` answers.
pub(crate) fn disasm_json(image: &Image, decoded: &Decoded<'_>) -> serde_json::Value {
    let rows: Vec<_> = decoded
        .instrs
        .iter()
        .map(|i| {
            let raw = &decoded.bytes[(i.address - decoded.start) as usize..(i.end() - decoded.start) as usize];
            let raw = raw.iter().fold(String::with_capacity(raw.len() * 2), |mut s, b| {
                use std::fmt::Write as _;
                let _ = write!(s, "{b:02x}");
                s
            });
            json!({
                "address": hex(i.address), "len": i.len, "bytes": raw,
                "mnemonic": i.mnemonic, "text": i.text, "flow": flow_name(i.flow), "target": i.flow.direct_target().map(hex),
                "note": note(image, i),
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

/// The instructions as text, one per line, with a note where an operand points into the image.
pub(crate) fn disasm_text(image: &Image, decoded: &Decoded<'_>) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for i in &decoded.instrs {
        // A note that only repeats an address the text already shows says nothing.
        let note = note(image, i).filter(|n| !(n.starts_with("0x") && !n.contains(' ') && i.text.contains(n.as_str())));
        let note = note.map(|n| format!("   ; {n}")).unwrap_or_default();
        let _ = writeln!(out, "{}  {}{note}", hex(i.address), i.text);
    }
    out
}

pub(crate) fn disasm(image: &Image, address: u64, length: Option<u64>, json: bool) -> Result<()> {
    let decoded = decode(image, address, length)?;
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &disasm_json(image, &decoded))?;
        writeln!(out)?;
        return Ok(());
    }
    out.write_all(disasm_text(image, &decoded).as_bytes())?;
    Ok(())
}

fn flow_name(flow: Flow) -> &'static str {
    match flow {
        Flow::Next => "next",
        Flow::Jump(Target::Direct(_)) => "jump",
        Flow::Jump(Target::Indirect) => "jump_indirect",
        Flow::CondJump(_) => "cond_jump",
        Flow::Call(Target::Direct(_)) => "call",
        Flow::Call(Target::Indirect) => "call_indirect",
        Flow::Return => "return",
        Flow::Trap => "trap",
        Flow::Halt => "halt",
        Flow::Interrupt => "interrupt",
    }
}

/// What an operand points at: an import slot, a string, or an address inside the image.
pub(crate) fn note(image: &Image, i: &Instr) -> Option<String> {
    let mut candidates: Vec<u64> = Vec::new();
    for operand in &i.operands {
        match operand {
            Operand::Mem(m) => {
                if let Some(a) = m.absolute {
                    candidates.push(a);
                }
                // A 32-bit displacement with a base is an absolute address only when it maps: the
                // old pipeline read these off the relocations; here the image bounds are the test.
                if !image.is_64() && m.absolute.is_none() && m.disp > 0 && image.is_mapped(m.disp as u64) {
                    candidates.push(m.disp as u64);
                }
            }
            Operand::Imm { value, .. } if !image.is_64() => {
                let v = *value as u64 & 0xffff_ffff;
                if image.is_mapped(v) {
                    candidates.push(v);
                }
            }
            Operand::Imm { value, .. } if image.is_mapped(*value as u64) => candidates.push(*value as u64),
            _ => {}
        }
    }
    let target = candidates.into_iter().find(|&a| image.is_mapped(a))?;
    if let Some(import) = image.import_at(target) {
        return Some(format!("{} -> {}!{}", hex(target), import.dll, import.display_name()));
    }
    if let Some(s) = image.printable_cstr(target, 1, 80) {
        return Some(format!("{} {:?}", hex(target), s));
    }
    // A pointer-sized slot that holds an address: say where it points.
    if let Ok(slot) = image.pointer(target)
        && image.is_mapped(slot)
        && slot != 0
    {
        return Some(format!("{} -> {}", hex(target), hex(slot)));
    }
    Some(hex(target))
}
