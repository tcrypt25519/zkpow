use alloc::format;
use zkvm_test::test;

use super::*;

fn apply_errors() -> [ApplyError; 3] {
    [
        ApplyError::TimestampTooOld {
            height:    12,
            timestamp: ts(1_700_000_000),
            median:    ts(1_700_000_600),
        },
        ApplyError::PowInsufficient {
            height: 7,
            hash:   BlockHash::new([0xFF; 32]),
            nbits:  CompactTarget::new(0x1d00_ffff),
        },
        ApplyError::InsufficientColdHeaders {
            required: 10,
            actual:   3,
        },
    ]
}

#[test]
fn apply_errors_map_to_their_own_code() {
    let [timestamp_too_old, pow_insufficient, insufficient_cold_headers] = apply_errors();

    assert_eq!(timestamp_too_old.error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
    assert_eq!(pow_insufficient.error_code(), Some(ErrorCode::POW_INSUFFICIENT));
    assert_eq!(
        insufficient_cold_headers.error_code(),
        Some(ErrorCode::INSUFFICIENT_COLD_HEADERS)
    );
}

/// Every apply error is a consensus rejection, so each one has to reach the guest's output as its
/// own non-zero byte rather than collapsing into the catch-all.
#[test]
fn apply_error_return_codes_are_distinct_and_non_zero() {
    let mut seen = Vec::new();
    for error in apply_errors() {
        let return_code = error.return_code();
        assert!(return_code.is_failure(), "{error} committed a success byte");
        assert_ne!(return_code, ReturnCode::UNKNOWN_ERROR, "{error} lost its own code");
        assert_eq!(return_code.error_code(), error.error_code());
        assert!(!seen.contains(&return_code), "{error} shares a byte with another error");
        seen.push(return_code);
    }
}

/// An error with no code of its own still has to exit non-zero; success is the one thing it cannot
/// report.
#[test]
fn an_error_without_a_code_reports_unknown_error() {
    struct Uncoded;

    impl ReportableError for Uncoded {
        fn error_code(&self) -> Option<ErrorCode> {
            None
        }
    }

    assert_eq!(Uncoded.return_code(), ReturnCode::UNKNOWN_ERROR);
    assert!(Uncoded.return_code().is_failure());
}

/// The whole point of the split: the message names the values that were compared, which the
/// committed byte cannot.
#[test]
fn apply_error_messages_carry_the_compared_values() {
    let [timestamp_too_old, pow_insufficient, insufficient_cold_headers] = apply_errors();

    assert_eq!(
        format!("{timestamp_too_old}"),
        "header 12 timestamp 1700000000 is not greater than the median 1700000600"
    );
    assert_eq!(
        format!("{pow_insufficient}"),
        format!(
            "header 7 hash {} does not meet the target for nbits 0x1d00ffff",
            BlockHash::new([0xFF; 32])
        )
    );
    assert_eq!(
        format!("{insufficient_cold_headers}"),
        "a cold start needs at least 10 headers, got 3"
    );
}

#[test]
fn parse_error_messages_carry_the_offending_lengths() {
    assert_eq!(
        format!("{}", ParseError::InvalidLength {
            expected: 185,
            actual:   184,
        }),
        "invalid length: expected 185, got 184"
    );
    assert_eq!(
        format!("{}", ParseError::Misaligned { required: 4 }),
        "misaligned input requires 4-byte alignment"
    );
    assert_eq!(
        format!("{}", ParseError::Truncated {
            offset: 8,
            needed: 32,
            actual: 12,
        }),
        "truncated input at offset 8: need 32 bytes, got 12"
    );
}

/// A rejected header reports the height it would have had, not the height of the tip it was
/// applied to.
#[test]
fn a_rejected_header_reports_its_own_height_and_hash() {
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

#[test]
fn a_cold_start_reports_how_short_the_batch_was() {
    let mut state = test_state();
    let headers = [HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1),
        nonce:       7,
    }];

    let failure = state
        .apply_headers_from_genesis::<TestConsensus>(&headers, &[])
        .expect_err("one header cannot fill the median window");

    assert_eq!(failure, ApplyError::InsufficientColdHeaders {
        required: WINDOW_SIZE - 1,
        actual:   1,
    });
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 8_000_000)]
fn test_unmocked_timestamp_too_old_rejection_at_height_12() {
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

    // Take 13 headers (blocks 1..=13). Mutate the 12th header (index 11, block 12)
    // to have timestamp equal to genesis timestamp (1231006505).
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

    // Header 12's timestamp is now <= median time past of blocks 1..11, triggering TimestampTooOld at height 12
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

    // Verify public values commit format on failure matches expected error code and height
    let pv_bytes = failure_public_value_bytes(&state, ErrorCode::TIMESTAMP_TOO_OLD);
    let parsed_pv = PublicValues::parse(&pv_bytes).expect("parse public values");
    assert!(parsed_pv.is_failure());
    assert_eq!(parsed_pv.validation_error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
    assert_eq!(parsed_pv.state.height + 1, 12);
}

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 7_500_000)]
fn test_unmocked_pow_insufficient_rejection_at_height_1() {
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

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 8_000_000)]
fn test_pv_public_values_format_and_committed_state_match() {
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
    state
        .apply_headers_from_genesis::<TestConsensus>(&header_inputs, &hints)
        .expect("10 valid headers must validate");

    assert_eq!(state.height, 10);
    assert_eq!(state.block_hash, blocks[10].expected_hash);

    // Construct public values as guest does
    let verifier_key = VerifierKeyDigest::default();
    let pv = PublicValues::new(state, verifier_key, ReturnCode::SUCCESS);
    let pv_bytes = pv.to_bytes();

    assert_eq!(pv_bytes.len(), PublicValues::SIZE);
    let parsed = PublicValues::parse(&pv_bytes).expect("PublicValues::parse");
    assert!(parsed.is_success());
    assert_eq!(parsed.state.height, 10);
    assert_eq!(parsed.state, state);
    assert_eq!(parsed.verifier_key, verifier_key);
}

