use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::batch_runner::run_single_batch_with_config;
use crate::memory_monitor;
use crate::pipeline::batch::NO_HEADERS_REMAINING_PREFIX;
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

fn is_header_exhaustion_error(err: &BoxError) -> bool {
    err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX)
}

pub async fn run_batch_session() -> Result<u32, BoxError> {
    let timestamp = session_timestamp();
    let mut memory_history = StageHistory::new(["Batch start", "Before work", "Batch end"]);
    memory_monitor::log_point(
        "session_memory_start",
        "Session memory snapshot before batches",
    );

    let mut batch_config = config_from_env()?;
    let max_batches = batch_config.batch_count;
    let out_dir = if batch_config.output_dir == std::path::Path::new(".") {
        PathBuf::from(format!("profiling/sp1/continuous/{}", timestamp))
    } else {
        batch_config.output_dir.clone()
    };

    tracing::info!(
        "Run started for {} batches of {} headers each",
        max_batches,
        batch_config.num_headers
    );
    tracing::info!("Outputs will be written to: {}", out_dir.display());

    let mut current_prev_proof = batch_config.prev_proof_path.clone();
    let mut trusted_start_height = None;
    let mut batch_count: u32 = 0;

    loop {
        if batch_count >= max_batches {
            tracing::info!("Reached ZKPOW_BATCH_COUNT={max_batches}; stopping continuous prover");
            break;
        }

        batch_count += 1;

        std::fs::create_dir_all(&out_dir).expect("failed to create batch output dir");

        batch_config.output_dir = out_dir.clone();
        batch_config.prev_proof_path = current_prev_proof.clone();
        batch_config.trusted_start_height = trusted_start_height;

        tracing::info!("=== Starting Batch {} ===", batch_count);
        tracing::info!("  output dir: {}", out_dir.display());
        if let Some(prev) = &current_prev_proof {
            tracing::info!("  extending from: {}", prev.display());
        }

        let start_memory = memory_monitor::log_point(
            "batch_memory_start",
            "Batch memory snapshot before batch work",
        );
        let batch_started = std::time::Instant::now();

        let artifacts = match run_single_batch_with_config(&batch_config).await {
            Ok(artifacts) => artifacts,
            Err(err) if is_header_exhaustion_error(&err) => {
                tracing::info!(
                    "No remaining headers in database; stopping continuous prover after {} batches",
                    batch_count - 1
                );
                batch_count -= 1;
                break;
            }
            Err(err) => return Err(err),
        };
        let compressed_path = artifacts.compressed_path.clone();
        let first_new_height = artifacts.first_new_height;
        let end_height = artifacts.end_height;
        let before_prove_memory = artifacts.before_prove_sample;
        let batch_elapsed_secs = batch_started.elapsed().as_secs_f64();
        drop(artifacts);

        let end_memory = memory_monitor::log_point(
            "batch_memory_after_drop",
            "Batch memory snapshot after dropping batch artifacts",
        );
        if batch_config.execute_only {
            current_prev_proof = None;
            trusted_start_height = Some(end_height);
        } else {
            current_prev_proof = compressed_path.clone();
            trusted_start_height = None;
        }
        if let Some(path) = compressed_path.as_ref() {
            tracing::info!(
                "=== Batch {} complete. Next proof: {} ===",
                batch_count,
                path.display()
            );
        }
        if memory_monitor::logging_enabled() {
            tracing::info!(
                batch = batch_count,
                first_new_height,
                end_height,
                elapsed_secs = batch_elapsed_secs,
                "Batch memory summary after dropping batch artifacts"
            );
            memory_monitor::log_delta(
                "batch_memory_after_drop",
                start_memory,
                end_memory,
                batch_started.elapsed(),
                "Batch memory summary after dropping batch artifacts",
            );
        }
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

    Ok(batch_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    fn env_test_mutex() -> &'static Mutex<()> {
        static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_MUTEX.get_or_init(|| Mutex::new(()))
    }

    fn set_env(name: &str, value: &str) {
        // SAFETY: tests hold a global mutex to serialize process-environment mutation.
        unsafe { std::env::set_var(name, value) };
    }

    fn remove_env(name: &str) {
        // SAFETY: tests hold a global mutex to serialize process-environment mutation.
        unsafe { std::env::remove_var(name) };
    }

    #[test]
    fn classifies_header_exhaustion_errors_by_prefix() {
        let cases = [
            (
                format!(
                    "{NO_HEADERS_REMAINING_PREFIX}: starting at height {}",
                    123u32
                ),
                true,
            ),
            ("some other error".to_string(), false),
        ];

        for (message, expected) in cases {
            let err: BoxError = message.clone().into();
            assert_eq!(
                is_header_exhaustion_error(&err),
                expected,
                "message={message:?}"
            );
        }
    }

    #[tokio::test]
    async fn stops_after_reaching_configured_batch_count() {
        let _guard = env_test_mutex().lock().await;

        set_env("ZKPOW_BATCH_COUNT", "0");
        set_env("ZKPOW_BATCH_SIZE", "1");
        set_env("ZKPOW_EXECUTE_ONLY", "1");

        let result = run_batch_session().await;

        remove_env("ZKPOW_BATCH_COUNT");
        remove_env("ZKPOW_BATCH_SIZE");
        remove_env("ZKPOW_EXECUTE_ONLY");

        let completed_batches = result.expect("session should exit cleanly");
        assert_eq!(completed_batches, 0);
    }

    #[tokio::test]
    async fn stops_after_exhausting_headers_in_database() {
        let _guard = env_test_mutex().lock().await;

        set_env("ZKPOW_BATCH_COUNT", "5");
        set_env("ZKPOW_BATCH_SIZE", "0");
        set_env("ZKPOW_EXECUTE_ONLY", "1");
        set_env(
            "ZKPOW_DB_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db"),
        );

        let result = run_batch_session().await;

        remove_env("ZKPOW_BATCH_COUNT");
        remove_env("ZKPOW_BATCH_SIZE");
        remove_env("ZKPOW_EXECUTE_ONLY");
        remove_env("ZKPOW_DB_PATH");

        let completed_batches = result.expect("session should stop on exhaustion without error");
        assert_eq!(completed_batches, 0);
    }
}
