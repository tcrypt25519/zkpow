//! Bitcoin Header Chain Prover — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. prev_height: u32
//!   2. If prev_height > 0: prev_vk([u32;8]), pv_digest([u8;32]), prev_state_bytes (192 bytes)
//!   3. num_headers: u32
//!   4. headers_bytes: Vec<u8> — num_headers * 44 bytes (NewHeader instances)
//!
//! Output: serialized State (192 bytes) on success, or state + error_code + header_index on error.

#![no_main]
sp1_zkvm::entrypoint!(main);

use bitcoin_header_chain_core::{
    add_timestamp_window, bits_to_target, check_median_timestamp, hash_meets_target,
    retarget_target, target_exceeds, target_to_bits, u256_add, work_from_bits, BlockHash,
    BlockHeader, CompactTarget, NewHeader, State, ValidationErrorCode, GENESIS_NBITS,
    MAINNET_GENESIS_HASH_RAW, NEW_HEADER_SIZE, STATE_SIZE, WINDOW_SIZE,
};

mod sha256;
use sha256::{double_sha256_80, sha256_192};

// ============================================================================
// Error Handling
// ============================================================================

/// Commit the last valid state plus error information, then halt.
fn commit_error(state: &State, error_code: ValidationErrorCode, header_index: u32) -> ! {
    let state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&state_bytes);
    sp1_zkvm::io::commit_slice(&[error_code.as_byte()]);
    sp1_zkvm::io::commit_slice(&header_index.to_le_bytes());
    sp1_zkvm::syscalls::syscall_halt(0);
}

// ============================================================================
// Main Program
// ============================================================================

pub fn main() {
    // --- Read inputs --------------------------------------------------------
    let prev_height = sp1_zkvm::io::read::<u32>();

    // Initialize state: either from previous proof or default (genesis start)
    let mut state = if prev_height > 0 {
        // Read previous proof verification data
        let prev_vk = sp1_zkvm::io::read::<[u32; 8]>();
        let prev_pv_digest = sp1_zkvm::io::read::<[u8; 32]>();
        let prev_state_bytes_vec = sp1_zkvm::io::read_vec();

        if prev_state_bytes_vec.len() != STATE_SIZE {
            panic!(
                "Previous proof public values wrong size: expected {}, got {}",
                STATE_SIZE,
                prev_state_bytes_vec.len()
            );
        }

        // In-circuit verification of the previous proof
        sp1_zkvm::lib::verify::verify_sp1_proof(&prev_vk, &prev_pv_digest);

        let mut prev_state_bytes = [0u8; STATE_SIZE];
        prev_state_bytes.copy_from_slice(&prev_state_bytes_vec);
        let actual_prev_pv_digest = sha256_192(&prev_state_bytes);
        if actual_prev_pv_digest != prev_pv_digest {
            panic!("Previous proof public values digest mismatch");
        }

        let s = State::parse(&prev_state_bytes).expect("previous state bytes should parse");

        if s.height != prev_height {
            panic!(
                "Height mismatch in previous state: expected {}, got {}",
                prev_height, s.height
            );
        }

        s
    } else {
        State::default()
    };

    if state.height == 0 {
        state.nbits = CompactTarget::from_consensus(GENESIS_NBITS);
        state.target = bits_to_target(CompactTarget::from_consensus(GENESIS_NBITS));
    }

    let num_headers = sp1_zkvm::io::read::<u32>();
    let headers_bytes = sp1_zkvm::io::read_vec();

    // Validate header byte count: must be num_headers * 44 (NewHeader size)
    if headers_bytes.len() != (num_headers as usize) * NEW_HEADER_SIZE {
        commit_error(&state, ValidationErrorCode::HeaderCountMismatch, 0);
    }

    // --- Process each header ------------------------------------------------
    for i in 0..num_headers {
        let offset = (i as usize) * NEW_HEADER_SIZE;
        let new_header =
            NewHeader::parse_at(&headers_bytes, offset).expect("input header bytes should parse");
        let validated_count = state.height + 1;

        // Median timestamp check — always runs when height > 0.
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
            commit_error(&state, ValidationErrorCode::TimestampTooOld, i);
        }

        // Construct the full 80-byte header from authenticated state + prover input
        println!("cycle-tracker-start: sha256d");
        let header = BlockHeader::from_state(&state, &new_header);
        let block_hash = BlockHash::from_raw(double_sha256_80(header.as_bytes()));
        println!("cycle-tracker-end: sha256d");

        // PoW check: hash must meet target (uses state.nbits directly — no conversion)
        if !hash_meets_target(block_hash, state.nbits) {
            commit_error(&state, ValidationErrorCode::PowInsufficient, i);
        }

        // Genesis special case: the first constructed header must match Bitcoin mainnet genesis.
        if state.height == 0 {
            if block_hash.as_raw() != &MAINNET_GENESIS_HASH_RAW {
                commit_error(&state, ValidationErrorCode::GenesisHashMismatch, i);
            }
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(state.nbits);
            state.epoch_start_timestamp = new_header.timestamp;
            state.timestamps[0] = new_header.timestamp;
            state.sorted_nibbles = 0;
        } else {
            // Median timestamp count (before adding this one)
            // Add timestamp to circular buffer
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
            if state.height % 2016 == 0 {
                state.epoch_start_timestamp = new_header.timestamp;
            }

            // `state.height` is the next chain height to validate. Once this block
            // is accepted, `validated_count` becomes the number of blocks in the
            // authenticated prefix. Retargeting runs at that point so the new
            // difficulty applies to the next header we construct.
            if validated_count % 2016 == 0 {
                println!("cycle-tracker-start: retarget");
                let actual_timespan = new_header
                    .timestamp
                    .wrapping_sub(state.epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                let pow_limit = bits_to_target(CompactTarget::from_consensus(GENESIS_NBITS));
                let mut new_target = retarget_target(state.target, clamped, expected_timespan);
                if target_exceeds(new_target, pow_limit) {
                    new_target = pow_limit;
                }
                state.nbits = target_to_bits(new_target);
                state.target = new_target;
                println!("cycle-tracker-end: retarget");
            }

            state.chain_work = u256_add(state.chain_work, work_from_bits(state.nbits));
        }

        // Always update prev_blockhash and height
        state.prev_blockhash = block_hash;
        state.height = validated_count;
    }

    // --- Commit success output ----------------------------------------------
    let final_state_bytes = state.to_bytes();
    sp1_zkvm::io::commit_slice(&final_state_bytes);
    sp1_zkvm::syscalls::syscall_halt(0);
}
