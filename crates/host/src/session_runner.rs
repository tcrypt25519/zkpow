use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::batch_runner::run_single_batch_with_config;
use crate::memory_monitor;
use crate::proof_pipeline::config_from_env;
use memory_usage::{StageHistory, StageMetric};

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
    let mut memory_history = StageHistory::new(["Batch start", "Before prove", "Batch end"]);
    memory_monitor::log_point(
        "session_memory_start",
        "Session memory snapshot before batches",
    );

    let max_batches = effective_max_batches(default_max_batches);
    let mut batch_config = config_from_env()?;
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

    let mut current_prev_proof = batch_config.prev_proof_path.clone();
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

        batch_config.output_dir = output_dir.clone();
        batch_config.prev_proof_path = current_prev_proof.clone();

        tracing::info!("=== Starting Batch {} ===", batch_count);
        tracing::info!("  output dir: {}", output_dir.display());
        if let Some(prev) = &current_prev_proof {
            tracing::info!("  extending from: {}", prev.display());
        }

        let start_memory = memory_monitor::log_point(
            "batch_memory_start",
            "Batch memory snapshot before proof generation",
        );
        let batch_started = std::time::Instant::now();

        let artifacts = run_single_batch_with_config(&batch_config).await?;
        let compressed_path = artifacts.compressed_path.clone();
        let first_new_height = artifacts.first_new_height;
        let end_height = artifacts.end_height;
        let before_prove_memory = artifacts.before_prove_sample;
        let batch_elapsed_secs = batch_started.elapsed().as_secs_f64();
        drop(artifacts);

        let end_memory = memory_monitor::log_point(
            "batch_memory_after_drop",
            "Batch memory snapshot after dropping proof artifacts",
        );
        current_prev_proof = compressed_path.clone();
        if let Some(path) = compressed_path.as_ref() {
            tracing::info!(
                "=== Batch {} complete. Next proof: {} ===",
                batch_count,
                path.display()
            );
        } else {
            tracing::info!(
                "=== Batch {} complete. Execute-only mode produced no chained proof ===",
                batch_count
            );
        }
        tracing::info!(
            batch = batch_count,
            first_new_height,
            end_height,
            elapsed_secs = batch_elapsed_secs,
            "Batch memory summary after dropping proof artifacts"
        );
        memory_monitor::log_delta(
            "batch_memory_after_drop",
            start_memory,
            end_memory,
            batch_started.elapsed(),
            "Batch memory summary after dropping proof artifacts",
        );
        memory_history.push_iteration([start_memory, before_prove_memory, end_memory])?;
    }

    if memory_monitor::logging_enabled() {
        println!(
            "\n{}",
            memory_history.render_table(StageMetric::RssKb, "BATCH RSS MATRIX (KB)")
        );
        println!(
            "\n{}",
            memory_history.render_table(StageMetric::LiveKb, "BATCH LIVE HEAP MATRIX (KB)")
        );
        memory_monitor::log_point(
            "session_memory_end",
            "Session memory snapshot after batches",
        );
    }

    Ok(())
}
