use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::util;
use crate::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest,
};
use num_bigint::BigUint;
use sp1_prover::build::{build_constraints_and_witness, try_build_groth16_artifacts_dir};
use sp1_prover::worker::{cpu_worker_builder, SP1LocalNodeBuilder};
use sp1_recursion_gnark_ffi::Groth16Bn254Prover;
use sp1_sdk::prelude::*;
use sp1_sdk::proof::SP1Proof;
use sp1_sdk::{HashableKey, SP1Context, SP1ProofWithPublicValues};

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

pub const ELF: Elf = include_elf!("bitcoin-header-chain-program");
pub const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
pub const DEFAULT_DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bitcoin_headers.sqlite");

#[derive(Debug, Clone)]
pub struct ProofGenerationConfig {
    pub prev_proof_path: Option<PathBuf>,
    pub num_headers: u32,
    pub db_path: PathBuf,
    pub output_dir: PathBuf,
}

#[derive(Debug)]
pub struct ProofArtifacts {
    pub compressed_path: PathBuf,
    pub groth16_path: PathBuf,
    pub compressed_proof: SP1ProofWithPublicValues,
    pub groth16_proof: SP1ProofWithPublicValues,
    pub first_new_height: u32,
    pub end_height: u32,
}

fn parse_genesis_hash() -> Result<util::BlockHash, BoxError> {
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)?
        .try_into()
        .map_err(|_| "genesis hash should be 32 bytes")?;
    genesis_hash.reverse();
    Ok(util::BlockHash::from_raw(genesis_hash))
}

pub fn config_from_env() -> ProofGenerationConfig {
    ProofGenerationConfig {
        prev_proof_path: std::env::var("PREV_PROOF").ok().map(PathBuf::from),
        num_headers: std::env::var("NUM_HEADERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100),
        db_path: PathBuf::from(DEFAULT_DB_PATH),
        output_dir: std::env::var("OUTPUT_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
    }
}

pub async fn generate_and_save_proofs(
    config: &ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    let previous_proof: Option<SP1ProofWithPublicValues> =
        timed_sync("load_previous_proof", || {
            config
                .prev_proof_path
                .as_ref()
                .map(SP1ProofWithPublicValues::load)
                .transpose()
        })?;

    let genesis_hash = timed_sync("parse_genesis_hash", parse_genesis_hash)?;

    let current_state = timed_sync("resolve_current_state", || -> Result<_, BoxError> {
        if let Some(prev_proof) = previous_proof.as_ref() {
            let prev_public_values =
                HeaderChainPublicValues::parse(prev_proof.public_values.as_ref())
                    .map_err(|err| err.to_string())?;
            let state = match prev_public_values {
                HeaderChainPublicValues::Success(state) => state,
                HeaderChainPublicValues::Failure(failure) => {
                    return Err(format!(
                        "previous proof ended in error: {} at header {}",
                        failure.error_code, failure.header_index,
                    )
                    .into());
                }
            };
            if state.genesis_hash != genesis_hash {
                return Err("previous proof genesis mismatch".into());
            }
            Ok(state)
        } else {
            let genesis_header = util::load_header_from_db(path_to_str(&config.db_path)?, 0);
            Ok(util::genesis_state(genesis_header, genesis_hash))
        }
    })?;

    let start_height = current_state.height;
    let first_new_height = start_height + 1;
    let raw_headers = timed_sync("load_headers", || -> Result<_, BoxError> {
        Ok(util::load_headers_from_db(
            path_to_str(&config.db_path)?,
            first_new_height as u64,
            config.num_headers as u64,
        ))
    })?;
    let headers = timed_sync("decode_headers", || -> Result<_, BoxError> {
        Ok(util::raw_headers_to_new_headers(&raw_headers))
    })?;
    let loaded_count = headers.len() as u32;

    let node = timed_async("build_local_node", || async {
        SP1LocalNodeBuilder::from_worker_client_builder(cpu_worker_builder())
            .build()
            .await
    })
    .await?;
    let vk = timed_async("setup_vkey", || async { node.setup(&ELF).await }).await?;
    let expected_state = timed_sync("simulate_expected_state", || -> Result<_, BoxError> {
        Ok(util::compute_final_state(&current_state, &headers))
    })?;
    let expected_pv = expected_state.to_bytes();

    let recursive_proof = timed_sync("build_recursive_proof", || -> Result<_, BoxError> {
        Ok(if let Some(prev_proof_val) = previous_proof.as_ref() {
            RecursiveProof {
                verifier_key: VerifierKeyDigest::from_raw(vk.hash_u32()),
                public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(
                    &prev_proof_val.public_values.to_vec(),
                )),
            }
        } else {
            RecursiveProof::default()
        })
    })?;
    let input = timed_sync("build_input", || -> Result<_, BoxError> {
        match Input::new(current_state.clone(), recursive_proof, headers.clone()) {
            Ok(input) => Ok(input),
            Err(err) => Err(err.to_string().into()),
        }
    })?;

    let mut stdin = SP1Stdin::new();
    timed_sync("serialize_input", || -> Result<(), BoxError> {
        stdin.write_vec(input.to_bytes());
        Ok(())
    })?;
    if let Some(prev_proof) = previous_proof.as_ref() {
        timed_sync("write_recursive_proof", || -> Result<(), BoxError> {
            let SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
                return Err("previous proof is not compressed".into());
            };
            stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
            Ok(())
        })?;
    }

    let (public_values, _, report) = timed_async("execute_program", || async {
        node.execute(&ELF, stdin.clone(), SP1Context::default())
            .await
    })
    .await?;
    tracing::info!(
        "Execution succeeded: {} cycles",
        report.total_instruction_count()
    );

    timed_sync(
        "verify_execution_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(&public_values.to_vec(), &expected_pv, "execution")
        },
    )?;

    let compressed_proof: SP1ProofWithPublicValues = timed_async("prove_compressed", || async {
        node.prove(&ELF, stdin.clone(), SP1Context::default()).await
    })
    .await?
    .into();
    timed_sync(
        "verify_compressed_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(
                &compressed_proof.public_values.to_vec(),
                &expected_pv,
                "compressed proof",
            )
        },
    )?;
    timed_sync("verify_compressed_proof", || -> Result<(), BoxError> {
        Ok(node.verify(&vk, &compressed_proof.proof)?)
    })?;

    timed_sync("create_output_dir", || -> Result<(), BoxError> {
        std::fs::create_dir_all(&config.output_dir)?;
        Ok(())
    })?;
    let compressed_path = config.output_dir.join(format!(
        "proof_height_{}_to_{}.bin",
        first_new_height,
        start_height + loaded_count,
    ));
    timed_sync("save_compressed_proof", || -> Result<(), BoxError> {
        compressed_proof.save(&compressed_path)?;
        Ok(())
    })?;

    let wrap_proof = timed_async("shrink_wrap", || async {
        node.shrink_wrap(&compressed_proof.proof).await
    })
    .await?;
    let build_dir = timed_async("build_groth16_artifacts", || async {
        try_build_groth16_artifacts_dir(&wrap_proof.vk, &wrap_proof.proof).await
    })
    .await?;
    let (_, witness) = timed_sync("build_groth16_witness", || -> Result<_, BoxError> {
        Ok(build_constraints_and_witness(
            &wrap_proof.vk,
            &wrap_proof.proof,
        )?)
    })?;
    let expected_vkey_hash = BigUint::from_bytes_be(&vk.bytes32_raw()).to_string();
    let expected_public_values_digest = compressed_proof.public_values.hash_bn254().to_string();

    let groth16_inner = timed_async("prove_groth16", || async move {
        tokio::task::spawn_blocking(move || {
            let prover = Groth16Bn254Prover::new();
            let proof = prover.prove(witness, &build_dir);
            let [vkey_hash, committed_values_digest, exit_code, vk_root, proof_nonce] =
                proof.public_inputs.clone();

            assert_eq!(
                vkey_hash, expected_vkey_hash,
                "Groth16 proof verifying-key hash does not match the program VK",
            );
            assert_eq!(
                committed_values_digest, expected_public_values_digest,
                "Groth16 proof public-values digest does not match the compressed proof",
            );

            let parse_biguint = |value: &str, label: &str| {
                value
                    .parse::<BigUint>()
                    .unwrap_or_else(|_| panic!("failed to parse {label} as BigUint"))
            };

            prover
                .verify(
                    &proof,
                    &parse_biguint(&vkey_hash, "Groth16 vkey hash"),
                    &parse_biguint(&committed_values_digest, "Groth16 public-values digest"),
                    &parse_biguint(&exit_code, "Groth16 exit code"),
                    &parse_biguint(&vk_root, "Groth16 recursion VK root"),
                    &parse_biguint(&proof_nonce, "Groth16 proof nonce"),
                    &build_dir,
                )
                .expect("native Groth16 verification failed");

            proof
        })
        .await
    })
    .await?;

    let groth16_proof = SP1ProofWithPublicValues::new(
        SP1Proof::Groth16(groth16_inner),
        compressed_proof.public_values.clone(),
        compressed_proof.sp1_version.clone(),
    );
    let groth16_path = config.output_dir.join(format!(
        "proof_height_{}_to_{}_groth16.bin",
        first_new_height,
        start_height + loaded_count,
    ));
    timed_sync("save_groth16_proof", || -> Result<(), BoxError> {
        groth16_proof.save(&groth16_path)?;
        Ok(())
    })?;

    Ok(ProofArtifacts {
        compressed_path,
        groth16_path,
        compressed_proof,
        groth16_proof,
        first_new_height,
        end_height: start_height + loaded_count,
    })
}

fn path_to_str(path: &Path) -> Result<&str, BoxError> {
    path.to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", path.display()).into())
}

fn timed_sync<T, E, F>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    tracing::info!("{label} started");
    let output = f().map_err(Into::into);
    tracing::info!("{label} finished in {:?}", started.elapsed());
    output
}

async fn timed_async<T, E, F, Fut>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    tracing::info!("{label} started");
    let output = f().await.map_err(Into::into);
    tracing::info!("{label} finished in {:?}", started.elapsed());
    output
}

fn verify_public_values(pv: &[u8], expected_pv: &[u8], label: &str) -> Result<(), BoxError> {
    let parsed = HeaderChainPublicValues::parse(pv).map_err(|err| err.to_string())?;
    match parsed {
        HeaderChainPublicValues::Success(state) => {
            if state.to_bytes() != expected_pv {
                return Err(format!(
                    "{label} public values mismatch: expected {}, got {}",
                    hex::encode(expected_pv),
                    hex::encode(state.to_bytes()),
                )
                .into());
            }
            Ok(())
        }
        HeaderChainPublicValues::Failure(failure) => Err(format!(
            "{label} ended in error {} at header {}",
            failure.error_code, failure.header_index,
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_output_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bitcoin-header-chain-proof-pipeline-{}-{}",
            std::process::id(),
            nanos,
        ))
    }

    #[tokio::test]
    async fn generates_linked_compressed_and_groth16_proofs() {
        let output_dir = unique_test_output_dir();
        let config = ProofGenerationConfig {
            prev_proof_path: None,
            num_headers: 1,
            db_path: PathBuf::from(DEFAULT_DB_PATH),
            output_dir: output_dir.clone(),
        };

        let artifacts = generate_and_save_proofs(&config)
            .await
            .expect("proof pipeline should succeed");

        assert_eq!(artifacts.first_new_height, 1);
        assert_eq!(artifacts.end_height, 1);
        assert!(artifacts.compressed_path.exists());
        assert!(artifacts.groth16_path.exists());

        let saved_compressed = SP1ProofWithPublicValues::load(&artifacts.compressed_path)
            .expect("saved compressed proof should load");
        let saved_groth16 = SP1ProofWithPublicValues::load(&artifacts.groth16_path)
            .expect("saved groth16 proof should load");

        assert_eq!(
            saved_compressed.public_values.to_vec(),
            saved_groth16.public_values.to_vec(),
            "saved Groth16 proof should commit to the same public values as the compressed proof",
        );
        assert_eq!(
            saved_compressed.public_values.hash_bn254().to_string(),
            match &saved_groth16.proof {
                SP1Proof::Groth16(proof) => proof.public_inputs[1].clone(),
                other => panic!("expected Groth16 proof, got {other:?}"),
            },
            "saved Groth16 proof should carry the compressed proof public-values digest",
        );
        match saved_compressed.proof {
            SP1Proof::Compressed(_) => {}
            other => panic!("expected compressed proof, got {other:?}"),
        }
    }
}
