//! The instruction cases of the lifts that have no native oracle on this machine, as the input
//! half of a vector: the opcode an arm names, the encoding and the assembly. For `AArch64` the
//! cases come from `testdata/vectors/arm64/templates.txt`, one template a form with its operands
//! as classes, expanded over a few sample values each and assembled by `llvm-mc` in WSL
//! (`write_arm64_cases`, ignored: it needs `wsl`). `arm64_cases` replays nothing: it checks that
//! every arm of `arm64.rs` has a template, that the committed cases are the templates' expansion,
//! and that each encoding decodes back to the opcode its template names. The output half, the
//! processor's state after each case, comes from the harness in `vectors.rs`. The case model
//! (templates, classes, cases) is `common/mod.rs`, which `vectors.rs` shares.
//!
//! The 32-bit ARM cases (`testdata/vectors/arm32`) follow the same shape with the Thumb-2 lift's
//! `Op` kinds for opcodes and byte strings for encodings, an IT block and the instructions it
//! predicates being one case; the groups named `arm_*` are ARM code, assembled for the record and
//! not held against the Thumb lift.
//!
//! The 32-bit x86 cases run on this machine's own processor: `testdata/vectors/harness/x86_32.c`
//! is a program clang builds for i686 that WOW64 runs, with the forms the 64-bit instruction
//! oracle cannot (its `NOT_RUN` list and the x87 unit). `harness_runs_three_cases` (ignored: it
//! needs `clang`) builds it and checks three cases against the states written here;
//! `write_x86_32_vectors` (ignored too) rewrites `testdata/vectors/x86` from iced's 32-bit code
//! table through it, by the generator of `common/x86_32.rs`, and `write_x87_vectors` rewrites
//! `testdata/vectors/x87` the same way by the x87 generator of `common/x87.rs`.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use common::x86_32::{
    self as x86, CASE_IP32, DATA32, EAX, EBX, ESP, HarnessIn, HarnessOut, OUT32_LEN, STACK32_END, STATUS_ACCESS_VIOLATION,
    STATUS_BREAKPOINT, WINDOW32, X87Image, build_harness32, pack_in32, run_harness32, unpack_out32,
};
use common::x87;
use common::{Case, Template, arm32, arm64_dir, expand, read_cases, read_templates, write_cases, wsl_path};
use ra_asm_arm64::Arm64;
use ra_asm_arm64::thumb::{self, Op};

// region The arm list

/// The identifiers that follow `prefix` in `text`: the variants a `match` names. The prefix must
/// start a path, so `Op::` does not match inside `BinOp::`.
fn idents_after(text: &str, prefix: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    let mut consumed = 0;
    while let Some(at) = rest.find(prefix) {
        let starts_path = text[..consumed + at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let ident: String = rest[at + prefix.len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if starts_path && !ident.is_empty() {
            out.insert(ident);
        }
        consumed += at + prefix.len();
        rest = &text[consumed..];
    }
    out
}

/// The yaxpeax opcodes with an arm in `lift_instr`, read off the source as the x86 oracle reads
/// `x86.rs`: every `O::Name` between the function's head and the helper that follows it.
fn lifted_opcodes() -> BTreeSet<String> {
    let source = include_str!("../src/arm64.rs");
    let start = source.find("fn lift_instr(").expect("lift_instr");
    let end = source[start..].find("fn narrow_access(").expect("the end of lift_instr") + start;
    idents_after(&source[start..end], "O::")
}

/// The arms no template can name, each with its reason.
const NOT_ASSEMBLED: &[(&str, &str)] = &[("Invalid", "the decoder's answer to an undefined word, which has no assembly")];

/// The `Op` kinds with an arm in the Thumb-2 lift's `lift_instr`, read off `thumb.rs` the same way.
fn lifted_kinds() -> BTreeSet<String> {
    let source = include_str!("../src/thumb.rs");
    let start = source.find("fn lift_instr(").expect("lift_instr");
    let end = source[start..].find("// endregion").expect("the end of lift_instr") + start;
    idents_after(&source[start..end], "Op::")
}

/// The Thumb-2 arms no template names, each with its reason.
const NOT_TEMPLATED: &[(&str, &str)] = &[
    ("Svc", "a system call runs the host's kernel"),
    ("It", "an IT block is written with the instruction it predicates, whose kind the template names"),
];

// endregion

/// The name of the opcode yaxpeax decodes `code` to, without a condition or barrier argument.
fn decoded_opcode(code: u32) -> Option<String> {
    let decoded = Arm64.decode_full(&code.to_le_bytes(), 0)?;
    let name = format!("{:?}", decoded.inst.opcode);
    Some(name.split('(').next().unwrap_or(&name).to_owned())
}

/// The kind of the last instruction of a Thumb case, as the lift's `Op` names it: an IT block's
/// condition is carried to the instructions it predicates, as the analysis carries it.
fn decoded_kind(bytes: &[u8]) -> Option<String> {
    let mut at = 0;
    let mut it: Option<(u8, u8, usize)> = None;
    let mut last = None;
    while at < bytes.len() {
        let cond = it.map(|(first, mask, position)| thumb::it_condition_at(first, mask, position));
        let d = thumb::decode(&bytes[at..], arm32::IP + at as u64, cond)?;
        at += usize::from(d.len);
        if let Op::It { first, mask } = d.op {
            it = Some((first, mask, 0));
        } else if let Some(state) = &mut it {
            state.2 += 1;
            if state.2 >= thumb::it_len(state.1) {
                it = None;
            }
        }
        last = Some(d.op);
    }
    let name = format!("{:?}", last?);
    Some(name.split([' ', '(', '{']).next().unwrap_or(&name).to_owned())
}

// region Assembling

/// The encodings `llvm-mc` printed, one per line of input, in memory order.
fn encodings(stdout: &str, lines: usize, stderr: &str) -> Vec<Vec<u8>> {
    let codes: Vec<Vec<u8>> = stdout
        .lines()
        .filter_map(|line| line.split_once("encoding: [").map(|(_, rest)| rest))
        .map(|rest| {
            rest.trim_end_matches(']')
                .split(',')
                .map(|byte| {
                    u8::from_str_radix(byte.trim().trim_start_matches("0x"), 16)
                        .unwrap_or_else(|_| panic!("llvm-mc left a fixup in `{rest}`: a branch or literal takes an immediate target"))
                })
                .collect()
        })
        .collect();
    assert_eq!(codes.len(), lines, "llvm-mc encoded {} of {lines} lines:\n{stderr}", codes.len());
    codes
}

/// Assembles `lines` of ARM or Thumb-2 code with `llvm-mc` in WSL; the divide extension is on,
/// as the `-cpu max` of qemu has it.
fn assemble32(wsl: &Path, scratch: &Path, lines: &[String], arm: bool) -> Vec<Vec<u8>> {
    let source = scratch.join(if arm { "arm.s" } else { "thumb.s" });
    fs::write(&source, lines.join("\n") + "\n").unwrap();
    let source_wsl = wsl_path(&source);
    let (triple, attrs) =
        if arm { ("-triple=armv7-linux-gnueabi", "-mattr=+hwdiv-arm") } else { ("-triple=thumbv7-linux-gnueabi", "-mattr=+hwdiv") };
    let output = Command::new(wsl)
        .args(["-d", "Ubuntu-24.04", "--", "llvm-mc", triple, attrs, "-show-encoding", source_wsl.as_str()])
        .output()
        .expect("wsl runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "llvm-mc failed:\n{stderr}");
    encodings(&String::from_utf8_lossy(&output.stdout), lines.len(), &stderr)
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
    encodings(&String::from_utf8_lossy(&output.stdout), lines.len(), &stderr)
        .into_iter()
        .map(|bytes| {
            assert_eq!(bytes.len(), 4, "a four-byte encoding: {bytes:02x?}");
            bytes.iter().rev().fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte))
        })
        .collect()
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

// region 32-bit ARM

/// The committed 32-bit ARM cases are the templates' expansion, every arm of the Thumb-2 lift has
/// a template or a reason not to, and every Thumb case decodes to the kind its template names.
#[test]
fn arm32_cases() {
    let templates = read_templates(&arm32::arm32_dir().join("templates.txt"));
    let lifted = lifted_kinds();
    let named: BTreeSet<&str> = templates.iter().filter(|t| !arm32::is_arm_group(&t.group)).map(|t| t.opcode.as_str()).collect();
    let excused: BTreeSet<&str> = NOT_TEMPLATED.iter().map(|(name, _)| *name).collect();

    let unknown: Vec<_> = named.iter().filter(|name| !lifted.contains(**name)).collect();
    assert!(unknown.is_empty(), "templates name kinds lift_instr has no arm for: {unknown:?}");
    let stale: Vec<_> = excused.iter().filter(|name| !lifted.contains(**name) || named.contains(*name)).collect();
    assert!(stale.is_empty(), "NOT_TEMPLATED names kinds without an arm or with a template: {stale:?}");
    let missing: Vec<_> = lifted.iter().filter(|name| !named.contains(name.as_str()) && !excused.contains(name.as_str())).collect();
    assert!(missing.is_empty(), "arms of thumb.rs's lift_instr without a template in templates.txt: {missing:?}");

    let mut groups: BTreeMap<&str, Vec<&Template>> = BTreeMap::new();
    for template in &templates {
        groups.entry(template.group.as_str()).or_default().push(template);
    }
    let (mut thumb, mut arm) = (0, 0);
    for (group, templates) in &groups {
        let cases = arm32::read_cases(group);
        let expected: Vec<(&str, String)> =
            templates.iter().flat_map(|t| arm32::expand(&t.asm).into_iter().map(move |asm| (t.opcode.as_str(), asm))).collect();
        let committed: Vec<(&str, String)> = cases.iter().map(|c| (c.opcode.as_str(), c.asm.clone())).collect();
        if committed != expected {
            let first = committed.iter().zip(&expected).position(|(c, e)| c != e).unwrap_or_else(|| committed.len().min(expected.len()));
            panic!(
                "cases/{group}.cases is not the expansion of its templates (run write_arm32_cases): line {} has {:?}, the templates give {:?}",
                first + 1,
                committed.get(first),
                expected.get(first)
            );
        }
        if arm32::is_arm_group(group) {
            arm += cases.len();
            continue;
        }
        for case in &cases {
            let decoded = decoded_kind(&case.bytes);
            assert_eq!(
                decoded.as_deref(),
                Some(case.opcode.as_str()),
                "{group}: `{}` ({}) decodes to {decoded:?}, the template says {}",
                case.asm,
                arm32::hex(&case.bytes),
                case.opcode
            );
        }
        thumb += cases.len();
    }
    println!("arm32 cases: {thumb} Thumb cases and {arm} ARM cases in {} groups over {} kinds", groups.len(), named.len());
}

/// Rewrites `arm32/cases/<group>.cases` from the templates: `llvm-mc` in WSL assembles every
/// expansion, as Thumb-2 or as ARM by the group, an IT block's instructions one line each.
#[test]
#[ignore = "rewrites testdata/vectors/arm32/cases with llvm-mc in WSL"]
fn write_arm32_cases() {
    let Some(wsl) = ra_asm_testing::tools::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "arm32_cases");
    let templates = read_templates(&arm32::arm32_dir().join("templates.txt"));
    let mut groups: BTreeMap<&str, Vec<(&str, String)>> = BTreeMap::new();
    for template in &templates {
        let expanded = arm32::expand(&template.asm).into_iter().map(|asm| (template.opcode.as_str(), asm));
        groups.entry(template.group.as_str()).or_default().extend(expanded);
    }
    for arm in [false, true] {
        let selected: Vec<(&str, &Vec<(&str, String)>)> =
            groups.iter().filter(|(group, _)| arm32::is_arm_group(group) == arm).map(|(group, expanded)| (*group, expanded)).collect();
        let lines: Vec<String> =
            selected.iter().flat_map(|(_, expanded)| expanded.iter()).flat_map(|(_, asm)| asm.split("; ").map(str::to_owned)).collect();
        let mut codes = assemble32(&wsl, &scratch, &lines, arm).into_iter();
        for (group, expanded) in selected {
            let cases: Vec<arm32::Case> = expanded
                .iter()
                .map(|(kind, asm)| {
                    let bytes = asm.split("; ").flat_map(|_| codes.next().expect("an encoding per instruction")).collect();
                    arm32::Case { opcode: (*kind).to_owned(), bytes, asm: asm.clone() }
                })
                .collect();
            arm32::write_cases(group, &cases);
            println!("arm32/cases/{group}.cases: {} cases", cases.len());
        }
    }
}

// endregion

// region The 32-bit x86 harness

/// The harness gives the states of `add eax, ebx`, `pushad` and a load from an unmapped address:
/// the breakpoint after the first two with their flags, registers and stack bytes, the access
/// violation of the third at the case's own address with every register as it was.
#[test]
#[ignore = "builds testdata/vectors/harness/x86_32.c with clang and runs it under WOW64"]
fn harness_runs_three_cases() {
    let Some(clang) = ra_asm_testing::tools::tool("clang") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-lift-tests", "harness32");
    let exe = build_harness32(&clang, &scratch);
    let esp = STACK32_END - 0x100;
    let zeros = [0u8; WINDOW32];
    let mut regs = [0u32; 8];
    regs[ESP] = esp;

    let x87 = X87Image::init();
    let mut add = HarnessIn { r: regs, eflags: 0, code: vec![0x01, 0xD8], window_at: DATA32, window: zeros, x87 };
    add.r[EAX] = 1;
    add.r[EBX] = 2;
    let pushed = [0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444, esp, 0x5555_5555, 0x6666_6666, 0x7777_7777];
    let pushad = HarnessIn { r: pushed, eflags: 0, code: vec![0x60], window_at: esp - 32, window: zeros, x87 };
    let load = HarnessIn { r: regs, eflags: 0, code: vec![0xA1, 0, 0, 0, 0], window_at: DATA32, window: zeros, x87 };

    let mut blob = Vec::new();
    for case in [&add, &pushad, &load] {
        pack_in32(&mut blob, case);
    }
    let (status, results) = run_harness32(&exe, &scratch, "three", &blob);
    assert!(status.success(), "the harness failed: {status}");
    assert_eq!(results.len(), 3 * OUT32_LEN, "the harness answered {} bytes", results.len());
    let outs: Vec<HarnessOut> = results.chunks_exact(OUT32_LEN).map(unpack_out32).collect();

    // 1 + 2 = 3: two bits set in the low byte, so PF alone. The unit comes back as it went in: the
    // integer cases leave it initialized.
    let mut after_add = regs;
    after_add[EAX] = 3;
    after_add[EBX] = 2;
    assert_eq!(
        outs[0],
        HarnessOut { code: STATUS_BREAKPOINT, eip: CASE_IP32 + 2, fault_at: 0, eflags: 1 << 2, r: after_add, stray: 0, window: zeros, x87 },
        "add eax, ebx"
    );

    // pushad stores eax first at the top, so the window from the new esp reads edi up to eax, and
    // the esp it stores is the one before the push.
    let mut after_pushad = pushed;
    after_pushad[ESP] = esp - 32;
    let mut window = zeros;
    for (i, value) in pushed.iter().rev().enumerate() {
        window[4 * i..4 * i + 4].copy_from_slice(&value.to_le_bytes());
    }
    assert_eq!(
        outs[1],
        HarnessOut { code: STATUS_BREAKPOINT, eip: CASE_IP32 + 1, fault_at: 0, eflags: 0, r: after_pushad, stray: 0, window, x87 },
        "pushad"
    );

    assert_eq!(
        outs[2],
        HarnessOut { code: STATUS_ACCESS_VIOLATION, eip: CASE_IP32, fault_at: 0, eflags: 0, r: regs, stray: 0, window: zeros, x87 },
        "mov eax, [0]"
    );
    println!("x86_32 harness: 3 cases as expected");
}

/// Rewrites `testdata/vectors/x87/<group>.vec` from iced's 32-bit code table: every x87 form over
/// the generator's stacks and states, run by the harness under WOW64. The `x87` test of
/// `vectors.rs` then says which forms differ, for `PENDING.txt`.
#[test]
#[ignore = "rewrites testdata/vectors/x87/*.vec with the harness under WOW64"]
fn write_x87_vectors() {
    let Some(clang) = ra_asm_testing::tools::tool("clang") else { return };
    let written = x87::write_vectors(&clang, &x87::x87_dir());
    for (group, n) in &written {
        println!("x87/{group}.vec: {n} cases");
    }
    println!("x87 vectors: {} cases in {} groups", written.values().sum::<usize>(), written.len());
}

/// Rewrites `testdata/vectors/x86/<group>.vec` from iced's 32-bit code table: every integer
/// form the lift has an arm for over the generator's states, run by the harness under WOW64.
/// The `x86_32` test of `vectors.rs` then says which forms differ, for `PENDING.txt`.
#[test]
#[ignore = "rewrites testdata/vectors/x86/*.vec with the harness under WOW64"]
fn write_x86_32_vectors() {
    let Some(clang) = ra_asm_testing::tools::tool("clang") else { return };
    let written = x86::write_vectors(&clang, &x86::x86_dir());
    for (group, n) in &written {
        println!("x86/{group}.vec: {n} cases");
    }
    println!("x86_32 vectors: {} cases in {} groups", written.values().sum::<usize>(), written.len());
}

// endregion
