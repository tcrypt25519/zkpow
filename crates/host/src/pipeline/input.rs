//! Host configuration (environment-driven) and guest stdin construction.

use std::{collections::HashMap, env::VarError, path::PathBuf};

use sp1_sdk::{prelude::*, SP1Proof};
use zkpow_controller::storage::S3Config;

use crate::{
    pipeline::{BoxError, PipelineMode, ProofGenerationConfig, ProverBackend},
    util,
    util::VerifierKeyDigest,
};

pub const ENV_ZKPOW_RUN_ID: &str = "ZKPOW_RUN_ID";
pub const ENV_ZKPOW_BATCH_SIZE: &str = "ZKPOW_BATCH_SIZE";
pub const ENV_ZKPOW_BATCH_COUNT: &str = "ZKPOW_BATCH_COUNT";
pub const ENV_ZKPOW_DB_PATH: &str = "ZKPOW_DB_PATH";
pub const ENV_ZKPOW_PROOF_STORE_DIR: &str = "ZKPOW_PROOF_STORE_DIR";
pub const ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS: &str = "ZKPOW_SHOW_COMPRESSED_PROOF_SPANS";
pub const ENV_ZKPOW_CONTROLLER_URL: &str = "ZKPOW_CONTROLLER_URL";
pub const ENV_ZKPOW_S3_BUCKET: &str = "ZKPOW_S3_BUCKET";
pub const ENV_ZKPOW_S3_REGION: &str = "ZKPOW_S3_REGION";
pub const ENV_ZKPOW_S3_ENDPOINT: &str = "ZKPOW_S3_ENDPOINT";
pub const ENV_ZKPOW_S3_ACCESS_KEY: &str = "ZKPOW_S3_ACCESS_KEY";
pub const ENV_ZKPOW_S3_SECRET_KEY: &str = "ZKPOW_S3_SECRET_KEY";
pub const ENV_ZKPOW_S3_PREFIX: &str = "ZKPOW_S3_PREFIX";

pub const ENV_ZKPOW_EXECUTE_ONLY: &str = "ZKPOW_EXECUTE_ONLY";
pub const ENV_ZKPOW_GENERATE_GROTH16: &str = "ZKPOW_GENERATE_GROTH16";

pub const ENV_ZKPOW_USE_CUDA: &str = "ZKPOW_USE_CUDA";
pub const ENV_ZKPOW_CUDA_DEVICE_ID: &str = "ZKPOW_CUDA_DEVICE_ID";

const DEFAULT_RUN_ID: &str = "dev";
const DEFAULT_PROOF_STORE_DIR: &str = "proof_store";
use zkpow_controller::DEFAULT_BATCH_SIZE;
const DEFAULT_BATCH_COUNT: u32 = 500;

pub(crate) trait EnvSource {
    fn get(&self, var_name: &str) -> Result<String, VarError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, var_name: &str) -> Result<String, VarError> {
        std::env::var(var_name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MapEnvSource {
    values: HashMap<String, String>,
}

impl MapEnvSource {
    pub fn new(values: HashMap<String, String>) -> Self {
        Self { values }
    }
}

impl EnvSource for MapEnvSource {
    fn get(&self, var_name: &str) -> Result<String, VarError> {
        self.values.get(var_name).cloned().ok_or(VarError::NotPresent)
    }
}

pub(crate) fn parse_bool_env(source: &impl EnvSource, var_name: &'static str) -> Result<bool, BoxError> {
    parse_bool_env_or(source, var_name, false)
}

pub(crate) fn parse_bool_env_or(
    source: &impl EnvSource,
    var_name: &'static str,
    default: bool,
) -> Result<bool, BoxError> {
    match source.get(var_name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => {
                Err(format!("invalid {var_name} value `{value}`; expected one of 1,true,yes,on,0,false,no,off").into())
            },
        },
        Err(VarError::NotPresent) => Ok(default),
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn parse_u32_env(source: &impl EnvSource, var_name: &'static str) -> Result<Option<u32>, BoxError> {
    match source.get(var_name) {
        Ok(value) => Ok(Some(
            value
                .parse()
                .map_err(|err| format!("invalid {var_name} value `{value}`: {err}"))?,
        )),
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn parse_u32_env_or(source: &impl EnvSource, var_name: &'static str, default: u32) -> Result<u32, BoxError> {
    Ok(parse_u32_env(source, var_name)?.unwrap_or(default))
}

fn parse_path_env<E: EnvSource>(source: &E, var_name: &'static str) -> Option<PathBuf> {
    source.get(var_name).ok().map(PathBuf::from)
}

pub(crate) fn ensure_cuda_requested_configuration(
    use_cuda: bool,
    cuda_device_id: Option<u32>,
) -> Result<ProverBackend, BoxError> {
    if !use_cuda {
        if cuda_device_id.is_some() {
            return Err(format!("{ENV_ZKPOW_CUDA_DEVICE_ID} is only valid when {ENV_ZKPOW_USE_CUDA}=1").into());
        }
        return Ok(ProverBackend::Cpu);
    }

    #[cfg(feature = "CUDA")]
    {
        return Ok(ProverBackend::Cuda);
    }

    #[cfg(not(feature = "CUDA"))]
    let _ = cuda_device_id;
    #[cfg(not(feature = "CUDA"))]
    Err(format!("{ENV_ZKPOW_USE_CUDA}=1 requires building zkpow-host with `--features CUDA`").into())
}

pub(crate) fn config_from_source(source: &impl EnvSource) -> Result<ProofGenerationConfig, BoxError> {
    let use_cuda = parse_bool_env(source, ENV_ZKPOW_USE_CUDA)?;
    let cuda_device_id = parse_u32_env(source, ENV_ZKPOW_CUDA_DEVICE_ID)?;

    let execute_only = parse_bool_env(source, ENV_ZKPOW_EXECUTE_ONLY)?;
    let generate_groth16 = parse_bool_env(source, ENV_ZKPOW_GENERATE_GROTH16)?;
    if execute_only && generate_groth16 {
        return Err(format!("{ENV_ZKPOW_GENERATE_GROTH16} cannot be enabled when {ENV_ZKPOW_EXECUTE_ONLY}=1").into());
    }

    let local_batch_size = parse_u32_env_or(source, ENV_ZKPOW_BATCH_SIZE, DEFAULT_BATCH_SIZE)?;
    let batch_count = parse_u32_env_or(source, ENV_ZKPOW_BATCH_COUNT, DEFAULT_BATCH_COUNT)?;

    let configured_backend = ensure_cuda_requested_configuration(use_cuda, cuda_device_id)?;
    let prover_backend = if execute_only && configured_backend == ProverBackend::Cpu {
        ProverBackend::Mock
    } else {
        configured_backend
    };

    Ok(ProofGenerationConfig {
        run_id: source.get(ENV_ZKPOW_RUN_ID).unwrap_or_else(|_| DEFAULT_RUN_ID.to_string()),
        local_batch_size,
        batch_count,
        db_path: parse_path_env(source, ENV_ZKPOW_DB_PATH).unwrap_or_else(zkpow_controller::db_path),
        mode: if execute_only {
            PipelineMode::ExecuteOnly
        } else if generate_groth16 {
            PipelineMode::ProveCompressedAndGroth16
        } else {
            PipelineMode::ProveCompressed
        },
        prover_backend,
        cuda_device_id,
    })
}

pub fn config_from_env() -> Result<ProofGenerationConfig, BoxError> {
    config_from_source(&ProcessEnv)
}

pub(crate) fn proof_store_dir_from_source(source: &impl EnvSource) -> PathBuf {
    parse_path_env(source, ENV_ZKPOW_PROOF_STORE_DIR).unwrap_or_else(|| PathBuf::from(DEFAULT_PROOF_STORE_DIR))
}

pub fn proof_store_dir_from_env() -> PathBuf {
    proof_store_dir_from_source(&ProcessEnv)
}

pub(crate) fn controller_url_from_source(source: &impl EnvSource) -> Option<String> {
    source.get(ENV_ZKPOW_CONTROLLER_URL).ok()
}

pub fn controller_url_from_env() -> Option<String> {
    controller_url_from_source(&ProcessEnv)
}

pub(crate) fn s3_storage_config_from_source(source: &impl EnvSource) -> Result<Option<S3Config>, BoxError> {
    let bucket = match source.get(ENV_ZKPOW_S3_BUCKET) {
        Ok(bucket) => bucket,
        Err(VarError::NotPresent) => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let require = |var_name: &'static str| -> Result<String, BoxError> {
        source
            .get(var_name)
            .map_err(|_| format!("{var_name} is required when {ENV_ZKPOW_S3_BUCKET} is set").into())
    };

    Ok(Some(S3Config {
        bucket,
        region: require(ENV_ZKPOW_S3_REGION)?,
        endpoint: require(ENV_ZKPOW_S3_ENDPOINT)?,
        access_key: require(ENV_ZKPOW_S3_ACCESS_KEY)?,
        secret_key: require(ENV_ZKPOW_S3_SECRET_KEY)?,
        prefix: source.get(ENV_ZKPOW_S3_PREFIX).unwrap_or_default(),
    }))
}

pub fn s3_storage_config_from_env() -> Result<Option<S3Config>, BoxError> {
    s3_storage_config_from_source(&ProcessEnv)
}

/// Assemble the guest's stdin. Header and median-hint bytes come straight
/// from the batch grant and the previous proof is moved in, not cloned —
/// the host pipes witness data through without reinterpreting it.
pub fn build_stdin(
    verifier_key: VerifierKeyDigest,
    state: &util::State,
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
