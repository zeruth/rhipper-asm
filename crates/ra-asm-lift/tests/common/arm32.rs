//! The 32-bit ARM case model: the classes of `testdata/vectors/arm32/templates.txt`, whose groups
//! are Thumb-2 code unless named `arm_*`, and the committed `cases/<group>.cases` files, whose
//! encoding is a byte string in memory order (two to ten bytes: a halfword, a word, or an IT
//! block with the instructions it predicates).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use super::expand_with;

/// The directory of the 32-bit ARM templates, cases and vectors.
pub(crate) fn arm32_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/arm32")
}

/// The address of the case instruction, as the harness and the replay place it.
pub(crate) const IP: u64 = 0x3000_2000;

/// Whether the group's cases are ARM code; every other group is Thumb-2.
pub(crate) fn is_arm_group(group: &str) -> bool {
    group.starts_with("arm_")
}

/// The sample values of each operand class, co-indexed as the `AArch64` ones are. The low
/// register classes (`L*`) stay in r0..r7 for the 16-bit encodings; the register lists of one
/// role never collide with those of another, so a base is never in its own list and a writeback
/// never names its data register; `ncond` is the inverse of `cond`, position by position, for
/// the `else` of an IT block.
pub(crate) const CLASSES: &[(&str, &str)] = &[
    ("Ld", "r0 r3 r5 r7"),
    ("Ln", "r1 r4 r6 r2"),
    ("Lm", "r2 r7 r0 r5"),
    ("Lt", "r0 r3 r5 r7"),
    ("Lb", "r1 r4 r6 r2"),
    ("Li", "r2 r7 r0 r5"),
    ("Rd", "r0 r5 r9 r12"),
    ("Rn", "r1 r8 r11 r14"),
    ("Rm", "r2 r10 r12 r6"),
    ("Ra", "r3 r4 r7 r11"),
    ("Rs", "r3 r9 r6 r12"),
    ("Rt", "r0 r5 r9 r12"),
    ("Rt2", "r3 r4 r7 r11"),
    ("Rb", "r1 r8 r11 r4"),
    ("Ri", "r2 r10 r12 r6"),
    ("Re", "r0 r4 r6 r10"),
    ("Ro", "r1 r5 r7 r11"),
    ("Rbd", "r2 r8 r12 r3"),
    ("cond", "eq ne cs cc mi pl vs vc hi ls ge lt gt le"),
    ("ncond", "ne eq cc cs pl mi vc vs ls hi lt ge le gt"),
    ("imm3", "0 1 7 4"),
    ("imm8", "0 1 0xff 0x80"),
    ("imm7x4", "0 4 508 256"),
    ("imm8x4", "0 4 1020 512"),
    ("modimm", "0 0xff 0xff00ff00 0x80000000"),
    // The ARM modified immediates: a byte rotated, so no repeated-byte shape.
    ("aimm", "0 0xff 0xff00 0x80000000"),
    ("imm12", "0 1 0xfff 0x800"),
    ("imm16", "0 0xffff 0x1234 0x8000"),
    ("shift5", "1 7 31 16"),
    ("shift5z", "0 1 31 16"),
    ("shift5s", "1 7 31 32"),
    ("sh2", "0 1 2 3"),
    ("rot", "0 8 16 24"),
    // Co-indexed: a whole register, a byte in the middle, a halfword above the bottom, the top bit.
    ("lsb", "0 4 16 31"),
    ("width", "32 8 16 1"),
    ("off5", "0 1 16 31"),
    ("off5x2", "0 2 32 62"),
    ("off5x4", "0 4 64 124"),
    ("off8", "-255 -8 0 255"),
    ("off8x4", "0 4 128 252"),
    ("dual8", "-252 -8 0 252"),
    ("off12", "0 4 0x7ff 0xfff"),
    ("off12n", "-4 -8 -0x7ff -0xfff"),
    ("dual", "-1020 -8 0 1020"),
    ("dualw", "-1020 -8 8 1020"),
    ("label11", "8 -8 1024 -2048"),
    ("label8", "8 -8 254 -256"),
    ("label20", "8 -8 2048 -2048"),
    // A branch predicated by an IT block sits at IP + 2, and one of ARM code reads its PC as
    // IP + 8: their backward targets must miss the prologue's middle (a loop back to the case)
    // and the case itself.
    ("itlabel", "8 -16 1024 -2048"),
    ("alabel", "8 -16 2048 -2048"),
    ("cb", "0 8 64 126"),
    ("adr8", "0 8 512 1020"),
    ("adr12", "8 -8 1024 -1024"),
    ("lit8", "0 8 512 1020"),
    ("lit12", "8 -8 1024 -1024"),
    ("list", "{r0, r3} | {r3, r5, r7} | {r5} | {r0, r3, r5, r7}"),
    ("listpc", "{r5, pc} | {pc} | {r0, r3, r7, pc} | {r3, r5, pc}"),
    ("listlr", "{r5, lr} | {lr} | {r0, r3, r7, lr} | {r3, r5, lr}"),
    ("wlist", "{r9, r10} | {r0, r12} | {r2, r5, r9} | {r3, r7, r10, r12}"),
    ("wlistpc", "{r9, pc} | {r0, r12, pc} | {r2, r5, r9, pc} | {r10, pc}"),
    ("wlistlr", "{r9, lr} | {r0, r12, lr} | {r2, r5, r9, lr} | {r10, lr}"),
    ("bkpt", "0 0x55 0xff 1"),
];

/// The template's assembly lines, one per sample.
pub(crate) fn expand(asm: &str) -> Vec<String> {
    expand_with(asm, CLASSES)
}

/// One line of a `cases/<group>.cases` file: the kind, the encoding's bytes in memory order and
/// the assembly, whose instructions an IT block separates with `; `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Case {
    pub(crate) opcode: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) asm: String,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The bytes a hex string names, in order.
pub(crate) fn parse_hex(text: &str, where_: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0 && !text.is_empty(), "{where_}: {text} is not a byte string");
    text.as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap_or_else(|e| panic!("{where_}: {text}: {e}")))
        .collect()
}

pub(crate) fn cases_file(group: &str) -> PathBuf {
    arm32_dir().join("cases").join(format!("{group}.cases"))
}

pub(crate) fn read_cases(group: &str) -> Vec<Case> {
    let path = cases_file(group);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(number, line)| {
            let where_ = format!("{}:{}", path.display(), number + 1);
            let mut words = line.split_whitespace();
            let (Some(opcode), Some(code)) = (words.next(), words.next()) else { panic!("{where_}: not `kind bytes asm`") };
            let asm = words.collect::<Vec<_>>().join(" ");
            assert!(!asm.is_empty(), "{where_}: no assembly");
            Case { opcode: opcode.to_owned(), bytes: parse_hex(code, &where_), asm }
        })
        .collect()
}

pub(crate) fn write_cases(group: &str, cases: &[Case]) {
    let mut text = String::new();
    for case in cases {
        writeln!(text, "{} {} {}", case.opcode, hex(&case.bytes), case.asm).unwrap();
    }
    let path = cases_file(group);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
