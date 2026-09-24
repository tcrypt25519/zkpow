use alloc::vec::Vec;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{
    BlockTimestamp, CompactTarget, HeaderInput, State, WINDOW_SIZE,
};

#[zkvm_test]
fn header_rejects_timestamp_equal_to_median() {
    let _hash_guard = set_hash_success();
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
    state.apply_headers_from_genesis::<TestConsensus>(&headers10, &hints).unwrap();

    let median = median_time_past(&state);
    let hdr = HeaderInput {
        version:     1,
        merkle_root: [0x66; 32],
        timestamp:   median,
        nonce:       1,
    };
    let median_hints = median_hints_for_headers(&state, &[hdr]);
    let failure = state
        .apply_headers::<TestConsensus>(&[hdr], &median_hints)
        .expect_err("timestamp equal to median must fail");
    assert_eq!(expect_error_code(failure), ErrorCode::TIMESTAMP_TOO_OLD);
}

#[zkvm_test]
fn header_rejects_timestamp_older_than_median_deep_in_batch() {
    const DIP_INDEX: usize = 6;

    let state = ordered_warm_state(20);
    let base = 1_000 + 20 * 600;
    let timestamps: Vec<u32> = (1..=12u32).map(|offset| base + offset * 600).collect();

    let _hash_guard = set_hash_success();

    let mut parent = state;
    apply_sequentially(&mut parent, &headers_from_timestamps(&timestamps[..DIP_INDEX])).unwrap();
    let median = median_time_past(&parent).into_inner();

    for (dip, expected) in [
        (
            median - 1,
            Err(ApplyError::TimestampTooOld {
                height:    parent.height + 1,
                timestamp: ts(median - 1),
                median:    ts(median),
            }),
        ),
        (
            median,
            Err(ApplyError::TimestampTooOld {
                height:    parent.height + 1,
                timestamp: ts(median),
                median:    ts(median),
            }),
        ),
    ] {
        let mut dipped = timestamps.clone();
        dipped[DIP_INDEX] = dip;
        let headers = headers_from_timestamps(&dipped);

        let mut batched = state;
        let hints = median_hints_for_headers(&batched, &headers);
        assert_eq!(
            batched.apply_headers::<TestConsensus>(&headers, &hints),
            expected,
            "timestamp {dip} against median {median}"
        );
        assert_eq!(batched.height, parent.height);
        assert_eq!(batched.timestamps, parent.timestamps, "failed batch must keep the sealed timestamps");
    }
}

#[zkvm_test]
fn cold_batch_failure_keeps_sealed_timestamps() {
    const DIP_INDEX: usize = 5;
    let _hash_guard = set_hash_success();
    let mut timestamps: Vec<u32> = (1..=10).map(|i| i * 10).collect();
    timestamps[DIP_INDEX] = 0;
    let headers = headers_from_timestamps(&timestamps);

    let mut state = test_state();
    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[])
        .expect_err("the dipped header must fail");
    assert_eq!(expect_error_code(failure), ErrorCode::TIMESTAMP_TOO_OLD);
    assert_eq!(state.height as usize, DIP_INDEX);
    for (height, expected) in timestamps[..DIP_INDEX].iter().enumerate() {
        assert_eq!(state.timestamps[height + 1], ts(*expected));
    }
    assert_eq!(state.timestamps[DIP_INDEX + 1], ts(0), "the rejected header must not be stored");
}

#[cfg(not(target_os = "zkvm"))]
#[test]
fn hint_validation_rejects_wrong_rank_hint() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(7), ts(8), ts(9), ts(10), ts(11)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(12),
        nonce:       7,
    }];

    let result = std::panic::catch_unwind(|| {
        let _hash_guard = set_hash_success();
        let mut state = state;
        state.apply_headers::<TestConsensus>(&headers, &[ts(4)]).unwrap();
    });

    assert!(result.is_err());
}

#[cfg(target_os = "zkvm")]
#[zkvm_test(should_panic, exit_code = 3)]
fn hint_validation_rejects_claimed_median_older_than_half_window_zkvm() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(7), ts(8), ts(9), ts(10), ts(11)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(12),
        nonce:       7,
    }];
    let _hash_guard = set_hash_success();
    let mut state = state;
    // ts(4) is older than the median (ts(6)) -> less_count=3, equal_count=1 <= 5 -> panics with exit 3
    state.apply_headers::<TestConsensus>(&headers, &[ts(4)]).unwrap();
}

#[cfg(target_os = "zkvm")]
#[zkvm_test(should_panic, exit_code = 3)]
fn hint_validation_rejects_claimed_median_newer_than_half_window_zkvm() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(7), ts(8), ts(9), ts(10), ts(11)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(12),
        nonce:       7,
    }];
    let _hash_guard = set_hash_success();
    let mut state = state;
    // ts(9) is newer than the median -> less_count=8 > 5 -> panics with exit 3
    state.apply_headers::<TestConsensus>(&headers, &[ts(9)]).unwrap();
}

#[cfg(target_os = "zkvm")]
#[zkvm_test(should_panic, exit_code = 3)]
fn hint_validation_rejects_claimed_median_not_in_window_zkvm() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(10), ts(20), ts(30), ts(40), ts(50), ts(60), ts(70), ts(80), ts(90), ts(100), ts(110)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(120),
        nonce:       7,
    }];
    let _hash_guard = set_hash_success();
    let mut state = state;
    // ts(65) is not in the window (equal_count == 0) -> panics with exit 3
    state.apply_headers::<TestConsensus>(&headers, &[ts(65)]).unwrap();
}

#[cfg(target_os = "zkvm")]
#[zkvm_test(should_panic, exit_code = 3)]
fn hint_validation_rejects_leftover_trailing_median_hints_zkvm() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(7), ts(8), ts(9), ts(10), ts(11)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(12),
        nonce:       7,
    }];
    let _hash_guard = set_hash_success();
    let mut state = state;
    // Header only needs 1 hint (ts(6)), but prover supplied 2 hints -> leftover hint panics with exit 3
    state.apply_headers::<TestConsensus>(&headers, &[ts(6), ts(99)]).unwrap();
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 8_000_000)]
fn unmocked_chain_rejects_timestamp_too_old_at_height_12() {
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

    let mut header_inputs: Vec<HeaderInput> = blocks[1..=13]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();
    header_inputs[11].timestamp = BlockTimestamp::new(1231006505);

    let hints = median_hints_for_headers(&state, &header_inputs);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();

    assert_eq!(err.error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
    match err {
        ApplyError::TimestampTooOld { height, timestamp, median } => {
            assert_eq!(height, 12);
            assert_eq!(timestamp, BlockTimestamp::new(1231006505));
            assert!(timestamp <= median);
        }
        other => panic!("expected TimestampTooOld, got {:?}", other),
    }

    let pv_bytes = failure_public_value_bytes(&state, ErrorCode::TIMESTAMP_TOO_OLD);
    let parsed_pv = PublicValues::parse(&pv_bytes).expect("parse public values");
    assert!(parsed_pv.is_failure());
    assert_eq!(parsed_pv.validation_error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
    assert_eq!(parsed_pv.state.height + 1, 12);
}
