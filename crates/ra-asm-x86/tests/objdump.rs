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

/// `llvm-objdump` prints `140001000: mov eax, ecx` per instruction, with tabs between the parts.
fn objdump(name: &str) -> Option<Vec<(u64, String)>> {
    let output = Command::new("llvm-objdump")
        .args(["-d", "--no-show-raw-insn", "-M", "intel", "--section=.text"])
        .arg(fixture(name))
        .output()
        .ok()?;
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
    for name in names {
        let Some(theirs) = objdump(name) else {
            eprintln!("llvm-objdump is not on the PATH: the oracle comparison is skipped");
            return;
        };
        let image = ra_asm_pe::load(&fixture(name)).unwrap();
        let text = image.sections.iter().find(|s| s.name == ".text").unwrap();
        let start = image.va(text.virtual_address);
        let bytes = image.read_to_section_end(start).unwrap();
        let arch = X86::for_pointer_size(image.pointer_size());
        let ours = arch.decode_all(bytes, start);
        let our_addresses: Vec<u64> = ours.iter().map(|i| i.address).collect();
        let their_addresses: Vec<u64> = theirs.iter().map(|(a, _)| *a).collect();
        assert_eq!(our_addresses, their_addresses, "{name}: the instruction boundaries");
        let mut mismatches = Vec::new();
        for (i, (_, mnemonic)) in ours.iter().zip(&theirs) {
            let first = i.text.split_whitespace().next().unwrap_or("");
            if !same_mnemonic(first, mnemonic) {
                mismatches.push(format!("{:#x}: ours {:?}, objdump {mnemonic:?}", i.address, i.text));
            }
        }
        assert!(mismatches.is_empty(), "{name}: {} mnemonic differences:\n{}", mismatches.len(), mismatches.join("\n"));
        assert!(!ours.is_empty());
    }
}
