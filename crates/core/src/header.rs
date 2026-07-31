use bytemuck::{Pod, PodCastError, Zeroable};
use sp1_cycle_macros::tracked;

#[cfg(test)]
use crate::{check_exact_len, ParseError};
use crate::{copy_to_bytes, BlockHash, BlockTimestamp, CompactTarget, BLOCK_HEADER_SIZE, NEW_HEADER_SIZE};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct Header {
    /// TODO: Make a newtype for Version.
    /// The maximally correct underlying type is a NonZero<i32>
    pub version: u32,
    pub prev_blockhash: BlockHash,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub compact_target: CompactTarget,
    pub nonce: u32,
}

impl Header {
    /// Serialize to exactly [`BLOCK_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; BLOCK_HEADER_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`BLOCK_HEADER_SIZE`] bytes.
    #[cfg(test)]
    #[sp1_cycle::tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, BLOCK_HEADER_SIZE)?;
        Ok(bytemuck::pod_read_unaligned(bytes))
    }
}

/// Prover-supplied fields for a new header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct HeaderInput {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nonce: u32,
}

impl HeaderInput {
    /// Extract the prover-supplied subset of fields from a full [`Header`].
    #[must_use]
    pub fn from_header(header: &Header) -> Self {
        Self {
            version: header.version,
            merkle_root: header.merkle_root,
            timestamp: header.timestamp,
            nonce: header.nonce,
        }
    }

    /// Parse a [`HeaderInput`] from exactly [`NEW_HEADER_SIZE`] bytes.
    #[tracked]
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, PodCastError> {
        bytemuck::try_from_bytes::<Self>(bytes)
    }

    /// Serialize to exactly [`NEW_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> &[u8; NEW_HEADER_SIZE] {
        let byte_slice = bytemuck::bytes_of(self);
        byte_slice.as_array().expect("a")
    }
}
