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
    assert_eq!(image.machine(), ra_asm_pe::Machine::Mips);
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

/// `testdata/mips/control`: the control fixture as a static mipsel executable linked with
/// `--emit-relocs` (see its source), whose figures are `llvm-readobj`'s.
fn mips_control() -> ra_asm_pe::Image {
    let image = ra_asm_pe::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mips/control")).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    image
}

#[test]
fn the_mips_control_is_a_little_endian_o32_image() {
    use ra_asm_format::elf::MipsAbi;
    let image = mips_control();
    assert_eq!(image.machine(), ra_asm_pe::Machine::Mips);
    assert_eq!(image.machine().name(), "mips");
    assert!(!image.big_endian() && !image.is_64());
    assert_eq!(image.pointer_size(), 4);
    assert_eq!((image.image_base(), image.entry_point()), (0x1_0000, Some(0x2_0200)));
    let start = image.symbols.iter().find(|s| s.name == "_start").unwrap();
    assert_eq!(image.entry_point(), Some(start.va));

    let elf = image.elf.as_ref().unwrap();
    assert_eq!(elf.flags, 0x1000_1001);
    let mips = elf.mips.as_ref().unwrap();
    assert_eq!((mips.abi, mips.abi.name(), mips.arch()), (MipsAbi::O32, "o32", Some("mips2")));
    assert!(mips.noreorder() && !mips.abicalls());
    let abiflags = mips.abiflags.unwrap();
    assert_eq!((abiflags.version, abiflags.isa_level, abiflags.isa_rev), (0, 2, 0));
    assert_eq!((abiflags.gpr_bits, abiflags.cpr1_bits, abiflags.cpr2_bits, abiflags.fp_abi), (32, 32, 0, 5));
    assert_eq!((abiflags.isa_ext, abiflags.ases, abiflags.flags1, abiflags.flags2), (0, 0, 0, 0));
    let reginfo = mips.reginfo.unwrap();
    assert_eq!((reginfo.gpr_mask, reginfo.cpr_mask, reginfo.gp), (0xb27f_0fff, [0; 4], 0x3_8850));
    assert_eq!(image.global_pointer, Some(0x3_8850));
}

#[test]
fn the_mips_control_sections_are_llvm_readobjs() {
    let image = mips_control();
    let sections: Vec<(&str, u64, u32, bool)> =
        image.sections.iter().map(|s| (s.name.as_str(), image.va(s.virtual_address), s.virtual_size, s.is_code())).collect();
    assert_eq!(
        sections,
        [
            (".MIPS.abiflags", 0x1_0118, 24, false),
            (".reginfo", 0x1_0130, 24, false),
            (".rodata", 0x1_0148, 184, false),
            (".text", 0x2_0200, 1580, true),
            (".data", 0x3_0830, 40, false),
            (".got", 0x3_0860, 8, false),
            (".bss", 0x3_0870, 12, false),
        ]
    );
    assert!(image.sections.iter().find(|s| s.name == ".bss").unwrap().is_uninitialized());
}

#[test]
fn the_mips_control_keeps_its_static_relocations() {
    use ra_asm_format::elf::reloc;
    let image = mips_control();
    let elf = image.elf.as_ref().unwrap();
    // `.rel.text` and `.rel.rodata` apply to mapped sections; `.rel.pdr` does not and is left out.
    let count = |kind: u32| elf.static_relocations.iter().filter(|r| r.kind == kind).count();
    assert_eq!(elf.static_relocations.len(), 131 + 23);
    assert_eq!((count(reloc::MIPS_32), count(reloc::MIPS_26)), (23, 53));
    let symbol = |r: &ra_asm_format::elf::Relocation| elf.symbols[r.symbol as usize].name.as_str();
    let first = elf.static_relocations[0];
    assert_eq!((first.offset, first.kind, symbol(&first)), (0x2_0200, reloc::MIPS_HI16, "_gp"));
    let call = elf.static_relocations[2];
    assert_eq!((call.offset, call.kind, symbol(&call)), (0x2_020c, reloc::MIPS_26, "control"));

    // The `lui`/`addiu` of `_start` build `_gp`; every pair's target lies in the image.
    let mips = elf.mips.as_ref().unwrap();
    assert_eq!(mips.pairs[0], ra_asm_format::elf::MipsPair { high: 0x2_0200, low: 0x2_0204, target: 0x3_8850 });
    let write = image.symbols.iter().find(|s| s.name == "write").unwrap().va;
    assert!(mips.pairs.iter().any(|p| (p.high, p.low, p.target) == (0x2_0270, 0x2_0274, write)), "{:x?}", mips.pairs);
    assert!(mips.pairs.iter().skip(1).all(|p| image.section_of(p.target).is_some()), "{:x?}", mips.pairs);

    // The words of `.rodata` that hold an address are its 32-bit relocations: the switch's table
    // and `ops` into `.text`, then the names.
    let words: Vec<u64> = image.relocations.iter().filter(|r| r.kind == ra_asm_pe::RelocKind::HighLow).map(|r| r.va).collect();
    assert_eq!(words, (0..23).map(|i| 0x1_0148 + 4 * i).collect::<Vec<_>>());
    for &va in &words[..13] {
        assert!(image.is_code(u64::from(image.u32(va).unwrap())), "{va:#x}");
    }
    assert_eq!(image.relocations.len(), elf.static_relocations.len());
}

/// The freestanding hello for `mips-linux-gnu`: the mipsel hello's big-endian twin, whose figures
/// are `llvm-readobj`'s.
fn mips_be_hello() -> ra_asm_pe::Image {
    let image = ra_asm_pe::load(&fixture("hello_mips")).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let elf = image.elf.as_ref().unwrap();
    assert!(!elf.wide && elf.big_endian && image.big_endian());
    assert_eq!(image.order(), ra_asm_format::ByteOrder::Big);
    assert_eq!(image.machine(), ra_asm_pe::Machine::Mips);
    assert_eq!(image.pointer_size(), 4);
    image
}

#[test]
fn mips_be_hello_enters_at_start() {
    let image = mips_be_hello();
    let elf = image.elf.as_ref().unwrap();
    assert_eq!((elf.kind, elf.machine, elf.flags), (ra_asm_format::elf::ET_EXEC, 8, 0x7000_1001));
    assert_eq!(image.entry_point(), Some(0x2_01c0));
    let start = image.symbols.iter().find(|s| s.name == "__start").unwrap();
    assert_eq!(image.entry_point(), Some(start.va));
    assert!(image.is_code(start.va));
    // `addiu $sp, $sp, -16`, the start routine's first word, read in the file's order.
    assert_eq!(image.u32(start.va).unwrap(), 0x27bd_fff0);
    assert!(image.imports.is_empty() && image.exports.is_none() && image.relocations.is_empty());
}

#[test]
fn mips_be_hello_symbols_are_llvm_readobjs() {
    let image = mips_be_hello();
    let symbols: Vec<(u64, &str)> = image.symbols.iter().map(|s| (s.va, s.name.as_str())).collect();
    assert_eq!(
        symbols,
        [
            (0x2_0150, "GetStdHandle"),
            (0x2_0164, "WriteFile"),
            (0x2_01a8, "ExitProcess"),
            (0x2_01c0, "__start"),
            (0x2_01d8, "fs_write"),
            (0x2_01f0, "fs_exit"),
            (0x2_0208, "square"),
            (0x2_0210, "main"),
        ]
    );
    let elf = image.elf.as_ref().unwrap();
    let sized: Vec<(&str, u64)> = elf.symbols.iter().filter(|s| s.is_function()).map(|s| (s.name.as_str(), s.size)).collect();
    assert!(sized.contains(&("main", 136)) && sized.contains(&("WriteFile", 68)), "{sized:?}");
    // `.reginfo`'s `$gp` reads big-endian too, and is `_gp`'s.
    assert_eq!(image.global_pointer, Some(0x3_8290));
}

#[test]
fn an_x86_elf_has_no_global_pointer() {
    let image = ra_asm_pe::load(&fixture("hello_linux64")).unwrap();
    assert_eq!(image.global_pointer, None);
    assert!(!image.big_endian());
}
