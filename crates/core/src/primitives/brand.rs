use core::marker::PhantomData;

use bytemuck::{Pod, Zeroable};

use super::u256;

/// Generic branded newtype wrapper providing type-safe distinction without runtime overhead.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable)]
#[cfg_attr(
    not(target_os = "zkvm"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[repr(transparent)]
pub struct Branded<Tag, T>(T, PhantomData<fn(Tag) -> Tag>);

impl<Tag, T> Branded<Tag, T> {
    pub const SIZE: usize = core::mem::size_of::<T>();

    #[must_use]
    pub const fn new(inner: T) -> Self {
        Self(inner, PhantomData)
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }

    #[must_use]
    pub fn as_inner(&self) -> &T {
        &self.0
    }

    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<Tag: Pod + Zeroable, T: Pod + Zeroable> Branded<Tag, T> {
    pub fn from_bytes(bytes: &[u8]) -> &T {
        bytemuck::from_bytes::<T>(bytes)
    }

    // `&[u8; Self::SIZE]` isn't expressible on stable (generic associated const
    // as array length); N is checked against size_of::<T>() at runtime instead.
    pub fn to_bytes<const N: usize>(&self) -> &[u8; N] {
        bytemuck::bytes_of(&self.0).as_array().expect("N must equal size_of::<T>()")
    }
}

impl<Tag, T> core::ops::Deref for Branded<Tag, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Tag, T> core::ops::DerefMut for Branded<Tag, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<Tag, T: core::fmt::Debug> core::fmt::Debug for Branded<Tag, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl<Tag, T: core::ops::Add<Output = T>> core::ops::Add for Branded<Tag, T> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0, PhantomData)
    }
}

impl<Tag, T: Copy + core::ops::Add<Output = T>> core::ops::AddAssign for Branded<Tag, T> {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0 + rhs.0;
    }
}

/// Blanket implementations for `Branded<Tag, u32>`.
impl<Tag: Pod + Zeroable> Branded<Tag, u32> {
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::new(u32::from_le_bytes(bytes))
    }

    #[must_use]
    pub fn to_le_bytes(&self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        // Safety: u32 has no alignment padding, crate enforces LE target.
        unsafe { core::slice::from_raw_parts(&self.0 as *const u32 as *const u8, 4) }
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 4, "from_le_bytes_slice: expected 4 bytes, got {}", slice.len());
        let bytes: [u8; 4] = slice.try_into().unwrap();
        Self::from_le_bytes(bytes)
    }
}

/// Blanket implementations for `Branded<Tag, [u8; 32]>`.
impl<Tag: Pod + Zeroable> Branded<Tag, [u8; 32]> {
    #[must_use]
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }

    #[must_use]
    pub fn to_le_bytes(&self) -> [u8; 32] {
        self.0
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 32, "from_le_bytes_slice: expected 32 bytes, got {}", slice.len());
        let bytes: [u8; 32] = slice.try_into().unwrap();
        Self::new(bytes)
    }
}

/// Blanket implementations for `Branded<Tag, u256>`.
impl<Tag: Pod + Zeroable> Branded<Tag, u256> {
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
        self.0.to_le_bytes()
    }

    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        // Safety: u256 is repr(transparent) over [u64; 4] and crate enforces LE target.
        unsafe { core::slice::from_raw_parts(&self.0 as *const u256 as *const u8, 32) }
    }

    #[must_use]
    pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
        assert!(slice.len() == 32, "from_le_bytes_slice: expected 32 bytes, got {}", slice.len());
        let bytes: [u8; 32] = slice.try_into().unwrap();
        Self::from_le_bytes(bytes)
    }

    #[must_use]
    pub fn as_limbs(&self) -> &[u64; 4] {
        self.0.as_limbs()
    }
}
