use crate::{BlockHash, BlockTimestamp, CompactTarget};
use bytemuck::{Pod, PodCastError, Zeroable};
use derive_size::Size;
use sp1_cycle_macros::tracked;
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Pod, Zeroable, Size)]
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
    // const SIZE: usize = 80;

    /// Serialize to exactly [`BLOCK_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Header::SIZE] {
        *bytemuck::bytes_of(self).as_array().expect("header is 80 bytes")
    }

    /// Parse a [`Header`] from exactly [`BLOCK_HEADER_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<&Self, PodCastError> {
        bytemuck::try_from_bytes::<Self>(bytes)
    }
}

/// Prover-supplied fields for a new header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Size)]
pub struct HeaderInput {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nonce: u32,
}

impl HeaderInput {
    // const SIZE: usize = 44;

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
    pub fn to_bytes(&self) -> &[u8; Self::SIZE] {
        let byte_slice = bytemuck::bytes_of(self);
        byte_slice.as_array().expect("a")
    }
}
