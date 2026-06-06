//! SP1 memory stress test.
//!
//! Proves a minimal batch (1 header by default) in a loop to isolate whether
//! the memory ratchet is inside SP1's proving layer or in application code.
//!
//! Usage:
//!   cargo run --release -p zkpow-host --bin sp1_stress
//!
//! Environment:
//!   ITERATIONS      Number of prove loops (default: 5)
//!   ZKPOW_BATCH_SIZE     Headers per batch (default: 1)
//!
//! If RSS ratchets upward across iterations with the prover cached, the leak
//! is inside SP1. If RSS is flat, the leak is in application-level state
//! retained between batches.

use std::path::PathBuf;

#[cfg(feature = "memory-diagnostics")]
use memory_usage::TrackingAllocator;
use memory_usage::{StageHistory, StageMetric};
#[cfg(feature = "memory-diagnostics")]
use std::alloc::System;
use zkpow_host::config::db_path;
use zkpow_host::memory_monitor;
use zkpow_host::observability;
use zkpow_host::pipeline::input::{
    ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_DB_PATH, ENV_ZKPOW_EXECUTE_ONLY,
};
use zkpow_host::pipeline::{generate_and_save_proofs, ProofGenerationConfig, ProverBackend};

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
static ALLOCATOR: TrackingAllocator<'static, System> =
    TrackingAllocator::new(System, &memory_monitor::ALLOCATION_TRACKER);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    observability::init();

    let iterations: u32 = std::env::var("ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let num_headers: u32 = std::env::var(ENV_ZKPOW_BATCH_SIZE)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    std::fs::create_dir_all("logs").ok();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let execute_only = std::env::var(ENV_ZKPOW_EXECUTE_ONLY)
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    tracing::info!(
        iterations,
        num_headers,
        execute_only,
        "SP1 stress test starting"
    );
    memory_monitor::log_point("stress_session_start", "Stress session memory snapshot");

    let mut prev_proof_path: Option<PathBuf> = None;
    let mut baseline_rss_kb: Option<u64> = None;
    let mut history = StageHistory::new([
        "Stage 0 (Start)",
        "Stage 1 (Before prove)",
        "Stage 2 (Drop)",
    ]);

    for i in 1..=iterations {
        let config = ProofGenerationConfig {
            prev_proof_path: prev_proof_path.clone(),
            trusted_start_height: None,
            num_headers,
            db_path: std::env::var(ENV_ZKPOW_DB_PATH)
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(db_path())),
            output_dir: PathBuf::from(format!("profiling/sp1/stress/{}/iter_{}", timestamp, i)),
            batch_count: 1,
            generate_groth16: false,
            execute_only,
            prover_backend: if execute_only {
                ProverBackend::Mock
            } else {
                ProverBackend::Cpu
            },
            cuda_device_id: None,
        };

        std::fs::create_dir_all(&config.output_dir).ok();

        let start_mem = memory_monitor::log_point("stress_iter_start", "Stress iteration start");

        let iter_start = std::time::Instant::now();
        let artifacts = generate_and_save_proofs(&config).await?;
        let compressed_path = artifacts.compressed_path.clone();
        if !execute_only && compressed_path.is_none() {
            return Err("sp1_stress expected compressed proof but got none".into());
        }
        let before_prove_mem = artifacts.before_prove_sample;
        drop(artifacts);
        let iter_elapsed = iter_start.elapsed();

        let end_mem = memory_monitor::log_point("stress_iter_end", "Stress iteration complete");
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

        history.push_iteration([start_mem, before_prove_mem, end_mem])?;
        prev_proof_path = compressed_path;
        tracing::info!(
            iteration = i,
            elapsed_secs = iter_elapsed.as_secs_f64(),
            "Iteration done"
        );
    }

    if memory_monitor::logging_enabled() {
        println!(
            "\n{}",
            history.render_table(StageMetric::RssKb, "SP1 STRESS RSS MATRIX (KB)")
        );
        println!(
            "\n{}",
            history.render_table(StageMetric::LiveKb, "SP1 STRESS LIVE MATRIX (KB)")
        );

        memory_monitor::log_point("stress_session_end", "Stress session memory snapshot");
    }
    tracing::info!("SP1 stress test complete");
    Ok(())
}
