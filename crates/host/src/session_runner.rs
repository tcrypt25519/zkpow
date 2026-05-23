use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::memory_profiler;
use crate::proof_pipeline::{
    config_from_env, generate_and_save_proofs, log_execution_report, BoxError,
};

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

fn log_artifacts(artifacts: &crate::proof_pipeline::ProofArtifacts) {
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
    tracing::info!("========================================");
    tracing::info!(
        "TOTAL PROVING TIME: {:.2} seconds",
        artifacts.total_duration_secs,
    );
    tracing::info!("========================================");
    if artifacts.phase_timings.is_empty() {
        tracing::info!("Proving time breakdown: unavailable");
    } else {
        tracing::info!("Proving time breakdown:");
        for phase in &artifacts.phase_timings {
            let pct = if artifacts.total_duration_secs > 0.0 {
                (phase.total_duration_secs * 100.0) / artifacts.total_duration_secs
            } else {
                0.0
            };
            tracing::info!(
                "  {}: {:.2}s ({:.2}%){}",
                phase.label,
                phase.total_duration_secs,
                pct,
                if phase.invocations > 1 {
                    format!(" across {} invocations", phase.invocations)
                } else {
                    String::new()
                }
            );
        }
    }
    log_execution_report(&artifacts.execution_report, artifacts.total_duration_secs);
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
        let log_path = PathBuf::from(format!("logs/mem_{timestamp}.log"));
        memory_profiler::spawn_mem_logger(log_path, Duration::from_secs(1));
        tracing::info!("Memory profiling enabled; writing to logs/mem_{timestamp}.log");
    }

    if memory_diagnostics_dump_enabled {
        tracing::info!("Memory diagnostics dumps enabled; writing jemalloc snapshots to logs/");
        memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
            "logs/jemalloc_{timestamp}_session_start.json"
        )));
    }

    let max_batches = effective_max_batches(default_max_batches);
    let explicit_output_dir = env::var("OUTPUT_DIR").ok().map(PathBuf::from);
    let continuous_dir = PathBuf::from(format!("profiling/sp1/continuous/{timestamp}"));

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

        let config = config_from_env().expect("invalid proof generation configuration");
        tracing::info!(
            "=== Starting Batch {} ===",
            batch_count
        );
        tracing::info!("  output dir: {}", output_dir.display());
        if let Some(prev) = &current_prev_proof {
            tracing::info!("  extending from: {}", prev.display());
        }
        tracing::info!(
            "Starting proof generation with backend {:?}{}",
            config.prover_backend,
            config
                .cuda_device_id
                .map(|id| format!(" (CUDA device {})", id))
                .unwrap_or_default(),
        );

        let start_memory = memory_profiler::capture_snapshot();
        memory_profiler::log_snapshot(
            "batch_memory_start",
            &start_memory,
            "Batch memory snapshot before proof generation",
        );
        if memory_diagnostics_dump_enabled {
            memory_profiler::maybe_dump_allocator_stats(&PathBuf::from(format!(
                "logs/jemalloc_{timestamp}_batch_{batch_count:04}_start.json"
            )));
        }
        let batch_started = std::time::Instant::now();

        let artifacts = generate_and_save_proofs(&config).await?;
        let end_memory = memory_profiler::capture_snapshot();
        let compressed_path = artifacts.compressed_path.clone();

        log_artifacts(&artifacts);
        tracing::info!(
            batch = batch_count,
            first_new_height = artifacts.first_new_height,
            end_height = artifacts.end_height,
            elapsed_secs = batch_started.elapsed().as_secs_f64(),
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
                "logs/jemalloc_{timestamp}_batch_{batch_count:04}_after_drop.json"
            )));
        }

        current_prev_proof = Some(compressed_path);
    }

    Ok(())
}
