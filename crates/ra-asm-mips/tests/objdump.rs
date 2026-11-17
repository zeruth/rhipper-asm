//! The decoder against llvm-objdump: every word of the `.text` of the executables in
//! `testdata/mips/listings`, `testdata/mips/control.c` at four optimisation levels in both byte
//! orders, decodes to the text objdump printed for it in the committed `<name>.objdump`, and a word
//! objdump leaves `<unknown>` decodes to nothing. `listings/build.sh` made both in WSL; the test
//! reads files only, so it runs on any host.

use std::path::{Path, PathBuf};

use ra_asm_arch::Arch;
use ra_asm_mips::Mips;

fn listings() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mips/listings")
}

/// One instruction row of a listing: `   20540: 03 00 61 14  \tbne\t$3, $1, 0x20550 <control+0x314>`.
struct Row {
    address: u64,
    bytes: [u8; 4],
    text: String,
}

fn rows(listing: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for line in listing.lines() {
        // The file and symbol header lines start with no hexadecimal address before their colon.
        let Some((address, rest)) = line.trim_start().split_once(':') else { continue };
        let Ok(address) = u64::from_str_radix(address, 16) else { continue };
        let Some((bytes, text)) = rest.split_once('\t') else { continue };
        let bytes: Vec<u8> = bytes.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        let bytes: [u8; 4] = bytes.as_slice().try_into().unwrap_or_else(|_| panic!("{address:#x}: not one word: {line:?}"));
        out.push(Row { address, bytes, text: normalize(text) });
    }
    out
}

/// objdump's text in the decoder's spelling: no `<symbol+offset>` after an address or immediate,
/// `div` and `divu` with their two sources only (objdump prints `$zero` as a first operand the
/// word does not have), and a trap without its code field, which the decoder does not print.
fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text.trim_end();
    while let Some(open) = rest.find(" <") {
        out.push_str(&rest[..open]);
        rest = rest[open..].find('>').map_or("", |close| &rest[open + close + 1..]);
    }
    out.push_str(rest);
    for div in ["div", "divu"] {
        if let Some(operands) = out.strip_prefix(&format!("{div}\t$zero, ")) {
            out = format!("{div}\t{operands}");
        }
    }
    let trap = ["teq", "tne", "tge", "tgeu", "tlt", "tltu"];
    if let Some((mnemonic, operands)) = out.split_once('\t')
        && trap.contains(&mnemonic)
        && operands.matches(", ").count() == 2
        && let Some((registers, _code)) = operands.rsplit_once(", ")
    {
        out = format!("{mnemonic}\t{registers}");
    }
    out
}

#[test]
fn agrees_with_llvm_objdump_on_every_listing() {
    let mut names: Vec<PathBuf> = std::fs::read_dir(listings())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "objdump"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 8, "the listings are not the eight of build.sh: {names:?}");
    let (mut compared, mut mismatches) = (0usize, Vec::new());
    for listing in &names {
        let executable = listing.with_extension("");
        let name = executable.file_name().unwrap().to_string_lossy().into_owned();
        let elf = std::fs::read(&executable).unwrap();
        assert_eq!(&elf[..4], b"\x7fELF", "{name} is not an ELF file");
        let mips = Mips { big_endian: elf[5] == 2, ..Mips::default() };
        let image = ra_asm_pe::load(&executable).unwrap();
        let theirs = rows(&std::fs::read_to_string(listing).unwrap());
        assert!(theirs.len() > 300, "{name}'s listing has only {} rows", theirs.len());
        for row in &theirs {
            let word = &image.read_to_section_end(row.address).unwrap()[..4];
            assert_eq!(word, &row.bytes[..], "{name} {:#x}: the listing's bytes are not the executable's", row.address);
            let ours = mips.decode(word, row.address);
            match (&ours, row.text == "<unknown>") {
                (None, true) => {}
                (None, false) => mismatches.push(format!("{name} {:#x}: ours nothing, objdump {:?}", row.address, row.text)),
                (Some(instr), _) if instr.text != row.text => {
                    mismatches.push(format!("{name} {:#x}: ours {:?}, objdump {:?}", row.address, instr.text, row.text));
                }
                (Some(_), _) => {}
            }
            compared += 1;
        }
    }
    assert!(mismatches.is_empty(), "{} differences from llvm-objdump:\n{}", mismatches.len(), mismatches.join("\n"));
    assert!(compared >= 2000, "only {compared} instructions compared");
}

#[test]
fn objdumps_spellings_become_the_decoders() {
    assert_eq!(normalize("jal\t0x20dfc <fail>"), "jal\t0x20dfc");
    assert_eq!(normalize("addiu\t$sp, $sp, -0x10 <sink+0xfffffffffffcef68>"), "addiu\t$sp, $sp, -0x10");
    assert_eq!(normalize("div\t$zero, $4, $5"), "div\t$4, $5");
    assert_eq!(normalize("teq\t$5, $zero, 0x7"), "teq\t$5, $zero");
    assert_eq!(normalize("syscall"), "syscall");
}
