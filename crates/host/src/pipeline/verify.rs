//! SP1-backed proof verification for the in-process (local mode) controller.
//!
//! In local mode the controller runs inside the host process, so it borrows
//! the host's prepared prover to cryptographically verify uploaded proofs —
//! the same check a standalone `zkpow-controller-serve` performs when built
//! with its `sp1-verify` feature.

use sp1_sdk::prelude::*;
use sp1_sdk::SP1ProofWithPublicValues;

use zkpow_controller::verify::ProofVerifier;

use crate::pipeline::proof::PreparedProver;

pub struct PreparedProverVerifier {
    prepared: PreparedProver,
}

impl PreparedProverVerifier {
    pub(crate) fn new(prepared: PreparedProver) -> Self {
        Self { prepared }
    }
}

impl ProofVerifier for PreparedProverVerifier {
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
        let verified = match &self.prepared {
            PreparedProver::Mock { prover, proving_key } => {
                prover.verify(&proof, proving_key.verifying_key(), None)
            }
            PreparedProver::Cpu { prover, proving_key } => {
                prover.verify(&proof, proving_key.verifying_key(), None)
            }
            #[cfg(feature = "CUDA")]
            PreparedProver::Cuda { prover, proving_key } => {
                prover.verify(&proof, proving_key.verifying_key(), None)
            }
        };
        verified.map_err(|err| format!("SP1 verification failed: {err}"))
    }
}
