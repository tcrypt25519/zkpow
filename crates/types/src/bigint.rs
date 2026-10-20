use core::ops::Deref;

/// A 256-bit unsigned integer stored as four little-endian u64 limbs.
#[allow(non_camel_case_types)]
#[wiretype::wiretype]
#[repr(transparent)]
#[cfg_attr(
    not(target_os = "zkvm"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
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

    /// Construct from eight 32-bit big-endian SHA state words.
    #[must_use]
    pub const fn from_words(words: &[u64; 8]) -> Self {
        Self([
            ((words[0] as u32).swap_bytes() as u64) | (((words[1] as u32).swap_bytes() as u64) << 32),
            ((words[2] as u32).swap_bytes() as u64) | (((words[3] as u32).swap_bytes() as u64) << 32),
            ((words[4] as u32).swap_bytes() as u64) | (((words[5] as u32).swap_bytes() as u64) << 32),
            ((words[6] as u32).swap_bytes() as u64) | (((words[7] as u32).swap_bytes() as u64) << 32),
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

    /// Return the number of leading zero bits in the 256-bit representation.
    #[must_use]
    pub const fn leading_zeros(&self) -> u32 {
        if self.0[3] != 0 {
            self.0[3].leading_zeros()
        } else if self.0[2] != 0 {
            64 + self.0[2].leading_zeros()
        } else if self.0[1] != 0 {
            128 + self.0[1].leading_zeros()
        } else {
            192 + self.0[0].leading_zeros()
        }
    }

    /// Return the minimum number of bits required to represent this integer.
    #[must_use]
    pub const fn bits(&self) -> u32 {
        256 - self.leading_zeros()
    }
}

impl core::ops::Add for u256 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut result = [0u64; 4];
        let mut carry = 0u64;
        for i in 0..4 {
            let (sum, overflow) = self.0[i].overflowing_add(rhs.0[i]);
            let (sum_with_carry, overflow2) = sum.overflowing_add(carry);
            result[i] = sum_with_carry;
            carry = (overflow as u64) + (overflow2 as u64);
        }
        Self(result)
    }
}

impl core::ops::MulAssign<u32> for u256 {
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

impl core::ops::Mul<u32> for u256 {
    type Output = Self;

    fn mul(mut self, rhs: u32) -> Self::Output {
        self *= rhs;
        self
    }
}

impl PartialOrd for u256 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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

impl TryFrom<&[u8]> for u256 {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 32 {
            return Err("Invalid length");
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(value);
        Ok(Self::from_le_bytes(arr))
    }
}

/// A [`u256`] excluding the all-ones maximum.
///
/// Block work is `2^256 / (target + 1)`, so a target must survive `+ 1` without wrapping. Storing
/// it as `NonMaxU256` makes that a type guarantee instead of a runtime check.
#[wiretype::wiretype]
#[derive(PartialOrd, Ord)]
#[repr(transparent)]
pub struct NonMaxU256(u256);

impl NonMaxU256 {
    /// Construct a value unless every limb is saturated.
    pub const fn new(value: u256) -> Option<Self> {
        let limbs = value.as_limbs();
        if limbs[0] == u64::MAX && limbs[1] == u64::MAX && limbs[2] == u64::MAX && limbs[3] == u64::MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    /// [`NonMaxU256::new`] from limbs. Panics when every limb is `u64::MAX`.
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self::try_from_limbs(limbs).expect("should be given non-max limbs")
    }
    #[must_use]
    pub const fn try_from_limbs(limbs: [u64; 4]) -> Option<Self> {
        Self::new(u256::from_limbs(limbs))
    }

    #[must_use]
    pub const fn into_inner(self) -> u256 {
        self.0
    }

    #[must_use]
    pub const fn as_inner(&self) -> &u256 {
        &self.0
    }
}

impl Deref for NonMaxU256 {
    type Target = u256;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
