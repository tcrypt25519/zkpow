use crate::types::u256;
use core::marker::PhantomData;

/// Generic branded newtype wrapper providing type-safe distinction without runtime overhead.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Branded<Tag, T> {
    inner: T,
    _tag: PhantomData<fn(Tag) -> Tag>,
}

impl<Tag, T> Branded<Tag, T> {
    #[must_use]
    pub const fn new(inner: T) -> Self {
        Self {
            inner,
            _tag: PhantomData,
        }
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<Tag, T> core::ops::Deref for Branded<Tag, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Tag, T: core::fmt::Debug> core::fmt::Debug for Branded<Tag, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
}

/// Blanket implementations for u256.
impl<Tag> Branded<Tag, u256> {
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self::new(u256::from_limbs(limbs))
    }

    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        Self::new(u256::from_le_bytes(bytes))
    }

    #[must_use]
    pub fn to_le_bytes(self) -> [u8; 32] {
        self.inner.to_le_bytes()
    }
}

/// Blanket implementations for u32.
impl<Tag> Branded<Tag, u32> {
    /// Construct from little-endian bytes.
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::new(u32::from_le_bytes(bytes))
    }

    /// Construct from a consensus-encoded u32 (generic constructor).
    #[must_use]
    pub const fn from_inner(value: u32) -> Self {
        Self::new(value)
    }

    pub fn to_le_bytes(self) -> [u8; 4] {
        self.inner.to_le_bytes()
    }

    pub fn wrapping_sub(self, other: Self) -> u32 {
        self.inner.wrapping_sub(*other)
    }
}

/// Blanket implementations for fixed-size byte arrays (useful for BlockHash and similar).
impl<Tag> Branded<Tag, [u8; 32]> {
    /// Construct from raw little-endian bytes.
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self::new(raw)
    }

    /// Borrow the raw little-endian bytes.
    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.inner
    }

    /// Consume into raw little-endian bytes.
    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.inner
    }
}
