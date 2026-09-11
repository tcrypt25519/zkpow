use alloc::vec::Vec;
use sp1_cycle::run_track;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{
    advance_ordered_timestamp_run, timestamp_slot,
    CompactTarget, HeaderInput, State, WINDOW_SIZE,
};

#[zkvm_test]
fn ordered_window_median_accepts_strictly_increasing_timestamps() {
    let cases = [("initial through first wrap", 0, 12), ("two wraps", 1, 23)];

    for (name, start_height, end_height) in cases {
        let mut state = test_state();

        if start_height == 0 {
            assert_eq!(median_time_past(&state), expected_upper_median(0), "{name}: height 0");
        }

        for height in 1..=end_height {
            state.height = height;
            state.timestamps[timestamp_slot(height)] = ts(height);
            assert_eq!(state.height, height, "{name}: state height");
            if height >= start_height {
                assert_eq!(
                    median_time_past(&state),
                    expected_upper_median(height),
                    "{name}: median at height {height}"
                );
            }
        }
    }
}

#[zkvm_test(max_cycles = 500_000)]
fn ring_buffer_slot_overwrites_only_next_slot() {
    let _hash_guard = set_hash_success();
    let original = [
        ts(10),
        ts(20),
        ts(30),
        ts(40),
        ts(50),
        ts(60),
        ts(70),
        ts(80),
        ts(90),
        ts(100),
        ts(110),
    ];
    let mut state: State = State {
        block_hash: BlockHash::new([0x11; 32]),
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        height: WINDOW_SIZE as u32,
        timestamps: original,
        ..State::default()
    };
    let header = HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(999),
        nonce:       7,
    };
    let median_hints = median_hints_for_headers(&state, &[header]);
    run_track("ring_buffer_slot_advance", || {
        state.apply_headers::<TestConsensus>(&[header], &median_hints).unwrap();
    });

    let mut expected = original;
    expected[1] = ts(999);
    assert_eq!(state.timestamps, expected);
}

#[zkvm_test]
fn ordered_window_median_matches_sequential_across_median_window_wrap() {
    let _hash_guard = set_hash_success();
    let headers: Vec<HeaderInput> = (1..=23)
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: [timestamp as u8; 32],
            timestamp:   ts(timestamp),
            nonce:       timestamp,
        })
        .collect();

    let mut sequential = test_state();
    let (initial, rest) = headers.split_at(10);
    let median_hints = median_hints_for_headers(&sequential, initial);
    sequential
        .apply_headers_from_genesis::<TestConsensus>(initial, &median_hints)
        .expect("first 10 headers should validate");
    for header in rest {
        let median_hints = median_hints_for_headers(&sequential, core::slice::from_ref(header));
        sequential
            .apply_headers::<TestConsensus>(core::slice::from_ref(header), &median_hints)
            .expect("sequential validation should succeed");
    }

    let hints = median_hints_for_headers(&test_state(), &headers);
    let mut batched = test_state();
    batched
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect("batched validation should succeed");

    assert_eq!(batched, sequential);
}

#[zkvm_test]
fn hinted_apply_headers_matches_sorted_apply_headers_across_window_wrap() {
    let _hash_guard = set_hash_success();
    let headers: Vec<HeaderInput> = (1..=23)
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: [timestamp as u8; 32],
            timestamp:   ts(timestamp),
            nonce:       timestamp,
        })
        .collect();
    let state = test_state();
    let hints = median_hints_for_headers(&state, &headers);

    let mut hinted = state;
    hinted
        .apply_headers_from_genesis::<TestConsensus>(&headers, &hints)
        .expect("hinted validation should succeed");
    assert_eq!(hinted.height, 23);
}

#[zkvm_test]
fn hinted_median_validation_accepts_duplicate_median_values() {
    let _hash_guard = set_hash_success();
    let mut state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(6), ts(6), ts(7), ts(8), ts(9)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(10),
        nonce:       7,
    }];

    state
        .apply_headers::<TestConsensus>(&headers, &[ts(6)])
        .expect("duplicate median values should be accepted");
}

#[zkvm_test]
fn ordered_window_median_is_batch_split_invariant() {
    for dip_index in 0..12usize {
        let state = ordered_warm_state(20);

        let base = 1_000 + 20 * 600;
        let mut timestamps: Vec<u32> = (1..=12u32).map(|offset| base + offset * 600).collect();
        // Out of order versus its parent, but still comfortably above the real median-time-past.
        timestamps[dip_index] -= 1_500;
        let headers = headers_from_timestamps(&timestamps);

        let _hash_guard = set_hash_success();

        let mut sequential = state;
        let sequential_result = apply_sequentially(&mut sequential, &headers);

        let mut batched = state;
        let hints = median_hints_for_headers(&batched, &headers);
        let batched_result = batched.apply_headers::<TestConsensus>(&headers, &hints);

        assert_eq!(sequential_result, Ok(()), "dip at index {dip_index} is a valid block");
        assert_eq!(
            batched_result, sequential_result,
            "dip at index {dip_index}: batching changed the verdict"
        );
        assert_eq!(batched, sequential, "dip at index {dip_index}: batching changed the state");
    }
}

#[zkvm_test]
fn ordered_timestamp_run_increments_on_equal_timestamps() {
    let mut run = 5u64;
    // Equal timestamp must not reset the run
    run = advance_ordered_timestamp_run(run, 1000, 1000);
    assert_eq!(run, 6);
    // Increasing timestamp must increment the run
    run = advance_ordered_timestamp_run(run, 1000, 1001);
    assert_eq!(run, 7);
    // Decreasing timestamp resets the run to 0
    run = advance_ordered_timestamp_run(run, 1001, 1000);
    assert_eq!(run, 0);
}
