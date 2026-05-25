use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::batch_runner::run_single_batch;
use crate::memory_profiler;
use crate::proof_pipeline::clear_prepared_prover_cache;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

fn session_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_secs()
        .to_string()
}

fn effective_max_batches(default_max_batches: u32) -> u32 {
    env::var("MAX_BATCHES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default_max_batches)
}

pub async fn run_batch_session(default_max_batches: u32) -> Result<(), BoxError> {
    let timestamp = session_timestamp();

    let memory_profiling_enabled = env::var("MEMORY_PROFILING").as_deref() == Ok("1");
    let memory_diagnostics_dump_enabled =
        env::var("MEMORY_DIAGNOSTICS_DUMP").as_deref() == Ok("1");

    if memory_profiling_enabled || memory_diagnostics_dump_enabled {
        std::fs::create_dir_all("logs").ok();
    }

    if memory_profiling_enabled {
        std::fs::create_dir_all("logs").ok();
        let log_path = PathBuf::from(format!("logs/mem_{}.log", timestamp));
        memory_profiler::spawn_mem_logger(log_path, Duration::from_secs(1));
        tracing::info!(
            "Memory profiling enabled; writing to logs/mem_{}.log",
            timestamp
        );
    }

    if memory_diagnostics_dump_enabled {
        tracing::info!(
            "Memory diagnostics dumps enabled; writing jemalloc snapshots to logs/"
        );
        memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
            "logs/jemalloc_{}_session_start.json",
            timestamp
        )));
    }

    let max_batches = effective_max_batches(default_max_batches);
    let explicit_output_dir = env::var("OUTPUT_DIR").ok().map(PathBuf::from);
    let continuous_dir = PathBuf::from(format!("profiling/sp1/continuous/{}", timestamp));
    if max_batches == 1 {
        if let Some(output_dir) = &explicit_output_dir {
            std::fs::create_dir_all(output_dir).expect("failed to create output dir");
            tracing::info!(
                "Single-batch session started; output dir {}",
                output_dir.display()
            );
        } else {
            std::fs::create_dir_all(&continuous_dir)
                .expect("failed to create continuous profiling dir");
            tracing::info!(
                "Continuous profiling session started; outputs in {}",
                continuous_dir.display()
            );
        }
    } else {
        std::fs::create_dir_all(&continuous_dir)
            .expect("failed to create continuous profiling dir");
        tracing::info!(
            "Continuous profiling session started; outputs in {}",
            continuous_dir.display()
        );
    }

    let mut current_prev_proof: Option<PathBuf> = env::var("PREV_PROOF").ok().map(PathBuf::from);
    let mut batch_count: u32 = 0;
    let prover_rebuild_every: u32 = env::var("PROVER_REBUILD_EVERY_N_BATCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if prover_rebuild_every > 0 {
        tracing::info!(
            prover_rebuild_every,
            "Periodic prover rebuild enabled; clears cache every N batches"
        );
    }

    loop {
        if batch_count >= max_batches {
            tracing::info!("Reached MAX_BATCHES={max_batches}; stopping continuous prover");
            break;
        }

        batch_count += 1;
        let output_dir = if max_batches == 1 {
            explicit_output_dir
                .clone()
                .unwrap_or_else(|| continuous_dir.join(format!("batch_{batch_count}/proofs")))
        } else {
            continuous_dir.join(format!("batch_{batch_count}/proofs"))
        };
        std::fs::create_dir_all(&output_dir).expect("failed to create batch output dir");

        env::set_var("OUTPUT_DIR", &output_dir);
        match current_prev_proof.as_deref() {
            Some(path) => env::set_var("PREV_PROOF", path),
            None => env::remove_var("PREV_PROOF"),
        }

        tracing::info!("=== Starting Batch {} ===", batch_count);
        tracing::info!("  output dir: {}", output_dir.display());
        if let Some(prev) = &current_prev_proof {
            tracing::info!("  extending from: {}", prev.display());
        }

        let start_memory = memory_profiler::capture_snapshot();
        memory_profiler::log_snapshot(
            "batch_memory_start",
            &start_memory,
            "Batch memory snapshot before proof generation",
        );
        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{}_batch_{:04}_start.json",
                timestamp, batch_count
            )));
        }
        let batch_started = std::time::Instant::now();

        let artifacts = run_single_batch().await?;
        let compressed_path = artifacts.compressed_path.clone();
        let first_new_height = artifacts.first_new_height;
        let end_height = artifacts.end_height;
        let batch_elapsed_secs = batch_started.elapsed().as_secs_f64();
        drop(artifacts);

        // Aggressive allocator purge: attempt to return freed pages to the OS.
        // Only effective when built with --features memory-diagnostics (jemalloc).
        memory_profiler::maybe_purge_allocator();

        let end_memory = memory_profiler::capture_snapshot();
        current_prev_proof = Some(compressed_path.clone());
        tracing::info!(
            "=== Batch {} complete. Next proof: {} ===",
            batch_count,
            compressed_path.display()
        );
        tracing::info!(
            batch = batch_count,
            first_new_height,
            end_height,
            elapsed_secs = batch_elapsed_secs,
            "Batch memory summary after dropping proof artifacts"
        );
        memory_profiler::log_snapshot_delta(
            "batch_memory_after_drop",
            &start_memory,
            &end_memory,
            batch_started.elapsed(),
            "Batch memory summary after dropping proof artifacts",
        );
        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{}_batch_{:04}_after_drop.json",
                timestamp, batch_count
            )));
        }

        // Periodic prover rebuild: if configured, clear the cached prover every N batches
        // so the next batch rebuilds it from scratch. This resets all SP1 internal state
        // (artifact client, channels, etc.) and is the strongest in-process mitigation
        // for memory leaks inside SP1's proving layer.
        if prover_rebuild_every > 0 && batch_count % prover_rebuild_every == 0 && batch_count < max_batches {
            tracing::info!(
                batch = batch_count,
                prover_rebuild_every,
                "Periodic prover rebuild triggered; clearing prepared prover cache"
            );
            clear_prepared_prover_cache();
        }
    }

    Ok(())
}
