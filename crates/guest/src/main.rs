//! zkpow — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit constructs the full 80-byte header from
//! authenticated state, then hashes and validates.
//!
//! Input protocol:
//!   1. encoded_input: Vec<u8>  (ProofCarryingState bytes)
//!   2. state_witness: Vec<u8>  (State bytes)
//!   3. header_hints: Vec<u8>
//!   4. median_time_past_hints: Vec<u8>
//!   5. If `claim.height > 0`, recursive proof witness (via write_proof)
//!
//! Output: MinimalPublicValues (169 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

use zkpow_core::{
    cycle_track, parse_median_hints, parse_new_headers, Claim, Header, MinimalPublicValues,
    Proof, ProofCarryingState, State, VerifierKeyDigest, MINIMAL_PV_SIZE,
    CONTINUATION_DATA_SIZE,
};

mod sha256;

use sha256::{sha256_116bytes, sha256_169bytes, sha256d_80bytes};

/// Hash a full 80-byte Bitcoin header with SHA256d.
fn hash_header(header: &Header) -> zkpow_core::BlockHash {
    cycle_track("crypto/hash_header", || {
        let header_bytes = header.to_bytes();
        zkpow_core::BlockHash::new(sha256d_80bytes(&header_bytes))
    })
}

/// Compute the continuation digest: SHA-256 of the serialized private continuation fields.
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    cycle_track("crypto/continuation_digest", || {
        let cd_bytes: [u8; CONTINUATION_DATA_SIZE] = state.continuation_bytes();
        sha256_116bytes(&cd_bytes)
    })
}

/// Commit the minimal public values and halt.
fn commit_minimal_pv(pv: &MinimalPublicValues) -> ! {
    let bytes: [u8; MINIMAL_PV_SIZE] = pv.to_bytes();
    sp1_zkvm::io::commit_slice(&bytes);
    sp1_zkvm::syscalls::syscall_halt(0)
}

// ============================================================================
// Recursive proof verification
// ============================================================================

fn verify_recursive_proof(
    proof: &Proof,
    verifier_key: &VerifierKeyDigest,
    prior_claim: &Claim,
    prior_continuation_bytes: &[u8; CONTINUATION_DATA_SIZE],
) {
    cycle_track("recursive/verify_proof", || {
        // Reject continuation from a failed prior proof.
        if proof.exit_code != 0 {
            panic!(
                "recursive continuation rejected: prior proof has exit code {}",
                proof.exit_code
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
            *verifier_key,
        );
        let prior_pv_bytes: [u8; MINIMAL_PV_SIZE] = prior_pv.to_bytes();
        let actual_pv_hash = cycle_track("recursive/public_values_digest", || {
            sha256_169bytes(&prior_pv_bytes)
        });

        if actual_pv_hash != proof.public_values_digest.into_raw() {
            panic!("recursive proof public values digest mismatch");
        }

        // 3. Verify the SP1 proof.
        cycle_track("recursive/sp1_verify", || {
            sp1_zkvm::lib::verify::verify_sp1_proof(
                verifier_key.as_raw(),
                proof.public_values_digest.as_raw(),
            );
        });
    });
}

pub fn main() {
    cycle_track("main", || {
        let input_bytes = sp1_zkvm::io::read_vec();
        let state_bytes = sp1_zkvm::io::read_vec();
        let header_hint_bytes = sp1_zkvm::io::read_vec();
        let median_hint_bytes = sp1_zkvm::io::read_vec();

        let pcs = ProofCarryingState::parse(&input_bytes).expect("input should parse");
        let mut state = cycle_track("input/parse_state_witness", || {
            State::parse(&state_bytes).expect("state witness should parse")
        });
        cycle_track("input/verify_state_claim", || {
            assert!(
                state.genesis_hash == pcs.claim.genesis_hash
                    && state.block_hash == pcs.claim.tip_hash
                    && state.chain_work == pcs.claim.chain_work
                    && state.height == pcs.claim.height,
                "state witness public claim mismatch"
            );
        });
        let header_hints = cycle_track("input/parse_header_hints", || {
            parse_new_headers(&header_hint_bytes).expect("new header hints should parse")
        });
        let median_hints = cycle_track("input/parse_median_hints", || {
            parse_median_hints(&median_hint_bytes, header_hints.len())
                .expect("median time past hints should parse")
        });

        if pcs.claim.height > 0 {
            let prior_continuation_bytes: [u8; CONTINUATION_DATA_SIZE] =
                state.continuation_bytes();
            verify_recursive_proof(
                &pcs.proof,
                &pcs.verifier_key,
                &pcs.claim,
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
                pcs.verifier_key,
            );
            commit_minimal_pv(&pv);
        }

        let digest = compute_continuation_digest(&state);
        let pv = MinimalPublicValues::success(
            &state.public_claim(),
            digest,
            pcs.verifier_key,
        );
        commit_minimal_pv(&pv);
    });
}
