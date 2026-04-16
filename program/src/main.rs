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
    check_median_timestamp, BlockHash, Input, InputError, State, ValidationErrorCode, STATE_SIZE,
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

    let mut state = input.state.clone();
    let encoded_state: [u8; STATE_SIZE] = state
        .to_bytes()
        .try_into()
        .expect("state serialization should match STATE_SIZE");
    if state.height == 0 {
        let genesis_block_hash = BlockHash::from_raw(double_sha256_80(&state.header.to_bytes()));
        if genesis_block_hash != state.genesis_hash {
            panic!("genesis state header must hash to its configured genesis hash");
        }
        state.block_hash = genesis_block_hash;
    }

    if let Some(recursive_proof) = input.recursive_proof {
        sp1_zkvm::lib::verify::verify_sp1_proof(
            recursive_proof.verifier_key.as_raw(),
            recursive_proof.public_values_digest.as_raw(),
        );

        let actual_public_values_digest = sha256_240(&encoded_state);
        if actual_public_values_digest != recursive_proof.public_values_digest.into_raw() {
            panic!("recursive proof public values digest mismatch");
        }
    }

    // --- Process each header ------------------------------------------------
    for (i, new_header) in input.headers.iter().copied().enumerate() {
        let new_header_index = i as u32;

        // Median timestamp check over the timestamps already tracked in state.
        // Run this before hashing so timestamp violations surface directly.
        let timestamp_count = state.timestamp_count();
        if timestamp_count > 0
            && check_median_timestamp(
                &state.timestamps,
                state.sorted_nibbles,
                timestamp_count,
                new_header.timestamp,
            )
        {
            commit_error(
                &state,
                ValidationErrorCode::TimestampTooOld,
                new_header_index,
            );
        }

        println!("cycle-tracker-start: sha256d");
        let next_state = state.next(new_header, |header| {
            BlockHash::from_raw(double_sha256_80(&header.to_bytes()))
        });
        println!("cycle-tracker-end: sha256d");

        if let Err(error_code) = next_state.validate() {
            commit_error(&state, error_code, new_header_index);
        }

        state = next_state;
    }

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
