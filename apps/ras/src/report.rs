//! The reports of the `ras` subcommands over a loaded image: a table, or JSON with `--json`.
//! An address prints as `0x` and at least eight hexadecimal digits, as the workbench reads it.

use std::io::Write;

use anyhow::Result;
use ra_asm_pe::{Directory, Image};
use serde::Serialize;
use serde_json::json;

/// `0x` and at least eight hexadecimal digits.
pub(crate) fn hex(va: u64) -> String {
    format!("0x{va:08x}")
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

fn print_warnings(image: &Image) {
    for warning in &image.warnings {
        eprintln!("warning: {warning}");
    }
}

/// The directories the image has: name, address and size.
fn directories_of(image: &Image) -> Vec<(&'static str, String, u32)> {
    let o = &image.headers.optional;
    [
        ("export", Directory::Export),
        ("import", Directory::Import),
        ("resource", Directory::Resource),
        ("exception", Directory::Exception),
        ("security", Directory::Security),
        ("base_reloc", Directory::BaseReloc),
        ("debug", Directory::Debug),
        ("tls", Directory::Tls),
        ("load_config", Directory::LoadConfig),
        ("iat", Directory::Iat),
        ("delay_import", Directory::DelayImport),
        ("clr_runtime", Directory::ClrRuntime),
    ]
    .into_iter()
    .filter_map(|(name, index)| {
        let d = o.directory(index);
        (!d.is_empty()).then(|| (name, hex(image.va(d.rva)), d.size))
    })
    .collect()
}

/// `pe32`, `pe32+` or `xbe`.
fn format_name(image: &Image) -> &'static str {
    if image.xbe.is_some() {
        "xbe"
    } else if image.is_64() {
        "pe32+"
    } else {
        "pe32"
    }
}

/// The headers of the image as JSON: what `ras info --json` prints and `ras serve` answers.
pub(crate) fn info_json(image: &Image) -> serde_json::Value {
    let h = &image.headers;
    let o = &h.optional;
    let directories = directories_of(image);
    json!({
            "path": image.path.as_ref().map(|p| p.display().to_string()),
            "machine": image.machine().name(),
            "format": format_name(image),
            "dll": h.coff.is_dll(),
            "xbe": image.xbe.as_ref().map(|x| json!({
                "kernel": x.kernel.name(),
                "title": x.title_name,
                "title_id": hex(u64::from(x.title_id)),
                "region": x.region,
                "libraries": x.libraries.iter().map(ra_asm_pe::xbe::LibraryVersion::display).collect::<Vec<_>>(),
            })),
            "subsystem": o.subsystem_name(),
            "image_base": hex(o.image_base),
            "entry_point": image.entry_point().map(hex),
            "size_of_image": o.size_of_image,
            "section_alignment": o.section_alignment,
            "file_alignment": o.file_alignment,
            "linker_version": format!("{}.{}", o.linker_version.0, o.linker_version.1),
            "os_version": format!("{}.{}", o.os_version.0, o.os_version.1),
            "subsystem_version": format!("{}.{}", o.subsystem_version.0, o.subsystem_version.1),
            "timestamp": h.coff.timestamp,
            "characteristics": h.coff.characteristics,
            "dll_characteristics": o.dll_characteristics,
            "sections": image.sections.len(),
            "imports": image.imports.len(),
            "exports": image.exports.as_ref().map_or(0, |e| e.exports.len()),
            "relocations": image.relocations.len(),
            "runtime_functions": image.runtime_functions.len(),
            "tls_callbacks": image.tls.as_ref().map_or(0, |t| t.callbacks.len()),
            "pdb": image.debug.iter().find_map(|d| d.codeview.as_ref()).map(|cv| cv.pdb_path.clone()),
            "directories": directories.iter().map(|(name, va, size)| json!({"name": name, "va": va, "size": size})).collect::<Vec<_>>(),
            "warnings": image.warnings,
    })
}

pub(crate) fn info(image: &Image, json: bool) -> Result<()> {
    if json {
        return print_json(&info_json(image));
    }
    let h = &image.headers;
    let o = &h.optional;
    let directories = directories_of(image);

    println!("machine        {}", image.machine().name());
    println!("format         {}{}", format_name(image).to_uppercase(), if h.coff.is_dll() { " (DLL)" } else { "" });
    if let Some(x) = &image.xbe {
        println!("title          {} ({:#010x}, region {:#x}, {} kernel)", x.title_name, x.title_id, x.region, x.kernel.name());
        let libraries: Vec<String> = x.libraries.iter().map(ra_asm_pe::xbe::LibraryVersion::display).collect();
        println!("libraries      {}", if libraries.is_empty() { "none".to_owned() } else { libraries.join(", ") });
    }
    println!("subsystem      {} ({}.{})", o.subsystem_name(), o.subsystem_version.0, o.subsystem_version.1);
    println!("image base     {}", hex(o.image_base));
    println!("entry point    {}", image.entry_point().map_or_else(|| "none".to_owned(), hex));
    println!("size of image  {:#x}", o.size_of_image);
    println!("alignment      section {:#x}, file {:#x}", o.section_alignment, o.file_alignment);
    println!("linker         {}.{}", o.linker_version.0, o.linker_version.1);
    println!("timestamp      {:#010x}", h.coff.timestamp);
    println!(
        "flags          characteristics {:#06x}, dll characteristics {:#06x}{}{}",
        h.coff.characteristics,
        o.dll_characteristics,
        if o.dll_characteristics & ra_asm_pe::headers::OptionalHeader::DYNAMIC_BASE != 0 { ", dynamic base" } else { "" },
        if h.coff.characteristics & ra_asm_pe::headers::CoffHeader::RELOCS_STRIPPED != 0 { ", relocs stripped" } else { "" },
    );
    if let Some(cv) = image.debug.iter().find_map(|d| d.codeview.as_ref()) {
        println!("pdb            {} ({}, age {})", cv.pdb_path, cv.guid, cv.age);
    }
    println!(
        "counts         {} sections, {} imports, {} exports, {} relocations, {} runtime functions, {} TLS callbacks",
        image.sections.len(),
        image.imports.len(),
        image.exports.as_ref().map_or(0, |e| e.exports.len()),
        image.relocations.len(),
        image.runtime_functions.len(),
        image.tls.as_ref().map_or(0, |t| t.callbacks.len()),
    );
    println!("directories");
    for (name, va, size) in &directories {
        println!("  {name:<14} {va}  {size:#x}");
    }
    print_warnings(image);
    Ok(())
}

pub(crate) fn sections(image: &Image, json: bool) -> Result<()> {
    if json {
        let rows: Vec<_> = image
            .sections
            .iter()
            .map(|s| {
                json!({
                    "name": s.name, "va": hex(image.va(s.virtual_address)), "virtual_size": s.virtual_size,
                    "raw_offset": s.raw_offset, "raw_size": s.raw_size, "flags": s.flags(), "characteristics": s.characteristics,
                })
            })
            .collect();
        return print_json(&rows);
    }
    println!("{:<10} {:<18} {:>10} {:>10} {:>10}  flags", "name", "va", "vsize", "raw off", "raw size");
    for s in &image.sections {
        println!(
            "{:<10} {:<18} {:>#10x} {:>#10x} {:>#10x}  {}",
            s.name,
            hex(image.va(s.virtual_address)),
            s.virtual_size,
            s.raw_offset,
            s.raw_size,
            s.flags()
        );
    }
    print_warnings(image);
    Ok(())
}

pub(crate) fn imports(image: &Image, json: bool) -> Result<()> {
    if json {
        let rows: Vec<_> = image
            .imports
            .iter()
            .map(
                |i| json!({"dll": i.dll, "name": i.name, "ordinal": i.ordinal, "hint": i.hint, "iat": hex(i.iat_va), "delayed": i.delayed}),
            )
            .collect();
        return print_json(&rows);
    }
    let mut last = "";
    for i in &image.imports {
        if i.dll != last {
            println!("{}{}", i.dll, if i.delayed { " (delay loaded)" } else { "" });
            last = &i.dll;
        }
        match (&i.name, i.ordinal) {
            (Some(name), _) => println!("  {}  {name} (hint {})", hex(i.iat_va), i.hint),
            (None, ordinal) => println!("  {}  #{}", hex(i.iat_va), ordinal.unwrap_or(0)),
        }
    }
    print_warnings(image);
    Ok(())
}

pub(crate) fn exports(image: &Image, json: bool) -> Result<()> {
    let Some(table) = &image.exports else {
        if json {
            return print_json(&json!({"dll_name": null, "exports": []}));
        }
        println!("no exports");
        return Ok(());
    };
    if json {
        let rows: Vec<_> = table
            .exports
            .iter()
            .map(|e| json!({"name": e.name, "ordinal": e.ordinal, "va": e.va.map(hex), "forwarder": e.forwarder}))
            .collect();
        return print_json(&json!({"dll_name": table.dll_name, "ordinal_base": table.ordinal_base, "exports": rows}));
    }
    println!("{} (ordinal base {})", table.dll_name, table.ordinal_base);
    for e in &table.exports {
        let name = e.name.as_deref().unwrap_or("(no name)");
        match (&e.forwarder, e.va) {
            (Some(forwarder), _) => println!("  #{:<5} {:<18} {name} -> {forwarder}", e.ordinal, "forwarder"),
            (None, Some(va)) => println!("  #{:<5} {:<18} {name}", e.ordinal, hex(va)),
            (None, None) => println!("  #{:<5} {:<18} {name}", e.ordinal, "?"),
        }
    }
    print_warnings(image);
    Ok(())
}

pub(crate) fn relocs(image: &Image, json: bool) -> Result<()> {
    if json {
        let rows: Vec<_> =
            image.relocations.iter().map(|r| json!({"va": hex(r.va), "kind": format!("{:?}", r.kind).to_lowercase()})).collect();
        return print_json(&rows);
    }
    for r in &image.relocations {
        let section = image.section_of(r.va).map_or("?", |s| s.name.as_str());
        println!("{}  {:<8} {section}", hex(r.va), format!("{:?}", r.kind).to_lowercase());
    }
    println!("{} relocations", image.relocations.len());
    print_warnings(image);
    Ok(())
}

pub(crate) fn debug(image: &Image, json: bool) -> Result<()> {
    if json {
        let rows: Vec<_> = image
            .debug
            .iter()
            .map(|d| {
                json!({
                    "kind": d.kind_name(), "kind_value": d.kind, "timestamp": d.timestamp, "size": d.size, "rva": d.rva, "file_offset": d.file_offset,
                    "codeview": d.codeview.as_ref().map(|cv| json!({"guid": cv.guid, "age": cv.age, "pdb_path": cv.pdb_path})),
                })
            })
            .collect();
        return print_json(&rows);
    }
    for d in &image.debug {
        println!("{:<24} {:#x} bytes at file offset {:#x}", d.kind_name(), d.size, d.file_offset);
        if let Some(cv) = &d.codeview {
            println!("  pdb {}  guid {}  age {}", cv.pdb_path, cv.guid, cv.age);
        }
    }
    if image.debug.is_empty() {
        println!("no debug directory");
    }
    print_warnings(image);
    Ok(())
}

/// The coverage of a lift as JSON: what `ras lift --report --json` prints and `ras functions
/// --json` carries as `coverage`.
pub(crate) fn coverage_json(coverage: &ra_asm_lift::Coverage) -> serde_json::Value {
    json!({
        "traps": coverage.traps(),
        "uncovered": coverage.uncovered.iter().map(|u| json!({"mnemonic": u.mnemonic, "count": u.count, "example": hex(u.example)})).collect::<Vec<_>>(),
        "functions": coverage.functions.iter().map(|(entry, name, traps)| json!({"addr": hex(*entry), "name": name, "traps": traps})).collect::<Vec<_>>(),
        "unknown_imports": coverage.unknown_imports,
        "unresolved_jumps": coverage.unresolved_jumps.iter().map(|(at, entry)| json!({"addr": hex(*at), "function": hex(*entry)})).collect::<Vec<_>>(),
    })
}

/// The coverage of a lift as four tables, each headed by its name; an empty table says `none`.
pub(crate) fn coverage(coverage: &ra_asm_lift::Coverage, json: bool) -> Result<()> {
    if json {
        return print_json(&coverage_json(coverage));
    }
    let mut out = std::io::stdout().lock();
    writeln!(out, "uncovered instructions ({} traps)", coverage.traps())?;
    if coverage.uncovered.is_empty() {
        writeln!(out, "  none")?;
    }
    for u in &coverage.uncovered {
        writeln!(out, "  {:<12} {:>6}  {}", u.mnemonic, u.count, hex(u.example))?;
    }
    writeln!(out, "functions by trap count ({})", coverage.functions.len())?;
    if coverage.functions.is_empty() {
        writeln!(out, "  none")?;
    }
    for (entry, name, traps) in &coverage.functions {
        writeln!(out, "  {:<18} {:>6}  {name}", hex(*entry), traps)?;
    }
    writeln!(out, "imports without a prototype ({})", coverage.unknown_imports.len())?;
    if coverage.unknown_imports.is_empty() {
        writeln!(out, "  none")?;
    }
    for name in &coverage.unknown_imports {
        writeln!(out, "  {name}")?;
    }
    writeln!(out, "unresolved indirect jumps ({})", coverage.unresolved_jumps.len())?;
    if coverage.unresolved_jumps.is_empty() {
        writeln!(out, "  none")?;
    }
    for (at, entry) in &coverage.unresolved_jumps {
        writeln!(out, "  {:<18} in {}", hex(*at), hex(*entry))?;
    }
    Ok(())
}

pub(crate) fn functions(image: &Image, pdata: bool, json: bool) -> Result<()> {
    if !pdata {
        anyhow::bail!("function discovery lands with milestone C; use --pdata for the exception table");
    }
    if json {
        let rows: Vec<_> = image
            .runtime_functions
            .iter()
            .map(|f| {
                let chained = f.unwind_info(image).ok().and_then(|u| u.chained).map(|p| hex(p.begin));
                json!({"begin": hex(f.begin), "end": hex(f.end), "size": f.len(), "unwind": hex(f.unwind), "chained_to": chained})
            })
            .collect();
        return print_json(&rows);
    }
    for f in &image.runtime_functions {
        let unwind = f.unwind_info(image);
        let note = match &unwind {
            Ok(u) if u.chained.is_some() => format!("  piece of {}", hex(u.chained.unwrap().begin)),
            Ok(u) if u.handler.is_some() => format!("  handler {}", hex(u.handler.unwrap())),
            Ok(_) => String::new(),
            Err(e) => format!("  unwind: {e:#}"),
        };
        println!("{}  {}  {:>6}{note}", hex(f.begin), hex(f.end), f.len());
    }
    println!("{} runtime functions", image.runtime_functions.len());
    print_warnings(image);
    Ok(())
}
