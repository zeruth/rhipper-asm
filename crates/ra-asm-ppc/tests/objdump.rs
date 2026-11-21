//! The disassembler against `llvm-objdump`: every instruction of the fixture programs
//! (`testdata/ppc/ppc.elf`, `ppcfp.elf`) reads as the listing kept beside each has it (`ppc.txt`,
//! `ppcfp.txt`), word by word.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ra_asm_ppc::disasm::disassemble;

/// The instructions of a listing, and the mnemonics among them.
fn agrees(listing: &str) -> (usize, BTreeSet<String>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ppc").join(listing);
    let (mut count, mut mnemonics) = (0, BTreeSet::new());
    for line in std::fs::read_to_string(path).unwrap().lines() {
        // `address: bb bb bb bb <tab> mnemonic operands`, the bytes as the file has them.
        let Some((address, rest)) = line.trim_start().split_once(": ") else { continue };
        let Ok(address) = u64::from_str_radix(address, 16) else { continue };
        let mut parts = rest.split_whitespace();
        let bytes: Vec<u8> = parts.by_ref().take(4).filter_map(|byte| u8::from_str_radix(byte, 16).ok()).collect();
        let [b0, b1, b2, b3] = bytes[..] else { continue };
        let listed = parts.collect::<Vec<_>>().join(" ");
        // The name the disassembler adds to a target is the linker's, not the instruction's.
        let listed = listed.split(" <").next().unwrap_or(&listed).to_owned();
        // A word the code keeps among its instructions (the offset to its constants) is none.
        if listed == "<unknown>" {
            continue;
        }
        let word = u32::from_be_bytes([b0, b1, b2, b3]);
        let ours = disassemble(word, address).unwrap_or_else(|| panic!("{address:#x}: {word:08x} ({listed}) has no text"));
        assert_eq!(ours, listed, "{address:#x}: {word:08x}");
        mnemonics.insert(ours.split(' ').next().unwrap_or_default().to_owned());
        count += 1;
    }
    (count, mnemonics)
}

#[test]
fn the_disassembler_agrees_with_objdump() {
    let (count, mnemonics) = agrees("ppc.txt");
    assert!(count > 250, "the listing has {count} instructions");
    assert!(mnemonics.len() >= 50, "the fixture uses {} mnemonics: {mnemonics:?}", mnemonics.len());
}

#[test]
fn the_floating_point_reads_as_objdump_lists_it() {
    let (count, mnemonics) = agrees("ppcfp.txt");
    assert!(count > 500, "the listing has {count} instructions");
    for used in ["lfs", "lfd", "stfd", "stfsu", "fadd", "fsubs", "fmul", "fdivs", "fmadd", "fcmpu", "fctiwz", "frsp", "fneg", "fmr"] {
        assert!(mnemonics.contains(used), "the fixture has no {used}: {mnemonics:?}");
    }
}
