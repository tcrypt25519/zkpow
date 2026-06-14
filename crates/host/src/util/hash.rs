use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

use super::{BlockHash, ContinuationData, Header, State};

/// Compute SHA256d of the given data.
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Hash a full Bitcoin header with SHA256d.
#[must_use]
pub fn hash_header(header: &Header) -> BlockHash {
    BlockHash::new(sha256d(&header.to_bytes()))
}

/// Compute SHA-256 digest of public values.
pub fn compute_pv_digest(committed_bytes: &[u8]) -> [u8; 32] {
    let digest = SP1PublicValues::from(committed_bytes).hash();
    digest
        .try_into()
        .expect("SP1 public values hash must be 32 bytes")
}

/// Compute the continuation digest: SHA-256 of the serialized continuation data.
pub fn continuation_digest(data: &ContinuationData) -> [u8; 32] {
    Sha256::digest(data.to_bytes()).into()
}

/// Compute the continuation digest directly from a [`State`].
pub fn continuation_digest_from_state(state: &State) -> [u8; 32] {
    continuation_digest(&ContinuationData::from_state(state))
}
