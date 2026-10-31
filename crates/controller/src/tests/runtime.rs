use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use zkpow_core::{PublicValues, ReturnCode, VerifierKeyDigest};
use zkpow_store::HeaderStore;

use super::*;
use crate::{
    protocol::Batch,
    runtime::{ControllerConfig, ControllerRuntime},
    storage::{ArtifactStore, LocalArtifactStore},
    verify::ProofVerifier,
};

const TEST_DATABASE_FILE: &str = "headers.sqlite";
const EMPTY_DATABASE_RUN_ID: &str = "anyrun01";

struct ControllableStore {
    inner:    LocalArtifactStore,
    fail_put: Arc<AtomicBool>,
    fail_get: Arc<AtomicBool>,
}

impl ArtifactStore for ControllableStore {
    fn put_target(&self, run_id: &str, object: &str) -> Result<String, crate::BoxError> {
        if self.fail_put.load(Ordering::SeqCst) {
            return Err("simulated PUT target failure".into());
        }
        self.inner.put_target(run_id, object)
    }

    fn get_target(&self, run_id: &str, object: &str) -> Result<String, crate::BoxError> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err("simulated GET target failure".into());
        }
        self.inner.get_target(run_id, object)
    }

    fn fetch(&self, run_id: &str, object: &str) -> Result<Vec<u8>, crate::BoxError> {
        self.inner.fetch(run_id, object)
    }
}

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

fn unique_nonce() -> u128 {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    (time << 32) | id as u128
}

struct BytesArePublicValues(VerifierKeyDigest);

impl ProofVerifier for BytesArePublicValues {
    fn verifier_key_digest(&self) -> VerifierKeyDigest {
        self.0
    }

    fn verify_compressed(&self, proof: &[u8], expected: &[u8]) -> Result<(), String> {
        (proof == expected)
            .then_some(())
            .ok_or_else(|| "wrong public values".to_string())
    }
}

struct Harness {
    runtime: ControllerRuntime<Mainnet>,
    root:    std::path::PathBuf,
    db_path: std::path::PathBuf,
    run_ids: Mutex<Vec<String>>,
}

impl Harness {
    fn new() -> Self {
        Self::with_faulty_storage(Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)))
    }

    fn with_faulty_storage(fail_put: Arc<AtomicBool>, fail_get: Arc<AtomicBool>) -> Self {
        let nonce = unique_nonce();
        let root = std::env::temp_dir().join(format!("zkpow-runtime-{nonce}"));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join(TEST_DATABASE_FILE);
        let (store, genesis_hash) = zkpow_store::test_utils::seeded_store_at(&db_path, 20);
        let mut config = ControllerConfig::new(genesis_hash, 2);
        config.pull_timeout = Duration::from_millis(100);
        let storage = ControllableStore {
            inner: LocalArtifactStore::new(&root),
            fail_put,
            fail_get,
        };
        let runtime = ControllerRuntime::new(
            store,
            Box::new(storage),
            Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
            config,
        );
        Self {
            runtime,
            root,
            db_path,
            run_ids: Mutex::new(Vec::new()),
        }
    }

    fn run(&self) -> String {
        let counter = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let run = format!("r{:07x}", counter & 0x0fffffff);
        self.run_ids.lock().unwrap().push(run.clone());
        run
    }

    fn upload_expected(&self, run: &str, batch: &Batch) {
        let state = HeaderStore::open(&self.db_path)
            .unwrap()
            .state_at_height::<Mainnet>(batch.end_height, self.genesis_hash())
            .unwrap()
            .expect("state at height");
        let public_values = PublicValues::new(state, batch.verifier_key, ReturnCode::SUCCESS);
        let path = &batch.proof_upload;
        std::fs::write(path, public_values.to_bytes()).unwrap();
        assert!(path.contains(run));
    }

    fn genesis_hash(&self) -> BlockHash {
        let store = HeaderStore::open(&self.db_path).unwrap();
        BlockHash::from(&store.load_header_record(0).unwrap().expect("genesis record").header)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[tokio::test]
async fn run_id_is_the_entire_request_and_completion() {
    let harness = Harness::new();
    let run = harness.run();
    let batch = harness.runtime.pull_batch(&run).await.unwrap();
    assert_eq!(batch.first_new_height, 1);
    assert_eq!(batch.end_height, 2);
    assert!(batch.previous_proof.is_none());

    harness.upload_expected(&run, &batch);
    harness.runtime.complete(&run).await.unwrap();

    let next = harness.runtime.pull_batch(&run).await.unwrap();
    assert_eq!(next.first_new_height, 3);
    assert!(next.previous_proof.is_some());
}

#[tokio::test]
async fn completion_requires_the_controller_selected_proof() {
    let harness = Harness::new();
    let run = harness.run();
    harness.runtime.pull_batch(&run).await.unwrap();
    let error = harness.runtime.complete(&run).await.unwrap_err();
    assert!(error.message.contains("failed to fetch uploaded proof"));
    assert_eq!(
        harness.runtime.pull_batch(&run).await.unwrap_err().kind,
        crate::protocol::ErrorKind::Conflict
    );
}

#[tokio::test]
async fn only_one_batch_may_be_outstanding() {
    let harness = Harness::new();
    let run = harness.run();
    let first = harness.runtime.pull_batch(&run).await.unwrap();

    // A second pull before completing the first should wait (and time out,
    // since nothing ever completes it in this test) rather than succeed.
    assert!(harness.runtime.pull_batch(&run).await.is_err());

    harness.upload_expected(&run, &first);
    harness.runtime.complete(&run).await.unwrap();

    let second = harness.runtime.pull_batch(&run).await.unwrap();
    assert_eq!(second.first_new_height, first.end_height + 1);
}

#[tokio::test]
async fn resuming_into_a_different_artifact_root_names_the_missing_proof() {
    let harness = Harness::new();
    let run = harness.run();
    let batch = harness.runtime.pull_batch(&run).await.unwrap();
    harness.upload_expected(&run, &batch);
    harness.runtime.complete(&run).await.unwrap();

    let elsewhere = std::env::temp_dir().join(format!("zkpow-runtime-elsewhere-{}", unique_nonce()));
    let moved = ControllerRuntime::<Mainnet>::new(
        HeaderStore::open(&harness.db_path).unwrap(),
        Box::new(LocalArtifactStore::new(&elsewhere)),
        Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
        ControllerConfig::new(harness.genesis_hash(), 2),
    );
    let error = moved.pull_batch(&run).await.unwrap_err();
    assert_eq!(error.kind, crate::protocol::ErrorKind::Internal);
    assert!(error.message.contains(&run), "error should name the run: {}", error.message);
    assert!(
        error.message.contains("proof_2.bin"),
        "error should name the proof: {}",
        error.message
    );

    std::fs::remove_dir_all(&elsewhere).ok();
}

#[tokio::test]
async fn an_empty_header_database_is_not_reported_as_a_run_problem() {
    let nonce = unique_nonce();
    let root = std::env::temp_dir().join(format!("zkpow-runtime-emptydb-{nonce}"));
    std::fs::create_dir_all(&root).unwrap();
    let genesis_hash = <Mainnet as zkpow_params::Consensus>::GENESIS_HASH;
    let empty = ControllerRuntime::<Mainnet>::new(
        HeaderStore::open(root.join("empty.db")).unwrap(),
        Box::new(LocalArtifactStore::new(&root)),
        Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
        ControllerConfig::new(genesis_hash, 2),
    );

    let error = empty.pull_batch(EMPTY_DATABASE_RUN_ID).await.unwrap_err();
    assert!(
        error.message.contains("header database"),
        "error should name the database: {}",
        error.message
    );
    assert!(
        !error.message.contains(EMPTY_DATABASE_RUN_ID),
        "error should not blame the run id: {}",
        error.message
    );

    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn put_target_failure_rolls_back_pending_and_retry_succeeds() {
    let fail_put = Arc::new(AtomicBool::new(false));
    let fail_get = Arc::new(AtomicBool::new(false));
    let harness = Harness::with_faulty_storage(fail_put.clone(), fail_get.clone());
    let run = harness.run();

    // Enable PUT target failure
    fail_put.store(true, Ordering::SeqCst);
    let err = harness.runtime.pull_batch(&run).await.unwrap_err();
    assert!(err.message.contains("storage PUT target failed"));

    // Retry should succeed immediately rather than waiting for timeout.
    fail_put.store(false, Ordering::SeqCst);
    let batch = harness
        .runtime
        .pull_batch(&run)
        .await
        .expect("retry after PUT failure should succeed");
    assert_eq!(batch.first_new_height, 1);
    assert_eq!(batch.end_height, 2);
}

#[tokio::test]
async fn get_target_failure_rolls_back_pending_and_retry_succeeds() {
    let fail_put = Arc::new(AtomicBool::new(false));
    let fail_get = Arc::new(AtomicBool::new(false));
    let harness = Harness::with_faulty_storage(fail_put.clone(), fail_get.clone());
    let run = harness.run();

    // Complete the first batch so there is a previous proof.
    let first = harness.runtime.pull_batch(&run).await.unwrap();
    harness.upload_expected(&run, &first);
    harness.runtime.complete(&run).await.unwrap();

    // Pulling the next batch requires a GET target for the previous proof.
    fail_get.store(true, Ordering::SeqCst);
    let err = harness.runtime.pull_batch(&run).await.unwrap_err();
    assert!(err.message.contains("storage GET target failed"));

    // Retry should succeed immediately once failure is cleared.
    fail_get.store(false, Ordering::SeqCst);
    let second = harness
        .runtime
        .pull_batch(&run)
        .await
        .expect("retry after GET failure should succeed");
    assert_eq!(second.first_new_height, 3);
    assert_eq!(second.end_height, 4);
    assert!(second.previous_proof.is_some());
}

#[test]
fn run_id_validation_rejects_empty_absolute_parent_and_separators() {
    use crate::runtime::validate_run_id;

    assert_eq!(validate_run_id("").unwrap_err().kind, crate::protocol::ErrorKind::BadRequest);
    assert_eq!(validate_run_id("..").unwrap_err().kind, crate::protocol::ErrorKind::BadRequest);
    assert_eq!(validate_run_id(".").unwrap_err().kind, crate::protocol::ErrorKind::BadRequest);
    assert_eq!(
        validate_run_id("/abs/path").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("../escape").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("nested/run").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("nested\\run").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("run\0id").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("run-0001").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("short").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );
    assert_eq!(
        validate_run_id("toolongrunid").unwrap_err().kind,
        crate::protocol::ErrorKind::BadRequest
    );

    assert!(validate_run_id("run00001").is_ok());
    assert!(validate_run_id("RUN12345").is_ok());
}

#[tokio::test]
async fn runtime_pull_batch_and_complete_reject_unsafe_run_ids() {
    let harness = Harness::new();

    let unsafe_ids = [
        "",
        "..",
        ".",
        "/absolute/escape",
        "../escape",
        "nested/run",
        "nested\\run",
        "run-0001",
        "short",
        "toolongrunid",
    ];

    for unsafe_id in unsafe_ids {
        let pull_err = harness.runtime.pull_batch(unsafe_id).await.unwrap_err();
        assert_eq!(
            pull_err.kind,
            crate::protocol::ErrorKind::BadRequest,
            "pull_batch should reject {unsafe_id:?} with BadRequest"
        );

        let complete_err = harness.runtime.complete(unsafe_id).await.unwrap_err();
        assert_eq!(
            complete_err.kind,
            crate::protocol::ErrorKind::BadRequest,
            "complete should reject {unsafe_id:?} with BadRequest"
        );
    }
}

#[tokio::test]
async fn guest_compatible_batch_size_advances_and_completes() {
    let nonce = unique_nonce();
    let root = std::env::temp_dir().join(format!("zkpow-runtime-batch10-{nonce}"));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join(TEST_DATABASE_FILE);
    let (store, genesis_hash) = zkpow_store::test_utils::seeded_store_at(&db_path, 25);
    let mut config = ControllerConfig::new(genesis_hash, 10);
    config.pull_timeout = Duration::from_millis(100);
    let storage = LocalArtifactStore::new(&root);
    let runtime = ControllerRuntime::<Mainnet>::new(
        store,
        Box::new(storage),
        Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
        config,
    );

    let run = "run00010";
    let batch = runtime.pull_batch(run).await.unwrap();
    assert_eq!(batch.first_new_height, 1);
    assert_eq!(batch.end_height, 10);
    assert_eq!(batch.header_bytes.len(), 10 * zkpow_core::HeaderInput::SIZE);
    assert!(batch.previous_proof.is_none());

    let state = HeaderStore::open(&db_path)
        .unwrap()
        .state_at_height::<Mainnet>(batch.end_height, genesis_hash)
        .unwrap()
        .expect("state at height");
    let public_values = PublicValues::new(state, batch.verifier_key, ReturnCode::SUCCESS);
    std::fs::write(&batch.proof_upload, public_values.to_bytes()).unwrap();
    runtime.complete(run).await.unwrap();

    let next = runtime.pull_batch(run).await.unwrap();
    assert_eq!(next.first_new_height, 11);
    assert_eq!(next.end_height, 20);
    assert_eq!(next.header_bytes.len(), 10 * zkpow_core::HeaderInput::SIZE);
    assert!(next.previous_proof.is_some());

    std::fs::remove_dir_all(&root).ok();
}
