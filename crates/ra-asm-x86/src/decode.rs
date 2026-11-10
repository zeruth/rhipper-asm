//! The decoder skeleton. `decode_one` decodes one instruction from a byte slice at an address;
//! the first milestone fills the opcode table in from the smallest reliable subset: prefixes,
//! then legacy single-byte opcodes.

use ra_asm_ir::instr::Instr;

/// Decodes one instruction from `bytes` at virtual address `address`.
///
/// Returns the instruction and the length it consumed. It fails when the bytes hold no
/// instruction the subset supports; it never decodes past the end of the slice.
pub fn decode_one(bytes: &[u8], address: u64) -> anyhow::Result<(Instr, usize)> {
    anyhow::ensure!(!bytes.is_empty(), "no bytes at {address:#x}");
    // The placeholder decodes `nop` (0x90) only; the opcode table grows next.
    match bytes[0] {
        0x90 => Ok((Instr::new(address, "nop"), 1)),
        other => anyhow::bail!("unsupported opcode {other:#04x} at {address:#x}"),
    }
}
