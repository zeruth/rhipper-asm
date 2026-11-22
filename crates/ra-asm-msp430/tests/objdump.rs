//! The disassembler against `llvm-objdump`: every instruction of the fixture program
//! (`testdata/msp430/msp430.elf`) reads as the listing kept beside it has it (`msp430.txt`),
//! its length with it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ra_asm_msp430::disasm::disassemble;

#[test]
fn the_disassembler_agrees_with_objdump() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/msp430/msp430.txt");
    let (mut count, mut mnemonics) = (0, BTreeSet::new());
    for line in std::fs::read_to_string(path).unwrap().lines() {
        // `address: bb bb .. <tab> mnemonic <tab> operands`, the bytes as the file has them.
        let mut fields = line.split('\t');
        let Some((address, bytes)) = fields.next().and_then(|head| head.trim().split_once(": ")) else { continue };
        let Ok(address) = u16::from_str_radix(address, 16) else { continue };
        let bytes: Vec<u8> = bytes.split_whitespace().filter_map(|byte| u8::from_str_radix(byte, 16).ok()).collect();
        let listed = fields.collect::<Vec<_>>().join("\t");
        assert!(!bytes.is_empty() && bytes.len().is_multiple_of(2) && !listed.is_empty(), "{line}");
        let words: Vec<u16> = bytes.chunks(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        let (ours, len) = disassemble(&words, address).unwrap_or_else(|| panic!("{address:#x}: {words:04x?} ({listed}) has no text"));
        assert_eq!(ours, listed, "{address:#x}: {words:04x?}");
        assert_eq!(usize::from(len), bytes.len(), "{address:#x}: the length of {listed}");
        mnemonics.insert(ours.split('\t').next().unwrap_or_default().to_owned());
        count += 1;
    }
    assert!(count > 1200, "the listing has {count} instructions");
    for used in
        ["mov", "mov.b", "add", "sub", "cmp", "bit.b", "xor", "rra", "rrc", "swpb", "sxt", "push", "pop", "call", "ret", "br", "jne", "jl"]
    {
        assert!(mnemonics.contains(used), "the fixture has no {used}: {mnemonics:?}");
    }
}
