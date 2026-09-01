use alloc::{format, vec};

use zkvm_test::zkvm_test;

use super::*;

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

#[zkvm_test]
fn public_values_size_is_exactly_state_plus_key_plus_return_code() {
    assert_eq!(State::SIZE, 152);
    assert_eq!(core::mem::size_of::<VerifierKeyDigest>(), 32);
    assert_eq!(PublicValues::SIZE, 152 + 32 + 1);
}

#[zkvm_test]
fn public_values_success_round_trips() {
    let state = sample_state();
    let public_values = PublicValues::new(state, sample_verifier_key(), ReturnCode::SUCCESS);
    let bytes = public_values.to_bytes();

    assert_eq!(bytes.len(), PublicValues::SIZE);
    let parsed = PublicValues::parse(&bytes).unwrap();
    assert_eq!(parsed.state, state);
    assert_eq!(parsed.verifier_key, sample_verifier_key());
    assert!(parsed.is_success());
    assert_eq!(parsed.return_code, ReturnCode::SUCCESS);
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
fn public_values_layout_matches_exact_spec() {
    let state = sample_state();
    let key = sample_verifier_key();
    let public_values = PublicValues::new(state, key, ReturnCode::SUCCESS);
    let bytes = public_values.to_bytes();

    assert_eq!(&bytes[0..State::SIZE], bytemuck::bytes_of(&state));
    assert_eq!(&bytes[State::SIZE..State::SIZE + 32], key.to_bytes());
    assert_eq!(bytes[PublicValues::SIZE - 1], 0);

    let failure = PublicValues::new(state, key, ReturnCode::POW_INSUFFICIENT);
    let bytes = failure.to_bytes();
    assert_eq!(bytes[PublicValues::SIZE - 1], ErrorCode::POW_INSUFFICIENT.get());
}

#[zkvm_test]
fn public_values_parse_rejects_wrong_length() {
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

/// The return code is one byte on the wire, so the success case has to ride in the `Option`'s
/// niche rather than a discriminant of its own.
#[zkvm_test]
fn a_return_code_is_one_byte() {
    assert_eq!(core::mem::size_of::<ErrorCode>(), 1);
    assert_eq!(core::mem::size_of::<Option<ErrorCode>>(), 1);
    assert_eq!(core::mem::size_of::<ReturnCode>(), 1);
}

#[zkvm_test]
fn zero_is_success_and_never_an_error_code() {
    assert_eq!(ErrorCode::new(0), None);
    assert!(ReturnCode::new(0).is_success());
    for byte in 1..=u8::MAX {
        assert_eq!(ErrorCode::new(byte).unwrap().get(), byte);
        assert!(ReturnCode::new(byte).is_failure());
    }
}

#[zkvm_test]
fn every_defined_code_has_a_name() {
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

/// Public values can come from a build of the guest that defines codes this one does not, so an
/// unrecognized byte is still a failure and still prints something a reader can act on.
#[zkvm_test]
fn an_undefined_code_is_still_a_failure() {
    let code = ErrorCode::new(200).unwrap();

    assert_eq!(code.name(), None);
    assert_eq!(format!("{code}"), "Unrecognized error code 200");

    let parsed =
        PublicValues::parse(&PublicValues::new(sample_state(), sample_verifier_key(), code.into()).to_bytes()).unwrap();
    assert!(parsed.is_failure());
    assert_eq!(parsed.validation_error_code(), Some(code));
}

#[zkvm_test]
fn success_prints_as_success() {
    assert_eq!(format!("{}", ReturnCode::SUCCESS), "Success");
}
