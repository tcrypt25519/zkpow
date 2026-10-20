use alloc::vec::Vec;
use core::mem::{align_of, size_of};

use zkvm_test::zkvm_test;

use crate::{block_work_from_target, tests::*, BlockHash, BlockWork, ChainWork, CompactTarget, HeaderInput, State};

#[zkvm_test(max_cycles = 100_000)]
fn state_and_header_fixed_sizes_match_protocol() {
    assert_eq!(HeaderInput::SIZE, 44);
    assert_eq!(State::SIZE, 152);
    assert_eq!(align_of::<Header>(), 4);
    assert_eq!(align_of::<HeaderInput>(), 4);
    assert_eq!(align_of::<BlockWork>(), 8);
    assert_eq!(align_of::<ChainWork>(), 8);
    assert_eq!(align_of::<State>(), 8);
}

#[zkvm_test]
fn state_and_header_input_layout_and_offsets() {
    assert_eq!(size_of::<State>(), 152);
    assert_eq!(align_of::<State>(), 8);

    assert_eq!(size_of::<HeaderInput>(), 44);
    assert_eq!(align_of::<HeaderInput>(), 4);

    let state = State {
        chain_work:      ChainWork::from_limbs([1, 2, 3, 4]),
        genesis_hash:    BlockHash::new([0x11; 32]),
        block_hash:      BlockHash::new([0x22; 32]),
        height:          42,
        current_nbits:   CompactTarget::new(0x1d00ffff),
        epoch_timestamp: BlockTimestamp::new(1000),
        timestamps:      [BlockTimestamp::new(2000); WINDOW_SIZE],
    };
    let bytes = state.to_bytes();
    assert_eq!(bytes.len(), 152);
    let parsed: &State = bytemuck::parse(&bytes);
    assert_eq!(*parsed, state);
}

#[zkvm_test]
fn apply_headers_treats_height_zero_state_as_trusted_anchor() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    state.genesis_hash = BlockHash::new([0x11; 32]);
    state.block_hash = BlockHash::new([0x22; 32]);
    let headers: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    let hints = median_hints_for_headers(&state, &headers);
    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect("height-zero anchor should already be trusted");

    assert_eq!(state.height, 10);
    assert_eq!(state.genesis_hash, BlockHash::new([0x11; 32]));
    assert_eq!(state.block_hash, zero_hash());
}

#[zkvm_test(max_cycles = 1_200_000)]
fn apply_headers_flushes_deferred_chain_work_on_success() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();

    let hints = median_hints_for_headers(&state, &headers);
    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect("headers should validate");

    let block_work = block_work_from_target(GENESIS_TARGET);
    let mut new_chain_work = block_work * 10u32;
    new_chain_work += BlockWork::default();
    let expected = ChainWork::default() + new_chain_work;
    assert_eq!(state.chain_work, expected);
}

#[zkvm_test]
fn apply_headers_flushes_deferred_chain_work_for_longer_run() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers: Vec<HeaderInput> = (1..=12)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();

    let hints = median_hints_for_headers(&state, &headers);
    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect("validation should succeed");

    let block_work = block_work_from_target(GENESIS_TARGET);
    let expected = block_work * 12;
    let expected = ChainWork::default() + expected;
    assert_eq!(state.chain_work, expected);
}

#[zkvm_test]
fn chain_work_adds_block_work_with_two_limb_carry_and_wrap() {
    let cases = [
        (
            "carry propagation",
            ChainWork::from_limbs([u64::MAX, 0, 0, 0]),
            BlockWork::from_limbs([1, 0, 0, 0]),
            ChainWork::from_limbs([0, 1, 0, 0]),
        ),
        (
            "wrap at 128 bits",
            ChainWork::from_limbs([u64::MAX; 4]),
            BlockWork::from_limbs([1, 0, 0, 0]),
            ChainWork::default(),
        ),
    ];

    for (name, left, right, expected) in cases {
        assert_eq!(left + right, expected, "{name}");
    }
}

#[zkvm_test]
fn block_work_mul_u32_scales_by_small_count() {
    let value = BlockWork::from_limbs([3, 0, 0, 0]);
    let result = value * 7u32;
    assert_eq!(result, ChainWork::from_limbs([21, 0, 0, 0]));
}
