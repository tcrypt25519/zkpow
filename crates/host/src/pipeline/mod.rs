//! The host proving pipeline.
//!
//! The host is a stateless I/O driver between the controller and the guest:
//! it pulls batches from the controller, pipes them into the SP1 zkVM,
//! uploads the resulting artifacts to controller-chosen targets, and reports
//! completions. It validates nothing itself — resume points, batch
//! expectations, and result verification all live in the controller.
//!
//! # The double-buffered loop
//!
//! While the GPU proves batch N, the host has already spawned the pull for
//! batch N+1 (the controller long-polls it until batch N-1's completion is
//! verified) and is finishing batch N-1's upload + completion notice in the
//! background. The moment proof N finishes, batch N+1 is already staged in
//! memory and the guest restarts immediately; proof N's upload, completion
//! notice, and the pull for batch N+2 then run while N+1 proves.
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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use memuse::{StageHistory, StageMetric};
use sp1_sdk::prelude::*;
use sp1_sdk::{ExecutionReport, MockProver, ProverClient, SP1ProofWithPublicValues};

use crate::memory_monitor;
use crate::util::VerifierKeyDigest;
use diagnostics::{clear_phase_timings, collected_phase_timings, timed_sync};
use execution::execute_batch;
use input::{build_stdin, config_from_env, controller_url_from_env, genesis_hash};
use proof::compressed::generate_compressed_proof;
use proof::groth16::generate_groth16_proof;
use proof::{get_prepared_prover, PreparedProver};
use transport::{ControllerTransport, HttpController, LocalController};

use zkpow_controller::protocol::{CompleteAck, CompleteRequest, PullBatchRequest, Target};
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
    /// Manual override: preload the held proof witness from this file
    /// instead of downloading the controller's verified-tip proof.
    pub prev_proof_path: Option<PathBuf>,
    pub num_headers: u32,
    pub batch_count: u32,
    pub db_path: PathBuf,
    pub generate_groth16: bool,
    pub execute_only: bool,
    pub prover_backend: ProverBackend,
    pub cuda_device_id: Option<u32>,
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

fn is_header_exhaustion_error(err: &BoxError) -> bool {
    err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX)
}

/// Resolves the controller transport from the environment exactly once, then
/// dispatches into a monomorphized copy of [`run_batch_session_with`] for
/// that concrete transport — no `Box<dyn Trait>` or vtable calls in the
/// batch loop.
pub async fn run_batch_session() -> Result<u32, BoxError> {
    let config = config_from_env()?;
    match controller_url_from_env() {
        Some(url) => {
            tracing::info!("Using remote controller at {url}");
            run_batch_session_with(Arc::new(HttpController::new(url)), config).await
        }
        None => {
            let runtime = build_local_controller(&config).await?;
            run_batch_session_with(Arc::new(LocalController::new(runtime)), config).await
        }
    }
}

/// Build the in-process controller for local mode: same runtime, database,
/// storage, and verification as `zkpow-controller-serve`, minus the socket.
async fn build_local_controller(
    config: &ProofGenerationConfig,
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
    let verifier: Option<Box<dyn ProofVerifier>> = if config.execute_only {
        None
    } else {
        let prepared = get_prepared_prover(config.prover_backend, config.cuda_device_id).await?;
        Some(Box::new(verify::PreparedProverVerifier::new(prepared)))
    };

    let mut controller_config = ControllerConfig::new(genesis_hash());
    controller_config.allow_unproven = config.execute_only;
    Ok(ControllerRuntime::new(
        db,
        storage,
        verifier,
        controller_config,
    ))
}

/// The prover backing one session. Execute-only sessions run the RISC-V
/// executor without ever computing a proving key; the guest receives the
/// default (all-zero) verifier key digest and skips proof recursion.
enum SessionProver {
    Executor(MockProver),
    Prover(PreparedProver),
}

impl SessionProver {
    fn verifier_key(&self) -> VerifierKeyDigest {
        match self {
            SessionProver::Executor(_) => VerifierKeyDigest::default(),
            SessionProver::Prover(prepared) => prepared.verifier_key_digest(),
        }
    }
}

/// The compressed proof the host keeps in memory to witness the next
/// batch's recursion.
struct HeldProof {
    end_height: u32,
    proof: SP1ProofWithPublicValues,
}

/// Everything one batch produced.
struct BatchOutcome {
    public_values: Vec<u8>,
    compressed_proof: Option<SP1ProofWithPublicValues>,
    groth16_proof: Option<SP1ProofWithPublicValues>,
    /// Cycle statistics, available in execute-only mode (proving does not
    /// run a separate execution pass).
    execution_report: Option<ExecutionReport>,
}

/// Background task handle that aborts its task when dropped, so an error
/// exit never leaves a long-polling pull or an upload dangling.
struct Inflight<T>(tokio::task::JoinHandle<Result<T, BoxError>>);

impl<T> Inflight<T> {
    fn spawn(
        future: impl std::future::Future<Output = Result<T, BoxError>> + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
    {
        Self(tokio::spawn(future))
    }

    async fn join(mut self) -> Result<T, BoxError> {
        match (&mut self.0).await {
            Ok(result) => result,
            Err(err) => Err(format!("background task failed: {err}").into()),
        }
    }
}

impl<T> Drop for Inflight<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn pull_request(
    config: &ProofGenerationConfig,
    verifier_key: VerifierKeyDigest,
    held_tip: Option<u32>,
) -> PullBatchRequest {
    PullBatchRequest {
        run_id: config.run_id.clone(),
        verifier_key,
        num_headers: config.num_headers,
        held_tip,
    }
}

async fn run_batch_session_with<T: ControllerTransport>(
    transport: Arc<T>,
    config: ProofGenerationConfig,
) -> Result<u32, BoxError> {
    let mut memory_history = StageHistory::new(["Batch start", "Before prove", "Batch end"]);
    memory_monitor::log_point(
        "session_memory_start",
        "Session memory snapshot before batches",
    );

    tracing::info!(
        run_id = %config.run_id,
        "Run started for {} batches of {} headers each",
        config.batch_count,
        config.num_headers
    );

    let session_prover = if config.execute_only {
        tracing::info!("Execute-only session; using the RISC-V executor without proving");
        SessionProver::Executor(ProverClient::builder().mock().build().await)
    } else {
        log_prover_backend_selection(config.prover_backend, config.cuda_device_id);
        SessionProver::Prover(
            get_prepared_prover(config.prover_backend, config.cuda_device_id).await?,
        )
    };
    let verifier_key = session_prover.verifier_key();

    // Manual resume override: preload the held proof witness from a file.
    let mut held: Option<HeldProof> = match &config.prev_proof_path {
        Some(path) => {
            tracing::info!(
                "{} set; preloading held proof from {}",
                input::ENV_ZKPOW_PREV_PROOF,
                path.display()
            );
            Some(held_proof_from_proof(
                SP1ProofWithPublicValues::load(path)?,
                None,
            )?)
        }
        None => None,
    };

    let http = reqwest::Client::new();
    let mut completed: u32 = 0;
    let mut inflight_pull: Option<Inflight<zkpow_controller::protocol::Batch>> = None;
    let mut inflight_io: Option<Inflight<CompleteAck>> = None;

    loop {
        if completed >= config.batch_count {
            tracing::info!(
                "Reached ZKPOW_BATCH_COUNT={}; stopping continuous prover",
                config.batch_count
            );
            break;
        }

        // ---- 1. Take the batch prefetched during the previous prove ----
        let pull_result = match inflight_pull.take() {
            Some(inflight) => inflight.join().await,
            None => {
                let request =
                    pull_request(&config, verifier_key, held.as_ref().map(|h| h.end_height));
                transport.pull_batch(&request).await
            }
        };
        let batch = match pull_result {
            Ok(batch) => batch,
            Err(err) if is_header_exhaustion_error(&err) => {
                tracing::info!(
                    "No remaining headers in database; stopping continuous prover after {} batches",
                    completed
                );
                break;
            }
            Err(err) => {
                // A failed completion notice explains a failed pull better
                // than the pull error itself (e.g. the controller halted the
                // run); surface it first if it errored.
                if let Some(io) = inflight_io.take() {
                    io.join().await?;
                }
                return Err(err);
            }
        };
        let zkpow_controller::protocol::Batch {
            first_new_height,
            end_height,
            current_state,
            header_bytes,
            median_hint_bytes,
            prev_proof_fetch,
            proof_put,
        } = batch;

        let start_memory = memory_monitor::log_point(
            "batch_memory_start",
            "Batch memory snapshot before batch work",
        );
        let batch_started = Instant::now();
        tracing::info!(
            "=== Starting batch {} (heights {first_new_height}..={end_height}) ===",
            completed + 1,
        );

        // ---- 2. Recover the proof witness when resuming mid-run ----
        if let Some(target) = &prev_proof_fetch {
            tracing::info!("Fetching verified-tip proof to resume the run");
            let bytes = artifacts::fetch_artifact(&http, target).await?;
            let proof: SP1ProofWithPublicValues = bincode::deserialize(&bytes)
                .map_err(|err| format!("resume proof does not deserialize: {err}"))?;
            held = Some(held_proof_from_proof(proof, Some(current_state.height))?);
        }
        let recursion = !config.execute_only && current_state.height > 0;
        if recursion && held.as_ref().map(|h| h.end_height) != Some(current_state.height) {
            return Err(format!(
                "cannot extend run {} from height {}: held proof witness covers {:?}",
                config.run_id,
                current_state.height,
                held.as_ref().map(|h| h.end_height)
            )
            .into());
        }

        // ---- 3. Prefetch the next batch while the GPU is busy. The
        // controller long-polls it until this batch's predecessor verifies.
        if completed + 1 < config.batch_count {
            let request = pull_request(&config, verifier_key, Some(end_height));
            let transport = transport.clone();
            inflight_pull = Some(Inflight::spawn(async move {
                transport.pull_batch(&request).await
            }));
        }

        // ---- 4. Feed the guest and execute/prove. The witness bytes and the
        // held proof are moved into stdin untouched — the host never decodes
        // or copies them.
        clear_phase_timings();
        let stdin = timed_sync("serialize_input", || {
            let previous = match &session_prover {
                SessionProver::Prover(prepared) if recursion => {
                    let witness = held.take().expect("held proof checked above");
                    Some((witness.proof, prepared.verifying_key()))
                }
                _ => None,
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
        let outcome = run_batch(&session_prover, config.mode(), stdin).await?;

        // ---- 5. The previous batch's upload + completion must have landed ----
        if let Some(io) = inflight_io.take() {
            io.join().await?;
        }

        // ---- 6. Stage the new witness and ship this batch's proof to the
        // controller-chosen target in the background ----
        let proof_bytes = outcome
            .compressed_proof
            .as_ref()
            .map(|proof| bincode::serialize(proof))
            .transpose()
            .map_err(|err| format!("failed to serialize compressed proof: {err}"))?;

        // The Groth16 wrap is a terminal on-chain artifact, not part of the
        // IVC chain the controller tracks. When produced, save it locally.
        if let Some(groth16) = &outcome.groth16_proof {
            let path = format!("proof_height_{first_new_height}_to_{end_height}_groth16.bin");
            groth16
                .save(&path)
                .map_err(|err| format!("failed to save groth16 proof: {err}"))?;
            tracing::info!("Saved Groth16 proof to {path}");
        }

        log_batch_completion(&outcome, batch_started.elapsed().as_secs_f64());

        held = outcome
            .compressed_proof
            .map(|proof| HeldProof { end_height, proof });

        inflight_io = Some(ship_batch(
            transport.clone(),
            http.clone(),
            config.run_id.clone(),
            end_height,
            proof_put,
            outcome.public_values,
            proof_bytes,
        ));
        completed += 1;

        let end_memory = memory_monitor::log_point(
            "batch_memory_end",
            "Batch memory snapshot after handing off batch artifacts",
        );
        memory_history.push_iteration([start_memory, before_prove_memory, end_memory])?;
    }

    // ---- Drain the final upload + completion; abandon any dangling pull ----
    if let Some(io) = inflight_io.take() {
        io.join().await?;
    }
    drop(inflight_pull); // aborts a still-long-polling prefetch

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

    Ok(completed)
}

/// Upload this batch's proof to the controller-chosen target, then report
/// completion. Runs in the background while the next batch proves.
fn ship_batch<T: ControllerTransport>(
    transport: Arc<T>,
    http: reqwest::Client,
    run_id: String,
    end_height: u32,
    proof_put: Option<Target>,
    public_values: Vec<u8>,
    proof_bytes: Option<Vec<u8>>,
) -> Inflight<CompleteAck> {
    Inflight::spawn(async move {
        let has_proof = proof_bytes.is_some();
        if let Some(bytes) = proof_bytes {
            let target = proof_put
                .as_ref()
                .ok_or("controller returned no proof upload target")?;
            timed_upload("compressed proof", &http, target, bytes).await?;
        }
        let ack = transport
            .complete(&CompleteRequest {
                run_id,
                end_height,
                has_proof,
                public_values,
            })
            .await?;
        tracing::info!(
            verified_height = ack.verified_height,
            "Controller verified batch"
        );
        Ok(ack)
    })
}

async fn timed_upload(
    label: &str,
    http: &reqwest::Client,
    target: &Target,
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
    session_prover: &SessionProver,
    mode: PipelineMode,
    stdin: SP1Stdin,
) -> Result<BatchOutcome, BoxError> {
    match session_prover {
        SessionProver::Executor(prover) => {
            let (public_values, execution_report) = execute_batch(prover, stdin).await?;
            Ok(BatchOutcome {
                public_values,
                compressed_proof: None,
                groth16_proof: None,
                execution_report: Some(execution_report),
            })
        }
        SessionProver::Prover(prepared) => match prepared {
            PreparedProver::Mock {
                prover,
                proving_key,
            } => prove_batch(prover, proving_key, stdin, mode).await,
            PreparedProver::Cpu {
                prover,
                proving_key,
            } => prove_batch(prover, proving_key, stdin, mode).await,
            #[cfg(feature = "CUDA")]
            PreparedProver::Cuda {
                prover,
                proving_key,
            } => prove_batch(prover, proving_key, stdin, mode).await,
        },
    }
}

async fn prove_batch<P, K>(
    prover: &P,
    proving_key: &K,
    stdin: SP1Stdin,
    mode: PipelineMode,
) -> Result<BatchOutcome, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let compressed_proof = generate_compressed_proof(prover, proving_key, stdin).await?;

    let groth16_proof = if mode == PipelineMode::ProveCompressedAndGroth16 {
        Some(generate_groth16_proof(&compressed_proof, proving_key.verifying_key()).await?)
    } else {
        None
    };

    Ok(BatchOutcome {
        public_values: compressed_proof.public_values.to_vec(),
        compressed_proof: Some(compressed_proof),
        groth16_proof,
        execution_report: None,
    })
}

fn held_proof_from_proof(
    proof: SP1ProofWithPublicValues,
    expected_end_height: Option<u32>,
) -> Result<HeldProof, BoxError> {
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
    Ok(HeldProof { end_height, proof })
}

fn log_batch_completion(outcome: &BatchOutcome, total_duration_secs: f64) {
    let Some(report) = &outcome.execution_report else {
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
        return;
    };

    tracing::info!("Execution completed in {total_duration_secs:.2} seconds");
    log_execution_report(report, total_duration_secs);
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
        assert!(config.execute_only);
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
        let _env = SessionEnv::new("batch-count");
        set_env("ZKPOW_BATCH_COUNT", "0");
        set_env(ENV_ZKPOW_BATCH_SIZE, "1");

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

    /// Drives real execute-only batches end to end through the in-process
    /// controller: pull -> execute -> complete -> ledger advance, twice, with
    /// the second session resuming from the run ledger.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_only_session_advances_and_resumes() {
        let _guard = env_test_mutex().lock().await;
        let env = SessionEnv::new("advance-resume");
        set_env("ZKPOW_BATCH_COUNT", "2");
        set_env(ENV_ZKPOW_BATCH_SIZE, "3");

        let completed = run_batch_session()
            .await
            .expect("first session should complete");
        assert_eq!(completed, 2);

        // Second session must resume at height 6 and prove 2 more batches.
        let completed = run_batch_session()
            .await
            .expect("second session should complete");
        assert_eq!(completed, 2);

        let record = HeaderStore::open(db_path())
            .unwrap()
            .load_run(&env.run_id)
            .unwrap()
            .expect("run should exist in ledger");
        assert_eq!(record.verified_height, 12);
    }
}
