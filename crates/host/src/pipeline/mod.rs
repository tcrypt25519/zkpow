pub(crate) mod batch;
pub(crate) mod diagnostics;
pub(crate) mod execution;
pub mod input;
pub(crate) mod proof;

use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use memory_usage::StageSample;
use memory_usage::{StageHistory, StageMetric};
use sp1_sdk::prelude::*;
use sp1_sdk::ProverClient;
use sp1_sdk::{ExecutionReport, SP1ProofWithPublicValues};

use crate::memory_monitor;
use batch::{prepare_batch, NO_HEADERS_REMAINING_PREFIX};
use diagnostics::{clear_phase_timings, collected_phase_timings, timed_async, timed_sync};
use execution::{execute_batch_with_prover, execute_batch_without_proof, UnprovenBatchInput};
use input::{config_from_env, parse_genesis_hash};
use proof::compressed::generate_compressed_proof;
use proof::groth16::generate_groth16_proof;
use proof::{get_prepared_prover, PreparedProver};

use crate::util::{DbConfig, DbConn};

pub use diagnostics::log_execution_report;
pub use diagnostics::PhaseTiming;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub const ELF: Elf = include_elf!("zkpow-guest");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverBackend {
    Mock,
    Cpu,
    Cuda,
}

#[derive(Debug, Clone)]
pub struct ProofGenerationConfig {
    pub prev_proof_path: Option<PathBuf>,
    pub trusted_start_height: Option<u32>,
    pub num_headers: u32,
    pub batch_count: u32,
    pub db_path: PathBuf,
    pub output_dir: PathBuf,
    pub generate_groth16: bool,
    pub execute_only: bool,
    pub prover_backend: ProverBackend,
    pub cuda_device_id: Option<u32>,
}

#[derive(Debug)]
pub struct ProofArtifacts {
    pub compressed_path: Option<PathBuf>,
    pub groth16_path: Option<PathBuf>,
    pub compressed_proof: Option<SP1ProofWithPublicValues>,
    pub groth16_proof: Option<SP1ProofWithPublicValues>,
    pub before_prove_sample: StageSample,
    pub execution_report: ExecutionReport,
    pub first_new_height: u32,
    pub end_height: u32,
    pub total_duration_secs: f64,
    pub phase_timings: Vec<PhaseTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    ExecuteOnly,
    ProveCompressed,
    ProveCompressedAndGroth16,
}

impl ProofGenerationConfig {
    pub fn mode(&self) -> PipelineMode {
        if self.execute_only {
            PipelineMode::ExecuteOnly
        } else if self.generate_groth16 {
            PipelineMode::ProveCompressedAndGroth16
        } else {
            PipelineMode::ProveCompressed
        }
    }
}

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
    let db = DbConfig::new(&batch_config.db_path)
        .connect()
        .map_err(|e| format!("failed to open database: {e}"))?;
    let out_dir = if batch_config.output_dir == std::path::Path::new(".") {
        PathBuf::from(format!("profiling/sp1/continuous/{timestamp}"))
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

        std::fs::create_dir_all(&out_dir)?;
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
        let batch_started = Instant::now();

        let artifacts = match run_single_batch(&batch_config, &db).await {
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

pub async fn run_single_batch(config: &ProofGenerationConfig, db: &DbConn) -> Result<ProofArtifacts, BoxError> {
    let action = if config.execute_only {
        "Starting execution"
    } else {
        "Starting proof generation"
    };
    tracing::info!(
        "{} with backend {:?}{}",
        action,
        config.prover_backend,
        config
            .cuda_device_id
            .map(|id| format!(" (CUDA device {id})"))
            .unwrap_or_default(),
    );

    let artifacts = generate_and_save_proofs(config, db).await?;
    log_batch_completion(config.execute_only, &artifacts);
    Ok(artifacts)
}

fn log_batch_completion(execute_only: bool, artifacts: &ProofArtifacts) {
    tracing::info!(
        "Complete: validated headers from height {} to {}",
        artifacts.first_new_height,
        artifacts.end_height,
    );
    if execute_only {
        tracing::info!(
            "Execution completed in {:.2} seconds",
            artifacts.total_duration_secs
        );
        return;
    }

    if let Some(compressed_path) = artifacts.compressed_path.as_ref() {
        tracing::info!("Saved compressed proof to {}", compressed_path.display());
    }
    if let Some(groth16_path) = artifacts.groth16_path.as_ref() {
        tracing::info!("Saved Groth16 proof to {}", groth16_path.display());
    }
    tracing::info!("========================================");
    tracing::info!(
        "TOTAL PROVING TIME: {:.2} seconds",
        artifacts.total_duration_secs
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

fn log_prover_backend_selection(prover_backend: ProverBackend, cuda_device_id: Option<u32>) {
    match prover_backend {
        ProverBackend::Mock => tracing::info!("Using the Mock prover for instant execution"),
        ProverBackend::Cpu => {
            if cfg!(feature = "CUDA") {
                tracing::info!(
                    "CUDA support is compiled in, but CUDA=1 was not set; using the CPU prover"
                );
            } else {
                tracing::info!("CUDA support is not compiled in; using the CPU prover");
            }
        }
        ProverBackend::Cuda => {
            tracing::info!(
                "CUDA=1 requested; preparing the GPU prover{}",
                cuda_device_id
                    .map(|id| format!(" on device {id}"))
                    .unwrap_or_else(|| " on device 0".to_owned()),
            );
        }
    }
}

pub async fn generate_and_save_proofs(
    config: &ProofGenerationConfig,
    db: &DbConn,
) -> Result<ProofArtifacts, BoxError> {
    if config.mode() != PipelineMode::ExecuteOnly {
        log_prover_backend_selection(config.prover_backend, config.cuda_device_id);
    }
    if config.mode() == PipelineMode::ExecuteOnly && config.prover_backend == ProverBackend::Mock {
        return execute_without_setup(config, db).await;
    }

    match get_prepared_prover(config.prover_backend, config.cuda_device_id).await? {
        PreparedProver::Mock {
            prover,
            proving_key,
        } => run_with_prover(config, db, "mock", &prover, &proving_key).await,
        PreparedProver::Cpu {
            prover,
            proving_key,
        } => run_with_prover(config, db, "cpu", &prover, &proving_key).await,
        #[cfg(feature = "CUDA")]
        PreparedProver::Cuda {
            prover,
            proving_key,
        } => run_with_prover(config, db, "cuda", &prover, &proving_key).await,
    }
}

async fn execute_without_setup(config: &ProofGenerationConfig, db: &DbConn) -> Result<ProofArtifacts, BoxError> {
    if config.prev_proof_path.is_some() {
        tracing::info!("Execute-only resume needs a previous proof witness; preparing prover");
        match get_prepared_prover(config.prover_backend, config.cuda_device_id).await? {
            PreparedProver::Mock {
                prover,
                proving_key,
            } => return run_with_prover(config, db, "mock", &prover, &proving_key).await,
            _ => unreachable!("mock backend should prepare a mock prover"),
        }
    }

    clear_phase_timings();
    let overall_start = Instant::now();
    let genesis_hash = parse_genesis_hash()?;
    let batch = prepare_batch(
        config.prev_proof_path.as_ref(),
        config.trusted_start_height,
        config.num_headers,
        db,
        genesis_hash,
    )?;
    let prover = timed_async("build_mock_executor", || async {
        Ok::<_, BoxError>(ProverClient::builder().mock().build().await)
    })
    .await?;

    let executed = execute_batch_without_proof(
        &prover,
        UnprovenBatchInput {
            current_state: &batch.current_state,
            headers: &batch.headers,
            median_hints: &batch.median_hints,
            expected_state: &batch.expected_state,
            expected_continuation_digest: batch.expected_continuation_digest,
        },
    )
    .await?;

    Ok(ProofArtifacts::execution_only(
        &batch,
        executed,
        overall_start.elapsed(),
    ))
}

async fn run_with_prover<P, K>(
    config: &ProofGenerationConfig,
    db: &DbConn,
    prover_name: &str,
    prover: &P,
    proving_key: &K,
) -> Result<ProofArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    clear_phase_timings();
    let overall_start = Instant::now();

    let genesis_hash = parse_genesis_hash()?;
    let batch = prepare_batch(
        config.prev_proof_path.as_ref(),
        config.trusted_start_height,
        config.num_headers,
        db,
        genesis_hash,
    )?;

    if config.mode() == PipelineMode::ExecuteOnly {
        let executed = execute_batch_with_prover(prover_name, prover, proving_key, &batch).await?;
        return Ok(ProofArtifacts::execution_only(
            &batch,
            executed,
            overall_start.elapsed(),
        ));
    }

    let compressed = generate_compressed_proof(prover_name, prover, proving_key, &batch).await?;

    std::fs::create_dir_all(&config.output_dir)?;
    let compressed_path = config.output_dir.join(format!(
        "proof_height_{}_to_{}.bin",
        batch.first_new_height, batch.end_height,
    ));
    timed_sync("save_compressed_proof", || -> Result<(), BoxError> {
        compressed.compressed_proof.save(&compressed_path)?;
        Ok(())
    })?;

    let (groth16_path, groth16_proof) = if config.mode() == PipelineMode::ProveCompressedAndGroth16
    {
        let groth16_proof =
            generate_groth16_proof(&compressed.compressed_proof, &compressed.vk).await?;
        let groth16_path = config.output_dir.join(format!(
            "proof_height_{}_to_{}_groth16.bin",
            batch.first_new_height, batch.end_height,
        ));
        timed_sync("save_groth16_proof", || -> Result<(), BoxError> {
            groth16_proof.save(&groth16_path)?;
            Ok(())
        })?;
        (Some(groth16_path), Some(groth16_proof))
    } else {
        tracing::info!("Skipping Groth16 wrapping; set ZKPOW_GENERATE_GROTH16=1 to enable it");
        (None, None)
    };

    Ok(ProofArtifacts {
        compressed_path: Some(compressed_path),
        groth16_path,
        compressed_proof: Some(compressed.compressed_proof),
        groth16_proof,
        before_prove_sample: compressed.before_prove_sample,
        execution_report: compressed.execution_report,
        first_new_height: batch.first_new_height,
        end_height: batch.end_height,
        total_duration_secs: overall_start.elapsed().as_secs_f64(),
        phase_timings: collected_phase_timings(),
    })
}

impl ProofArtifacts {
    fn execution_only(
        batch: &batch::PreparedBatch,
        executed: execution::ExecutedBatchArtifacts,
        elapsed: std::time::Duration,
    ) -> Self {
        Self {
            compressed_path: None,
            groth16_path: None,
            compressed_proof: None,
            groth16_proof: None,
            before_prove_sample: executed.before_prove_sample,
            execution_report: executed.execution_report,
            first_new_height: batch.first_new_height,
            end_height: batch.end_height,
            total_duration_secs: elapsed.as_secs_f64(),
            phase_timings: collected_phase_timings(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::diagnostics::{
        format_claim_mismatch_with_mode, highlight_with_expected_mode, value_prefix, HighlightMode,
    };
    use crate::pipeline::execution::verify_public_values;
    use crate::pipeline::input::{
        config_from_source, ensure_cuda_requested_configuration, parse_bool_env, parse_u32_env,
        EnvSource, MapEnvSource, ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_EXECUTE_ONLY,
        ENV_ZKPOW_GENERATE_GROTH16, ENV_ZKPOW_USE_CUDA,
    };
    use crate::util;
    use std::collections::HashMap;
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
    fn parses_generate_groth16_env() {
        let cases = [
            (Some("true"), Ok(true)),
            (Some("false"), Ok(false)),
            (None, Ok(false)),
            (Some("invalid"), Err(())),
        ];

        for (value, expected) in cases {
            let source = value.map_or_else(MapEnvSource::default, |value| {
                let mut values = HashMap::new();
                values.insert(ENV_ZKPOW_GENERATE_GROTH16.to_string(), value.to_string());
                MapEnvSource::new(values)
            });

            let parsed = parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16);
            match expected {
                Ok(expected) => assert_eq!(parsed.unwrap(), expected, "value={value:?}"),
                Err(()) => assert!(parsed.is_err(), "value={value:?} should error"),
            }
        }
    }

    #[test]
    fn rejects_cuda_device_without_cuda_flag() {
        assert!(ensure_cuda_requested_configuration(false, Some(0)).is_err());
        assert!(ensure_cuda_requested_configuration(false, None).is_ok());
    }

    #[test]
    #[cfg(not(feature = "CUDA"))]
    fn rejects_cuda_when_feature_is_not_compiled_in() {
        let err = ensure_cuda_requested_configuration(true, None).unwrap_err();
        assert!(
            err.to_string().contains("--features CUDA"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn config_validation_matrix_invalid_values() {
        struct SingleValueEnv {
            key: &'static str,
            value: &'static str,
        }

        impl EnvSource for SingleValueEnv {
            fn get(&self, var_name: &str) -> Result<String, std::env::VarError> {
                if var_name == self.key {
                    Ok(self.value.to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            }
        }

        enum Parser {
            Bool,
            U32,
        }

        let cases = [
            (ENV_ZKPOW_USE_CUDA, "maybe", Parser::Bool),
            ("ZKPOW_CUDA_DEVICE_ID", "abc", Parser::U32),
            (ENV_ZKPOW_GENERATE_GROTH16, "yessir", Parser::Bool),
        ];

        for (key, value, parser) in cases {
            let source = SingleValueEnv { key, value };
            let parsed = match parser {
                Parser::Bool => parse_bool_env(&source, key).map(|_| ()),
                Parser::U32 => parse_u32_env(&source, key).map(|_| ()),
            };

            assert!(parsed.is_err(), "{key}={value:?} should be invalid");
        }
    }

    #[test]
    fn execute_only_defaults_to_mock_backend() {
        let mut values = HashMap::new();
        values.insert(ENV_ZKPOW_EXECUTE_ONLY.to_string(), "1".to_string());
        values.insert(ENV_ZKPOW_BATCH_SIZE.to_string(), "1".to_string());

        let source = MapEnvSource::new(values);
        let config = config_from_source(&source).unwrap();

        assert_eq!(config.prover_backend, ProverBackend::Mock);
        assert!(config.execute_only);
    }

    #[test]
    fn pipeline_error_mapping_parses_failure_codes() {
        let state: util::State = util::State::default();
        let digest = [0xAAu8; 32];
        let vk = crate::util::VerifierKeyDigest::from_raw([0x11u32; 8]);
        let claim = state.public_claim();
        let pv_bytes = util::MinimalPublicValues::failure(
            &claim,
            zkpow_core::ValidationErrorCode::PowInsufficient,
            1234,
            digest,
            vk,
        )
        .to_bytes();

        let expected = util::MinimalPublicValues::success(&claim, digest, vk).to_bytes();
        let err = verify_public_values(&pv_bytes, &expected, "unit-test").unwrap_err();
        assert!(err.to_string().contains("error"));
        assert!(err.to_string().contains("height 1234"));
    }

    #[test]
    fn highlight_marks_mismatches_and_zero_padding() {
        let cases = [
            (
                "bracket mismatch",
                "00010203",
                "0001ff04",
                HighlightMode::Brackets,
                false,
                "0001[ff04]",
                "    [0203]",
            ),
            (
                "ansi mismatch",
                "00010203",
                "0001ff04",
                HighlightMode::Ansi,
                false,
                "0001\x1b[31mff04\x1b[0m",
                "    \x1b[32m0203\x1b[0m",
            ),
            (
                "bright leading zero bytes",
                "00000102",
                "00000002",
                HighlightMode::Ansi,
                true,
                "\x1b[97m0000\x1b[0m\x1b[31m00\x1b[0m02",
                "    \x1b[32m01\x1b[0m  ",
            ),
        ];

        for (
            name,
            expected_value,
            actual_value,
            mode,
            highlight_zeroes,
            expected_actual_row,
            expected_expected_row,
        ) in cases
        {
            let (actual_output, expected_output) =
                highlight_with_expected_mode(expected_value, actual_value, mode, highlight_zeroes);

            assert_eq!(actual_output, expected_actual_row, "{name}: actual row");
            assert_eq!(
                expected_output, expected_expected_row,
                "{name}: expected row"
            );
        }
    }

    #[test]
    fn claim_mismatch_splits_32_byte_fields_and_pairs_corrections() {
        let actual = util::Claim {
            genesis_hash: util::BlockHash::from_le_bytes([0x11; 32]),
            tip_hash: util::BlockHash::from_le_bytes([0x22; 32]),
            chain_work: util::ChainWork::from_le_bytes([0x33; 32]),
            height: 1,
        };
        let expected = util::Claim {
            tip_hash: util::BlockHash::from_le_bytes([0x44; 32]),
            height: 2,
            ..actual
        };

        let output = format_claim_mismatch_with_mode(&actual, &expected, HighlightMode::Brackets);

        let tip_hash_value_prefix = value_prefix("tip_hash");
        assert!(output.contains(
            &format!("tip_hash: [22222222222222222222222222222222]\n{tip_hash_value_prefix}[44444444444444444444444444444444]\n\n{tip_hash_value_prefix}[22222222222222222222222222222222]")
        ));
        assert!(output.contains(&format!(
            "{tip_hash_value_prefix}[44444444444444444444444444444444]"
        )));
        assert!(output.contains("height: 000000[01]"));
        assert!(output.contains(&format!("{}[02]", value_prefix("height"))));
        assert!(output.contains("expected claim:"));
        assert!(output.contains(
            "tip_hash: 44444444444444444444444444444444\n                44444444444444444444444444444444"
        ));
    }

    #[test]
    fn claim_mismatch_keeps_matching_32_byte_fields_to_two_lines() {
        let claim = util::Claim {
            genesis_hash: util::BlockHash::from_le_bytes([0x11; 32]),
            tip_hash: util::BlockHash::from_le_bytes([0x22; 32]),
            chain_work: util::ChainWork::from_le_bytes([0x33; 32]),
            height: 1,
        };

        let output = format_claim_mismatch_with_mode(&claim, &claim, HighlightMode::Brackets);

        assert!(output.contains(
            "genesis_hash: 11111111111111111111111111111111\n                    11111111111111111111111111111111,"
        ));
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
