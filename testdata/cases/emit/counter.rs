pub extern "C" fn counter(step: u64) -> u64 {
    let mut old: u64 = 0u64;
    unsafe { old = core::ptr::read_unaligned(((&raw mut count) as *mut u8) as *const u64); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(((&raw mut count) as *mut u8) as *mut u64, old.wrapping_add(step)); } // memory at a computed address
    return old;
}
