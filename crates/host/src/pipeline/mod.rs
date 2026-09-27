//! The host proving pipeline.
//!
//! The host is a stateless I/O driver between the controller and the guest:
//! it pulls a batch from the controller, pipes it into the SP1 zkVM, uploads
//! the resulting artifact, and reports completion, one batch at a time. It
//! validates nothing itself — resume points, batch expectations, and result
//! verification all live in the controller.
//!
//! The host and controller run in the same process: a full controller
//! runtime (header database with the run ledger, artifact storage, proof
//! verification) is built in-process and called directly.

pub mod artifacts;
pub(crate) mod diagnostics;
pub(crate) mod execution;
pub mod input;
pub(crate) mod proof;
pub mod verify;

use std::time::Instant;

use diagnostics::{clear_phase_timings, collected_phase_timings, timed_sync};
pub use diagnostics::{log_execution_report, PhaseTiming};
use execution::execute_batch;
use input::{build_stdin, config_from_env};
use memuse::{StageHistory, StageMetric, StageSample};
use proof::{compressed::generate_compressed_proof, get_prepared_prover, PreparedProver};
use sp1_sdk::{prelude::*, ProverClient, SP1ProofWithPublicValues};
pub use zkpow_controller::NO_HEADERS_REMAINING_PREFIX;
use zkpow_controller::{
    protocol::Batch,
    runtime::{ControllerConfig, ControllerRuntime},
    storage::{ArtifactStore, LocalArtifactStore},
    verify::ProofVerifier,
    HeaderStore,
};
use zkpow_core::PublicValues;
use zkpow_params::Consensus;

use crate::memory_monitor;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub const ELF: Elf = include_elf!("zkpow-guest");

/// Which SP1 prover the host uses. `Mock` executes without real proving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverBackend {
    Mock,
    Cpu,
}

/// One proving session's settings, read from `ZKPOW_*` variables by [`input::config_from_env`].
#[derive(Debug, Clone)]
pub struct ProofGenerationConfig {
    /// The IVC chain this host extends; the controller tracks progress per
    /// run id in its run ledger.
    pub run_id:           String,
    pub local_batch_size: u32,
    pub batch_count:      u32,
    /// The SQLite header database — run ledger and header content.
    pub db_path:          std::path::PathBuf,
    pub execute_only:     bool,
    pub prover_backend:   ProverBackend,
}

pub(crate) fn is_header_exhaustion_error(err: &BoxError) -> bool {
    err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX)
}

/// Prove controller-selected batches until `ZKPOW_BATCH_COUNT` is reached or the header database
/// runs out, returning how many batches completed.
pub async fn run_batch_session<C: Consensus>() -> Result<u32, BoxError> {
    let config = config_from_env::<C>()?;
    if config.batch_count == 0 {
        tracing::info!("ZKPOW_BATCH_COUNT=0; nothing to do");
        return Ok(0);
    }

    if config.execute_only {
        return run_execution_session::<C>(config).await;
    }

    log_prover_backend_selection(config.prover_backend);
    let prepared = get_prepared_prover(config.prover_backend).await?;
    let runtime = build_local_controller::<C>(&config, &prepared)?;
    run_batch_session_with(runtime, config, prepared).await
}

/// Build the in-process controller: run ledger, header content, local-disk
/// artifact storage, and proof verification.
fn build_local_controller<C: Consensus>(
    config: &ProofGenerationConfig,
    prepared: &PreparedProver,
) -> Result<ControllerRuntime<C>, BoxError> {
    let db = HeaderStore::open(&config.db_path).map_err(|err| format!("failed to open database: {err}"))?;

    let root = input::proof_store_dir_from_env();
    tracing::info!(root = %root.display(), "Using local artifact storage");
    let storage: Box<dyn ArtifactStore> = Box::new(LocalArtifactStore::new(root));

    // The in-process controller cryptographically verifies uploaded proofs
    // with the same prepared prover the host proves with.
    let verifier = Box::new(verify::PreparedProverVerifier::new(prepared.clone())) as Box<dyn ProofVerifier>;

    let controller_config = ControllerConfig::new(C::GENESIS_HASH, config.local_batch_size);
    Ok(ControllerRuntime::new(db, storage, verifier, controller_config))
}

struct BatchCompletion {
    proof_upload: String,
    proof_bytes:  Vec<u8>,
}

async fn run_execution_session<C: Consensus>(config: ProofGenerationConfig) -> Result<u32, BoxError> {
    tracing::info!("Execute-only diagnostic; controller progress and artifact storage are disabled");
    let db = HeaderStore::open(&config.db_path).map_err(|err| format!("failed to open database: {err}"))?;
    let prover = ProverClient::builder().mock().build().await;
    let genesis_hash = C::GENESIS_HASH;
    let mut state = db.state_at_height::<C>(0, genesis_hash);
    let mut completed = 0;

    while completed < config.batch_count {
        let first_new_height = state.height + 1;
        let witness = db.load_header_batch_witness(first_new_height as u64, config.local_batch_size as u64);
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
        tracing::info!(first_new_height, end_height, "Execution completed in {elapsed:.2} seconds");
        log_execution_report(&report, elapsed);
        state = db.state_at_height::<C>(end_height, genesis_hash);
        completed += 1;
    }
    Ok(completed)
}

async fn run_batch_session_with<C: Consensus>(
    runtime: ControllerRuntime<C>,
    config: ProofGenerationConfig,
    prepared: PreparedProver,
) -> Result<u32, BoxError> {
    let mut memory_history =
        memory_monitor::logging_enabled().then(|| StageHistory::new(["Batch start", "Before prove", "Batch end"]));
    memory_monitor::log_point("session_memory_start", "Session memory snapshot before batches");

    tracing::info!(
        run_id = %config.run_id,
        "Run started for up to {} controller-selected batches",
        config.batch_count
    );

    let mut completed = 0;
    let mut batch = classify_pull(runtime.pull_batch(&config.run_id).await.map_err(Into::into), completed)?;

    while let Some(current_batch) = batch {
        let start_memory = memory_monitor::log_point("batch_memory_start", "Batch memory snapshot before batch work");
        let (completion, before_prove_memory) = process_batch(&prepared, completed + 1, current_batch).await?;
        complete_batch(&runtime, &config.run_id, completion).await?;
        completed += 1;

        let end_memory = memory_monitor::log_point("batch_memory_end", "Batch memory snapshot after completing batch");
        if let Some(history) = &mut memory_history {
            history.push_iteration([start_memory, before_prove_memory, end_memory])?;
        }

        batch = if completed >= config.batch_count {
            None
        } else {
            classify_pull(runtime.pull_batch(&config.run_id).await.map_err(Into::into), completed)?
        };
    }

    if completed == config.batch_count {
        tracing::info!("Reached ZKPOW_BATCH_COUNT={}; stopping continuous prover", config.batch_count);
    }

    if let Some(history) = memory_history {
        println!("\n{}", history.render_table(StageMetric::RssKb, "BATCH RSS MATRIX (KB)"));
        println!("\n{}", history.render_table(StageMetric::LiveKb, "BATCH LIVE HEAP MATRIX (KB)"));
        memory_monitor::log_point("session_memory_end", "Session memory snapshot after batches");
    }

    Ok(completed)
}

async fn process_batch(
    prepared: &PreparedProver,
    batch_number: u32,
    batch: Batch,
) -> Result<(BatchCompletion, StageSample), BoxError> {
    let Batch {
        first_new_height,
        end_height,
        verifier_key,
        current_state,
        header_bytes,
        median_hint_bytes,
        previous_proof,
        proof_upload,
    } = batch;

    let started = Instant::now();
    tracing::info!("=== Starting batch {batch_number} (heights {first_new_height}..={end_height}) ===");

    if verifier_key != prepared.verifier_key_digest() {
        return Err("controller verifier key does not match the host proving key".into());
    }

    let recursion = current_state.height > 0;
    let previous = if recursion {
        let target = previous_proof
            .as_ref()
            .ok_or("controller omitted the previous proof for a recursive batch")?;
        let bytes = artifacts::fetch_artifact(target).await?;
        let proof: SP1ProofWithPublicValues =
            postcard::from_bytes(&bytes).map_err(|err| format!("previous proof does not deserialize: {err}"))?;
        Some((validate_previous_proof(proof, current_state.height)?, prepared.verifying_key()))
    } else {
        None
    };

    clear_phase_timings();
    let stdin = timed_sync("serialize_input", || {
        build_stdin(verifier_key, &current_state, header_bytes, median_hint_bytes, previous)
    })?;
    let before_prove_memory =
        memory_monitor::log_point("batch_memory_before_prove", "Batch memory snapshot before proving");
    let compressed_proof = run_batch(prepared, stdin).await?;

    let proof_bytes = postcard::to_allocvec(&compressed_proof)
        .map_err(|err| format!("failed to serialize compressed proof: {err}"))?;

    log_batch_completion(started.elapsed().as_secs_f64());

    Ok((
        BatchCompletion {
            proof_upload,
            proof_bytes,
        },
        before_prove_memory,
    ))
}

fn classify_pull(result: Result<Batch, BoxError>, completed: u32) -> Result<Option<Batch>, BoxError> {
    match result {
        Ok(batch) => Ok(Some(batch)),
        Err(err) if is_header_exhaustion_error(&err) => {
            tracing::info!(
                "No remaining headers in database; stopping continuous prover after {} batches",
                completed
            );
            Ok(None)
        },
        Err(err) => Err(err),
    }
}

async fn complete_batch<C: Consensus>(
    runtime: &ControllerRuntime<C>,
    run_id: &str,
    completion: BatchCompletion,
) -> Result<(), BoxError> {
    timed_upload("compressed proof", &completion.proof_upload, completion.proof_bytes).await?;
    runtime.complete(run_id).await?;
    tracing::info!("Controller verified batch");
    Ok(())
}

async fn timed_upload(label: &str, target: &str, bytes: Vec<u8>) -> Result<(), BoxError> {
    let size = bytes.len();
    let started = Instant::now();
    artifacts::put_artifact(target, bytes).await?;
    tracing::info!(bytes = size, elapsed_secs = started.elapsed().as_secs_f64(), "Uploaded {label}");
    Ok(())
}

async fn run_batch(prepared: &PreparedProver, stdin: SP1Stdin) -> Result<SP1ProofWithPublicValues, BoxError> {
    match prepared {
        PreparedProver::Mock { prover, proving_key } => prove_batch(prover, proving_key, stdin).await,
        PreparedProver::Cpu { prover, proving_key } => prove_batch(prover, proving_key, stdin).await,
    }
}

async fn prove_batch<P, K>(prover: &P, proving_key: &K, stdin: SP1Stdin) -> Result<SP1ProofWithPublicValues, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    generate_compressed_proof(prover, proving_key, stdin).await
}

fn validate_previous_proof(
    proof: SP1ProofWithPublicValues,
    expected_end_height: u32,
) -> Result<SP1ProofWithPublicValues, BoxError> {
    let end_height = match PublicValues::parse(proof.public_values.as_slice()) {
        Ok(pv) if pv.is_success() => pv.state.height,
        Ok(pv) => {
            let error_code = pv.validation_error_code().expect("failure has an error code");
            return Err(format!("previous proof ended in error {error_code} at height {}", pv.state.height + 1).into());
        },
        Err(err) => return Err(format!("previous proof has malformed public values: {err}").into()),
    };
    if end_height != expected_end_height {
        return Err(format!(
            "previous proof covers height {end_height} but the run resumes from {expected_end_height}"
        )
        .into());
    }
    Ok(proof)
}

fn log_batch_completion(total_duration_secs: f64) {
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

fn log_prover_backend_selection(prover_backend: ProverBackend) {
    match prover_backend {
        ProverBackend::Mock => tracing::info!("Using the Mock prover for instant execution"),
        ProverBackend::Cpu => tracing::info!("Using the CPU prover"),
    }
}
