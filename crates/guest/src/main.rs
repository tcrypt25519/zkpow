//! zkpow — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. encoded_input: Vec<u8>
//!   2. header_hints: Vec<u8>
//!   3. median_time_past_hints: Vec<u8>
//!   4. If `state.height > 0`: a recursive proof witness written via `write_proof`
//!
//! Output: serialized State on success, or state + error_code + failure_height on error.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkpow_core::{
    encode_failure_metadata, BlockHash, Header, InputMut, MedianTimePastHintsRef, NewHeader,
    NewHeaderHintsRef, RecursiveProof, State, ValidationErrorCode,
    ValidationState, PRIVATE_CONTINUATION_STATE_SIZE, STATE_SIZE,
};

mod sha256;
use sha256::{sha256_296bytes, sha256_88bytes, sha256d_80bytes};

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: ValidationErrorCode, failure_height: u32) -> ! {
    commit_error_output(state, error_code, failure_height);
    sp1_zkvm::syscalls::syscall_halt(0)
}

/// Hash a full 80-byte Bitcoin header with SHA256d.
#[sp1_derive::cycle_tracker]
fn hash_header(header: &Header) -> BlockHash {
    let header_bytes: &[u8; 80] = unsafe { &*(header as *const Header as *const [u8; 80]) };
    BlockHash::from_raw(sha256d_80bytes(header_bytes))
}

#[sp1_derive::cycle_tracker]
fn commit_state(state_bytes: &[u8; STATE_SIZE]) {
    sp1_zkvm::io::commit_slice(state_bytes);
}

#[sp1_derive::cycle_tracker]
fn commit_failure_metadata(error_code: ValidationErrorCode, failure_height: u32) {
    let metadata = encode_failure_metadata(error_code, failure_height);
    sp1_zkvm::io::commit_slice(&metadata);
}

#[sp1_derive::cycle_tracker]
fn serialize_state(state: &State) -> [u8; STATE_SIZE] {
    state.to_bytes()
}

fn state_bytes(state: &State) -> &[u8; STATE_SIZE] {
    // State is the fixed-width repr(C) wire type committed as public values.
    unsafe { &*(state as *const State as *const [u8; STATE_SIZE]) }
}

/// Compute the continuation digest for a state.
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    let vs = ValidationState::from_state(state);
    let pcs_bytes: [u8; PRIVATE_CONTINUATION_STATE_SIZE] = vs.private.to_bytes();
    sha256_88bytes(&pcs_bytes)
}

/// Build the 296-byte success public values: State || continuation_digest.
fn build_success_pv(state_bytes: &[u8; STATE_SIZE], digest: &[u8; 32]) -> [u8; 296] {
    let mut pv = [0u8; 296];
    pv[..STATE_SIZE].copy_from_slice(state_bytes);
    pv[STATE_SIZE..].copy_from_slice(digest);
    pv
}

fn parse_input<'a>(input_bytes: &'a mut [u8]) -> InputMut<'a> {
    InputMut::parse(input_bytes).expect("input should parse")
}

#[sp1_derive::cycle_tracker]
fn parse_header_hints<'a>(hint_bytes: &'a [u8]) -> NewHeaderHintsRef<'a> {
    NewHeaderHintsRef::parse(hint_bytes).expect("new header hints should parse")
}

#[sp1_derive::cycle_tracker]
fn parse_median_hints<'a>(hint_bytes: &'a [u8], header_count: usize) -> MedianTimePastHintsRef<'a> {
    MedianTimePastHintsRef::parse(hint_bytes, header_count)
        .expect("median time past hints should parse")
}

#[sp1_derive::cycle_tracker]
fn verify_recursive_proof(state: &State, recursive_proof: &RecursiveProof) {
    // Reject continuation from a failed prior proof.
    if recursive_proof.previous_return_code != 0 {
        panic!(
            "recursive continuation rejected: prior proof has return code {}",
            recursive_proof.previous_return_code
        );
    }

    sp1_zkvm::lib::verify::verify_sp1_proof(
        recursive_proof.verifier_key.as_raw(),
        recursive_proof.public_values_digest.as_raw(),
    );

    // Reconstruct the prior proof's public values (State || continuation_digest)
    // and verify the digest matches.
    let prior_digest = compute_continuation_digest(state);
    let prior_pv = build_success_pv(state_bytes(state), &prior_digest);
    let actual_public_values_digest = sha256_296bytes(&prior_pv);
    if actual_public_values_digest != recursive_proof.public_values_digest.into_raw() {
        panic!("recursive proof public values digest mismatch");
    }
}

#[sp1_derive::cycle_tracker]
fn apply_headers_or_commit(
    state: &mut State,
    headers: &[NewHeader],
    median_hints: &MedianTimePastHintsRef<'_>,
) {
    if let Err(failure) = state.apply_headers_in_place(headers, median_hints.medians, hash_header) {
        commit_error(
            &failure.last_valid_state,
            failure.error_code,
            failure.failure_height,
        );
    }
}

#[sp1_derive::cycle_tracker]
fn commit_error_output(state: &State, error_code: ValidationErrorCode, failure_height: u32) {
    let state_bytes = serialize_state(state);
    commit_state(&state_bytes);
    commit_failure_metadata(error_code, failure_height);
    // Append continuation digest so verifiers can validate the private state.
    let digest = compute_continuation_digest(state);
    sp1_zkvm::io::commit_slice(&digest);
}

#[sp1_derive::cycle_tracker]
fn commit_success(state_bytes: &[u8; STATE_SIZE], digest: &[u8; 32]) {
    sp1_zkvm::io::commit_slice(state_bytes);
    sp1_zkvm::io::commit_slice(digest);
}

// ============================================================================
// Main Program
// ============================================================================

#[sp1_derive::cycle_tracker]
pub fn main() {
    let mut input_bytes = sp1_zkvm::io::read_vec();
    {
        let input = parse_input(&mut input_bytes);
        let header_hint_bytes = sp1_zkvm::io::read_vec();
        let header_hints = parse_header_hints(&header_hint_bytes);
        let median_hint_bytes = sp1_zkvm::io::read_vec();
        let median_hints = parse_median_hints(&median_hint_bytes, header_hints.headers.len());

        input.state.update_genesis_hash(hash_header);

        if input.state.height > 0 {
            verify_recursive_proof(input.state, input.recursive_proof);
        }

        apply_headers_or_commit(input.state, header_hints.headers, &median_hints);
    }

    // After apply_headers_or_commit, input_bytes[..STATE_SIZE] holds the final state.
    let final_state_bytes: &[u8; STATE_SIZE] =
        unsafe { &*(input_bytes.as_ptr() as *const [u8; STATE_SIZE]) };
    let final_state = State::parse(final_state_bytes).expect("state should parse");
    let digest = compute_continuation_digest(&final_state);
    commit_success(final_state_bytes, &digest);
    sp1_zkvm::syscalls::syscall_halt(0);
}
