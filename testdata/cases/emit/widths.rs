pub extern "C" fn widths(x: u64) -> u64 {
    return ((x as u8) as u64).wrapping_add((((x as u16) as i16) as i64) as u64);
}
