use std::path::PathBuf;

use zkpow_controller::NO_HEADERS_REMAINING_PREFIX;
use zkpow_params::Mainnet;

mod artifacts;
mod diagnostics;

use crate::pipeline::{
    is_header_exhaustion_error, run_batch_session_with_store, run_execution_session, BoxError, ProofGenerationConfig,
    ProverBackend,
};

#[test]
fn classifies_header_exhaustion_errors_by_prefix() {
    let cases = [
        (format!("{NO_HEADERS_REMAINING_PREFIX}: starting at height {}", 123u32), true),
        ("some other error".to_string(), false),
    ];

    for (message, expected) in cases {
        let err: BoxError = message.clone().into();
        assert_eq!(is_header_exhaustion_error(&err), expected, "message={message:?}");
    }
}

struct SessionScratch {
    dir: PathBuf,
}

impl SessionScratch {
    fn new(test_name: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("zkpow-host-session-{test_name}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(dir.join("artifacts")).unwrap();
        Self { dir }
    }

    fn artifacts_dir(&self) -> PathBuf {
        self.dir.join("artifacts")
    }
}

impl Drop for SessionScratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn test_config(
    run_id: String,
    local_batch_size: u32,
    batch_count: u32,
    proof_store_dir: PathBuf,
) -> ProofGenerationConfig {
    ProofGenerationConfig {
        run_id,
        local_batch_size,
        batch_count,
        db_path: PathBuf::new(),
        proof_store_dir,
        execute_only: true,
        prover_backend: ProverBackend::Mock,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stops_after_reaching_configured_batch_count() {
    let scratch = SessionScratch::new("batch-count");
    let store = zkpow_store::test_utils::create_test_fixture_store();
    let config = test_config("test-batch-count".to_string(), 1, 0, scratch.artifacts_dir());

    let completed = run_batch_session_with_store::<Mainnet>(store, config)
        .await
        .expect("session should exit cleanly");
    assert_eq!(completed, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stops_after_exhausting_headers_in_database() {
    let scratch = SessionScratch::new("exhausted");
    let store = zkpow_store::test_utils::create_test_fixture_store();
    let config = test_config("test-exhausted".to_string(), 128, 5, scratch.artifacts_dir());

    let completed = run_batch_session_with_store::<Mainnet>(store, config)
        .await
        .expect("session should stop on exhaustion without error");
    assert_eq!(completed, 1);
}

/// Execute-only is deliberately a repeatable local diagnostic. It does
/// not create or advance a production run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_only_session_is_separate_from_run_progress() {
    let scratch = SessionScratch::new("advance-resume");
    let run_id = "test-advance-resume".to_string();
    let store = zkpow_store::test_utils::create_test_fixture_store();
    let config = test_config(run_id.clone(), 10, 2, scratch.artifacts_dir());

    let completed = run_execution_session::<Mainnet>(&store, config.clone())
        .await
        .expect("first session should complete");
    assert_eq!(completed, 2);

    let completed = run_execution_session::<Mainnet>(&store, config)
        .await
        .expect("second session should complete");
    assert_eq!(completed, 2);

    assert!(store.load_run(&run_id).unwrap().is_none());
}
