use sp1_sdk::prelude::*;
use tracing::Instrument;

use crate::pipeline::diagnostics::{timed_async, timed_sync};
use crate::pipeline::execution::check_guest_success;
use crate::pipeline::BoxError;

/// Prove a batch directly from its stdin. The prover executes the guest
/// itself, so there is deliberately no separate execution pass first.
pub(crate) async fn generate_compressed_proof<P, K>(
    prover: &P,
    proving_key: &K,
    stdin: SP1Stdin,
) -> Result<SP1ProofWithPublicValues, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let compressed_proof = timed_async("prove_compressed", || async {
        async { prover.prove(proving_key, stdin).compressed().await }
            .instrument(tracing::info_span!("prove_compressed_detail"))
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;

    // Local integrity checks before shipping the artifact: the guest must
    // not have reported a failure, and the proof must verify against our own
    // verifying key. The controller independently re-validates both.
    check_guest_success(compressed_proof.public_values.as_slice(), "proving")?;
    timed_sync("verify_compressed_proof", || -> Result<(), BoxError> {
        prover
            .verify(&compressed_proof, proving_key.verifying_key(), None)
            .map_err(|err| -> BoxError { err.to_string().into() })
    })?;

    Ok(compressed_proof)
}
