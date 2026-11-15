use super::*;

#[test]
fn the_rows_make_a_table() {
    assert_eq!(Mips::machine().table.rows().len(), FORMS.len());
}

#[test]
fn every_transfer_is_delayed_and_nothing_else_is() {
    for form in FORMS {
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
