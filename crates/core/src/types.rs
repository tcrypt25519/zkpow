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

    /// Subtract `rhs` from `self` in place. Returns `true` if there was an underflow (borrow out).
    #[must_use]
    pub fn sub_assign(&mut self, rhs: Self) -> bool {
        let mut borrow = false;

        let (a, b1) = self.0[0].overflowing_sub(rhs.0[0]);
        let (a, b2) = a.overflowing_sub(borrow as u64);
        self.0[0] = a;
        borrow = b1 || b2;

        let (a, b1) = self.0[1].overflowing_sub(rhs.0[1]);
        let (a, b2) = a.overflowing_sub(borrow as u64);
        self.0[1] = a;
        borrow = b1 || b2;

        let (a, b1) = self.0[2].overflowing_sub(rhs.0[2]);
        let (a, b2) = a.overflowing_sub(borrow as u64);
        self.0[2] = a;
        borrow = b1 || b2;

        let (a, b1) = self.0[3].overflowing_sub(rhs.0[3]);
        let (a, b2) = a.overflowing_sub(borrow as u64);
        self.0[3] = a;
        borrow = b1 || b2;

        borrow
    }

    /// Return whether `self >= rhs`.
    #[must_use]
    pub fn gte(self, rhs: Self) -> bool {
        for i in (0..4).rev() {
            if self.0[i] != rhs.0[i] {
                return self.0[i] > rhs.0[i];
            }
        }
        true
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
