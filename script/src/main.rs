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
    let headers_bytes = util::load_headers_from_db(DB_PATH, start_height, num_headers);
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
    let proof = client.prove(&pk, stdin.clone()).compressed()
        .await
        .expect("proving failed");
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

    // Save compressed proof
    let proof_path = format!(
        "proof_height_{}_to_{}.bin",
        start_height,
        start_height + loaded_count - 1
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

    tracing::info!(
        "Complete: validated {} headers from height {} to height {}",
        loaded_count,
        start_height,
        start_height + loaded_count - 1
    );
}

/// Compute the expected state after validating headers, optionally extending a previous state.
/// Sliding window constants — must match the program.
const WINDOW_SIZE: usize = 11;
const NIBBLE_BITS: usize = 4;
const NIBBLE_MASK: u64 = 0xF;

fn get_nibble(packed: u64, pos: usize) -> u8 {
    ((packed >> (pos * NIBBLE_BITS)) & NIBBLE_MASK) as u8
}

fn find_insert_position(
    timestamps: &[u32; WINDOW_SIZE],
    packed: u64,
    count: usize,
    ts: u32,
) -> usize {
    for i in 0..count {
        let idx = get_nibble(packed, i) as usize;
        if ts < timestamps[idx] {
            return i;
        }
    }
    count
}

fn find_index_position(packed: u64, count: usize, target: usize) -> usize {
    for i in 0..count {
        if get_nibble(packed, i) as usize == target {
            return i;
        }
    }
    count
}

fn remove_nibble(packed: u64, pos: usize, _count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed >> ((pos + 1) * NIBBLE_BITS)) << (pos * NIBBLE_BITS);
    lower | upper
}

fn insert_nibble(packed: u64, pos: usize, val: u8, count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed & !lower_mask) << NIBBLE_BITS;
    let new_packed = lower | ((val as u64) << (pos * NIBBLE_BITS)) | upper;
    let new_mask = (1u64 << ((count + 1) * NIBBLE_BITS)) - 1;
    new_packed & new_mask
}

fn rebuild_packed(timestamps: &[u32; WINDOW_SIZE], len: usize) -> u64 {
    let mut indices: [u8; WINDOW_SIZE] = [0; WINDOW_SIZE];
    let mut sorted_count = 0;
    for (i, ts) in timestamps.iter().take(len).enumerate() {
        let mut pos = sorted_count;
        for (j, idx) in indices.iter().take(sorted_count).enumerate() {
            if *ts < timestamps[*idx as usize] {
                pos = j;
                break;
            }
        }
        for k in (pos + 1..sorted_count + 1).rev() {
            indices[k] = indices[k - 1];
        }
        indices[pos] = i as u8;
        sorted_count += 1;
    }
    let mut packed = 0u64;
    for (i, idx) in indices.iter().take(sorted_count).enumerate() {
        packed |= (*idx as u64) << (i * NIBBLE_BITS);
    }
    packed
}

fn add_timestamp_window(
    timestamps: &mut [u32; WINDOW_SIZE],
    head: u8,
    len: usize,
    packed: u64,
    ts: u32,
) -> (u8, usize, u64) {
    if len < WINDOW_SIZE {
        timestamps[head as usize] = ts;
        let pos = find_insert_position(timestamps, packed, len, ts);
        let new_packed = insert_nibble(packed, pos, head, len);
        (
            (head + 1) % WINDOW_SIZE as u8,
            len + 1,
            new_packed,
        )
    } else {
        let pos_old = find_index_position(packed, len, head as usize);
        let packed_without = remove_nibble(packed, pos_old, len);
        let pos_new = find_insert_position(timestamps, packed_without, len - 1, ts);
        let new_packed = insert_nibble(packed_without, pos_new, head, len - 1);
        timestamps[head as usize] = ts;
        (
            (head + 1) % WINDOW_SIZE as u8,
            len,
            new_packed,
        )
    }
}

fn compute_expected_state(
    start_height: u64,
    num_headers: u64,
    headers_bytes: &[u8],
    prev_pv: Option<&[u8]>,
) -> ([u64; 4], u32, [u32; 11]) {
    let (mut work, mut epoch_ts, mut median, mut median_head, mut median_len, mut median_packed) =
        if let Some(pv) = prev_pv {
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
                    if total == 0 { 0u8 } else { (total % 11) as u8 }
                },
                {
                    let total = u64::from_le_bytes(pv[64..72].try_into().unwrap());
                    if total == 0 { 0usize } else { total.min(11) as usize }
                },
                {
                    let total = u64::from_le_bytes(pv[64..72].try_into().unwrap());
                    let len = if total == 0 { 0usize } else { total.min(11) as usize };
                    let ts: [u32; WINDOW_SIZE] = core::array::from_fn(|i| {
                        u32::from_le_bytes(pv[(188 + i * 4)..(192 + i * 4)].try_into().unwrap())
                    });
                    rebuild_packed(&ts, len)
                },
            )
        } else {
            ([0u64; 4], 1231006505u32, [0u32; WINDOW_SIZE], 0u8, 0usize, 0u64)
        };

    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header = &headers_bytes[offset..offset + 80];
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());
        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let w = util::work_from_bits(bits);
        work = util::u256_add(work, w);

        let height = start_height + i;
        (median_head, median_len, median_packed) =
            add_timestamp_window(&mut median, median_head, median_len, median_packed, timestamp);

        if height > 0 && height.is_multiple_of(2016) {
            epoch_ts = timestamp;
        }
    }

    (work, epoch_ts, median)
}
