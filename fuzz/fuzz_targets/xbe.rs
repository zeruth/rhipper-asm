//! The original Xbox's container: the bytes after its magic, so every input reaches the XBE reader
//! rather than the PE headers'.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_pe::{Image, xbe};

fuzz_target!(|data: &[u8]| {
    let mut file = xbe::MAGIC.to_le_bytes().to_vec();
    file.extend_from_slice(data.get(4..).unwrap_or_default());
    if let Ok(image) = Image::parse(file) {
        ra_asm_fuzz::walk(&image);
    }
});
