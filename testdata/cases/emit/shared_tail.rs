pub extern "C" fn shared_tail(n: u32, p: *mut u8) -> u32 {
    let mut i: u32 = 0u32;
    'L_ret: {
        if n != 0u32 {
            loop {
                unsafe { i = core::ptr::read_unaligned(p as *const u32); } // memory at a computed address
                if (i as i32) < (n as i32) {
                    if i == 7u32 {
                        unsafe { core::ptr::write_unaligned(p as *mut u32, 1u32); } // memory at a computed address
                    } else {
                        unsafe { core::ptr::write_unaligned(p as *mut u32, i.wrapping_add(1u32)); } // memory at a computed address
                        continue;
                    }
                } else {
                    unsafe { core::ptr::write_unaligned(p as *mut u32, 0u32); } // memory at a computed address
                }
                break 'L_ret;
            }
        }
    }
    unsafe { core::ptr::write_unaligned(p as *mut u32, 1u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 2u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 3u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 4u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 5u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 6u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 7u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 8u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 9u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 10u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 11u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 12u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 13u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 14u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 15u32); } // memory at a computed address
    unsafe { core::ptr::write_unaligned(p as *mut u32, 16u32); } // memory at a computed address
    unsafe { return core::ptr::read_unaligned(p as *const u32); } // memory at a computed address
}
