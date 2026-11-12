//! What the targets share: the walks over a parsed image that the loader leaves to its callers,
//! and the split of a decoder's input into its mode, its address and its bytes.

use ra_asm_pe::{Image, Machine, exception, resources};

/// Reads every part of `image` the pipeline reads after the load: the string table, the `.res`
/// file, the C++ frames and the unwind information of each entry of the exception table. A
/// refusal is an `Err` or an empty result; only a panic or a runaway allocation is a finding.
pub fn walk(image: &Image) {
    let _ = resources::string_table(image);
    let _ = resources::res_file(image);
    let _ = exception::cxx_frames(image);
    let arm64 = matches!(image.machine(), Machine::Arm64);
    for function in &image.runtime_functions {
        if arm64 {
            let _ = function.arm64_unwind(image).and_then(|unwind| unwind.operations());
        } else if let Ok(info) = function.unwind_info(image) {
            let _ = info.operations();
            let _ = info.frame_size();
        }
    }
}

/// The first byte picks the mode, the next eight the address of the first instruction; the rest
/// are the bytes. The address is the fuzzer's too, so a run at the top of the space is in reach.
pub fn split(data: &[u8]) -> Option<(u8, u64, &[u8])> {
    let (&mode, rest) = data.split_first()?;
    let (address, bytes) = rest.split_at_checked(8)?;
    Some((mode, u64::from_le_bytes(address.try_into().ok()?), bytes))
}
