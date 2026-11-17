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
