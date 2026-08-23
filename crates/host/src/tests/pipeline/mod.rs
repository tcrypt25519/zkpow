use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use zkpow_controller::{db_path, HeaderStore, NO_HEADERS_REMAINING_PREFIX};

mod artifacts;
mod transport;

use crate::pipeline::{
    input::{
        config_from_env, ensure_cuda_requested_configuration, parse_bool_env, parse_u32_env, ENV_ZKPOW_BATCH_SIZE,
        ENV_ZKPOW_CUDA_DEVICE_ID, ENV_ZKPOW_EXECUTE_ONLY, ENV_ZKPOW_GENERATE_GROTH16, ENV_ZKPOW_RUN_ID,
        ENV_ZKPOW_USE_CUDA,
    },
    is_header_exhaustion_error, run_batch_session, BoxError, PipelineMode, ProverBackend,
};

fn _env_test_mutex() -> &'static Mutex<()> {
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
    let _guard = _env_test_mutex().lock().unwrap();
    let cases = [
        (Some("true"), Ok(true)),
        (Some("false"), Ok(false)),
        (None, Ok(false)),
        (Some("invalid"), Err(())),
    ];

    for (value, expected) in cases {
        match value {
            Some(v) => set_env(ENV_ZKPOW_GENERATE_GROTH16, v),
            None => remove_env(ENV_ZKPOW_GENERATE_GROTH16),
        }

        let parsed = parse_bool_env(ENV_ZKPOW_GENERATE_GROTH16);
        match expected {
            Ok(expected) => assert_eq!(parsed.unwrap(), expected, "value={value:?}"),
            Err(()) => assert!(parsed.is_err(), "value={value:?} should error"),
        }
    }
    remove_env(ENV_ZKPOW_GENERATE_GROTH16);
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
    assert!(err.to_string().contains("--features CUDA"), "unexpected error: {err}",);
}

#[test]
fn config_validation_matrix_invalid_values() {
    let _guard = _env_test_mutex().lock().unwrap();

    set_env(ENV_ZKPOW_USE_CUDA, "maybe");
    assert!(
        parse_bool_env(ENV_ZKPOW_USE_CUDA).is_err(),
        "{ENV_ZKPOW_USE_CUDA}=maybe should be invalid"
    );
    remove_env(ENV_ZKPOW_USE_CUDA);

    set_env(ENV_ZKPOW_CUDA_DEVICE_ID, "abc");
    assert!(
        parse_u32_env(ENV_ZKPOW_CUDA_DEVICE_ID).is_err(),
        "{ENV_ZKPOW_CUDA_DEVICE_ID}=abc should be invalid"
    );
    remove_env(ENV_ZKPOW_CUDA_DEVICE_ID);

    set_env(ENV_ZKPOW_GENERATE_GROTH16, "yessir");
    assert!(
        parse_bool_env(ENV_ZKPOW_GENERATE_GROTH16).is_err(),
        "{ENV_ZKPOW_GENERATE_GROTH16}=yessir should be invalid"
    );
    remove_env(ENV_ZKPOW_GENERATE_GROTH16);
}

#[test]
fn execute_only_defaults_to_mock_backend_and_dev_run() {
    let _guard = _env_test_mutex().lock().unwrap();
    set_env(ENV_ZKPOW_EXECUTE_ONLY, "1");
    set_env(ENV_ZKPOW_BATCH_SIZE, "1");

    let config = config_from_env().unwrap();

    assert_eq!(config.prover_backend, ProverBackend::Mock);
    assert_eq!(config.mode, PipelineMode::ExecuteOnly);
    assert_eq!(config.run_id, "dev");

    remove_env(ENV_ZKPOW_EXECUTE_ONLY);
    remove_env(ENV_ZKPOW_BATCH_SIZE);
}

#[test]
fn run_id_env_overrides_default() {
    let _guard = _env_test_mutex().lock().unwrap();
    set_env(ENV_ZKPOW_RUN_ID, "mainnet-01");

    let config = config_from_env().unwrap();
    assert_eq!(config.run_id, "mainnet-01");

    remove_env(ENV_ZKPOW_RUN_ID);
}

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

/// Points the session at a scratch artifact dir and a unique run id, and
/// deletes the run's ledger row from the (shared, real) header database
/// again on drop.
struct SessionEnv {
    dir:    PathBuf,
    run_id: String,
}

impl SessionEnv {
    fn new(test_name: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("zkpow-host-session-{test_name}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run_id = format!("test-{test_name}-{nonce}");
        set_env("ZKPOW_PROOF_STORE_DIR", dir.join("artifacts").to_str().unwrap());
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
    let _guard = _env_test_mutex().lock().unwrap();
    let env = SessionEnv::new("batch-count");
    set_env("ZKPOW_BATCH_COUNT", "0");
    set_env(ENV_ZKPOW_BATCH_SIZE, "1");
    set_env("ZKPOW_DB_PATH", env.dir.to_str().unwrap());

    let completed = run_batch_session().await.expect("session should exit cleanly");
    assert_eq!(completed, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stops_after_exhausting_headers_in_database() {
    let _guard = _env_test_mutex().lock().unwrap();
    let _env = SessionEnv::new("exhausted");
    set_env("ZKPOW_BATCH_COUNT", "5");
    set_env(ENV_ZKPOW_BATCH_SIZE, "0");
    set_env("ZKPOW_DB_PATH", concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db"));

    let completed = run_batch_session()
        .await
        .expect("session should stop on exhaustion without error");
    assert_eq!(completed, 0);
}

/// Execute-only is deliberately a repeatable local diagnostic. It does
/// not create or advance a production run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execute_only_session_is_separate_from_run_progress() {
    let _guard = _env_test_mutex().lock().unwrap();
    let env = SessionEnv::new("advance-resume");
    set_env("ZKPOW_BATCH_COUNT", "2");
    set_env(ENV_ZKPOW_BATCH_SIZE, "3");

    let completed = run_batch_session().await.expect("first session should complete");
    assert_eq!(completed, 2);

    let completed = run_batch_session().await.expect("second session should complete");
    assert_eq!(completed, 2);

    assert!(HeaderStore::open(db_path()).unwrap().load_run(&env.run_id).unwrap().is_none());
}
