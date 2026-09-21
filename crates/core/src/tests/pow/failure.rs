use alloc::vec::Vec;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{
    ApplyError, BlockHash, CompactTarget, HeaderInput, State, WINDOW_SIZE,
};

#[zkvm_test]
fn header_rejects_mocked_hash_exceeding_target() {
    let _hash_guard = set_hash_fail();
    let mut state = test_state();
    let headers10: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    let hints = median_hints_for_headers(&state, &headers10);
    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers10, &hints)
        .expect_err("expected PoW failure");
    assert_eq!(expect_error_code(failure), ErrorCode::POW_INSUFFICIENT);
}

#[zkvm_test]
fn rejected_header_reports_its_own_height_and_hash() {
    let _hash_guard = set_hash_fail();
    let mut state = State {
        height: 5,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1000),
        nonce:       7,
    }];
    let hints = median_hints_for_headers(&state, &headers);

    let failure = state
        .apply_headers::<TestConsensus>(&headers, &hints)
        .expect_err("the mocked hash exceeds the target");

    assert_eq!(failure, ApplyError::PowInsufficient {
        height: 6,
        hash:   BlockHash::new([0xFF; 32]),
        nbits:  CompactTarget::new(GENESIS_NBITS),
    });
    assert_eq!(state.height, 5, "the rejected header must not advance the tip");
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 7_500_000)]
fn unmocked_chain_rejects_corrupted_nonce_at_height_1() {
    let fixture_str = zkvm_test::fixture_str();
    let blocks = parse_fixture_blocks(&fixture_str);

    let mut state = State {
        chain_work:      ChainWork::from_limbs(*TestConsensus::PARAMS.genesis_work.as_limbs()),
        genesis_hash:    TestConsensus::GENESIS_HASH,
        block_hash:      TestConsensus::GENESIS_HASH,
        height:          0,
        current_nbits:   TestConsensus::PARAMS.genesis.compact_target,
        epoch_timestamp: TestConsensus::PARAMS.genesis.timestamp,
        timestamps:      {
            let mut arr = [BlockTimestamp::new(0); WINDOW_SIZE];
            arr[0] = TestConsensus::PARAMS.genesis.timestamp;
            arr
        },
    };

    let mut header_inputs: Vec<HeaderInput> = blocks[1..=10]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();
    header_inputs[0].nonce ^= 0xFF;

    let hints = median_hints_for_headers(&state, &header_inputs);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();

    assert_eq!(err.error_code(), Some(ErrorCode::POW_INSUFFICIENT));
    match err {
        ApplyError::PowInsufficient { height, .. } => {
            assert_eq!(height, 1);
        }
        other => panic!("expected PowInsufficient, got {:?}", other),
    }

    let pv_bytes = failure_public_value_bytes(&state, ErrorCode::POW_INSUFFICIENT);
    let parsed_pv = PublicValues::parse(&pv_bytes).expect("parse public values");
    assert!(parsed_pv.is_failure());
    assert_eq!(parsed_pv.validation_error_code(), Some(ErrorCode::POW_INSUFFICIENT));
    assert_eq!(parsed_pv.state.height + 1, 1);
}

#[zkvm_test(fixture = "headers_fixture.bin")]
fn unmocked_chain_rejects_parent_blockhash_mismatch() {
    let fixture_str = zkvm_test::fixture_str();
    let blocks = parse_fixture_blocks(&fixture_str);

    // Set an invalid parent block hash in the state
    let mut state = State {
        chain_work:      ChainWork::from_limbs(*TestConsensus::PARAMS.genesis_work.as_limbs()),
        genesis_hash:    TestConsensus::GENESIS_HASH,
        block_hash:      BlockHash::new([0xAA; 32]), // Mismatched parent hash!
        height:          0,
        current_nbits:   TestConsensus::PARAMS.genesis.compact_target,
        epoch_timestamp: TestConsensus::PARAMS.genesis.timestamp,
        timestamps:      {
            let mut arr = [BlockTimestamp::new(0); WINDOW_SIZE];
            arr[0] = TestConsensus::PARAMS.genesis.timestamp;
            arr
        },
    };

    let header_inputs: Vec<HeaderInput> = blocks[1..=10]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();

    let hints = median_hints_for_headers(&state, &header_inputs);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();
    assert!(matches!(err, ApplyError::PowInsufficient { height: 1, .. }));
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn block_work_witness_rejects_understated_work() {
    let target = zkpow_params::Mainnet::PARAMS.genesis_target;
    let true_work = crate::compute_block_work_division(target);
    let mut bad_work_u256 = true_work.into_inner();
    let _ = crate::sub_assign(&mut bad_work_u256, zkpow_types::u256::from_limbs([1, 0, 0, 0]));
    let bad_work = zkpow_types::BlockWork::from_limbs(bad_work_u256.into_limbs());
    let divisor = crate::increment_target(target.into_inner());
    crate::verify_block_work_witness(bad_work, divisor);
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn block_work_witness_rejects_overstated_work() {
    let target = zkpow_params::Mainnet::PARAMS.genesis_target;
    let true_work = crate::compute_block_work_division(target);
    let bad_work = zkpow_types::BlockWork::from_limbs(
        (true_work.into_inner() + zkpow_types::u256::from_limbs([1, 0, 0, 0])).into_limbs(),
    );
    let divisor = crate::increment_target(target.into_inner());
    crate::verify_block_work_witness(bad_work, divisor);
}

