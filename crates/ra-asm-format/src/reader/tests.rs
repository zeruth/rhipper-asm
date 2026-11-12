use super::Reader;

#[test]
fn reads_little_endian_values() {
    let bytes: [u8; 9] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xff];
    let r = Reader::new(&bytes);
    assert_eq!(r.u8(0, "a").unwrap(), 0x01);
    assert_eq!(r.u16(0, "a").unwrap(), 0x0201);
    assert_eq!(r.u32(0, "a").unwrap(), 0x0403_0201);
    assert_eq!(r.u64(0, "a").unwrap(), 0x0807_0605_0403_0201);
    assert_eq!(r.i32(5, "a").unwrap(), i32::from_le_bytes([0x06, 0x07, 0x08, 0xff]));
}

#[test]
fn a_read_past_the_end_names_the_field_and_the_offset() {
    let bytes = [0u8; 4];
    let error = Reader::new(&bytes).u32(2, "the magic").unwrap_err();
    assert_eq!(format!("{error:#}"), "the magic at offset 0x2 (0x4 bytes) is past the end of the data (0x4 bytes)");
}

#[test]
fn an_offset_overflow_is_a_refusal_not_a_panic() {
    let bytes = [0u8; 4];
    assert!(Reader::new(&bytes).slice(usize::MAX, 2, "x").is_err());
    assert_eq!(Reader::new(b"a\0").cstr(0, usize::MAX, "x").unwrap(), "a");
    let mut c = Reader::new(&bytes).at(2);
    let error = c.skip(usize::MAX).u8("the flags").unwrap_err();
    assert!(format!("{error:#}").starts_with("the flags at offset 0xffffffffffffffff"), "{error:#}");
}

#[test]
fn cstr_stops_at_the_terminator_and_refuses_an_unterminated_string() {
    let bytes = b"KERNEL32.dll\0junk";
    let r = Reader::new(bytes);
    assert_eq!(r.cstr(0, 64, "dll name").unwrap(), "KERNEL32.dll");
    let error = r.cstr(13, 3, "dll name").unwrap_err();
    assert_eq!(format!("{error:#}"), "dll name at offset 0xd has no terminator within 3 bytes");
}

#[test]
fn fixed_str_cuts_at_the_first_nul_and_keeps_a_full_field() {
    let r = Reader::new(b".text\0\0\0.rdata\0\0.textbss");
    assert_eq!(r.fixed_str(0, 8, "name").unwrap(), ".text");
    assert_eq!(r.fixed_str(8, 8, "name").unwrap(), ".rdata");
    assert_eq!(r.fixed_str(16, 8, "name").unwrap(), ".textbss");
}

#[test]
fn a_cursor_advances_by_what_it_read() {
    let bytes: [u8; 8] = [0x4d, 0x5a, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00];
    let mut c = Reader::new(&bytes).at(0);
    assert_eq!(c.u16("e_magic").unwrap(), 0x5a4d);
    assert_eq!(c.position(), 2);
    assert_eq!(c.skip(2).u32("e_cp").unwrap(), 3);
    assert_eq!(c.position(), 8);
    assert!(c.u8("past").is_err());
}
