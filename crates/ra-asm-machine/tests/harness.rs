//! The guest harness of `testdata/vectors/harness` on its first described machine: guest.c with
//! the trampoline of mipsel.S, built by run.sh with clang in WSL and run under
//! `qemu-mipsel-static`. A case is its code at the case page's `CASE_IP` with every register,
//! hi, lo and a window of the data region; its result is the signal that stopped it, the pc it
//! stopped at, the registers, hi, lo and the window after it.

use std::fs;
use std::path::Path;
use std::process::Command;

const CASE_IP: u32 = 0x3000_2000;
const DATA_BASE: u32 = 0x4000_0000;
const WINDOW: usize = 64;
const CODE: usize = 16;
/// `sizeof(struct case_in)` and `sizeof(struct case_out)` of guest.c's MIPS build.
const IN_LEN: usize = 224;
const OUT_LEN: usize = 212;
const SIGTRAP: u32 = 5;

const T0: usize = 8;
const T1: usize = 9;
const T2: usize = 10;

/// One case in, as `struct case_in` lays it out.
struct CaseIn {
    r: [u32; 32],
    hi: u32,
    lo: u32,
    code: Vec<u32>,
    window_at: u32,
    window: [u8; WINDOW],
}

/// One case out, as `struct case_out` lays it out; `stray` is the first address outside the
/// window a store reached, plus one.
#[derive(Debug, PartialEq, Eq)]
struct CaseOut {
    sig: u32,
    pc: u32,
    r: [u32; 32],
    hi: u32,
    lo: u32,
    stray: u32,
    window: [u8; WINDOW],
}

fn pack(blob: &mut Vec<u8>, case: &CaseIn) {
    let start = blob.len();
    for word in case.r.iter().chain([&case.hi, &case.lo]) {
        blob.extend_from_slice(&word.to_le_bytes());
    }
    let mut code = [0u8; CODE];
    for (slot, word) in code.chunks_exact_mut(4).zip(&case.code) {
        slot.copy_from_slice(&word.to_le_bytes());
    }
    blob.extend_from_slice(&(4 * case.code.len() as u32).to_le_bytes());
    blob.extend_from_slice(&case.window_at.to_le_bytes());
    blob.extend_from_slice(&code);
    blob.extend_from_slice(&case.window);
    assert_eq!(blob.len() - start, IN_LEN);
}

fn unpack(bytes: &[u8]) -> CaseOut {
    let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
    CaseOut {
        sig: word(0),
        pc: word(1),
        r: std::array::from_fn(|n| word(2 + n)),
        hi: word(34),
        lo: word(35),
        stray: word(36),
        window: bytes[4 * 37..4 * 37 + WINDOW].try_into().unwrap(),
    }
}

/// The path WSL sees for a Windows path: `C:\a\b` is `/mnt/c/a/b`.
fn wsl_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.as_bytes() {
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic() => format!("/mnt/{}{}", drive.to_ascii_lowercase() as char, &text[2..]),
        _ => text,
    }
}

/// Builds the guest for `machine` with run.sh in `scratch` and runs `blob` through it; the
/// results it wrote.
fn run_guest(wsl: &Path, scratch: &Path, machine: &str, blob: &[u8]) -> Vec<u8> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness/run.sh");
    let input = scratch.join("cases.bin");
    let output = scratch.join("results.bin");
    fs::write(&input, blob).unwrap();
    let run = Command::new(wsl)
        .args(["-d", "Ubuntu-24.04", "--", "bash"])
        .arg(wsl_path(&script))
        .arg(machine)
        .args([wsl_path(scratch), wsl_path(&input), wsl_path(&output)])
        .output()
        .expect("wsl runs");
    assert!(
        run.status.success(),
        "run.sh {machine} failed: {}\n{}{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    fs::read(&output).unwrap_or_else(|e| panic!("{}: {e}", output.display()))
}

/// The harness gives the states of `addu`, `lw` and a taken `beq` with its delay slot: each
/// stopped by the `break` after it (the branch's at its target), every register but the ones it
/// writes as it went in, $1 and $4 (which the page's prologue loads), $ra (which the jump into
/// the page loads in its slot) and hi and lo included.
#[test]
#[ignore = "builds testdata/vectors/harness/guest.c for mipsel in WSL and runs it under qemu-mipsel-static"]
fn mipsel_runs_three_cases() {
    let Some(wsl) = ra_asm_testing::tool("wsl") else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-machine-tests", "harness-mipsel");
    // A value of its own in every register, so a register the trampoline loads or saves wrong
    // shows; $0 reads zero whatever goes in.
    let regs: [u32; 32] = std::array::from_fn(|n| if n == 0 { 0 } else { 0x1000_0000 | ((n as u32) << 16) | n as u32 });
    let (hi, lo) = (0x1357_9bdf, 0x2468_ace0);
    let zeros = [0u8; WINDOW];

    // addu $t0, $t1, $t2
    let mut addu = CaseIn { r: regs, hi, lo, code: vec![0x012a_4021], window_at: DATA_BASE, window: zeros };
    addu.r[T1] = 0x7fff_ffff;
    addu.r[T2] = 1;
    // lw $t0, 4($t1)
    let mut window = zeros;
    window[4..8].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    let mut lw = CaseIn { r: regs, hi, lo, code: vec![0x8d28_0004], window_at: DATA_BASE, window };
    lw.r[T1] = DATA_BASE;
    // beq $t0, $t1, 3 with addiu $t2, $t2, 1 in its slot
    let mut beq = CaseIn { r: regs, hi, lo, code: vec![0x1109_0003, 0x254a_0001], window_at: DATA_BASE, window: zeros };
    beq.r[T0] = 5;
    beq.r[T1] = 5;
    beq.r[T2] = 7;

    let mut blob = Vec::new();
    for case in [&addu, &lw, &beq] {
        pack(&mut blob, case);
    }
    let results = run_guest(&wsl, &scratch, "mipsel", &blob);
    assert_eq!(results.len(), 3 * OUT_LEN, "the harness answered {} bytes", results.len());
    let outs: Vec<CaseOut> = results.chunks_exact(OUT_LEN).map(unpack).collect();

    // addu wraps without the overflow trap of add.
    let mut after = addu.r;
    after[T0] = 0x8000_0000;
    assert_eq!(outs[0], CaseOut { sig: SIGTRAP, pc: CASE_IP + 4, r: after, hi, lo, stray: 0, window: zeros }, "addu $t0, $t1, $t2");

    let mut after = lw.r;
    after[T0] = 0x1122_3344;
    assert_eq!(outs[1], CaseOut { sig: SIGTRAP, pc: CASE_IP + 4, r: after, hi, lo, stray: 0, window }, "lw $t0, 4($t1)");

    // The slot runs before the transfer, and the target is the slot's address plus 12.
    let mut after = beq.r;
    after[T2] = 8;
    assert_eq!(
        outs[2],
        CaseOut { sig: SIGTRAP, pc: CASE_IP + 16, r: after, hi, lo, stray: 0, window: zeros },
        "beq $t0, $t1, 3; addiu $t2, $t2, 1"
    );
    println!("mipsel harness: 3 cases as expected");
}
