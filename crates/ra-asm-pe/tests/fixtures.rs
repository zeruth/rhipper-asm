//! The loader against the fixtures under `testdata/bin`, and against `llvm-readobj` when it is on
//! the PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_pe::{Image, Machine, OptionalMagic, RelocKind};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

fn load(name: &str) -> Image {
    let image = ra_asm_pe::load(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    assert!(image.warnings.is_empty(), "{name} loaded with warnings: {:?}", image.warnings);
    image
}

#[test]
fn hello64_headers_and_sections() {
    let image = load("hello64.exe");
    assert_eq!(image.machine(), Machine::Amd64);
    assert_eq!(image.headers.optional.magic, OptionalMagic::Pe32Plus);
    assert_eq!(image.image_base(), 0x1_4000_0000);
    assert!(!image.headers.coff.is_dll());
    assert_eq!(image.headers.optional.subsystem_name(), "windows_cui");
    let names: Vec<_> = image.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, [".text", ".rdata", ".pdata"]);
    let entry = image.entry_point().unwrap();
    assert!(image.is_code(entry), "the entry point {entry:#x} lies in a code section");
    assert_eq!(image.section_of(entry).unwrap().name, ".text");
}

#[test]
fn hello64_imports_three_kernel32_functions() {
    let image = load("hello64.exe");
    let mut names: Vec<_> = image.imports.iter().map(|i| (i.dll.as_str(), i.name.as_deref().unwrap())).collect();
    names.sort_unstable();
    assert_eq!(names, [("KERNEL32.dll", "ExitProcess"), ("KERNEL32.dll", "GetStdHandle"), ("KERNEL32.dll", "WriteFile")]);
    // Every IAT slot lies in a data section and is the width of a pointer apart from the next.
    for import in &image.imports {
        assert_eq!(image.section_of(import.iat_va).unwrap().name, ".rdata");
        assert!(image.import_at(import.iat_va).is_some());
    }
}

#[test]
fn hello32_is_pe32_with_relocations() {
    let image = load("hello32.exe");
    assert_eq!(image.machine(), Machine::I386);
    assert_eq!(image.headers.optional.magic, OptionalMagic::Pe32);
    assert_eq!(image.pointer_size(), 4);
    assert_eq!(image.image_base(), 0x40_0000);
    assert!(!image.relocations.is_empty(), "a 32-bit image with dynamic base holds HIGHLOW relocations");
    assert!(image.relocations.iter().all(|r| r.kind == RelocKind::HighLow));
    // Each relocated field holds an address inside the image.
    for r in &image.relocations {
        let target = u64::from(image.u32(r.va).unwrap());
        assert!(image.is_mapped(target), "relocation at {:#x} points at {target:#x}, outside the image", r.va);
    }
    assert!(image.runtime_functions.is_empty(), "x86 has no exception table");
}

#[test]
fn control64_has_runtime_functions_for_its_code_and_a_pdb_record() {
    let image = load("control64.exe");
    // The leaf functions have no entry; the seven with a frame or a call do.
    assert!(image.runtime_functions.len() >= 6, "control has at least six functions with frames, found {}", image.runtime_functions.len());
    let mut last_end = 0;
    for f in &image.runtime_functions {
        assert!(f.begin >= last_end, "the exception table is sorted and the entries do not overlap");
        assert!(image.is_code(f.begin) && image.is_code(f.end - 1));
        let unwind = f.unwind_info(&image).unwrap();
        assert_eq!(unwind.version, 1);
        last_end = f.end;
    }
    let entry = image.entry_point().unwrap();
    assert!(image.runtime_functions.iter().any(|f| f.contains(entry)), "the entry point has an entry");
    let codeview = image.debug.iter().find_map(|d| d.codeview.as_ref()).expect("a CodeView record");
    assert_eq!(codeview.pdb_path, "control.pdb");
    assert_eq!(codeview.age, 1);
}

#[test]
fn control_reads_its_strings() {
    for name in ["control64.exe", "control32.exe"] {
        let image = load(name);
        let rdata = image.sections.iter().find(|s| s.name == ".rdata").unwrap();
        let bytes = image.read_to_section_end(image.va(rdata.virtual_address)).unwrap();
        let offset = bytes.windows(6).position(|w| w == b"fail\r\n").unwrap_or_else(|| panic!("{name} holds the fail string"));
        let va = image.va(rdata.virtual_address) + offset as u64;
        assert_eq!(image.cstr(va, 64).unwrap(), "fail\r\n");
        assert_eq!(image.printable_cstr(va, 4, 64), Some("fail\r\n"));
    }
}

#[test]
fn exports64_lists_names_an_ordinal_only_export_and_a_forwarder() {
    let image = load("exports64.dll");
    assert!(image.headers.coff.is_dll());
    let table = image.exports.as_ref().expect("an export table");
    assert_eq!(table.dll_name, "exports64.dll");
    let add = table.by_name("ra_add").expect("ra_add");
    assert!(image.is_code(add.va.unwrap()));
    let counter = table.by_name("ra_counter").expect("ra_counter");
    assert_eq!(image.u32(counter.va.unwrap()).unwrap(), 7);
    let tick = table.by_name("ra_tick").expect("ra_tick");
    assert_eq!(tick.forwarder.as_deref(), Some("KERNEL32.GetTickCount"));
    assert!(tick.va.is_none());
    let hidden = table.exports.iter().find(|e| e.ordinal == 20).expect("ordinal 20");
    assert!(hidden.name.is_none(), "ra_hidden is exported by ordinal only");
    assert!(image.is_code(hidden.va.unwrap()));
}

#[test]
fn a_read_outside_every_section_is_refused_with_the_address() {
    let image = load("hello64.exe");
    let gap = image.va(image.sections[0].end_rva()) + 0x100;
    let error = image.read(gap, 4).unwrap_err();
    assert!(format!("{error:#}").contains(&format!("{gap:#x}")), "{error:#}");
    assert!(image.read(image.image_base() - 1, 1).is_err());
    assert!(image.read(image.va(image.size_of_image()), 1).is_err());
}

#[test]
fn a_truncated_file_names_the_field() {
    let bytes = std::fs::read(fixture("hello64.exe")).unwrap();
    let error = Image::parse(bytes[..0x80].to_vec()).unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("past the end"), "{text}");
    let error = Image::parse(b"not a pe".to_vec()).unwrap_err();
    assert_eq!(format!("{error:#}"), "the DOS magic at offset 0x0 is 0x6f6e, not MZ");
}

// region The llvm-readobj oracle

fn readobj(name: &str) -> Option<String> {
    let output = Command::new("llvm-readobj").args(["--sections", "--coff-imports", "--coff-exports"]).arg(fixture(name)).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `llvm-readobj` prints `  Name: .text (2E 74 65 78 74 00 00 00)` and `  VirtualAddress: 0x1000` per section,
/// `  Symbol: ExitProcess (390)` per import, and `  Name: ra_add` with `  RVA: 0x1000` per export.
fn oracle_sections(text: &str) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    let mut name = None;
    let mut size = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name: ").filter(|rest| rest.contains('(')) {
            name = Some(rest.split(" (").next().unwrap().to_owned());
        } else if let Some(rest) = line.strip_prefix("VirtualSize: ") {
            size = u32::from_str_radix(rest.trim_start_matches("0x"), 16).ok();
        } else if let Some(rest) = line.strip_prefix("VirtualAddress: ")
            && let (Some(n), Some(s)) = (name.take(), size.take())
        {
            out.push((n, u32::from_str_radix(rest.trim_start_matches("0x"), 16).unwrap(), s));
        }
    }
    out
}

fn oracle_imports(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dll = String::new();
    let mut in_import = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "Import {" || trimmed == "DelayImport {" {
            in_import = true;
        } else if in_import && trimmed == "}" {
            in_import = false;
        } else if in_import {
            if let Some(rest) = trimmed.strip_prefix("Name: ") {
                rest.clone_into(&mut dll);
            } else if let Some(rest) = trimmed.strip_prefix("Symbol: ") {
                out.push((dll.clone(), rest.split(" (").next().unwrap().to_owned()));
            }
        }
    }
    out
}

#[test]
fn agrees_with_llvm_readobj() {
    let names = ["hello64.exe", "hello32.exe", "control64.exe", "control32.exe", "exports64.dll", "exports32.dll"];
    let mut compared = 0;
    for name in names {
        let Some(text) = readobj(name) else {
            eprintln!("llvm-readobj is not on the PATH: the oracle comparison is skipped");
            return;
        };
        let image = load(name);
        let ours: Vec<_> = image.sections.iter().map(|s| (s.name.clone(), s.virtual_address, s.virtual_size)).collect();
        assert_eq!(ours, oracle_sections(&text), "{name}: sections");
        let mut ours: Vec<_> = image.imports.iter().map(|i| (i.dll.clone(), i.display_name())).collect();
        let mut theirs = oracle_imports(&text);
        ours.sort();
        theirs.sort();
        assert_eq!(ours, theirs, "{name}: imports");
        compared += 1;
    }
    assert_eq!(compared, names.len());
}

// endregion
