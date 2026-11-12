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
fn u256_addition_carries_and_wraps() {
    for (a, expected) in [
        ([u64::MAX, 0, 0, 0], [0, 1, 0, 0]),
        ([u64::MAX, u64::MAX, u64::MAX, 0], [0, 0, 0, 1]),
        ([u64::MAX; 4], [0; 4]),
    ] {
        assert_eq!(
            u256::from_limbs(a) + u256::from_limbs([1, 0, 0, 0]),
            u256::from_limbs(expected),
            "{a:?}"
        );
    }
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
    for (index, byte) in [(0, 0xef), (7, 0x01), (8, 0x88), (15, 0x11)] {
        assert_eq!(bytes[index], byte, "{index}");
    }
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
    for (len, ok) in [(32, true), (31, false), (33, false)] {
        assert_eq!(u256::try_from(&[0x42u8; 33][..len]).is_ok(), ok, "{len}");
    }
}

#[test]
fn u256_from_words_matches_from_le_bytes() {
    let raw_bytes: [u8; 32] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x01, 0x02,
        0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
    ];

    let words: [u64; 8] = core::array::from_fn(|i| {
        let chunk: [u8; 4] = raw_bytes[i * 4..(i + 1) * 4].try_into().unwrap();
        u32::from_be_bytes(chunk) as u64
    });

    let from_bytes = u256::from_le_bytes(raw_bytes);
    let from_words = u256::from_words(&words);
    assert_eq!(from_words, from_bytes);
}

#[test]
fn u256_leading_zeros_and_bits_edge_cases() {
    for (limbs, leading_zeros, bits) in [
        ([0; 4], 256, 0),
        ([1, 0, 0, 0], 255, 1),
        ([1 << 63, 0, 0, 0], 192, 64),
        ([0, 1, 0, 0], 191, 65),
        ([u64::MAX; 4], 0, 256),
        ([0, 0, 0, 1 << 63], 0, 256),
    ] {
        let value = u256::from_limbs(limbs);
        assert_eq!(value.leading_zeros(), leading_zeros, "{limbs:?}");
        assert_eq!(value.bits(), bits, "{limbs:?}");
    }
}
