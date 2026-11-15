//! The kinds of a signature, as the crate gives them without the IR.

use ra_asm_protos::kind::Kind;

/// A wide integer and a double take two 32-bit slots or one 64-bit slot; an integer and a float
/// take one slot of either size.
#[test]
fn stack_widths_follow_the_slot() {
    for slot in [4, 8] {
        assert_eq!(Kind::Int.stack_width(slot), slot);
        assert_eq!(Kind::Float.stack_width(slot), slot);
        assert_eq!(Kind::Wide.stack_width(slot), 8);
        assert_eq!(Kind::Double.stack_width(slot), 8);
    }
}

/// The kinds compare by variant, as the signatures' fixpoint compares them.
#[test]
fn kinds_compare_by_variant() {
    let kinds = [Kind::Int, Kind::Wide, Kind::Float, Kind::Double];
    for (i, a) in kinds.iter().enumerate() {
        for (j, b) in kinds.iter().enumerate() {
            assert_eq!(a == b, i == j);
        }
    }
    assert_eq!(format!("{:?}", Kind::Wide), "Wide");
}
