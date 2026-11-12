//! The XBE loader against the fixture under `testdata/bin`, and the writer that makes the fixture:
//! `xbe32.xbe` is the PE clang builds from `testdata/src/xbe.c` (at the Xbox's base, without
//! relocations) rewritten as the XDK's image builder would write it, by the ignored
//! `write_xbe32_fixture`, which `testdata/build.ps1` runs after leaving the PE at `out/xbe32.exe`.

use std::path::{Path, PathBuf};

use ra_asm_pe::xbe::{self, Kernel};
use ra_asm_pe::{Directory, Image, Machine, OptionalMagic};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// The retail kernel's keys, as the writer encodes with them.
const RETAIL_ENTRY_KEY: u32 = 0xA8FC_57AB;
const RETAIL_THUNK_KEY: u32 = 0x5B6D_40B6;

/// The XBE section flags.
const WRITABLE: u32 = 0x1;
const PRELOAD: u32 = 0x2;
const EXECUTABLE: u32 = 0x4;

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    fn put_u32(&mut self, at: usize, v: u32) {
        self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn zeros(&mut self, n: usize) {
        self.bytes.resize(self.bytes.len() + n, 0);
    }

    fn align(&mut self, to: usize) {
        let len = self.bytes.len().div_ceil(to) * to;
        self.bytes.resize(len, 0);
    }

    fn cstr(&mut self, s: &str) -> usize {
        let at = self.bytes.len();
        self.bytes.extend_from_slice(s.as_bytes());
        self.bytes.push(0);
        at
    }

    fn wstr(&mut self, s: &str, units: usize) {
        let mut n = 0;
        for u in s.encode_utf16().take(units - 1) {
            self.u16(u);
            n += 1;
        }
        for _ in n..units {
            self.u16(0);
        }
    }
}

/// The PE at the Xbox's base as an XBE: the header, the certificate, the section headers and
/// names, two library versions, the debug names, then each section's raw data on its own page.
/// The kernel thunk table is the `.kthunk` section; the keys are the retail kernel's.
fn write_xbe(pe: &Image, pe_name: &str) -> Vec<u8> {
    let base = u32::try_from(pe.image_base()).unwrap();
    let o = &pe.headers.optional;
    let thunks = pe.sections.iter().find(|s| s.name == ".kthunk").expect("the PE keeps its kernel thunk table in .kthunk");
    let tls = o.directory(Directory::Tls);
    let mut w = Writer { bytes: Vec::new() };
    w.zeros(0x178);
    let certificate = w.bytes.len();
    w.u32(0x1ec);
    w.u32(pe.headers.coff.timestamp);
    w.u32(0x5241_0001);
    w.wstr("Rhipper XBE fixture", 40);
    w.zeros(16 * 4);
    w.u32(0x0000_0006); // allowed media: a hard disk, a DVD-X2
    w.u32(0x0000_0007); // every region
    w.u32(0);
    w.u32(0);
    w.u32(1); // version
    w.zeros(0x1ec - (w.bytes.len() - certificate));
    w.align(4);
    let section_headers = w.bytes.len();
    w.zeros(pe.sections.len() * 0x38);
    let counts = w.bytes.len();
    w.zeros((pe.sections.len() + 1) * 2);
    w.align(4);
    let names: Vec<usize> = pe.sections.iter().map(|s| w.cstr(&s.name)).collect();
    w.align(4);
    let libraries = w.bytes.len();
    for (name, build) in [("XAPILIB", 5849u16), ("XBOXKRNL", 5849)] {
        let mut bytes = [0u8; 8];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        w.bytes.extend_from_slice(&bytes);
        w.u16(1);
        w.u16(0);
        w.u16(build);
        w.u16(0x4001); // approved, the debug flag clear
    }
    let debug_path = w.cstr(&format!("D:\\rhipper\\{pe_name}"));
    let debug_name = debug_path + "D:\\rhipper\\".len();
    w.align(2);
    let debug_wide = w.bytes.len();
    w.wstr(pe_name, pe_name.len() + 1);
    w.align(0x1000);
    let size_of_headers = w.bytes.len();

    // The raw data, each section on a page of its own, as the sections were in the PE.
    let mut raw = Vec::new();
    let file = pe.file();
    for (i, s) in pe.sections.iter().enumerate() {
        let start = s.raw_offset as usize;
        let len = (s.raw_size.min(s.mapped_size()) as usize).min(file.len().saturating_sub(start));
        let at = size_of_headers + raw.len();
        raw.extend_from_slice(&file[start..start + len]);
        raw.resize(raw.len().div_ceil(0x1000) * 0x1000, 0);
        let mut flags = PRELOAD;
        if s.is_writable() {
            flags |= WRITABLE;
        }
        if s.is_executable() || s.is_code() {
            flags |= EXECUTABLE;
        }
        let h = section_headers + i * 0x38;
        w.put_u32(h, flags);
        w.put_u32(h + 0x04, base + s.virtual_address);
        w.put_u32(h + 0x08, s.mapped_size());
        w.put_u32(h + 0x0c, at as u32);
        w.put_u32(h + 0x10, len as u32);
        w.put_u32(h + 0x14, base + names[i] as u32);
        w.put_u32(h + 0x18, 1);
        w.put_u32(h + 0x1c, base + (counts + 2 * i) as u32);
        w.put_u32(h + 0x20, base + (counts + 2 * i + 2) as u32);
    }

    let header_address = |offset: usize| base + offset as u32;
    w.put_u32(0x000, xbe::MAGIC);
    w.put_u32(0x104, base);
    w.put_u32(0x108, size_of_headers as u32);
    w.put_u32(0x10c, o.size_of_image);
    w.put_u32(0x110, 0x178);
    w.put_u32(0x114, pe.headers.coff.timestamp);
    w.put_u32(0x118, header_address(certificate));
    w.put_u32(0x11c, pe.sections.len() as u32);
    w.put_u32(0x120, header_address(section_headers));
    w.put_u32(0x124, 0);
    w.put_u32(0x128, (base + o.entry_point) ^ RETAIL_ENTRY_KEY);
    w.put_u32(0x12c, if tls.is_empty() { 0 } else { base + tls.rva });
    w.put_u32(0x130, o.stack_commit as u32);
    w.put_u32(0x134, o.heap_reserve as u32);
    w.put_u32(0x138, o.heap_commit as u32);
    w.put_u32(0x13c, base);
    w.put_u32(0x140, o.size_of_image);
    w.put_u32(0x144, o.checksum);
    w.put_u32(0x148, pe.headers.coff.timestamp);
    w.put_u32(0x14c, header_address(debug_path));
    w.put_u32(0x150, header_address(debug_name));
    w.put_u32(0x154, header_address(debug_wide));
    w.put_u32(0x158, (base + thunks.virtual_address) ^ RETAIL_THUNK_KEY);
    w.put_u32(0x15c, 0);
    w.put_u32(0x160, 2);
    w.put_u32(0x164, header_address(libraries));
    w.put_u32(0x168, header_address(libraries + 0x10));
    w.put_u32(0x16c, header_address(libraries));
    w.put_u32(0x170, 0);
    w.put_u32(0x174, 0);
    w.bytes.extend_from_slice(&raw);
    w.bytes
}

/// Writes `testdata/bin/xbe32.xbe` from the PE `testdata/build.ps1` leaves at `out/xbe32.exe`
/// (or where `RA_XBE_PE` points).
#[test]
#[ignore = "writes the fixture from the PE clang built; testdata/build.ps1 runs it"]
fn write_xbe32_fixture() {
    let pe_path = std::env::var("RA_XBE_PE").map_or_else(|_| fixture("../../out/xbe32.exe"), PathBuf::from);
    let pe = Image::parse(std::fs::read(&pe_path).unwrap()).unwrap();
    assert_eq!(pe.image_base(), 0x10000, "the PE is linked at the Xbox's base");
    assert!(pe.relocations.is_empty(), "the PE is linked /fixed");
    let bytes = write_xbe(&pe, pe_path.file_name().unwrap().to_str().unwrap());
    std::fs::write(fixture("xbe32.xbe"), bytes).unwrap();
}

fn load() -> Image {
    let image = ra_asm_pe::load(&fixture("xbe32.xbe")).unwrap_or_else(|e| panic!("xbe32.xbe: {e:#}"));
    assert!(image.warnings.is_empty(), "xbe32.xbe loaded with warnings: {:?}", image.warnings);
    image
}

#[test]
fn xbe32_loads_as_a_32_bit_x86_image_at_the_xbox_base() {
    let image = load();
    assert_eq!(image.machine(), Machine::I386);
    assert_eq!(image.headers.optional.magic, OptionalMagic::Pe32);
    assert_eq!(image.pointer_size(), 4);
    assert_eq!(image.image_base(), 0x10000);
    assert_eq!(image.headers.optional.subsystem_name(), "xbox");
    let xbe = image.xbe.as_ref().expect("the image is an XBE");
    assert_eq!(xbe.kernel, Kernel::Retail);
    assert_eq!(xbe.title_name, "Rhipper XBE fixture");
    assert_eq!(xbe.title_id, 0x5241_0001);
    assert_eq!(
        xbe.libraries.iter().map(ra_asm_pe::xbe::LibraryVersion::display).collect::<Vec<_>>(),
        ["XAPILIB 1.0.5849", "XBOXKRNL 1.0.5849"]
    );
    assert_eq!(xbe.debug_path.as_deref(), Some("D:\\rhipper\\xbe32.exe"));
    let entry = image.entry_point().unwrap();
    assert_eq!(entry, xbe.entry_va);
    assert!(image.is_code(entry), "the entry point {entry:#x} lies in a code section");
    assert_eq!(image.section_of(entry).unwrap().name, ".text");
    let names: Vec<_> = image.sections.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&".text") && names.contains(&".kthunk"), "{names:?}");
    assert!(image.sections.iter().all(|s| s.is_readable()));
    assert!(image.section_of_rva(0).is_none() && image.is_mapped(image.image_base()), "the header is mapped at the base");
    // The map beside the fixture is the PE's: the same sections, the same addresses.
    assert!(image.symbols.iter().any(|s| s.name == "_XbeEntry"), "{:?}", image.symbols);
    assert!(image.symbols.iter().any(|s| s.name == "_fib"), "{:?}", image.symbols);
}

#[test]
fn xbe32_imports_the_kernel_by_ordinal_with_names_from_the_table() {
    let image = load();
    let imports: Vec<_> = image.imports.iter().map(|i| (i.dll.as_str(), i.ordinal.unwrap(), i.name.as_deref().unwrap())).collect();
    assert_eq!(
        imports,
        [
            ("xboxkrnl.exe", 8, "DbgPrint"),
            ("xboxkrnl.exe", 14, "ExAllocatePool"),
            ("xboxkrnl.exe", 17, "ExFreePool"),
            ("xboxkrnl.exe", 49, "HalReturnToFirmware")
        ]
    );
    let xbe = image.xbe.as_ref().unwrap();
    for (i, import) in image.imports.iter().enumerate() {
        assert_eq!(import.iat_va, xbe.kernel_thunk_va + 4 * i as u64);
        assert_eq!(image.section_of(import.iat_va).unwrap().name, ".kthunk");
        assert_eq!(image.u32(import.iat_va).unwrap(), 0x8000_0000 | u32::from(import.ordinal.unwrap()));
        assert_eq!(import.display_name(), import.name.clone().unwrap());
    }
    assert!(image.tls.is_none(), "the fixture has no TLS");
}

#[test]
fn a_header_encoded_under_no_known_key_is_refused_with_its_offset() {
    let mut bytes = std::fs::read(fixture("xbe32.xbe")).unwrap();
    bytes[0x128..0x12c].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    let error = format!("{:#}", Image::parse(bytes).unwrap_err());
    assert!(error.contains("0x128") && error.contains("no known kernel"), "{error}");
}
