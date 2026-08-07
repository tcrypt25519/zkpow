//! Stateful run coordination for proving hosts.
//!
//! The host identifies a run and nothing else. The controller selects the
//! batch, verifier key, recursive witness, artifact names, and expected public
//! values. Completion means the selected proof has already been uploaded.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use tokio::sync::Notify;
use zkpow_core::{serialize_inputs, serialize_minimal_public_values, BlockHash, State, VerifierKeyDigest};
use zkpow_store::{HeaderStore, RunRecord};

use crate::{
    digest::continuation_digest_from_state,
    protocol::{Batch, ControllerError},
    storage::ArtifactStore,
    verify::ProofVerifier,
    NO_HEADERS_REMAINING_PREFIX,
};

pub const MAX_OUTSTANDING_BATCHES: usize = 2;
const DEFAULT_PULL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub genesis_hash: BlockHash,
    pub batch_size: u32,
    pub pull_timeout: Duration,
}

impl ControllerConfig {
    pub fn new(genesis_hash: BlockHash, batch_size: u32) -> Self {
        Self {
            genesis_hash,
            batch_size,
            pull_timeout: DEFAULT_PULL_TIMEOUT,
        }
    }
}

struct FetchedBatch {
    current_state: State,
    header_bytes: Vec<u8>,
    median_hint_bytes: Vec<u8>,
    expected_state: State,
    expected_public_values: Vec<u8>,
    first_new_height: u32,
    end_height: u32,
}

struct PendingBatch {
    end_height: u32,
    expected_state: State,
    expected_public_values: Vec<u8>,
    proof_object: String,
}

struct PreparedBatch {
    current_state: State,
    result: Result<FetchedBatch, ControllerError>,
}

struct RunSession {
    verified_state: State,
    verified_proof_object: Option<String>,
    pending: VecDeque<PendingBatch>,
    prepared: Option<PreparedBatch>,
    halted: Option<String>,
}

impl RunSession {
    fn tip_state(&self) -> State {
        self.pending
            .back()
            .map(|batch| batch.expected_state)
            .unwrap_or(self.verified_state)
    }

    fn tip_proof_object(&self) -> Option<String> {
        self.pending
            .back()
            .map(|batch| batch.proof_object.clone())
            .or_else(|| self.verified_proof_object.clone())
    }
}

struct ControllerCore {
    store: HeaderStore,
    genesis_hash: BlockHash,
    verifier_key: VerifierKeyDigest,
    batch_size: u32,
    runs: HashMap<String, RunSession>,
}

enum Pull {
    Ready(Box<ReadyBatch>),
    Wait,
}

struct ReadyBatch {
    fetched: FetchedBatch,
    proof_object: String,
    previous_proof_object: Option<String>,
}

struct RuntimeInner {
    core: Mutex<ControllerCore>,
    progress: Notify,
    storage: Arc<dyn ArtifactStore>,
    verifier: Arc<dyn ProofVerifier>,
    verifier_key: VerifierKeyDigest,
    pull_timeout: Duration,
}

#[derive(Clone)]
pub struct ControllerRuntime {
    inner: Arc<RuntimeInner>,
}

fn lock_core(core: &Mutex<ControllerCore>) -> MutexGuard<'_, ControllerCore> {
    core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn validate_run_id(run_id: &str) -> Result<(), ControllerError> {
    if run_id.is_empty() {
        return Err(ControllerError::bad_request("run id must not be empty"));
    }
    Ok(())
}

impl ControllerRuntime {
    pub fn new(
        store: HeaderStore,
        storage: Box<dyn ArtifactStore>,
        verifier: Box<dyn ProofVerifier>,
        config: ControllerConfig,
    ) -> Self {
        let verifier_key = verifier.verifier_key_digest();
        Self {
            inner: Arc::new(RuntimeInner {
                core: Mutex::new(ControllerCore {
                    store,
                    genesis_hash: config.genesis_hash,
                    verifier_key,
                    batch_size: config.batch_size,
                    runs: HashMap::new(),
                }),
                progress: Notify::new(),
                storage: storage.into(),
                verifier: verifier.into(),
                verifier_key,
                pull_timeout: config.pull_timeout,
            }),
        }
    }

    pub async fn pull_batch(&self, run_id: &str) -> Result<Batch, ControllerError> {
        validate_run_id(run_id)?;
        let deadline = tokio::time::Instant::now() + self.inner.pull_timeout;
        loop {
            let notified = self.inner.progress.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let pull = lock_core(&self.inner.core).pull(run_id)?;
            match pull {
                Pull::Ready(ready) => {
                    let batch = self.attach_storage(run_id, *ready)?;
                    self.prepare_ahead(run_id.to_owned());
                    tracing::info!(
                        run_id,
                        first_new_height = batch.first_new_height,
                        end_height = batch.end_height,
                        "serving batch"
                    );
                    return Ok(batch);
                },
                Pull::Wait if tokio::time::Instant::now() >= deadline => {
                    return Err(ControllerError::timeout(format!(
                        "run {run_id} did not verify its oldest pending proof within {:?}",
                        self.inner.pull_timeout
                    )));
                },
                Pull::Wait => {
                    let _ = tokio::time::timeout_at(deadline, notified).await;
                },
            }
        }
    }

    pub async fn complete(&self, run_id: &str) -> Result<(), ControllerError> {
        validate_run_id(run_id)?;
        let (end_height, proof_object, expected_public_values) = {
            let mut core = lock_core(&self.inner.core);
            let session = core.session_mut(run_id)?;
            let pending = session.pending.front().ok_or_else(|| {
                ControllerError::bad_request(format!("run {run_id} has no pending batch to complete"))
            })?;
            (
                pending.end_height,
                pending.proof_object.clone(),
                pending.expected_public_values.clone(),
            )
        };

        let storage = self.inner.storage.clone();
        let verifier = self.inner.verifier.clone();
        let verification_run = run_id.to_owned();
        let verification_object = proof_object.clone();
        let verification = tokio::task::spawn_blocking(move || {
            storage
                .fetch(&verification_run, &verification_object)
                .map_err(|err| format!("failed to fetch uploaded proof {verification_object}: {err}"))
                .and_then(|proof| verifier.verify_compressed(&proof, &expected_public_values))
        })
        .await
        .map_err(|err| ControllerError::internal(format!("proof verifier task failed: {err}")))?;
        if let Err(err) = verification {
            let message = format!("completion verification failed for {proof_object}: {err}");
            if let Ok(session) = lock_core(&self.inner.core).session_mut(run_id) {
                session.halted = Some(message.clone());
            }
            self.inner.progress.notify_waiters();
            return Err(ControllerError::bad_request(message));
        }

        {
            let mut core = lock_core(&self.inner.core);
            let session = core.session_mut(run_id)?;
            if session.pending.front().map(|batch| batch.end_height) != Some(end_height) {
                return Err(ControllerError::conflict(format!(
                    "pending batch for run {run_id} changed while verifying"
                )));
            }
            let expected_state = session.pending.front().expect("front checked").expected_state;
            core.store
                .record_run_progress(run_id, end_height, Some(&proof_object))
                .map_err(|err| ControllerError::internal(format!("failed to persist run progress: {err}")))?;
            let session = core.session_mut(run_id)?;
            session.pending.pop_front();
            session.verified_state = expected_state;
            session.verified_proof_object = Some(proof_object);
        }
        self.inner.progress.notify_waiters();
        tracing::info!(run_id, verified_height = end_height, "batch verified");
        Ok(())
    }

    fn attach_storage(&self, run_id: &str, ready: ReadyBatch) -> Result<Batch, ControllerError> {
        let storage_error = |operation: &str, err: crate::BoxError| {
            ControllerError::internal(format!("storage {operation} failed: {err}"))
        };
        let previous_proof = ready
            .previous_proof_object
            .map(|object| {
                self.inner
                    .storage
                    .get_target(run_id, &object)
                    .map_err(|err| storage_error("GET target", err))
            })
            .transpose()?;
        let proof_upload = self
            .inner
            .storage
            .put_target(run_id, &ready.proof_object)
            .map_err(|err| storage_error("PUT target", err))?;
        let fetched = ready.fetched;
        Ok(Batch {
            first_new_height: fetched.first_new_height,
            end_height: fetched.end_height,
            verifier_key: self.inner.verifier_key,
            current_state: fetched.current_state,
            header_bytes: fetched.header_bytes,
            median_hint_bytes: fetched.median_hint_bytes,
            previous_proof,
            proof_upload,
        })
    }

    fn prepare_ahead(&self, run_id: String) {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            lock_core(&inner.core).prepare_ahead(&run_id);
        });
    }
}

impl ControllerCore {
    fn state_at(&self, height: u32) -> State {
        self.store.state_at_height(height, self.genesis_hash)
    }

    fn session_mut(&mut self, run_id: &str) -> Result<&mut RunSession, ControllerError> {
        self.runs
            .get_mut(run_id)
            .ok_or_else(|| ControllerError::bad_request(format!("run {run_id} has no active session")))
    }

    fn ensure_session(&mut self, run_id: &str) -> Result<(), ControllerError> {
        if self.runs.contains_key(run_id) {
            return Ok(());
        }
        let record = self
            .store
            .load_run(run_id)
            .map_err(|err| ControllerError::internal(format!("run ledger read failed: {err}")))?;
        let record = match record {
            Some(record) if record.verifier_key != self.verifier_key => {
                return Err(ControllerError::conflict(format!(
                    "run {run_id} belongs to a different controller verifier key"
                )));
            },
            Some(record) => record,
            None => {
                let record = RunRecord {
                    verifier_key: self.verifier_key,
                    verified_height: 0,
                    proof_object: None,
                };
                self.store
                    .create_run(run_id, &record)
                    .map_err(|err| ControllerError::internal(format!("run ledger create failed: {err}")))?;
                record
            },
        };
        self.runs.insert(
            run_id.to_owned(),
            RunSession {
                verified_state: self.state_at(record.verified_height),
                verified_proof_object: record.proof_object,
                pending: VecDeque::new(),
                prepared: None,
                halted: None,
            },
        );
        Ok(())
    }

    fn pull(&mut self, run_id: &str) -> Result<Pull, ControllerError> {
        self.ensure_session(run_id)?;
        let (tip_state, previous_proof_object, prepared) = {
            let session = self.session_mut(run_id)?;
            if let Some(reason) = &session.halted {
                return Err(ControllerError::conflict(format!("run {run_id} is halted: {reason}")));
            }
            if session.pending.len() >= MAX_OUTSTANDING_BATCHES {
                return Ok(Pull::Wait);
            }
            (session.tip_state(), session.tip_proof_object(), session.prepared.take())
        };

        let mut fetched = match prepared {
            Some(prepared) if prepared.current_state == tip_state => prepared.result?,
            _ => self.fetch_batch(tip_state)?,
        };
        let proof_object = format!("proof_{}.bin", fetched.end_height);
        let session = self.session_mut(run_id)?;
        session.pending.push_back(PendingBatch {
            end_height: fetched.end_height,
            expected_state: fetched.expected_state,
            expected_public_values: std::mem::take(&mut fetched.expected_public_values),
            proof_object: proof_object.clone(),
        });
        Ok(Pull::Ready(Box::new(ReadyBatch {
            fetched,
            proof_object,
            previous_proof_object,
        })))
    }

    fn prepare_ahead(&mut self, run_id: &str) {
        let Some(session) = self.runs.get(run_id) else {
            return;
        };
        if session.halted.is_some() || session.prepared.is_some() {
            return;
        }
        let current_state = session.tip_state();
        let result = self.fetch_batch(current_state);
        if let Some(session) = self.runs.get_mut(run_id) {
            session.prepared = Some(PreparedBatch { current_state, result });
        }
    }

    fn fetch_batch(&self, current_state: State) -> Result<FetchedBatch, ControllerError> {
        if self.batch_size == 0 {
            return Err(ControllerError::bad_request("controller batch size must be nonzero"));
        }
        let first_new_height = current_state.height + 1;
        let witness = self
            .store
            .load_header_batch_witness(first_new_height as u64, self.batch_size as u64);
        if witness.headers.is_empty() {
            return Err(ControllerError::exhausted(format!(
                "{NO_HEADERS_REMAINING_PREFIX}: starting at height {first_new_height}"
            )));
        }
        let end_height = current_state.height + witness.headers.len() as u32;
        let expected_state = self.state_at(end_height);
        let expected_claim = expected_state.public_claim();
        let expected_public_values = serialize_minimal_public_values(
            &expected_claim.chain_work,
            &self.verifier_key,
            &continuation_digest_from_state(&expected_state),
            &expected_claim.genesis_hash,
            &expected_claim.tip_hash,
            expected_claim.height,
            0,
        )
        .to_vec();
        Ok(FetchedBatch {
            current_state,
            header_bytes: serialize_inputs(&witness.headers),
            median_hint_bytes: serialize_inputs(&witness.median_time_past_hints),
            expected_state,
            expected_public_values,
            first_new_height,
            end_height,
        })
    }
}

