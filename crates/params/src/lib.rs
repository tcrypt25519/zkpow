//! Consensus parameters: the [`Consensus`] trait, the [`ConsensusParams`] behind it, and the
//! concrete networks.
//!
//! This crate sits between `zkpow-types` and `zkpow-core`: it declares what a network *is*, and
//! `zkpow-core` executes the state transition generically over it.
//!
//! A network's values are written once, in its `impl Consensus`. Reach them through the trait
//! (`Mainnet::PARAMS.genesis`, `Mainnet::GENESIS_HASH`) rather than re-declaring them.

#![no_std]
#![cfg_attr(all(test, target_os = "zkvm"), feature(custom_test_frameworks))]
#![cfg_attr(all(test, target_os = "zkvm"), test_runner(zkvm_test::test_runner))]

use zkpow_types::{BlockHash, BlockTimestamp, BlockWork, CompactTarget, Header, MerkleRoot, Target};

mod consensus;
pub use consensus::{
    Consensus,
    ConsensusParams, // WINDOW_LARGE_HALF_SIZE, WINDOW_MAX_INDEX, WINDOW_SIZE, WINDOW_SMALL_HALF_SIZE,
};

mod networks;
pub use networks::*;

#[cfg(test)]
mod tests;
