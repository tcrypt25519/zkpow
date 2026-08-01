use std::sync::Mutex;
use std::time::Duration;

use zkpow_core::{MinimalPublicValues, VerifierKeyDigest};
use zkpow_sha256;
use zkpow_store::db_path;

use super::*;
use crate::{
    protocol::{Batch, FileLocation},
    runtime::{ControllerConfig, ControllerRuntime},
    storage::LocalArtifactStore,
    verify::ProofVerifier,
};

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
    runtime: ControllerRuntime,
    root: std::path::PathBuf,
    run_ids: Mutex<Vec<String>>,
    nonce: u128,
}

impl Harness {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("zkpow-runtime-{nonce}"));
        let store = HeaderStore::open(db_path()).unwrap();
        let genesis_hash = BlockHash::new(zkpow_sha256::double(&store.load_header_record(0).header.to_bytes()));
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
            .state_at_height(batch.end_height, self.genesis_hash());
        let bytes = MinimalPublicValues::success(
            &state.public_claim(),
            continuation_digest_from_state(&state),
            batch.verifier_key,
        )
        .to_bytes();
        let FileLocation::Path(path) = &batch.proof_upload else {
            panic!("expected path")
        };
        std::fs::write(path, bytes).unwrap();
        assert!(path.contains(run));
        let FileLocation::Path(path) = &batch.continuation_upload else {
            panic!("expected path")
        };
        std::fs::write(path, &batch.continuation_bytes).unwrap();
    }

    fn genesis_hash(&self) -> BlockHash {
        let store = HeaderStore::open(db_path()).unwrap();
        BlockHash::new(zkpow_sha256::double(&store.load_header_record(0).header.to_bytes()))
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
    let batch = harness.runtime.pull_batch(&run).await.unwrap();
    let FileLocation::Path(path) = &batch.continuation_upload else {
        panic!("expected path")
    };
    std::fs::write(path, &batch.continuation_bytes).unwrap();
    let error = harness.runtime.complete(&run).await.unwrap_err();
    assert!(error.message.contains("failed to fetch uploaded proof"));
    assert_eq!(
        harness.runtime.pull_batch(&run).await.unwrap_err().kind,
        crate::protocol::ErrorKind::Conflict
    );
}

#[tokio::test]
async fn two_batches_may_be_outstanding() {
    let harness = Harness::new();
    let run = harness.run("pipeline");
    let first = harness.runtime.pull_batch(&run).await.unwrap();
    let second = harness.runtime.pull_batch(&run).await.unwrap();
    assert_eq!(second.first_new_height, first.end_height + 1);
    assert!(harness.runtime.pull_batch(&run).await.is_err());
}
