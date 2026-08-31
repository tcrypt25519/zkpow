use alloc::format;

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
        required: TestConsensus::PARAMS.window_size - 1,
        actual:   1,
    });
}
