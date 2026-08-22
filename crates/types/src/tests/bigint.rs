use crate::{u256, NonMaxU256};

#[test]
fn non_max_u256_accepts_valid_boundaries() {
    assert_eq!(NonMaxU256::new(u256::default()).unwrap().into_inner(), u256::default());

    let largest_valid = u256::from_limbs([u64::MAX, u64::MAX, u64::MAX, u64::MAX - 1]);
    assert_eq!(NonMaxU256::new(largest_valid).unwrap().into_inner(), largest_valid);
}

#[test]
fn non_max_u256_rejects_max_value() {
    let max = u256::from_limbs([u64::MAX; 4]);
    assert_eq!(NonMaxU256::new(max), None);
}
