use crate::brand::{BlockHash, BlockTimestamp, CompactTarget};

/// A Bitcoin block header
#[wiretype::wiretype]
#[repr(C)]
pub struct Header {
    /// TODO: Make a newtype for Version.
    /// The maximally correct underlying type is a `NonZero<i32>`
    pub version:        u32,
    pub prev_blockhash: BlockHash,
    pub merkle_root:    [u8; 32],
    pub timestamp:      BlockTimestamp,
    pub compact_target: CompactTarget,
    pub nonce:          u32,
}

impl Header {
    /// Only for const evaluation. `zkpow-params` derives a network's genesis hash at compile
    /// time. At runtime this copies for no reason; use [`Header::as_bytes`].
    #[must_use]
    pub const fn to_bytes_const(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Header::SIZE];

        let version = self.version.to_le_bytes();
        let prev_blockhash = *self.prev_blockhash.as_inner();
        let timestamp = (*self.timestamp.as_inner()).to_le_bytes();
        let compact_target = (*self.compact_target.as_inner()).to_le_bytes();
        let nonce = self.nonce.to_le_bytes();

        let mut i = 0;
        while i < 4 {
            bytes[i] = version[i];
            bytes[68 + i] = timestamp[i];
            bytes[72 + i] = compact_target[i];
            bytes[76 + i] = nonce[i];
            i += 1;
        }

        let mut i = 0;
        while i < 32 {
            bytes[4 + i] = prev_blockhash[i];
            bytes[36 + i] = self.merkle_root[i];
            i += 1;
        }

        bytes
    }
}

/// A block hash is the SHA-256d of the header's wire bytes. This is the only place that computes
/// one; the guest reaches the same value through the zkVM's syscall-backed path.
impl From<&Header> for BlockHash {
    fn from(header: &Header) -> Self {
        #[cfg(target_os = "zkvm")]
        {
            BlockHash::new(zkpow_sha256::of_header(
                header.version,
                header.prev_blockhash.as_inner(),
                &header.merkle_root,
                *header.timestamp.as_inner(),
                *header.compact_target.as_inner(),
                header.nonce,
            ))
        }
        #[cfg(not(target_os = "zkvm"))]
        {
            use sha2::{Digest, Sha256};
            let first = Sha256::digest(header.as_bytes());
            let second = Sha256::digest(first);
            BlockHash::new(second.into())
        }
    }
}
