use core::fmt;

use bytemuck::{Pod, Zeroable};

use crate::bigint::{u256, NonMaxU256};

macro_rules! define_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident($inner:ty);
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(transparent)]
        $vis struct $name($inner);

        impl $name {
            pub const SIZE: usize = core::mem::size_of::<$inner>();

            #[must_use]
            pub const fn new(inner: $inner) -> Self {
                Self(inner)
            }

            #[must_use]
            pub fn into_inner(self) -> $inner {
                self.0
            }

            #[must_use]
            pub const fn as_inner(&self) -> &$inner {
                &self.0
            }

            #[must_use]
            pub fn as_inner_mut(&mut self) -> &mut $inner {
                &mut self.0
            }
        }

        impl core::ops::Deref for $name {
            type Target = $inner;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl core::ops::DerefMut for $name {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

macro_rules! define_pod_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident($inner:ty);
    ) => {
        define_newtype! {
            #[derive(Pod, Zeroable)]
            $(#[$meta])*
            $vis struct $name($inner);
        }

        impl $name {
            /// Cast `bytes` to the inner type. Panics unless the length and alignment match exactly.
            pub fn from_bytes(bytes: &[u8]) -> &$inner {
                bytemuck::from_bytes::<$inner>(bytes)
            }

            /// The value's bytes. `N` must equal the inner type's size or this panics.
            pub fn to_bytes<const N: usize>(&self) -> &[u8; N] {
                bytemuck::bytes_of(&self.0)
                    .as_array()
                    .expect("N must equal the inner type's size")
            }
        }
    };
}

macro_rules! impl_add {
    ($name:ident) => {
        impl core::ops::Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl core::ops::AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                self.0 = self.0 + rhs.0;
            }
        }
    };
}

macro_rules! define_u32_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident(u32);
    ) => {
        define_pod_newtype! {
            $(#[$meta])*
            $vis struct $name(u32);
        }
        impl_add!($name);

        impl $name {
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
                // Safety: u32 has no alignment padding, and the crate enforces a little-endian target.
                unsafe { core::slice::from_raw_parts(&self.0 as *const u32 as *const u8, 4) }
            }

            #[must_use]
            pub fn from_le_bytes_slice(slice: &[u8]) -> Self {
                assert!(slice.len() == 4, "from_le_bytes_slice: expected 4 bytes, got {}", slice.len());
                let bytes: [u8; 4] = slice.try_into().unwrap();
                Self::from_le_bytes(bytes)
            }
        }
    };
}

macro_rules! define_bytes32_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident([u8; 32]);
    ) => {
        define_pod_newtype! {
            $(#[$meta])*
            $vis struct $name([u8; 32]);
        }

        impl $name {
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
    };
}

macro_rules! define_u256_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident(u256);
    ) => {
        define_pod_newtype! {
            $(#[$meta])*
            $vis struct $name(u256);
        }
        impl_add!($name);

        impl $name {
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
                // Safety: u256 is transparent over [u64; 4], and the crate enforces a little-endian target.
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
    };
}

define_bytes32_newtype! {
    /// A double-SHA-256 digest of a block header.
    pub struct BlockHash([u8; 32]);
}

define_u32_newtype! {
    /// A block's Unix timestamp.
    pub struct BlockTimestamp(u32);
}

define_u256_newtype! {
    /// The amount of work proven by a single block.
    pub struct BlockWork(u256);
}

define_u256_newtype! {
    /// The cumulative work of a chain of blocks.
    pub struct ChainWork(u256);
}

define_newtype! {
    /// The threshold which a valid [`BlockHash`] must not exceed.
    pub struct Target(NonMaxU256);
}

define_u32_newtype! {
    /// The compact representation of a [`Target`].
    pub struct CompactTarget(u32);
}

impl fmt::Display for BlockTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Target {
    /// Build a target from four little-endian 64-bit limbs.
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self::new(NonMaxU256::from_limbs(limbs))
    }
}

impl core::ops::Add<BlockWork> for ChainWork {
    type Output = ChainWork;

    fn add(self, rhs: BlockWork) -> Self::Output {
        Self::new(self.into_inner() + rhs.into_inner())
    }
}

impl core::ops::AddAssign<BlockWork> for ChainWork {
    fn add_assign(&mut self, rhs: BlockWork) {
        *self = *self + rhs;
    }
}

impl core::ops::Mul<u32> for BlockWork {
    type Output = ChainWork;

    fn mul(self, rhs: u32) -> Self::Output {
        ChainWork::new(self.into_inner() * rhs)
    }
}

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
