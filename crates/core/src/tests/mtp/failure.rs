use alloc::vec::Vec;

use zkvm_test::zkvm_test;

use crate::{tests::*, HeaderInput, State};

#[zkvm_test]
fn header_rejects_timestamp_equal_to_median() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers10: Vec<HeaderInput> = (1..=10)
        .map(|i| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([i as u8; 32]),
            timestamp:   ts(i * 10),
            nonce:       i,
        })
        .collect();

    state.apply_headers_from_genesis::<TestConsensus>(&headers10, &[]).unwrap();

    let median = ts(50);
    let hdr = HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([0x66; 32]),
        timestamp:   median,
        nonce:       1,
    };

    let failure = state
        .apply_headers::<TestConsensus>(&[hdr], &[])
        .expect_err("timestamp equal to median must fail");
    assert_eq!(expect_error_code(failure), ErrorCode::TIMESTAMP_TOO_OLD);
}

#[zkvm_test]
fn header_rejects_timestamp_older_than_median_deep_in_batch() {
    const DIP_INDEX: usize = 6;

    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let prefix: Vec<u32> = (1..=20).map(|height| 1_000 + height * 600).collect();
    state
        .apply_headers_from_genesis::<TestConsensus>(&headers_from_timestamps(&prefix), &[])
        .unwrap();
    let timestamps: Vec<u32> = (1..=12).map(|offset| 13_000 + offset * 600).collect();
    let mut parent = state;
    parent
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&timestamps[..DIP_INDEX]), &[])
        .unwrap();
    // Before height 27 the window contains heights 16 through 26; its median is height 21.
    let median = 13_600;

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

        assert_eq!(
            batched.apply_headers::<TestConsensus>(&headers, &[]),
            expected,
            "timestamp {dip} against median {median}"
        );
        assert_eq!(batched.height, parent.height);
        assert_eq!(
            batched.timestamps, parent.timestamps,
            "failed batch must keep the sealed timestamps"
        );
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

fn unordered_state() -> State {
    State {
        height: 10,
        timestamps: [
            ts(10),
            ts(20),
            ts(30),
            ts(40),
            ts(50),
            ts(60),
            ts(70),
            ts(80),
            ts(90),
            ts(110),
            ts(100),
        ],
        ..test_state()
    }
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn hint_validation_rejects_claimed_median_older_than_half_window_zkvm() {
    let _hash_guard = set_hash_success();
    unordered_state()
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&[120]), &[ts(40)])
        .unwrap();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn hint_validation_rejects_claimed_median_newer_than_half_window_zkvm() {
    let _hash_guard = set_hash_success();
    unordered_state()
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&[120]), &[ts(90)])
        .unwrap();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn hint_validation_rejects_claimed_median_not_in_window_zkvm() {
    let _hash_guard = set_hash_success();
    unordered_state()
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&[120]), &[ts(65)])
        .unwrap();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn hint_validation_rejects_leftover_trailing_median_hints_zkvm() {
    let _hash_guard = set_hash_success();
    unordered_state()
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&[120]), &[ts(60), ts(99)])
        .unwrap();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn unordered_window_rejects_a_missing_median_hint() {
    let _hash_guard = set_hash_success();
    unordered_state()
        .apply_headers::<TestConsensus>(&headers_from_timestamps(&[120]), &[])
        .unwrap();
}

#[zkvm_test(should_panic(expected = "halt: invalid hint (3)"), exit_code = 3)]
fn cold_batch_rejects_leftover_median_hints() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers = headers_from_timestamps(&(1..=10).map(|i| i * 10).collect::<Vec<_>>());
    state.apply_headers_from_genesis::<TestConsensus>(&headers, &[ts(99)]).unwrap();
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 1_000_000)]
fn unmocked_chain_rejects_timestamp_too_old_at_height_12() {
    let fixture_bytes = zkvm_test::fixture();
    let blocks = parse_fixture_blocks(&fixture_bytes);

    let mut state = State::anchor::<TestConsensus>();

    let mut header_inputs: Vec<HeaderInput> = blocks[1..=13]
        .iter()
        .map(|(_, b)| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   b.timestamp,
            nonce:       b.nonce,
        })
        .collect();
    header_inputs[11].timestamp = BlockTimestamp::new(1231006505);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &[])
        .unwrap_err();

    assert_eq!(state.height, 11);
    match err {
        ApplyError::TimestampTooOld {
            height,
            timestamp,
            median,
        } => {
            assert_eq!(height, 12);
            assert_eq!(timestamp, BlockTimestamp::new(1231006505));
            assert!(timestamp <= median);
        },
        other => panic!("expected TimestampTooOld, got {:?}", other),
    }
}
