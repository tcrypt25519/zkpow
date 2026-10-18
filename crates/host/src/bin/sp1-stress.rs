//! SP1 memory stress test.
//!
//! Proves a minimal batch (1 header by default) in a loop to isolate whether
//! the memory ratchet is inside SP1's proving layer or in application code.
//!
//! Usage:
//!   cargo run --release -p zkpow-host --bin sp1-stress
//!
//! Environment:
//!   ITERATIONS           Number of prove loops (default: 5)
//!   ZKPOW_BATCH_SIZE     Headers per batch (default: 1)
//!   ZKPOW_EXECUTE_ONLY   Skip proving (default: on for this stress test)
//!
//! If RSS ratchets upward across iterations with the prover cached, the leak
//! is inside SP1. If RSS is flat, the leak is in application-level state
//! retained between batches.

use std::path::PathBuf;

use zkpow_host::{
    memory_monitor, observability,
    pipeline::input::{ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_EXECUTE_ONLY},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    observability::init();

    let iterations: u32 = std::env::var("ITERATIONS").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let num_headers: u32 = std::env::var(ENV_ZKPOW_BATCH_SIZE)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let execute_only = std::env::var(ENV_ZKPOW_EXECUTE_ONLY)
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    std::fs::create_dir_all("logs").ok();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();
    let scratch = PathBuf::from(format!("profiling/sp1/stress/{timestamp}"));
    std::fs::create_dir_all(&scratch).ok();

    // The stress loop runs one batch per session iteration against a
    // throwaway run: artifacts under the scratch dir, in-process controller,
    // mock/CPU prover cached across iterations. The run's ledger row is
    // deleted again at the end.
    let run_id = format!("stress-{timestamp}");
    // SAFETY: single-threaded at this point; no other env readers yet.
    unsafe {
        std::env::set_var("ZKPOW_RUN_ID", &run_id);
        std::env::set_var("ZKPOW_PROOF_STORE_DIR", scratch.join("artifacts"));
        std::env::set_var("ZKPOW_BATCH_COUNT", "1");
        std::env::set_var(ENV_ZKPOW_BATCH_SIZE, num_headers.to_string());
        std::env::set_var(ENV_ZKPOW_EXECUTE_ONLY, if execute_only { "1" } else { "0" });
    }

    tracing::info!(iterations, num_headers, execute_only, "SP1 stress test starting");
    memory_monitor::log_point("stress_session_start", "Stress session memory snapshot");

    let mut baseline_rss_kb: Option<u64> = None;

    for i in 1..=iterations {
        // Sample directly: this binary's whole purpose is memory numbers,
        // which must be real even without the memory-diagnostics feature.
        let start_mem = memory_monitor::sample();

        let iter_start = std::time::Instant::now();
        let completed = zkpow_host::pipeline::run_batch_session::<zkpow_params::Mainnet>().await?;
        if completed != 1 {
            return Err(format!("stress iteration {i} completed {completed} batches").into());
        }
        let iter_elapsed = iter_start.elapsed();

        let end_mem = memory_monitor::sample();
        memory_monitor::log_delta(
            "stress_iter_end",
            start_mem,
            end_mem,
            iter_elapsed,
            "Stress iteration complete (artifacts dropped)",
        );

        // Compute ratchet against baseline and previous iteration.
        let start_rss = start_mem.rss_kb();
        let end_rss = end_mem.rss_kb();

        let ratchet_kb = end_rss as i64 - start_rss as i64;
        tracing::info!(
            iteration = i,
            start_rss_kb = start_rss,
            end_rss_kb = end_rss,
            ratchet_kb,
            "Iteration memory summary"
        );

        if i == 1 {
            baseline_rss_kb = Some(start_rss);
        }
        if let Some(baseline) = baseline_rss_kb {
            let cumulative_ratchet = end_rss as i64 - baseline as i64;
            tracing::info!(
                iteration = i,
                baseline_rss_kb = baseline,
                cumulative_ratchet_kb = cumulative_ratchet,
                "Cumulative ratchet since iteration 1 start"
            );
        }

        tracing::info!(iteration = i, elapsed_secs = iter_elapsed.as_secs_f64(), "Iteration done");
    }

    if memory_monitor::logging_enabled() {
        memory_monitor::log_point("stress_session_end", "Stress session memory snapshot");
    }

    zkpow_controller::HeaderStore::open(zkpow_controller::db_path())?.delete_run(&run_id)?;
    tracing::info!("SP1 stress test complete");
    Ok(())
}
