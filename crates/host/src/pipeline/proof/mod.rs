pub(crate) mod compressed;
pub(crate) mod groth16;

use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;

#[cfg(feature = "CUDA")]
use sp1_sdk::CudaProver;
use sp1_sdk::{CpuProver, MockProver, Prover, ProverClient, SP1ProvingKey};

use crate::pipeline::diagnostics::timed_async;
#[cfg(feature = "CUDA")]
use crate::pipeline::diagnostics::timed_sync;
use crate::pipeline::ELF;
use crate::pipeline::{BoxError, ProofGenerationConfig, ProverBackend};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedProverConfig {
    backend: ProverBackend,
    cuda_device_id: Option<u32>,
}

#[derive(Clone)]
pub(crate) enum PreparedProver {
    Mock {
        prover: MockProver,
        proving_key: SP1ProvingKey,
    },
    Cpu {
        prover: CpuProver,
        proving_key: SP1ProvingKey,
    },
    #[cfg(feature = "CUDA")]
    Cuda {
        prover: CudaProver,
        proving_key: <CudaProver as Prover>::ProvingKey,
    },
}

static PREPARED_PROVER: OnceLock<AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>>> =
    OnceLock::new();

fn prepared_prover_store() -> &'static AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>> {
    PREPARED_PROVER.get_or_init(|| AsyncMutex::new(None))
}

pub(crate) async fn get_prepared_prover(
    config: &ProofGenerationConfig,
) -> Result<PreparedProver, BoxError> {
    let desired = PreparedProverConfig {
        backend: config.prover_backend,
        cuda_device_id: config.cuda_device_id,
    };

    {
        let guard = prepared_prover_store().lock().await;
        if let Some((cached_cfg, prepared)) = guard.as_ref() {
            if *cached_cfg == desired {
                tracing::info!(
                    backend = ?desired.backend,
                    cuda_device_id = desired.cuda_device_id,
                    "reusing prepared prover"
                );
                return Ok(prepared.clone());
            }
            tracing::info!(
                cached_backend = ?cached_cfg.backend,
                cached_cuda_device_id = cached_cfg.cuda_device_id,
                requested_backend = ?desired.backend,
                requested_cuda_device_id = desired.cuda_device_id,
                "prepared prover cache miss due to config change"
            );
        }
    }

    tracing::info!(
        backend = ?desired.backend,
        cuda_device_id = desired.cuda_device_id,
        "building prepared prover"
    );

    let prepared = match config.prover_backend {
        ProverBackend::Mock => {
            let prover = timed_async("build_mock_prover", || async {
                Ok::<_, BoxError>(ProverClient::builder().mock().build().await)
            })
            .await?;
            let proving_key =
                timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
            PreparedProver::Mock {
                prover,
                proving_key,
            }
        }
        ProverBackend::Cpu => {
            let prover = timed_async("build_cpu_prover", || async {
                Ok::<_, BoxError>(ProverClient::builder().cpu().build().await)
            })
            .await?;
            let proving_key =
                timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
            PreparedProver::Cpu {
                prover,
                proving_key,
            }
        }
        ProverBackend::Cuda => {
            #[cfg(feature = "CUDA")]
            {
                let report =
                    timed_sync("cuda_preflight", || crate::cuda_env::run_preflight(config))?;
                crate::cuda_env::log_preflight(&report);
                let prover = timed_async("build_cuda_prover", || async {
                    let device_id = config.cuda_device_id;
                    let handle = tokio::spawn(async move {
                        let builder = if let Some(device_id) = device_id {
                            ProverClient::builder().cuda().with_device_id(device_id)
                        } else {
                            ProverClient::builder().cuda()
                        };
                        builder.build().await
                    });
                    handle.await.map_err(|err| -> BoxError {
                        format!("failed to initialize CUDA prover task: {err}").into()
                    })
                })
                .await?;
                let proving_key =
                    timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
                PreparedProver::Cuda {
                    prover,
                    proving_key,
                }
            }

            #[cfg(not(feature = "CUDA"))]
            unreachable!("CUDA config should already be rejected when the CUDA feature is absent")
        }
    };

    let mut guard = prepared_prover_store().lock().await;
    *guard = Some((desired, prepared.clone()));
    Ok(prepared)
}
