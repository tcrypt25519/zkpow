use bytemuck::{Pod, Zeroable};
#[cfg(feature = "host")]
use serde::{Deserialize, Serialize};

use crate::brand::Branded;

/// A 256-bit unsigned integer stored as four little-endian u64 limbs.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Pod, Zeroable)]
#[cfg_attr(feature = "host", derive(Serialize, Deserialize))]
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

impl core::ops::MulAssign<u32> for u256 {
    #[sp1_cycle::tracked]
    fn mul_assign(&mut self, rhs: u32) {
        let multiplier = rhs as u128;
        let mut carry = 0u128;
        for limb in self.0.iter_mut() {
            let product = (*limb as u128) * multiplier + carry;
            *limb = product as u64;
            carry = product >> 64;
        }
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

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct TargetTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct CompactTargetTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct BlockWorkTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct ChainWorkTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct BlockTimestampTag;
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct BlockHashTag;

pub type BlockHash = Branded<BlockHashTag, [u8; 32]>;
pub type BlockTimestamp = Branded<BlockTimestampTag, u32>;
pub type Target = Branded<TargetTag, u256>;
pub type CompactTarget = Branded<CompactTargetTag, u32>;
pub type BlockWork = Branded<BlockWorkTag, u128>;
pub type ChainWork = Branded<ChainWorkTag, u256>;

impl core::fmt::Display for BlockHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut bytes = self.to_le_bytes();
        bytes.reverse();
        for byte in bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl core::ops::Add for ChainWork {
    type Output = Self;

    #[sp1_cycle::tracked]
    fn add(self, rhs: Self) -> Self::Output {
        let a = self.as_limbs();
        let b = rhs.as_limbs();
        let mut result = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let sum = (a[i] as u128) + (b[i] as u128) + carry;
            result[i] = sum as u64;
            carry = sum >> 64;
        }
        ChainWork::from_limbs(result)
    }
}

impl core::ops::MulAssign<u32> for ChainWork {
    fn mul_assign(&mut self, rhs: u32) {
        *self.as_inner_mut() *= rhs;
    }
}

impl core::ops::MulAssign<u32> for Target {
    fn mul_assign(&mut self, rhs: u32) {
        *self.as_inner_mut() *= rhs;
    }
}
