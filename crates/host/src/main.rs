//! Bitcoin Header Chain Prover — Host Script
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

use zkpow_host::observability;
use zkpow_host::proof_pipeline::{config_from_env, generate_and_save_proofs};

#[tokio::main]
async fn main() {
    println!(
        "Host script started. For detailed tracing, set RUST_LOG=info (e.g., RUST_LOG=info cargo run)."
    );
    observability::init();

    let config = config_from_env().expect("invalid proof generation configuration");
    let artifacts = generate_and_save_proofs(&config)
        .await
        .expect("proof generation pipeline failed");

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
    tracing::info!(
        "Total proving time: {:.2} seconds",
        artifacts.total_duration_secs,
    );
}
