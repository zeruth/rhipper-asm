//! The decoder against the disassembler: every instruction of the fixture program
//! (`testdata/riscv/rv64.elf`, built from `src/rv64.c`) decodes to the text `llvm-objdump -d -M
//! no-aliases` lists for it (`rv64.txt` beside it), word by word.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ra_asm_riscv::{Width, decode};

/// The instructions of the listing: each one's address, its word and its text, the mnemonic and
/// the operands a space apart and the name the disassembler adds to a target left out.
fn listed() -> Vec<(u64, u32, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/riscv/rv64.txt");
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let Some((address, rest)) = line.trim_start().split_once(": ") else { continue };
        let (Ok(address), Some((word, text))) = (u64::from_str_radix(address, 16), rest.split_once(char::is_whitespace)) else { continue };
        let Ok(word) = u32::from_str_radix(word, 16) else { continue };
        let text = text.split(" <").next().unwrap_or(text);
        out.push((address, word, text.split_whitespace().collect::<Vec<_>>().join(" ")));
    }
    out
}

#[test]
fn the_decoder_agrees_with_objdump() {
    let listed = listed();
    assert!(listed.len() > 200, "the listing has {} instructions", listed.len());
    let mut mnemonics = BTreeSet::new();
    for (address, word, expected) in &listed {
        let decoded =
            decode(*word, *address, Width::Rv64).unwrap_or_else(|| panic!("{address:#x}: {word:08x} ({expected}) does not decode"));
        assert_eq!(&decoded.to_string(), expected, "{address:#x}: {word:08x}");
        mnemonics.insert(decoded.mnemonic);
    }
    // The fixture is written to use most of the two sets.
    assert!(mnemonics.len() >= 44, "the fixture uses {} instructions: {mnemonics:?}", mnemonics.len());
    for name in
        ["lui", "jal", "jalr", "bge", "bltu", "lbu", "lhu", "sd", "sb", "srai", "addiw", "mul", "div", "divu", "divuw", "sraw", "ecall"]
    {
        assert!(mnemonics.contains(name), "the fixture has no {name}");
    }
}
