//! Construction and instrumented job dispatch for the SP1 CPU proving worker.
//!
//! `sp1-prover`'s CPU worker caps the number of shards proven concurrently
//! at a hardcoded value and gives no per-job visibility beyond stage-level
//! spans. This crate owns that configuration surface ([`WorkerConfig`]) and
//! wraps every execute/prove call the host makes in a `tracing` span
//! ([`execute`], [`prove_compressed`]) tagged with a job id, input shape, and
//! completion/failure events. The concurrency cap itself is read directly
//! from `SHARD_PROVER_PERMITS` inside `sp1_prover::worker::cpu_worker_builder_with_machine`.

mod config;
mod jobs;

#[cfg(test)]
mod tests;

pub use config::{WorkerConfig, SHARD_PROVER_PERMITS_ENV};
pub use jobs::{execute, prove_compressed};
use sp1_sdk::{CpuProver, ProverClient};

/// Build the CPU proving worker, logging the resolved [`WorkerConfig`].
///
/// The permit count itself is applied by `sp1-prover` when it builds the
/// underlying worker (it reads [`SHARD_PROVER_PERMITS_ENV`] directly); this
/// just surfaces the value the caller should expect in this process's logs.
pub async fn build_cpu_worker(config: &WorkerConfig) -> CpuProver {
    tracing::info!(
        shard_prover_permits = config.shard_prover_permits,
        "building CPU proving worker"
    );
    ProverClient::builder().cpu().build().await
}
