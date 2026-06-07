use sp1_sdk::prelude::*;
use tracing::Instrument;

use crate::pipeline::batch::PreparedBatch;
use crate::pipeline::diagnostics::{timed_async, timed_sync};
use crate::pipeline::execution::{execute_batch_with_prover, verify_public_values};
use crate::pipeline::BoxError;

pub(crate) struct CompressedProofArtifacts {
    pub(crate) vk: sp1_prover::SP1VerifyingKey,
    pub(crate) compressed_proof: SP1ProofWithPublicValues,
    pub(crate) before_prove_sample: memory_usage::StageSample,
    pub(crate) execution_report: sp1_sdk::ExecutionReport,
}

pub(crate) async fn generate_compressed_proof<P, K>(
    prover_name: &str,
    prover: &P,
    proving_key: &K,
    batch: &PreparedBatch,
) -> Result<CompressedProofArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let executed = execute_batch_with_prover(prover_name, prover, proving_key, batch).await?;

    let compressed_proof = timed_async("prove_compressed", || async {
        async { prover.prove(proving_key, executed.stdin).compressed().await }
            .instrument(tracing::info_span!("prove_compressed_detail"))
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;
    timed_sync(
        "verify_compressed_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(
                &compressed_proof.public_values.to_vec(),
                &executed.expected_pv,
                "compressed proof",
            )
        },
    )?;
    timed_sync("verify_compressed_proof", || -> Result<(), BoxError> {
        prover
            .verify(&compressed_proof, proving_key.verifying_key(), None)
            .map_err(|err| -> BoxError { err.to_string().into() })
    })?;

    Ok(CompressedProofArtifacts {
        vk: proving_key.verifying_key().clone(),
        compressed_proof,
        before_prove_sample: executed.before_prove_sample,
        execution_report: executed.execution_report,
    })
}
