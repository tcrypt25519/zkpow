use alloc::format;
use zkvm_test::zkvm_test;

use crate::tests::*;
use crate::{
    BlockHash, ChainWork, HeaderInput, PublicValues, ReturnCode, State, VerifierKeyDigest,
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

    assert_eq!(bytes.len(), PublicValues::SIZE);
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
    let public_values = PublicValues::new(state, key, ReturnCode::SUCCESS);
    let bytes = public_values.to_bytes();

    assert_eq!(&bytes[0..State::SIZE], bytemuck::bytes_of(&state));
    assert_eq!(&bytes[State::SIZE..State::SIZE + 32], key.to_bytes());
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

#[zkvm_test(fixture = "headers_fixture.bin", max_cycles = 8_000_000)]
fn public_values_matches_committed_in_vm_state() {
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
