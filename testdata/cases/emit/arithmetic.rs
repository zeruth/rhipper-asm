pub extern "C" fn arithmetic(a: u64, b: u64) -> u64 {
    let mut sum: u64 = 0u64;
    sum = a.wrapping_add(b);
    return ((sum.wrapping_mul(a.wrapping_sub(b))).wrapping_mul(3u64)) ^ sum;
}
