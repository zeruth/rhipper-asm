//! The ELF container over the Linux fixture: clang's hello for `x86_64-linux-gnu`, linked by lld
//! against a stub `libc.so.6` (see `testdata/build.ps1`), mapped as the same image a PE makes.

use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

#[test]
fn the_linux_hello_maps_as_an_image() {
    let image = ra_asm_pe::load(&fixture("hello_linux64")).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let elf = image.elf.as_ref().unwrap();
    assert!(elf.wide && !elf.big_endian);
    assert_eq!(elf.interpreter.as_deref(), Some("/lib64/ld-linux-x86-64.so.2"));
    assert_eq!(elf.needed, ["libc.so.6"]);
    assert_eq!(image.machine(), ra_asm_pe::Machine::Amd64);
    assert!(image.is_64());
    assert_eq!(image.image_base(), 0x20_0000);

    // The allocated sections, at their own addresses, with the code where the segments put it.
    let names: Vec<&str> = image.sections.iter().map(|s| s.name.as_str()).collect();
    for name in [".dynsym", ".dynstr", ".rela.plt", ".eh_frame", ".text", ".plt", ".dynamic", ".got.plt"] {
        assert!(names.contains(&name), "no {name} in {names:?}");
    }
    assert!(!names.contains(&".symtab") && !names.contains(&".comment"), "a section the process does not map: {names:?}");
    let text = image.sections.iter().find(|s| s.name == ".text").unwrap();
    assert!(text.is_code());

    // The symbol table names the functions; the entry is `_start`.
    let symbols: Vec<&str> = image.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbols.contains(&"square") && symbols.contains(&"_start"), "{symbols:?}");
    let start = image.symbols.iter().find(|s| s.name == "_start").unwrap();
    assert_eq!(image.entry_point(), Some(start.va));
    assert!(image.is_code(start.va));
    // `_start` begins `push rax`: the mapped bytes are the file's.
    assert_eq!(image.u8(start.va).unwrap(), 0x50);

    // The PLT's slots are the imports, from the one library the program needs.
    let imports: Vec<(&str, &str)> = image.imports.iter().map(|i| (i.dll.as_str(), i.name.as_deref().unwrap())).collect();
    assert_eq!(imports, [("libc.so.6", "write"), ("libc.so.6", "exit")]);
    for i in &image.imports {
        assert_eq!(image.section_of(i.iat_va).map(|s| s.name.as_str()), Some(".got.plt"));
    }
    // An executable exports nothing; a fixed one has no address to relocate.
    assert!(image.exports.is_none());
    assert!(image.relocations.is_empty());

    // `.eh_frame` records the two functions the compiler kept, `square` and `_start`.
    for name in ["square", "_start"] {
        let va = image.symbols.iter().find(|s| s.name == name).unwrap().va;
        assert!(elf.functions.iter().any(|&(begin, end)| begin == va && end > begin), "{name} at {va:#x}: {:x?}", elf.functions);
    }
}

/// The freestanding hello for `mipsel-linux-gnu`, static and linked by lld (see
/// `testdata/src/freestanding`): a 32-bit little-endian MIPS file with no dynamic section.
fn mipsel_hello() -> ra_asm_pe::Image {
    let image = ra_asm_pe::load(&fixture("hello_mipsel")).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let elf = image.elf.as_ref().unwrap();
    assert!(!elf.wide && !elf.big_endian && !image.big_endian());
    assert_eq!(image.machine(), ra_asm_pe::Machine::Other(8));
    assert!(!image.is_64());
    assert_eq!(image.pointer_size(), 4);
    image
}

#[test]
fn the_mipsel_hello_enters_at_start() {
    let image = mipsel_hello();
    assert_eq!(image.image_base(), 0x1_0000);
    assert_eq!(image.entry_point(), Some(0x2_01c0));
    let start = image.symbols.iter().find(|s| s.name == "__start").unwrap();
    assert_eq!(image.entry_point(), Some(start.va));
    assert!(image.is_code(start.va));
    let symbols: Vec<&str> = image.symbols.iter().map(|s| s.name.as_str()).collect();
    for name in ["main", "square", "fs_write", "fs_exit"] {
        assert!(symbols.contains(&name), "no {name} in {symbols:?}");
    }
    // A static file binds nothing and relocates nothing.
    assert!(image.imports.is_empty() && image.exports.is_none() && image.relocations.is_empty());
}

#[test]
fn the_mipsel_hello_text_is_its_code_segment() {
    let image = mipsel_hello();
    let text = image.sections.iter().find(|s| s.name == ".text").unwrap();
    assert!(text.is_code());
    assert_eq!((image.va(text.virtual_address), image.va(text.end_rva())), (0x2_0150, 0x2_0298));
    let elf = image.elf.as_ref().unwrap();
    let segment = elf.segments.iter().find(|s| s.is_load() && s.flags & ra_asm_format::elf::PF_X != 0).unwrap();
    assert_eq!((segment.vaddr, segment.vaddr + segment.memsz), (0x2_0150, 0x2_0298));
    // The other loaded sections are data, not code.
    for name in [".reginfo", ".got"] {
        let section = image.sections.iter().find(|s| s.name == name).unwrap();
        assert!(!section.is_code(), "{name}");
    }
}

#[test]
fn the_mipsel_hello_global_pointer_is_its_gp() {
    let image = mipsel_hello();
    // lld puts `_gp` 0x7ff0 past the start of `.got`, and `.reginfo` records the same value.
    let got = image.sections.iter().find(|s| s.name == ".got").unwrap();
    assert_eq!(image.va(got.virtual_address) + 0x7ff0, 0x3_8290);
    assert_eq!(image.global_pointer, Some(0x3_8290));
    let gp = image.elf.as_ref().unwrap().symbols.iter().find(|s| s.name == "_gp").unwrap();
    assert_eq!(image.global_pointer, Some(gp.value));
}

#[test]
fn an_x86_elf_has_no_global_pointer() {
    let image = ra_asm_pe::load(&fixture("hello_linux64")).unwrap();
    assert_eq!(image.global_pointer, None);
    assert!(!image.big_endian());
}
