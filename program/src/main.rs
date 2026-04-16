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
    add_timestamp_window, bits_to_target, check_median_timestamp, hash_meets_target,
    retarget_target, target_exceeds, target_to_bits, u256_add, work_from_bits, BlockHash, Input,
    InputError, State, ValidationErrorCode, GENESIS_NBITS, STATE_SIZE, WINDOW_SIZE,
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
        let header_index = i as u32;
        let validated_height = state.height + 1;
        let required_nbits = state.next_nbits;

        // Median timestamp check over the timestamps already tracked in state.
        // Run this before hashing so timestamp violations surface directly.
        let timestamp_count = (state.height as usize).min(WINDOW_SIZE);
        if timestamp_count > 0
            && check_median_timestamp(
                &state.timestamps,
                state.sorted_nibbles,
                timestamp_count,
                new_header.timestamp,
            )
        {
            commit_error(&state, ValidationErrorCode::TimestampTooOld, header_index);
        }

        // Construct the full 80-byte header from authenticated state + prover input
        println!("cycle-tracker-start: sha256d");
        let header = new_header.into_header(state.block_hash, required_nbits);
        let block_hash = BlockHash::from_raw(double_sha256_80(&header.to_bytes()));
        println!("cycle-tracker-end: sha256d");

        // PoW check: hash must meet the target required for this next header.
        if !hash_meets_target(block_hash, required_nbits) {
            commit_error(&state, ValidationErrorCode::PowInsufficient, header_index);
        }

        let slot = (state.height as usize) % WINDOW_SIZE;
        state.sorted_nibbles = add_timestamp_window(
            &mut state.timestamps,
            timestamp_count,
            state.sorted_nibbles,
            new_header.timestamp,
            slot,
        );

        // The first block of a new epoch becomes the reference point for the
        // next retarget window.
        if validated_height % 2016 == 0 {
            state.epoch_start_timestamp = new_header.timestamp;
        }

        // Retarget once the just-accepted block completes an epoch so the new
        // difficulty applies to the next header we construct.
        if (validated_height + 1) % 2016 == 0 {
            println!("cycle-tracker-start: retarget");
            let actual_timespan = new_header
                .timestamp
                .wrapping_sub(state.epoch_start_timestamp);
            let expected_timespan: u32 = 2016 * 600;
            let clamped = actual_timespan
                .max(expected_timespan / 4)
                .min(expected_timespan * 4);
            let pow_limit = bits_to_target(GENESIS_NBITS.into());
            let mut new_target = retarget_target(state.next_target(), clamped, expected_timespan);
            if target_exceeds(new_target, pow_limit) {
                new_target = pow_limit;
            }
            state.next_nbits = target_to_bits(new_target);
            println!("cycle-tracker-end: retarget");
        }

        state.chain_work = u256_add(state.chain_work, work_from_bits(required_nbits));
        state.header = header;
        state.block_hash = block_hash;
        state.height = validated_height;
    }

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
