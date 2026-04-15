//! Automated tests for error handling in the Bitcoin header chain prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code,
//! then runs the zkVM program via `client.execute()` and checks the public values.
//!
//! Input protocol (header-construction architecture):
//!   stdin: prev_height(u32) → [prev_vk(32) + pv_digest(32) + pv_bytes] → num_headers(u32) → headers(44 bytes each)
//!   output: state(192) on success, or state(192) + error_code(1) + header_index(4) on error

use sp1_sdk::prelude::*;
use sp1_sdk::utils;
use sp1_sdk::{Elf, HashableKey, Prover, ProverClient, SP1Stdin};

use bitcoin_header_chain_script::util;
use bitcoin_header_chain_script::util::{
    HeaderChainPublicValues, ValidationErrorCode, NEW_HEADER_SIZE,
};

const ELF: Elf = include_elf!("bitcoin-header-chain-program");

const DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bitcoin_headers.sqlite",);

/// Run the program with given inputs and return the raw public values.
async fn run_and_get_pv(stdin: SP1Stdin) -> Result<Vec<u8>, String> {
    let client = ProverClient::builder().mock().build().await;
    let (public_values, _report) = client
        .execute(ELF, stdin)
        .await
        .map_err(|e| format!("execution failed: {}", e))?;
    Ok(public_values.to_vec())
}

fn expect_success(pv: &[u8]) -> Result<util::State, String> {
    let parsed =
        HeaderChainPublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;
    match parsed {
        HeaderChainPublicValues::Success(state) => Ok(state),
        HeaderChainPublicValues::Failure(failure) => Err(format!(
            "expected success, got error {} at header {}",
            failure.error_code, failure.header_index,
        )),
    }
}

fn expect_failure(
    pv: &[u8],
    expected_code: ValidationErrorCode,
    expected_header_index: u32,
) -> Result<(), String> {
    let parsed =
        HeaderChainPublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;

    match parsed {
        HeaderChainPublicValues::Success(state) => Err(format!(
            "expected error {}, got success at height {}",
            expected_code, state.height,
        )),
        HeaderChainPublicValues::Failure(failure) => {
            if failure.error_code != expected_code {
                return Err(format!(
                    "expected error {}, got {}",
                    expected_code, failure.error_code,
                ));
            }
            if failure.header_index != expected_header_index {
                return Err(format!(
                    "expected header index {}, got {}",
                    expected_header_index, failure.header_index,
                ));
            }
            Ok(())
        }
    }
}

fn check(name: &str, result: Result<(), String>) {
    match result {
        Ok(()) => println!("  {} ... ok", name),
        Err(e) => println!("  {} ... FAILED: {}", name, e),
    }
}

fn raw_header_bits(raw_headers: &[u8], height: usize) -> Result<u32, String> {
    let start = height
        .checked_mul(80)
        .ok_or_else(|| format!("header offset overflow for height {}", height))?;
    let end = start + 80;
    let raw_header = raw_headers
        .get(start..end)
        .ok_or_else(|| format!("missing raw header at height {}", height))?;
    let bits = raw_header
        .get(72..76)
        .ok_or_else(|| format!("missing bits field at height {}", height))?;
    let bits: [u8; 4] = bits
        .try_into()
        .map_err(|_| format!("invalid bits field width at height {}", height))?;
    Ok(u32::from_le_bytes(bits))
}

#[tokio::main]
async fn main() {
    utils::setup_logger();
    println!("Running error handling tests...\n");

    // === Success cases ===
    check("success_100_headers", test_success_100_headers().await);
    check(
        "retarget_boundary_schedule",
        test_retarget_boundary_schedule().await,
    );
    check(
        "recursive_chain_success",
        test_recursive_chain_success().await,
    );

    // === Input validation errors ===
    check(
        "error_header_count_mismatch",
        test_error_header_count_mismatch().await,
    );

    // === Timestamp validation errors ===
    check(
        "error_timestamp_too_old",
        test_error_timestamp_too_old().await,
    );

    // === PoW errors ===
    check(
        "error_pow_insufficient",
        test_error_pow_insufficient().await,
    );

    println!("\nDone.");
}

// ============================================================================
// Tests
// ============================================================================

async fn test_success_100_headers() -> Result<(), String> {
    let raw_headers = util::load_headers_from_db(DB_PATH, 0, 100);
    let headers_bytes = util::raw_headers_to_new_headers(&raw_headers);

    let mut stdin = SP1Stdin::new();
    stdin.write::<u32>(&0); // prev_height = 0 (genesis start)
    stdin.write::<u32>(&100);
    stdin.write_vec(headers_bytes);

    let pv = run_and_get_pv(stdin).await?;
    expect_success(&pv).map(|_| ())
}

async fn test_retarget_boundary_schedule() -> Result<(), String> {
    const FIRST_BOUNDARY_HEIGHT: usize = 2016;
    // Mainnet first changes difficulty at height 32256. We simulate through the
    // end of the previous epoch and assert the state carries the exact bits that
    // appear in the next raw header.
    const RETARGET_HEIGHT: usize = 32256;
    const EPOCH_LENGTH: usize = 2016;

    let first_epoch_raw = util::load_headers_from_db(DB_PATH, 0, FIRST_BOUNDARY_HEIGHT as u64);
    let first_epoch_headers = util::raw_headers_to_new_headers(&first_epoch_raw);
    let first_epoch_state =
        util::compute_expected_state(0, FIRST_BOUNDARY_HEIGHT as u32, &first_epoch_headers, None);
    let first_retarget_bits = raw_header_bits(
        &util::load_headers_from_db(DB_PATH, FIRST_BOUNDARY_HEIGHT as u64, 1),
        0,
    )?;
    if first_epoch_state.nbits != first_retarget_bits {
        return Err(format!(
            "expected first retarget boundary bits {:#x}, got {:#x}",
            first_retarget_bits, first_epoch_state.nbits,
        ));
    }

    let raw_headers = util::load_headers_from_db(DB_PATH, 0, RETARGET_HEIGHT as u64);
    let new_headers = util::raw_headers_to_new_headers(&raw_headers);
    let state = util::compute_expected_state(0, RETARGET_HEIGHT as u32, &new_headers, None);

    if state.height != RETARGET_HEIGHT as u32 {
        return Err(format!(
            "expected validated height {}, got {}",
            RETARGET_HEIGHT, state.height,
        ));
    }

    let previous_epoch_bits = raw_header_bits(&raw_headers, RETARGET_HEIGHT - 1)?;
    let next_header_bits = raw_header_bits(
        &util::load_headers_from_db(DB_PATH, RETARGET_HEIGHT as u64, 1),
        0,
    )?;

    if previous_epoch_bits == next_header_bits {
        return Err(format!(
            "expected a real retarget boundary at height {}, but bits stayed at {:#x}",
            RETARGET_HEIGHT, next_header_bits,
        ));
    }

    if state.nbits != next_header_bits {
        return Err(format!(
            "expected next-header bits {:#x} after completing epoch, got {:#x}",
            next_header_bits, state.nbits,
        ));
    }

    let pre_boundary_raw = util::load_headers_from_db(DB_PATH, 0, (RETARGET_HEIGHT - 1) as u64);
    let pre_boundary_headers = util::raw_headers_to_new_headers(&pre_boundary_raw);
    let pre_boundary_state =
        util::compute_expected_state(0, (RETARGET_HEIGHT - 1) as u32, &pre_boundary_headers, None);

    if pre_boundary_state.nbits != previous_epoch_bits {
        return Err(format!(
            "expected pre-boundary bits {:#x}, got {:#x}",
            previous_epoch_bits, pre_boundary_state.nbits,
        ));
    }

    if (pre_boundary_state.height as usize).is_multiple_of(EPOCH_LENGTH) {
        return Err("pre-boundary state unexpectedly ended on an epoch boundary".to_string());
    }

    Ok(())
}

async fn test_error_header_count_mismatch() -> Result<(), String> {
    let raw_headers = util::load_headers_from_db(DB_PATH, 0, 10);
    let headers_bytes = util::raw_headers_to_new_headers(&raw_headers);

    let mut stdin = SP1Stdin::new();
    stdin.write::<u32>(&0);
    stdin.write::<u32>(&20); // Claim 20, provide only 10 * 44 bytes
    stdin.write_vec(headers_bytes);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::HeaderCountMismatch, 0)
}

async fn test_error_timestamp_too_old() -> Result<(), String> {
    // Load blocks 0-12 so the median buffer is full (11 blocks after genesis).
    // Block 12's median check uses blocks 1-11 timestamps.
    // We corrupt block 12's timestamp to be older than the median.
    let raw_headers = util::load_headers_from_db(DB_PATH, 0, 13);
    let mut headers_bytes = util::raw_headers_to_new_headers(&raw_headers);

    // Set block 12's timestamp (offset 12*44 + 36) to genesis timestamp,
    // which is older than all block 1-11 timestamps.
    // NewHeader layout: version(4) + merkle_root(32) + timestamp(4) + nonce(4)
    // timestamp is at offset 36 within each 44-byte NewHeader
    let offset = 12 * NEW_HEADER_SIZE + 36;
    let genesis_ts: u32 = 1231006505;
    headers_bytes[offset] = (genesis_ts & 0xFF) as u8;
    headers_bytes[offset + 1] = ((genesis_ts >> 8) & 0xFF) as u8;
    headers_bytes[offset + 2] = ((genesis_ts >> 16) & 0xFF) as u8;
    headers_bytes[offset + 3] = ((genesis_ts >> 24) & 0xFF) as u8;

    let mut stdin = SP1Stdin::new();
    stdin.write::<u32>(&0);
    stdin.write::<u32>(&13);
    stdin.write_vec(headers_bytes);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::TimestampTooOld, 12)
}

async fn test_error_pow_insufficient() -> Result<(), String> {
    // Load blocks 0-1, corrupt block 1's nonce.
    let raw_headers = util::load_headers_from_db(DB_PATH, 0, 2);
    let mut headers_bytes = util::raw_headers_to_new_headers(&raw_headers);

    // Block 1 starts at offset 44. NewHeader nonce is at offset 40 within each 44-byte struct.
    let off = NEW_HEADER_SIZE + 40;
    headers_bytes[off] ^= 0xFF; // corrupt block 1's nonce

    let mut stdin = SP1Stdin::new();
    stdin.write::<u32>(&0);
    stdin.write::<u32>(&2);
    stdin.write_vec(headers_bytes);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::PowInsufficient, 1)
}

async fn test_recursive_chain_success() -> Result<(), String> {
    let client = ProverClient::from_env().await;
    let pk = client
        .setup(ELF)
        .await
        .map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();

    // === Run 1: Genesis → Block 9 ===
    let raw1 = util::load_headers_from_db(DB_PATH, 0, 10);
    let headers1 = util::raw_headers_to_new_headers(&raw1);
    let mut stdin1 = SP1Stdin::new();
    stdin1.write::<u32>(&0); // prev_height = 0
    stdin1.write::<u32>(&10);
    stdin1.write_vec(headers1);

    let proof1 = client
        .prove(&pk, stdin1)
        .compressed()
        .await
        .map_err(|e| format!("Run 1 proof generation failed: {}", e))?;
    let pv1_bytes = proof1.public_values.to_vec();

    // Verify Run 1 state
    let state1 = expect_success(&pv1_bytes)?;
    if state1.height != 10 {
        return Err(format!("Run 1: expected height 10, got {}", state1.height));
    }

    // === Run 2: Extend from Run 1 (blocks 10-19) ===
    let raw2 = util::load_headers_from_db(DB_PATH, 10, 10);
    let headers2 = util::raw_headers_to_new_headers(&raw2);
    let mut stdin2 = SP1Stdin::new();
    stdin2.write::<u32>(&10); // prev_height = 10
    stdin2.write::<[u32; 8]>(&vk.hash_u32());
    let pv_digest = util::compute_pv_digest(&pv1_bytes);
    stdin2.write::<[u8; 32]>(&pv_digest);
    stdin2.write_vec(state1.to_bytes());
    let sp1_sdk::SP1Proof::Compressed(inner_proof) = &proof1.proof else {
        return Err("Run 1 proof is not compressed".to_string());
    };
    stdin2.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    stdin2.write::<u32>(&10);
    stdin2.write_vec(headers2);

    let (pv2, _) = client
        .execute(ELF, stdin2)
        .await
        .map_err(|e| format!("Run 2 execution failed: {}", e))?;
    let pv2 = pv2.to_vec();
    expect_success(&pv2).map(|_| ())
}
