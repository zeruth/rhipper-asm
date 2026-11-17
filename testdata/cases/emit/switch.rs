pub extern "C" fn dispatch(k: u32) -> u32 {
    match k {
        0u32 => {
            return 100u32;
        }
        1u32 | 2u32 => {
            return 200u32;
        }
        7u32 => {
            return 700u32;
        }
        _ => {
            return k.wrapping_add(1u32);
        }
    }
}
