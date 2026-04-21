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

use zkpow_core::cycle_track;
use zkpow_core::{BlockHash, Header, Input, InputError, State, ValidationErrorCode, STATE_SIZE};

mod sha256;
use sha256::{double_sha256_80, sha256_232};

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: ValidationErrorCode, header_index: u32) -> ! {
    let state_bytes = cycle_track("program/commit_error/serialize_state", || state.to_bytes());
    cycle_track("program/commit_error", || {
        cycle_track("program/commit_error/commit_state", || {
            sp1_zkvm::io::commit_slice(&state_bytes);
        });
        cycle_track("program/commit_error/commit_error_code", || {
            sp1_zkvm::io::commit_slice(&[error_code.as_byte()]);
        });
        cycle_track("program/commit_error/commit_header_index", || {
            sp1_zkvm::io::commit_slice(&header_index.to_le_bytes());
        });
    });
    cycle_track("program/commit_error/halt", || {
        sp1_zkvm::syscalls::syscall_halt(0);
    })
}

/// Commit a header-payload parse failure using the authenticated input state.
fn commit_header_payload_length_error(input_bytes: &[u8]) -> ! {
    let mut state = cycle_track("program/commit_parse_error/parse_state", || {
        State::parse(&input_bytes[..STATE_SIZE]).expect("input should contain an initial state")
    });
    if state.height == 0 && state.genesis_hash == BlockHash::default() {
        cycle_track("program/commit_parse_error/genesis_hash", || {
            let block_hash = hash_header(&state.header);
            state.block_hash = block_hash;
            state.genesis_hash = block_hash;
        });
    }
    commit_error(&state, ValidationErrorCode::HeaderPayloadLengthInvalid, 0)
}

/// Hash a full Bitcoin header with SHA256d.
fn hash_header(header: &Header) -> BlockHash {
    cycle_track("hash/sha256d", || {
        BlockHash::from_raw(double_sha256_80(&header.to_bytes()))
    })
}

// ============================================================================
// Main Program
// ============================================================================

pub fn main() {
    let input_bytes = sp1_zkvm::io::read_vec();
    let input = cycle_track("program/parse_input", || {
        match Input::parse(&input_bytes, hash_header) {
            Ok(input) => input,
            Err(InputError::HeaderPayloadLengthInvalid { .. }) => {
                commit_header_payload_length_error(&input_bytes)
            }
            Err(err) => panic!("input should parse: {err:?}"),
        }
    });

    let state = input.state.clone();
    if state.height > 0 {
        cycle_track("program/verify_recursive_proof", || {
            let recursive_proof = input.recursive_proof;
            sp1_zkvm::lib::verify::verify_sp1_proof(
                recursive_proof.verifier_key.as_raw(),
                recursive_proof.public_values_digest.as_raw(),
            );

            let actual_public_values_digest = sha256_232(&state.to_bytes());
            if actual_public_values_digest != recursive_proof.public_values_digest.into_raw() {
                panic!("recursive proof public values digest mismatch");
            }
        });
    }

    let final_state = cycle_track("program/apply_headers", || {
        match state.apply_headers(&input.headers, hash_header) {
            Ok(state) => state,
            Err(failure) => commit_error(
                &failure.last_valid_state,
                failure.error_code,
                failure.header_index,
            ),
        }
    });

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = cycle_track("program/commit_success/serialize_state", || {
        final_state.to_bytes()
    });
    cycle_track("program/commit_success", || {
        cycle_track("program/commit_success/commit_slice", || {
            sp1_zkvm::io::commit_slice(&final_state_bytes);
        });
        cycle_track("program/commit_success/halt", || {
            sp1_zkvm::syscalls::syscall_halt(0);
        });
    });
}
