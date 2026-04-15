//! Automated tests for error handling in the Bitcoin header chain prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code,
//! then runs the zkVM program via `client.execute()` and checks the public values.
//!
//! Input protocol (header-construction architecture):
//!   stdin: prev_height(u32) → [prev_vk(32) + pv_digest(32) + pv_bytes] → num_headers(u32) → headers(44 bytes each)
//!   output: state(192) on success, or state(192) + error_code(1) + header_index(4) on error

use sp1_sdk::prelude::*;
use sp1_sdk::{Prover, SP1Stdin, HashableKey, Elf};
use sp1_sdk::utils;

use bitcoin_header_chain_script::util;
use bitcoin_header_chain_script::util::{STATE_SIZE, NEW_HEADER_SIZE};

const ELF: Elf = include_elf!("bitcoin-header-chain-program");

const DB_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../bitcoin_headers.sqlite",
);

// Error codes from the program (revised — header-construction architecture)
#[allow(dead_code)]
const STATUS_SUCCESS: u8 = 0;
const STATUS_HEADER_COUNT_MISMATCH: u8 = 1;
const STATUS_POW_INSUFFICIENT: u8 = 2;
const STATUS_TIMESTAMP_TOO_OLD: u8 = 3;
#[allow(dead_code)]
const STATUS_GENESIS_HASH_MISMATCH: u8 = 4;
// Removed: prev_blockhash mismatch, bits mismatch — impossible by construction

/// Run the program with given inputs and return the raw public values.
async fn run_and_get_pv(stdin: SP1Stdin) -> Result<Vec<u8>, String> {
    let client = sp1_sdk::ProverClient::from_env().await;
    let (public_values, _report) = client
        .execute(ELF, stdin)
        .await
        .map_err(|e| format!("execution failed: {}", e))?;
    Ok(public_values.to_vec())
}

/// Parse the result: returns Ok(()) on success, or Err((error_code, header_index)) on failure.
fn parse_result(pv: &[u8]) -> Result<(), (u8, u32)> {
    if pv.len() == STATE_SIZE {
        Ok(())
    } else if pv.len() == STATE_SIZE + 1 + 4 {
        let error_code = pv[STATE_SIZE];
        let header_index =
            u32::from_le_bytes(pv[STATE_SIZE + 1..STATE_SIZE + 5].try_into().unwrap());
        Err((error_code, header_index))
    } else {
        panic!("Unexpected PV length: {} bytes", pv.len());
    }
}

fn check(name: &str, result: Result<(), String>) {
    match result {
        Ok(()) => println!("  {} ... ok", name),
        Err(e) => println!("  {} ... FAILED: {}", name, e),
    }
}

#[tokio::main]
async fn main() {
    utils::setup_logger();
    println!("Running error handling tests...\n");

    // === Success cases ===
    check("success_100_headers", test_success_100_headers().await);
    check("recursive_chain_success", test_recursive_chain_success().await);

    // === Input validation errors ===
    check("error_header_count_mismatch", test_error_header_count_mismatch().await);

    // === Timestamp validation errors ===
    check("error_timestamp_too_old", test_error_timestamp_too_old().await);

    // === PoW errors ===
    check("error_pow_insufficient", test_error_pow_insufficient().await);

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
    parse_result(&pv).map_err(|(code, detail)| {
        format!("expected success, got error code {} detail {}", code, detail)
    })
}

async fn test_error_header_count_mismatch() -> Result<(), String> {
    let raw_headers = util::load_headers_from_db(DB_PATH, 0, 10);
    let headers_bytes = util::raw_headers_to_new_headers(&raw_headers);

    let mut stdin = SP1Stdin::new();
    stdin.write::<u32>(&0);
    stdin.write::<u32>(&20); // Claim 20, provide only 10 * 44 bytes
    stdin.write_vec(headers_bytes);

    let pv = run_and_get_pv(stdin).await?;
    match parse_result(&pv) {
        Err((code, _detail)) => {
            if code != STATUS_HEADER_COUNT_MISMATCH {
                return Err(format!(
                    "expected HEADER_COUNT_MISMATCH ({}), got {}",
                    STATUS_HEADER_COUNT_MISMATCH, code
                ));
            }
            Ok(())
        }
        Ok(()) => Err("expected error but got success".to_string()),
    }
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
    match parse_result(&pv) {
        Err((code, detail)) => {
            if code != STATUS_TIMESTAMP_TOO_OLD {
                return Err(format!(
                    "expected TIMESTAMP_TOO_OLD ({}), got {}",
                    STATUS_TIMESTAMP_TOO_OLD, code
                ));
            }
            if detail != 12 {
                return Err(format!("expected detail 12, got {}", detail));
            }
            Ok(())
        }
        Ok(()) => Err("expected error but got success".to_string()),
    }
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
    match parse_result(&pv) {
        Err((code, detail)) => {
            if code != STATUS_POW_INSUFFICIENT {
                return Err(format!(
                    "expected POW_INSUFFICIENT ({}), got {}",
                    STATUS_POW_INSUFFICIENT, code
                ));
            }
            if detail != 1 {
                return Err(format!("expected detail 1 (block 1), got {}", detail));
            }
            Ok(())
        }
        Ok(()) => Err("expected error but got success".to_string()),
    }
}

async fn test_recursive_chain_success() -> Result<(), String> {
    // === Run 1: Genesis → Block 9 ===
    let raw1 = util::load_headers_from_db(DB_PATH, 0, 10);
    let headers1 = util::raw_headers_to_new_headers(&raw1);
    let mut stdin1 = SP1Stdin::new();
    stdin1.write::<u32>(&0); // prev_height = 0
    stdin1.write::<u32>(&10);
    stdin1.write_vec(headers1);

    let client = sp1_sdk::ProverClient::from_env().await;
    let pk = client.setup(ELF).await.map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();

    let (pv1, _) = client
        .execute(ELF, stdin1)
        .await
        .map_err(|e| format!("Run 1: {}", e))?;
    let pv1_bytes = pv1.to_vec();
    if pv1_bytes.len() != STATE_SIZE {
        return Err(format!(
            "Run 1: unexpected PV length {} (expected {})",
            pv1_bytes.len(),
            STATE_SIZE
        ));
    }

    // Verify Run 1 state
    let state1 = util::State::from_bytes(&pv1_bytes);
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
    stdin2.write_vec(pv1_bytes);
    stdin2.write::<u32>(&10);
    stdin2.write_vec(headers2);

    let pv2 = run_and_get_pv(stdin2).await?;
    parse_result(&pv2).map_err(|(code, detail)| {
        format!("expected success, got error code {} detail {}", code, detail)
    })
}
