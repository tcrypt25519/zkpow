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

    #[must_use]
    pub fn as_inner(&self) -> &T {
        &self.inner
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

/// Blanket implementations for `Branded<Tag, u256>`.
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
    pub fn to_le_bytes(&self) -> [u8; 32] {
        self.inner.to_le_bytes()
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        // Safety: u256 is repr(transparent) over [u64; 4] and crate enforces LE target.
        unsafe { core::slice::from_raw_parts(&self.inner as *const u256 as *const u8, 32) }
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 32, "from_le_bytes_slice: expected 32 bytes, got {}", slice.len());
        let bytes: [u8; 32] = slice.try_into().unwrap();
        Self::from_le_bytes(bytes)
    }

    #[must_use]
    pub fn as_limbs(&self) -> &[u64; 4] {
        self.inner.as_limbs()
    }
}

/// Blanket implementations for `Branded<Tag, u32>`.
impl<Tag> Branded<Tag, u32> {
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::new(u32::from_le_bytes(bytes))
    }

    #[must_use]
    pub fn to_le_bytes(&self) -> [u8; 4] {
        self.inner.to_le_bytes()
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        // Safety: u32 has no alignment padding, crate enforces LE target.
        unsafe { core::slice::from_raw_parts(&self.inner as *const u32 as *const u8, 4) }
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 4, "from_le_bytes_slice: expected 4 bytes, got {}", slice.len());
        let bytes: [u8; 4] = slice.try_into().unwrap();
        Self::from_le_bytes(bytes)
    }
}

/// Blanket implementations for `Branded<Tag, [u8; 32]>`.
impl<Tag> Branded<Tag, [u8; 32]> {
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }

    #[must_use]
    pub fn to_le_bytes(&self) -> [u8; 32] {
        self.inner
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        &self.inner
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 32, "from_le_bytes_slice: expected 32 bytes, got {}", slice.len());
        let bytes: [u8; 32] = slice.try_into().unwrap();
        Self::new(bytes)
    }
}
