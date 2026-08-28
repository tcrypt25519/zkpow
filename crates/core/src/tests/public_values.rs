use alloc::vec;

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

#[test]
fn public_values_size_is_exactly_state_plus_key_plus_return_code() {
    assert_eq!(State::SIZE, 152);
    assert_eq!(core::mem::size_of::<VerifierKeyDigest>(), 32);
    assert_eq!(PublicValues::SIZE, 152 + 32 + 1);
}

#[test]
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

#[test]
fn public_values_failure_round_trips() {
    let codes = [
        ErrorCode::HEADER_PAYLOAD_LENGTH_INVALID,
        ErrorCode::POW_INSUFFICIENT,
        ErrorCode::TIMESTAMP_TOO_OLD,
        ErrorCode::GENESIS_HASH_MISMATCH,
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

#[test]
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

#[test]
fn public_values_parse_rejects_wrong_length() {
    let too_short = vec![0u8; PublicValues::SIZE - 1];
    let too_long = vec![0u8; PublicValues::SIZE + 1];

    assert!(matches!(
        PublicValues::parse(&too_short).unwrap_err(),
        InputParseError::Parse(ParseError::InvalidLength { .. })
    ));
    assert!(matches!(
        PublicValues::parse(&too_long).unwrap_err(),
        InputParseError::Parse(ParseError::InvalidLength { .. })
    ));
}

#[test]
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
