//! The instruction cases of the lifts that have no native oracle on this machine, as the input
//! half of a vector: the opcode an arm names, the encoding and the assembly. For `AArch64` the
//! cases come from `testdata/vectors/arm64/templates.txt`, one template a form with its operands
//! as classes, expanded over a few sample values each and assembled by `llvm-mc` in WSL
//! (`write_arm64_cases`, ignored: it needs `wsl`). `arm64_cases` replays nothing: it checks that
//! every arm of `arm64.rs` has a template, that the committed cases are the templates' expansion,
//! and that each encoding decodes back to the opcode its template names. The output half, the
//! processor's state after each case, is the harness's and comes with the replay.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_arm64::Arm64;

/// The directory of the `AArch64` templates and cases.
fn arm64_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/arm64")
}

// region The arm list

/// The yaxpeax opcodes with an arm in `lift_instr`, read off the source as the x86 oracle reads
/// `x86.rs`: every `O::Name` between the function's head and the helper that follows it.
fn lifted_opcodes() -> BTreeSet<String> {
    let source = include_str!("../src/arm64.rs");
    let start = source.find("fn lift_instr(").expect("lift_instr");
    let end = source[start..].find("fn narrow_access(").expect("the end of lift_instr") + start;
    let mut out = BTreeSet::new();
    let mut rest = &source[start..end];
    while let Some(at) = rest.find("O::") {
        let ident: String = rest[at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !ident.is_empty() {
            out.insert(ident);
        }
        rest = &rest[at + 3..];
    }
    out
}

/// The arms no template can name, each with its reason.
const NOT_ASSEMBLED: &[(&str, &str)] = &[("Invalid", "the decoder's answer to an undefined word, which has no assembly")];

// endregion

// region Templates

/// The sample values of each operand class, by the name a template writes in braces. The lists
/// are co-indexed: sample `k` of a template takes value `k` of each class, modulo its length, so
/// a template with four-valued classes gives four cases and one with `{cond}` fourteen. The
/// register lists of one role never collide with those of another, so a writeback never names
/// its own data register, and each list has the zero register or the stack pointer where the
/// encoding allows one.
const CLASSES: &[(&str, &str)] = &[
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

fn class_values(name: &str) -> Vec<&'static str> {
    let (_, values) = CLASSES.iter().find(|(class, _)| *class == name).unwrap_or_else(|| panic!("no operand class {{{name}}}"));
    values.split_whitespace().collect()
}

/// One line of `templates.txt`: the group it is under, the opcode and the assembly with classes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Template {
    group: String,
    opcode: String,
    asm: String,
}

fn read_templates(path: &Path) -> Vec<Template> {
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
fn classes_of(asm: &str) -> Vec<&str> {
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
fn expand(asm: &str) -> Vec<String> {
    let classes: Vec<(&str, Vec<&str>)> = classes_of(asm).into_iter().map(|name| (name, class_values(name))).collect();
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
struct Case {
    opcode: String,
    code: u32,
    asm: String,
}

fn cases_file(group: &str) -> PathBuf {
    arm64_dir().join("cases").join(format!("{group}.cases"))
}

fn read_cases(group: &str) -> Vec<Case> {
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

fn write_cases(group: &str, cases: &[Case]) {
    let mut text = String::new();
    for case in cases {
        writeln!(text, "{} {:08x} {}", case.opcode, case.code, case.asm).unwrap();
    }
    let path = cases_file(group);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The name of the opcode yaxpeax decodes `code` to, without a condition or barrier argument.
fn decoded_opcode(code: u32) -> Option<String> {
    let decoded = Arm64.decode_full(&code.to_le_bytes(), 0)?;
    let name = format!("{:?}", decoded.inst.opcode);
    Some(name.split('(').next().unwrap_or(&name).to_owned())
}

// endregion

// region Assembling

/// The path WSL sees for a Windows path: `C:\a\b` is `/mnt/c/a/b`.
fn wsl_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.as_bytes() {
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic() => format!("/mnt/{}{}", drive.to_ascii_lowercase() as char, &text[2..]),
        _ => text,
    }
}

/// Assembles `lines` with `llvm-mc` in WSL and returns their encodings, in order. Every
/// architecture extension is on, so a `bc.cond` or a pointer-authentication hint assembles.
fn assemble(wsl: &Path, scratch: &Path, lines: &[String]) -> Vec<u32> {
    let source = scratch.join("cases.s");
    fs::write(&source, lines.join("\n") + "\n").unwrap();
    let source_wsl = wsl_path(&source);
    let output = Command::new(wsl)
        .args(["-d", "Ubuntu-24.04", "--", "llvm-mc", "-triple=aarch64", "-mattr=+all", "-show-encoding", source_wsl.as_str()])
        .output()
        .expect("wsl runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "llvm-mc failed:\n{stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let codes: Vec<u32> = stdout
        .lines()
        .filter_map(|line| line.split_once("// encoding: [").map(|(_, rest)| rest))
        .map(|rest| {
            let bytes: Vec<u32> = rest
                .trim_end_matches(']')
                .split(',')
                .map(|byte| u32::from_str_radix(byte.trim().trim_start_matches("0x"), 16).expect("an encoding byte"))
                .collect();
            assert_eq!(bytes.len(), 4, "a four-byte encoding: {rest}");
            bytes.iter().rev().fold(0, |acc, byte| (acc << 8) | byte)
        })
        .collect();
    assert_eq!(codes.len(), lines.len(), "llvm-mc encoded {} of {} lines:\n{stderr}", codes.len(), lines.len());
    codes
}

// endregion

/// The committed cases are the templates' expansion, every arm of `arm64.rs` has a template or
/// a reason not to, and every encoding decodes to the opcode its template names.
#[test]
fn arm64_cases() {
    let templates = read_templates(&arm64_dir().join("templates.txt"));
    let lifted = lifted_opcodes();
    let named: BTreeSet<&str> = templates.iter().map(|t| t.opcode.as_str()).collect();
    let excused: BTreeSet<&str> = NOT_ASSEMBLED.iter().map(|(name, _)| *name).collect();

    let unknown: Vec<_> = named.iter().filter(|name| !lifted.contains(**name)).collect();
    assert!(unknown.is_empty(), "templates name opcodes lift_instr has no arm for: {unknown:?}");
    let stale: Vec<_> = excused.iter().filter(|name| !lifted.contains(**name) || named.contains(*name)).collect();
    assert!(stale.is_empty(), "NOT_ASSEMBLED names opcodes without an arm or with a template: {stale:?}");
    let missing: Vec<_> = lifted.iter().filter(|name| !named.contains(name.as_str()) && !excused.contains(name.as_str())).collect();
    assert!(missing.is_empty(), "arms of lift_instr without a template in templates.txt: {missing:?}");

    let mut groups: BTreeMap<&str, Vec<&Template>> = BTreeMap::new();
    for template in &templates {
        groups.entry(template.group.as_str()).or_default().push(template);
    }
    let mut total = 0;
    for (group, templates) in &groups {
        let cases = read_cases(group);
        let expected: Vec<(&str, String)> =
            templates.iter().flat_map(|t| expand(&t.asm).into_iter().map(move |asm| (t.opcode.as_str(), asm))).collect();
        let committed: Vec<(&str, String)> = cases.iter().map(|c| (c.opcode.as_str(), c.asm.clone())).collect();
        if committed != expected {
            let first = committed.iter().zip(&expected).position(|(c, e)| c != e).unwrap_or_else(|| committed.len().min(expected.len()));
            panic!(
                "cases/{group}.cases is not the expansion of its templates (run write_arm64_cases): line {} has {:?}, the templates give {:?}",
                first + 1,
                committed.get(first),
                expected.get(first)
            );
        }
        for case in &cases {
            let decoded = decoded_opcode(case.code);
            assert_eq!(
                decoded.as_deref(),
                Some(case.opcode.as_str()),
                "{group}: `{}` ({:08x}) decodes to {decoded:?}, the template says {}",
                case.asm,
                case.code,
                case.opcode
            );
        }
        total += cases.len();
    }
    println!("arm64 cases: {total} cases in {} groups over {} opcodes", groups.len(), named.len());
}

/// Rewrites `cases/<group>.cases` from the templates: `llvm-mc` in WSL assembles every expansion.
#[test]
#[ignore = "rewrites testdata/vectors/arm64/cases with llvm-mc in WSL"]
fn write_arm64_cases() {
    let Some(wsl) = ra_asm_testing::tools::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "arm64_cases");
    let templates = read_templates(&arm64_dir().join("templates.txt"));
    let mut groups: BTreeMap<&str, Vec<(&str, String)>> = BTreeMap::new();
    for template in &templates {
        let expanded = expand(&template.asm).into_iter().map(|asm| (template.opcode.as_str(), asm));
        groups.entry(template.group.as_str()).or_default().extend(expanded);
    }
    let lines: Vec<String> = groups.values().flatten().map(|(_, asm)| asm.clone()).collect();
    let mut codes = assemble(&wsl, &scratch, &lines).into_iter();
    for (group, expanded) in &groups {
        let cases: Vec<Case> = expanded
            .iter()
            .zip(codes.by_ref())
            .map(|((opcode, asm), code)| Case { opcode: (*opcode).to_owned(), code, asm: asm.clone() })
            .collect();
        write_cases(group, &cases);
        println!("cases/{group}.cases: {} cases", cases.len());
    }
}
