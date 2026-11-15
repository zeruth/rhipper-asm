use crate::{ByteOrder, Reader};

const BYTES: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

#[test]
fn order_is_little_by_default() {
    let r = Reader::new(&BYTES);
    assert_eq!(r.order(), ByteOrder::Little);
    assert_eq!(r.at(0).order(), ByteOrder::Little);
    assert_eq!(ByteOrder::default(), ByteOrder::Little);
}

#[test]
fn order_16_bits_both_ways() {
    let le = Reader::new(&BYTES);
    let be = le.with_order(ByteOrder::Big);
    assert_eq!(le.u16(0, "a").unwrap(), 0x0201);
    assert_eq!(be.u16(0, "a").unwrap(), 0x0102);
    assert_eq!(be.at(6).u16("a").unwrap(), 0x0708);
}

#[test]
fn order_32_bits_both_ways() {
    let le = Reader::new(&BYTES);
    let be = le.with_order(ByteOrder::Big);
    assert_eq!(le.u32(0, "a").unwrap(), 0x0403_0201);
    assert_eq!(be.u32(0, "a").unwrap(), 0x0102_0304);
    assert_eq!(be.i32(4, "a").unwrap(), 0x0506_0708);
    let mut c = be.at(0);
    assert_eq!((c.u32("a").unwrap(), c.u32("b").unwrap()), (0x0102_0304, 0x0506_0708));
    assert_eq!(c.position(), 8);
}

#[test]
fn order_64_bits_both_ways() {
    let le = Reader::new(&BYTES);
    let be = le.with_order(ByteOrder::Big);
    assert_eq!(le.u64(0, "a").unwrap(), 0x0807_0605_0403_0201);
    assert_eq!(be.u64(0, "a").unwrap(), 0x0102_0304_0506_0708);
    assert_eq!(be.at(0).u64("a").unwrap(), 0x0102_0304_0506_0708);
    assert_eq!(be.with_order(ByteOrder::Little).u64(0, "a").unwrap(), 0x0807_0605_0403_0201);
}

#[test]
fn order_does_not_change_a_refusal() {
    let be = Reader::new(&BYTES).with_order(ByteOrder::Big);
    let error = be.u32(6, "the magic").unwrap_err();
    assert_eq!(format!("{error:#}"), "the magic at offset 0x6 (0x4 bytes) is past the end of the data (0x8 bytes)");
    let error = be.at(4).skip(2).u64("the entry").unwrap_err();
    assert_eq!(format!("{error:#}"), "the entry at offset 0x6 (0x8 bytes) is past the end of the data (0x8 bytes)");
}
