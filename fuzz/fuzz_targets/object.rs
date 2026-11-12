//! The relocatable objects and their archives: a COFF `.obj`, an ELF `.o`, a `.lib` or a `.a`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_pe::object;

fuzz_target!(|data: &[u8]| {
    let _ = object::parse("fuzz.lib", data);
});
