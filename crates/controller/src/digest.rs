use sha2::{Digest, Sha256};
use zkpow_core::{ContinuationData, State};

/// Compute the continuation digest: SHA-256 of the serialized continuation data.
pub fn continuation_digest(data: &ContinuationData) -> [u8; 32] {
    Sha256::digest(data.to_bytes()).into()
}

/// Compute the continuation digest directly from a [`State`].
pub fn continuation_digest_from_state(state: &State) -> [u8; 32] {
    continuation_digest(&ContinuationData::from_state(state))
}
