//! Proof-of-work verification trait with compile-time dispatch.
//!
//! Two implementations are provided:
//! - `RealPoWVerifier`: full consensus PoW check (hash vs target)
//! - `DummyPoWVerifier`: always returns true (for testing post-PoW validation)
//!
//! The active implementation is selected via the `dummy-pow` Cargo feature.
//! Without the feature, `RealPoWVerifier` is used (default).

/// Trait for proof-of-work verification.
///
/// Checks whether a pre-computed `block_hash` (double SHA-256 of the header)
/// satisfies the difficulty target encoded in `bits`.
pub trait PoWVerifier {
    fn check(block_hash: &[u8; 32], bits: u32) -> bool;
}

/// Real consensus PoW verification.
///
/// Verifies that the given block hash is less than or equal to the target
/// encoded in `bits`.
#[allow(dead_code)]
pub struct RealPoWVerifier;

impl PoWVerifier for RealPoWVerifier {
    fn check(block_hash: &[u8; 32], bits: u32) -> bool {
        crate::hash_meets_target(block_hash, bits)
    }
}

/// Dummy PoW verifier — always returns true.
///
/// Use with `--features dummy-pow` for testing chain linkage, bits
/// validation, median timestamp checks, and retargeting without
/// needing to grind real block headers.
#[allow(dead_code)]
pub struct DummyPoWVerifier;

impl PoWVerifier for DummyPoWVerifier {
    fn check(_block_hash: &[u8; 32], _bits: u32) -> bool {
        true
    }
}

// Resolve at compile time — zero overhead, fully monomorphized.
#[cfg(not(feature = "dummy-pow"))]
pub type ActiveVerifier = RealPoWVerifier;

#[cfg(feature = "dummy-pow")]
pub type ActiveVerifier = DummyPoWVerifier;

// Compile-time assertions to prevent misconfiguration.
#[cfg(feature = "dummy-pow")]
const _: () = {
    fn _assert_type<T: ?Sized>() {}
    fn _check() {
        _assert_type::<DummyPoWVerifier>();
    }
};

#[cfg(not(feature = "dummy-pow"))]
const _: () = {
    fn _assert_type<T: ?Sized>() {}
    fn _check() {
        _assert_type::<RealPoWVerifier>();
    }
};
