use super::{Outlines, symbol};

#[test]
fn a_function_is_named_after_its_row_and_field_values() {
    assert_eq!(symbol("rtps", &[]), "rtps");
    assert_eq!(symbol("mul.ps", &[3, -4]), "mul_ps_3_m4");
}

#[test]
fn a_name_already_taken_gets_a_number() {
    let mut outlines = Outlines::new();
    assert_eq!(outlines.fresh("a_1"), "a_1");
    assert_eq!(outlines.fresh("a_1"), "a_1_2");
    assert_eq!(outlines.fresh("a_1"), "a_1_3");
}
