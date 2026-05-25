//! SP1 memory stress test.
//!
//! Proves a minimal batch (1 header by default) in a loop to isolate whether
//! the memory ratchet is inside SP1's proving layer or in application code.
//!
//! Usage:
//!   cargo run --release -p zkpow-host --bin sp1_stress
//!
//! Environment:
//!   ITERATIONS              Number of prove loops (default: 5)
//!   NUM_HEADERS             Headers per batch (default: 1)
//!   FRESH_PROVER=1          Drop and rebuild CpuProver before each iteration.
//!                           Isolates whether leak is in prover state vs. SP1 internals.
//!   MEMORY_DIAGNOSTICS_DUMP=1  Write jemalloc JSON dumps per iteration
//!
//! Interpretation:
//!   - Cached prover (default): if RSS ratchets, leak is inside SP1 proving engine.
//!   - FRESH_PROVER=1: if ratchet stops, the prover holds state between iterations.
//!     If ratchet continues with fresh prover, leak is in SP1 internals (rayon thread
//!     locals, jemalloc fragmentation, or global caches outside the prover object).

#[cfg(feature = "memory-diagnostics")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use std::path::PathBuf;
use std::time::Duration;

use zkpow_host::memory_profiler;
use zkpow_host::observability;
use zkpow_host::proof_pipeline::{
    artifact_client_stats, clear_prepared_prover_cache, generate_and_save_proofs,
    ProofGenerationConfig, ProverBackend,
};

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
    // When FRESH_PROVER=1, drop and rebuild the CpuProver before each iteration.
    // This tests whether memory accumulation is inside the prover itself or elsewhere.
    let fresh_prover_each_iter =
        std::env::var("FRESH_PROVER").as_deref() == Ok("1");

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
        fresh_prover_each_iter,
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

        if fresh_prover_each_iter && i > 1 {
            tracing::info!(iteration = i, "FRESH_PROVER: dropping cached prover before iteration");
            clear_prepared_prover_cache();
            // Allow the allocator to return freed pages before we snapshot memory.
            memory_profiler::maybe_purge_allocator();
        }

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

        #[cfg(feature = "memory-diagnostics")]
        let mem_before_drop = memory_profiler::capture_snapshot();
        #[cfg(feature = "memory-diagnostics")]
        memory_profiler::log_snapshot(
            "before_artifacts_drop",
            &mem_before_drop,
            "Memory before dropping ProofArtifacts",
        );

        drop(artifacts);

        #[cfg(feature = "memory-diagnostics")]
        let mem_after_drop = memory_profiler::capture_snapshot();
        #[cfg(feature = "memory-diagnostics")]
        memory_profiler::log_snapshot_delta(
            "after_artifacts_drop",
            &mem_before_drop,
            &mem_after_drop,
            Duration::ZERO,
            "Memory after dropping ProofArtifacts",
        );

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
        let start_rss = start_mem.rss_kb;
        let end_rss = end_mem.rss_kb;
        let post_purge_rss = post_purge_mem.rss_kb;

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

        // Log artifact client stats to check for artifact leaks.
        if let Some((artifact_count, artifact_bytes)) = artifact_client_stats().await {
            tracing::info!(
                iteration = i,
                artifact_count,
                artifact_bytes,
                artifact_mb = artifact_bytes / (1024 * 1024),
                "Artifact client stats after proof"
            );
        }

        prev_proof_path = Some(compressed_path);
        tracing::info!(iteration = i, elapsed_secs = iter_elapsed.as_secs_f64(), "Iteration done");
    }

    tracing::info!("SP1 stress test complete");
    Ok(())
}
