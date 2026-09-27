use alloc::{format, vec};

use zkvm_test::zkvm_test;

use crate::{
    tests::*, ApplyError, BlockHash, ChainWork, ErrorCode, ParseError, PublicValues, ReportableError, ReturnCode,
    State, VerifierKeyDigest,
};

fn sample_state() -> State {
    State {
        block_hash: BlockHash::new([0xAB; 32]),
        genesis_hash: BlockHash::new([0xCD; 32]),
        chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
        height: 42,
        ..State::default()
    }
}

fn sample_verifier_key() -> VerifierKeyDigest {
    VerifierKeyDigest::from_raw([0xA5A5_A5A5; 8])
}

fn sample_apply_errors() -> [ApplyError; 3] {
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

#[zkvm_test]
fn public_values_failure_round_trips() {
    let codes = [
        ErrorCode::HEADER_PAYLOAD_LENGTH_INVALID,
        ErrorCode::POW_INSUFFICIENT,
        ErrorCode::TIMESTAMP_TOO_OLD,
        ErrorCode::GENESIS_HASH_MISMATCH,
        ErrorCode::INSUFFICIENT_COLD_HEADERS,
        ErrorCode::UNKNOWN_ERROR,
    ];

    for code in codes {
        let public_values = PublicValues::new(sample_state(), sample_verifier_key(), ReturnCode::from(code));
        let bytes = public_values.to_bytes();
        let parsed = PublicValues::parse(&bytes).unwrap();
        assert!(parsed.is_failure());
        assert_eq!(parsed.validation_error_code(), Some(code));
        assert_eq!(parsed.return_code.into_u8(), code.get());
    }
}

#[zkvm_test]
fn public_values_layout_encodes_failure_return_code() {
    let failure = PublicValues::new(sample_state(), sample_verifier_key(), ReturnCode::POW_INSUFFICIENT);
    let bytes = failure.to_bytes();
    assert_eq!(bytes[PublicValues::SIZE - 1], ErrorCode::POW_INSUFFICIENT.get());
}

#[zkvm_test]
fn public_values_parse_rejects_truncated_and_excess_lengths() {
    let too_short = vec![0u8; PublicValues::SIZE - 1];
    let too_long = vec![0u8; PublicValues::SIZE + 1];

    assert!(matches!(
        PublicValues::parse(&too_short).unwrap_err(),
        ParseError::InvalidLength { .. }
    ));
    assert!(matches!(
        PublicValues::parse(&too_long).unwrap_err(),
        ParseError::InvalidLength { .. }
    ));
}

#[zkvm_test]
fn public_values_success_and_failure_helpers_are_mutually_exclusive() {
    let success = PublicValues::new(sample_state(), sample_verifier_key(), ReturnCode::SUCCESS);
    assert!(success.is_success());
    assert!(!success.is_failure());
    assert_eq!(success.validation_error_code(), None);

    let failure = PublicValues::new(sample_state(), sample_verifier_key(), ReturnCode::TIMESTAMP_TOO_OLD);
    assert!(!failure.is_success());
    assert!(failure.is_failure());
    assert_eq!(failure.validation_error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
}

#[zkvm_test]
fn return_code_is_represented_as_single_byte() {
    assert_eq!(core::mem::size_of::<ErrorCode>(), 1);
    assert_eq!(core::mem::size_of::<Option<ErrorCode>>(), 1);
    assert_eq!(core::mem::size_of::<ReturnCode>(), 1);
}

#[zkvm_test]
fn every_defined_error_code_has_a_descriptive_name() {
    let named = [
        (ErrorCode::HEADER_PAYLOAD_LENGTH_INVALID, "Header payload length invalid"),
        (ErrorCode::POW_INSUFFICIENT, "PoW insufficient"),
        (ErrorCode::TIMESTAMP_TOO_OLD, "Timestamp too old"),
        (ErrorCode::GENESIS_HASH_MISMATCH, "Genesis hash mismatch"),
        (ErrorCode::INSUFFICIENT_COLD_HEADERS, "Insufficient cold headers"),
        (ErrorCode::UNKNOWN_ERROR, "Unknown error"),
    ];

    for (code, name) in named {
        assert_eq!(code.name(), Some(name));
        assert_eq!(format!("{code}"), name);
        assert_eq!(format!("{}", ReturnCode::from(code)), name);
    }
}

#[zkvm_test]
fn undefined_error_code_remains_a_failure() {
    let code = ErrorCode::new(200).unwrap();

    assert_eq!(code.name(), None);
    assert_eq!(format!("{code}"), "Unrecognized error code 200");

    let parsed =
        PublicValues::parse(&PublicValues::new(sample_state(), sample_verifier_key(), code.into()).to_bytes()).unwrap();
    assert!(parsed.is_failure());
    assert_eq!(parsed.validation_error_code(), Some(code));
}

#[zkvm_test]
fn apply_errors_map_to_their_own_codes() {
    let [timestamp_too_old, pow_insufficient, insufficient_cold_headers] = sample_apply_errors();

    assert_eq!(timestamp_too_old.error_code(), Some(ErrorCode::TIMESTAMP_TOO_OLD));
    assert_eq!(pow_insufficient.error_code(), Some(ErrorCode::POW_INSUFFICIENT));
    assert_eq!(
        insufficient_cold_headers.error_code(),
        Some(ErrorCode::INSUFFICIENT_COLD_HEADERS)
    );
}

#[zkvm_test]
fn apply_error_return_codes_are_distinct_and_non_zero() {
    let mut seen = Vec::new();
    for error in sample_apply_errors() {
        let return_code = error.return_code();
        assert!(return_code.is_failure(), "{error} committed a success byte");
        assert_ne!(return_code, ReturnCode::UNKNOWN_ERROR, "{error} lost its own code");
        assert_eq!(return_code.error_code(), error.error_code());
        assert!(!seen.contains(&return_code), "{error} shares a byte with another error");
        seen.push(return_code);
    }
}

#[zkvm_test]
fn error_without_a_code_reports_unknown_error() {
    struct Uncoded;

    impl ReportableError for Uncoded {
        fn error_code(&self) -> Option<ErrorCode> {
            None
        }
    }

    assert_eq!(Uncoded.return_code(), ReturnCode::UNKNOWN_ERROR);
    assert!(Uncoded.return_code().is_failure());
}

#[zkvm_test]
fn apply_error_messages_carry_the_compared_values() {
    let [timestamp_too_old, pow_insufficient, insufficient_cold_headers] = sample_apply_errors();

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

#[zkvm_test]
fn parse_error_messages_carry_offending_lengths() {
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
