use sha2::{Digest, Sha256};

use crate::of_public_values;

fn assert_public_values_hash(bytes: [u8; 185]) {
    let expected: [u8; 32] = Sha256::digest(bytes).into();
    let actual = of_public_values(&bytes[..152], &bytes[152..184], bytes[184]);
    assert_eq!(actual, expected);
}

#[test]
fn public_values_optimized_hash_matches_standard_sha256() {
    assert_public_values_hash(core::array::from_fn(|i| (i & 0xFF) as u8));
}

#[test]
fn public_values_optimized_hash_matches_sha256_for_zero_input() {
    assert_public_values_hash([0u8; 185]);
}

#[test]
fn public_values_optimized_hash_matches_sha256_for_high_entropy_input() {
    assert_public_values_hash(core::array::from_fn(|i| (i.wrapping_mul(251) as u8).wrapping_add(1)));
}
