//! Automated integration tests for error handling and execution in the zkpow prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code or success,
//! then runs the zkVM program via `client.execute()` and checks public values.

use sp1_sdk::{prelude::*, Elf, HashableKey, Prover, ProverClient, SP1Stdin};
use zkpow_controller::db_path;
use zkpow_host::{
    util,
    util::{PublicValues, VerifierKeyDigest},
};
use zkpow_params::{Consensus, Mainnet};

const ELF: Elf = include_elf!("zkpow-guest");

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
    let pv = PublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;
    if pv.is_success() {
        Ok(pv.state)
    } else {
        Err(format!("expected success, got error at height {}", pv.state.height + 1))
    }
}

fn consensus_bits(bits: util::CompactTarget) -> u32 {
    bits.into_inner()
}

fn open_db() -> zkpow_controller::HeaderStore {
    zkpow_controller::HeaderStore::open(db_path()).expect("failed to open database")
}

fn mainnet_genesis_state() -> util::State {
    let db = open_db();
    let genesis = db.load_header_record(0);
    zkpow_controller::genesis_state_from_record(genesis, Mainnet::GENESIS_HASH)
}

fn build_stdin(
    verifier_key: VerifierKeyDigest,
    state: &util::State,
    headers: &[util::HeaderInput],
    sparse_hints: &[util::BlockTimestamp],
) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(verifier_key.to_bytes().to_vec());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(util::serialize_inputs(headers));
    stdin.write_vec(util::serialize_inputs(sparse_hints));
    stdin
}

fn compute_final_state_with_sparse_hints(
    initial_state: &util::State,
    headers: &[util::HeaderInput],
    sparse_hints: &[util::BlockTimestamp],
) -> util::State {
    if initial_state.height == 0 {
        util::compute_final_state_cold::<Mainnet>(initial_state, headers, sparse_hints)
    } else {
        util::compute_final_state_warm::<Mainnet>(initial_state, headers, sparse_hints)
    }
}

#[tokio::test]
async fn test_success_100_headers() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 100);
    let stdin = build_stdin(
        VerifierKeyDigest::default(),
        &genesis_state,
        &witness.headers,
        &witness.median_time_past_hints,
    );

    let pv = run_and_get_pv(stdin).await?;
    expect_success(&pv).map(|_| ())
}

#[tokio::test]
async fn test_retarget_boundary_schedule() -> Result<(), String> {
    const FIRST_BOUNDARY_TIP_HEIGHT: usize = 2015;
    const RETARGET_HEIGHT: usize = 32256;
    const RETARGET_TIP_HEIGHT: usize = RETARGET_HEIGHT - 1;
    const EPOCH_LENGTH: usize = Mainnet::PARAMS.epoch_length as usize;
    let db = open_db();
    let genesis_state = mainnet_genesis_state();

    let witness_first_epoch = db.load_header_batch_witness(1, FIRST_BOUNDARY_TIP_HEIGHT as u64);
    let first_epoch_state = compute_final_state_with_sparse_hints(
        &genesis_state,
        &witness_first_epoch.headers,
        &witness_first_epoch.median_time_past_hints,
    );
    let first_retarget_bits = db.load_compact_target((FIRST_BOUNDARY_TIP_HEIGHT + 1) as u64).into_inner();
    if consensus_bits(first_epoch_state.current_nbits) != first_retarget_bits {
        return Err(format!(
            "expected first retarget boundary bits {:#x}, got {:#x}",
            first_retarget_bits,
            consensus_bits(first_epoch_state.current_nbits),
        ));
    }

    let witness_retarget = db.load_header_batch_witness(1, RETARGET_TIP_HEIGHT as u64);
    let state = compute_final_state_with_sparse_hints(
        &genesis_state,
        &witness_retarget.headers,
        &witness_retarget.median_time_past_hints,
    );

    let previous_epoch_bits = db.load_compact_target(RETARGET_TIP_HEIGHT as u64).into_inner();
    let next_header_bits = db.load_compact_target(RETARGET_HEIGHT as u64).into_inner();

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
    let boundary_state = compute_final_state_with_sparse_hints(
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
    let pre_boundary_state = compute_final_state_with_sparse_hints(
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

#[tokio::test]
async fn test_error_recursive_on_failed_proof() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 10);

    let state_at_10 =
        compute_final_state_with_sparse_hints(&genesis_state, &witness.headers, &witness.median_time_past_hints);

    let stdin = build_stdin(
        VerifierKeyDigest::from_raw([0u32; 8]),
        &state_at_10,
        &witness.headers,
        &witness.median_time_past_hints,
    );

    let client = ProverClient::builder().mock().build().await;
    let result = client.execute(ELF, stdin).await;

    match result {
        Err(_) => Ok(()),
        Ok((pv, _)) => {
            let pv_bytes = pv.to_vec();
            match PublicValues::parse(&pv_bytes) {
                Ok(pv) if pv.is_success() => {
                    Err("expected guest to reject failed prior proof, but got a success state".to_string())
                },
                _ => Ok(()),
            }
        },
    }
}

#[tokio::test]
#[ignore = "requires full SP1 proof to verify continuation binding"]
async fn test_error_recursive_on_tampered_state_witness() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 10);
    let state_at_10 =
        compute_final_state_with_sparse_hints(&genesis_state, &witness.headers, &witness.median_time_past_hints);

    let verifier_key = VerifierKeyDigest::from_raw([0u32; 8]);

    let mut tampered_state = state_at_10;
    tampered_state.epoch_timestamp = util::BlockTimestamp::default();

    let empty_headers = [];
    let empty_hints = Vec::new();
    let stdin = build_stdin(verifier_key, &tampered_state, &empty_headers, &empty_hints);

    let client = ProverClient::builder().mock().build().await;
    let result = client.execute(ELF, stdin).await;

    match result {
        Err(_) => Ok(()),
        Ok((pv, _)) => {
            let pv_bytes = pv.to_vec();
            match PublicValues::parse(&pv_bytes) {
                Ok(pv) if pv.is_success() => {
                    Err("expected guest to reject tampered continuation state, but got success".to_string())
                },
                _ => Ok(()),
            }
        },
    }
}

#[tokio::test]
async fn test_pv_public_values_format() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 10);
    let stdin = build_stdin(
        VerifierKeyDigest::default(),
        &genesis_state,
        &witness.headers,
        &witness.median_time_past_hints,
    );

    let pv = run_and_get_pv(stdin).await?;

    if pv.len() != PublicValues::SIZE {
        return Err(format!(
            "expected {} bytes (PublicValues::SIZE), got {}",
            PublicValues::SIZE,
            pv.len()
        ));
    }

    let parsed = PublicValues::parse(&pv).map_err(|e| format!("PublicValues::parse: {e}"))?;
    if parsed.is_failure() {
        return Err(format!("expected success error_code, got {:?}", parsed.return_code));
    }
    if parsed.state.height != 10 {
        return Err(format!("expected height 10, got {}", parsed.state.height));
    }

    let genesis_state2 = mainnet_genesis_state();
    let final_state =
        compute_final_state_with_sparse_hints(&genesis_state2, &witness.headers, &witness.median_time_past_hints);
    if parsed.state != final_state {
        return Err("committed state does not match host-computed final state".to_string());
    }

    Ok(())
}

#[tokio::test]
#[ignore = "generates full SP1 compressed proof (~30s)"]
async fn test_recursive_chain_success() -> Result<(), String> {
    let client = ProverClient::from_env().await;
    let pk = client.setup(ELF).await.map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();
    let db = open_db();

    // === Run 1: genesis state → block 10 ===
    let genesis_state = mainnet_genesis_state();
    let witness1 = db.load_header_batch_witness(1, 10);
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());
    let stdin1 = build_stdin(
        verifier_key,
        &genesis_state,
        &witness1.headers,
        &witness1.median_time_past_hints,
    );

    let proof1 = client
        .prove(&pk, stdin1)
        .compressed()
        .await
        .map_err(|e| format!("Run 1 proof generation failed: {}", e))?;
    let pv1_bytes = proof1.public_values.to_vec();

    let state1 = expect_success(&pv1_bytes)?;
    if state1.height != 10 {
        return Err(format!("Run 1: expected height 10, got {}", state1.height));
    }

    // === Run 2: Extend from Run 1 (blocks 11-20) ===
    let genesis_state2 = mainnet_genesis_state();
    let witness_to_10 = db.load_header_batch_witness(1, 10);
    let state1 = compute_final_state_with_sparse_hints(
        &genesis_state2,
        &witness_to_10.headers,
        &witness_to_10.median_time_past_hints,
    );

    let witness2 = db.load_header_batch_witness(11, 10);
    let mut stdin2 = build_stdin(verifier_key, &state1, &witness2.headers, &witness2.median_time_past_hints);
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
