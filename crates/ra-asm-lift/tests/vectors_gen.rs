//! The instruction cases of the lifts that have no native oracle on this machine, as the input
//! half of a vector: the opcode an arm names, the encoding and the assembly. For `AArch64` the
//! cases come from `testdata/vectors/arm64/templates.txt`, one template a form with its operands
//! as classes, expanded over a few sample values each and assembled by `llvm-mc` in WSL
//! (`write_arm64_cases`, ignored: it needs `wsl`). `arm64_cases` replays nothing: it checks that
//! every arm of `arm64.rs` has a template, that the committed cases are the templates' expansion,
//! and that each encoding decodes back to the opcode its template names. The output half, the
//! processor's state after each case, comes from the harness in `vectors.rs`. The case model
//! (templates, classes, cases) is `common/mod.rs`, which `vectors.rs` shares.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use common::{Case, Template, arm64_dir, expand, read_cases, read_templates, write_cases, wsl_path};
use ra_asm_arm64::Arm64;

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

/// The name of the opcode yaxpeax decodes `code` to, without a condition or barrier argument.
fn decoded_opcode(code: u32) -> Option<String> {
    let decoded = Arm64.decode_full(&code.to_le_bytes(), 0)?;
    let name = format!("{:?}", decoded.inst.opcode);
    Some(name.split('(').next().unwrap_or(&name).to_owned())
}

// region Assembling

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
