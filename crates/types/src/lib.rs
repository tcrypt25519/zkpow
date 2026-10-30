//! The consensus value types: strongly typed primitives and the block header.
//!
//! A leaf crate by design. `zkpow-params` needs these to declare a network and `zkpow-core` needs
//! them to run the state transition, so neither can depend on the other.

#![no_std]
#![cfg_attr(all(test, target_os = "zkvm"), feature(custom_test_frameworks))]
#![cfg_attr(all(test, target_os = "zkvm"), test_runner(zkvm_test::test_runner))]

mod bigint;
mod brand;
mod header;
mod run_id;
mod target_triplet;

#[cfg(test)]
mod tests;

pub use ::wiretype::wiretype;
pub use bigint::{u256, NonMaxU256};
pub use brand::{BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, Target};
pub use header::Header;
pub use run_id::{RunId, RunIdError};
pub use target_triplet::TargetTriplet;
