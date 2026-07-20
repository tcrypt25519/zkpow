//! The host proving pipeline.
//!
//! The host is a stateless I/O driver between the controller and the guest:
//! it pulls batches from the controller, pipes them into the SP1 zkVM,
//! uploads the resulting artifacts to controller-chosen targets, and reports
//! completions. It validates nothing itself — resume points, batch
//! expectations, and result verification all live in the controller.
//!
//! # Batch scheduling
//!
//! Proofs are recursively ordered, but their I/O is not. While the prover
//! extends batch N, the host completes N-1 and pulls N+1. `try_join!` keeps
//! those failures in the active iteration instead of hiding them in detached
//! prefetch tasks.
//!
//! Transport selection: `ZKPOW_CONTROLLER_URL` picks the remote HTTP
//! controller; otherwise a full controller runtime (header database with the
//! run ledger, artifact storage, proof verification) is built in-process and
//! spoken to over the identical binary protocol.

pub mod artifacts;
pub(crate) mod diagnostics;
pub(crate) mod execution;
pub mod input;
pub(crate) mod proof;
pub mod transport;
pub mod verify;

use std::time::Instant;

use memuse::{StageHistory, StageMetric, StageSample};
use sp1_sdk::prelude::*;
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues};

use crate::memory_monitor;
use diagnostics::{clear_phase_timings, collected_phase_timings, timed_sync};
use execution::execute_batch;
use input::{build_stdin, config_from_env, controller_url_from_env, genesis_hash};
use proof::compressed::generate_compressed_proof;
use proof::groth16::generate_groth16_proof;
use proof::{get_prepared_prover, PreparedProver};
use transport::{ControllerTransport, HttpController, LocalController};

use zkpow_controller::protocol::{Batch, FileLocation};
use zkpow_controller::runtime::{ControllerConfig, ControllerRuntime};
use zkpow_controller::storage::{ArtifactStore, LocalArtifactStore, S3ArtifactStore};
use zkpow_controller::verify::ProofVerifier;
use zkpow_controller::HeaderStore;
use zkpow_core::HeaderChainPublicValues;

pub use diagnostics::log_execution_report;
pub use diagnostics::PhaseTiming;
pub use zkpow_controller::NO_HEADERS_REMAINING_PREFIX;

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
    /// The IVC chain this host extends; the controller tracks progress per
    /// run id in its database.
    pub run_id: String,
    /// Used only by the in-process controller and execute-only diagnostic.
    /// A remote controller ignores this host setting and selects its own size.
    pub local_batch_size: u32,
    pub batch_count: u32,
    pub db_path: std::path::PathBuf,
    pub mode: PipelineMode,
    pub prover_backend: ProverBackend,
    pub cuda_device_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    ExecuteOnly,
    ProveCompressed,
    ProveCompressedAndGroth16,
}

impl PipelineMode {
    fn generate_groth16(self) -> bool {
        self == Self::ProveCompressedAndGroth16
    }
}

fn is_header_exhaustion_error(err: &BoxError) -> bool {
    err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX)
}

/// Resolves the controller transport from the environment exactly once, then
/// dispatches into a monomorphized copy of [`run_batch_session_with`] for
/// that concrete transport — no `Box<dyn Trait>` or vtable calls in the
/// batch loop.
pub async fn run_batch_session() -> Result<u32, BoxError> {
    let config = config_from_env()?;
    if config.batch_count == 0 {
        tracing::info!("ZKPOW_BATCH_COUNT=0; nothing to do");
        return Ok(0);
    }

    if config.mode == PipelineMode::ExecuteOnly {
        if controller_url_from_env().is_some() {
            return Err(
                "execute-only is a local diagnostic mode and does not use the controller protocol"
                    .into(),
            );
        }
        return run_execution_session(config).await;
    }

    log_prover_backend_selection(config.prover_backend, config.cuda_device_id);
    let prepared = get_prepared_prover(config.prover_backend, config.cuda_device_id).await?;
    match controller_url_from_env() {
        Some(url) => {
            tracing::info!("Using remote controller at {url}");
            run_batch_session_with(HttpController::new(url), config, prepared).await
        }
        None => {
            let runtime = build_local_controller(&config, &prepared)?;
            run_batch_session_with(LocalController::new(runtime), config, prepared).await
        }
    }
}

/// Build the in-process controller for local mode: same runtime, database,
/// storage, and verification as `zkpow-controller-serve`, minus the socket.
fn build_local_controller(
    config: &ProofGenerationConfig,
    prepared: &PreparedProver,
) -> Result<ControllerRuntime, BoxError> {
    let db = HeaderStore::open(&config.db_path)
        .map_err(|err| format!("failed to open database: {err}"))?;

    let storage: Box<dyn ArtifactStore> = match input::s3_storage_config_from_env()? {
        Some(s3) => {
            tracing::info!(bucket = s3.bucket, "Using S3 artifact storage");
            Box::new(S3ArtifactStore::new(&s3)?)
        }
        None => {
            let root = input::proof_store_dir_from_env();
            tracing::info!(root = %root.display(), "Using local artifact storage");
            Box::new(LocalArtifactStore::new(root))
        }
    };

    // The in-process controller cryptographically verifies uploaded proofs
    // with the same prepared prover the host proves with.
    let verifier =
        Box::new(verify::PreparedProverVerifier::new(prepared.clone())) as Box<dyn ProofVerifier>;

    let controller_config = ControllerConfig::new(genesis_hash(), config.local_batch_size);
    Ok(ControllerRuntime::new(
        db,
        storage,
        verifier,
        controller_config,
    ))
}

/// The compressed proof the host keeps in memory to witness the next
/// batch's recursion.
struct PreviousProof {
    end_height: u32,
    proof: SP1ProofWithPublicValues,
}

/// Everything one batch produced.
struct BatchOutcome {
    compressed_proof: SP1ProofWithPublicValues,
    groth16_proof: Option<SP1ProofWithPublicValues>,
}

struct BatchCompletion {
    proof_upload: FileLocation,
    proof_bytes: Vec<u8>,
    continuation_upload: FileLocation,
    continuation_bytes: Vec<u8>,
}

async fn run_execution_session(config: ProofGenerationConfig) -> Result<u32, BoxError> {
    tracing::info!(
        "Execute-only diagnostic; controller progress and artifact storage are disabled"
    );
    let store = HeaderStore::open(&config.db_path)
        .map_err(|err| format!("failed to open database: {err}"))?;
    let prover = ProverClient::builder().mock().build().await;
    let genesis_hash = genesis_hash();
    let mut state = store.state_at_height(0, genesis_hash);
    let mut completed = 0;

    while completed < config.batch_count {
        let first_new_height = state.height + 1;
        let witness = store
            .load_header_batch_witness(first_new_height as u64, config.local_batch_size as u64);
        if witness.headers.is_empty() {
            break;
        }
        let end_height = state.height + witness.headers.len() as u32;
        clear_phase_timings();
        let stdin = build_stdin(
            zkpow_core::VerifierKeyDigest::default(),
            &state,
            zkpow_core::serialize_inputs(&witness.headers),
            zkpow_core::serialize_inputs(&witness.median_time_past_hints),
            None,
        )?;
        let started = Instant::now();
        let (_, report) = execute_batch(&prover, stdin).await?;
        let elapsed = started.elapsed().as_secs_f64();
        tracing::info!(
            first_new_height,
            end_height,
            "Execution completed in {elapsed:.2} seconds"
        );
        log_execution_report(&report, elapsed);
        state = store.state_at_height(end_height, genesis_hash);
        completed += 1;
    }
    Ok(completed)
}

async fn run_batch_session_with<T: ControllerTransport>(
    transport: T,
    config: ProofGenerationConfig,
    prepared: PreparedProver,
) -> Result<u32, BoxError> {
    let mut memory_history = memory_monitor::logging_enabled()
        .then(|| StageHistory::new(["Batch start", "Before prove", "Batch end"]));
    memory_monitor::log_point(
        "session_memory_start",
        "Session memory snapshot before batches",
    );

    tracing::info!(
        run_id = %config.run_id,
        "Run started for up to {} controller-selected batches",
        config.batch_count
    );

    let mut previous_proof: Option<PreviousProof> = None;

    let http = reqwest::Client::new();
    let mut completed = 0;
    let mut batch = classify_pull(transport.pull_batch(&config.run_id).await, completed)?;
    let mut pending_completion: Option<BatchCompletion> = None;

    while let Some(current_batch) = batch {
        let start_memory = memory_monitor::log_point(
            "batch_memory_start",
            "Batch memory snapshot before batch work",
        );
        let prove = process_batch(
            &http,
            &prepared,
            &config,
            completed + 1,
            current_batch,
            &mut previous_proof,
        );
        let finish_previous = async {
            if let Some(completion) = pending_completion.take() {
                complete_batch(&transport, &http, &config.run_id, completion).await?;
            }
            Ok::<(), BoxError>(())
        };
        let pull_next = async {
            if completed + 1 >= config.batch_count {
                Ok(None)
            } else {
                classify_pull(transport.pull_batch(&config.run_id).await, completed)
            }
        };
        let ((completion, before_prove_memory), (), pulled) =
            tokio::try_join!(prove, finish_previous, pull_next)?;
        completed += 1;
        pending_completion = Some(completion);
        batch = pulled;

        let end_memory = memory_monitor::log_point(
            "batch_memory_end",
            "Batch memory snapshot after completing batch",
        );
        if let Some(history) = &mut memory_history {
            history.push_iteration([start_memory, before_prove_memory, end_memory])?;
        }
    }

    if let Some(completion) = pending_completion {
        complete_batch(&transport, &http, &config.run_id, completion).await?;
    }

    if completed == config.batch_count {
        tracing::info!(
            "Reached ZKPOW_BATCH_COUNT={}; stopping continuous prover",
            config.batch_count
        );
    }

    if let Some(history) = memory_history {
        println!(
            "\n{}",
            history.render_table(StageMetric::RssKb, "BATCH RSS MATRIX (KB)")
        );
        println!(
            "\n{}",
            history.render_table(StageMetric::LiveKb, "BATCH LIVE HEAP MATRIX (KB)")
        );
        memory_monitor::log_point(
            "session_memory_end",
            "Session memory snapshot after batches",
        );
    }

    Ok(completed)
}

async fn process_batch(
    http: &reqwest::Client,
    prepared: &PreparedProver,
    config: &ProofGenerationConfig,
    batch_number: u32,
    batch: Batch,
    previous_proof: &mut Option<PreviousProof>,
) -> Result<(BatchCompletion, StageSample), BoxError> {
    let Batch {
        first_new_height,
        end_height,
        verifier_key,
        current_state,
        header_bytes,
        median_hint_bytes,
        previous_proof: previous_proof_location,
        proof_upload,
        continuation_bytes,
        continuation_upload,
    } = batch;

    let started = Instant::now();
    tracing::info!(
        "=== Starting batch {batch_number} (heights {first_new_height}..={end_height}) ==="
    );

    if verifier_key != prepared.verifier_key_digest() {
        return Err("controller verifier key does not match the host proving key".into());
    }

    let recursion = current_state.height > 0;
    if recursion
        && previous_proof.as_ref().map(|proof| proof.end_height) != Some(current_state.height)
    {
        let target = previous_proof_location
            .as_ref()
            .ok_or("controller omitted the previous proof for a recursive batch")?;
        tracing::info!("Fetching controller-selected previous proof");
        let bytes = artifacts::fetch_artifact(http, target).await?;
        let proof: SP1ProofWithPublicValues = postcard::from_bytes(&bytes)
            .map_err(|err| format!("previous proof does not deserialize: {err}"))?;
        *previous_proof = Some(previous_proof_from_proof(
            proof,
            Some(current_state.height),
        )?);
    }

    if recursion
        && previous_proof.as_ref().map(|proof| proof.end_height) != Some(current_state.height)
    {
        return Err(format!(
            "cannot extend run {} from height {}: previous proof covers {:?}",
            config.run_id,
            current_state.height,
            previous_proof.as_ref().map(|proof| proof.end_height)
        )
        .into());
    }

    clear_phase_timings();
    let stdin = timed_sync("serialize_input", || {
        let previous = if recursion {
            let proof = previous_proof.take().expect("previous proof checked above");
            Some((proof.proof, prepared.verifying_key()))
        } else {
            None
        };
        build_stdin(
            verifier_key,
            &current_state,
            header_bytes,
            median_hint_bytes,
            previous,
        )
    })?;
    let before_prove_memory = memory_monitor::log_point(
        "batch_memory_before_prove",
        "Batch memory snapshot before proving",
    );
    let outcome = run_batch(prepared, config.mode.generate_groth16(), stdin).await?;

    let proof_bytes = postcard::to_allocvec(&outcome.compressed_proof)
        .map_err(|err| format!("failed to serialize compressed proof: {err}"))?;

    if let Some(groth16) = &outcome.groth16_proof {
        let path = format!("proof_height_{first_new_height}_to_{end_height}_groth16.bin");
        groth16
            .save(&path)
            .map_err(|err| format!("failed to save groth16 proof: {err}"))?;
        tracing::info!("Saved Groth16 proof to {path}");
    }

    log_batch_completion(&outcome, started.elapsed().as_secs_f64());
    *previous_proof = Some(PreviousProof {
        end_height,
        proof: outcome.compressed_proof,
    });

    Ok((
        BatchCompletion {
            proof_upload,
            proof_bytes,
            continuation_upload,
            continuation_bytes,
        },
        before_prove_memory,
    ))
}

fn classify_pull(
    result: Result<Batch, BoxError>,
    completed: u32,
) -> Result<Option<Batch>, BoxError> {
    match result {
        Ok(batch) => Ok(Some(batch)),
        Err(err) if is_header_exhaustion_error(&err) => {
            tracing::info!(
                "No remaining headers in database; stopping continuous prover after {} batches",
                completed
            );
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn complete_batch<T: ControllerTransport>(
    transport: &T,
    http: &reqwest::Client,
    run_id: &str,
    completion: BatchCompletion,
) -> Result<(), BoxError> {
    tokio::try_join!(
        timed_upload(
            "compressed proof",
            http,
            &completion.proof_upload,
            completion.proof_bytes,
        ),
        timed_upload(
            "continuation data",
            http,
            &completion.continuation_upload,
            completion.continuation_bytes,
        ),
    )?;
    transport.complete(run_id).await?;
    tracing::info!("Controller verified batch");
    Ok(())
}

async fn timed_upload(
    label: &str,
    http: &reqwest::Client,
    target: &FileLocation,
    bytes: Vec<u8>,
) -> Result<(), BoxError> {
    let size = bytes.len();
    let started = Instant::now();
    artifacts::put_artifact(http, target, bytes).await?;
    tracing::info!(
        bytes = size,
        elapsed_secs = started.elapsed().as_secs_f64(),
        "Uploaded {label}"
    );
    Ok(())
}

async fn run_batch(
    prepared: &PreparedProver,
    generate_groth16: bool,
    stdin: SP1Stdin,
) -> Result<BatchOutcome, BoxError> {
    match prepared {
        PreparedProver::Mock {
            prover,
            proving_key,
        } => prove_batch(prover, proving_key, stdin, generate_groth16).await,
        PreparedProver::Cpu {
            prover,
            proving_key,
        } => prove_batch(prover, proving_key, stdin, generate_groth16).await,
        #[cfg(feature = "CUDA")]
        PreparedProver::Cuda {
            prover,
            proving_key,
        } => prove_batch(prover, proving_key, stdin, generate_groth16).await,
    }
}

async fn prove_batch<P, K>(
    prover: &P,
    proving_key: &K,
    stdin: SP1Stdin,
    generate_groth16: bool,
) -> Result<BatchOutcome, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let compressed_proof = generate_compressed_proof(prover, proving_key, stdin).await?;

    let groth16_proof = if generate_groth16 {
        Some(generate_groth16_proof(&compressed_proof, proving_key.verifying_key()).await?)
    } else {
        None
    };

    Ok(BatchOutcome {
        compressed_proof,
        groth16_proof,
    })
}

fn previous_proof_from_proof(
    proof: SP1ProofWithPublicValues,
    expected_end_height: Option<u32>,
) -> Result<PreviousProof, BoxError> {
    let end_height = match HeaderChainPublicValues::parse(proof.public_values.as_slice()) {
        Ok(HeaderChainPublicValues::Success { claim, .. }) => claim.height,
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => {
            return Err(format!(
                "previous proof ended in error {} at height {}",
                failure.error_code, failure.failure_height
            )
            .into());
        }
        Err(err) => return Err(format!("previous proof has malformed public values: {err}").into()),
    };
    if let Some(expected) = expected_end_height {
        if end_height != expected {
            return Err(format!(
                "previous proof covers height {end_height} but the run resumes from {expected}"
            )
            .into());
        }
    }
    Ok(PreviousProof { end_height, proof })
}

fn log_batch_completion(outcome: &BatchOutcome, total_duration_secs: f64) {
    let _ = outcome;
    tracing::info!("========================================");
    tracing::info!("TOTAL PROVING TIME: {total_duration_secs:.2} seconds");
    tracing::info!("========================================");
    let phase_timings = collected_phase_timings();
    if phase_timings.is_empty() {
        tracing::info!("Proving time breakdown: unavailable");
    } else {
        tracing::info!("Proving time breakdown:");
        for phase in &phase_timings {
            let pct = if total_duration_secs > 0.0 {
                (phase.total_duration_secs * 100.0) / total_duration_secs
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::input::{
        config_from_source, ensure_cuda_requested_configuration, parse_bool_env, parse_u32_env,
        EnvSource, MapEnvSource, ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_EXECUTE_ONLY,
        ENV_ZKPOW_GENERATE_GROTH16, ENV_ZKPOW_RUN_ID, ENV_ZKPOW_USE_CUDA,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use tokio::sync::Mutex;
    use zkpow_controller::db_path;

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
    fn execute_only_defaults_to_mock_backend_and_dev_run() {
        let mut values = HashMap::new();
        values.insert(ENV_ZKPOW_EXECUTE_ONLY.to_string(), "1".to_string());
        values.insert(ENV_ZKPOW_BATCH_SIZE.to_string(), "1".to_string());

        let source = MapEnvSource::new(values);
        let config = config_from_source(&source).unwrap();

        assert_eq!(config.prover_backend, ProverBackend::Mock);
        assert_eq!(config.mode, PipelineMode::ExecuteOnly);
        assert_eq!(config.run_id, "dev");
    }

    #[test]
    fn run_id_env_overrides_default() {
        let mut values = HashMap::new();
        values.insert(ENV_ZKPOW_RUN_ID.to_string(), "mainnet-01".to_string());
        let config = config_from_source(&MapEnvSource::new(values)).unwrap();
        assert_eq!(config.run_id, "mainnet-01");
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

    /// Points the session at a scratch artifact dir and a unique run id, and
    /// deletes the run's ledger row from the (shared, real) header database
    /// again on drop.
    struct SessionEnv {
        dir: PathBuf,
        run_id: String,
    }

    impl SessionEnv {
        fn new(test_name: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "zkpow-host-session-{test_name}-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let run_id = format!("test-{test_name}-{nonce}");
            set_env(
                "ZKPOW_PROOF_STORE_DIR",
                dir.join("artifacts").to_str().unwrap(),
            );
            set_env(ENV_ZKPOW_RUN_ID, &run_id);
            set_env(ENV_ZKPOW_EXECUTE_ONLY, "1");
            Self { dir, run_id }
        }
    }

    impl Drop for SessionEnv {
        fn drop(&mut self) {
            if let Ok(db) = HeaderStore::open(db_path()) {
                db.delete_run(&self.run_id).ok();
            }
            for name in [
                "ZKPOW_PROOF_STORE_DIR",
                ENV_ZKPOW_RUN_ID,
                ENV_ZKPOW_EXECUTE_ONLY,
                "ZKPOW_BATCH_COUNT",
                ENV_ZKPOW_BATCH_SIZE,
                "ZKPOW_DB_PATH",
            ] {
                remove_env(name);
            }
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stops_after_reaching_configured_batch_count() {
        let _guard = env_test_mutex().lock().await;
        let env = SessionEnv::new("batch-count");
        set_env("ZKPOW_BATCH_COUNT", "0");
        set_env(ENV_ZKPOW_BATCH_SIZE, "1");
        set_env("ZKPOW_DB_PATH", env.dir.to_str().unwrap());

        let completed = run_batch_session()
            .await
            .expect("session should exit cleanly");
        assert_eq!(completed, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stops_after_exhausting_headers_in_database() {
        let _guard = env_test_mutex().lock().await;
        let _env = SessionEnv::new("exhausted");
        set_env("ZKPOW_BATCH_COUNT", "5");
        set_env(ENV_ZKPOW_BATCH_SIZE, "0");
        set_env(
            "ZKPOW_DB_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db"),
        );

        let completed = run_batch_session()
            .await
            .expect("session should stop on exhaustion without error");
        assert_eq!(completed, 0);
    }

    /// Execute-only is deliberately a repeatable local diagnostic. It does
    /// not create or advance a production run.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_only_session_is_separate_from_run_progress() {
        let _guard = env_test_mutex().lock().await;
        let env = SessionEnv::new("advance-resume");
        set_env("ZKPOW_BATCH_COUNT", "2");
        set_env(ENV_ZKPOW_BATCH_SIZE, "3");

        let completed = run_batch_session()
            .await
            .expect("first session should complete");
        assert_eq!(completed, 2);

        let completed = run_batch_session()
            .await
            .expect("second session should complete");
        assert_eq!(completed, 2);

        assert!(HeaderStore::open(db_path())
            .unwrap()
            .load_run(&env.run_id)
            .unwrap()
            .is_none());
    }
}
