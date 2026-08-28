use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use zkpow_core::{serialize_minimal_public_values, VerifierKeyDigest};
use zkpow_store::{db_path, headers_bin_path, HeaderBinReader};

use super::*;
use crate::{
    protocol::Batch,
    runtime::{ControllerConfig, ControllerRuntime},
    storage::LocalArtifactStore,
    verify::ProofVerifier,
};

/// A wall-clock nonce alone can collide between tests that construct a
/// `Harness`/temp path microseconds apart under parallel test execution
/// (observed: one test's `Harness::drop` removing another's still-live temp
/// dir). Mixed into every nonce below to guarantee uniqueness regardless of
/// clock resolution.
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
    runtime: ControllerRuntime,
    root: std::path::PathBuf,
    run_ids: Mutex<Vec<String>>,
    nonce: u128,
}

impl Harness {
    fn new() -> Self {
        let nonce = unique_nonce();
        let root = std::env::temp_dir().join(format!("zkpow-runtime-{nonce}"));
        let store = HeaderStore::open(db_path()).unwrap();
        let headers = HeaderBinReader::open(headers_bin_path()).unwrap();
        let genesis_hash = BlockHash::from(&store.load_header_record(0).header);
        let mut config = ControllerConfig::new(genesis_hash, 2);
        config.pull_timeout = Duration::from_millis(100);
        let runtime = ControllerRuntime::new(
            store,
            headers,
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
        let claim = state.public_claim();
        let bytes = serialize_minimal_public_values(
            &claim.chain_work,
            batch.verifier_key.to_le_bytes_slice(),
            &continuation_digest_from_state(&state),
            &claim.genesis_hash,
            &claim.tip_hash,
            claim.height,
            0,
        );
        let path = &batch.proof_upload;
        std::fs::write(path, bytes).unwrap();
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

/// A `headers.bin` snapshot that's fallen behind the run ledger's recorded
/// progress (e.g. resynced `headers.db` without regenerating the dump) must
/// surface as a clean error, not a panic deep inside a seek past EOF.
#[tokio::test]
async fn pull_batch_rejects_stale_headers_bin_snapshot() {
    let nonce = unique_nonce();
    let db_path = std::env::temp_dir().join(format!("zkpow-stale-headers-{nonce}.sqlite"));
    let bin_path = std::env::temp_dir().join(format!("zkpow-stale-headers-{nonce}.bin"));
    let root = std::env::temp_dir().join(format!("zkpow-stale-headers-store-{nonce}"));

    // headers.bin covers only heights 0..3, but the run ledger claims
    // verified progress at height 5.
    let dummy_records: Vec<HeaderRecord> = (0..3)
        .map(|height| HeaderRecord {
            height,
            header: zkpow_core::Header::default(),
            chain_work: zkpow_core::ChainWork::default(),
            median_time_past: zkpow_core::BlockTimestamp::default(),
            ordered_timestamp_run: 0,
        })
        .collect();
    zkpow_store::flat::write_snapshot(&bin_path, &dummy_records).unwrap();

    let store = HeaderStore::open(&db_path).unwrap();
    let run_id = format!("test-stale-headers-{nonce}");
    store
        .create_run(
            &run_id,
            &zkpow_store::RunRecord {
                verifier_key: VerifierKeyDigest::from_raw([7; 8]),
                verified_height: 5,
                proof_object: None,
            },
        )
        .unwrap();

    let headers = HeaderBinReader::open(&bin_path).unwrap();
    let runtime = ControllerRuntime::new(
        store,
        headers,
        Box::new(LocalArtifactStore::new(&root)),
        Box::new(BytesArePublicValues(VerifierKeyDigest::from_raw([7; 8]))),
        ControllerConfig::new(BlockHash::default(), 2),
    );

    let error = runtime.pull_batch(&run_id).await.unwrap_err();
    assert!(
        error.message.contains("dump-bin"),
        "expected a clear regenerate-the-snapshot error, got: {}",
        error.message
    );

    std::fs::remove_file(&db_path).ok();
    std::fs::remove_file(db_path.with_extension("sqlite-shm")).ok();
    std::fs::remove_file(db_path.with_extension("sqlite-wal")).ok();
    std::fs::remove_file(&bin_path).ok();
    std::fs::remove_dir_all(&root).ok();
}
