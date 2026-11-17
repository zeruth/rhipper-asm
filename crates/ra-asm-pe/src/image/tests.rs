use std::path::Path;

use ra_asm_format::ByteOrder;

use super::Image;

/// A build of the MIPS control fixture: `control` little-endian, `control.be` big-endian, the same
/// source and layout.
fn mips(name: &str) -> Image {
    crate::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mips").join(name)).unwrap()
}

#[test]
fn a_big_endian_image_reads_its_values_big_endian() {
    let (le, be) = (mips("control"), mips("control.be"));
    assert_eq!((le.order(), be.order()), (ByteOrder::Little, ByteOrder::Big));
    assert!(be.big_endian() && !be.is_64());
    // Both enter at `lui $gp, %hi(_gp)`: the same instruction in either order.
    let entry = be.entry_point().unwrap();
    assert_eq!(le.entry_point(), Some(entry));
    let word = be.u32(entry).unwrap();
    assert_eq!((le.u32(entry).unwrap() >> 16, word >> 16), (0x3c1c, 0x3c1c));
    assert_eq!(be.read(entry, 2).unwrap(), [0x3c, 0x1c]);
    assert_eq!(be.u16(entry).unwrap(), 0x3c1c);
    assert_eq!(be.pointer(entry).unwrap(), u64::from(word));
    assert_eq!(be.pointer_in(be.read(entry, 4).unwrap()), u64::from(word));
    assert_eq!(be.reader_at(entry).unwrap().u32(0, "word").unwrap(), word);
    assert_eq!(be.u64(entry).unwrap() >> 32, u64::from(word));
}

#[test]
fn a_big_endian_image_writes_its_words_big_endian() {
    let mut be = mips("control.be");
    let data = be.sections.iter().find(|s| s.name == ".data").map(|s| be.va(s.virtual_address)).unwrap();
    be.write_pointer(data, 0x1234_5678).unwrap();
    assert_eq!(be.read(data, 4).unwrap(), [0x12, 0x34, 0x56, 0x78]);
    assert_eq!(be.pointer(data).unwrap(), 0x1234_5678);
    be.write_u32(data + 4, 0x0102_0304).unwrap();
    assert_eq!(be.read(data + 4, 4).unwrap(), [1, 2, 3, 4]);
}
