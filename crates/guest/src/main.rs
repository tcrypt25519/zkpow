//! zkpow — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit hashes those fields with authenticated state
//! directly, then validates PoW and consensus state transitions.
//!
//! Input protocol:
//!   1. verifier_key: Vec<u8>
//!   2. state_witness: Vec<u8>        (State bytes)
//!   3. header_hints: Vec<u8>
//!   4. median_time_past_hints: Vec<u8>
//!   5. If `state.height > 0`, compressed SP1 proof witness (via write_proof)
//!
//! Output: MinimalPublicValues (169 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

mod sha256;

use zkpow_core::{
    cycle_track, parse_median_hints, parse_new_headers, tracked, Claim, MinimalPublicValues, State,
    VerifierKeyDigest, CONTINUATION_DATA_SIZE, MINIMAL_PV_SIZE,
};

use sha256::{sha256_169bytes, sha256_52bytes, sha256d_new_header};

/// Compute the continuation digest: SHA-256 of the serialized continuation data.
#[tracked]
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    let cd_bytes: [u8; CONTINUATION_DATA_SIZE] = state.continuation_bytes();
    sha256_52bytes(&cd_bytes)
}

/// Commit the minimal public values and halt.
fn commit_minimal_pv(pv: &MinimalPublicValues) -> ! {
    let bytes: [u8; MINIMAL_PV_SIZE] = pv.to_bytes();
    sp1_zkvm::io::commit_slice(&bytes);
    sp1_zkvm::syscalls::syscall_halt(0)
}

#[tracked]
fn verify_prior_proof(
    verifier_key: &[u32; 8],
    prior_claim: &Claim,
    prior_continuation_bytes: &[u8; CONTINUATION_DATA_SIZE],
) {
    // 1. Hash the prior continuation data.
    let continuation_digest = cycle_track!({ sha256_52bytes(prior_continuation_bytes) });

    // 2. Reconstruct the prior proof's minimal public values and hash them.
    let prior_pv = MinimalPublicValues::success(
        prior_claim,
        continuation_digest,
        VerifierKeyDigest::from_raw(*verifier_key),
    );
    let prior_pv_bytes: [u8; MINIMAL_PV_SIZE] = prior_pv.to_bytes();
    let actual_pv_hash = cycle_track!({ sha256_169bytes(&prior_pv_bytes) });

    // 3. Verify the SP1 proof.
    cycle_track!({
        sp1_zkvm::lib::verify::verify_sp1_proof(verifier_key, &actual_pv_hash);
    });
}

pub fn main() {
    cycle_track!("main", {
        let input_bytes = sp1_zkvm::io::read_vec();
        let state_bytes = sp1_zkvm::io::read_vec();
        let header_hint_bytes = sp1_zkvm::io::read_vec();
        let median_hint_bytes = sp1_zkvm::io::read_vec();

        let verifier_key_bytes: &[u8; 32] = input_bytes
            .as_slice()
            .try_into()
            .expect("verifier key must be exactly 32 bytes");
        // SP1's read_vec() returns an 8-byte-aligned buffer.
        let verifier_key = unsafe { &*verifier_key_bytes.as_ptr().cast::<[u32; 8]>() };
        let mut state = State::parse(&state_bytes).expect("state witness should parse");
        let header_hints = <NewHeader as FixedSize>::slice_from_bytes(&header_hint_bytes)
            .expect("new header hints should parse");
        let median_hints =
            parse_median_hints(&median_hint_bytes).expect("median time past hints should parse");

        if state.height > 0 {
            let prior_claim = state.public_claim();
            let prior_continuation_bytes: [u8; CONTINUATION_DATA_SIZE] = state.continuation_bytes();
            verify_prior_proof(verifier_key, &prior_claim, &prior_continuation_bytes);
        }

        // Apply headers; on failure commit minimal PV and halt.
        if let Err(failure) = state.apply_headers(
            header_hints,
            median_hints,
            |header, prev_blockhash, nbits| {
                zkpow_core::BlockHash::new(sha256d_new_header(header, prev_blockhash, nbits))
            },
        ) {
            let digest = compute_continuation_digest(&failure.last_valid_state);
            let pv = MinimalPublicValues::failure(
                &failure.last_valid_state.public_claim(),
                failure.error_code,
                failure.failure_height,
                digest,
                VerifierKeyDigest::from_raw(*verifier_key),
            );
            commit_minimal_pv(&pv);
        }

        let digest = compute_continuation_digest(&state);
        let pv = MinimalPublicValues::success(
            &state.public_claim(),
            digest,
            VerifierKeyDigest::from_raw(*verifier_key),
        );
        commit_minimal_pv(&pv);
    });
}
