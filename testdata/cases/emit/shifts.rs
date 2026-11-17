pub extern "C" fn shifts(a: u32, b: u32) -> u32 {
    return ((((a.wrapping_shl(3u32 as u32)) as i32).wrapping_div(b as i32) as u32).wrapping_add((b.wrapping_shr(4u32 as u32)) % 10u32)) | ((a as i32).wrapping_shr(2u32 as u32) as u32);
}
