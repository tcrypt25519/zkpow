//! Proof verification hook for the controller.
//!
//! The controller always validates guest-committed public values against its
//! own database-derived expectation; a [`ProofVerifier`] additionally checks
//! the uploaded proof cryptographically. The trait keeps `sp1-sdk` out of
//! the controller's default dependency graph. The host injects an
//! SDK-backed implementation.

pub trait ProofVerifier: Send + Sync {
    fn verifier_key_digest(&self) -> zkpow_core::VerifierKeyDigest;

    /// Cryptographically verify a compressed proof artifact. The proof's
    /// committed public values must equal `expected_public_values` (which
    /// the controller has already validated against its database).
    fn verify_compressed(&self, proof_bytes: &[u8], expected_public_values: &[u8]) -> Result<(), String>;
}
