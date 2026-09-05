extern crate alloc;
use alloc::string::ToString;

use zkvm_test::test;
use zkpow_types::ChainWork;

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
fn mainnet_genesis_header_fields() {
    let genesis = &Mainnet::PARAMS.genesis;
    assert_eq!(genesis.version, 1);
    assert_eq!(genesis.prev_blockhash, BlockHash::new([0u8; 32]));
    assert_eq!(
        genesis.merkle_root,
        [
            0x3b, 0xa3, 0xed, 0xfd, 0x7a, 0x7b, 0x12, 0xb2, 0x7a, 0xc7, 0x2c, 0x3e, 0x67, 0x76, 0x8f, 0x61, 0x7f, 0xc8,
            0x1b, 0xc3, 0x88, 0x8a, 0x51, 0x32, 0x3a, 0x9f, 0xb8, 0xaa, 0x4b, 0x1e, 0x5e, 0x4a,
        ]
    );
    assert_eq!(genesis.timestamp, BlockTimestamp::new(1231006505));
    assert_eq!(genesis.compact_target, CompactTarget::new(0x1d00ffff));
    assert_eq!(genesis.nonce, 2083236893);
}

#[test]
fn mainnet_genesis_wire_bytes_round_trip() {
    let wire_bytes = Mainnet::PARAMS.genesis.as_bytes();
    assert_eq!(wire_bytes.len(), Header::SIZE);
    let parsed = Header::parse(wire_bytes).expect("genesis wire bytes must parse");
    assert_eq!(parsed, &Mainnet::PARAMS.genesis);
}

#[test]
fn mainnet_genesis_target_and_work() {
    assert_eq!(
        Mainnet::PARAMS.genesis_target,
        Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000])
    );
    assert_eq!(
        Mainnet::PARAMS.genesis_work,
        BlockWork::from_limbs([0x0000_0001_0001_0001, 0, 0, 0])
    );
    let chain_work = ChainWork::from_limbs(*Mainnet::PARAMS.genesis_work.as_limbs());
    assert_eq!(chain_work, ChainWork::from_limbs([0x0000_0001_0001_0001, 0, 0, 0]));
}

#[test]
fn derived_epoch_bounds() {
    assert_eq!(Mainnet::PARAMS.expected_epoch_time(), 600 * 2016);
    assert_eq!(Mainnet::PARAMS.max_epoch_timespan(), (600 * 2016 * 4) as i64);
    assert_eq!(Mainnet::PARAMS.min_epoch_timespan(), (600 * 2016 / 4) as i64);
}

#[test]
fn clamping_factor_bounds() {
    assert_eq!(Mainnet::PARAMS.max_epoch_over_mul, 4);
    assert_eq!(Mainnet::PARAMS.min_epoch_under_div, 4);
    assert_eq!(
        Mainnet::PARAMS.max_epoch_timespan(),
        Mainnet::PARAMS.expected_epoch_time() as i64 * 4
    );
    assert_eq!(
        Mainnet::PARAMS.min_epoch_timespan(),
        Mainnet::PARAMS.expected_epoch_time() as i64 / 4
    );
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
    assert_eq!(ShortEpoch::PARAMS.epoch_length, 32);
    assert_eq!(ShortEpoch::PARAMS.expected_epoch_time(), 600 * 32);
}
