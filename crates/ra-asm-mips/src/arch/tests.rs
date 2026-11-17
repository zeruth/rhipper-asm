use super::*;

#[test]
fn the_rows_make_a_table() {
    assert_eq!(Mips::machine().table.rows().len(), FORMS.len());
    assert_eq!(Mips::r5900_machine().table.rows().len(), FORMS.len() + R5900_FORMS.len());
}

#[test]
fn the_r5900_rows_decode_only_for_the_r5900() {
    let (mips, ee) = (Mips::default(), Mips { r5900: true, ..Mips::default() });
    // `sq $11, 0($sp)`, `paddw $11, $9, $10`, and `sd $ra, 8($sp)`, which MIPS III has too.
    let name = |arch: Mips, word: u32| arch.decode(&word.to_le_bytes(), 0x10_0000).map(|i| i.mnemonic);
    assert_eq!(name(ee, 0x7fab_0000).as_deref(), Some("sq"));
    assert_eq!(name(mips, 0x7fab_0000), None);
    assert_eq!(name(ee, 0x712a_5808).as_deref(), Some("paddw"));
    assert_eq!(name(mips, 0x712a_5808), None);
    assert_eq!(name(ee, 0xffbf_0008).as_deref(), Some("sd"));
    assert_eq!(name(mips, 0xffbf_0008), None);
    // `madd.s $f3, $f1, $f2`, `bc1t` and `lwc1 $f1, 0($sp)`: the EE's FPU.
    assert_eq!(name(ee, 0x4602_08dc).as_deref(), Some("madd.s"));
    assert_eq!(name(mips, 0x4602_08dc), None);
    assert_eq!(name(ee, 0x4501_0003).as_deref(), Some("bc1t"));
    assert_eq!(name(ee, 0xc7a1_0000).as_deref(), Some("lwc1"));
}

#[test]
fn allegrex_rows_decode() {
    let psp = Mips { allegrex: true, ..Mips::default() };
    let text = |word: u32| psp.decode(&word.to_le_bytes(), 0x10_0000).map(|i| i.text);
    let rows: &[(u32, &str)] = &[
        // SPECIAL3: the bit fields and the byte and bit shuffles.
        (0x7ca4_38c0, "ext\t$4, $5, 0x3, 0x7"),
        (0x7ca4_38c4, "ins\t$4, $5, 0x3, 0x7"),
        (0x7c03_1420, "seb\t$2, $3"),
        (0x7c03_1620, "seh\t$2, $3"),
        (0x7c03_10a0, "wsbh\t$2, $3"),
        (0x7c03_1520, "bitrev\t$2, $3"),
        // SPECIAL: the functions MIPS III gives its doubleword operations.
        (0x0064_102d, "min\t$2, $3, $4"),
        (0x0064_102c, "max\t$2, $3, $4"),
        (0x0060_1016, "clz\t$2, $3"),
        (0x0060_1017, "clo\t$2, $3"),
        (0x0064_001c, "madd\t$3, $4"),
        (0x0064_001d, "maddu\t$3, $4"),
        (0x0064_002e, "msub\t$3, $4"),
        (0x0064_002f, "msubu\t$3, $4"),
        // COP1: the loads and stores, the moves, the formats and the branches.
        (0xc7a1_0000, "lwc1\t0x1, 0x0($sp)"),
        (0xe7a1_0004, "swc1\t0x1, 0x4($sp)"),
        (0x4402_0800, "mfc1\t$2, 0x1"),
        (0x4442_0800, "cfc1\t$2, 0x1"),
        (0x4482_0800, "mtc1\t$2, 0x1"),
        (0x44c2_0800, "ctc1\t$2, 0x1"),
        (0x4602_08c0, "cop1.s\t0x3, 0x1, 0x2"),
        (0x4622_08c0, "cop1.d\t0x3, 0x1, 0x2"),
        (0x4680_08e0, "cop1.w\t0x3, 0x1"),
        (0x4500_0003, "bc1f\t0x100010"),
        (0x4501_0003, "bc1t\t0x100010"),
        (0x4502_0003, "bc1fl\t0x100010"),
        (0x4503_0003, "bc1tl\t0x100010"),
    ];
    for &(word, expected) in rows {
        assert_eq!(text(word).as_deref(), Some(expected), "{word:#010x}");
    }
}

#[test]
fn every_transfer_is_delayed_and_nothing_else_is() {
    for form in FORMS.iter().chain(R5900_FORMS) {
        let transfers = matches!(form.row.flow, Flow::Jump | Flow::Branch | Flow::Call | Flow::Return);
        assert_eq!(form.delay != Delay::None, transfers, "{}", form.row.name);
        assert_eq!(form.reach == Reach::Region, matches!(form.row.name, "j" | "jal"), "{}", form.row.name);
    }
}

#[test]
fn an_alias_wins_its_words() {
    let name = |word: u32| Mips::machine().table.decode(word).map(|decoded| decoded.row.name);
    assert_eq!(name(0), Some("nop"));
    assert_eq!(name(0x0000_0040), Some("sll"), "sll $zero, $zero, 1");
    assert_eq!(name(0x0040_1825), Some("move"), "or $3, $2, $zero");
    assert_eq!(name(0x0041_1825), Some("or"));
    assert_eq!(name(0x1000_0003), Some("b"));
    assert_eq!(name(0x1200_0003), Some("beqz"));
    assert_eq!(name(0x1201_0003), Some("beq"));
    assert_eq!(name(0x0411_0003), Some("bal"));
    assert_eq!(name(0x0491_0003), Some("bgezal"));
}

#[test]
fn a_must_be_zero_field_that_is_not_is_no_instruction() {
    assert!(Mips::machine().table.decode(0x0040_1865).is_none(), "or with a shift amount");
    assert!(Mips::machine().table.decode(0x0417_0001).is_none(), "REGIMM's rt 0x17");
}
