use alloc::format;

use zkpow_types::{BlockHash, ChainWork};
use zkvm_test::zkvm_test;

use crate::{tests::*, PublicValues, ReturnCode, State, VerifierKeyDigest};

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
fn public_values_size_is_state_plus_key_plus_return_code() {
    assert_eq!(State::SIZE, 152);
    assert_eq!(core::mem::size_of::<VerifierKeyDigest>(), 32);
    assert_eq!(PublicValues::SIZE, 152 + 32 + 1);
}

#[zkvm_test(max_cycles = 150_000)]
fn public_values_success_round_trips() {
    let state = sample_state();
    let public_values = PublicValues::new(state, sample_verifier_key(), ReturnCode::SUCCESS);
    let bytes = public_values.to_bytes();

    let parsed = PublicValues::parse(&bytes).unwrap();
    assert_eq!(parsed.state, state);
    assert_eq!(parsed.verifier_key, sample_verifier_key());
    assert!(parsed.is_success());
    assert_eq!(parsed.return_code, ReturnCode::SUCCESS);
}

#[zkvm_test]
fn public_values_layout_matches_exact_protocol_specification() {
    let state = sample_state();
    let key = sample_verifier_key();
    let bytes = PublicValues::serialize(&state, &key, ReturnCode::SUCCESS);

    assert_eq!(&bytes[..State::SIZE], state.as_bytes());
    assert_eq!(
        &bytes[State::SIZE..State::SIZE + VerifierKeyDigest::SIZE],
        &[0xA5; VerifierKeyDigest::SIZE]
    );
    assert_eq!(bytes[PublicValues::SIZE - 1], 0);
}

#[zkvm_test]
fn return_code_zero_is_success_and_never_an_error_code() {
    assert_eq!(ErrorCode::new(0), None);
    assert!(ReturnCode::new(0).is_success());
    for byte in 1..=u8::MAX {
        assert_eq!(ErrorCode::new(byte).unwrap().get(), byte);
        assert!(ReturnCode::new(byte).is_failure());
    }
}

#[zkvm_test]
fn success_prints_as_success_string() {
    assert_eq!(format!("{}", ReturnCode::SUCCESS), "Success");
}
