pub extern "C" fn swap(a: *mut u8, b: *mut u8) {
    let mut x: u32 = 0u32;
    unsafe { x = core::ptr::read_unaligned(a as *const u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(a as *mut u32, core::ptr::read_unaligned(b as *const u32)); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(b as *mut u32, x); } // memory at a computed address
    return;
}
