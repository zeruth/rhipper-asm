use super::{Rng, mask, sized, undefined_mask};

#[test]
fn a_value_keeps_its_width_in_bytes() {
    assert_eq!(sized(0x1234_5678_9abc, 32), [0x56, 0x78, 0x9a, 0xbc]);
    assert_eq!(sized(0x1ff, 8), [0xff]);
    assert_eq!(sized(u64::MAX, 64), [0xff; 8]);
}

#[test]
fn the_undefined_mask_sets_the_bit_of_each_register_number() {
    assert_eq!(undefined_mask(&[1], 32), [0, 0, 0, 2]);
    assert_eq!(undefined_mask(&[0, 9, 31], 32), [0x80, 0, 2, 1]);
    assert_eq!(undefined_mask(&[33], 34), [2, 0, 0, 0, 0]);
}

#[test]
fn a_seed_draws_the_same_values_within_their_width() {
    let (mut a, mut b) = (Rng(7), Rng(7));
    for _ in 0..256 {
        let value = a.value(12);
        assert_eq!(value, b.value(12));
        assert_eq!(value & !mask(12), 0);
    }
    assert_eq!(mask(64), u64::MAX);
}
