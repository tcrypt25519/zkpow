//! zkpow — Host Script
//!
//! Stateless prover driver: pulls batches from the controller, runs the
//! guest, uploads proof artifacts, and reports completions. The controller
//! owns resume state per ZKPOW_RUN_ID, so re-running the same command
//! continues the run where it left off.
//!
//! Usage:
//!   # Controller runs in-process against the local headers.db
//!   cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Name the IVC chain (default: "dev")
//!   ZKPOW_RUN_ID=mainnet-01 cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Run multiple batches in one process
//!   ZKPOW_BATCH_COUNT=10 cargo run --release -p zkpow-host --bin zkpow-host

use zkpow_host::{observability, pipeline::run_batch_session};
use zkpow_params::Mainnet;

#[tokio::main]
async fn main() {
    println!("Host script started. For detailed tracing, set RUST_LOG=info.");
    observability::init();

    if let Err(e) = run_batch_session::<Mainnet>().await {
        tracing::error!("proof generation pipeline failed: {e}");
        std::process::exit(1);
    }
}
