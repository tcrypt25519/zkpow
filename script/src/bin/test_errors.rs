//! Automated tests for error handling in the Bitcoin header chain prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code,
//! then runs the zkVM program via `client.execute()` and checks the public values.

use sp1_sdk::prelude::*;
use sp1_sdk::{Prover, SP1Stdin, HashableKey, Elf};
use sp1_sdk::utils;

use bitcoin_header_chain_script::util;

const ELF: Elf = include_elf!("bitcoin-header-chain-program");

const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

const DB_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../bitcoin_headers.sqlite"
);

const STATUS_SUCCESS: u8 = 0;
const STATUS_GENESIS_HASH_MISMATCH: u8 = 1;
const STATUS_PREV_BLOCKHASH_MISMATCH: u8 = 2;
const STATUS_POW_INSUFFICIENT: u8 = 3;
const STATUS_HEADER_COUNT_MISMATCH: u8 = 7;

/// Run the program with given inputs and return (success_code, error_detail).
async fn run_and_get_status(stdin: SP1Stdin) -> Result<(u8, u32), String> {
    let client = sp1_sdk::ProverClient::from_env().await;
    let (public_values, _report) = client
        .execute(ELF, stdin)
        .await
        .map_err(|e| format!("execution failed: {}", e))?;

    let pv = public_values.to_vec();
    if pv.len() < 237 {
        return Err(format!("PV too short: {} bytes", pv.len()));
    }

    let success_code = pv[232];
    let error_detail = u32::from_le_bytes(pv[233..237].try_into().unwrap());
    Ok((success_code, error_detail))
}

fn genesis_hash() -> [u8; 32] {
    let mut g: [u8; 32] = hex::decode(GENESIS_HASH_HEX).unwrap().try_into().unwrap();
    g.reverse();
    g
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

    check("success_100_headers", test_success_100_headers().await);
    check("error_header_count_mismatch", test_error_header_count_mismatch().await);
    check("error_genesis_hash_mismatch", test_error_genesis_hash_mismatch().await);
    check("error_prev_blockhash_mismatch", test_error_prev_blockhash_mismatch().await);
    check("error_pow_insufficient", test_error_pow_insufficient().await);
    check("recursive_chain_success", test_recursive_chain_success().await);

    println!("\nDone.");
}

// ============================================================================
// Tests
// ============================================================================

async fn test_success_100_headers() -> Result<(), String> {
    let genesis = genesis_hash();
    let headers_bytes = util::load_headers_from_db(DB_PATH, 0, 100);

    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&genesis);
    stdin.write::<bool>(&false);
    stdin.write::<u64>(&0);
    stdin.write::<u64>(&100);
    stdin.write_vec(headers_bytes);

    let (code, detail) = run_and_get_status(stdin).await?;
    if code != STATUS_SUCCESS {
        return Err(format!("expected success, got error code {}", code));
    }
    if detail != 0 {
        return Err(format!("expected no error detail, got {}", detail));
    }
    Ok(())
}

async fn test_error_header_count_mismatch() -> Result<(), String> {
    let genesis = genesis_hash();
    let headers_bytes = util::load_headers_from_db(DB_PATH, 0, 10);

    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&genesis);
    stdin.write::<bool>(&false);
    stdin.write::<u64>(&0);
    stdin.write::<u64>(&20); // Claim 20 headers, provide 10*80=800 bytes
    stdin.write_vec(headers_bytes);

    let (code, _detail) = run_and_get_status(stdin).await?;
    if code != STATUS_HEADER_COUNT_MISMATCH {
        return Err(format!("expected STATUS_HEADER_COUNT_MISMATCH ({}), got {}", STATUS_HEADER_COUNT_MISMATCH, code));
    }
    Ok(())
}

async fn test_error_genesis_hash_mismatch() -> Result<(), String> {
    let mut wrong_genesis = genesis_hash();
    wrong_genesis[0] ^= 0xFF;

    let headers_bytes = util::load_headers_from_db(DB_PATH, 0, 1);

    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&wrong_genesis);
    stdin.write::<bool>(&false);
    stdin.write::<u64>(&0);
    stdin.write::<u64>(&1);
    stdin.write_vec(headers_bytes);

    let (code, detail) = run_and_get_status(stdin).await?;
    if code != STATUS_GENESIS_HASH_MISMATCH {
        return Err(format!("expected STATUS_GENESIS_HASH_MISMATCH ({}), got {}", STATUS_GENESIS_HASH_MISMATCH, code));
    }
    if detail != 0 {
        return Err(format!("expected error detail 0, got {}", detail));
    }
    Ok(())
}

async fn test_error_prev_blockhash_mismatch() -> Result<(), String> {
    let genesis = genesis_hash();
    let mut headers_bytes = util::load_headers_from_db(DB_PATH, 1, 1);
    headers_bytes[4] ^= 0xFF;
    headers_bytes[5] ^= 0xFF;

    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&genesis);
    stdin.write::<bool>(&false);
    stdin.write::<u64>(&1);
    stdin.write::<u64>(&1);
    stdin.write_vec(headers_bytes);

    let (code, detail) = run_and_get_status(stdin).await?;
    if code != STATUS_PREV_BLOCKHASH_MISMATCH {
        return Err(format!("expected STATUS_PREV_BLOCKHASH_MISMATCH ({}), got {}", STATUS_PREV_BLOCKHASH_MISMATCH, code));
    }
    if detail != 0 {
        return Err(format!("expected error detail 0, got {}", detail));
    }
    Ok(())
}

async fn test_error_pow_insufficient() -> Result<(), String> {
    let genesis = genesis_hash();
    let mut headers_bytes = util::load_headers_from_db(DB_PATH, 1, 1);
    headers_bytes[76] ^= 0xFF; // corrupt nonce

    let mut stdin = SP1Stdin::new();
    stdin.write::<[u8; 32]>(&genesis);
    stdin.write::<bool>(&false);
    stdin.write::<u64>(&1);
    stdin.write::<u64>(&1);
    stdin.write_vec(headers_bytes);

    let (code, detail) = run_and_get_status(stdin).await?;
    if code != STATUS_POW_INSUFFICIENT {
        return Err(format!("expected STATUS_POW_INSUFFICIENT ({}), got {}", STATUS_POW_INSUFFICIENT, code));
    }
    if detail != 0 {
        return Err(format!("expected error detail 0, got {}", detail));
    }
    Ok(())
}

// NOTE: STATUS_BITS_MISMATCH cannot be tested — requires forging a header with
// valid PoW for the correct target but different bits. Computationally infeasible.

async fn test_recursive_chain_success() -> Result<(), String> {
    let genesis = genesis_hash();

    // Run 1: blocks 0-9
    let headers1 = util::load_headers_from_db(DB_PATH, 0, 10);
    let mut stdin1 = SP1Stdin::new();
    stdin1.write::<[u8; 32]>(&genesis);
    stdin1.write::<bool>(&false);
    stdin1.write::<u64>(&0);
    stdin1.write::<u64>(&10);
    stdin1.write_vec(headers1);

    let client = sp1_sdk::ProverClient::from_env().await;
    let pk = client.setup(ELF).await.map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();

    let (pv1, _) = client.execute(ELF, stdin1).await
        .map_err(|e| format!("Run 1: {}", e))?;
    let pv1_bytes = pv1.to_vec();
    if pv1_bytes[232] != STATUS_SUCCESS {
        return Err(format!("Run 1 failed with code {}", pv1_bytes[232]));
    }

    // Run 2: blocks 10-19, extending from Run 1
    let headers2 = util::load_headers_from_db(DB_PATH, 10, 10);
    let mut stdin2 = SP1Stdin::new();
    stdin2.write::<[u8; 32]>(&genesis);
    stdin2.write::<bool>(&true);
    stdin2.write::<[u32; 8]>(&vk.hash_u32());
    let pv_digest = util::compute_pv_digest(&pv1_bytes);
    stdin2.write::<[u8; 32]>(&pv_digest);
    stdin2.write_vec(pv1_bytes);
    stdin2.write::<u64>(&10);
    stdin2.write::<u64>(&10);
    stdin2.write_vec(headers2);

    let (code, detail) = run_and_get_status(stdin2).await?;
    if code != STATUS_SUCCESS {
        return Err(format!("expected success, got error code {}", code));
    }
    if detail != 0 {
        return Err(format!("expected no error detail, got {}", detail));
    }
    Ok(())
}
