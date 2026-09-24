use alloc::vec;
use alloc::vec::Vec;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{
    block_work_from_target, ApplyError, ChainWork, CompactTarget, ErrorCode, HeaderInput, State, WINDOW_SIZE,
};

#[zkvm_test]
fn cold_start_rejects_fewer_than_ten_headers() {
    let mut state = test_state();
    let headers = vec![
        HeaderInput {
            version:     1,
            merkle_root: [0; 32],
            timestamp:   ts(100),
            nonce:       0,
        };
        9
    ];
    let err = state.apply_headers_from_genesis::<TestConsensus>(&headers, &[]).unwrap_err();
    assert_eq!(
        err,
        ApplyError::InsufficientColdHeaders {
            required: 10,
            actual:   9,
        }
    );
}

#[cfg(not(target_os = "zkvm"))]
#[test]
#[should_panic(expected = "apply_headers requires a full MTP window")]
fn warm_apply_rejects_a_cold_state() {
    let mut state = test_state();
    let header = HeaderInput {
        version:     1,
        merkle_root: [0; 32],
        timestamp:   ts(100),
        nonce:       0,
    };
    let _ = state.apply_headers::<TestConsensus>(&[header], &[]);
}

#[zkvm_test]
fn failure_height_reports_absolute_chain_height() {
    let _hash_guard = set_hash_fail();
    // Start at the first warm height and fail on the first new header.
    let mut state: State = State {
        height: (WINDOW_SIZE - 1) as u32,
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
        .expect_err("should fail PoW");

    assert_eq!(expect_error_code(failure), ErrorCode::POW_INSUFFICIENT);
    assert_eq!(state.height + 1, WINDOW_SIZE as u32);
}

#[zkvm_test]
fn apply_headers_flushes_deferred_chain_work_before_failure() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let mut headers: Vec<HeaderInput> = (1..=11)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    headers.push(HeaderInput {
        version:     1,
        merkle_root: [12; 32],
        timestamp:   ts(0),
        nonce:       12,
    });

    let hints = median_hints_for_headers(&state, &headers);

    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect_err("12th header should fail timestamp validation");

    let work = block_work_from_target(GENESIS_TARGET);
    assert_eq!(state.chain_work, ChainWork::default() + work * 11);
    assert_eq!(state.height + 1, 12);
}

#[zkvm_test]
fn apply_headers_failure_output_includes_flushed_chain_work() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let mut headers: Vec<HeaderInput> = (1..=11)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: [i as u8; 32],
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();
    headers.push(HeaderInput {
        version:     1,
        merkle_root: [12; 32],
        timestamp:   ts(0),
        nonce:       12,
    });

    let hints = median_hints_for_headers(&state, &headers);
    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect_err("validation should fail on the 12th header");

    let public_values = failure_public_value_bytes(&state, expect_error_code(failure));
    let parsed = PublicValues::parse(&public_values).unwrap();
    assert!(parsed.is_failure());
    assert_eq!(parsed.validation_error_code().unwrap(), ErrorCode::TIMESTAMP_TOO_OLD);
    assert_eq!(parsed.state.height + 1, 12);
    assert_eq!(parsed.state.chain_work, state.chain_work);
}
