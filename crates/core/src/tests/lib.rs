use alloc::vec::Vec;

extern crate std;

mod input;

use super::*;


#[cfg(all(not(target_os = "zkvm"), test))]
#[must_use]
fn set_hash_success() -> test_hash_mock::MockHashGuard {
    test_hash_mock::set_success()
}

#[cfg(all(not(target_os = "zkvm"), test))]
#[must_use]
fn set_hash_fail() -> test_hash_mock::MockHashGuard {
    test_hash_mock::set_fail()
}

#[cfg(all(not(target_os = "zkvm"), test))]
mod test_hash_mock {
    std::thread_local! {
        static MOCK_HEADER_HASH: std::cell::Cell<Option<[u8; 32]>> =
            const { std::cell::Cell::new(None) };
    }

    pub(super) struct MockHashGuard;

    pub(super) fn get() -> Option<[u8; 32]> {
        MOCK_HEADER_HASH.with(|mock_hash| mock_hash.get())
    }

    pub(super) fn set_success() -> MockHashGuard {
        set([0; 32])
    }

    pub(super) fn set_fail() -> MockHashGuard {
        set([0xFF; 32])
    }

    fn set(hash: [u8; 32]) -> MockHashGuard {
        MOCK_HEADER_HASH.with(|mock_hash| {
            assert!(mock_hash.get().is_none(), "hash mock already set in this thread");
            mock_hash.set(Some(hash));
        });
        MockHashGuard
    }

    impl Drop for MockHashGuard {
        fn drop(&mut self) {
            MOCK_HEADER_HASH.with(|mock_hash| mock_hash.set(None));
        }
    }
}

#[cfg(all(not(target_os = "zkvm"), test))]
fn hash_header(
    version: u32,
    prev_blockhash: &BlockHash,
    merkle_root: [u8; 32],
    timestamp: BlockTimestamp,
    nbits: CompactTarget,
    nonce: u32,
) -> BlockHash {
    if let Some(hash) = test_hash_mock::get() {
        return BlockHash::new(hash);
    }
    host_hash_header(version, prev_blockhash, merkle_root, timestamp, nbits, nonce)
}


fn ts(seconds: u32) -> BlockTimestamp {
    BlockTimestamp::new(seconds)
}

fn zero_hash() -> BlockHash {
    BlockHash::new([0; 32])
}

fn hash_sample_header() -> BlockHash {
    hash_header(
        1,
        &BlockHash::new([0x11; 32]),
        [0x22; 32],
        ts(10),
        CompactTarget::new(GENESIS_NBITS),
        7,
    )
}

fn test_state() -> State {
    State {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    }
}

pub(crate) fn median_time_past(state: &State) -> BlockTimestamp {
    let count = timestamp_count(state.height as u64);
    let mut sorted = state.timestamps;
    if count >= WINDOW_SIZE {
        sorted.sort_unstable();
        sorted[WINDOW_SIZE / 2]
    } else {
        sorted[..count].sort_unstable();
        sorted[count / 2]
    }
}

fn apply_header(state: &mut State, timestamp: u32) {
    let _hash_guard = set_hash_success();
    let header = HeaderInput {
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(timestamp),
        nonce: 7,
    };
    let median_hints = median_hints_for_headers(state, &[header]);
    state.apply_headers(&[header], &median_hints).unwrap();
}

fn dense_median_hints_for_headers(initial_state: &State, headers: &[HeaderInput]) -> Vec<BlockTimestamp> {
    let mut state = *initial_state;
    let mut medians = Vec::with_capacity(headers.len());
    for header in headers {
        medians.push(median_time_past(&state));
        let timestamp_slot = (state.height as usize + 1) % WINDOW_SIZE;
        state.timestamps[timestamp_slot] = header.timestamp;
        state.height += 1;
    }
    medians
}

fn median_hints_for_headers(initial_state: &State, headers: &[HeaderInput]) -> Vec<BlockTimestamp> {
    let dense_hints = dense_median_hints_for_headers(initial_state, headers);
    let mut state = *initial_state;
    let warm_start = (WINDOW_SIZE - 1).saturating_sub(state.height as usize).min(headers.len());
    for header in &headers[..warm_start] {
        state.height += 1;
        state.timestamps[state.height as usize % WINDOW_SIZE] = header.timestamp;
    }

    let mut hints = Vec::new();
    if warm_start == headers.len() {
        return hints;
    }

    let mut ordered_run_len = ordered_timestamp_run(&state.timestamps, state.height.into());
    for index in warm_start..headers.len() {
        if !has_ordered_timestamp_window(ordered_run_len) {
            hints.push(dense_hints[index]);
        }
        let previous_timestamp = state.timestamps[state.height as usize % WINDOW_SIZE];
        state.height += 1;
        state.timestamps[state.height as usize % WINDOW_SIZE] = headers[index].timestamp;
        ordered_run_len = advance_ordered_timestamp_run(
            ordered_run_len,
            previous_timestamp.into_inner(),
            headers[index].timestamp.into_inner(),
        );
    }
    hints
}

fn failure_public_value_bytes(failure: &ApplyFailure) -> [u8; MINIMAL_PV_SIZE] {
    MinimalPublicValues::failure(
        &failure.last_valid_state.public_claim(),
        failure.error_code,
        failure.failure_height,
        [0xA5; 32],
        VerifierKeyDigest::from_raw([0xA5A5_A5A5; 8]),
    )
    .to_bytes()
}

fn expected_upper_median(height: u32) -> BlockTimestamp {
    if height < WINDOW_SIZE as u32 {
        ts(height.div_ceil(2))
    } else {
        ts(height - (WINDOW_SIZE as u32 / 2))
    }
}

#[test]
fn fixed_width_wire_sizes_match_protocol() {
    assert_eq!(NEW_HEADER_SIZE, 44);
    assert_eq!(STATE_SIZE, 144);
    assert_eq!(CLAIM_SIZE, 84);
    assert_eq!(CONTINUATION_DATA_SIZE, 52);
    assert_eq!(MINIMAL_PV_SIZE, 153);
    assert_eq!(align_of::<Header>(), 4);
    assert_eq!(align_of::<HeaderInput>(), 4);
    assert_eq!(align_of::<BlockWork>(), 8);
    assert_eq!(align_of::<ChainWork>(), 8);
    assert_eq!(align_of::<State>(), 8);
}

#[test]
fn minimal_public_values_round_trips() {
    let state: State = State {
        block_hash: BlockHash::new([0xAB; 32]),
        genesis_hash: BlockHash::new([0xCD; 32]),
        chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
        height: 42,
        ..State::default()
    };
    let digest = [0x11u8; 32];
    let verifier_key = VerifierKeyDigest::from_raw([9; 8]);
    let pv = MinimalPublicValues::success(&state.public_claim(), digest, verifier_key);
    let bytes = pv.to_bytes();
    assert_eq!(bytes.len(), MINIMAL_PV_SIZE);
    assert_eq!(bytes[121..153], verifier_key.to_bytes());
    let parsed = MinimalPublicValues::parse(&bytes).unwrap();
    assert_eq!(parsed.genesis_hash, state.genesis_hash);
    assert_eq!(parsed.tip_hash, state.block_hash);
    assert_eq!(parsed.height, state.height);
    assert_eq!(parsed.return_code, 0);
    assert_eq!(parsed.failure_height, 0);
    assert_eq!(parsed.continuation_digest, digest);
    assert_eq!(parsed.verifier_key, verifier_key);
}

#[test]
fn header_and_new_header_round_trip_exact_wire_bytes() {
    let mut header_bytes = [0u8; BLOCK_HEADER_SIZE];
    header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    header_bytes[4..36].copy_from_slice(&[0x55; 32]);
    header_bytes[36..68].copy_from_slice(&[0x66; 32]);
    header_bytes[68..72].copy_from_slice(&0x7788_99aau32.to_le_bytes());
    header_bytes[72..76].copy_from_slice(&0x1d00_ffffu32.to_le_bytes());
    header_bytes[76..80].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

    let header = Header::parse(&header_bytes).unwrap();
    assert_eq!(header.version, 0x1122_3344);
    assert_eq!(header.prev_blockhash, BlockHash::new([0x55; 32]));
    assert_eq!(header.merkle_root, [0x66; 32]);
    assert_eq!(header.timestamp, BlockTimestamp::new(0x7788_99aa));
    assert_eq!(header.compact_target, CompactTarget::new(0x1d00_ffff));
    assert_eq!(header.nonce, 0xbbcc_ddee);
    assert_eq!(header.to_bytes(), header_bytes);

    let mut new_header_bytes = [0u8; NEW_HEADER_SIZE];
    new_header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
    new_header_bytes[4..36].copy_from_slice(&[0x66; 32]);
    new_header_bytes[36..40].copy_from_slice(&0x7788_99aau32.to_le_bytes());
    new_header_bytes[40..44].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

    let new_header = HeaderInput::from_bytes(&new_header_bytes).unwrap();
    assert_eq!(new_header.version, 0x1122_3344);
    assert_eq!(new_header.merkle_root, [0x66; 32]);
    assert_eq!(new_header.timestamp, BlockTimestamp::new(0x7788_99aa));
    assert_eq!(new_header.nonce, 0xbbcc_ddee);
}

#[test]
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

#[test]
fn block_work_mul_u32_scales_by_small_count() {
    let value = BlockWork::from_limbs([3, 0, 0, 0]);

    let mut result = value;
    result *= 7;
    assert_eq!(result, BlockWork::from_limbs([21, 0, 0, 0]));
}

#[test]
fn check_proof_of_work_accepts_exact_target_boundary() {
    let hash = BlockHash::new(GENESIS_TARGET.to_le_bytes());
    assert!(check_proof_of_work(hash, GENESIS_TARGET));
}

#[test]
fn target_to_bits_round_trips_genesis_target() {
    let round_trip = target_to_bits(GENESIS_TARGET);
    assert_eq!(round_trip, CompactTarget::new(GENESIS_NBITS));
}

#[test]
fn target_from_bits_expands_genesis_bits() {
    let expanded = target_from_bits(CompactTarget::new(GENESIS_NBITS));
    assert_eq!(expanded, GENESIS_TARGET);
}

#[test]
fn scoped_hash_mock_overrides_and_restores_host_hashing() {
    let real_hash = hash_sample_header();
    {
        let _hash_guard = set_hash_success();
        assert_eq!(hash_sample_header(), zero_hash());
    }
    assert_eq!(hash_sample_header(), real_hash);

    {
        let _hash_guard = set_hash_fail();
        assert_eq!(hash_sample_header(), BlockHash::new([0xFF; 32]));
    }
    assert_eq!(hash_sample_header(), real_hash);
}

#[test]
fn scoped_hash_mock_rejects_nested_override() {
    let real_hash = hash_sample_header();
    let hash_guard = set_hash_success();

    let result = std::panic::catch_unwind(set_hash_fail);
    assert!(result.is_err());
    assert_eq!(hash_sample_header(), zero_hash());

    drop(hash_guard);
    assert_eq!(hash_sample_header(), real_hash);
}

#[test]
fn apply_headers_flushes_deferred_chain_work_on_success() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers = [
        HeaderInput {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(10),
            nonce: 7,
        },
        HeaderInput {
            version: 1,
            merkle_root: [0x33; 32],
            timestamp: ts(20),
            nonce: 8,
        },
    ];

    let hints = median_hints_for_headers(&state, &headers);
    state.apply_headers(&headers, &hints).expect("headers should validate");

    let work = block_work_from_target(GENESIS_TARGET).expect("genesis target is valid");
    let mut expected = work;
    expected *= 2;
    let expected = ChainWork::default() + expected + BlockWork::default();
    assert_eq!(state.chain_work, expected);
}

#[test]
fn apply_headers_flushes_deferred_chain_work_for_longer_run() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers = [
        HeaderInput {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(10),
            nonce: 7,
        },
        HeaderInput {
            version: 1,
            merkle_root: [0x33; 32],
            timestamp: ts(20),
            nonce: 8,
        },
        HeaderInput {
            version: 1,
            merkle_root: [0x44; 32],
            timestamp: ts(30),
            nonce: 9,
        },
    ];

    let hints = median_hints_for_headers(&state, &headers);
    state.apply_headers(&headers, &hints).expect("validation should succeed");

    let work = block_work_from_target(GENESIS_TARGET).expect("genesis target is valid");
    let mut expected = work;
    expected *= 3;
    let expected = ChainWork::default() + expected;
    assert_eq!(state.chain_work, expected);
}

#[test]
fn failure_height_is_absolute_chain_height() {
    let _hash_guard = set_hash_fail();
    // Start at height 5 and fail on the first new header → failure_height = 6.
    let mut state: State = State {
        height: 5,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    };
    let headers = [HeaderInput {
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(1000),
        nonce: 7,
    }];
    let hints = median_hints_for_headers(&state, &headers);
    // Use a hash that exceeds the target to trigger PowInsufficient.
    let failure = state.apply_headers(&headers, &hints).expect_err("should fail PoW");

    assert_eq!(failure.error_code, ValidationErrorCode::PowInsufficient);
    assert_eq!(failure.failure_height, 6); // last_valid_state.height(5) + 1
    assert_eq!(failure.last_valid_state.height, 5);
}

#[test]
fn apply_headers_flushes_deferred_chain_work_before_failure() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers = [
        HeaderInput {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(10),
            nonce: 7,
        },
        HeaderInput {
            version: 1,
            merkle_root: [0x33; 32],
            timestamp: ts(0),
            nonce: 8,
        },
    ];

    let hints = median_hints_for_headers(&state, &headers);

    let failure = state
        .apply_headers(&headers, &hints)
        .expect_err("second header should fail timestamp validation");

    let work = block_work_from_target(GENESIS_TARGET).expect("genesis target is valid");
    assert_eq!(failure.last_valid_state.chain_work, ChainWork::default() + work);
    assert_eq!(failure.failure_height, 2);
}

#[test]
fn apply_headers_failure_output_includes_flushed_chain_work() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let headers = [
        HeaderInput {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(10),
            nonce: 7,
        },
        HeaderInput {
            version: 1,
            merkle_root: [0x33; 32],
            timestamp: ts(0),
            nonce: 8,
        },
    ];

    let hints = median_hints_for_headers(&state, &headers);
    let failure = state
        .apply_headers(&headers, &hints)
        .expect_err("validation should fail on the second header");

    let public_values = failure_public_value_bytes(&failure);
    let parsed = MinimalPublicValues::parse(&public_values).unwrap();
    assert_eq!(parsed.return_code, ValidationErrorCode::TimestampTooOld.as_byte());
    assert_eq!(parsed.failure_height, 2);
    assert_eq!(parsed.chain_work, failure.last_valid_state.chain_work);
}

#[test]
fn apply_headers_matches_sequential_next_across_median_window_wrap() {
    let _hash_guard = set_hash_success();
    let headers: Vec<HeaderInput> = (1..=23)
        .map(|timestamp| HeaderInput {
            version: 1,
            merkle_root: [timestamp as u8; 32],
            timestamp: ts(timestamp),
            nonce: timestamp,
        })
        .collect();

    let mut sequential = test_state();
    for header in headers.iter().copied() {
        let median_hints = median_hints_for_headers(&sequential, &[header]);
        sequential
            .apply_headers(&[header], &median_hints)
            .expect("sequential validation should succeed");
    }

    let hints = median_hints_for_headers(&test_state(), &headers);
    let mut batched = test_state();
    batched
        .apply_headers(&headers, &hints)
        .expect("batched validation should succeed");

    assert_eq!(batched, sequential);
}

#[test]
fn hinted_apply_headers_matches_sorted_apply_headers_across_window_wrap() {
    let _hash_guard = set_hash_success();
    let headers: Vec<HeaderInput> = (1..=23)
        .map(|timestamp| HeaderInput {
            version: 1,
            merkle_root: [timestamp as u8; 32],
            timestamp: ts(timestamp),
            nonce: timestamp,
        })
        .collect();
    let state = test_state();
    let hints = median_hints_for_headers(&state, &headers);

    let mut hinted = state;
    hinted
        .apply_headers(&headers, &hints)
        .expect("hinted validation should succeed");
    assert_eq!(hinted.height, 23);
}

#[test]
fn hinted_median_validation_accepts_duplicate_median_values() {
    let _hash_guard = set_hash_success();
    let mut state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(6), ts(6), ts(7), ts(8), ts(9)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(10),
        nonce: 7,
    }];

    state
        .apply_headers(&headers, &[ts(6)])
        .expect("duplicate median values should be accepted");
}

#[test]
fn hinted_median_validation_rejects_wrong_rank_hint() {
    let state: State = State {
        height: WINDOW_SIZE as u32,
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        timestamps: [ts(1), ts(2), ts(3), ts(4), ts(5), ts(6), ts(7), ts(8), ts(9), ts(10), ts(11)],
        ..State::default()
    };
    let headers = [HeaderInput {
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(12),
        nonce: 7,
    }];

    let result = std::panic::catch_unwind(|| {
        let _hash_guard = set_hash_success();
        let mut state = state;
        state.apply_headers(&headers, &[ts(4)]).unwrap();
    });

    assert!(result.is_err());
}

#[test]
fn apply_headers_treats_height_zero_state_as_trusted_anchor() {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    state.genesis_hash = BlockHash::new([0x11; 32]);
    state.block_hash = BlockHash::new([0x22; 32]);
    let headers = [HeaderInput {
        version: 1,
        merkle_root: [0x33; 32],
        timestamp: ts(10),
        nonce: 7,
    }];
    let hints = median_hints_for_headers(&state, &headers);
    state
        .apply_headers(&headers, &hints)
        .expect("height-zero anchor should already be trusted");

    assert_eq!(state.height, 1);
    assert_eq!(state.genesis_hash, BlockHash::new([0x11; 32]));
    assert_eq!(state.block_hash, zero_hash());
}

#[test]
fn median_time_past_uses_upper_median_across_window_sizes() {
    let cases = [("initial through first wrap", 0, 12), ("two wraps", 1, 23)];

    for (name, start_height, end_height) in cases {
        let mut state = test_state();

        if start_height == 0 {
            assert_eq!(median_time_past(&state), expected_upper_median(0), "{name}: height 0");
        }

        for height in 1..=end_height {
            apply_header(&mut state, height);
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

#[test]
fn next_overwrites_only_the_next_ring_slot() {
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
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(999),
        nonce: 7,
    };
    let median_hints = median_hints_for_headers(&state, &[header]);
    state.apply_headers(&[header], &median_hints).unwrap();

    let mut expected = original;
    expected[1] = ts(999);
    assert_eq!(state.timestamps, expected);
}

#[test]
fn continuation_data_serializes_to_fixed_width() {
    let pcs = ContinuationData {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        epoch_start_timestamp: BlockTimestamp::new(500),
        timestamps: [BlockTimestamp::new(10); WINDOW_SIZE],
    };
    let bytes = pcs.to_bytes();
    assert_eq!(bytes.len(), CONTINUATION_DATA_SIZE);
    let parsed = ContinuationData::parse(&bytes).unwrap();
    assert_eq!(parsed, pcs);
}

#[test]
fn state_continuation_bytes_matches_continuation_data_to_bytes() {
    let state = State {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        epoch_start_timestamp: BlockTimestamp::new(500),
        timestamps: [BlockTimestamp::new(10); WINDOW_SIZE],
        ..Default::default()
    };
    let pcs = ContinuationData::from_state(&state);
    assert_eq!(state.continuation_bytes(), pcs.to_bytes());
}

#[test]
fn claim_serializes_to_fixed_width() {
    let claim = Claim {
        genesis_hash: BlockHash::new([1; 32]),
        tip_hash: BlockHash::new([2; 32]),
        chain_work: ChainWork::from_limbs([3, 4, 0, 0]),
        height: 99,
    };
    let bytes = claim.to_bytes();
    assert_eq!(bytes.len(), CLAIM_SIZE);
    assert_eq!(bytes[64..72], 3u64.to_le_bytes());
    assert_eq!(bytes[72..80], 4u64.to_le_bytes());
    assert_eq!(bytes[80..84], 99u32.to_le_bytes());
    let parsed = Claim::parse(&bytes).unwrap();
    assert_eq!(parsed, claim);
}

// Difficulty retargeting edge cases using the pure helper. These represent
// the extremes the state machine will clamp to during epoch transitions.
#[test]
fn retarget_timespan_cases_adjust_target_direction() {
    enum ExpectedDirection {
        Equal,
        NotGreater,
        NotLess,
    }

    let cases = [
        ("expected timespan", EXPECTED_EPOCH_TIMESPAN, ExpectedDirection::Equal),
        ("shorter timespan", EXPECTED_EPOCH_TIMESPAN / 4, ExpectedDirection::NotGreater),
        ("longer timespan", EXPECTED_EPOCH_TIMESPAN * 4, ExpectedDirection::NotLess),
        (
            "minimum clamped timespan",
            MIN_EPOCH_TIMESPAN as u32,
            ExpectedDirection::NotGreater,
        ),
        (
            "maximum clamped timespan",
            MAX_EPOCH_TIMESPAN as u32,
            ExpectedDirection::NotLess,
        ),
    ];

    for (name, timespan, expected) in cases {
        let old = GENESIS_TARGET;
        let (bits, new) = calculate_next_target_required(old, ts(0), ts(timespan));

        match expected {
            ExpectedDirection::Equal => {
                assert_eq!(bits, CompactTarget::new(GENESIS_NBITS), "{name}: bits");
                assert_eq!(new, old, "{name}: target");
            },
            ExpectedDirection::NotGreater => {
                assert!(new <= old, "{name}: target should be <= old");
            },
            ExpectedDirection::NotLess => {
                assert!(new >= old, "{name}: target should be >= old");
            },
        }
    }
}

// Malformed header scenarios mapped to ValidationErrorCode
#[test]
fn malformed_header_pow_and_timestamp_errors() {
    let mut state = test_state();
    let hdr1 = HeaderInput {
        version: 1,
        merkle_root: [0x22; 32],
        timestamp: ts(1),
        nonce: 7,
    };
    let hints = median_hints_for_headers(&state, &[hdr1]);

    // PowInsufficient on first header using a deliberately large hash value (> target).
    {
        let _hash_guard = set_hash_fail();
        state.apply_headers(&[hdr1], &hints).expect_err("expected PoW failure");
    }

    // TimestampTooOld on equality to median (<= median should fail).
    let _hash_guard = set_hash_success();
    let mut s2 = test_state();
    // First, accept a valid header to seed the window deterministically.
    let ok_hdr = HeaderInput {
        version: 2,
        merkle_root: [0x33; 32],
        timestamp: ts(1),
        nonce: 9,
    };
    let median_hints = median_hints_for_headers(&s2, &[ok_hdr]);
    s2.apply_headers(&[ok_hdr], &median_hints).unwrap();
    let median_after_1 = median_time_past(&s2);
    let bad_hdr = HeaderInput {
        version: 42,
        merkle_root: [0x44; 32],
        timestamp: median_after_1,
        nonce: 10,
    };
    let median_hints = median_hints_for_headers(&s2, &[bad_hdr]);
    let failure = s2
        .apply_headers(&[bad_hdr], &median_hints)
        .expect_err("timestamp equal to median must fail");
    assert_eq!(failure.error_code, ValidationErrorCode::TimestampTooOld);

    // Version field is not validated by consensus here; a weird version should still pass if PoW/timestamp do.
    let mut s3 = test_state();
    let median_start = median_time_past(&s3);
    let odd_version_hdr = HeaderInput {
        version: 0xFFFF_FFFF,
        merkle_root: [0x55; 32],
        timestamp: ts(median_start.into_inner().wrapping_add(1)),
        nonce: 1,
    };
    let median_hints = median_hints_for_headers(&s3, &[odd_version_hdr]);
    s3.apply_headers(&[odd_version_hdr], &median_hints)
        .expect("unexpected failure due to version only");
}

// Median time boundary: timestamp == median must fail
#[test]
fn median_time_boundary_equality_fails() {
    let mut state = test_state();
    // Seed a few headers to make median meaningful.
    for t in 1..=5 {
        apply_header(&mut state, t);
    }
    let median = median_time_past(&state);
    let hdr = HeaderInput {
        version: 1,
        merkle_root: [0x66; 32],
        timestamp: median,
        nonce: 1,
    };
    let median_hints = median_hints_for_headers(&state, &[hdr]);
    let failure = state
        .apply_headers(&[hdr], &median_hints)
        .expect_err("timestamp equal to median must fail");
    assert_eq!(failure.error_code, ValidationErrorCode::TimestampTooOld);
}
