pub extern "C" fn absolute(x: u64) -> u64 {
    let mut negated: u64 = 0u64;
    let mut r: u64 = 0u64;
    let mut r_next: u64 = 0u64;
    r_next = x;
    if (x as i64) < (0u64 as i64) {
        negated = 0u64.wrapping_sub(x);
        r_next = negated;
    }
    r = r_next;
    return r;
}
