use crate::{BlockHash, BlockTimestamp, CompactTarget, Header};

/// Prover-supplied fields for a new header.
///
/// The remaining header fields are not supplied: `prev_blockhash` comes from the state being
/// extended and `compact_target` from the active epoch, so a prover cannot choose either.
#[wiretype::wiretype]
pub struct HeaderInput {
    pub version:     u32,
    pub merkle_root: [u8; 32],
    pub timestamp:   BlockTimestamp,
    pub nonce:       u32,
}

impl HeaderInput {
    /// Complete this input into a full [`Header`] using the two fields a prover does not choose:
    /// the tip it extends and the target active for its epoch.
    #[must_use]
    pub fn to_header(&self, prev_blockhash: BlockHash, compact_target: CompactTarget) -> Header {
        Header {
            version: self.version,
            prev_blockhash,
            merkle_root: self.merkle_root,
            timestamp: self.timestamp,
            compact_target,
            nonce: self.nonce,
        }
    }
}
