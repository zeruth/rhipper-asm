//! The PE loader over any bytes, and the walks of the image it leaves to its callers.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_pe::Image;

fuzz_target!(|data: &[u8]| {
    if let Ok(image) = Image::parse(data.to_vec()) {
        ra_asm_fuzz::walk(&image);
    }
});
