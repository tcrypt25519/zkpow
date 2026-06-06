//! zkpow — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. encoded_input: Vec<u8>  (PublicChainClaim || RecursiveProof)
//!   2. state_witness: Vec<u8>  (State bytes)
//!   3. header_hints: Vec<u8>
//!   4. median_time_past_hints: Vec<u8>
//!   5. If `claim.height > 0`, recursive proof witness (via write_proof)
//!
//! Output: MinimalPublicValues (169 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkpow_core::{
    cycle_track, parse_median_hints_ref, parse_new_headers_ref, BlockHash, BlockTimestamp, Header,
    Input, MinimalPublicValues, NewHeader, PublicChainClaim, RecursiveProof, State,
    MINIMAL_PV_SIZE, PRIVATE_CONTINUATION_STATE_SIZE,
};

mod sha256;

use sha256::{sha256_116bytes, sha256_169bytes, sha256d_80bytes};

// ============================================================================
// Helpers
// ============================================================================

/// Hash a full 80-byte Bitcoin header with SHA256d.
fn hash_header(header: &Header) -> BlockHash {
    cycle_track("crypto/hash_header", || {
        let header_bytes = header.to_bytes();
        BlockHash::new(sha256d_80bytes(&header_bytes))
    })
}

/// Compute the continuation digest: SHA-256 of the serialized private continuation fields.
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    cycle_track("crypto/continuation_digest", || {
        let pcs_bytes: [u8; PRIVATE_CONTINUATION_STATE_SIZE] = state.continuation_bytes();
        sha256_116bytes(&pcs_bytes)
    })
}

/// Commit the minimal public values and halt.
fn commit_minimal_pv(pv: &MinimalPublicValues) -> ! {
    let bytes: [u8; MINIMAL_PV_SIZE] = pv.to_bytes();
    sp1_zkvm::io::commit_slice(&bytes);
    sp1_zkvm::syscalls::syscall_halt(0)
}

fn parse_input(input_bytes: &[u8]) -> Input {
    Input::parse(input_bytes).expect("input should parse")
}

fn parse_state_witness(state_bytes: &[u8]) -> State {
    cycle_track("input/parse_state_witness", || {
        State::parse(state_bytes).expect("state witness should parse")
    })
}

fn verify_state_claim(state: &State, claim: &PublicChainClaim) {
    cycle_track("input/verify_state_claim", || {
        assert!(
            state.genesis_hash == claim.genesis_hash
                && state.block_hash == claim.tip_hash
                && state.chain_work == claim.chain_work
                && state.height == claim.height,
            "state witness public claim mismatch"
        );
    })
}

fn parse_header_hints(hint_bytes: &[u8]) -> &[NewHeader] {
    cycle_track("input/parse_header_hints", || {
        parse_new_headers_ref(hint_bytes).expect("new header hints should parse")
    })
}

fn parse_median_hints(hint_bytes: &[u8], header_count: usize) -> &[BlockTimestamp] {
    cycle_track("input/parse_median_hints", || {
        parse_median_hints_ref(hint_bytes, header_count)
            .expect("median time past hints should parse")
    })
}

// ============================================================================
// Recursive proof verification (Step 5 soundness model)
// ============================================================================

fn verify_recursive_proof(
    recursive_proof: &RecursiveProof,
    prior_claim: &PublicChainClaim,
    prior_continuation_bytes: &[u8; PRIVATE_CONTINUATION_STATE_SIZE],
) {
    cycle_track("recursive/verify_proof", || {
        // Reject continuation from a failed prior proof.
        if recursive_proof.previous_return_code != 0 {
            panic!(
                "recursive continuation rejected: prior proof has return code {}",
                recursive_proof.previous_return_code
            );
        }

        // 1. Hash the supplied continuation state.
        let continuation_digest = cycle_track("recursive/continuation_digest", || {
            sha256_116bytes(prior_continuation_bytes)
        });

        // 2. Reconstruct the prior proof's minimal public values and hash them.
        let prior_pv = MinimalPublicValues::success(
            prior_claim,
            continuation_digest,
            recursive_proof.verifier_key,
        );
        let prior_pv_bytes: [u8; MINIMAL_PV_SIZE] = prior_pv.to_bytes();
        let actual_pv_hash = cycle_track("recursive/public_values_digest", || {
            sha256_169bytes(&prior_pv_bytes)
        });

        if actual_pv_hash != recursive_proof.public_values_digest.into_raw() {
            panic!("recursive proof public values digest mismatch");
        }

        // 3. Verify the SP1 proof.
        cycle_track("recursive/sp1_verify", || {
            sp1_zkvm::lib::verify::verify_sp1_proof(
                recursive_proof.verifier_key.as_raw(),
                recursive_proof.public_values_digest.as_raw(),
            );
        });
    });
}

// ============================================================================
// Main Program
// ============================================================================

pub fn main() {
    cycle_track("main", || {
        let input_bytes = sp1_zkvm::io::read_vec();
        let state_bytes = sp1_zkvm::io::read_vec();
        let header_hint_bytes = sp1_zkvm::io::read_vec();
        let median_hint_bytes = sp1_zkvm::io::read_vec();

        let input = parse_input(&input_bytes);
        let mut state = parse_state_witness(&state_bytes);
        verify_state_claim(&state, &input.claim);
        let header_hints = parse_header_hints(&header_hint_bytes);
        let median_hints = parse_median_hints(&median_hint_bytes, header_hints.len());

        if input.claim.height > 0 {
            let prior_continuation_bytes: [u8; PRIVATE_CONTINUATION_STATE_SIZE] =
                state.continuation_bytes();
            verify_recursive_proof(
                &input.recursive_proof,
                &input.claim,
                &prior_continuation_bytes,
            );
        }

        // Apply headers; on failure commit minimal PV and halt.
        if let Err(failure) = state.apply_headers(header_hints, median_hints, hash_header) {
            let digest = compute_continuation_digest(&failure.last_valid_state);
            let pv = MinimalPublicValues::failure(
                &failure.last_valid_state.public_claim(),
                failure.error_code,
                failure.failure_height,
                digest,
                input.recursive_proof.verifier_key,
            );
            commit_minimal_pv(&pv);
        }

        let digest = compute_continuation_digest(&state);
        let pv = MinimalPublicValues::success(
            &state.public_claim(),
            digest,
            input.recursive_proof.verifier_key,
        );
        commit_minimal_pv(&pv);
    });
}
