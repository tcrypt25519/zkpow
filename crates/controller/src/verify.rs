//! Proof verification hook for the controller.
//!
//! The controller always validates guest-committed public values against its
//! own database-derived expectation; a [`ProofVerifier`] additionally checks
//! the uploaded proof cryptographically. The trait keeps `sp1-sdk` out of
//! the controller's default dependency graph: the host injects an
//! SDK-backed implementation in local mode, and the serve binary builds one
//! from a guest ELF when compiled with the `sp1-verify` feature.

pub trait ProofVerifier: Send + Sync {
    /// Cryptographically verify a compressed proof artifact. The proof's
    /// committed public values must equal `expected_public_values` (which
    /// the controller has already validated against its database).
    fn verify_compressed(
        &self,
        proof_bytes: &[u8],
        expected_public_values: &[u8],
    ) -> Result<(), String>;
}

#[cfg(feature = "sp1-verify")]
pub use sp1::Sp1ElfVerifier;

#[cfg(feature = "sp1-verify")]
mod sp1 {
    use sp1_sdk::prelude::*;
    use sp1_sdk::{CpuProver, ProverClient, SP1ProofWithPublicValues};

    use super::ProofVerifier;
    use crate::BoxError;

    /// Verifies compressed proofs against the verifying key of a guest ELF
    /// loaded at startup, so the controller — not the host — decides which
    /// guest program a run may extend.
    pub struct Sp1ElfVerifier {
        prover: CpuProver,
        vk: sp1_prover::SP1VerifyingKey,
    }

    impl Sp1ElfVerifier {
        pub async fn from_elf(elf_bytes: Vec<u8>) -> Result<Self, BoxError> {
            let prover = ProverClient::builder().cpu().build().await;
            let proving_key = prover
                .setup(sp1_sdk::Elf::from(elf_bytes))
                .await
                .map_err(|err| format!("failed to set up guest ELF: {err}"))?;
            let vk = proving_key.verifying_key().clone();
            Ok(Self { prover, vk })
        }

        /// Digest the controller pins new runs to.
        pub fn verifier_key_digest(&self) -> zkpow_core::VerifierKeyDigest {
            zkpow_core::VerifierKeyDigest::from_raw(self.vk.hash_u32())
        }
    }

    impl ProofVerifier for Sp1ElfVerifier {
        fn verify_compressed(
            &self,
            proof_bytes: &[u8],
            expected_public_values: &[u8],
        ) -> Result<(), String> {
            let proof: SP1ProofWithPublicValues = bincode::deserialize(proof_bytes)
                .map_err(|err| format!("uploaded proof does not deserialize: {err}"))?;
            if proof.public_values.as_slice() != expected_public_values {
                return Err("uploaded proof commits different public values".to_string());
            }
            self.prover
                .verify(&proof, &self.vk, None)
                .map_err(|err| format!("SP1 verification failed: {err}"))
        }
    }
}
