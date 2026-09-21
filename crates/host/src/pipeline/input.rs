//! Host configuration (environment-driven) and guest stdin construction.

use std::path::PathBuf;

use sp1_sdk::{prelude::*, SP1Proof};

use crate::{
    pipeline::{BoxError, ProofGenerationConfig, ProverBackend},
    util,
    util::VerifierKeyDigest,
};

pub const ENV_ZKPOW_RUN_ID: &str = "ZKPOW_RUN_ID";
pub const ENV_ZKPOW_BATCH_SIZE: &str = "ZKPOW_BATCH_SIZE";
pub const ENV_ZKPOW_BATCH_COUNT: &str = "ZKPOW_BATCH_COUNT";
pub const ENV_ZKPOW_DB_PATH: &str = "ZKPOW_DB_PATH";
pub const ENV_ZKPOW_PROOF_STORE_DIR: &str = "ZKPOW_PROOF_STORE_DIR";
pub const ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS: &str = "ZKPOW_SHOW_COMPRESSED_PROOF_SPANS";

pub const ENV_ZKPOW_EXECUTE_ONLY: &str = "ZKPOW_EXECUTE_ONLY";

const DEFAULT_RUN_ID: &str = "dev";
const DEFAULT_PROOF_STORE_DIR: &str = "proof_store";
use zkpow_params::Consensus;
const DEFAULT_BATCH_COUNT: u32 = 500;

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

fn parse_path_env(var_name: &'static str) -> Option<PathBuf> {
    std::env::var_os(var_name).map(PathBuf::from)
}

pub fn config_from_env<C: Consensus>() -> Result<ProofGenerationConfig, BoxError> {
    let execute_only = parse_bool_env(ENV_ZKPOW_EXECUTE_ONLY)?;

    // One difficulty epoch per controller-selected batch unless overridden.
    let local_batch_size = parse_u32_env_or(ENV_ZKPOW_BATCH_SIZE, C::PARAMS.epoch_length)?;
    let batch_count = parse_u32_env_or(ENV_ZKPOW_BATCH_COUNT, DEFAULT_BATCH_COUNT)?;

    let prover_backend = if execute_only {
        ProverBackend::Mock
    } else {
        ProverBackend::Cpu
    };

    Ok(ProofGenerationConfig {
        run_id: std::env::var(ENV_ZKPOW_RUN_ID).unwrap_or_else(|_| DEFAULT_RUN_ID.to_string()),
        local_batch_size,
        batch_count,
        db_path: parse_path_env(ENV_ZKPOW_DB_PATH).unwrap_or_else(zkpow_controller::db_path),
        execute_only,
        prover_backend,
    })
}

pub fn proof_store_dir_from_env() -> PathBuf {
    parse_path_env(ENV_ZKPOW_PROOF_STORE_DIR).unwrap_or_else(|| PathBuf::from(DEFAULT_PROOF_STORE_DIR))
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
