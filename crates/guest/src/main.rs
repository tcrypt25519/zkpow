//! Bitcoin Header Chain Prover — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. encoded_input: Vec<u8>
//!   2. If `state.height > 0`: a recursive proof witness written via `write_proof`
//!
//! Output: serialized State on success, or state + error_code + header_index on error.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkpow_core::{
    BlockHash, Header, Input, InputError, NewHeader, RecursiveProof, State, ValidationErrorCode,
    STATE_SIZE,
};

mod sha256;
use sha256::{sha256_232bytes, sha256d_80bytes};

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: ValidationErrorCode, header_index: u32) -> ! {
    commit_error_output(state, error_code, header_index);
    sp1_zkvm::syscalls::syscall_halt(0)
}

/// Commit a header-payload parse failure using the authenticated input state.
fn commit_header_payload_length_error(input_bytes: &[u8]) -> ! {
    let mut state = parse_state(input_bytes);
    if state.height == 0 && state.genesis_hash == BlockHash::default() {
        initialize_genesis_hash(&mut state);
    }
    commit_error(&state, ValidationErrorCode::HeaderPayloadLengthInvalid, 0)
}

/// Hash a full 80-byte Bitcoin header with SHA256d.
#[sp1_derive::cycle_tracker]
fn hash_header(header: &Header) -> BlockHash {
    BlockHash::from_raw(sha256d_80bytes(&header.to_bytes()))
}

#[sp1_derive::cycle_tracker]
fn commit_state(state_bytes: &[u8; STATE_SIZE]) {
    sp1_zkvm::io::commit_slice(state_bytes);
}

#[sp1_derive::cycle_tracker]
fn commit_error_code(error_code: ValidationErrorCode) {
    sp1_zkvm::io::commit_slice(&[error_code.as_byte()]);
}

#[sp1_derive::cycle_tracker]
fn commit_header_index(header_index: u32) {
    sp1_zkvm::io::commit_slice(&header_index.to_le_bytes());
}

#[sp1_derive::cycle_tracker]
fn serialize_state(state: &State) -> [u8; STATE_SIZE] {
    state.to_bytes()
}

#[sp1_derive::cycle_tracker]
fn parse_state(input_bytes: &[u8]) -> State {
    State::parse(&input_bytes[..STATE_SIZE]).expect("input should contain an initial state")
}

#[sp1_derive::cycle_tracker]
fn initialize_genesis_hash(state: &mut State) {
    let block_hash = hash_header(&state.header);
    state.block_hash = block_hash;
    state.genesis_hash = block_hash;
}

#[sp1_derive::cycle_tracker]
fn parse_input(input_bytes: &[u8]) -> Input {
    match Input::parse(input_bytes, hash_header) {
        Ok(input) => input,
        Err(InputError::HeaderPayloadLengthInvalid { .. }) => {
            commit_header_payload_length_error(input_bytes)
        }
        Err(err) => panic!("input should parse: {err:?}"),
    }
}

#[sp1_derive::cycle_tracker]
fn verify_recursive_proof(state: &State, recursive_proof: &RecursiveProof) {
    sp1_zkvm::lib::verify::verify_sp1_proof(
        recursive_proof.verifier_key.as_raw(),
        recursive_proof.public_values_digest.as_raw(),
    );

    let actual_public_values_digest = sha256_232bytes(&state.to_bytes());
    if actual_public_values_digest != recursive_proof.public_values_digest.into_raw() {
        panic!("recursive proof public values digest mismatch");
    }
}

#[sp1_derive::cycle_tracker]
fn apply_headers_or_commit(state: &State, headers: &[NewHeader]) -> State {
    match state.apply_headers(headers, hash_header) {
        Ok(state) => state,
        Err(failure) => commit_error(
            &failure.last_valid_state,
            failure.error_code,
            failure.header_index,
        ),
    }
}

#[sp1_derive::cycle_tracker]
fn commit_error_output(state: &State, error_code: ValidationErrorCode, header_index: u32) {
    let state_bytes = serialize_state(state);
    commit_state(&state_bytes);
    commit_error_code(error_code);
    commit_header_index(header_index);
}

#[sp1_derive::cycle_tracker]
fn commit_success(final_state: &State) {
    let final_state_bytes = serialize_state(final_state);
    commit_state(&final_state_bytes);
}

// ============================================================================
// Main Program
// ============================================================================

#[sp1_derive::cycle_tracker]
pub fn main() {
    let input_bytes = sp1_zkvm::io::read_vec();
    let input = parse_input(&input_bytes);

    let state = input.state.clone();
    if state.height > 0 {
        verify_recursive_proof(&state, &input.recursive_proof);
    }

    let final_state = apply_headers_or_commit(&state, &input.headers);
    commit_success(&final_state);
    sp1_zkvm::syscalls::syscall_halt(0);
}
