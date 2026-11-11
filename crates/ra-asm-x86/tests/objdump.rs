//! The decoder against `llvm-objdump` on the `.text` of every fixture: the same instruction
//! boundaries, and the same mnemonic up to known spelling differences. It skips when the tool is
//! not on the PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_arch::Arch;
use ra_asm_x86::X86;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// The rhtest programs: MSVC's and clang-cl's x86 code over the MSVC C runtime.
fn rhtest_binaries() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest");
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "exe")))
        .collect();
    out.sort();
    out
}

/// `llvm-objdump` prints `140001000: mov eax, ecx` per instruction, with tabs between the parts.
fn objdump(path: &Path) -> Option<Vec<(u64, String)>> {
    let output =
        Command::new("llvm-objdump").args(["-d", "--no-show-raw-insn", "-M", "intel", "--section=.text"]).arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();
    for line in text.lines() {
        let Some((addr, rest)) = line.trim_start().split_once(':') else { continue };
        // The file header line starts with the path, whose drive letter is a hexadecimal digit too.
        if addr.len() < 6 {
            continue;
        }
        let Ok(address) = u64::from_str_radix(addr, 16) else { continue };
        let Some(first) = rest.split_whitespace().next() else { continue };
        // objdump prints a `lock` prefix as a row of its own; iced keeps it on the instruction.
        if let Some((_, last)) = rows.last_mut()
            && *last == "lock"
        {
            first.clone_into(last);
            continue;
        }
        rows.push((address, first.to_owned()));
    }
    Some(rows)
}

/// The spellings where iced and objdump differ for the same instruction.
fn same_mnemonic(ours: &str, theirs: &str) -> bool {
    if ours == theirs {
        return true;
    }
    // `66 90` is `xchg ax, ax` to iced and `nop` to objdump: the same two bytes.
    matches!((ours, theirs), ("mov", "movabs") | ("call", "callq") | ("ret", "retq") | ("jmp", "jmpq") | ("xchg", "nop"))
}

#[test]
fn agrees_with_llvm_objdump_on_the_text_section() {
    let names = ["hello64.exe", "hello32.exe", "control64.exe", "control32.exe", "exports64.dll", "exports32.dll"];
    let paths: Vec<PathBuf> = names.iter().map(|n| fixture(n)).chain(rhtest_binaries()).collect();
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy();
        let Some(theirs) = objdump(path) else {
            eprintln!("llvm-objdump is not on the PATH: the oracle comparison is skipped");
            return;
        };
        let image = ra_asm_pe::load(path).unwrap();
        let text = image.sections.iter().find(|s| s.name == ".text").unwrap();
        let start = image.va(text.virtual_address);
        let bytes = image.read_to_section_end(start).unwrap();
        let arch = X86::for_pointer_size(image.pointer_size());
        let ours = arch.decode_all(bytes, start);
        // The first boundary that differs, with both readings around it.
        let divergence = ours
            .iter()
            .zip(&theirs)
            .position(|(i, (a, _))| i.address != *a)
            .or_else(|| (ours.len() != theirs.len()).then(|| ours.len().min(theirs.len())));
        if let Some(at) = divergence {
            let from = at.saturating_sub(2);
            let ours_text: Vec<String> =
                ours[from..(at + 3).min(ours.len())].iter().map(|i| format!("{:#x}: {}", i.address, i.text)).collect();
            let theirs_text: Vec<String> = theirs[from..(at + 3).min(theirs.len())].iter().map(|(a, m)| format!("{a:#x}: {m}")).collect();
            panic!(
                "{name}: the instruction boundaries diverge after {} instructions ({} ours, {} objdump)\n  ours:    {}\n  objdump: {}",
                at,
                ours.len(),
                theirs.len(),
                ours_text.join(" | "),
                theirs_text.join(" | ")
            );
        }
        let mut mismatches = Vec::new();
        for (i, (_, mnemonic)) in ours.iter().zip(&theirs) {
            let first = i.text.split_whitespace().find(|w| *w != "lock").unwrap_or("");
            if !same_mnemonic(first, mnemonic) {
                mismatches.push(format!("{:#x}: ours {:?}, objdump {mnemonic:?}", i.address, i.text));
            }
        }
        assert!(mismatches.is_empty(), "{name}: {} mnemonic differences:\n{}", mismatches.len(), mismatches.join("\n"));
        assert!(!ours.is_empty());
    }
}
