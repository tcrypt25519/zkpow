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

use bitcoin_header_chain_core::{
    BlockHash, Header, Input, InputError, State, ValidationErrorCode, STATE_SIZE,
};

mod sha256;
use sha256::{double_sha256_80, sha256_240};

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: ValidationErrorCode, header_index: u32) -> ! {
    sp1_zkvm::io::commit_slice(&state.to_bytes());
    sp1_zkvm::io::commit_slice(&[error_code.as_byte()]);
    sp1_zkvm::io::commit_slice(&header_index.to_le_bytes());
    sp1_zkvm::syscalls::syscall_halt(0);
}

/// Hash a full Bitcoin header with double SHA-256.
fn hash_header(header: &Header) -> BlockHash {
    println!("cycle-tracker-start: sha256d");
    let block_hash = BlockHash::from_raw(double_sha256_80(&header.to_bytes()));
    println!("cycle-tracker-end: sha256d");
    block_hash
}

// ============================================================================
// Main Program
// ============================================================================

pub fn main() {
    let input_bytes = sp1_zkvm::io::read_vec();
    let input = match Input::parse(&input_bytes) {
        Ok(input) => input,
        Err(InputError::HeaderCountMismatch { .. }) if input_bytes.len() >= STATE_SIZE => {
            let state = State::parse(&input_bytes[..STATE_SIZE])
                .expect("state prefix should parse when header count mismatches");
            commit_error(&state, ValidationErrorCode::HeaderCountMismatch, 0);
        }
        Err(err) => panic!("input should parse: {}", err),
    };

    let state = input.state.clone();
    if let Err(error_code) = state.validate_initial(hash_header) {
        commit_error(&state, error_code, 0);
    }

    if let Some(recursive_proof) = input.recursive_proof {
        sp1_zkvm::lib::verify::verify_sp1_proof(
            recursive_proof.verifier_key.as_raw(),
            recursive_proof.public_values_digest.as_raw(),
        );

        let actual_public_values_digest = sha256_240(&state.to_bytes());
        if actual_public_values_digest != recursive_proof.public_values_digest.into_raw() {
            panic!("recursive proof public values digest mismatch");
        }
    }

    let final_state = match state.apply_headers(&input.headers, hash_header) {
        Ok(state) => state,
        Err(failure) => commit_error(
            &failure.last_valid_state,
            failure.error_code,
            failure.header_index,
        ),
    };

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = final_state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
