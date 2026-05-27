use crate::brand::Branded;

/// A 256-bit unsigned integer stored as four little-endian u64 limbs.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]

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
}

/// Tag types for branded newtypes.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TargetTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ChainWorkTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BlockTimestampTag;

pub type Target = Branded<TargetTag, u256>;
pub type ChainWork = Branded<ChainWorkTag, u256>;
pub type BlockTimestamp = Branded<BlockTimestampTag, u32>;

impl BlockTimestamp {
    pub fn as_i64(self) -> i64 {
        self.into_inner() as i64
    }
}

/// Bitcoin compact difficulty encoding (`nBits`).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactTarget(u32);

/// Tag type for BlockHash branded newtype.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BlockHashTag;

pub type BlockHash = crate::brand::Branded<BlockHashTag, [u8; 32]>;

impl CompactTarget {
    /// Return the compact consensus encoding.
    #[must_use]
    pub const fn into_inner(self) -> u32 {
        self.0
    }

    /// Construct from consensus-encoded compact bits.
    #[must_use]
    pub const fn from_inner(bits: u32) -> Self {
        Self(bits)
    }

    /// Convert to a 4-byte little-endian byte array.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
}
