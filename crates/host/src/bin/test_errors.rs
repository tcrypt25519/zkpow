//! Automated tests for error handling in the zkpow prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code,
//! then runs the zkVM program via `client.execute()` and checks the public values.
//!
//! Input protocol:
//!   stdin: encoded_input(Vec<u8>) + state witness(Vec<u8>) + header witness(Vec<u8>)
//!          + median-time-past witness(Vec<u8>)
//!          → [recursive proof witness when claim.height > 0]
//!   output: MinimalPublicValues (169 bytes)

use sp1_sdk::prelude::*;
use sp1_sdk::{Elf, HashableKey, Prover, ProverClient, SP1Stdin};

use zkpow_host::config::db_path;
use zkpow_host::{observability, util};

use zkpow_host::util::{
    HeaderChainPublicValues, Input, MinimalPublicValues, PublicValuesDigest, RecursiveProof,
    ValidationErrorCode, VerifierKeyDigest,
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

fn expect_success(pv: &[u8]) -> Result<util::PublicChainClaim, String> {
    let parsed =
        HeaderChainPublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;
    match parsed {
        HeaderChainPublicValues::Success { claim, .. } => Ok(claim),
        HeaderChainPublicValues::Failure { failure, .. } => Err(format!(
            "expected success, got error {} at height {}",
            failure.error_code, failure.failure_height,
        )),
    }
}

fn expect_failure(
    pv: &[u8],
    expected_code: ValidationErrorCode,
    expected_failure_height: u32,
) -> Result<(), String> {
    let parsed =
        HeaderChainPublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;

    match parsed {
        HeaderChainPublicValues::Success { claim, .. } => Err(format!(
            "expected error {}, got success at height {}",
            expected_code, claim.height,
        )),
        HeaderChainPublicValues::Failure { failure, .. } => {
            if failure.error_code != expected_code {
                return Err(format!(
                    "expected error {}, got {}",
                    expected_code, failure.error_code,
                ));
            }
            if failure.failure_height != expected_failure_height {
                return Err(format!(
                    "expected failure height {}, got {}",
                    expected_failure_height, failure.failure_height,
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

fn consensus_bits(bits: util::CompactTarget) -> u32 {
    bits.into_inner()
}

fn mainnet_genesis_hash() -> util::BlockHash {
    let mut bytes: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX)
        .expect("mainnet genesis hash should decode")
        .try_into()
        .expect("mainnet genesis hash should be 32 bytes");
    bytes.reverse();
    util::BlockHash::new(bytes)
}

fn open_db() -> util::DbConn {
    util::DbConfig::new(db_path())
        .connect()
        .expect("failed to open database")
}

fn mainnet_genesis_state() -> util::State {
    let db = open_db();
    let genesis = db.load_header_record(0);
    util::genesis_state_from_record(genesis, mainnet_genesis_hash())
}

fn stdin_for_input(
    input: &Input,
    state: &util::State,
    headers: &[util::NewHeader],
    hints: &[util::BlockTimestamp],
) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(util::serialize_new_headers(headers));
    stdin.write_vec(util::serialize_median_hints(hints));
    // No recursive witnesses for genesis-start inputs (height == 0).
    stdin
}

fn stdin_for_recursive_input(
    input: &Input,
    state: &util::State,
    headers: &[util::NewHeader],
    hints: &[util::BlockTimestamp],
) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(util::serialize_new_headers(headers));
    stdin.write_vec(util::serialize_median_hints(hints));
    stdin
}

fn input_for_state(state: &util::State, recursive_proof: RecursiveProof) -> Input {
    Input::new(state.public_claim(), recursive_proof)
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

    // === Recursive hardening ===
    check(
        "error_recursive_on_failed_proof",
        test_error_recursive_on_failed_proof().await,
    );
    check(
        "error_recursive_on_tampered_state_witness",
        test_error_recursive_on_tampered_state_witness().await,
    );

    // === Step 6: minimal PV format ===
    check("pv_minimal_format", test_pv_minimal_format().await);

    println!("\nDone.");
}

// ============================================================================
// Tests
// ============================================================================

async fn test_success_100_headers() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 100);
    let input = input_for_state(&genesis_state, RecursiveProof::default());
    let stdin = stdin_for_input(&input, &genesis_state, &witness.headers, &witness.median_time_past_hints);

    let pv = run_and_get_pv(stdin).await?;
    expect_success(&pv).map(|_| ())
}

async fn test_retarget_boundary_schedule() -> Result<(), String> {
    const FIRST_BOUNDARY_TIP_HEIGHT: usize = 2015;
    const RETARGET_HEIGHT: usize = 32256;
    const RETARGET_TIP_HEIGHT: usize = RETARGET_HEIGHT - 1;
    const EPOCH_LENGTH: usize = zkpow_core::EPOCH_LENGTH as usize;
    let db = open_db();
    let genesis_state = mainnet_genesis_state();

    let witness_first_epoch = db.load_header_batch_witness(1, FIRST_BOUNDARY_TIP_HEIGHT as u64);
    let first_epoch_state = util::compute_final_state_with_hints(
        &genesis_state,
        &witness_first_epoch.headers,
        &witness_first_epoch.median_time_past_hints,
    );
    println!(
        "retarget-debug: first_epoch loaded={} state.height={} state.current_nbits={:#x}",
        witness_first_epoch.headers.len(),
        first_epoch_state.height,
        consensus_bits(first_epoch_state.current_nbits),
    );
    let first_retarget_bits =
        db.load_header_record((FIRST_BOUNDARY_TIP_HEIGHT + 1) as u64)
            .header
            .compact_target
            .into_inner();
    if consensus_bits(first_epoch_state.current_nbits) != first_retarget_bits {
        return Err(format!(
            "expected first retarget boundary bits {:#x}, got {:#x}",
            first_retarget_bits,
            consensus_bits(first_epoch_state.current_nbits),
        ));
    }

    let witness_retarget = db.load_header_batch_witness(1, RETARGET_TIP_HEIGHT as u64);
    let state = util::compute_final_state_with_hints(
        &genesis_state,
        &witness_retarget.headers,
        &witness_retarget.median_time_past_hints,
    );
    println!(
        "retarget-debug: retarget loaded={} state.height={} state.current_nbits={:#x}",
        witness_retarget.headers.len(),
        state.height,
        consensus_bits(state.current_nbits),
    );

    let previous_epoch_bits = db.load_header_record(RETARGET_TIP_HEIGHT as u64)
        .header
        .compact_target
        .into_inner();
    let next_header_bits = db.load_header_record(RETARGET_HEIGHT as u64)
        .header
        .compact_target
        .into_inner();

    println!(
        "retarget-debug: prev_epoch_bits={:#x} next_header_bits={:#x}",
        previous_epoch_bits, next_header_bits,
    );

    if state.height != RETARGET_TIP_HEIGHT as u32 {
        return Err(format!(
            "expected validated height {}, got {}",
            RETARGET_TIP_HEIGHT, state.height,
        ));
    }

    if previous_epoch_bits == next_header_bits {
        return Err(format!(
            "expected a real retarget boundary at height {}, but bits stayed at {:#x}",
            RETARGET_HEIGHT, next_header_bits,
        ));
    }

    if consensus_bits(state.current_nbits) != previous_epoch_bits {
        return Err(format!(
            "expected current epoch bits {:#x} before boundary block, got {:#x}",
            previous_epoch_bits,
            consensus_bits(state.current_nbits),
        ));
    }

    let witness_boundary = db.load_header_batch_witness(RETARGET_HEIGHT as u64, 1);
    let boundary_state = util::compute_final_state_with_hints(
        &state,
        &witness_boundary.headers,
        &witness_boundary.median_time_past_hints,
    );
    if boundary_state.height != RETARGET_HEIGHT as u32 {
        return Err(format!(
            "expected boundary height {}, got {}",
            RETARGET_HEIGHT, boundary_state.height,
        ));
    }
    if consensus_bits(boundary_state.current_nbits) != next_header_bits {
        return Err(format!(
            "expected boundary bits {:#x}, got {:#x}",
            next_header_bits,
            consensus_bits(boundary_state.current_nbits),
        ));
    }

    let witness_pre_boundary = db.load_header_batch_witness(1, (RETARGET_HEIGHT - 2) as u64);
    let pre_boundary_state = util::compute_final_state_with_hints(
        &genesis_state,
        &witness_pre_boundary.headers,
        &witness_pre_boundary.median_time_past_hints,
    );

    if consensus_bits(pre_boundary_state.current_nbits) != previous_epoch_bits {
        return Err(format!(
            "expected pre-boundary bits {:#x}, got {:#x}",
            previous_epoch_bits,
            consensus_bits(pre_boundary_state.current_nbits),
        ));
    }

    if (pre_boundary_state.height as usize).is_multiple_of(EPOCH_LENGTH) {
        return Err("pre-boundary state unexpectedly ended on an epoch boundary".to_string());
    }

    Ok(())
}

async fn test_error_timestamp_too_old() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 13);
    let mut headers = witness.headers;
    headers[11].timestamp = util::BlockTimestamp::new(1231006505);
    let input = input_for_state(&genesis_state, RecursiveProof::default());
    let stdin = stdin_for_input(&input, &genesis_state, &headers, &witness.median_time_past_hints);

    let pv = run_and_get_pv(stdin).await?;
    expect_failure(&pv, ValidationErrorCode::TimestampTooOld, 12)
}

async fn test_error_pow_insufficient() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 2);
    let mut headers = witness.headers;
    headers[0].nonce ^= 0xFF;
    let input = input_for_state(&genesis_state, RecursiveProof::default());
    let stdin = stdin_for_input(&input, &genesis_state, &headers, &witness.median_time_past_hints);

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
    let db = open_db();

    // === Run 1: genesis state → block 10 ===
    let genesis_state = mainnet_genesis_state();
    let witness1 = db.load_header_batch_witness(1, 10);
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());
    let input1 = input_for_state(
        &genesis_state,
        RecursiveProof {
            verifier_key,
            ..Default::default()
        },
    );
    let stdin1 = stdin_for_input(&input1, &genesis_state, &witness1.headers, &witness1.median_time_past_hints);

    let proof1 = client
        .prove(&pk, stdin1)
        .compressed()
        .await
        .map_err(|e| format!("Run 1 proof generation failed: {}", e))?;
    let pv1_bytes = proof1.public_values.to_vec();

    // Verify Run 1 state
    let claim1 = expect_success(&pv1_bytes)?;
    if claim1.height != 10 {
        return Err(format!("Run 1: expected height 10, got {}", claim1.height));
    }

    // === Run 2: Extend from Run 1 (blocks 11-20) ===
    // Reconstruct the full state at height 10 from DB (needed for continuation witness).
    let genesis_state2 = mainnet_genesis_state();
    let witness_to_10 = db.load_header_batch_witness(1, 10);
    let state1 = util::compute_final_state_with_hints(
        &genesis_state2,
        &witness_to_10.headers,
        &witness_to_10.median_time_past_hints,
    );

    let witness2 = db.load_header_batch_witness(11, 10);
    let input2 = input_for_state(
        &state1,
        RecursiveProof {
            verifier_key,
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(&pv1_bytes)),
            previous_return_code: 0,
            ..Default::default()
        },
    );
    let mut stdin2 = stdin_for_recursive_input(&input2, &state1, &witness2.headers, &witness2.median_time_past_hints);
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

/// Verify that the guest panics when asked to extend a failed prior proof.
async fn test_error_recursive_on_failed_proof() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 5);

    let state_at_5 = util::compute_final_state_with_hints(
        &genesis_state,
        &witness.headers,
        &witness.median_time_past_hints,
    );

    // Craft a RecursiveProof that claims the prior proof had a failure (return code 2).
    let bad_recursive_proof = RecursiveProof {
        verifier_key: VerifierKeyDigest::from_raw([0u32; 8]),
        public_values_digest: PublicValuesDigest::from_raw([0u8; 32]),
        previous_return_code: 2,
        ..Default::default()
    };

    let input = input_for_state(&state_at_5, bad_recursive_proof);
    let stdin = stdin_for_recursive_input(&input, &state_at_5, &witness.headers, &witness.median_time_past_hints);

    let client = ProverClient::builder().mock().build().await;
    let result = client.execute(ELF, stdin).await;

    match result {
        Err(_) => Ok(()),
        Ok((pv, _)) => {
            let pv_bytes = pv.to_vec();
            match HeaderChainPublicValues::parse(&pv_bytes) {
                Ok(HeaderChainPublicValues::Success { .. }) => Err(
                    "expected guest to reject failed prior proof, but got a success state"
                        .to_string(),
                ),
                _ => Ok(()),
            }
        }
    }
}

/// Verify that private cached continuation fields are bound by the prior
/// proof's public-values digest, even though they are not part of the public
/// chain claim.
async fn test_error_recursive_on_tampered_state_witness() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 5);
    let state_at_5 = util::compute_final_state_with_hints(
        &genesis_state,
        &witness.headers,
        &witness.median_time_past_hints,
    );

    let original_digest = util::continuation_digest_from_state(&state_at_5);
    let verifier_key = VerifierKeyDigest::from_raw([0u32; 8]);
    let original_pv =
        MinimalPublicValues::success(&state_at_5.public_claim(), original_digest, verifier_key);
    let recursive_proof = RecursiveProof {
        verifier_key,
        public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(
            &original_pv.to_bytes(),
        )),
        previous_return_code: 0,
        ..Default::default()
    };

    let mut tampered_state = state_at_5.clone();
    tampered_state.current_work = util::ChainWork::default();

    let input = input_for_state(&state_at_5, recursive_proof);
    let empty_headers = [];
    let empty_hints = Vec::new();
    let stdin = stdin_for_recursive_input(&input, &tampered_state, &empty_headers, &empty_hints);

    let client = ProverClient::builder().mock().build().await;
    let result = client.execute(ELF, stdin).await;

    match result {
        Err(_) => Ok(()),
        Ok((pv, _)) => {
            let pv_bytes = pv.to_vec();
            match HeaderChainPublicValues::parse(&pv_bytes) {
                Ok(HeaderChainPublicValues::Success { .. }) => Err(
                    "expected guest to reject tampered continuation state, but got success"
                        .to_string(),
                ),
                _ => Ok(()),
            }
        }
    }
}

/// Verify the public-value format is the minimal 169-byte layout.
async fn test_pv_minimal_format() -> Result<(), String> {
    use zkpow_core::MINIMAL_PV_SIZE;

    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 10);
    let input = input_for_state(&genesis_state, RecursiveProof::default());
    let stdin = stdin_for_input(&input, &genesis_state, &witness.headers, &witness.median_time_past_hints);

    let pv = run_and_get_pv(stdin).await?;

    if pv.len() != MINIMAL_PV_SIZE {
        return Err(format!(
            "expected {} bytes (MINIMAL_PV_SIZE), got {}",
            MINIMAL_PV_SIZE,
            pv.len()
        ));
    }

    let claim = expect_success(&pv)?;
    if claim.height != 10 {
        return Err(format!("expected height 10, got {}", claim.height));
    }

    // Parse as MinimalPublicValues and verify continuation digest is non-zero.
    let mpv =
        MinimalPublicValues::parse(&pv).map_err(|e| format!("MinimalPublicValues::parse: {e}"))?;
    if mpv.continuation_digest == [0u8; 32] {
        return Err("continuation digest is all zeros".to_string());
    }
    if mpv.return_code != 0 {
        return Err(format!("expected return_code 0, got {}", mpv.return_code));
    }

    // Host-computed continuation digest must match.
    let genesis_state2 = mainnet_genesis_state();
    let final_state = util::compute_final_state_with_hints(
        &genesis_state2,
        &witness.headers,
        &witness.median_time_past_hints,
    );
    let host_digest = util::continuation_digest_from_state(&final_state);
    if host_digest != mpv.continuation_digest {
        return Err(format!(
            "host digest {:?} != committed digest {:?}",
            &host_digest[..4],
            &mpv.continuation_digest[..4]
        ));
    }

    Ok(())
}
