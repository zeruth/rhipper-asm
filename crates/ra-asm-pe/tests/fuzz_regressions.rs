//! The cases the first run of the loaders' fuzz targets (`fuzz/`) found, each as the smallest
//! file that shows it: a field the file controls once overflowed a sum, sized an allocation or
//! multiplied a walk. Each is now a refusal that names where it read, or a warning of the image.

use ra_asm_pe::{Directory, Image, object, resources};

const BASE: u64 = 0x1_4000_0000;
const OPTIONAL: usize = 0x58;
/// The one section of the image: 0x2000 bytes at RVA 0x1000, from file offset 0x200.
const SECTION_RVA: u32 = 0x1000;
const SECTION_SIZE: u32 = 0x2000;
const RAW_OFFSET: usize = 0x200;
const SUBDIRECTORY: u32 = 0x8000_0000;

/// A PE32+ image with one data section and no directories, which a case fills.
struct Pe {
    bytes: Vec<u8>,
}

impl Pe {
    fn new() -> Self {
        let mut pe = Self { bytes: vec![0; RAW_OFFSET + SECTION_SIZE as usize] };
        pe.put(0, b"MZ");
        pe.put_u32(0x3c, 0x40);
        pe.put(0x40, b"PE\0\0");
        pe.put_u16(0x44, 0x8664); // Machine
        pe.put_u16(0x46, 1); // NumberOfSections
        pe.put_u16(0x54, 112 + 16 * 8); // SizeOfOptionalHeader
        pe.put_u16(0x56, 0x22); // Characteristics
        pe.put_u16(OPTIONAL, 0x20b);
        pe.put_u64(OPTIONAL + 24, BASE);
        pe.put_u32(OPTIONAL + 32, 0x1000); // SectionAlignment
        pe.put_u32(OPTIONAL + 36, 0x200); // FileAlignment
        pe.put_u32(OPTIONAL + 56, SECTION_RVA + SECTION_SIZE); // SizeOfImage
        pe.put_u32(OPTIONAL + 60, RAW_OFFSET as u32); // SizeOfHeaders
        pe.put_u32(OPTIONAL + 108, 16); // NumberOfRvaAndSizes
        let section = OPTIONAL + 112 + 16 * 8;
        pe.put(section, b".data\0\0\0");
        pe.put_u32(section + 8, SECTION_SIZE);
        pe.put_u32(section + 12, SECTION_RVA);
        pe.put_u32(section + 16, SECTION_SIZE);
        pe.put_u32(section + 20, RAW_OFFSET as u32);
        pe.put_u32(section + 36, 0xc000_0040);
        pe
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn put_u16(&mut self, at: usize, v: u16) {
        self.put(at, &v.to_le_bytes());
    }

    fn put_u32(&mut self, at: usize, v: u32) {
        self.put(at, &v.to_le_bytes());
    }

    fn put_u64(&mut self, at: usize, v: u64) {
        self.put(at, &v.to_le_bytes());
    }

    /// Writes `bytes` at `rva`, inside the section.
    fn data(&mut self, rva: u32, bytes: &[u8]) {
        self.put(RAW_OFFSET + (rva - SECTION_RVA) as usize, bytes);
    }

    fn words(&mut self, rva: u32, words: &[u32]) {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.data(rva, &bytes);
    }

    fn directory(&mut self, directory: Directory, rva: u32, size: u32) {
        let at = OPTIONAL + 112 + 8 * directory as usize;
        self.put_u32(at, rva);
        self.put_u32(at + 4, size);
    }

    /// A resource directory at `offset` of the tree (at RVA 0x1000) whose `count` id entries,
    /// numbered from `first`, all name `data`.
    fn resource_directory(&mut self, offset: u32, first: u32, count: u16, data: u32) {
        let table = SECTION_RVA + offset;
        self.data(table + 14, &count.to_le_bytes());
        for i in 0..u32::from(count) {
            self.words(table + 16 + i * 8, &[first + i, data]);
        }
    }

    fn parse(self) -> Image {
        Image::parse(self.bytes).expect("the image parses")
    }
}

/// The warning the image keeps for `directory`.
fn warning<'a>(image: &'a Image, directory: &str) -> &'a str {
    let prefix = format!("{directory}: ");
    image.warnings.iter().find(|w| w.starts_with(&prefix)).unwrap_or_else(|| panic!("no {directory} warning in {:?}", image.warnings))
}

#[test]
fn the_minimal_image_parses_without_a_warning() {
    let image = Pe::new().parse();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    assert_eq!(image.sections.len(), 1);
}

#[test]
fn an_image_base_at_the_top_of_the_space_is_refused() {
    let mut pe = Pe::new();
    pe.put_u64(OPTIONAL + 24, 0xffff_ffff_ffff_0000);
    pe.words(0x1000, &[0x1000, 10, 0x3000]);
    pe.directory(Directory::BaseReloc, 0x1000, 10);
    let error = Image::parse(pe.bytes).unwrap_err();
    assert!(format!("{error:#}").starts_with("ImageBase 0xffffffffffff0000 leaves no room"), "{error:#}");
}

#[test]
fn an_export_count_past_the_image_is_a_warning_not_an_allocation() {
    let mut pe = Pe::new();
    pe.words(0x1000, &[0, 0, 0, 0, 1, u32::MAX, 0, 0, 0, 0]);
    pe.directory(Directory::Export, 0x1000, 40);
    let image = pe.parse();
    assert!(image.exports.is_none());
    let w = warning(&image, "exports");
    assert!(w.contains("counts 4294967295 functions, more than the image holds"), "{w}");
}

#[test]
fn export_ordinals_wrap_past_the_top_of_their_base() {
    let mut pe = Pe::new();
    pe.words(0x1000, &[0, 0, 0, 0, u32::MAX, 2, 0, 0x1100, 0, 0]);
    pe.words(0x1100, &[0x1200, 0x1204]);
    pe.directory(Directory::Export, 0x1000, 40);
    let image = pe.parse();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let exports = &image.exports.expect("an export table").exports;
    let listed: Vec<(u32, Option<u64>)> = exports.iter().map(|e| (e.ordinal, e.va)).collect();
    assert_eq!(listed, [(u32::MAX, Some(BASE + 0x1200)), (0, Some(BASE + 0x1204))]);
}

#[test]
fn a_relocation_past_the_last_rva_is_a_warning_with_its_block() {
    let mut pe = Pe::new();
    pe.words(0x1000, &[0xffff_f800, 12]);
    pe.data(0x1008, &[0xff, 0x3f, 0, 0]);
    pe.directory(Directory::BaseReloc, 0x1000, 12);
    let image = pe.parse();
    assert!(image.relocations.is_empty());
    let w = warning(&image, "relocations");
    assert!(w.contains("entry 0 of the block at offset 0x0 of the directory patches past RVA 0xffffffff"), "{w}");
}

#[test]
fn shared_resource_subdirectories_stop_at_the_tree_budget() {
    // Each level's 255 entries all name the one directory below: 255^3 leaves from 0x1828 bytes.
    let mut pe = Pe::new();
    pe.resource_directory(0, 1, 255, SUBDIRECTORY | 0x808);
    pe.resource_directory(0x808, 1, 255, SUBDIRECTORY | 0x1010);
    pe.resource_directory(0x1010, 1, 255, 0x1818);
    pe.words(0x2818, &[0x1000, 0, 0, 0]);
    pe.directory(Directory::Resource, 0x1000, 0x1828);
    let image = pe.parse();
    assert!(image.resources.is_empty());
    let w = warning(&image, "resources");
    assert!(w.contains("takes the tree past 262144 entries"), "{w}");
}

/// A string table of one block, `block`, whose first string is "A".
fn string_block(block: u32) -> Image {
    let mut pe = Pe::new();
    pe.resource_directory(0, 6, 1, SUBDIRECTORY | 0x18);
    pe.resource_directory(0x18, block, 1, SUBDIRECTORY | 0x30);
    pe.resource_directory(0x30, 0x409, 1, 0x48);
    pe.words(0x1048, &[0x1100, 4, 0, 0]);
    pe.data(0x1100, &[1, 0, b'A', 0]);
    pe.directory(Directory::Resource, 0x1000, 0x58);
    let image = pe.parse();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    assert_eq!(image.resources.len(), 1);
    image
}

#[test]
fn a_string_block_whose_ids_pass_the_top_holds_no_string() {
    assert_eq!(resources::string_table(&string_block(1)), [(0, "A".to_owned())]);
    assert!(resources::string_table(&string_block(0x7fff_ffff)).is_empty());
}

#[test]
fn a_debug_directory_size_bounds_the_walk_not_the_allocation() {
    let mut pe = Pe::new();
    pe.directory(Directory::Debug, 0x1000, 0xffff_fffc);
    let image = pe.parse();
    let w = warning(&image, "debug");
    assert!(w.contains("address 0x140003000 lies outside the image"), "{w}");
}

#[test]
fn an_exception_directory_size_bounds_the_walk_not_the_allocation() {
    let mut pe = Pe::new();
    pe.directory(Directory::Exception, 0x1000, 0xffff_fffc);
    let image = pe.parse();
    let w = warning(&image, "exception table");
    assert!(w.contains("address 0x140003000 lies outside the image"), "{w}");
}

/// An ELF64 relocatable object for x86-64: `.text` (16 bytes), a symbol table and a string table
/// at `strings_offset`. Its one symbol is the global function `f` at 4, `size` bytes long.
fn elf(size: u64, strings_offset: u64) -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    let mut put = |at: usize, bytes: &[u8]| b[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"\x7fELF\x02\x01\x01");
    put(16, &1u16.to_le_bytes()); // ET_REL
    put(18, &62u16.to_le_bytes()); // EM_X86_64
    put(40, &0x100u64.to_le_bytes()); // e_shoff
    put(58, &64u16.to_le_bytes()); // e_shentsize
    put(60, &4u16.to_le_bytes()); // e_shnum
    put(62, &3u16.to_le_bytes()); // e_shstrndx
    put(0x40, &[0xc3; 16]);
    // Symbol 1: "f", global function, in section 1.
    put(0x68, &1u32.to_le_bytes());
    put(0x6c, &[0x12, 0]);
    put(0x6e, &1u16.to_le_bytes());
    put(0x70, &4u64.to_le_bytes());
    put(0x78, &size.to_le_bytes());
    put(0x80, b"\0f\0.text\0");
    // Section headers 1 to 3; header 0 is the null one.
    let header = |n: usize, field: usize| 0x100 + n * 64 + field;
    put(header(1, 0), &3u32.to_le_bytes());
    put(header(1, 4), &1u32.to_le_bytes()); // SHT_PROGBITS
    put(header(1, 8), &6u64.to_le_bytes()); // SHF_ALLOC | SHF_EXECINSTR
    put(header(1, 24), &0x40u64.to_le_bytes());
    put(header(1, 32), &16u64.to_le_bytes());
    put(header(2, 4), &2u32.to_le_bytes()); // SHT_SYMTAB
    put(header(2, 24), &0x50u64.to_le_bytes());
    put(header(2, 32), &48u64.to_le_bytes());
    put(header(2, 40), &3u32.to_le_bytes());
    put(header(2, 56), &24u64.to_le_bytes());
    put(header(3, 4), &3u32.to_le_bytes()); // SHT_STRTAB
    put(header(3, 24), &strings_offset.to_le_bytes());
    put(header(3, 32), &9u64.to_le_bytes());
    b
}

#[test]
fn an_elf_function_size_past_the_top_ends_at_its_section() {
    let o = object::parse_object("f.o", &elf(u64::MAX, 0x80)).unwrap().expect("an ELF object");
    let functions: Vec<(&str, u32, u32)> = o.functions.iter().map(|f| (f.name.as_str(), f.offset, f.size)).collect();
    assert_eq!(functions, [("f", 4, 12)]);
    assert_eq!(o.sections[0].name, ".text");
}

#[test]
fn an_elf_string_table_past_the_top_is_a_refusal() {
    let error = object::parse_object("f.o", &elf(4, u64::MAX - 1)).unwrap_err();
    assert!(format!("{error:#}").contains("of section 3 is past its end"), "{error:#}");
}
