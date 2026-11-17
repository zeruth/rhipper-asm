pub extern "C" fn sum_loop(n: u64) -> u64 {
    let mut i: u64 = 0u64;
    let mut i_next: u64 = 0u64;
    let mut sum: u64 = 0u64;
    let mut sum_next: u64 = 0u64;
    let mut added: u64 = 0u64;
    let mut next: u64 = 0u64;
    let mut r: u64 = 0u64;
    let mut r_next: u64 = 0u64;
    i_next = 0u64;
    sum_next = 0u64;
    r_next = 0u64;
    if n != 0u64 {
        loop {
            i = i_next;
            sum = sum_next;
            added = sum.wrapping_add(i);
            next = i.wrapping_add(1u64);
            i_next = next;
            sum_next = added;
            r_next = added;
            if next < n {
                continue;
            }
            break;
        }
    }
    r = r_next;
    return r;
}
