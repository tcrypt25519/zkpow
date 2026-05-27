use crate::brand::Branded;

/// A 256-bit unsigned integer stored as four little-endian u64 limbs.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct u256([u64; 4]);

impl u256 {
    /// Construct from little-endian limbs.
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self(limbs)
    }

    /// Borrow the little-endian limbs.
    #[must_use]
    pub const fn as_limbs(&self) -> &[u64; 4] {
        &self.0
    }

    /// Consume into little-endian limbs.
    #[must_use]
    pub const fn into_limbs(self) -> [u64; 4] {
        self.0
    }

    /// Construct from a 32-byte little-endian byte array.
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for (idx, limb) in limbs.iter_mut().enumerate() {
            let start = idx * 8;
            *limb = u64::from_le_bytes(bytes[start..start + 8].try_into().unwrap());
        }
        Self(limbs)
    }

    /// Construct from a 32-byte big-endian byte array.
    #[must_use]
    pub fn from_be_bytes(bytes: [u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for (idx, limb) in limbs.iter_mut().enumerate() {
            let start = idx * 8;
            *limb = u64::from_be_bytes(bytes[start..start + 8].try_into().unwrap());
        }
        Self(limbs)
    }

    /// Convert to a 32-byte little-endian byte array.
    #[must_use]
    pub fn to_le_bytes(self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for (idx, limb) in self.0.iter().enumerate() {
            bytes[idx * 8..(idx + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        bytes
    }

    /// Convert to a 32-byte big-endian byte array.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for (idx, limb) in self.0.iter().enumerate() {
            bytes[idx * 8..(idx + 1) * 8].copy_from_slice(&limb.to_be_bytes());
        }
        bytes
    }

    // Returns the larger of the two.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        if self >= other {
            return self;
        }
        other
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TargetTag;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainWorkTag;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockHashTag;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct MerkleRootTag;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockTimestampTag;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompactTargetTag;

pub type Target = Branded<TargetTag, u256>;
pub type ChainWork = Branded<ChainWorkTag, u256>;
pub type BlockHash = [u8; 32];
pub type MerkleRoot = [u8; 32];
pub type BlockTimestamp = Branded<BlockTimestampTag, u32>;
pub type CompactTarget = Branded<CompactTargetTag, u32>;
