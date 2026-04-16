//! Bitcoin Header Chain Prover — Host Script
//!
//! Usage:
//!   # Run 1: start from a host-selected genesis state
//!   cargo run --release --bin bitcoin-header-chain-script
//!
//!   # Run 2: Extend from previous proof
//!   PREV_PROOF=proof_height_1_to_100.bin cargo run --release --bin bitcoin-header-chain-script

use sp1_sdk::prelude::*;
use sp1_sdk::HashableKey;
use sp1_sdk::ProverClient;

use bitcoin_header_chain_script::util;
use bitcoin_header_chain_script::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest,
};

const ELF: Elf = include_elf!("bitcoin-header-chain-program");
const GENESIS_HASH_HEX: &str = "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
const DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bitcoin_headers.sqlite");

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();

    let prev_proof_path: Option<String> = std::env::var("PREV_PROOF").ok();
    let previous_proof = prev_proof_path
        .as_ref()
        .map(|path| SP1ProofWithPublicValues::load(path).expect("failed to load previous proof"));

    // Decode genesis hash (reversed display form → raw bytes)
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)
        .expect("genesis hash should decode")
        .try_into()
        .expect("genesis hash should be 32 bytes");
    genesis_hash.reverse();
    let genesis_hash = util::BlockHash::from_raw(genesis_hash);

    let num_headers: u32 = std::env::var("NUM_HEADERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    // Determine the current authenticated state.
    let current_state = if let Some(prev_proof) = previous_proof.as_ref() {
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

        state
    } else {
        let genesis_header = util::load_header_from_db(DB_PATH, 0);
        util::genesis_state(genesis_header, genesis_hash)
    };
    let start_height = current_state.height;
    let first_new_height = start_height + 1;

    tracing::info!(
        "Starting: tip_height={}, next_height={}, headers={}, prev_proof={}",
        start_height,
        first_new_height,
        num_headers,
        prev_proof_path.as_deref().unwrap_or("none"),
    );

    let raw_headers =
        util::load_headers_from_db(DB_PATH, first_new_height as u64, num_headers as u64);
    let headers = util::raw_headers_to_new_headers(&raw_headers);
    let loaded_count = headers.len() as u32;
    tracing::info!(
        "Loaded {} headers ({} raw bytes)",
        loaded_count,
        raw_headers.len(),
    );

    // Setup prover
    let client = ProverClient::from_env().await;
    let pk = client.setup(ELF).await.expect("failed to setup prover");

    // Get the VK for this program
    let vk = pk.verifying_key();
    let vk_digest_u32: [u32; 8] = vk.hash_u32();
    tracing::info!("VK digest: {:?}", vk_digest_u32);

    // Compute expected state by simulating locally
    let expected_state = util::compute_final_state(&current_state, &headers);

    // Build expected PV (the state bytes the program will commit)
    let expected_pv = expected_state.to_bytes();

    let recursive_proof = previous_proof.as_ref().map(|prev_proof| RecursiveProof {
        verifier_key: VerifierKeyDigest::from_raw(vk_digest_u32),
        public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(
            &prev_proof.public_values.to_vec(),
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

    // Execute (dry run)
    tracing::info!("Executing program (dry run)...");
    let (public_values, report) = client
        .execute(ELF, stdin.clone())
        .await
        .expect("execution failed");
    tracing::info!(
        "Execution succeeded: {} cycles",
        report.total_instruction_count()
    );

    // Check the output
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
                "Public values verified successfully ({} bytes — success)",
                actual_pv.len()
            );
        }
        HeaderChainPublicValues::Failure(failure) => {
            tracing::warn!(
                "Program exited with error: code={}, header_index={}",
                failure.error_code,
                failure.header_index,
            );
            panic!(
                "zkVM program failed with error {} at header {}",
                failure.error_code, failure.header_index,
            );
        }
    }

    // Verify proof
    tracing::info!("Generating compressed proof...");
    let proof = client
        .prove(&pk, stdin.clone())
        .compressed()
        .await
        .expect("proving failed");
    tracing::info!("Generated compressed proof");

    // Verify public values on the real proof
    let proof_pv = proof.public_values.to_vec();
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
                "Proof public values verified ({} bytes — success)",
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
    tracing::info!("Verifying proof...");
    client
        .verify(&proof, vk, None)
        .expect("verification failed");
    tracing::info!("Proof verified successfully");

    // Save compressed proof
    let proof_path = format!(
        "proof_height_{}_to_{}.bin",
        first_new_height,
        start_height + loaded_count,
    );
    proof.save(&proof_path).expect("failed to save proof");
    tracing::info!("Compressed proof saved to {}", proof_path);

    // Generate Groth16 proof for on-chain verification
    tracing::info!("Generating Groth16 proof...");
    let groth16_proof = client
        .prove(&pk, stdin)
        .groth16()
        .await
        .expect("Groth16 proving failed");
    tracing::info!("Groth16 proof generated");

    // Verify Groth16 proof
    client
        .verify(&groth16_proof, vk, None)
        .expect("Groth16 verification failed");
    tracing::info!("Groth16 proof verified");

    let groth16_path = proof_path.replace(".bin", "_groth16.bin");
    groth16_proof
        .save(&groth16_path)
        .expect("failed to save Groth16 proof");
    tracing::info!("Groth16 proof saved to {}", groth16_path);

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
