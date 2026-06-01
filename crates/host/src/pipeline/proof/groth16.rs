use num_bigint::BigUint;
use sp1_prover::build::{build_constraints_and_witness, try_build_groth16_artifacts_dir};
use sp1_prover::worker::{cpu_worker_builder, SP1LocalNodeBuilder};
use sp1_recursion_gnark_ffi::Groth16Bn254Prover;
use sp1_sdk::prelude::*;
use sp1_sdk::proof::SP1Proof;
use std::path::PathBuf;

use crate::pipeline::diagnostics::{timed_async, timed_sync};
use crate::pipeline::BoxError;

pub async fn generate_groth16_proof(
    compressed_proof: &SP1ProofWithPublicValues,
    vk: &sp1_prover::SP1VerifyingKey,
) -> Result<SP1ProofWithPublicValues, BoxError> {
    let node = timed_async("build_local_node", || async {
        SP1LocalNodeBuilder::from_worker_client_builder(cpu_worker_builder())
            .build()
            .await
    })
    .await?;
    let wrap_proof = timed_async("shrink_wrap", || async {
        node.shrink_wrap(&compressed_proof.proof).await
    })
    .await?;
    let build_dir: PathBuf = timed_async("build_groth16_artifacts", || async {
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

    Ok(SP1ProofWithPublicValues::new(
        SP1Proof::Groth16(groth16_inner),
        compressed_proof.public_values.clone(),
        compressed_proof.sp1_version.clone(),
    ))
}
