//! Shared single-batch proof runner used by both `zkpow-host` and `continuous-prover`.

use crate::proof_pipeline::{
    config_from_env, generate_and_save_proofs, log_execution_report, BoxError, ProofArtifacts,
};

fn log_batch_completion(
    config: &crate::proof_pipeline::ProofGenerationConfig,
    artifacts: &ProofArtifacts,
) {
    tracing::info!(
        "Complete: validated headers from height {} to {}",
        artifacts.first_new_height,
        artifacts.end_height,
    );
    if config.execute_only {
        tracing::info!(
            "Execution completed in {:.2} seconds",
            artifacts.total_duration_secs
        );
        return;
    }

    if let Some(compressed_path) = artifacts.compressed_path.as_ref() {
        tracing::info!("Saved compressed proof to {}", compressed_path.display());
    }
    if let Some(groth16_path) = artifacts.groth16_path.as_ref() {
        tracing::info!("Saved Groth16 proof to {}", groth16_path.display());
    }
    tracing::info!("========================================");
    tracing::info!(
        "TOTAL PROVING TIME: {:.2} seconds",
        artifacts.total_duration_secs
    );
    tracing::info!("========================================");
    if artifacts.phase_timings.is_empty() {
        tracing::info!("Proving time breakdown: unavailable");
    } else {
        tracing::info!("Proving time breakdown:");
        for phase in &artifacts.phase_timings {
            let pct = if artifacts.total_duration_secs > 0.0 {
                (phase.total_duration_secs * 100.0) / artifacts.total_duration_secs
            } else {
                0.0
            };
            tracing::info!(
                "  {}: {:.2}s ({:.2}%){}",
                phase.label,
                phase.total_duration_secs,
                pct,
                if phase.invocations > 1 {
                    format!(" across {} invocations", phase.invocations)
                } else {
                    String::new()
                }
            );
        }
    }
    log_execution_report(&artifacts.execution_report, artifacts.total_duration_secs);
}

/// Run one proof batch, reading all configuration from environment variables.
/// Returns the generated [`ProofArtifacts`] on success.
pub async fn run_single_batch() -> Result<ProofArtifacts, BoxError> {
    let config = config_from_env()?;
    run_single_batch_with_config(&config).await
}

/// Run one proof batch from an explicit configuration.
pub async fn run_single_batch_with_config(
    config: &crate::proof_pipeline::ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    let action = if config.execute_only {
        "Starting execution"
    } else {
        "Starting proof generation"
    };
    tracing::info!(
        "{} with backend {:?}{}",
        action,
        config.prover_backend,
        config
            .cuda_device_id
            .map(|id| format!(" (CUDA device {})", id))
            .unwrap_or_default(),
    );
    let artifacts = generate_and_save_proofs(config).await?;
    log_batch_completion(config, &artifacts);
    Ok(artifacts)
}
