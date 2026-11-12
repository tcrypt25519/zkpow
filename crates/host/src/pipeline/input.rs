//! Host configuration (environment-driven) and guest stdin construction.
//!
//! This module is the single authoritative entry point for reading host
//! environment variables into typed configuration structures.

use std::path::PathBuf;

use sp1_sdk::{prelude::*, SP1Proof};
use zkpow_core::{State, VerifierKeyDigest};
use zkpow_params::Consensus;

use crate::{
    observability::ObservabilityConfig,
    pipeline::{BoxError, ProofGenerationConfig, ProverBackend},
};

const ENV_ZKPOW_DB_PATH: &str = "ZKPOW_DB_PATH";
const DEFAULT_DB_FILE_NAME: &str = "headers.db";
const ENV_SHARD_PROVER_PERMITS: &str = "SHARD_PROVER_PERMITS";
const WORKSPACE_ROOT_RELATIVE_TO_HOST: &str = "../..";

const ENV_ZKPOW_RUN_ID: &str = "ZKPOW_RUN_ID";
const ENV_ZKPOW_BATCH_SIZE: &str = "ZKPOW_BATCH_SIZE";
const ENV_ZKPOW_BATCH_COUNT: &str = "ZKPOW_BATCH_COUNT";
const ENV_ZKPOW_PROOF_STORE_DIR: &str = "ZKPOW_PROOF_STORE_DIR";
const ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS: &str = "ZKPOW_SHOW_COMPRESSED_PROOF_SPANS";

const ENV_ZKPOW_EXECUTE_ONLY: &str = "ZKPOW_EXECUTE_ONLY";
const ENV_ZKPOW_PROVER_BACKEND: &str = "ZKPOW_PROVER_BACKEND";
const ENV_ZKPOW_USE_CUDA: &str = "ZKPOW_USE_CUDA";
const ENV_ZKPOW_MEMORY_LOGGING: &str = "ZKPOW_MEMORY_LOGGING";

const ENV_SP1_ENABLE_TOKIO_CONSOLE: &str = "SP1_ENABLE_TOKIO_CONSOLE";
const ENV_TOKIO_CONSOLE_BIND: &str = "TOKIO_CONSOLE_BIND";
const ENV_ELEMENT_THRESHOLD: &str = "ELEMENT_THRESHOLD";
const ENV_ZKPOW_GAS_PER_SHARD: &str = "ZKPOW_GAS_PER_SHARD";

const DEFAULT_RUN_ID: &str = "develop_";
const DEFAULT_PROOF_STORE_DIR: &str = "proof_store";
const DEFAULT_BATCH_COUNT: u32 = 2016;
const DEFAULT_SHARD_PROVER_PERMITS: usize = 4;
const DEFAULT_SHARD_AREA: u64 = (1 << 28) + (1 << 27);
const DEFAULT_GAS_PER_SHARD: u64 = 7_800_000;

pub(crate) fn parse_bool_env(var_name: &'static str) -> Result<bool, BoxError> {
    parse_bool_env_or(var_name, false)
}

pub(crate) fn parse_bool_env_or(var_name: &'static str, default: bool) -> Result<bool, BoxError> {
    match std::env::var(var_name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => {
                Err(format!("invalid {var_name} value `{value}`; expected one of 1,true,yes,on,0,false,no,off").into())
            },
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn parse_u32_env(var_name: &'static str) -> Result<Option<u32>, BoxError> {
    match std::env::var(var_name) {
        Ok(value) => Ok(Some(
            value
                .parse()
                .map_err(|err| format!("invalid {var_name} value `{value}`: {err}"))?,
        )),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn parse_u32_env_or(var_name: &'static str, default: u32) -> Result<u32, BoxError> {
    Ok(parse_u32_env(var_name)?.unwrap_or(default))
}

fn parse_u64_env_or(var_name: &'static str, default: u64) -> Result<u64, BoxError> {
    match std::env::var(var_name) {
        Ok(value) => Ok(value
            .parse()
            .map_err(|err| format!("invalid {var_name} value `{value}`: {err}"))?),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(err.into()),
    }
}

fn parse_path_env(var_name: &'static str) -> Option<PathBuf> {
    std::env::var_os(var_name).map(PathBuf::from)
}

fn resolve_prover_backend(execute_only: bool) -> Result<ProverBackend, BoxError> {
    if execute_only {
        return Ok(ProverBackend::Mock);
    }
    if let Ok(backend_str) = std::env::var(ENV_ZKPOW_PROVER_BACKEND) {
        match backend_str.to_ascii_lowercase().as_str() {
            "cuda" | "gpu" => return Ok(ProverBackend::Cuda),
            "cpu" => return Ok(ProverBackend::Cpu),
            "mock" => return Ok(ProverBackend::Mock),
            other => {
                return Err(
                    format!("unknown {ENV_ZKPOW_PROVER_BACKEND}: `{other}`; expected 'cuda', 'cpu', or 'mock'").into(),
                )
            },
        }
    }
    if parse_bool_env(ENV_ZKPOW_USE_CUDA)? {
        return Ok(ProverBackend::Cuda);
    }
    Ok(ProverBackend::Cpu)
}

/// Read the session config from `ZKPOW_*` variables. Batch size defaults to one difficulty epoch.
pub fn config_from_env<C: Consensus>() -> Result<ProofGenerationConfig, BoxError> {
    let execute_only = parse_bool_env(ENV_ZKPOW_EXECUTE_ONLY)?;

    // One difficulty epoch per controller-selected batch unless overridden.
    let local_batch_size = parse_u32_env_or(ENV_ZKPOW_BATCH_SIZE, C::PARAMS.epoch_length)?;
    let batch_count = parse_u32_env_or(ENV_ZKPOW_BATCH_COUNT, DEFAULT_BATCH_COUNT)?;

    let prover_backend = resolve_prover_backend(execute_only)?;

    // Initialize downstream subsystems that read settings from environment
    let memory_logging = parse_bool_env_or(ENV_ZKPOW_MEMORY_LOGGING, false)?;
    crate::memory_monitor::init(memory_logging);

    let element_threshold = parse_u64_env_or(ENV_ELEMENT_THRESHOLD, DEFAULT_SHARD_AREA)?;
    let gas_per_shard = parse_u64_env_or(ENV_ZKPOW_GAS_PER_SHARD, DEFAULT_GAS_PER_SHARD)?;
    crate::pipeline::diagnostics::set_calibration(element_threshold, gas_per_shard);

    Ok(ProofGenerationConfig {
        run_id: std::env::var(ENV_ZKPOW_RUN_ID).unwrap_or_else(|_| DEFAULT_RUN_ID.to_string()),
        local_batch_size,
        batch_count,
        db_path: default_db_path(),
        proof_store_dir: proof_store_dir_from_env(),
        execute_only,
        prover_backend,
    })
}

/// Parse observability settings from the environment into a typed configuration struct.
pub fn observability_config_from_env() -> ObservabilityConfig {
    ObservabilityConfig {
        show_compressed_proof_spans: parse_bool_env_or(ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS, false).unwrap_or(false),
        enable_tokio_console: parse_bool_env_or(ENV_SP1_ENABLE_TOKIO_CONSOLE, false).unwrap_or(false),
        tokio_console_bind: std::env::var(ENV_TOKIO_CONSOLE_BIND).unwrap_or_else(|_| "127.0.0.1:6669".to_string()),
    }
}

pub fn default_db_path() -> PathBuf {
    parse_path_env(ENV_ZKPOW_DB_PATH).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(WORKSPACE_ROOT_RELATIVE_TO_HOST)
            .join(DEFAULT_DB_FILE_NAME)
    })
}

pub fn shard_prover_permits() -> usize {
    parse_u32_env(ENV_SHARD_PROVER_PERMITS)
        .ok()
        .flatten()
        .filter(|&p| p > 0)
        .map(|p| p as usize)
        .unwrap_or(DEFAULT_SHARD_PROVER_PERMITS)
}

pub fn proof_store_dir_from_env() -> PathBuf {
    parse_path_env(ENV_ZKPOW_PROOF_STORE_DIR).unwrap_or_else(|| PathBuf::from(DEFAULT_PROOF_STORE_DIR))
}

/// Assemble the guest's stdin. Header and median-hint bytes come straight
/// from the batch grant and the previous proof is moved in, not cloned.
/// The host pipes witness data through without reinterpreting it.
pub fn build_stdin(
    verifier_key: VerifierKeyDigest,
    state: &State,
    header_bytes: Vec<u8>,
    median_hint_bytes: Vec<u8>,
    previous_proof: Option<(SP1ProofWithPublicValues, &sp1_prover::SP1VerifyingKey)>,
) -> Result<SP1Stdin, BoxError> {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(verifier_key.to_bytes().to_vec());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(header_bytes);
    stdin.write_vec(median_hint_bytes);

    if let Some((prev_proof, vk)) = previous_proof {
        let SP1Proof::Compressed(inner_proof) = prev_proof.proof else {
            return Err("previous proof is not compressed".into());
        };
        stdin.write_proof(*inner_proof, vk.vk.clone());
    }

    Ok(stdin)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use zkpow_params::Mainnet;

    use super::*;

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
    fn memory_logging_initialization_from_env() {
        let _guard = _env_test_mutex().lock().unwrap();

        for value in ["1", "true", "yes", "on"] {
            set_env(ENV_ZKPOW_MEMORY_LOGGING, value);
            let _ = config_from_env::<Mainnet>();
            assert!(crate::memory_monitor::logging_enabled(), "{value} should enable logging");
        }
        for value in ["0", "false", "no", "off"] {
            set_env(ENV_ZKPOW_MEMORY_LOGGING, value);
            let _ = config_from_env::<Mainnet>();
            assert!(!crate::memory_monitor::logging_enabled(), "{value} should disable logging");
        }
        remove_env(ENV_ZKPOW_MEMORY_LOGGING);
        let _ = config_from_env::<Mainnet>();
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
        assert_eq!(config.run_id, "develop_");

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
}
