use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use zkpow_controller::NO_HEADERS_REMAINING_PREFIX;
use zkpow_params::Mainnet;
use zkpow_store::{db_path, HeaderStore, ENV_ZKPOW_DB_PATH};

mod artifacts;
mod diagnostics;

use crate::pipeline::{
    input::{
        config_from_env, parse_bool_env, parse_u32_env, ENV_ZKPOW_BATCH_COUNT, ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_EXECUTE_ONLY,
        ENV_ZKPOW_PROOF_STORE_DIR, ENV_ZKPOW_PROVER_BACKEND, ENV_ZKPOW_RUN_ID, ENV_ZKPOW_USE_CUDA,
    },
    is_header_exhaustion_error, run_batch_session, BoxError, ProverBackend,
};

const INVALID_BOOL_ENV_VALUE: &str = "maybe";
const INVALID_U32_ENV_VALUE: &str = "abc";
const TEST_RUN_ID: &str = "mainnet-01";

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
fn config_validation_matrix_invalid_values() {
    let _guard = _env_test_mutex().lock().unwrap();

    set_env(ENV_ZKPOW_EXECUTE_ONLY, INVALID_BOOL_ENV_VALUE);
    assert!(
        parse_bool_env(ENV_ZKPOW_EXECUTE_ONLY).is_err(),
        "{ENV_ZKPOW_EXECUTE_ONLY}={INVALID_BOOL_ENV_VALUE} should be invalid"
    );
    remove_env(ENV_ZKPOW_EXECUTE_ONLY);

    set_env(ENV_ZKPOW_BATCH_SIZE, INVALID_U32_ENV_VALUE);
    assert!(
        parse_u32_env(ENV_ZKPOW_BATCH_SIZE).is_err(),
        "{ENV_ZKPOW_BATCH_SIZE}={INVALID_U32_ENV_VALUE} should be invalid"
    );
    remove_env(ENV_ZKPOW_BATCH_SIZE);
}

#[test]
fn boolean_environment_parser_accepts_named_true_and_false_values() {
    let _guard = _env_test_mutex().lock().unwrap();

    for value in ["1", "true", "yes", "on", "TrUe"] {
        set_env(ENV_ZKPOW_EXECUTE_ONLY, value);
        assert!(parse_bool_env(ENV_ZKPOW_EXECUTE_ONLY).unwrap(), "{value} should be true");
    }
    for value in ["0", "false", "no", "off"] {
        set_env(ENV_ZKPOW_EXECUTE_ONLY, value);
        assert!(!parse_bool_env(ENV_ZKPOW_EXECUTE_ONLY).unwrap(), "{value} should be false");
    }
    remove_env(ENV_ZKPOW_EXECUTE_ONLY);
}

#[test]
fn prover_backend_aliases_select_typed_backends() {
    let _guard = _env_test_mutex().lock().unwrap();
    remove_env(ENV_ZKPOW_BATCH_SIZE);
    remove_env(ENV_ZKPOW_BATCH_COUNT);
    remove_env(ENV_ZKPOW_USE_CUDA);
    set_env(ENV_ZKPOW_EXECUTE_ONLY, "0");

    for (name, expected) in [
        ("cuda", ProverBackend::Cuda),
        ("gpu", ProverBackend::Cuda),
        ("CPU", ProverBackend::Cpu),
        ("mock", ProverBackend::Mock),
    ] {
        set_env(ENV_ZKPOW_PROVER_BACKEND, name);
        assert_eq!(config_from_env::<Mainnet>().unwrap().prover_backend, expected);
    }

    set_env(ENV_ZKPOW_PROVER_BACKEND, "tpu");
    assert!(config_from_env::<Mainnet>().is_err());

    remove_env(ENV_ZKPOW_EXECUTE_ONLY);
    remove_env(ENV_ZKPOW_PROVER_BACKEND);
}

#[test]
#[cfg(not(feature = "memory-diagnostics"))]
fn memory_logging_preserves_its_narrow_truthy_set() {
    use crate::pipeline::input::ENV_ZKPOW_MEMORY_LOGGING;

    let _guard = _env_test_mutex().lock().unwrap();

    for value in ["1", "true", "on"] {
        set_env(ENV_ZKPOW_MEMORY_LOGGING, value);
        assert!(crate::memory_monitor::logging_enabled(), "{value} should enable logging");
    }
    for value in ["yes", "TRUE", ""] {
        set_env(ENV_ZKPOW_MEMORY_LOGGING, value);
        assert!(!crate::memory_monitor::logging_enabled(), "{value} should not enable logging");
    }
    remove_env(ENV_ZKPOW_MEMORY_LOGGING);
    assert!(!crate::memory_monitor::logging_enabled());
}

#[test]
fn execute_only_defaults_to_mock_backend_and_dev_run() {
    let _guard = _env_test_mutex().lock().unwrap();
    set_env(ENV_ZKPOW_EXECUTE_ONLY, "1");
    set_env(ENV_ZKPOW_BATCH_SIZE, "1");

    let config = config_from_env::<Mainnet>().unwrap();

    assert_eq!(config.prover_backend, ProverBackend::Mock);
    assert!(config.execute_only);
    assert_eq!(config.run_id, "dev");

    remove_env(ENV_ZKPOW_EXECUTE_ONLY);
    remove_env(ENV_ZKPOW_BATCH_SIZE);
}

#[test]
fn run_id_env_overrides_default() {
    let _guard = _env_test_mutex().lock().unwrap();
    set_env(ENV_ZKPOW_RUN_ID, TEST_RUN_ID);

    let config = config_from_env::<Mainnet>().unwrap();
    assert_eq!(config.run_id, TEST_RUN_ID);

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
        set_env(ENV_ZKPOW_PROOF_STORE_DIR, dir.join("artifacts").to_str().unwrap());
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
            ENV_ZKPOW_PROOF_STORE_DIR,
            ENV_ZKPOW_RUN_ID,
            ENV_ZKPOW_EXECUTE_ONLY,
            ENV_ZKPOW_BATCH_COUNT,
            ENV_ZKPOW_BATCH_SIZE,
            ENV_ZKPOW_DB_PATH,
        ] {
            remove_env(name);
        }
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn stops_after_reaching_configured_batch_count() {
    let _guard = _env_test_mutex().lock().unwrap();
    let env = SessionEnv::new("batch-count");
    set_env(ENV_ZKPOW_BATCH_COUNT, "0");
    set_env(ENV_ZKPOW_BATCH_SIZE, "1");
    set_env(ENV_ZKPOW_DB_PATH, env.dir.to_str().unwrap());

    let completed = run_batch_session::<Mainnet>().await.expect("session should exit cleanly");
    assert_eq!(completed, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn stops_after_exhausting_headers_in_database() {
    let _guard = _env_test_mutex().lock().unwrap();
    let env = SessionEnv::new("exhausted");
    let db_path = env.dir.join("headers.sqlite");
    zkpow_store::test_utils::create_real_seeded_store(&db_path, 10);
    set_env(ENV_ZKPOW_BATCH_COUNT, "5");
    set_env(ENV_ZKPOW_BATCH_SIZE, "10");
    set_env(ENV_ZKPOW_DB_PATH, db_path.to_str().unwrap());

    let completed = run_batch_session::<Mainnet>()
        .await
        .expect("session should stop on exhaustion without error");
    assert_eq!(completed, 1);
}

/// Execute-only is deliberately a repeatable local diagnostic. It does
/// not create or advance a production run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::await_holding_lock)]
async fn execute_only_session_is_separate_from_run_progress() {
    let _guard = _env_test_mutex().lock().unwrap();
    let env = SessionEnv::new("advance-resume");
    set_env(ENV_ZKPOW_BATCH_COUNT, "2");
    set_env(ENV_ZKPOW_BATCH_SIZE, "10");

    let completed = run_batch_session::<Mainnet>().await.expect("first session should complete");
    assert_eq!(completed, 2);

    let completed = run_batch_session::<Mainnet>().await.expect("second session should complete");
    assert_eq!(completed, 2);

    assert!(HeaderStore::open(db_path()).unwrap().load_run(&env.run_id).unwrap().is_none());
}
