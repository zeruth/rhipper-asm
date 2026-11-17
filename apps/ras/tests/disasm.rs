//! `ras disasm` over the MIPS fixtures: a delay slot sits indented under its transfer, and a likely
//! branch's slot says it runs on the taken edge only.

use std::path::Path;
use std::process::Command;

fn ras(fixture: &str, args: &[&str]) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata").join(fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg("disasm").arg(&path).args(args).output().unwrap();
    assert!(output.status.success(), "ras disasm {fixture} {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn mips_function_shows_its_delay_slots() {
    // `collatz` of the control fixture: one run of code, which ends after the slot of its return.
    let collatz = ras("bin/control_mipsel", &["0x205c4"]);
    assert!(collatz.starts_with("0x000205c4  addiu\t$4, $zero, 0x1b\n"), "{collatz}");
    assert!(
        collatz.ends_with(
            "0x000205ec  bne\t$1, $3, 0x205d0\n\
             0x000205f0    move\t$4, $1   ; delay slot\n\
             0x000205f4  jr\t$ra\n\
             0x000205f8    nop   ; delay slot\n"
        ),
        "{collatz}"
    );

    // `strlen` of the PlayStation 2 fixture, whose loop closes on a likely branch.
    let strlen = ras("ps2/hello.elf", &["0x102960", "0x138"]);
    for lines in [
        "0x001029f4  beql\t$3, $zero, 0x1029d8\n\
         0x001029f8    addiu\t$5, $5, 0x10   ; delay slot, taken edge only\n\
         0x001029fc  b\t0x102a74\n",
        "0x001029fc  b\t0x102a74\n0x00102a00    ",
    ] {
        assert!(strlen.contains(lines), "no\n{lines}\nin:\n{strlen}");
    }
    assert!(strlen.lines().any(|l| l.ends_with("; delay slot")), "{strlen}");
    assert!(!strlen.contains("\n0x001029fc    "), "a branch after a slot is no slot:\n{strlen}");
}
