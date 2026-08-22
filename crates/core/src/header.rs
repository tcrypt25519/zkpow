use bytemuck::{Pod, PodCastError, Zeroable};
use derive_size::Size;
use sp1_cycle_macros::tracked;

use crate::{BlockHash, BlockTimestamp, CompactTarget, Header};

/// Prover-supplied fields for a new header.
///
/// The remaining header fields are not supplied: `prev_blockhash` comes from the state being
/// extended and `compact_target` from the active epoch, so a prover cannot choose either.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Size)]
pub struct HeaderInput {
    pub version:     u32,
    pub merkle_root: [u8; 32],
    pub timestamp:   BlockTimestamp,
    pub nonce:       u32,
}

impl HeaderInput {
    /// Parse a [`HeaderInput`] from exactly [`HeaderInput::SIZE`] bytes.
    #[tracked]
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, PodCastError> {
        bytemuck::try_from_bytes::<Self>(bytes)
    }

    /// Serialize to exactly [`HeaderInput::SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> &[u8; Self::SIZE] {
        let byte_slice = bytemuck::bytes_of(self);
        byte_slice.as_array().expect("HeaderInput is SIZE bytes")
    }

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
