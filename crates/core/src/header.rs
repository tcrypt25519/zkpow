#[cfg(test)]
use crate::{copy_from_bytes, tracked, ParseError};
use crate::{
    copy_to_bytes, BlockHash, BlockTimestamp, CompactTarget, BLOCK_HEADER_SIZE, NEW_HEADER_SIZE,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        copy_from_bytes(bytes)
    }
}

/// Prover-supplied fields for a new header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewHeader {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nonce: u32,
}

impl NewHeader {
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

    /// Parse a [`NewHeader`] from exactly [`NEW_HEADER_SIZE`] bytes.
    #[cfg(test)]
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        copy_from_bytes(bytes)
    }

    /// Serialize to exactly [`NEW_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; NEW_HEADER_SIZE] {
        copy_to_bytes(self)
    }

    /// Materialize a full [`Header`] using authenticated chain context.
    #[must_use]
    pub fn into_header(self, prev_blockhash: BlockHash, compact_target: CompactTarget) -> Header {
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

#[inline]
const fn sha256_word_from_le_u32(value: u32) -> u64 {
    value.swap_bytes() as u64
}

#[inline]
pub(crate) const fn sha256_word_from_bytes(bytes: &[u8], word_index: usize) -> u64 {
    let i = word_index * 4;
    ((bytes[i] as u64) << 24)
        | ((bytes[i + 1] as u64) << 16)
        | ((bytes[i + 2] as u64) << 8)
        | (bytes[i + 3] as u64)
}

/// Build the first SHA-256 block words for a Bitcoin header represented by
/// disjoint prover/header-state fields.
#[doc(hidden)]
#[inline]
pub fn header_first_sha256_block_words(
    header: &NewHeader,
    prev_blockhash: &BlockHash,
) -> [u64; 64] {
    let mut words = [0u64; 64];
    words[0] = sha256_word_from_le_u32(header.version);
    for i in 0..8 {
        words[1 + i] = sha256_word_from_bytes(prev_blockhash.as_inner(), i);
    }
    for i in 0..7 {
        words[9 + i] = sha256_word_from_bytes(&header.merkle_root, i);
    }
    words
}

/// Build the second SHA-256 block words for a Bitcoin header represented by
/// disjoint prover/header-state fields.
#[doc(hidden)]
#[inline]
pub fn header_second_sha256_block_words(
    header: &NewHeader,
    compact_target: CompactTarget,
) -> [u64; 64] {
    let mut words = [0u64; 64];
    words[0] = sha256_word_from_bytes(&header.merkle_root, 7);
    words[1] = sha256_word_from_le_u32(header.timestamp.into_inner());
    words[2] = sha256_word_from_le_u32(compact_target.into_inner());
    words[3] = sha256_word_from_le_u32(header.nonce);
    words[4] = 0x80000000;
    words[15] = 0x280;
    words
}
