use alloc::{vec, vec::Vec};



use zkvm_test::zkvm_test;

extern crate std;

mod halt;
mod input;
mod public_values;

/// The transition is generic over the network, so its tests drive it with the real ones.
/// `ShortEpoch` differs from `Mainnet` only in `epoch_length`, which is what makes it useful here.
use zkpow_params::{Mainnet as TestConsensus, ShortEpoch};

use super::*;

const GENESIS_NBITS: u32 = *TestConsensus::PARAMS.genesis.compact_target.as_inner();
const GENESIS_TARGET: Target = TestConsensus::PARAMS.genesis_target;

const EXPECTED_EPOCH_TIMESPAN: u32 = TestConsensus::PARAMS.expected_epoch_time();
const MIN_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.min_epoch_timespan();
const MAX_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.max_epoch_timespan();

fn expect_error_code(e: ApplyError) -> ErrorCode {
    //fn expect_error_code(e: Option<ErrorCode>) -> ErrorCode {
    e.error_code().expect("All ApplyErrors return an ErrorCode")
}

#[must_use]
fn set_hash_success() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_success()
}

#[must_use]
fn set_hash_fail() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_fail()
}

fn ts(seconds: u32) -> BlockTimestamp {
    BlockTimestamp::new(seconds)
}

fn zero_hash() -> BlockHash {
    BlockHash::new([0; 32])
}

fn sample_header() -> Header {
    Header {
        version:        1,
        prev_blockhash: BlockHash::new([0x11; 32]),
        merkle_root:    [0x22; 32],
        timestamp:      ts(10),
        compact_target: CompactTarget::new(GENESIS_NBITS),
        nonce:          7,
    }
}

fn hash_sample_header() -> BlockHash {
    hash_header(&sample_header())
}

fn test_state() -> State {
    State {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    }
}

pub(crate) fn median_time_past(state: &State) -> BlockTimestamp {
    let count = timestamp_count(state.height);
    let mut sorted = state.timestamps;
    if count >= WINDOW_SIZE {
        sorted.sort_unstable();
        sorted[WINDOW_SIZE / 2]
    } else {
        sorted[..count].sort_unstable();
        sorted[count / 2]
    }
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
    let warm_start = if state.height == 0 {
        (WINDOW_SIZE - 1).min(headers.len())
    } else {
        0
    };
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

fn failure_public_value_bytes(state: &State, error_code: ErrorCode) -> [u8; PublicValues::SIZE] {
    PublicValues::new(
        *state,
        VerifierKeyDigest::from_raw([0xA5A5_A5A5; 8]),
        ReturnCode::from(error_code),
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

#[zkvm_test]
fn fixed_width_wire_sizes_match_protocol() {
    assert_eq!(HeaderInput::SIZE, 44);
    assert_eq!(State::SIZE, 152);
    assert_eq!(PublicValues::SIZE, 185);
    assert_eq!(align_of::<Header>(), 4);
    assert_eq!(align_of::<HeaderInput>(), 4);
    assert_eq!(align_of::<BlockWork>(), 8);
    assert_eq!(align_of::<ChainWork>(), 8);
    assert_eq!(align_of::<State>(), 8);
}

#[zkvm_test]
fn public_values_round_trips() {
    let state: State = State {
        block_hash: BlockHash::new([0xAB; 32]),
        genesis_hash: BlockHash::new([0xCD; 32]),
        chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
        height: 42,
        ..State::default()
    };
    let verifier_key = VerifierKeyDigest::from_raw([9; 8]);
    let public_values = PublicValues::new(state, verifier_key, ReturnCode::SUCCESS);
    let bytes = public_values.to_bytes();
    assert_eq!(bytes.len(), PublicValues::SIZE);
    assert_eq!(&bytes[0..State::SIZE], bytemuck::bytes_of(&state));
    assert_eq!(&bytes[State::SIZE..State::SIZE + 32], verifier_key.to_bytes());
    assert_eq!(bytes[PublicValues::SIZE - 1], 0);
    let parsed = PublicValues::parse(&bytes).unwrap();
    assert_eq!(parsed.state, state);
    assert_eq!(parsed.verifier_key, verifier_key);
    assert!(parsed.is_success());
}

#[zkvm_test]
fn header_and_new_header_round_trip_exact_wire_bytes() {
    let mut header_bytes = [0u8; Header::SIZE];
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
    assert_eq!(header.as_bytes(), header_bytes.as_slice());

    let mut new_header_bytes = [0u8; HeaderInput::SIZE];
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

#[zkvm_test]
fn check_proof_of_work_accepts_exact_target_boundary() {
    let hash = BlockHash::new(GENESIS_TARGET.to_le_bytes());
    assert!(check_proof_of_work(hash, &GENESIS_TARGET));
}

#[zkvm_test]
fn target_to_bits_round_trips_genesis_target() {
    let round_trip = target_to_bits(GENESIS_TARGET);
    assert_eq!(round_trip, CompactTarget::new(GENESIS_NBITS));
}

#[zkvm_test]
fn target_from_bits_expands_genesis_bits() {
    let expanded = target_from_bits(CompactTarget::new(GENESIS_NBITS));
    assert_eq!(expanded, GENESIS_TARGET);
}

#[zkvm_test]
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

#[cfg(not(target_os = "zkvm"))]
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

#[zkvm_test]
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
fn failure_height_is_absolute_chain_height() {
    let _hash_guard = set_hash_fail();
    // Start at height 5 and fail on the first new header → failure_height = 6.
    let mut state: State = State {
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
    // Use a hash that exceeds the target to trigger PowInsufficient.
    let failure = state
        .apply_headers::<TestConsensus>(&headers, &hints)
        .expect_err("should fail PoW");

    assert_eq!(expect_error_code(failure), ErrorCode::POW_INSUFFICIENT);
    assert_eq!(state.height + 1, 6);
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

#[zkvm_test]
fn apply_headers_matches_sequential_next_across_median_window_wrap() {
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

#[cfg(not(target_os = "zkvm"))]
#[test]
fn hinted_median_validation_rejects_wrong_rank_hint() {
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
fn hinted_median_validation_rejects_wrong_rank_hint_zkvm() {
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
    state.apply_headers::<TestConsensus>(&headers, &[ts(4)]).unwrap();
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

#[zkvm_test]
fn median_time_past_uses_upper_median_across_window_sizes() {
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

#[zkvm_test]
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
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(999),
        nonce:       7,
    };
    let median_hints = median_hints_for_headers(&state, &[header]);
    state.apply_headers::<TestConsensus>(&[header], &median_hints).unwrap();

    let mut expected = original;
    expected[1] = ts(999);
    assert_eq!(state.timestamps, expected);
}

// Difficulty retargeting edge cases using the pure helper. These represent
// the extremes the state machine will clamp to during epoch transitions.
#[zkvm_test]
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
        let (bits, new, _) = calculate_next_target_required::<TestConsensus>(old, ts(0), ts(timespan));

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
#[zkvm_test]
fn malformed_header_pow_and_timestamp_errors() {
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

    // PowInsufficient on first header using a deliberately large hash value (> target).
    {
        let _hash_guard = set_hash_fail();
        let failure = state
            .apply_headers_from_genesis::<TestConsensus>(&headers10, &hints)
            .expect_err("expected PoW failure");
        assert_eq!(expect_error_code(failure), ErrorCode::POW_INSUFFICIENT);
    }

    // TimestampTooOld on equality to median (<= median should fail).
    let _hash_guard = set_hash_success();
    let mut s2 = test_state();
    // First, accept 10 valid headers to seed the window deterministically.
    s2.apply_headers_from_genesis::<TestConsensus>(&headers10, &hints).unwrap();
    let median_after_10 = median_time_past(&s2);
    let bad_hdr = HeaderInput {
        version:     42,
        merkle_root: [0x44; 32],
        timestamp:   median_after_10,
        nonce:       10,
    };
    let median_hints = median_hints_for_headers(&s2, &[bad_hdr]);
    let failure = s2
        .apply_headers::<TestConsensus>(&[bad_hdr], &median_hints)
        .expect_err("timestamp equal to median must fail");
    assert_eq!(expect_error_code(failure), ErrorCode::TIMESTAMP_TOO_OLD);

    // Version field is not validated by consensus here; a weird version should still pass if PoW/timestamp do.
    let mut s3 = test_state();
    let mut odd_version_headers = headers10.clone();
    odd_version_headers[0].version = 0xFFFF_FFFF;
    let hints3 = median_hints_for_headers(&s3, &odd_version_headers);
    s3.apply_headers_from_genesis::<TestConsensus>(&odd_version_headers, &hints3)
        .expect("unexpected failure due to version only");
}

// Median time boundary: timestamp == median must fail
#[zkvm_test]
fn median_time_boundary_equality_fails() {
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

/// A call that starts exactly one block before a retarget boundary must retarget once. The
/// cold-start pass builds an `EpochManager` even when it has no headers to process, and that
/// guard used to commit the new nbits on drop, so the warm pass then retargeted a second time
/// from the already-retargeted value.
#[zkvm_test]
fn boundary_header_retargets_once_per_call() {
    assert_boundary_retargets_once::<TestConsensus>();
    // Same assertion at a different epoch length, so a hardcoded 2016 anywhere in the transition
    // would show up here.
    assert_boundary_retargets_once::<ShortEpoch>();
}

fn assert_boundary_retargets_once<C: Consensus>() {
    let _hash_guard = set_hash_success();
    let epoch_length = C::PARAMS.epoch_length;

    let mut state = test_state();
    state.height = epoch_length - 1;
    state.epoch_timestamp = ts(1_000_000);
    state.timestamps = [ts(1_000_600); WINDOW_SIZE];

    let (expected_nbits, ..) = calculate_next_target_required::<C>(
        target_from_bits(state.current_nbits),
        state.epoch_timestamp,
        state.timestamps[timestamp_slot(state.height)],
    );
    assert_ne!(
        expected_nbits, state.current_nbits,
        "test needs a boundary that actually changes nbits"
    );

    let header = HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1_000_700),
        nonce:       7,
    };
    state.apply_headers::<C>(&[header], &[]).expect("boundary header should apply");

    assert_eq!(state.height, epoch_length);
    assert_eq!(state.current_nbits, expected_nbits);
}

/// The epoch boundary must come from the parameters, not a constant. `ShortEpoch` retargets every
/// 32 blocks, so a batch that spans one boundary under it spans none under mainnet.
#[zkvm_test]
fn epoch_boundary_is_read_from_the_parameters() {
    assert_eq!(TestConsensus::PARAMS.epoch_length, 2016);
    assert_eq!(ShortEpoch::PARAMS.epoch_length, 32);

    let _hash_guard = set_hash_success();
    let mut state = test_state();
    state.epoch_timestamp = ts(1_000_000);
    state.timestamps = [ts(1_000_600); WINDOW_SIZE];
    state.height = ShortEpoch::PARAMS.epoch_length - 1;
    let starting_nbits = state.current_nbits;

    let header = HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1_000_700),
        nonce:       7,
    };

    let mut short = state;
    short.apply_headers::<ShortEpoch>(&[header], &[]).expect("should apply");
    assert_ne!(
        short.current_nbits, starting_nbits,
        "ShortEpoch crossed a boundary and must retarget"
    );

    let mut main = state;
    main.apply_headers::<TestConsensus>(&[header], &[]).expect("should apply");
    assert_eq!(
        main.current_nbits, starting_nbits,
        "mainnet is mid-epoch here and must not retarget"
    );
}

/// A warm chain whose timestamps have been strictly increasing so far, ready for a batch that
/// contains the first out-of-order block.
fn ordered_warm_state(height: u32) -> State {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let timestamps: Vec<u32> = (1..=height).map(|step| 1_000 + step * 600).collect();
    let headers = headers_from_timestamps(&timestamps);
    let hints = median_hints_for_headers(&state, &headers);
    state.apply_headers_from_genesis::<TestConsensus>(&headers, &hints).unwrap();
    state
}

fn headers_from_timestamps(timestamps: &[u32]) -> Vec<HeaderInput> {
    timestamps
        .iter()
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: [0x22; 32],
            timestamp:   ts(*timestamp),
            nonce:       7,
        })
        .collect()
}

fn apply_sequentially(state: &mut State, headers: &[HeaderInput]) -> Result<(), ApplyError> {
    for header in headers {
        let hints = median_hints_for_headers(state, core::slice::from_ref(header));
        state.apply_headers::<TestConsensus>(core::slice::from_ref(header), &hints)?;
    }
    Ok(())
}

/// The ordered-window shortcut reads the median straight out of the ring buffer, so it has to
/// index by the height being sealed rather than the height the batch started at. Reading by the
/// batch's starting height aliases a slot that later headers in the same batch have already
/// overwritten, which silently degrades the median-time-past check into "newer than some recent
/// block". A strictly increasing chain never notices; the first out-of-order block does.
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

/// Pins the boundary itself: deep inside a batch, the accepted timestamp is the one just past the
/// median-time-past of the header's own parent.
#[zkvm_test]
fn ordered_window_median_boundary_holds_deep_in_a_batch() {
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
        (median + 1, Ok(())),
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
    }
}

#[zkvm_test]
fn apply_headers_from_genesis_requires_at_least_ten_headers() {
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

#[zkvm_test]
fn state_and_header_input_layout_and_offsets() {
    assert_eq!(core::mem::size_of::<State>(), 152);
    assert_eq!(core::mem::align_of::<State>(), 8);

    assert_eq!(core::mem::size_of::<HeaderInput>(), 44);
    assert_eq!(core::mem::align_of::<HeaderInput>(), 4);

    let state = State {
        chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
        genesis_hash: BlockHash::new([0x11; 32]),
        block_hash: BlockHash::new([0x22; 32]),
        height: 42,
        current_nbits: CompactTarget::new(0x1d00ffff),
        epoch_timestamp: BlockTimestamp::new(1000),
        timestamps: [BlockTimestamp::new(2000); WINDOW_SIZE],
    };
    let bytes = state.to_bytes();
    assert_eq!(bytes.len(), 152);
    let parsed: &State = bytemuck::from_bytes(&bytes);
    assert_eq!(*parsed, state);
}

#[zkvm_test]
fn retargeting_clamping_upper_and_lower_bounds() {
    let old_target = Target::from_limbs([0, 0, 0, 0x0000_0000_7FFF_0000]);

    // 100 days vs 56 days (clamped upper bound: 4x expected timespan)
    let (_, target_100_days, work_100_days) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(100 * 86400));
    let (_, target_56_days, work_56_days) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(56 * 86400));
    assert_eq!(
        target_100_days, target_56_days,
        "timespan > 4x must clamp to 4x"
    );
    assert_eq!(work_100_days, work_56_days);

    // 1 day vs 3.5 days (clamped lower bound: 0.25x expected timespan)
    let (_, target_1_day, work_1_day) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(86400));
    let (_, target_quarter, work_quarter) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(302400));
    assert_eq!(
        target_1_day, target_quarter,
        "timespan < 0.25x must clamp to 0.25x"
    );
    assert_eq!(work_1_day, work_quarter);

    // Excessively slow retarget clamped to genesis pow_limit
    let slow_target = Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]);
    let (nbits_clamped, target_clamped, _) =
        calculate_next_target_required::<TestConsensus>(slow_target, ts(0), ts(56 * 86400));
    assert_eq!(nbits_clamped, CompactTarget::new(GENESIS_NBITS));
    assert_eq!(target_clamped, GENESIS_TARGET);
}

fn parse_hex_byte(s: &[u8]) -> u8 {
    let hi = match s[0] {
        b'0'..=b'9' => s[0] - b'0',
        b'a'..=b'f' => s[0] - b'a' + 10,
        b'A'..=b'F' => s[0] - b'A' + 10,
        _ => panic!("invalid hex"),
    };
    let lo = match s[1] {
        b'0'..=b'9' => s[1] - b'0',
        b'a'..=b'f' => s[1] - b'a' + 10,
        b'A'..=b'F' => s[1] - b'A' + 10,
        _ => panic!("invalid hex"),
    };
    (hi << 4) | lo
}

fn parse_hex_32_str(s: &str) -> [u8; 32] {
    let bytes = s.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = parse_hex_byte(&bytes[i * 2..i * 2 + 2]);
    }
    out
}

fn parse_u32_str(s: &str) -> u32 {
    let mut val = 0u32;
    for &b in s.as_bytes() {
        val = val * 10 + (b - b'0') as u32;
    }
    val
}

#[allow(dead_code)]
struct ParsedBlock {
    expected_hash: BlockHash,
    version:        u32,
    prev_blockhash: BlockHash,
    merkle_root:    [u8; 32],
    timestamp:      u32,
    compact_target: u32,
    nonce:          u32,
}

fn parse_fixture_blocks(fixture_str: &str) -> Vec<ParsedBlock> {
    let mut blocks = Vec::new();
    for line in fixture_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let expected_hash = BlockHash::new(parse_hex_32_str(parts.next().unwrap()));
        let version = parse_u32_str(parts.next().unwrap());
        let prev_blockhash = BlockHash::new(parse_hex_32_str(parts.next().unwrap()));
        let merkle_root = parse_hex_32_str(parts.next().unwrap());
        let timestamp = parse_u32_str(parts.next().unwrap());
        let compact_target = parse_u32_str(parts.next().unwrap());
        let nonce = parse_u32_str(parts.next().unwrap());
        blocks.push(ParsedBlock {
            expected_hash,
            version,
            prev_blockhash,
            merkle_root,
            timestamp,
            compact_target,
            nonce,
        });
    }
    blocks
}

#[zkvm_test(fixture = "headers_fixture.bin")]
fn test_real_bitcoin_header_chain_unmocked_verification() {
    let fixture_str = zkvm_test::fixture_str();
    let blocks = parse_fixture_blocks(&fixture_str);
    assert_eq!(blocks.len(), 50, "fixture must contain genesis + 49 blocks");

    // Genesis is block 0
    let genesis = &blocks[0];
    assert_eq!(genesis.expected_hash, TestConsensus::GENESIS_HASH);

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

    // Blocks 1..50 (49 headers)
    let header_inputs: Vec<HeaderInput> = blocks[1..50]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();

    let hints = median_hints_for_headers(&state, &header_inputs);

    // NOTICE: NO MOCK! This verifies real SHA-256 and real PoW across all 49 blocks!
    state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .expect("all 49 real Bitcoin headers must validate without mocks");

    assert_eq!(state.height, 49);
    assert_eq!(state.block_hash, blocks[49].expected_hash);

    // 50 blocks total (genesis + 49) at 0x1d00ffff
    let expected_work = ChainWork::from_limbs([0x0000_0001_0001_0001 * 50, 0, 0, 0]);
    assert_eq!(state.chain_work, expected_work);
}


#[zkvm_test(fixture = "headers_fixture.bin")]
fn test_unmocked_pow_insufficient_rejection() {
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

    // Take 10 headers but corrupt the first header's nonce so PoW fails
    let mut header_inputs: Vec<HeaderInput> = blocks[1..=10]
        .iter()
        .map(|b| HeaderInput {
            version:     b.version,
            merkle_root: b.merkle_root,
            timestamp:   BlockTimestamp::new(b.timestamp),
            nonce:       b.nonce,
        })
        .collect();
    header_inputs[0].nonce ^= 0x1234_5678;

    let hints = median_hints_for_headers(&state, &header_inputs);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();
    assert!(matches!(err, ApplyError::PowInsufficient { height: 1, .. }));
}

#[zkvm_test(fixture = "headers_fixture.bin")]
fn test_unmocked_prev_blockhash_mismatch_rejection() {
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

    // Because block_hash != genesis_hash, the computed SHA256 of block 1 will not match block 1's valid hash
    // and PoW will fail for target 0x1d00ffff
    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();
    assert!(matches!(err, ApplyError::PowInsufficient { height: 1, .. }));
}

#[zkvm_test(fixture = "headers_fixture.bin")]
fn test_timestamp_not_greater_than_median_rejection() {
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

    // Set header 1's timestamp to genesis timestamp (1231006505), which is <= median
    header_inputs[0].timestamp = TestConsensus::PARAMS.genesis.timestamp;

    let _mock_guard = set_hash_success();
    let hints = median_hints_for_headers(&state, &header_inputs);

    let err = state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .unwrap_err();
    assert!(matches!(err, ApplyError::TimestampTooOld { height: 1, .. }));
}

