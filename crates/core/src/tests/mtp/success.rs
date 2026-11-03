use alloc::vec::Vec;

use zkpow_types::CompactTarget;
use zkvm_test::zkvm_test;

use crate::{tests::*, HeaderInput, State};

#[zkvm_test]
fn ordered_window_median_matches_sequential_across_median_window_wrap() {
    let _hash_guard = set_hash_success();
    let headers: Vec<HeaderInput> = (1..=23)
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([timestamp as u8; 32]),
            timestamp:   ts(timestamp),
            nonce:       timestamp,
        })
        .collect();

    let mut sequential = test_state();
    let (initial, rest) = headers.split_at(10);

    sequential
        .apply_headers_from_genesis::<TestConsensus>(initial, &[])
        .expect("first 10 headers should validate");
    for header in rest {
        sequential
            .apply_headers::<TestConsensus>(core::slice::from_ref(header), &[])
            .expect("sequential validation should succeed");
    }

    let mut batched = test_state();
    batched
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[])
        .expect("batched validation should succeed");

    assert_eq!(batched, sequential);
}

#[zkvm_test]
fn cold_unordered_window_uses_sorted_median_then_warm_hint() {
    let _hash_guard = set_hash_success();
    let headers = headers_from_timestamps(&[100, 200, 150, 250, 300, 350, 400, 450, 500, 550, 600]);
    let mut state = test_state();

    state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[ts(300)])
        .expect("cold unordered and warm hinted paths should both validate");
    assert_eq!(state.height, 11);
}

#[zkvm_test]
fn hinted_median_validation_accepts_duplicate_median_values() {
    let _hash_guard = set_hash_success();
    let mut state: State = State {
        height: 10,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(6), ts(6), ts(7), ts(9), ts(8)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([0x22; 32]),
        timestamp:   ts(10),
        nonce:       7,
    }];

    state
        .apply_headers::<TestConsensus>(&headers, &[ts(6)])
        .expect("duplicate median values should be accepted");
}

#[zkvm_test(max_cycles = 8_000_000)]
fn ordered_window_median_is_batch_split_invariant() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let initial = headers_from_timestamps(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    state.apply_headers_from_genesis::<TestConsensus>(&initial, &[]).unwrap();
    let headers = headers_from_timestamps(&[11, 12, 13, 10, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]);

    // The decrease at height 14 needs hints for heights 15 through 24.
    // These are the medians of those specific prior windows, listed independently of MTP code.
    let hints = [9, 10, 10, 11, 12, 13, 15, 16, 17, 18].map(ts);
    let hints_per_header = [0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10];
    let mut batched = state;
    batched.apply_headers::<TestConsensus>(&headers, &hints).unwrap();
    assert_eq!(batched.height, 25);
    assert_eq!(batched.chain_work, TestConsensus::PARAMS.genesis_work * 25);

    for split in 1..headers.len() {
        let mut split_state = state;
        let hint_count = hints_per_header[split - 1];
        split_state
            .apply_headers::<TestConsensus>(&headers[..split], &hints[..hint_count])
            .unwrap();
        split_state
            .apply_headers::<TestConsensus>(&headers[split..], &hints[hint_count..])
            .unwrap();
        assert_eq!(split_state, batched, "batch split at header index {split}");
    }
}

#[zkvm_test]
fn cold_batch_checks_the_upper_median_before_each_header() {
    let _hash_guard = set_hash_success();
    for height in [2, 4, 6, 8, 10] {
        // Prior timestamps are 0, 10, ..., (height - 1) * 10.
        // With an even count, the upper median is height / 2 * 10.
        let median = height / 2 * 10;
        for timestamp in [median - 1, median, median + 1] {
            let mut timestamps: Vec<u32> = (1..=10).map(|i| i * 10).collect();
            timestamps[height as usize - 1] = timestamp;
            let headers = headers_from_timestamps(&timestamps);
            let mut state = test_state();
            let result = state.apply_headers_from_genesis::<TestConsensus>(&headers, &[]);
            if timestamp <= median {
                assert_eq!(
                    result,
                    Err(ApplyError::TimestampTooOld {
                        height,
                        timestamp: ts(timestamp),
                        median: ts(median),
                    })
                );
                assert_eq!(state.height, height - 1);
            } else {
                result.unwrap();
                assert_eq!(state.height, 10);
            }
        }
    }
}
