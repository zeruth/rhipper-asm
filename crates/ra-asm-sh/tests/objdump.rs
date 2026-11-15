//! The decoder against `sh-elf-objdump` of KallistiOS: every word of the fixture's `.text`, as
//! objdump printed it in `testdata/dc/control.objdump`, decodes to the same text, and the words
//! objdump leaves as `.word` decode to nothing.

use std::path::{Path, PathBuf};

use ra_asm_arch::{Arch, Delay};
use ra_asm_sh::Sh4;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/dc").join(name)
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

/// The bytes of the `.text` section of a little-endian ELF32 object.
fn text_section(elf: &[u8]) -> &[u8] {
    assert_eq!(&elf[..6], b"\x7fELF\x01\x01", "control.o is not a little-endian ELF32 file");
    let (count, names) = (u16_at(elf, 0x30), u16_at(elf, 0x32));
    let strings = u32_at(section_header(elf, names), 16);
    let header = (0..count)
        .map(|i| section_header(elf, i))
        .find(|h| elf[strings + u32_at(h, 0)..].starts_with(b".text\0"))
        .expect("control.o has no .text section");
    let (offset, size) = (u32_at(header, 16), u32_at(header, 20));
    &elf[offset..offset + size]
}

/// The `i`th entry of the section header table.
fn section_header(elf: &[u8], i: usize) -> &[u8] {
    let (table, size) = (u32_at(elf, 0x20), u16_at(elf, 0x2e));
    &elf[table + i * size..table + (i + 1) * size]
}

/// One instruction row of the listing: `  4a:\t05 d1       \tmov.l\t60 <_fib+0x2e>,r1\t! 32 <_fib>`.
struct Row {
    address: u64,
    bytes: [u8; 2],
    text: String,
}

fn rows(listing: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for line in listing.lines() {
        // The file and symbol header lines start with no hexadecimal address before their colon.
        let Some((address, rest)) = line.trim_start().split_once(':') else { continue };
        let Ok(address) = u64::from_str_radix(address, 16) else { continue };
        let Some((bytes, text)) = rest.strip_prefix('\t').and_then(|r| r.split_once('\t')) else { continue };
        let bytes: Vec<u8> = bytes.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        let bytes: [u8; 2] = bytes.as_slice().try_into().unwrap_or_else(|_| panic!("{address:#x}: not one 16-bit word: {line:?}"));
        out.push(Row { address, bytes, text: normalize(text) });
    }
    out
}

/// objdump's text in the decoder's spelling: no `! value` comment after a PC-relative load, and a
/// target as `0x<address>` instead of `<address> <symbol+offset>`.
fn normalize(text: &str) -> String {
    let text = text.split("\t!").next().unwrap_or(text).trim_end();
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(" <") {
        let close = rest[open..].find('>').map_or(rest.len(), |c| open + c + 1);
        let head = &rest[..open];
        let start = head.rfind(['\t', ',']).map_or(0, |i| i + 1);
        out.push_str(&head[..start]);
        out.push_str("0x");
        out.push_str(&head[start..]);
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// The transfers whose flow takes effect after the next instruction.
fn delayed(mnemonic: &str) -> bool {
    matches!(mnemonic, "bt.s" | "bf.s") || matches!(mnemonic, "bra" | "bsr" | "braf" | "bsrf" | "jmp" | "jsr" | "rts" | "rte")
}

#[test]
fn agrees_with_sh_elf_objdump_on_the_text_section() {
    let elf = std::fs::read(fixture("control.o")).unwrap();
    let text = text_section(&elf);
    let listing = std::fs::read_to_string(fixture("control.objdump")).unwrap();
    let theirs = rows(&listing);
    assert!(theirs.len() > 500, "the listing has only {} rows", theirs.len());
    // objdump decodes in the default mode, both `fpscr` bits clear, whatever `frchg` or `fschg` ran before.
    let arch = Sh4::default();
    let mut mismatches = Vec::new();
    for row in &theirs {
        let at = usize::try_from(row.address).unwrap();
        assert_eq!(text[at..at + 2], row.bytes, "{:#x}: the listing's bytes are not control.o's", row.address);
        let ours = arch.decode(&row.bytes, row.address);
        match (&ours, row.text.starts_with(".word")) {
            (None, true) => {}
            (None, false) => mismatches.push(format!("{:#x}: ours nothing, objdump {:?}", row.address, row.text)),
            (Some(instr), _) if instr.text != row.text => {
                mismatches.push(format!("{:#x}: ours {:?}, objdump {:?}", row.address, instr.text, row.text));
            }
            (Some(instr), _) => {
                let slot = arch.delay(instr) == Delay::Slot;
                if slot != delayed(&instr.mnemonic) {
                    mismatches.push(format!("{:#x}: {} has a delay slot: {slot}", row.address, instr.mnemonic));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "{} differences from sh-elf-objdump:\n{}", mismatches.len(), mismatches.join("\n"));
}

#[test]
fn the_fpu_modes_read_the_same_word_two_ways() {
    let single = Sh4::default();
    let double = Sh4 { double: true, ..Sh4::default() };
    let pairs = Sh4 { pairs: true, ..Sh4::default() };
    // `fadd fr2,fr4` and `fadd dr2,dr4`; an odd register names no pair.
    assert_eq!(single.decode_word(0xf420, 0).unwrap().text, "fadd\tfr2,fr4");
    assert_eq!(double.decode_word(0xf420, 0).unwrap().text, "fadd\tdr2,dr4");
    assert!(double.decode_word(0xf520, 0).is_none());
    // `fmov fr3,fr0` and, with `fpscr.sz`, the pair `xd2` to `dr0`.
    assert_eq!(single.decode_word(0xf03c, 0).unwrap().text, "fmov\tfr3,fr0");
    assert_eq!(pairs.decode_word(0xf03c, 0).unwrap().text, "fmov\txd2,dr0");
    // `fipr` and `ftrv` are single-precision only.
    assert_eq!(single.decode_word(0xf4ed, 0).unwrap().text, "fipr\tfv0,fv4");
    assert_eq!(single.decode_word(0xf9fd, 0).unwrap().text, "ftrv\txmtrx,fv8");
    assert!(double.decode_word(0xf4ed, 0).is_none());
}

#[test]
fn a_delayed_transfer_continues_past_its_slot() {
    let arch = Sh4::default();
    // `bt.s 0x2e` at 4 and `bt 0x2e` at 4: the delayed form falls through past the slot at 6.
    let delayed = arch.decode_word(0x8d13, 4).unwrap();
    assert_eq!(arch.delay(&delayed), Delay::Slot);
    assert_eq!(Sh4::successors(&delayed), [0x2e, 8]);
    let at_once = arch.decode_word(0x8913, 4).unwrap();
    assert_eq!(arch.delay(&at_once), Delay::None);
    assert_eq!(Sh4::successors(&at_once), [0x2e, 6]);
}
