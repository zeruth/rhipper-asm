//! The ELF reader: the bytes after its magic, so every input reaches it rather than the COFF one.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_pe::object;

fuzz_target!(|data: &[u8]| {
    let mut file = b"\x7fELF".to_vec();
    file.extend_from_slice(data.get(4..).unwrap_or_default());
    let _ = object::parse_object("fuzz.o", &file);
});
