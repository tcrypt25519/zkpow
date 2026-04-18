//! Bitcoin Header Chain Prover — Host Script
//!
//! Usage:
//!   # Run 1: start from a host-selected genesis state
//!   cargo run --release --bin bitcoin-header-chain-script
//!
//!   # Run 2: Extend from previous proof
//!   PREV_PROOF=proof_height_1_to_100.bin cargo run --release --bin bitcoin-header-chain-script

use sp1_sdk::prelude::*;
use sp1_sdk::proof::SP1Proof;
use sp1_sdk::HashableKey;
use sp1_sdk::{SP1Context, SP1ProofWithPublicValues};

use num_bigint::BigUint;
use sp1_prover::build::{build_constraints_and_witness, try_build_groth16_artifacts_dir};
use sp1_prover::worker::{cpu_worker_builder, SP1LocalNodeBuilder};
use sp1_recursion_gnark_ffi::Groth16Bn254Prover;

use bitcoin_header_chain_script::util;
use bitcoin_header_chain_script::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest,
};

const ELF: Elf = include_elf!("bitcoin-header-chain-program");
const GENESIS_HASH_HEX: &str = "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
const DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bitcoin_headers.sqlite");

#[tokio::main]
async fn main() {
    println!("Host script started. For detailed tracing, set RUST_LOG=info (e.g., RUST_LOG=info cargo run).");
    sp1_sdk::utils::setup_logger();

    tracing::info!("Attempting to load previous proof, if any.");
    let prev_proof_path: Option<String> = std::env::var("PREV_PROOF").ok();
    let previous_proof = prev_proof_path
        .as_ref()
        .map(|path| SP1ProofWithPublicValues::load(path).expect("failed to load previous proof"));
    tracing::info!(
        "Previous proof status: {}",
        if previous_proof.is_some() {
            "loaded"
        } else {
            "none"
        }
    );

    tracing::info!("Decoding genesis hash: {}", GENESIS_HASH_HEX);
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)
        .expect("genesis hash should decode")
        .try_into()
        .expect("genesis hash should be 32 bytes");
    genesis_hash.reverse();
    let genesis_hash = util::BlockHash::from_raw(genesis_hash);
    tracing::info!("Genesis hash decoded.");

    tracing::info!("Determining number of headers to process.");
    let num_headers: u32 = std::env::var("NUM_HEADERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    tracing::info!("Processing {} headers.", num_headers);

    tracing::info!("Determining current authenticated state.");
    let current_state = if let Some(prev_proof) = previous_proof.as_ref() {
        tracing::info!("Loading state from previous proof.");
        let prev_public_values = HeaderChainPublicValues::parse(prev_proof.public_values.as_ref())
            .expect("failed to parse previous proof public values");
        let state = match prev_public_values {
            HeaderChainPublicValues::Success(state) => state,
            HeaderChainPublicValues::Failure(failure) => {
                panic!(
                    "previous proof ended in error: {} at header {}",
                    failure.error_code, failure.header_index,
                );
            }
        };
        assert_eq!(
            state.genesis_hash, genesis_hash,
            "Previous proof genesis mismatch",
        );
        tracing::info!("State loaded from previous proof, height: {}", state.height);
        state
    } else {
        tracing::info!("Creating genesis state.");
        let genesis_header = util::load_header_from_db(DB_PATH, 0);
        let state = util::genesis_state(genesis_header, genesis_hash);
        tracing::info!("Genesis state created, height: {}", state.height);
        state
    };
    let start_height = current_state.height;
    let first_new_height = start_height + 1;

    tracing::info!(
        "State initialization complete: current_state_height={}, next_height_to_process={}, num_headers_to_load={}, prev_proof_path={}",
        start_height,
        first_new_height,
        num_headers,
        prev_proof_path.as_deref().unwrap_or("none"),
    );

    tracing::info!("Loading raw headers from DB (path: {}, start_height: {}, count: {}).", DB_PATH, first_new_height, num_headers);
    let raw_headers =
        util::load_headers_from_db(DB_PATH, first_new_height as u64, num_headers as u64);
    let headers = util::raw_headers_to_new_headers(&raw_headers);
    let loaded_count = headers.len() as u32;
    tracing::info!(
        "Loaded {} headers ({} raw bytes) from DB.",
        loaded_count,
        raw_headers.len(),
    );

    tracing::info!("Initializing SP1 local prover node.");
    let node = SP1LocalNodeBuilder::from_worker_client_builder(cpu_worker_builder())
        .build()
        .await
        .expect("failed to initialize local prover node");
    tracing::info!("SP1 local prover node initialized.");

    tracing::info!("Setting up verifier key (VK) for the ELF program.");
    let vk = node.setup(&ELF).await.expect("failed to setup prover");
    let vk_digest_u32: [u32; 8] = vk.hash_u32();
    tracing::info!("Verifier key setup complete. VK digest: {:?}", vk_digest_u32);

    tracing::info!("Computing expected state by simulating locally.");
    let expected_state = util::compute_final_state(&current_state, &headers);
    tracing::info!("Expected state computed (tip_height: {}, chain_work: {:x?}).", expected_state.height, expected_state.chain_work);

    // Build expected PV (the state bytes the program will commit)
    let expected_pv = expected_state.to_bytes();

    tracing::info!("Preparing recursive proof and SP1 Stdin.");
    let recursive_proof = previous_proof
        .as_ref()
        .map(|prev_proof_val| RecursiveProof {
            verifier_key: VerifierKeyDigest::from_raw(vk_digest_u32),
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(
                &prev_proof_val.public_values.to_vec(),
            )),
        });
    let input = Input::new(current_state.clone(), recursive_proof, headers.clone())
        .expect("host input should satisfy invariants");

    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    if let Some(prev_proof) = previous_proof.as_ref() {
        let sp1_sdk::SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
            panic!("Previous proof is not compressed");
        };
        stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    }
    tracing::info!("SP1 Stdin prepared.");

    // Execute (dry run)
    tracing::info!("Executing program (dry run)...");
    let (public_values, _, report) = node
        .execute(&ELF, stdin.clone(), SP1Context::default())
        .await
        .expect("execution failed");
    tracing::info!(
        "Execution succeeded: {} cycles",
        report.total_instruction_count()
    );

    tracing::info!("Verifying public values from execution (dry run).");
    let actual_pv = public_values.to_vec();
    let parsed_public_values = HeaderChainPublicValues::parse(&actual_pv)
        .expect("failed to parse execution public values");

    match parsed_public_values {
        HeaderChainPublicValues::Success(state) => {
            assert_eq!(
                state.to_bytes(),
                expected_pv,
                "Public values mismatch!\n  expected: {}\n  actual:   {}",
                hex::encode(expected_pv),
                hex::encode(state.to_bytes()),
            );
            tracing::info!(
                "Public values from execution verified successfully ({} bytes — success).",
                actual_pv.len()
            );
        }
        HeaderChainPublicValues::Failure(failure) => {
            tracing::error!(
                "Program exited with error during dry run: code={}, header_index={}",
                failure.error_code,
                failure.header_index,
            );
            panic!(
                "zkVM program failed with error {} at header {}",
                failure.error_code, failure.header_index,
            );
        }
    }

    tracing::info!("Starting compressed proof generation.");
    let compressed_proof: SP1ProofWithPublicValues = node
        .prove(&ELF, stdin.clone(), SP1Context::default())
        .await
        .expect("proving failed")
        .into();
    tracing::info!("Compressed proof generated successfully.");

    // Verify public values on the real proof
    tracing::info!("Verifying public values from generated compressed proof.");
    let proof_pv = compressed_proof.public_values.to_vec();
    let parsed_proof_public_values =
        HeaderChainPublicValues::parse(&proof_pv).expect("failed to parse proof public values");

    match parsed_proof_public_values {
        HeaderChainPublicValues::Success(state) => {
            assert_eq!(
                state.to_bytes(),
                expected_pv,
                "Proof public values mismatch!\n  expected: {}\n  actual:   {}",
                hex::encode(expected_pv),
                hex::encode(state.to_bytes()),
            );
            tracing::info!(
                "Public values from compressed proof verified successfully ({} bytes — success).",
                proof_pv.len()
            );
        }
        HeaderChainPublicValues::Failure(failure) => {
            panic!(
                "generated proof ended in error {} at header {}",
                failure.error_code, failure.header_index,
            );
        }
    }

    // Verify proof
    tracing::info!("Verifying compressed proof cryptographically.");
    node.verify(&vk, &compressed_proof.proof)
        .expect("compressed proof verification failed");
    tracing::info!("Compressed proof verified successfully.");

    // Save compressed proof
    tracing::info!("Saving compressed proof to file.");
    let proof_path = format!(
        "proof_height_{}_to_{}.bin",
        first_new_height,
        start_height + loaded_count,
    );
    compressed_proof
        .save(&proof_path)
        .expect("failed to save proof");
    tracing::info!("Compressed proof saved to {}.", proof_path);

    tracing::info!("Generating shrink/wrap proof from compressed proof.");
    let wrap_proof = node
        .shrink_wrap(&compressed_proof.proof)
        .await
        .expect("failed to convert compressed proof into wrap proof");
    tracing::info!("Shrink/wrap proof generated.");

    tracing::info!("Preparing Groth16 witness from wrap proof.");
    let build_dir = try_build_groth16_artifacts_dir(&wrap_proof.vk, &wrap_proof.proof)
        .await
        .expect("failed to prepare Groth16 artifacts");
    let (_, witness) = build_constraints_and_witness(&wrap_proof.vk, &wrap_proof.proof)
        .expect("failed to build Groth16 witness");
    let expected_vkey_hash = BigUint::from_bytes_be(&vk.bytes32_raw()).to_string();
    let expected_public_values_digest = compressed_proof.public_values.hash_bn254().to_string();
    tracing::info!("Groth16 witness prepared.");

    tracing::info!("Generating and verifying Groth16 proof from wrapped proof.");
    let groth16_inner = tokio::task::spawn_blocking(move || {
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
    .expect("Groth16 proof task panicked");
    tracing::info!("Groth16 proof generated and verified.");

    let final_groth16_proof = SP1ProofWithPublicValues::new(
        SP1Proof::Groth16(groth16_inner),
        compressed_proof.public_values.clone(),
        compressed_proof.sp1_version.clone(),
    );

    tracing::info!("Saving Groth16 proof to file.");
    let groth16_path = proof_path.replace(".bin", "_groth16.bin");
    final_groth16_proof
        .save(&groth16_path)
        .expect("failed to save Groth16 proof");
    tracing::info!("Groth16 proof saved to {}.", groth16_path);

    // Print final state info
    tracing::info!(
        "Complete: validated {} headers from height {} to height {}",
        loaded_count,
        first_new_height,
        start_height + loaded_count,
    );
    let work_hex: String = expected_state
        .chain_work
        .as_limbs()
        .iter()
        .rev()
        .map(|w| format!("{:016x}", w))
        .collect();
    tracing::info!("Cumulative chain work: 0x{}", work_hex);
}
