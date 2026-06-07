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

    /// Borrow the little-endian limbs mutably.
    #[must_use]
    pub const fn as_limbs_mut(&mut self) -> &mut [u64; 4] {
        &mut self.0
    }

    /// Consume into little-endian limbs.
    #[must_use]
    pub const fn into_limbs(self) -> [u64; 4] {
        self.0
    }

    /// Construct from a 32-byte little-endian byte array.
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        Self([
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ])
    }

    /// Serialize to a 32-byte little-endian byte array.
    #[must_use]
    pub fn to_le_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&self.0[0].to_le_bytes());
        out[8..16].copy_from_slice(&self.0[1].to_le_bytes());
        out[16..24].copy_from_slice(&self.0[2].to_le_bytes());
        out[24..32].copy_from_slice(&self.0[3].to_le_bytes());
        out
    }
}

impl PartialOrd for u256 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Magnitude-correct ordering: compare from most-significant limb (index 3) down.
impl Ord for u256 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                core::cmp::Ordering::Equal => continue,
                ord => return ord,
            }
        }
        core::cmp::Ordering::Equal
    }
}

//
// Define our Branded newtypes
//

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TargetTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct CompactTargetTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ChainWorkTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BlockTimestampTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BlockHashTag;

pub type Target = Branded<TargetTag, u256>;
pub type ChainWork = Branded<ChainWorkTag, u256>;
pub type BlockTimestamp = Branded<BlockTimestampTag, u32>;
pub type CompactTarget = Branded<CompactTargetTag, u32>;
pub type BlockHash = Branded<BlockHashTag, [u8; 32]>;
