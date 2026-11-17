pub extern "C" fn blend(a: f64, b: f64, n: u32) -> f64 {
    let mut scaled: f64 = 0.0f64;
    scaled = ((a + b) * b) / ((n as i32) as f64);
    return if scaled < a { scaled } else { a };
}
