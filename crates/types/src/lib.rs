//! The consensus value types: branded primitives and the block header.
//!
//! A leaf crate by design. `zkpow-params` needs these to declare a network and `zkpow-core` needs
//! them to run the state transition, so neither can depend on the other.

#![no_std]

mod bigint;
mod brand;
mod bytes;
mod header;

#[cfg(test)]
mod tests;

pub use bigint::{u256, NonMaxU256};
pub use brand::{
    BlockHash, BlockHashTag, BlockTimestamp, BlockTimestampTag, BlockWork, BlockWorkTag, Branded, ChainWork,
    ChainWorkTag, CompactTarget, CompactTargetTag, Target, TargetTag,
};
pub use bytes::ByteInfo;
pub use header::Header;
