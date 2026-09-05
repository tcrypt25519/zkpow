use core::cmp::Ordering;

use zkvm_test::test;

use crate::{u256, NonMaxU256};

#[test]
fn non_max_u256_accepts_valid_boundaries() {
    assert_eq!(NonMaxU256::new(u256::default()).unwrap().into_inner(), u256::default());

    let largest_valid = u256::from_limbs([u64::MAX, u64::MAX, u64::MAX, u64::MAX - 1]);
    assert_eq!(NonMaxU256::new(largest_valid).unwrap().into_inner(), largest_valid);

    let zero = NonMaxU256::from_limbs([0, 0, 0, 0]);
    assert_eq!(zero.into_inner(), u256::default());
}

#[test]
fn non_max_u256_rejects_max_value() {
    let max = u256::from_limbs([u64::MAX; 4]);
    assert_eq!(NonMaxU256::new(max), None);
    assert_eq!(NonMaxU256::try_from_limbs([u64::MAX; 4]), None);
}

#[test]
fn u256_limb_access_and_mutation() {
    let mut val = u256::from_limbs([10, 20, 30, 40]);
    assert_eq!(val.as_limbs(), &[10, 20, 30, 40]);

    val.as_limbs_mut()[0] = 99;
    assert_eq!(val.as_limbs()[0], 99);

    assert_eq!(val.into_limbs(), [99, 20, 30, 40]);
}

#[test]
fn u256_addition_with_carry_propagation() {
    // Carry across limb 0 to limb 1
    let a = u256::from_limbs([u64::MAX, 0, 0, 0]);
    let b = u256::from_limbs([1, 0, 0, 0]);
    assert_eq!(a + b, u256::from_limbs([0, 1, 0, 0]));

    // Carry cascade through all 4 limbs
    let c = u256::from_limbs([u64::MAX, u64::MAX, u64::MAX, 0]);
    let d = u256::from_limbs([1, 0, 0, 0]);
    assert_eq!(c + d, u256::from_limbs([0, 0, 0, 1]));
}

#[test]
fn u256_addition_wrapping_at_256_bits() {
    let max = u256::from_limbs([u64::MAX; 4]);
    let one = u256::from_limbs([1, 0, 0, 0]);
    assert_eq!(max + one, u256::from_limbs([0, 0, 0, 0]));
}

#[test]
fn u256_mul_u32_scaling_and_carry() {
    let val = u256::from_limbs([u64::MAX, 0, 0, 0]);
    let scaled = val * 2u32;
    // u64::MAX * 2 = 2 * (2^64 - 1) = 2^65 - 2 = (2^64 - 2) + 1 * 2^64
    assert_eq!(scaled, u256::from_limbs([u64::MAX - 1, 1, 0, 0]));

    let mut val2 = u256::from_limbs([3, 0, 0, 0]);
    val2 *= 5u32;
    assert_eq!(val2, u256::from_limbs([15, 0, 0, 0]));
}

#[test]
fn u256_endian_byte_roundtrip() {
    let limbs = [0x0123_4567_89ab_cdef, 0x1122_3344_5566_7788, 0, 0];
    let original = u256::from_limbs(limbs);

    let bytes = original.to_le_bytes();
    let recovered = u256::from_le_bytes(bytes);
    assert_eq!(recovered, original);

    // Verify little-endian order of bytes
    assert_eq!(bytes[0], 0xef);
    assert_eq!(bytes[7], 0x01);
    assert_eq!(bytes[8], 0x88);
    assert_eq!(bytes[15], 0x11);
}

#[test]
fn u256_ordering_compares_most_significant_limbs_first() {
    // Higher limb dominates regardless of lower limbs
    let smaller = u256::from_limbs([u64::MAX, u64::MAX, u64::MAX, 0]);
    let larger = u256::from_limbs([0, 0, 0, 1]);
    assert_eq!(smaller.cmp(&larger), Ordering::Less);
    assert!(smaller < larger);

    let equal1 = u256::from_limbs([1, 2, 3, 4]);
    let equal2 = u256::from_limbs([1, 2, 3, 4]);
    assert_eq!(equal1.cmp(&equal2), Ordering::Equal);
}

#[test]
fn u256_try_from_slice_validates_exact_length() {
    let valid_bytes = [0x42u8; 32];
    assert!(u256::try_from(valid_bytes.as_slice()).is_ok());

    let short_bytes = [0x42u8; 31];
    assert!(u256::try_from(short_bytes.as_slice()).is_err());

    let long_bytes = [0x42u8; 33];
    assert!(u256::try_from(long_bytes.as_slice()).is_err());
}
