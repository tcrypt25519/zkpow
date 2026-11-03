use alloc::vec::Vec;

use zkpow_types::{BlockHash, CompactTarget};
use zkvm_test::zkvm_test;

use crate::{tests::*, ApplyError, HeaderInput, State};

#[zkvm_test]
fn header_rejects_mocked_hash_exceeding_target() {
    let _hash_guard = set_hash_fail();
    let mut state = test_state();
    let headers10: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([i as u8; 32]),
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();

    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers10, &[])
        .expect_err("expected PoW failure");
    assert_eq!(expect_error_code(failure), ErrorCode::POW_INSUFFICIENT);
}

#[zkvm_test]
fn rejected_header_reports_its_own_height_and_hash() {
    let _hash_guard = set_hash_fail();
    let mut state = State {
        height: 100,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([0x22; 32]),
        timestamp:   ts(1000),
        nonce:       7,
    }];

    let failure = state
        .apply_headers::<TestConsensus>(&headers, &[])
        .expect_err("the mocked hash exceeds the target");

    assert_eq!(failure, ApplyError::PowInsufficient {
        height: 101,
        hash:   BlockHash::new([0xFF; 32]),
        nbits:  CompactTarget::new(GENESIS_NBITS),
    });
    assert_eq!(state.height, 100, "the rejected header must not advance the tip");
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 1_000_000)]
fn unmocked_chain_rejects_corrupted_nonce_at_height_1() {
    let fixture_bytes = zkvm_test::fixture();
    let blocks = parse_fixture_blocks(&fixture_bytes);

    let mut state = State::anchor::<TestConsensus>();

    let mut header_inputs: Vec<HeaderInput> = blocks[1..=10]
        .iter()
        .map(|(_, b)| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   b.timestamp,
            nonce:       b.nonce,
        })
        .collect();
    header_inputs[0].nonce ^= 0xFF;

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &[])
        .unwrap_err();

    assert_eq!(state, State::anchor::<TestConsensus>());
    match err {
        ApplyError::PowInsufficient { height, .. } => {
            assert_eq!(height, 1);
        },
        other => panic!("expected PowInsufficient, got {:?}", other),
    }
}

#[zkvm_test(fixture = "headers_fixture.bin")]
fn unmocked_chain_rejects_parent_blockhash_mismatch() {
    let fixture_bytes = zkvm_test::fixture();
    let blocks = parse_fixture_blocks(&fixture_bytes);

    // Set an invalid parent block hash in the state
    let mut state = State::anchor::<TestConsensus>();
    state.block_hash = BlockHash::new([0xAA; 32]);

    let header_inputs: Vec<HeaderInput> = blocks[1..=10]
        .iter()
        .map(|(_, b)| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   b.timestamp,
            nonce:       b.nonce,
        })
        .collect();

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &[])
        .unwrap_err();
    assert!(matches!(err, ApplyError::PowInsufficient { height: 1, .. }));
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn block_work_witness_rejects_understated_work() {
    let target = zkpow_params::Mainnet::PARAMS.genesis_target;
    let (divisor, true_work) = crate::compute_block_work_division(&target);
    let mut bad_work_u256 = true_work.into_inner();
    let _ = crate::sub_assign(&mut bad_work_u256, zkpow_types::u256::from_limbs([1, 0, 0, 0]));
    let bad_work = zkpow_types::BlockWork::from_limbs(bad_work_u256.into_limbs());
    crate::verify_block_work_witness(bad_work, divisor);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn block_work_witness_rejects_overstated_work() {
    let target = zkpow_params::Mainnet::PARAMS.genesis_target;
    let (divisor, true_work) = crate::compute_block_work_division(&target);
    let bad_work = zkpow_types::BlockWork::from_limbs(
        (true_work.into_inner() + zkpow_types::u256::from_limbs([1, 0, 0, 0])).into_limbs(),
    );
    crate::verify_block_work_witness(bad_work, divisor);
}
