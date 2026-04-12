//! Bitcoin Header Chain Prover — Host Script
//!
//! Usage:
//!   # Run 1: Genesis → Block 99
//!   cargo run --release --bin bitcoin-header-chain-script
//!
//!   # Run 2: Extend from previous proof
//!   PREV_PROOF=proof_height_0_to_99.bin START_HEIGHT=100 NUM_HEADERS=100 \
//!     cargo run --release --bin bitcoin-header-chain-script

use sp1_sdk::prelude::*;
use sp1_sdk::ProverClient;
use sp1_sdk::HashableKey;

use bitcoin_header_chain_script::util;

const ELF: Elf = include_elf!("bitcoin-header-chain-program");
const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
const DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bitcoin_headers.sqlite");

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();

    let start_height: u64 = std::env::var("START_HEIGHT").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let num_headers: u64 = std::env::var("NUM_HEADERS").ok().and_then(|s| s.parse().ok()).unwrap_or(100);
    let prev_proof_path: Option<String> = std::env::var("PREV_PROOF").ok();
    let has_prev_proof = prev_proof_path.is_some();

    // Decode genesis hash (reversed display form → raw bytes)
    let mut genesis_hash = [0u8; 32];
    genesis_hash.copy_from_slice(&hex::decode(GENESIS_HASH_HEX).unwrap());
    genesis_hash.reverse();

    tracing::info!("Starting: height={}, headers={}, prev_proof={}",
        start_height, num_headers, prev_proof_path.as_deref().unwrap_or("none"));

    // Load raw headers
    let headers_bytes = util::load_headers_from_db(&DB_PATH, start_height, num_headers);
    let loaded_count = (headers_bytes.len() / 80) as u64;
    tracing::info!("Loaded {} headers", loaded_count);

    // Setup prover
    let client = ProverClient::from_env().await;
    let pk = client.setup(ELF).await.expect("failed to setup prover");

    // Get the VK for this program
    let vk = pk.verifying_key();
    let vk_digest_u32: [u32; 8] = vk.hash_u32();
    tracing::info!("VK digest: {:?}", vk_digest_u32);

    // If extending a previous proof, extract state from its public values
    let prev_pv: Option<Vec<u8>> = if has_prev_proof {
        let path = prev_proof_path.as_ref().unwrap();
        let prev_proof = SP1ProofWithPublicValues::load(path).expect("failed to load previous proof");
        let pv = prev_proof.public_values.as_ref();

        assert!(pv.len() >= 237, "Previous proof PV too short: {}", pv.len());
        assert_eq!(pv[232], 0, "Previous proof did not succeed");
        assert_eq!(&pv[0..32], &genesis_hash[..], "Previous proof genesis mismatch");

        Some(pv.to_vec())
    } else {
		assert_eq!(start_height, 0, "Previous proof required for non-genesis header");
        None
    };

    // Compute expected outputs by running the same logic the zkVM will run
    let (expected_cumulative_work, expected_epoch_ts, expected_median) =
        compute_expected_state(start_height, loaded_count, &headers_bytes, prev_pv.as_deref());

    let final_header: [u8; 80] = headers_bytes[(headers_bytes.len() - 80)..].try_into().unwrap();
    let final_hash = util::double_sha256_host(&final_header);

    // Prepare stdin
    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&genesis_hash);
    stdin.write::<bool>(&has_prev_proof);

    if has_prev_proof {
        let pv_bytes = prev_pv.as_ref().unwrap();
        // VK digest as [u32; 8]
        stdin.write::<[u32; 8]>(&vk_digest_u32);
        // PV digest: SHA-256 of the previous proof's public values
        let pv_digest: [u8; 32] = util::compute_pv_digest(pv_bytes);
        stdin.write::<[u8; 32]>(&pv_digest);
        // Previous public values (so the guest can extract starting state)
        stdin.write_vec(pv_bytes.clone());

        // Write the actual proof for recursive verification
        let path = prev_proof_path.as_ref().unwrap();
        let prev_proof = SP1ProofWithPublicValues::load(path).expect("failed to load proof");
        let sp1_sdk::SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
            panic!("Previous proof is not compressed");
        };
        stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    }

    stdin.write::<u64>(&start_height);
    stdin.write::<u64>(&loaded_count);
    stdin.write_vec(headers_bytes.clone());

    // Execute (dry run)
    tracing::info!("Executing program (dry run)...");
    let (_, report) = client.execute(ELF, stdin.clone()).await.expect("execution failed");
    tracing::info!("Execution succeeded: {} cycles", report.total_instruction_count());

    // Generate compressed proof
    tracing::info!("Generating compressed proof...");
    let proof = client.prove(&pk, stdin).compressed().await.expect("proving failed");
    tracing::info!("Generated compressed proof");

    // Verify public values
    let total_headers = if has_prev_proof {
        let pv = prev_pv.as_ref().unwrap();
        u64::from_le_bytes(pv[64..72].try_into().unwrap()) + loaded_count
    } else {
        loaded_count
    };

    let expected_pv = util::build_expected_public_values(
        &genesis_hash, &final_hash, total_headers, &final_header,
        expected_cumulative_work, expected_epoch_ts, expected_median,
    );
    let actual_pv = proof.public_values.to_vec();
    assert_eq!(actual_pv, expected_pv,
        "Public values mismatch!\n  expected: {}\n  actual:   {}",
        hex::encode(&expected_pv), hex::encode(&actual_pv));
    tracing::info!("Public values verified successfully ({} bytes)", actual_pv.len());

    // Verify proof
    tracing::info!("Verifying proof...");
    client.verify(&proof, vk, None).expect("verification failed");
    tracing::info!("Proof verified successfully");

    // Save
    let proof_path = format!("proof_height_{}_to_{}.bin", start_height, start_height + loaded_count - 1);
    proof.save(&proof_path).expect("failed to save proof");
    tracing::info!("Proof saved to {}", proof_path);
}

/// Compute the expected state after validating headers, optionally extending a previous state.
fn compute_expected_state(
    start_height: u64,
    num_headers: u64,
    headers_bytes: &[u8],
    prev_pv: Option<&[u8]>,
) -> ([u64; 4], u32, [u32; 11]) {
    let (mut work, mut epoch_ts, mut median, mut median_count) = if let Some(pv) = prev_pv {
        (
            [
                u64::from_le_bytes(pv[152..160].try_into().unwrap()),
                u64::from_le_bytes(pv[160..168].try_into().unwrap()),
                u64::from_le_bytes(pv[168..176].try_into().unwrap()),
                u64::from_le_bytes(pv[176..184].try_into().unwrap()),
            ],
            u32::from_le_bytes(pv[184..188].try_into().unwrap()),
            core::array::from_fn(|i| u32::from_le_bytes(pv[(188 + i * 4)..(192 + i * 4)].try_into().unwrap())),
            {
                let total = u64::from_le_bytes(pv[64..72].try_into().unwrap());
                if total == 0 { 0u32 } else { total.min(11) as u32 }
            },
        )
    } else {
        ([0u64; 4], 1231006505u32, [0u32; 11], 0u32)
    };

    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header = &headers_bytes[offset..offset + 80];
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());
        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let w = util::work_from_bits(bits);
        work = util::u256_add(work, w);

        let height = start_height + i;
        if height == 0 {
            median[0] = timestamp;
            median_count = 1;
        } else {
            if median_count < 11 {
                median[median_count as usize] = timestamp;
                median_count += 1;
            } else {
                for j in 0..10 {
                    median[j as usize] = median[(j + 1) as usize];
                }
                median[10] = timestamp;
            }
        }

        if height > 0 && height % 2016 == 0 {
            epoch_ts = timestamp;
        }
    }

    (work, epoch_ts, median)
}
