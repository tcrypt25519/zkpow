use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use zkpow_core::{PublicValues, ReturnCode, VerifierKeyDigest};
use zkpow_store::{db_path, HeaderStore};

use super::*;
use crate::{
    protocol::Batch,
    runtime::{ControllerConfig, ControllerRuntime},
    storage::LocalArtifactStore,
    verify::ProofVerifier,
};

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
    run_ids: Mutex<Vec<String>>,
    nonce:   u128,
}

impl Harness {
    fn new() -> Self {
        let nonce = unique_nonce();
        let root = std::env::temp_dir().join(format!("zkpow-runtime-{nonce}"));
        let genesis_hash = {
            let temp = HeaderStore::open(db_path()).unwrap();
            BlockHash::from(&temp.load_header_record(0).header)
        };
        let store = HeaderStore::open(db_path()).unwrap();
        let mut config = ControllerConfig::new(genesis_hash, 2);
        config.pull_timeout = Duration::from_millis(100);
        let runtime = ControllerRuntime::new(
            store,
            Box::new(LocalArtifactStore::new(&root)),
            Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
            config,
        );
        Self {
            runtime,
            root,
            run_ids: Mutex::new(Vec::new()),
            nonce,
        }
    }

    fn run(&self, name: &str) -> String {
        let run = format!("test-{}-{name}", self.nonce);
        self.run_ids.lock().unwrap().push(run.clone());
        run
    }

    fn upload_expected(&self, run: &str, batch: &Batch) {
        let state = HeaderStore::open(db_path())
            .unwrap()
            .state_at_height::<Mainnet>(batch.end_height, self.genesis_hash());
        let public_values = PublicValues::new(state, batch.verifier_key, ReturnCode::SUCCESS);
        let path = &batch.proof_upload;
        std::fs::write(path, public_values.to_bytes()).unwrap();
        assert!(path.contains(run));
    }

    fn genesis_hash(&self) -> BlockHash {
        let store = HeaderStore::open(db_path()).unwrap();
        BlockHash::from(&store.load_header_record(0).header)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Ok(store) = HeaderStore::open(db_path()) {
            for run in self.run_ids.lock().unwrap().iter() {
                store.delete_run(run).ok();
            }
        }
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[tokio::test]
async fn run_id_is_the_entire_request_and_completion() {
    let harness = Harness::new();
    let run = harness.run("advance");
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
    let run = harness.run("missing-proof");
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
    let run = harness.run("serial");
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
    let run = harness.run("moved-store");
    let batch = harness.runtime.pull_batch(&run).await.unwrap();
    harness.upload_expected(&run, &batch);
    harness.runtime.complete(&run).await.unwrap();

    let elsewhere = std::env::temp_dir().join(format!("zkpow-runtime-elsewhere-{}", unique_nonce()));
    let moved = ControllerRuntime::<Mainnet>::new(
        HeaderStore::open(db_path()).unwrap(),
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
    let genesis_hash = {
        let store = HeaderStore::open(db_path()).unwrap();
        BlockHash::from(&store.load_header_record(0).header)
    };
    let empty = ControllerRuntime::<Mainnet>::new(
        HeaderStore::open(root.join("empty.db")).unwrap(),
        Box::new(LocalArtifactStore::new(&root)),
        Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
        ControllerConfig::new(genesis_hash, 2),
    );

    let error = empty.pull_batch("any-run").await.unwrap_err();
    assert!(
        error.message.contains("header database"),
        "error should name the database: {}",
        error.message
    );
    assert!(
        !error.message.contains("any-run"),
        "error should not blame the run id: {}",
        error.message
    );

    std::fs::remove_dir_all(&root).ok();
}
