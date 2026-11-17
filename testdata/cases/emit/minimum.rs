pub extern "C" fn minimum(a: u32, b: u32) -> u32 {
    return if (a as i32) < (b as i32) { a } else { b };
}
