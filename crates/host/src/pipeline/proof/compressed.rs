use sp1_sdk::prelude::*;
use tracing::Instrument;

use crate::pipeline::diagnostics::{timed_async, timed_sync};
use crate::pipeline::execution::execute_batch;
use crate::pipeline::BoxError;

pub(crate) struct CompressedProofArtifacts {
    pub(crate) vk: sp1_prover::SP1VerifyingKey,
    pub(crate) compressed_proof: SP1ProofWithPublicValues,
    pub(crate) before_prove_sample: memuse::StageSample,
    pub(crate) execution_report: sp1_sdk::ExecutionReport,
}

pub(crate) async fn generate_compressed_proof<P, K>(
    prover_name: &str,
    prover: &P,
    proving_key: &K,
    stdin: SP1Stdin,
) -> Result<CompressedProofArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    // Execute first: catching a guest failure here costs seconds instead of
    // a wasted GPU proving run.
    let executed = execute_batch(prover, stdin).await?;
    tracing::info!(prover = prover_name, "Execution completed");

    let compressed_proof = timed_async("prove_compressed", || async {
        async { prover.prove(proving_key, executed.stdin).compressed().await }
            .instrument(tracing::info_span!("prove_compressed_detail"))
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;

    // Local integrity checks before shipping the artifact: the proof must
    // commit exactly what execution committed, and must verify against our
    // own verifying key. The controller independently re-validates.
    if compressed_proof.public_values.as_slice() != executed.public_values.as_slice() {
        return Err("compressed proof commits different public values than execution".into());
    }
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
