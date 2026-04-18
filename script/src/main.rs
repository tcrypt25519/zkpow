//! Bitcoin Header Chain Prover — Host Script
//!
//! Usage:
//!   # Run 1: start from a host-selected genesis state
//!   cargo run --release --bin bitcoin-header-chain-script
//!
//!   # Run 2: Extend from previous proof
//!   PREV_PROOF=proof_height_1_to_100.bin cargo run --release --bin bitcoin-header-chain-script

use bitcoin_header_chain_script::proof_pipeline::{config_from_env, generate_and_save_proofs};

#[tokio::main]
async fn main() {
    println!("Host script started. For detailed tracing, set RUST_LOG=info (e.g., RUST_LOG=info cargo run).");
    sp1_sdk::utils::setup_logger();

    let config = config_from_env();
    let artifacts = generate_and_save_proofs(&config)
        .await
        .expect("proof generation pipeline failed");

    tracing::info!(
        "Complete: validated headers from height {} to {}",
        artifacts.first_new_height,
        artifacts.end_height,
    );
    tracing::info!(
        "Saved compressed proof to {} and Groth16 proof to {}",
        artifacts.compressed_path.display(),
        artifacts.groth16_path.display(),
    );
}
