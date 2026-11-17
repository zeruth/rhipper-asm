pub extern "C" fn early_return(x: u32, p: *mut u8) -> u32 {
    let mut v: u32 = 0u32;
    if p == (core::ptr::null_mut()) {
        return 4294967295u32;
    }
    unsafe { v = core::ptr::read_unaligned(p as *const u32); } // memory at a computed address
    if v == 0u32 {
        return 0u32;
    }
    if v > x {
        return 1u32;
    } else {
        return x.wrapping_sub(v);
    }
}
