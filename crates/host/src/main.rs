//! zkpow — Host Script
//!
//! Proves Bitcoin header batches and saves the result.
//!
//! Usage:
//!   # First run: start from genesis
//!   cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Subsequent runs: extend from previous proof
//!   PREV_PROOF=proof_height_1_to_100.bin cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Also emit a Groth16-wrapped proof
//!   GENERATE_GROTH16=1 cargo run --release -p zkpow-host --bin zkpow-host
//!
//!   # Use the CUDA prover (must compile with --features CUDA)
//!   CUDA=1 cargo run --release -p zkpow-host --features CUDA --bin zkpow-host
//!
//!   # Run multiple batches in one process
//!   MAX_BATCHES=10 cargo run --release -p zkpow-host --bin zkpow-host

use zkpow_host::observability;
use zkpow_host::session_runner::run_batch_session;

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() {
    println!(
        "Host script started. For detailed tracing, set RUST_LOG=info."
    );
    observability::init();

    if let Err(e) = run_batch_session(1).await {
        tracing::error!("proof generation pipeline failed: {e}");
        std::process::exit(1);
    }
}
