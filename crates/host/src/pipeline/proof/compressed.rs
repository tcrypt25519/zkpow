use sp1_sdk::prelude::*;

use crate::pipeline::{diagnostics::timed_async, BoxError};

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
    timed_async("prove_compressed", || async {
        zkpow_worker::prove_compressed(prover, proving_key, stdin)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await
}
