//! The controller runtime: run-scoped batch coordination.
//!
//! One [`ControllerRuntime`] owns the header database, the persistent run
//! ledger, artifact storage, and (optionally) a proof verifier. Hosts drive
//! it exclusively through two pull-based operations, in encoded protocol
//! frames ([`crate::protocol`]) regardless of transport:
//!
//! * [`ControllerRuntime::handle_pull_batch`] — grant the next batch of a
//!   run, long-polling while too much handed-out work is still unverified.
//! * [`ControllerRuntime::handle_complete`] — accept a batch's public
//!   values, validate them (and the uploaded proof, when a verifier is
//!   configured), persist progress, and wake waiting pulls.
//!
//! # Pipelining
//!
//! At most [`MAX_UNVERIFIED_BATCHES`] batches may be handed out beyond the
//! verified tip. In the steady state that is exactly the double-buffered
//! pipeline: the guest is proving batch N, the host holds batch N+1 in RAM,
//! and the host's pull for batch N+2 long-polls until the completion of
//! batch N lands. After every grant the controller immediately pre-formats
//! the following batch in memory, so a pull is answered from RAM.
//!
//! # Crash and restart behavior
//!
//! Only verified progress is persisted (see [`crate::ledger`]). Handed-out
//! but unverified work lives in memory and is simply re-issued: when a host
//! reports (via `held_tip`) that it can only resume from the verified tip,
//! outstanding handed batches are dropped and re-served.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;

use zkpow_core::{
    BlockHash, BlockTimestamp, HeaderChainPublicValues, MinimalPublicValues, NewHeader, State,
    VerifierKeyDigest,
};
use zkpow_store::HeaderStore;

use crate::diagnostics::format_claim_mismatch;
use crate::digest::continuation_digest_from_state;
use crate::ledger::{Ledger, RunRecord};
use crate::protocol::{
    decode_complete_request, decode_pull_batch_request, encode_batch_grant, encode_complete_ack,
    BatchGrant, CompleteAck, CompleteRequest, ControllerError, PullBatchRequest,
};
use crate::storage::ArtifactStore;
use crate::verify::ProofVerifier;
use crate::NO_HEADERS_REMAINING_PREFIX;

/// Maximum batches handed out beyond the verified tip. Two is exactly the
/// double-buffered pipeline: one proving on the GPU, one staged in host RAM,
/// while the completion of the batch before them is in flight.
pub const MAX_UNVERIFIED_BATCHES: usize = 2;

const DEFAULT_PULL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub genesis_hash: BlockHash,
    /// Accept completions that did not upload a proof (execute-only runs).
    pub allow_unproven: bool,
    /// Upper bound on how long a pull long-polls for the previous batch to
    /// verify before returning an error.
    pub pull_timeout: Duration,
}

impl ControllerConfig {
    pub fn new(genesis_hash: BlockHash) -> Self {
        Self { genesis_hash, allow_unproven: false, pull_timeout: DEFAULT_PULL_TIMEOUT }
    }
}

/// The data needed to build one batch's SP1 stdin witness plus the
/// controller-only expectation for validating its completion.
#[derive(Debug, Clone)]
pub struct FetchedBatch {
    pub current_state: State,
    pub headers: Vec<NewHeader>,
    pub median_hints: Vec<BlockTimestamp>,
    pub expected_state: State,
    pub expected_continuation_digest: [u8; 32],
    pub first_new_height: u32,
    pub end_height: u32,
}

/// A batch granted to the host whose completion has not been verified yet.
struct HandedBatch {
    end_height: u32,
    expected_state: State,
    expected_continuation_digest: [u8; 32],
    proof_object: String,
}

/// The next batch pre-formatted in controller RAM (the read-ahead buffer).
struct PreparedBatch {
    /// `num_headers` the batch was built for; a pull requesting a different
    /// size recomputes instead.
    requested: u32,
    fetched: FetchedBatch,
}

struct RunSession {
    verifier_key: VerifierKeyDigest,
    verified_state: State,
    /// Storage object of the proof at `verified_state.height`, when one exists.
    verified_proof_object: Option<String>,
    handed: VecDeque<HandedBatch>,
    prepared: Option<PreparedBatch>,
    /// Batch size of the most recent pull; used by read-ahead preparation.
    batch_size: u32,
    /// Fatal validation failure: the run refuses all work until an operator
    /// intervenes (in-memory only; a controller restart clears it).
    halted: Option<String>,
}

impl RunSession {
    fn tip_state(&self) -> &State {
        self.handed.back().map(|batch| &batch.expected_state).unwrap_or(&self.verified_state)
    }

    fn tip_height(&self) -> u32 {
        self.tip_state().height
    }
}

struct ControllerCore {
    store: HeaderStore,
    ledger: Ledger,
    genesis_hash: BlockHash,
    runs: HashMap<String, RunSession>,
}

enum TryPull {
    Ready(Box<ReadyGrant>),
    /// Too many unverified handed-out batches; long-poll for progress.
    Wait,
}

struct ReadyGrant {
    fetched: FetchedBatch,
    proof_object: String,
    groth16_object: String,
    /// Object name of the verified-tip proof the host must download first.
    fetch_prev: Option<String>,
}

struct RuntimeInner {
    core: Mutex<ControllerCore>,
    /// Woken on every verified completion (and on halt) so long-polling
    /// pulls re-check their run.
    progress: Notify,
    storage: Box<dyn ArtifactStore>,
    verifier: Option<Box<dyn ProofVerifier>>,
    config: ControllerConfig,
}

/// Cheaply clonable handle; all clones share one controller state.
#[derive(Clone)]
pub struct ControllerRuntime {
    inner: Arc<RuntimeInner>,
}

/// A request that panics while holding this lock (e.g. an out-of-range
/// height hitting a `.expect()` deep in `HeaderStore`) poisons a plain
/// `Mutex::lock().unwrap()`, which would otherwise make every later request
/// panic too and take the whole controller down permanently. Such panics
/// happen within read-only queries before any session field is mutated, so
/// recovering the guard from a poisoned lock is sound.
fn lock_core(core: &Mutex<ControllerCore>) -> MutexGuard<'_, ControllerCore> {
    core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ControllerRuntime {
    pub fn new(
        store: HeaderStore,
        ledger: Ledger,
        storage: Box<dyn ArtifactStore>,
        verifier: Option<Box<dyn ProofVerifier>>,
        config: ControllerConfig,
    ) -> Self {
        let genesis_hash = config.genesis_hash;
        Self {
            inner: Arc::new(RuntimeInner {
                core: Mutex::new(ControllerCore {
                    store,
                    ledger,
                    genesis_hash,
                    runs: HashMap::new(),
                }),
                progress: Notify::new(),
                storage,
                verifier,
                config,
            }),
        }
    }

    /// Decode, serve, and encode a pull-batch request. Shared verbatim by
    /// the HTTP server and the host's in-process local transport.
    pub async fn handle_pull_batch(&self, request: &[u8]) -> Result<Vec<u8>, ControllerError> {
        let request = decode_pull_batch_request(request).map_err(ControllerError::bad_request)?;
        let grant = self.pull_batch(&request).await?;
        encode_batch_grant(&grant).map_err(ControllerError::internal)
    }

    /// Decode, apply, and encode a completion request. Shared verbatim by
    /// the HTTP server and the host's in-process local transport.
    pub async fn handle_complete(&self, request: &[u8]) -> Result<Vec<u8>, ControllerError> {
        let request = decode_complete_request(request).map_err(ControllerError::bad_request)?;
        let ack = self.complete(&request).await?;
        Ok(encode_complete_ack(&ack))
    }

    pub async fn pull_batch(
        &self,
        request: &PullBatchRequest,
    ) -> Result<BatchGrant, ControllerError> {
        let deadline = tokio::time::Instant::now() + self.inner.config.pull_timeout;
        loop {
            // Register interest in progress notifications *before* checking
            // state, so a completion landing between the check and the await
            // cannot be missed.
            let notified = self.inner.progress.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            // The guard must not be held across the long-poll await below,
            // both for lock hygiene and because completions need the lock to
            // make the progress being waited on.
            let attempt = {
                let mut core = lock_core(&self.inner.core);
                core.try_pull(request)
            };
            match attempt? {
                TryPull::Ready(ready) => {
                    let grant = self.build_grant(&request.run_id, *ready)?;
                    self.spawn_prepare_ahead(request.run_id.clone());
                    tracing::info!(
                        run_id = %request.run_id,
                        first_new_height = grant.first_new_height,
                        end_height = grant.end_height,
                        "granted batch"
                    );
                    return Ok(grant);
                }
                TryPull::Wait => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(ControllerError::timeout(format!(
                            "previous batch for run {} was not verified within {:?}",
                            request.run_id, self.inner.config.pull_timeout
                        )));
                    }
                    // Wake on progress or deadline, then re-check.
                    let _ = tokio::time::timeout_at(deadline, notified).await;
                }
            }
        }
    }

    pub async fn complete(
        &self,
        request: &CompleteRequest,
    ) -> Result<CompleteAck, ControllerError> {
        // Phase 1: validate the committed public values against the
        // database-derived expectation for the oldest handed-out batch.
        let (proof_object, expected_pv) = {
            let mut core = lock_core(&self.inner.core);
            let allow_unproven = self.inner.config.allow_unproven;
            let session = core.session_mut(&request.run_id)?;
            let front = session.handed.front().ok_or_else(|| {
                ControllerError::bad_request(format!(
                    "run {} has no pending batch to complete",
                    request.run_id
                ))
            })?;
            if front.end_height != request.end_height {
                return Err(ControllerError::bad_request(format!(
                    "completion reports end height {} but the pending batch ends at {}",
                    request.end_height, front.end_height
                )));
            }
            if !request.has_proof && !allow_unproven {
                return Err(ControllerError::bad_request(
                    "controller requires an uploaded proof to complete a batch",
                ));
            }
            let expected_pv = MinimalPublicValues::success(
                &front.expected_state.public_claim(),
                front.expected_continuation_digest,
                session.verifier_key,
            )
            .to_bytes();
            if request.public_values != expected_pv {
                let message = describe_public_values_rejection(
                    &request.public_values,
                    &front.expected_state,
                );
                session.halted = Some(message.clone());
                drop(core);
                self.inner.progress.notify_waiters();
                return Err(ControllerError::bad_request(message));
            }
            (front.proof_object.clone(), expected_pv)
        };

        // Phase 2: independently verify the uploaded proof, outside the lock
        // (storage fetch and STARK verification are slow).
        if request.has_proof {
            if let Some(verifier) = &self.inner.verifier {
                let result = self
                    .inner
                    .storage
                    .fetch(&request.run_id, &proof_object)
                    .map_err(|err| {
                        format!("failed to fetch uploaded proof {proof_object}: {err}")
                    })
                    .and_then(|bytes| verifier.verify_compressed(&bytes, &expected_pv));
                if let Err(err) = result {
                    let message =
                        format!("proof verification failed for {proof_object}: {err}");
                    if let Ok(session) =
                        lock_core(&self.inner.core).session_mut(&request.run_id)
                    {
                        session.halted = Some(message.clone());
                    }
                    self.inner.progress.notify_waiters();
                    return Err(ControllerError::bad_request(message));
                }
            }
        }

        // Phase 3: persist and advance.
        {
            let mut core = lock_core(&self.inner.core);
            let session = core.session_mut(&request.run_id)?;
            let front_end = session.handed.front().map(|front| front.end_height);
            if front_end != Some(request.end_height) {
                return Err(ControllerError::conflict(format!(
                    "pending batch for run {} changed while verifying height {}",
                    request.run_id, request.end_height
                )));
            }
            let expected_state = session.handed.front().expect("front checked").expected_state.clone();
            let proof_object = request.has_proof.then_some(proof_object);
            core.ledger
                .record_progress(&request.run_id, request.end_height, proof_object.as_deref())
                .map_err(|err| {
                    ControllerError::internal(format!("failed to persist run progress: {err}"))
                })?;
            let session = core.session_mut(&request.run_id)?;
            session.handed.pop_front();
            session.verified_state = expected_state;
            session.verified_proof_object = proof_object;
            tracing::info!(
                run_id = %request.run_id,
                verified_height = request.end_height,
                "batch verified"
            );
        }
        self.inner.progress.notify_waiters();
        Ok(CompleteAck { verified_height: request.end_height })
    }

    fn build_grant(
        &self,
        run_id: &str,
        ready: ReadyGrant,
    ) -> Result<BatchGrant, ControllerError> {
        let storage = &self.inner.storage;
        fn internal(context: &str, err: crate::BoxError) -> ControllerError {
            ControllerError::internal(format!("storage target error ({context}): {err}"))
        }
        let prev_proof_fetch = ready
            .fetch_prev
            .map(|object| {
                storage.get_target(run_id, &object).map_err(|err| internal("fetch prev", err))
            })
            .transpose()?;
        let proof_put = Some(
            storage
                .put_target(run_id, &ready.proof_object)
                .map_err(|err| internal("proof put", err))?,
        );
        let groth16_put = Some(
            storage
                .put_target(run_id, &ready.groth16_object)
                .map_err(|err| internal("groth16 put", err))?,
        );
        let FetchedBatch {
            current_state, headers, median_hints, first_new_height, end_height, ..
        } = ready.fetched;
        Ok(BatchGrant {
            first_new_height,
            end_height,
            current_state,
            headers,
            median_hints,
            prev_proof_fetch,
            proof_put,
            groth16_put,
        })
    }

    /// Pre-format the next batch of `run_id` into RAM so the following pull
    /// is answered without touching the database on the request path.
    fn spawn_prepare_ahead(&self, run_id: String) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            lock_core(&inner.core).prepare_ahead(&run_id);
        });
    }
}

impl ControllerCore {
    /// Reconstruct full consensus state at an arbitrary height.
    fn state_at(&self, height: u32) -> State {
        self.store.state_at_height(height, self.genesis_hash)
    }

    fn session_mut(&mut self, run_id: &str) -> Result<&mut RunSession, ControllerError> {
        self.runs.get_mut(run_id).ok_or_else(|| {
            ControllerError::bad_request(format!("run {run_id} has no active session"))
        })
    }

    /// Load `run_id`'s session into memory, creating the run (and its ledger
    /// row) on first contact.
    fn ensure_session(&mut self, request: &PullBatchRequest) -> Result<(), ControllerError> {
        if self.runs.contains_key(&request.run_id) {
            return Ok(());
        }
        let record = self
            .ledger
            .load(&request.run_id)
            .map_err(|err| ControllerError::internal(format!("ledger read failed: {err}")))?;
        let session = match record {
            Some(record) => {
                tracing::info!(
                    run_id = %request.run_id,
                    verified_height = record.verified_height,
                    "resuming run from ledger"
                );
                RunSession {
                    verifier_key: record.verifier_key,
                    verified_state: self.state_at(record.verified_height),
                    verified_proof_object: record.proof_object,
                    handed: VecDeque::new(),
                    prepared: None,
                    batch_size: request.num_headers,
                    halted: None,
                }
            }
            None => {
                let start_height = request.start_height.unwrap_or(0);
                let verified_state = self.state_at(start_height);
                self.ledger
                    .create(&RunRecord {
                        run_id: request.run_id.clone(),
                        verifier_key: request.verifier_key,
                        verified_height: start_height,
                        proof_object: None,
                    })
                    .map_err(|err| {
                        ControllerError::internal(format!("ledger create failed: {err}"))
                    })?;
                tracing::info!(run_id = %request.run_id, start_height, "created new run");
                RunSession {
                    verifier_key: request.verifier_key,
                    verified_state,
                    verified_proof_object: None,
                    handed: VecDeque::new(),
                    prepared: None,
                    batch_size: request.num_headers,
                    halted: None,
                }
            }
        };
        self.runs.insert(request.run_id.clone(), session);
        Ok(())
    }

    fn try_pull(&mut self, request: &PullBatchRequest) -> Result<TryPull, ControllerError> {
        self.ensure_session(request)?;

        // Phase 1: reconcile the host against the run, decide whether a
        // batch may be served right now.
        let (tip_state, prepared, fetch_prev) = {
            let session = self.session_mut(&request.run_id)?;
            if session.verifier_key != request.verifier_key {
                return Err(ControllerError::conflict(format!(
                    "run {} is pinned to a different verifier key",
                    request.run_id
                )));
            }
            if let Some(reason) = &session.halted {
                return Err(ControllerError::conflict(format!(
                    "run {} is halted: {reason}",
                    request.run_id
                )));
            }
            session.batch_size = request.num_headers;

            let tip_height = session.tip_height();
            let verified_height = session.verified_state.height;
            // A host holding nothing is equivalent to holding the (nonexistent)
            // genesis proof: nothing needs fetching to continue from height 0,
            // and execute-only runs never hold proofs at all.
            let holds_tip = match request.held_tip {
                Some(held) => held == tip_height,
                None => tip_height == verified_height,
            };
            let mut fetch_prev = None;
            if !holds_tip {
                let can_reset = request.held_tip.is_none()
                    || request.held_tip == Some(verified_height);
                if !can_reset {
                    return Err(ControllerError::conflict(format!(
                        "host holds proof for height {:?} but run {} is at height {} \
                         (verified through {})",
                        request.held_tip, request.run_id, tip_height, verified_height
                    )));
                }
                if !session.handed.is_empty() {
                    tracing::warn!(
                        run_id = %request.run_id,
                        dropped = session.handed.len(),
                        "dropping unverified handed-out batches; re-serving from verified tip"
                    );
                    session.handed.clear();
                }
                session.prepared = None;
            }
            if request.held_tip.is_none() && verified_height > 0 {
                fetch_prev = session.verified_proof_object.clone();
            }

            if session.handed.len() >= MAX_UNVERIFIED_BATCHES {
                return Ok(TryPull::Wait);
            }
            (session.tip_state().clone(), session.prepared.take(), fetch_prev)
        };

        // Phase 2: use the read-ahead buffer when it matches, otherwise
        // format the batch from the database now.
        let fetched = match prepared {
            Some(prepared)
                if prepared.requested == request.num_headers
                    && prepared.fetched.current_state == tip_state =>
            {
                prepared.fetched
            }
            _ => self.fetch_batch(&tip_state, request.num_headers)?,
        };

        // Phase 3: record the handed-out batch and its expectations.
        let proof_object = format!(
            "proof_height_{}_to_{}.bin",
            fetched.first_new_height, fetched.end_height
        );
        let groth16_object = format!(
            "proof_height_{}_to_{}_groth16.bin",
            fetched.first_new_height, fetched.end_height
        );
        let session = self.session_mut(&request.run_id)?;
        session.handed.push_back(HandedBatch {
            end_height: fetched.end_height,
            expected_state: fetched.expected_state.clone(),
            expected_continuation_digest: fetched.expected_continuation_digest,
            proof_object: proof_object.clone(),
        });
        Ok(TryPull::Ready(Box::new(ReadyGrant {
            fetched,
            proof_object,
            groth16_object,
            fetch_prev,
        })))
    }

    fn prepare_ahead(&mut self, run_id: &str) {
        let Some(session) = self.runs.get(run_id) else { return };
        if session.halted.is_some() || session.prepared.is_some() {
            return;
        }
        let tip_state = session.tip_state().clone();
        let requested = session.batch_size;
        // Header exhaustion (or any fetch failure) is not an error here; the
        // next pull recomputes and reports it on the request path.
        if let Ok(fetched) = self.fetch_batch(&tip_state, requested) {
            if let Some(session) = self.runs.get_mut(run_id) {
                if session.prepared.is_none() {
                    session.prepared = Some(PreparedBatch { requested, fetched });
                }
            }
        }
    }

    /// Fetch the `num_headers` headers after `current_state` and compute the
    /// expected end state.
    fn fetch_batch(
        &self,
        current_state: &State,
        num_headers: u32,
    ) -> Result<FetchedBatch, ControllerError> {
        let first_new_height = current_state.height + 1;
        let witness =
            self.store.load_header_batch_witness(first_new_height as u64, num_headers as u64);
        if witness.headers.is_empty() {
            return Err(ControllerError::exhausted(format!(
                "{NO_HEADERS_REMAINING_PREFIX}: starting at height {first_new_height}"
            )));
        }
        let loaded_count = witness.headers.len() as u32;
        let end_height = current_state.height + loaded_count;

        let expected_state = self.state_at(end_height);
        let expected_continuation_digest = continuation_digest_from_state(&expected_state);

        Ok(FetchedBatch {
            current_state: current_state.clone(),
            headers: witness.headers,
            median_hints: witness.median_time_past_hints,
            expected_state,
            expected_continuation_digest,
            first_new_height,
            end_height,
        })
    }
}

fn describe_public_values_rejection(public_values: &[u8], expected_state: &State) -> String {
    match HeaderChainPublicValues::parse(public_values) {
        Ok(HeaderChainPublicValues::Success { claim, .. }) => {
            if claim == expected_state.public_claim() {
                "guest public values mismatch: claim matches but continuation digest or \
                 verifier key differs"
                    .to_string()
            } else {
                format!(
                    "guest public values mismatch:\n{}",
                    format_claim_mismatch(&expected_state.public_claim(), &claim)
                )
            }
        }
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => format!(
            "guest ended in error {} at height {}",
            failure.error_code, failure.failure_height
        ),
        Err(err) => format!("malformed public values: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Target;
    use crate::storage::LocalArtifactStore;
    use zkpow_core::sha256d;
    use zkpow_store::db_path;

    fn temp_path(test_name: &str, suffix: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zkpow-runtime-{test_name}-{}-{nonce}{suffix}",
            std::process::id()
        ))
    }

    fn genesis_hash() -> BlockHash {
        let db = HeaderStore::open(db_path()).expect("failed to open test database");
        BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()))
    }

    struct TestRuntime {
        runtime: ControllerRuntime,
        ledger_path: std::path::PathBuf,
        artifact_root: std::path::PathBuf,
    }

    fn test_runtime(test_name: &str, configure: impl FnOnce(&mut ControllerConfig)) -> TestRuntime {
        let ledger_path = temp_path(test_name, ".db");
        let artifact_root = temp_path(test_name, "-artifacts");
        TestRuntime {
            runtime: runtime_at(&ledger_path, &artifact_root, configure),
            ledger_path,
            artifact_root,
        }
    }

    fn runtime_at(
        ledger_path: &std::path::Path,
        artifact_root: &std::path::Path,
        configure: impl FnOnce(&mut ControllerConfig),
    ) -> ControllerRuntime {
        let store = HeaderStore::open(db_path()).expect("failed to open test database");
        let ledger = Ledger::open(ledger_path).expect("failed to open ledger");
        let mut config = ControllerConfig::new(genesis_hash());
        config.allow_unproven = true;
        config.pull_timeout = Duration::from_millis(100);
        configure(&mut config);
        ControllerRuntime::new(
            store,
            ledger,
            Box::new(LocalArtifactStore::new(artifact_root)),
            None,
            config,
        )
    }

    impl Drop for TestRuntime {
        fn drop(&mut self) {
            std::fs::remove_file(&self.ledger_path).ok();
            std::fs::remove_dir_all(&self.artifact_root).ok();
        }
    }

    fn pull_request(run_id: &str, num_headers: u32, held_tip: Option<u32>) -> PullBatchRequest {
        PullBatchRequest {
            run_id: run_id.to_string(),
            verifier_key: VerifierKeyDigest::default(),
            num_headers,
            held_tip,
            start_height: None,
        }
    }

    fn success_pv_for(end_height: u32) -> Vec<u8> {
        let db = HeaderStore::open(db_path()).expect("failed to open test database");
        let state = db.state_at_height(end_height, genesis_hash());
        MinimalPublicValues::success(
            &state.public_claim(),
            continuation_digest_from_state(&state),
            VerifierKeyDigest::default(),
        )
        .to_bytes()
        .to_vec()
    }

    fn complete_request(run_id: &str, end_height: u32, has_proof: bool) -> CompleteRequest {
        CompleteRequest {
            run_id: run_id.to_string(),
            end_height,
            has_proof,
            public_values: success_pv_for(end_height),
        }
    }

    #[tokio::test]
    async fn new_run_starts_at_genesis_and_advances() {
        let harness = test_runtime("advance", |_| {});
        let runtime = &harness.runtime;
        let run = "run-advance";

        let grant = runtime.pull_batch(&pull_request(run, 3, None)).await.unwrap();
        assert_eq!(grant.first_new_height, 1);
        assert_eq!(grant.end_height, 3);
        assert_eq!(grant.current_state.height, 0);
        assert_eq!(grant.headers.len(), 3);
        assert!(grant.prev_proof_fetch.is_none());
        assert!(matches!(grant.proof_put, Some(Target::Path(_))));

        let ack = runtime.complete(&complete_request(run, 3, false)).await.unwrap();
        assert_eq!(ack.verified_height, 3);

        let next = runtime.pull_batch(&pull_request(run, 3, None)).await.unwrap();
        assert_eq!(next.first_new_height, 4);
        assert_eq!(next.current_state.height, 3);
    }

    #[tokio::test]
    async fn pipeline_allows_two_outstanding_batches_then_waits() {
        let harness = test_runtime("outstanding", |_| {});
        let runtime = &harness.runtime;
        let run = "run-outstanding";

        let first = runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        let second =
            runtime.pull_batch(&pull_request(run, 2, Some(first.end_height))).await.unwrap();
        assert_eq!(second.first_new_height, 3);
        assert_eq!(second.current_state.height, first.end_height);

        // Third pull must long-poll and time out: nothing has been verified.
        let err = runtime
            .pull_batch(&pull_request(run, 2, Some(second.end_height)))
            .await
            .unwrap_err();
        assert_eq!(err.kind, crate::protocol::ErrorKind::Timeout);

        // Completing the first batch unblocks the third pull.
        runtime.complete(&complete_request(run, 2, false)).await.unwrap();
        let third = runtime
            .pull_batch(&pull_request(run, 2, Some(second.end_height)))
            .await
            .unwrap();
        assert_eq!(third.first_new_height, 5);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn long_polling_pull_wakes_on_completion() {
        let harness = test_runtime("wake", |config| {
            config.pull_timeout = Duration::from_secs(5);
        });
        let runtime = harness.runtime.clone();
        let run = "run-wake";

        let first = runtime.pull_batch(&pull_request(run, 1, None)).await.unwrap();
        let second =
            runtime.pull_batch(&pull_request(run, 1, Some(first.end_height))).await.unwrap();

        let poller = {
            let runtime = runtime.clone();
            let held = second.end_height;
            tokio::spawn(async move {
                runtime.pull_batch(&pull_request("run-wake", 1, Some(held))).await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        runtime.complete(&complete_request(run, first.end_height, false)).await.unwrap();

        let third = poller.await.unwrap().unwrap();
        assert_eq!(third.first_new_height, second.end_height + 1);
    }

    #[tokio::test]
    async fn mismatched_public_values_halt_the_run() {
        let harness = test_runtime("halt", |_| {});
        let runtime = &harness.runtime;
        let run = "run-halt";

        let grant = runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        // Public values for the wrong height: a valid claim that does not
        // match the pending batch's expected end state.
        let mut complete = complete_request(run, grant.end_height, false);
        complete.public_values = success_pv_for(1);

        let err = runtime.complete(&complete).await.unwrap_err();
        assert!(err.message.contains("mismatch"), "unexpected error: {err}");

        let err =
            runtime.pull_batch(&pull_request(run, 2, Some(grant.end_height))).await.unwrap_err();
        assert_eq!(err.kind, crate::protocol::ErrorKind::Conflict);
        assert!(err.message.contains("halted"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn verifier_key_is_pinned_per_run() {
        let harness = test_runtime("vk-pin", |_| {});
        let runtime = &harness.runtime;
        let run = "run-vk-pin";

        runtime.pull_batch(&pull_request(run, 1, None)).await.unwrap();

        let mut request = pull_request(run, 1, Some(1));
        request.verifier_key = VerifierKeyDigest::from_raw([9; 8]);
        let err = runtime.pull_batch(&request).await.unwrap_err();
        assert!(err.message.contains("verifier key"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn unproven_completion_is_rejected_unless_allowed() {
        let harness = test_runtime("require-proof", |config| {
            config.allow_unproven = false;
        });
        let runtime = &harness.runtime;
        let run = "run-require-proof";

        runtime.pull_batch(&pull_request(run, 1, None)).await.unwrap();
        let err = runtime.complete(&complete_request(run, 1, false)).await.unwrap_err();
        assert!(err.message.contains("requires an uploaded proof"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn run_resumes_from_ledger_with_proof_fetch_target() {
        let harness = test_runtime("resume", |_| {});
        let run = "run-resume";

        let grant = harness.runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        harness
            .runtime
            .complete(&complete_request(run, grant.end_height, true))
            .await
            .unwrap();

        // A fresh controller (same ledger) serves the next batch and tells
        // the empty-handed host where to fetch the verified-tip proof.
        let reborn = runtime_at(&harness.ledger_path, &harness.artifact_root, |_| {});
        let next = reborn.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        assert_eq!(next.first_new_height, grant.end_height + 1);
        let fetch = next.prev_proof_fetch.expect("resume should include a proof fetch target");
        let Target::Path(path) = fetch else { panic!("expected path target") };
        assert!(
            path.contains(&format!("proof_height_1_to_{}.bin", grant.end_height)),
            "unexpected fetch target: {path}"
        );
    }

    #[tokio::test]
    async fn host_restart_reissues_unverified_batches() {
        let harness = test_runtime("reissue", |_| {});
        let runtime = &harness.runtime;
        let run = "run-reissue";

        let first = runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        let _second =
            runtime.pull_batch(&pull_request(run, 2, Some(first.end_height))).await.unwrap();

        // Host restarts empty-handed: outstanding work is dropped and the
        // first batch is re-served from the verified tip (genesis).
        let reissued = runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        assert_eq!(reissued.first_new_height, 1);
        assert_eq!(reissued.end_height, first.end_height);
    }

    #[tokio::test]
    async fn foreign_held_tip_is_rejected() {
        let harness = test_runtime("foreign-tip", |_| {});
        let runtime = &harness.runtime;
        let run = "run-foreign-tip";

        runtime.pull_batch(&pull_request(run, 2, None)).await.unwrap();
        let err = runtime.pull_batch(&pull_request(run, 2, Some(999))).await.unwrap_err();
        assert_eq!(err.kind, crate::protocol::ErrorKind::Conflict);
    }

    #[tokio::test]
    async fn exhausted_headers_error_uses_stable_prefix() {
        let harness = test_runtime("exhausted", |_| {});
        let err = harness
            .runtime
            .pull_batch(&pull_request("run-exhausted", 0, None))
            .await
            .unwrap_err();
        assert_eq!(err.kind, crate::protocol::ErrorKind::Exhausted);
        assert!(err.message.starts_with(NO_HEADERS_REMAINING_PREFIX));
    }

    #[tokio::test]
    async fn trusted_start_height_seeds_new_runs() {
        let harness = test_runtime("trusted-start", |_| {});
        let mut request = pull_request("run-trusted-start", 1, None);
        request.start_height = Some(2016);

        let grant = harness.runtime.pull_batch(&request).await.unwrap();
        assert_eq!(grant.first_new_height, 2017);
        assert_eq!(grant.current_state.height, 2016);
        // No proof exists to fetch: the run starts from a trusted state.
        assert!(grant.prev_proof_fetch.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panicking_request_does_not_poison_the_controller() {
        let harness = test_runtime("poison", |_| {});
        let runtime = harness.runtime.clone();

        let mut request = pull_request("run-poison", 1, None);
        request.start_height = Some(u32::MAX);
        let panicked = {
            let runtime = runtime.clone();
            tokio::spawn(async move { runtime.pull_batch(&request).await })
        }
        .await
        .is_err();
        assert!(panicked, "expected an out-of-range start height to panic");

        // A later request recovers the poisoned lock and succeeds.
        runtime.pull_batch(&pull_request("run-poison-2", 1, None)).await.unwrap();
    }

    #[tokio::test]
    async fn handle_round_trip_matches_typed_api() {
        let harness = test_runtime("bytes", |_| {});
        let runtime = &harness.runtime;

        let request = pull_request("run-bytes", 2, None);
        let encoded = crate::protocol::encode_pull_batch_request(&request).unwrap();
        let grant_bytes = runtime.handle_pull_batch(&encoded).await.unwrap();
        let grant = crate::protocol::decode_batch_grant(&grant_bytes).unwrap();
        assert_eq!(grant.first_new_height, 1);
        assert_eq!(grant.end_height, 2);

        let complete = complete_request("run-bytes", 2, false);
        let encoded = crate::protocol::encode_complete_request(&complete).unwrap();
        let ack_bytes = runtime.handle_complete(&encoded).await.unwrap();
        let ack = crate::protocol::decode_complete_ack(&ack_bytes).unwrap();
        assert_eq!(ack.verified_height, 2);
    }
}
