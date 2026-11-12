//! The x86 decoder, 32- or 64-bit by the first byte, over a run at an address of the input's
//! choosing: iced's records, the neutral ones and the links between them.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_arch::Arch;
use ra_asm_x86::X86;

fuzz_target!(|data: &[u8]| {
    let Some((mode, address, bytes)) = ra_asm_fuzz::split(data) else { return };
    let x86 = if mode & 1 == 0 { X86::X64 } else { X86::X32 };
    for decoded in x86.decode_all_full(bytes, address) {
        let _ = (decoded.instr.end(), decoded.instr.indirect_operand(), decoded.instr.absolute_memory_operand());
    }
    let _ = x86.decode_all(bytes, address);
});
