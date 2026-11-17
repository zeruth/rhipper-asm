//! The PS-X EXE as an [`Image`], over the PSn00bSDK fixture `hello_psx.exe`: the header, the text
//! as one segment, RAM through its three mirrors, the places of the map that hold no byte, and a
//! truncated file refused at its field.

use std::path::PathBuf;

use ra_asm_format::psx::Region;
use ra_asm_pe::Image;
use ra_asm_pe::psx::{KSEG0, KSEG1, KUSEG, MAP};

fn hello() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello_psx.exe")
}

/// The header as `elf2x` wrote it: the entry at the SDK's `_start`, the text at `0x80010000`, one
/// 2 KiB block, and neither `gp` nor a stack, which the start routine and the BIOS set.
#[test]
fn the_header_gives_the_entry_the_text_and_the_registers() {
    let image = ra_asm_pe::load(&hello()).unwrap();
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    let exe = image.psx.as_ref().expect("a PS-X EXE");
    assert_eq!((exe.pc, exe.text), (0x8001_0088, (0x8001_0000, 0x800)));
    assert_eq!(exe.entry(), 0x8001_0088);
    assert_eq!(image.entry_point(), Some(0x8001_0088));
    assert_eq!((exe.gp, image.global_pointer), (0, None));
    assert_eq!(exe.initial_sp(), None, "the BIOS's stack");
    assert_eq!(image.platform(), Some("psx"));
    assert!(image.is_code(0x8001_0088));
}

/// The text is one segment at its `kseg0` address, the file's bytes after the 2 KiB header, and
/// nothing of the image lies around it.
#[test]
fn the_text_is_one_segment() {
    let bytes = std::fs::read(hello()).unwrap();
    let image = ra_asm_pe::load(&hello()).unwrap();
    assert_eq!(image.sections.len(), 1);
    let text = &image.sections[0];
    assert_eq!((text.name.as_str(), text.virtual_address, text.virtual_size, text.raw_offset), (".text", 0, 0x800, 0x800));
    assert_eq!((image.image_base(), image.size_of_image()), (0x8001_0000, 0x800));
    assert_eq!(image.section_of(0x8001_07ff).map(|s| s.name.as_str()), Some(".text"));
    assert_eq!(image.read(0x8001_0000, 0x800).unwrap(), &bytes[0x800..0x1000]);
    assert!(image.read(0x8001_0800, 1).is_err(), "the text ends at its size");
    assert!(image.read(0x8000_fffc, 4).is_err(), "nothing lies below the text");
}

/// RAM is one place seen through `kuseg`, `kseg0` and `kseg1`: the entry's word reads the same
/// through each, and each folds to the `kseg0` address.
#[test]
fn an_address_reads_the_same_through_each_mirror() {
    let bytes = std::fs::read(hello()).unwrap();
    let image = ra_asm_pe::load(&hello()).unwrap();
    let ram = image.memory_map().iter().find(|p| p.region == Region::Ram).expect("RAM in the map");
    assert!(ram.holds_bytes());
    assert_eq!((ram.start, ram.size), (0, 0x20_0000));
    let word = u32::from_le_bytes(bytes[0x888..0x88c].try_into().unwrap());
    let mirrors: Vec<u64> = ram.mirrors().collect();
    assert_eq!(mirrors, [KUSEG, KSEG0, KSEG1]);
    for mirror in mirrors {
        let va = mirror | 0x1_0088;
        assert_eq!(image.u32(va).unwrap(), word, "{va:#x}");
        assert_eq!(image.rva(va), Some(0x88), "{va:#x}");
        assert_eq!(image.memory_region(va), Some((Region::Ram, 0x8001_0088)), "{va:#x}");
    }
}

/// The BIOS, the scratchpad and the hardware registers are named places that the image holds no
/// byte of, through every segment that reaches them; the scratchpad is not on `kseg1`'s bus.
#[test]
fn the_bios_scratchpad_and_registers_are_places_without_bytes() {
    let image = ra_asm_pe::load(&hello()).unwrap();
    assert_eq!(image.memory_map(), &MAP);
    let names: Vec<&str> = image.memory_map().iter().filter(|p| !p.holds_bytes()).map(|p| p.name()).collect();
    assert_eq!(names, ["scratchpad", "hardware", "bios"]);
    for place in image.memory_map().iter().filter(|p| !p.holds_bytes()) {
        for va in place.mirrors() {
            let last = va + place.size - 4;
            assert_eq!(image.memory_region(va), Some((place.region, place.start)), "{va:#x}");
            assert_eq!(image.memory_region(last), Some((place.region, place.start + place.size - 4)), "{last:#x}");
            assert!(image.read(va, 4).is_err(), "{} at {va:#x} is no byte of the image", place.name());
        }
    }
    assert_eq!(image.memory_region(KSEG1 | ra_asm_format::psx::SCRATCHPAD), None);
    assert!(
        ra_asm_pe::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello64.exe")).unwrap().memory_map().is_empty()
    );
}

/// A file cut short is refused at the field it lost, by its name and offset.
#[test]
fn a_truncated_file_names_the_field() {
    let bytes = std::fs::read(hello()).unwrap();
    for (len, field) in [
        (0x12, "the initial pc at offset 0x10"),
        (0x1c, "the text size at offset 0x1c"),
        (0x34, "the stack offset at offset 0x34"),
        (0xc00, "the text at offset 0x800 (0x800 bytes)"),
    ] {
        let error = Image::parse(bytes[..len].to_vec()).unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains(field) && text.contains("past the end"), "{len:#x}: {text}");
    }
}
