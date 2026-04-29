//! zkpow — Host Script
//!
//! Usage:
//!   # Run 1: start from a host-selected genesis state
//!   cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Run 2: Extend from previous proof
//!   PREV_PROOF=proof_height_1_to_100.bin cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Optional: also emit a Groth16-wrapped proof
//!   GENERATE_GROTH16=1 cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Optional: enable CUDA proving (must compile with --features CUDA first)
//!   CUDA=1 cargo run --release -p zkpow-host --features CUDA --bin zkpow-host

use zkpow_host::observability;
use zkpow_host::proof_pipeline::{config_from_env, generate_and_save_proofs, log_execution_report};

#[tokio::main]
async fn main() {
    println!(
        "Host script started. For detailed tracing, set RUST_LOG=info (e.g., RUST_LOG=info cargo run)."
    );
    observability::init();

    let config = config_from_env().expect("invalid proof generation configuration");
    tracing::info!(
        "Starting proof generation with backend {:?}{}",
        config.prover_backend,
        config
            .cuda_device_id
            .map(|id| format!(" (CUDA device {})", id))
            .unwrap_or_default(),
    );
    let artifacts = match generate_and_save_proofs(&config).await {
        Ok(artifacts) => artifacts,
        Err(err) => {
            tracing::error!("proof generation pipeline failed: {err}");
            std::process::exit(1);
        }
    };

    tracing::info!(
        "Complete: validated headers from height {} to {}",
        artifacts.first_new_height,
        artifacts.end_height,
    );
    tracing::info!(
        "Saved compressed proof to {}",
        artifacts.compressed_path.display(),
    );
    if let Some(groth16_path) = artifacts.groth16_path.as_ref() {
        tracing::info!("Saved Groth16 proof to {}", groth16_path.display());
    } else {
        tracing::info!("Skipped Groth16 wrapping; set GENERATE_GROTH16=1 to emit it");
    }
    tracing::info!("========================================");
    tracing::info!(
        "TOTAL PROVING TIME: {:.2} seconds",
        artifacts.total_duration_secs,
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
