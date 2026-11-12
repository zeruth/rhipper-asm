//! The `AArch64` decoder over a run at an address of the input's choosing, with the links that
//! join an `adrp` to the load after it. The mode byte is read and ignored, so the x86 target's
//! inputs are this one's too.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ra_asm_arch::Arch;
use ra_asm_arm64::Arm64;

fuzz_target!(|data: &[u8]| {
    let Some((_, address, bytes)) = ra_asm_fuzz::split(data) else { return };
    for (n, word) in bytes.chunks_exact(4).enumerate() {
        let _ = Arm64.decode_full(word, address.wrapping_add(4 * n as u64));
    }
    for instr in Arm64.decode_all(bytes, address) {
        let _ = (instr.end(), instr.indirect_operand(), instr.absolute_memory_operand());
    }
});
