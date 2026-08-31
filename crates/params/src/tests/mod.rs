extern crate std;

use std::string::ToString;

use super::*;

/// The genesis hash is derived from the header, so this is what pins both the header bytes and the
/// const SHA-256 to reality.
#[test]
fn mainnet_genesis_hash_is_the_known_block_hash() {
    assert_eq!(
        Mainnet::GENESIS_HASH.to_string(),
        "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
    );
}

#[test]
fn genesis_hash_is_evaluated_at_compile_time() {
    const HASH: BlockHash = Mainnet::GENESIS_HASH;
    assert_eq!(HASH, Mainnet::PARAMS.genesis_hash());
}

#[test]
fn derived_epoch_bounds() {
    assert_eq!(Mainnet::PARAMS.expected_epoch_time(), 600 * 2016);
    assert_eq!(Mainnet::PARAMS.max_epoch_timespan(), (600 * 2016 * 4) as i64);
    assert_eq!(Mainnet::PARAMS.min_epoch_timespan(), (600 * 2016 / 4) as i64);
}

/// Reading a parameter through the trait must be a compile-time constant, not a runtime load.
#[test]
fn params_are_usable_in_a_const_context() {
    const EPOCH_LENGTH: u32 = Mainnet::PARAMS.epoch_length;
    const EXPECTED: u32 = Mainnet::PARAMS.expected_epoch_time();
    assert_eq!(EPOCH_LENGTH, 2016);
    assert_eq!(EXPECTED, 600 * 2016);
}

/// The two halves partition the window around its median, and the max index is the run length
/// needed to prove the whole window is ordered.
#[test]
fn derived_window_bounds() {
    assert_eq!(WINDOW_SIZE, 11);
    assert_eq!(WINDOW_SMALL_HALF_SIZE, 5);
    assert_eq!(WINDOW_LARGE_HALF_SIZE, 6);
    assert_eq!(WINDOW_SMALL_HALF_SIZE + WINDOW_LARGE_HALF_SIZE, WINDOW_SIZE);
    assert_eq!(WINDOW_MAX_INDEX, WINDOW_SIZE - 1);
}

/// `ShortEpoch` exists to give the transition tests a second `epoch_length`. If it ever matched
/// mainnet it would silently stop testing anything.
#[test]
fn short_epoch_differs_from_mainnet_only_in_epoch_length() {
    assert_ne!(ShortEpoch::PARAMS.epoch_length, Mainnet::PARAMS.epoch_length);
    assert_eq!(ShortEpoch::GENESIS_HASH, Mainnet::GENESIS_HASH);
    assert_eq!(ShortEpoch::PARAMS.genesis_target, Mainnet::PARAMS.genesis_target);
    assert_eq!(ShortEpoch::PARAMS.block_time, Mainnet::PARAMS.block_time);
}
