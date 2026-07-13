use crate::{brand::Branded, tracked};

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

impl core::ops::MulAssign<u32> for u256 {
    #[tracked]
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

    #[tracked]
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

// impl core::ops::Deref for Target {
//     type Target = u256;

//     #[inline]
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl core::ops::DerefMut for Target {
//     #[inline]
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         self.as_inner_mut()
//     }
// }
