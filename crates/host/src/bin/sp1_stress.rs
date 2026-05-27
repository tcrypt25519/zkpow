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
//!   NUM_HEADERS     Headers per batch (default: 1)
//!   MEMORY_DIAGNOSTICS_DUMP=1  Write jemalloc JSON dumps per iteration
//!
//! If RSS ratchets upward across iterations with the prover cached, the leak
//! is inside SP1 (or jemalloc fragmentation). If RSS is flat, the leak is in
//! application-level state retained between batches.

use std::path::PathBuf;
use std::time::Duration;

use zkpow_host::memory_profiler;
use zkpow_host::observability;
use zkpow_host::proof_pipeline::{generate_and_save_proofs, ProofGenerationConfig, ProverBackend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    observability::init();

    let iterations: u32 = std::env::var("ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let num_headers: u32 = std::env::var("NUM_HEADERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let memory_diagnostics_dump_enabled =
        std::env::var("MEMORY_DIAGNOSTICS_DUMP").as_deref() == Ok("1");

    std::fs::create_dir_all("logs").ok();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    if memory_diagnostics_dump_enabled {
        memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
            "logs/jemalloc_{}_stress_start.json",
            timestamp
        )));
    }

    tracing::info!(
        iterations,
        num_headers,
        "SP1 stress test starting"
    );

    let mut prev_proof_path: Option<PathBuf> = None;
    let mut baseline_rss_kb: Option<u64> = None;

    for i in 1..=iterations {
        let config = ProofGenerationConfig {
            prev_proof_path: prev_proof_path.clone(),
            num_headers,
            db_path: std::env::var("DB_PATH")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../headers.db"
                    ))
                }),
            output_dir: PathBuf::from(format!("profiling/sp1/stress/{}/iter_{}", timestamp, i)),
            generate_groth16: false,
            prover_backend: ProverBackend::Cpu,
            cuda_device_id: None,
        };

        std::fs::create_dir_all(&config.output_dir).ok();

        let start_mem = memory_profiler::capture_snapshot();
        memory_profiler::log_snapshot("stress_iter_start", &start_mem, "Stress iteration start");

        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{}_stress_iter_{:04}_start.json",
                timestamp, i
            )));
        }

        let iter_start = std::time::Instant::now();
        let artifacts = generate_and_save_proofs(&config).await?;
        let compressed_path = artifacts.compressed_path.clone();
        drop(artifacts);
        let iter_elapsed = iter_start.elapsed();

        let end_mem = memory_profiler::capture_snapshot();
        memory_profiler::log_snapshot_delta(
            "stress_iter_end",
            &start_mem,
            &end_mem,
            iter_elapsed,
            "Stress iteration complete (artifacts dropped)",
        );

        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{}_stress_iter_{:04}_end.json",
                timestamp, i
            )));
        }

        // Try to force allocator to return pages to OS.
        memory_profiler::maybe_purge_allocator();

        let post_purge_mem = memory_profiler::capture_snapshot();
        memory_profiler::log_snapshot_delta(
            "stress_iter_post_purge",
            &end_mem,
            &post_purge_mem,
            Duration::ZERO,
            "After allocator purge",
        );

        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{}_stress_iter_{:04}_post_purge.json",
                timestamp, i
            )));
        }

        // Compute ratchet against baseline and previous iteration.
        let start_rss = start_mem.rss_kb();
        let end_rss = end_mem.rss_kb();
        let post_purge_rss = post_purge_mem.rss_kb();

        if let (Some(start), Some(end)) = (start_rss, end_rss) {
            let ratchet_kb = end as i64 - start as i64;
            tracing::info!(
                iteration = i,
                start_rss_kb = start,
                end_rss_kb = end,
                post_purge_rss_kb = post_purge_rss,
                ratchet_kb,
                "Iteration memory summary"
            );

            if i == 1 {
                baseline_rss_kb = Some(start);
            }
            if let Some(baseline) = baseline_rss_kb {
                let cumulative_ratchet = end as i64 - baseline as i64;
                tracing::info!(
                    iteration = i,
                    baseline_rss_kb = baseline,
                    cumulative_ratchet_kb = cumulative_ratchet,
                    "Cumulative ratchet since iteration 1 start"
                );
            }
        }

        prev_proof_path = Some(compressed_path);
        tracing::info!(iteration = i, elapsed_secs = iter_elapsed.as_secs_f64(), "Iteration done");
    }

    tracing::info!("SP1 stress test complete");
    Ok(())
}
