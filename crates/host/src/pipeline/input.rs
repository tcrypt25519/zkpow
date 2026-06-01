use sp1_sdk::prelude::*;
use sp1_sdk::SP1Proof;
use std::env::VarError;
use std::path::PathBuf;

use crate::config::db_path;
use crate::pipeline::{BoxError, ProofGenerationConfig, ProverBackend};
use crate::util;
use crate::util::{Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest};

pub const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

pub fn parse_genesis_hash() -> Result<util::BlockHash, BoxError> {
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)?
        .try_into()
        .map_err(|_| "genesis hash should be 32 bytes")?;
    genesis_hash.reverse();
    Ok(util::BlockHash::new(genesis_hash))
}

pub fn parse_bool_env_with<F>(var_name: &'static str, mut get_env: F) -> Result<bool, BoxError>
where
    F: FnMut(&str) -> Result<String, VarError>,
{
    match get_env(var_name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!(
                "invalid {var_name} value `{value}`; expected one of 1,true,yes,on,0,false,no,off"
            )
            .into()),
        },
        Err(VarError::NotPresent) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

pub fn parse_bool_env(var_name: &'static str) -> Result<bool, BoxError> {
    parse_bool_env_with(var_name, |name| std::env::var(name))
}

pub fn parse_u32_env_with<F>(
    var_name: &'static str,
    mut get_env: F,
) -> Result<Option<u32>, BoxError>
where
    F: FnMut(&str) -> Result<String, VarError>,
{
    match get_env(var_name) {
        Ok(value) => {
            Ok(Some(value.parse().map_err(|err| {
                format!("invalid {var_name} value `{value}`: {err}")
            })?))
        }
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn parse_u32_env(var_name: &'static str) -> Result<Option<u32>, BoxError> {
    parse_u32_env_with(var_name, |name| std::env::var(name))
}

fn parse_u32_env_or_default<F>(
    var_name: &'static str,
    default: u32,
    mut get_env: F,
) -> Result<u32, BoxError>
where
    F: FnMut(&str) -> Result<String, VarError>,
{
    Ok(match parse_u32_env_with(var_name, &mut get_env)? {
        Some(value) => value,
        None => default,
    })
}

pub fn ensure_cuda_requested_configuration(
    use_cuda: bool,
    cuda_device_id: Option<u32>,
) -> Result<ProverBackend, BoxError> {
    if !use_cuda {
        if cuda_device_id.is_some() {
            return Err("CUDA_DEVICE_ID is only valid when CUDA=1".into());
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
    Err("CUDA=1 requires building zkpow-host with `--features CUDA`".into())
}

pub fn config_from_env_or_map<F>(mut get_env: F) -> Result<ProofGenerationConfig, BoxError>
where
    F: FnMut(&str) -> Result<String, VarError>,
{
    let use_cuda = parse_bool_env_with("CUDA", &mut get_env)?;
    let cuda_device_id = parse_u32_env_with("CUDA_DEVICE_ID", &mut get_env)?;

    let execute_only = parse_bool_env_with("EXECUTE_ONLY", &mut get_env)?;
    let generate_groth16 = parse_bool_env_with("GENERATE_GROTH16", &mut get_env)?;
    if execute_only && generate_groth16 {
        return Err("GENERATE_GROTH16 cannot be enabled when EXECUTE_ONLY=1".into());
    }
    let num_headers = parse_u32_env_or_default("NUM_HEADERS", 100, &mut get_env)?;

    let configured_backend = ensure_cuda_requested_configuration(use_cuda, cuda_device_id)?;
    let prover_backend = if execute_only && configured_backend == ProverBackend::Cpu {
        ProverBackend::Mock
    } else {
        configured_backend
    };

    Ok(ProofGenerationConfig {
        prev_proof_path: get_env("PREV_PROOF").ok().map(PathBuf::from),
        num_headers,
        db_path: get_env("DB_PATH")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(db_path())),
        output_dir: get_env("OUTPUT_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        generate_groth16,
        execute_only,
        prover_backend,
        cuda_device_id,
    })
}

pub fn config_from_env() -> Result<ProofGenerationConfig, BoxError> {
    config_from_env_or_map(|name| std::env::var(name))
}

pub fn build_recursive_proof(
    vk: &sp1_prover::SP1VerifyingKey,
    previous_proof: Option<&SP1ProofWithPublicValues>,
) -> Result<RecursiveProof, BoxError> {
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());
    Ok(if let Some(prev_proof_val) = previous_proof {
        let pv_bytes = prev_proof_val.public_values.to_vec();
        let previous_return_code = match zkpow_core::HeaderChainPublicValues::parse(&pv_bytes) {
            Ok(zkpow_core::HeaderChainPublicValues::Success { .. }) => 0u8,
            Ok(zkpow_core::HeaderChainPublicValues::Failure { failure, .. }) => {
                failure.error_code.as_byte()
            }
            Err(e) => {
                return Err(format!("failed to parse previous proof public values: {}", e).into())
            }
        };
        RecursiveProof {
            verifier_key,
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(&pv_bytes)),
            previous_return_code,
            ..Default::default()
        }
    } else {
        RecursiveProof {
            verifier_key,
            ..Default::default()
        }
    })
}

pub fn build_stdin(
    input: &Input,
    state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &util::MedianTimePastHints,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    vk: &sp1_prover::SP1VerifyingKey,
) -> Result<SP1Stdin, BoxError> {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(util::NewHeaderHintsRef { headers }.to_bytes());
    stdin.write_vec(median_hints.to_bytes());

    if let Some(prev_proof) = previous_proof {
        let SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
            return Err("previous proof is not compressed".into());
        };
        stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    }

    Ok(stdin)
}
