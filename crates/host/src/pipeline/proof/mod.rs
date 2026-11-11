pub(crate) mod compressed;

use std::sync::OnceLock;

use sp1_sdk::{
    env::{EnvProver, EnvProvingKey},
    prelude::*,
    Prover, ProverClient,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::pipeline::{diagnostics::timed_async, BoxError, ProverBackend, ELF};

static PREPARED_PROVER: OnceLock<AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedProverConfig {
    backend: ProverBackend,
}

#[derive(Clone)]
pub(crate) struct PreparedProver {
    prover: EnvProver,
    proving_key: EnvProvingKey,
}

impl PreparedProver {
    pub(crate) fn prover(&self) -> &EnvProver {
        &self.prover
    }

    pub(crate) fn proving_key(&self) -> &EnvProvingKey {
        &self.proving_key
    }

    /// The verifying key the guest recursion is pinned to.
    pub(crate) fn verifying_key(&self) -> &sp1_prover::SP1VerifyingKey {
        self.proving_key.verifying_key()
    }

    /// Digest of the verifying key, as committed in every proof's public values.
    pub(crate) fn verifier_key_digest(&self) -> zkpow_core::VerifierKeyDigest {
        zkpow_core::VerifierKeyDigest::from_raw(self.verifying_key().hash_u32())
    }
}

fn prepared_prover_store() -> &'static AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>> {
    PREPARED_PROVER.get_or_init(|| AsyncMutex::new(None))
}

pub(crate) async fn get_prepared_prover(prover_backend: ProverBackend) -> Result<PreparedProver, BoxError> {
    let desired = PreparedProverConfig { backend: prover_backend };

    {
        let guard = prepared_prover_store().lock().await;
        if let Some((cached_cfg, prepared)) = guard.as_ref() {
            if *cached_cfg == desired {
                tracing::info!(
                    backend = ?desired.backend,
                    "reusing prepared prover"
                );
                return Ok(prepared.clone());
            }
            tracing::info!(
                cached_backend = ?cached_cfg.backend,
                requested_backend = ?desired.backend,
                "prepared prover cache miss due to config change"
            );
        }
    }

    tracing::info!(
        backend = ?desired.backend,
        "building prepared prover"
    );

    let prover = match prover_backend {
        ProverBackend::Mock => {
            let prover = timed_async("build_mock_prover", || async {
                Ok::<_, BoxError>(ProverClient::builder().mock().build().await)
            })
            .await?;
            EnvProver::Mock(prover)
        },
        ProverBackend::Cpu => {
            let worker_config = crate::pipeline::input::worker_config_from_env();
            let prover = timed_async("build_cpu_prover", || async {
                Ok::<_, BoxError>(zkpow_worker::build_cpu_worker(&worker_config).await)
            })
            .await?;
            EnvProver::Cpu(prover)
        },
        ProverBackend::Cuda => {
            #[cfg(feature = "cuda")]
            {
                let prover = timed_async("build_cuda_prover", || async {
                    Ok::<_, BoxError>(ProverClient::builder().cuda().build().await)
                })
                .await?;
                EnvProver::Cuda(prover)
            }
            #[cfg(not(feature = "cuda"))]
            {
                return Err(
                    "CUDA prover backend requested, but zkpow-host was compiled without the `cuda` feature".into(),
                );
            }
        },
    };

    let proving_key = timed_async("setup_vkey", || async {
        prover
            .setup(ELF)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;

    let prepared = PreparedProver { prover, proving_key };

    let mut guard = prepared_prover_store().lock().await;
    *guard = Some((desired, prepared.clone()));
    Ok(prepared)
}
