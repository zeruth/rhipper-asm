pub extern "C" fn calls(x: u64) -> u64 {
    let mut first: u64 = 0u64;
    unsafe { first = report(x, 1u64); } // an import
    unsafe { let _ = report(first, 2u64); } // an import
    return first.wrapping_add(x);
}
