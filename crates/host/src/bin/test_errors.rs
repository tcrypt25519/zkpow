//! Automated tests for error handling in the zkpow prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code,
//! then runs the zkVM program via `client.execute()` and checks the public values.
//!
//! Input protocol:
//!   stdin: encoded_input(Vec<u8>) where trailing bytes are fixed-width `NewHeader`s
//!          → [recursive proof witness when state.height > 0]
//!   output: state on success, or state + error_code(1) + header_index(4) on error

use sp1_sdk::prelude::*;
use sp1_sdk::{Elf, HashableKey, Prover, ProverClient, SP1Stdin};

use zkpow_host::observability;
use zkpow_host::proof_pipeline::DEFAULT_DB_PATH;
use zkpow_host::util;
use zkpow_host::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, ValidationErrorCode,
    VerifierKeyDigest,
};

const ELF: Elf = include_elf!("zkpow-guest");

const MAINNET_GENESIS_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

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

fn consensus_bits(bits: util::CompactTarget) -> u32 {
    bits.to_consensus()
}

fn mainnet_genesis_hash() -> util::BlockHash {
    let mut bytes: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX)
        .expect("mainnet genesis hash should decode")
        .try_into()
        .expect("mainnet genesis hash should be 32 bytes");
    bytes.reverse();
    util::BlockHash::from_raw(bytes)
}

fn mainnet_genesis_state() -> util::State {
    let genesis = util::load_header_record_from_db(DEFAULT_DB_PATH, 0);
    util::genesis_state_from_record(genesis, mainnet_genesis_hash())
}

fn stdin_for_input(input: &Input, hints: &util::MedianTimePastHints) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(hints.to_bytes());
    stdin
}

#[tokio::main]
async fn main() {
    observability::init();
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
        "error_header_payload_length",
        test_error_header_payload_length().await,
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
    let genesis_state = mainnet_genesis_state();
    let records = util::load_header_records_from_db(DEFAULT_DB_PATH, 1, 100);
    let headers = util::records_to_new_headers(&records);
    let hints = util::median_time_past_hints_for_headers(&genesis_state, &headers);
    let input = Input::new(genesis_state, RecursiveProof::default(), headers)
        .map_err(|err| err.to_string())?;
    let stdin = stdin_for_input(&input, &hints);

    let pv = run_and_get_pv(stdin).await?;
    expect_success(&pv).map(|_| ())
}

async fn test_retarget_boundary_schedule() -> Result<(), String> {
    const FIRST_BOUNDARY_TIP_HEIGHT: usize = 2015;
    // Mainnet first changes difficulty at height 32256. We simulate through the
    // end of the previous epoch and assert the state carries the exact bits that
    // appear in the next raw header.
    const RETARGET_HEIGHT: usize = 32256;
    const RETARGET_TIP_HEIGHT: usize = RETARGET_HEIGHT - 1;
    const EPOCH_LENGTH: usize = 2016;
    let genesis_state = mainnet_genesis_state();

    let first_epoch_raw =
        util::load_headers_from_db(DEFAULT_DB_PATH, 1, FIRST_BOUNDARY_TIP_HEIGHT as u64);
    let first_epoch_headers = util::raw_headers_to_new_headers(&first_epoch_raw);
    let first_epoch_state = util::compute_final_state(&genesis_state, &first_epoch_headers);
    println!(
        "retarget-debug: first_epoch loaded={} state.height={} state.next_nbits={:#x}",
        first_epoch_headers.len(),
        first_epoch_state.height,
        consensus_bits(first_epoch_state.next_nbits),
    );
    let first_retarget_bits = raw_header_bits(
        &util::load_headers_from_db(DEFAULT_DB_PATH, (FIRST_BOUNDARY_TIP_HEIGHT + 1) as u64, 1),
        0,
    )?;
    if consensus_bits(first_epoch_state.next_nbits) != first_retarget_bits {
        return Err(format!(
            "expected first retarget boundary bits {:#x}, got {:#x}",
            first_retarget_bits,
            consensus_bits(first_epoch_state.next_nbits),
        ));
    }

    let raw_headers = util::load_headers_from_db(DEFAULT_DB_PATH, 1, RETARGET_TIP_HEIGHT as u64);
    let new_headers = util::raw_headers_to_new_headers(&raw_headers);
    let state = util::compute_final_state(&genesis_state, &new_headers);
    println!(
        "retarget-debug: retarget loaded={} state.height={} state.next_nbits={:#x}",
        new_headers.len(),
        state.height,
        consensus_bits(state.next_nbits),
    );
    println!(
        "retarget-debug: prev_epoch_bits={:#x} next_header_bits={:#x}",
        raw_header_bits(&raw_headers, RETARGET_TIP_HEIGHT - 1)?,
        raw_header_bits(
            &util::load_headers_from_db(DEFAULT_DB_PATH, RETARGET_HEIGHT as u64, 1),
            0,
        )?,
    );

    if state.height != RETARGET_TIP_HEIGHT as u32 {
        return Err(format!(
            "expected validated height {}, got {}",
            RETARGET_TIP_HEIGHT, state.height,
        ));
    }

    let previous_epoch_bits = raw_header_bits(&raw_headers, RETARGET_TIP_HEIGHT - 1)?;
    let next_header_bits = raw_header_bits(
        &util::load_headers_from_db(DEFAULT_DB_PATH, RETARGET_HEIGHT as u64, 1),
        0,
    )?;

    if previous_epoch_bits == next_header_bits {
        return Err(format!(
            "expected a real retarget boundary at height {}, but bits stayed at {:#x}",
            RETARGET_HEIGHT, next_header_bits,
        ));
    }

    if consensus_bits(state.next_nbits) != next_header_bits {
        return Err(format!(
            "expected next-header bits {:#x} after completing epoch, got {:#x}",
            next_header_bits,
            consensus_bits(state.next_nbits),
        ));
    }

    let pre_boundary_raw =
        util::load_headers_from_db(DEFAULT_DB_PATH, 1, (RETARGET_HEIGHT - 2) as u64);
    let pre_boundary_headers = util::raw_headers_to_new_headers(&pre_boundary_raw);
    let pre_boundary_state = util::compute_final_state(&genesis_state, &pre_boundary_headers);

    if consensus_bits(pre_boundary_state.next_nbits) != previous_epoch_bits {
        return Err(format!(
            "expected pre-boundary bits {:#x}, got {:#x}",
            previous_epoch_bits,
            consensus_bits(pre_boundary_state.next_nbits),
        ));
    }

    if (pre_boundary_state.height as usize).is_multiple_of(EPOCH_LENGTH) {
        return Err("pre-boundary state unexpectedly ended on an epoch boundary".to_string());
    }

    Ok(())
}

async fn test_error_header_payload_length() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let records = util::load_header_records_from_db(DEFAULT_DB_PATH, 1, 10);
    let headers = util::records_to_new_headers(&records);
    let input = Input::new(genesis_state, RecursiveProof::default(), headers)
        .map_err(|err| err.to_string())?;
    let mut encoded = input.to_bytes();
    encoded.pop();

    let mut stdin = SP1Stdin::new();
    stdin.write_vec(encoded);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::HeaderPayloadLengthInvalid, 0)
}

async fn test_error_timestamp_too_old() -> Result<(), String> {
    // Load blocks 0-12 so the median buffer is full (11 blocks after genesis).
    // Block 12's median check uses blocks 1-11 timestamps.
    // We corrupt block 12's timestamp to be older than the median.
    let genesis_state = mainnet_genesis_state();
    let records = util::load_header_records_from_db(DEFAULT_DB_PATH, 1, 13);
    let mut headers = util::records_to_new_headers(&records);
    let hints = util::median_time_past_hints_for_headers(&genesis_state, &headers);
    headers[11].timestamp = util::BlockTimestamp::from_consensus(1231006505);
    let input = Input::new(genesis_state, RecursiveProof::default(), headers)
        .map_err(|err| err.to_string())?;
    let stdin = stdin_for_input(&input, &hints);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::TimestampTooOld, 11)
}

async fn test_error_pow_insufficient() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let records = util::load_header_records_from_db(DEFAULT_DB_PATH, 1, 2);
    let mut headers = util::records_to_new_headers(&records);
    let hints = util::median_time_past_hints_for_headers(&genesis_state, &headers);
    headers[0].nonce ^= 0xFF;
    let input = Input::new(genesis_state, RecursiveProof::default(), headers)
        .map_err(|err| err.to_string())?;
    let stdin = stdin_for_input(&input, &hints);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::PowInsufficient, 0)
}

async fn test_recursive_chain_success() -> Result<(), String> {
    let client = ProverClient::from_env().await;
    let pk = client
        .setup(ELF)
        .await
        .map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();

    // === Run 1: genesis state → block 10 ===
    let genesis_state = mainnet_genesis_state();
    let records1 = util::load_header_records_from_db(DEFAULT_DB_PATH, 1, 10);
    let headers1 = util::records_to_new_headers(&records1);
    let hints1 = util::median_time_past_hints_for_headers(&genesis_state, &headers1);
    let input1 = Input::new(genesis_state, RecursiveProof::default(), headers1)
        .map_err(|err| err.to_string())?;
    let stdin1 = stdin_for_input(&input1, &hints1);

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

    // === Run 2: Extend from Run 1 (blocks 11-20) ===
    let records2 = util::load_header_records_from_db(DEFAULT_DB_PATH, 11, 10);
    let headers2 = util::records_to_new_headers(&records2);
    let hints2 = util::median_time_past_hints_for_headers(&state1, &headers2);
    let input2 = Input::new(
        state1,
        RecursiveProof {
            verifier_key: VerifierKeyDigest::from_raw(vk.hash_u32()),
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(&pv1_bytes)),
        },
        headers2,
    )
    .map_err(|err| err.to_string())?;
    let mut stdin2 = stdin_for_input(&input2, &hints2);
    let sp1_sdk::SP1Proof::Compressed(inner_proof) = &proof1.proof else {
        return Err("Run 1 proof is not compressed".to_string());
    };
    stdin2.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());

    let (pv2, _) = client
        .execute(ELF, stdin2)
        .await
        .map_err(|e| format!("Run 2 execution failed: {}", e))?;
    let pv2 = pv2.to_vec();
    expect_success(&pv2).map(|_| ())
}
