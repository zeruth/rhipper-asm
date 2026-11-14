//! The `AArch64` case model the vector tests share: the templates of
//! `testdata/vectors/arm64/templates.txt`, their expansion over the operand classes, the committed
//! `cases/<group>.cases` files, and the path WSL sees for a Windows path. `vectors_gen.rs` writes
//! and checks the cases; `vectors.rs` runs them under the harness and replays the vectors. The
//! 32-bit ARM model, with its own classes and byte-string cases, is `arm32`; the 32-bit x86
//! generator, which runs iced's code table through the WOW64 harness, is `x86_32`, and the x87
//! generator over the same harness is `x87`. The random states of every generator come from
//! [`Rng`], seeded per group.
#![allow(dead_code)]

pub(crate) mod arm32;
pub(crate) mod x86_32;
pub(crate) mod x87;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// The directory of the `AArch64` templates, cases and vectors.
pub(crate) fn arm64_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/arm64")
}

// region Templates

/// The sample values of each operand class, by the name a template writes in braces. The lists
/// are co-indexed: sample `k` of a template takes value `k` of each class, modulo its length, so
/// a template with four-valued classes gives four cases and one with `{cond}` fourteen. The
/// register lists of one role never collide with those of another, so a writeback never names
/// its own data register, and each list has the zero register or the stack pointer where the
/// encoding allows one.
pub(crate) const CLASSES: &[(&str, &str)] = &[
    ("Xd", "x0 x5 x17 x30"),
    ("Xn", "x1 x9 xzr x22"),
    ("Xm", "x2 x13 x28 xzr"),
    ("Xa", "x3 x11 x19 x27"),
    ("Xi", "x2 x13 x28 x7"),
    ("XdSP", "x0 sp x17 x30"),
    ("XnSP", "x1 sp x9 x22"),
    ("Wd", "w0 w5 w17 w30"),
    ("Wn", "w1 w9 wzr w22"),
    ("Wm", "w2 w13 w28 wzr"),
    ("Wa", "w3 w11 w19 w27"),
    ("Wi", "w2 w13 w28 w7"),
    ("WdSP", "w0 wsp w17 w30"),
    ("WnSP", "w1 wsp w9 w22"),
    ("Dd", "d0 d5 d17 d31"),
    ("Dn", "d1 d9 d20 d31"),
    ("Dm", "d2 d13 d28 d6"),
    ("Da", "d3 d11 d19 d27"),
    ("Sd", "s0 s5 s17 s31"),
    ("Sn", "s1 s9 s20 s31"),
    ("Sm", "s2 s13 s28 s6"),
    ("Sa", "s3 s11 s19 s27"),
    ("Qd", "q0 q5 q17 q31"),
    ("Qa", "q3 q11 q19 q27"),
    ("Vd", "v0 v5 v17 v31"),
    ("Vn", "v1 v9 v20 v31"),
    ("Vm", "v2 v13 v28 v6"),
    ("cond", "eq ne cs cc mi pl vs vc hi ls ge lt gt le"),
    ("barrier", "sy ish ishld nshst"),
    ("hint", "0 1 2 5"),
    ("imm16", "0 0xffff 0x1234 0x8000"),
    ("hw64", "0 16 32 48"),
    ("hw32", "0 16"),
    ("imm12", "0 1 0x7ff 0xfff"),
    ("shift6", "0 1 31 63"),
    ("shift5", "0 1 7 31"),
    ("imm3", "0 1 2 3"),
    ("imm5", "0 1 12 31"),
    ("nzcv", "0 15 4 9"),
    ("bimm64", "0xff 0x5555555555555555 0xffffffff00000000 0x1"),
    ("bimm32", "0xff 0x55555555 0xffff0000 0x1"),
    // Co-indexed with the next list: a whole-register field, a field in the middle, one placed
    // above the bottom and a shift by one, so both branches of each bitfield arm are reached.
    ("immr64", "0 4 32 63"),
    ("imms64", "63 11 7 0"),
    ("immr32", "0 4 16 31"),
    ("imms32", "31 11 7 0"),
    ("label", "8 -8 1024 -4096"),
    ("adr", "0 1 -1 0xfffff"),
    ("adrp", "0x1000 0x2000 -0x1000 0x7ffff000"),
    ("simm9", "-256 -1 0 255"),
    ("pimm1", "0 1 8 4095"),
    ("pimm2", "0 2 8 8190"),
    ("pimm4", "0 4 16 16380"),
    ("pimm8", "0 8 16 32760"),
    ("pimm16", "0 16 32 65520"),
    ("simm7x4", "-256 -8 8 252"),
    ("simm7x8", "-512 -16 16 504"),
    ("simm7x16", "-1024 -32 32 1008"),
    ("bit64", "0 31 32 63"),
    ("bit32", "0 1 15 31"),
    ("fimm", "1.0 0.5 -2.0 31.0"),
    ("movi64", "0xff00ff00ff00ff00 0 0xffffffffffffffff 0x00ff0000ff0000ff"),
    ("movi8", "0 1 0x7f 0xff"),
];

pub(crate) fn class_values(name: &str) -> Vec<&'static str> {
    class_values_in(CLASSES, name)
}

/// The sample values of class `name` in `classes`: separated by whitespace, or by `|` when a
/// value has spaces of its own (a register list).
pub(crate) fn class_values_in(classes: &[(&str, &'static str)], name: &str) -> Vec<&'static str> {
    let (_, values) = classes.iter().find(|(class, _)| *class == name).unwrap_or_else(|| panic!("no operand class {{{name}}}"));
    if values.contains('|') { values.split('|').map(str::trim).collect() } else { values.split_whitespace().collect() }
}

/// One line of `templates.txt`: the group it is under, the opcode and the assembly with classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Template {
    pub(crate) group: String,
    pub(crate) opcode: String,
    pub(crate) asm: String,
}

pub(crate) fn read_templates(path: &Path) -> Vec<Template> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut group = String::new();
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            group = name.to_owned();
            continue;
        }
        assert!(!group.is_empty(), "templates.txt:{}: a template before the first [group]", number + 1);
        let (opcode, asm) = line.split_once(char::is_whitespace).unwrap_or_else(|| panic!("templates.txt:{}: no assembly", number + 1));
        out.push(Template { group: group.clone(), opcode: opcode.to_owned(), asm: asm.trim().to_owned() });
    }
    out
}

/// The classes a template names, in order of appearance and with repeats.
pub(crate) fn classes_of(asm: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = asm;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}').expect("a closing brace") + open;
        out.push(&rest[open + 1..close]);
        rest = &rest[close + 1..];
    }
    out
}

/// The template's assembly lines, one per sample.
pub(crate) fn expand(asm: &str) -> Vec<String> {
    expand_with(asm, CLASSES)
}

/// The template's assembly lines over the classes of `table`, one per sample.
pub(crate) fn expand_with(asm: &str, table: &[(&str, &'static str)]) -> Vec<String> {
    let classes: Vec<(&str, Vec<&str>)> = classes_of(asm).into_iter().map(|name| (name, class_values_in(table, name))).collect();
    let samples = classes.iter().map(|(_, values)| values.len()).max().unwrap_or(1);
    (0..samples)
        .map(|k| {
            let mut text = asm.to_owned();
            for (name, values) in &classes {
                text = text.replacen(&format!("{{{name}}}"), values[k % values.len()], 1);
            }
            text
        })
        .collect()
}

// endregion

// region Cases

/// One line of a `cases/<group>.cases` file: the opcode, the 32-bit encoding and the assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Case {
    pub(crate) opcode: String,
    pub(crate) code: u32,
    pub(crate) asm: String,
}

pub(crate) fn cases_file(group: &str) -> PathBuf {
    arm64_dir().join("cases").join(format!("{group}.cases"))
}

pub(crate) fn read_cases(group: &str) -> Vec<Case> {
    let path = cases_file(group);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(number, line)| {
            let mut words = line.split_whitespace();
            let (Some(opcode), Some(code)) = (words.next(), words.next()) else {
                panic!("{}:{}: not `opcode code asm`", path.display(), number + 1)
            };
            let code = u32::from_str_radix(code, 16).unwrap_or_else(|e| panic!("{}:{}: {code}: {e}", path.display(), number + 1));
            let asm = words.collect::<Vec<_>>().join(" ");
            assert!(!asm.is_empty(), "{}:{}: no assembly", path.display(), number + 1);
            Case { opcode: opcode.to_owned(), code, asm }
        })
        .collect()
}

pub(crate) fn write_cases(group: &str, cases: &[Case]) {
    let mut text = String::new();
    for case in cases {
        writeln!(text, "{} {:08x} {}", case.opcode, case.code, case.asm).unwrap();
    }
    let path = cases_file(group);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

// endregion

/// The path WSL sees for a Windows path: `C:\a\b` is `/mnt/c/a/b`.
pub(crate) fn wsl_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.as_bytes() {
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic() => format!("/mnt/{}{}", drive.to_ascii_lowercase() as char, &text[2..]),
        _ => text,
    }
}

// region Random states

/// xorshift64*, as the x86 oracle's, so a group's states are the same on every regeneration.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub(crate) fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }

    pub(crate) fn chance(&mut self, out_of: u64) -> bool {
        self.below(out_of) == 0
    }

    /// A value with interesting shapes as often as a uniform one: small, all ones, a sign bit,
    /// a power of two, a byte pattern.
    pub(crate) fn value(&mut self) -> u64 {
        match self.below(8) {
            0 => self.below(16),
            1 => u64::MAX - self.below(16),
            2 => 1u64 << self.below(64),
            3 => (1u64 << self.below(64)).wrapping_sub(1),
            4 => 0x8000_0000_0000_0000 | self.below(16),
            5 => 0x7FFF_FFFF_FFFF_FFF0 | self.below(16),
            _ => self.next(),
        }
    }

    /// [`Self::value`] at 32 bits: the same shapes over a 32-bit register.
    pub(crate) fn value32(&mut self) -> u32 {
        match self.below(8) {
            0 => self.below(16) as u32,
            1 => u32::MAX - self.below(16) as u32,
            2 => 1u32 << self.below(32),
            3 => (1u32 << self.below(32)).wrapping_sub(1),
            4 => 0x8000_0000 | self.below(16) as u32,
            5 => 0x7FFF_FFF0 | self.below(16) as u32,
            _ => self.next() as u32,
        }
    }

    /// A finite double between about 1e-9 and 1e9 in magnitude: no NaN, no infinity, no
    /// denormal, so the arithmetic forms compare values and not the machine's NaN rules.
    pub(crate) fn f64_bits(&mut self, positive: bool) -> u64 {
        let sign = if positive { 0 } else { self.below(2) };
        let exponent = 1023 + self.below(61) - 30;
        let mantissa = self.next() & ((1 << 52) - 1);
        (sign << 63) | (exponent << 52) | mantissa
    }

    pub(crate) fn f32_bits(&mut self, positive: bool) -> u64 {
        let sign = if positive { 0 } else { self.below(2) };
        let exponent = 127 + self.below(41) - 20;
        let mantissa = self.next() & ((1 << 23) - 1);
        (sign << 31) | (exponent << 23) | mantissa
    }

    pub(crate) fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let v = self.next().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}

/// The seed of a group's states: FNV-1a of its name over a nonzero offset.
pub(crate) fn group_seed(group: &str) -> u64 {
    group.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

// endregion
