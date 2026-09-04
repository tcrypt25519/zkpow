use sha2::{Digest, Sha256};
use zkvm_test::test;

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

#[test]
fn guest_output_capture_demonstration() {
    #[cfg(target_os = "zkvm")]
    {
        sp1_zkvm::io::write(1, b">>> HELLO FROM SP1 GUEST STDOUT! <<<\n");
        sp1_zkvm::io::write(2, b">>> HELLO FROM SP1 GUEST STDERR! <<<\n");
    }
}

#[test(max_cycles = 500_000)]
fn budgeted_hash_within_limit() {
    assert_public_values_hash([1u8; 185]);
}

#[test(max_cycles = 50_000)]
#[ignore]
fn budgeted_hash_exceeding_limit() {
    assert_public_values_hash([2u8; 185]);
}

#[test]
fn subroutine_cycle_profiling_demonstration() {
    sp1_cycle::run_track("outer_pipeline", || {
        sp1_cycle::run_track("first_hash_pass", || {
            assert_public_values_hash([10u8; 185]);
        });
        sp1_cycle::run_track("second_hash_pass", || {
            assert_public_values_hash([20u8; 185]);
        });
    });
}


