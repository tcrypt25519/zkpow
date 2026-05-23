//! continuous-prover binary
//!
//! Replaces scripts/prove-chain.sh + scripts/prove-batch.sh.
//!
//! Runs proof batches in a loop, each extending the previous one, writing
//! outputs into timestamped batch directories under
//!   profiling/sp1/continuous/<timestamp>/batch_<N>/
//!
//! Environment variables match `zkpow-host`, but this binary defaults
//! `MAX_BATCHES` to 1 unless explicitly overridden.

use zkpow_host::observability;
use zkpow_host::session_runner::run_batch_session;

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() {
    observability::init();
    if let Err(err) = run_batch_session(1).await {
        tracing::error!("continuous prover failed: {err}");
        std::process::exit(1);
    }
}
