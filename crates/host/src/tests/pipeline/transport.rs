use std::time::Duration;

use zkpow_controller::{
    db_path, runtime::ControllerRuntime, runtime::ControllerConfig, storage::LocalArtifactStore, HeaderStore,
};
use zkpow_core::VerifierKeyDigest;

use crate::pipeline::transport::{ControllerTransport, HttpController, LocalController};

struct TestVerifier;

impl zkpow_controller::verify::ProofVerifier for TestVerifier {
    fn verifier_key_digest(&self) -> VerifierKeyDigest {
        VerifierKeyDigest::default()
    }

    fn verify_compressed(&self, _: &[u8], _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// Both transports share the real header database (including its `runs`
/// table) and one artifact root, so equal requests must produce equal
/// batches. Drop deletes the test's run row again.
struct Harness {
    run_id:        String,
    artifact_root: std::path::PathBuf,
}

fn test_runtime(artifact_root: &std::path::Path) -> ControllerRuntime {
    let store = HeaderStore::open(db_path()).expect("failed to open test database");
    let mut config = ControllerConfig::new(crate::pipeline::input::genesis_hash(), 13);
    config.pull_timeout = Duration::from_millis(200);
    ControllerRuntime::new(
        store,
        Box::new(LocalArtifactStore::new(artifact_root)),
        Box::new(TestVerifier),
        config,
    )
}

async fn local_and_http_transports(test_name: &str) -> (Harness, LocalController, HttpController) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifact_root =
        std::env::temp_dir().join(format!("zkpow-transport-{test_name}-{}-{nonce}", std::process::id()));

    let local = LocalController::new(test_runtime(&artifact_root));

    let app = zkpow_controller::http::build_router(test_runtime(&artifact_root));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let harness = Harness {
        run_id: format!("test-{test_name}-{nonce}"),
        artifact_root,
    };
    (harness, local, HttpController::new(format!("http://127.0.0.1:{port}")))
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Ok(db) = HeaderStore::open(db_path()) {
            db.delete_run(&self.run_id).ok();
        }
        std::fs::remove_dir_all(&self.artifact_root).ok();
    }
}

#[tokio::test]
async fn http_batch_matches_local() {
    let (harness, local, http) = local_and_http_transports("batch-parity").await;

    let local_batch = local.pull_batch(&harness.run_id).await.unwrap();
    let http_batch = http.pull_batch(&harness.run_id).await.unwrap();

    assert_eq!(local_batch, http_batch);
    assert_eq!(local_batch.first_new_height, 1);
    assert_eq!(local_batch.end_height, 13);
}

#[tokio::test]
async fn transport_errors_match_between_local_and_http() {
    let (_harness, local, http) = local_and_http_transports("error-parity").await;

    let local_err = local.pull_batch("").await.unwrap_err();
    let http_err = http.pull_batch("").await.unwrap_err();

    assert_eq!(local_err.to_string(), http_err.to_string());
    assert_eq!(local_err.to_string(), "run id must not be empty");
}
