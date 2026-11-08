use alloc::{vec, vec::Vec};

use zkpow_types::ChainWork;
use zkvm_test::zkvm_test;

use crate::{ApplyError, ErrorCode, HeaderInput, block_work_from_target, tests::*};

#[zkvm_test]
fn cold_start_rejects_fewer_than_ten_headers() {
    let mut state = test_state();
    let headers = vec![
        HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([0; 32]),
            timestamp:   ts(100),
            nonce:       0,
        };
        9
    ];
    let err = state.apply_headers_from_genesis::<TestConsensus>(&headers, &[]).unwrap_err();
    assert_eq!(err, ApplyError::InsufficientColdHeaders {
        required: 10,
        actual:   9,
    });
}

#[cfg(not(target_os = "zkvm"))]
#[test]
#[should_panic(expected = "a warm ring requires a full window")]
fn warm_apply_rejects_a_cold_state() {
    let mut state = test_state();
    let header = HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([0; 32]),
        timestamp:   ts(100),
        nonce:       0,
    };
    let _ = state.apply_headers::<TestConsensus>(&[header], &[]);
}

#[zkvm_test]
fn apply_headers_flushes_deferred_chain_work_before_failure() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let mut headers: Vec<HeaderInput> = (1..=11)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([i as u8; 32]),
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    headers.push(HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([12; 32]),
        timestamp:   ts(0),
        nonce:       12,
    });

    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[])
        .expect_err("12th header should fail timestamp validation");

    let work = block_work_from_target(&GENESIS_TARGET);
    assert_eq!(state.chain_work, ChainWork::default() + work * 11);
    assert_eq!(state.height + 1, 12);
}

#[zkvm_test]
fn cold_batch_failure_flushes_chain_work_for_accepted_headers() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let mut headers: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([i as u8; 32]),
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    headers[4].timestamp = ts(0);

    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[])
        .expect_err("5th header should fail timestamp validation");

    assert_eq!(expect_error_code(failure), ErrorCode::TIMESTAMP_TOO_OLD);
    assert_eq!(state.height, 4);
    assert_eq!(state.chain_work, ChainWork::default() + TestConsensus::PARAMS.genesis_work * 4);
}
