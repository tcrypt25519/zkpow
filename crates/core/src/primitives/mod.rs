//! Branded consensus primitives: [`Branded`] and [`u256`] plus the domain-specific
//! types built on top of them. Everything here is available as `primitives::<Type>`;
//! callers should not need to reach into the `brand` or `u256` submodules directly.

use bytemuck::{Pod, Zeroable};

mod brand;
mod bigint;
mod bytes;
pub use bytes::ByteInfo;
pub use brand::Branded;
pub use bigint::u256;
// pub use byte::Info as ByteInfo;

// Define tags for domain-specific versions of primitive types

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
pub struct  BlockHashTag;

/// BlockHash is a sha256d digest of a block header
pub type BlockHash = Branded<BlockHashTag, [u8; 32]>;

/// BlockTimestamp is the timestamp of a block
pub type BlockTimestamp = Branded<BlockTimestampTag, u32>;

/// BlockWork is the amount of work proven by a single block
pub type BlockWork = Branded<BlockWorkTag, u256>;

/// ChainWork is the cumulative work of a chain of blocks
pub type ChainWork = Branded<ChainWorkTag, u256>;

/// Target is the target for a valid BlockHash to be below
pub type Target = Branded<TargetTag, u256>;

/// CompactTarget is the compact representation of a target
pub type CompactTarget = Branded<CompactTargetTag, u32>;

// `ChainWork + ChainWork = ChainWork` and `ChainWork += ChainWork` come for free from the
// blanket `Add`/`AddAssign` impls on `Branded<Tag, T>` in `brand.rs`, since both sides share the
// `ChainWorkTag`. Only the cross-tag combinations need to be spelled out here.

// ChainWork + BlockWork = ChainWork
impl core::ops::Add<BlockWork> for ChainWork {
    type Output = ChainWork;

    fn add(self, rhs: BlockWork) -> Self::Output {
        Self::new(self.into_inner() + rhs.into_inner())
    }
}

// ChainWork += BlockWork
impl core::ops::AddAssign<BlockWork> for ChainWork {
    fn add_assign(&mut self, rhs: BlockWork) {
        *self = *self + rhs;
    }
}

// BlockWork * u32 = ChainWork
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
