//! Automated integration tests for error handling and execution in the zkpow prover.
//!
//! Each test crafts specific inputs that should trigger a particular error code or success,
//! then runs the zkVM program via `client.execute()` and checks public values.

use sp1_sdk::{Elf, Prover, ProverClient, SP1Stdin, prelude::*};
use wiretype::WireType;
use zkpow_controller::{compute_final_state_cold, compute_final_state_warm};
use zkpow_core::{HeaderInput, PublicValues, State, VerifierKeyDigest};
use zkpow_params::{Consensus, Mainnet};
use zkpow_store::{HeaderStore, genesis_state_from_record};
use zkpow_types::{BlockHash, BlockTimestamp, ChainWork, CompactTarget};

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

fn expect_success(pv: &[u8]) -> Result<State, String> {
    let pv = PublicValues::parse(pv).map_err(|err| format!("failed to parse PV: {err}"))?;
    if pv.is_success() {
        Ok(pv.state)
    } else {
        Err(format!("expected success, got error at height {}", pv.state.height + 1))
    }
}

fn consensus_bits(bits: CompactTarget) -> u32 {
    bits.into_inner()
}

fn open_db() -> HeaderStore {
    zkpow_store::test_utils::create_test_fixture_store()
}

fn mainnet_genesis_state() -> State {
    let db = open_db();
    let genesis = db.load_header_record(0).unwrap().expect("genesis record");
    genesis_state_from_record::<Mainnet>(genesis, Mainnet::GENESIS_HASH)
}

fn build_stdin(
    verifier_key: VerifierKeyDigest,
    state: &State,
    headers: &[HeaderInput],
    sparse_hints: &[BlockTimestamp],
) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(verifier_key.to_bytes().to_vec());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(headers.to_bytes());
    stdin.write_vec(sparse_hints.to_bytes());
    stdin
}

fn compute_final_state_with_sparse_hints(
    initial_state: &State,
    headers: &[HeaderInput],
    sparse_hints: &[BlockTimestamp],
) -> State {
    if initial_state.height == 0 {
        compute_final_state_cold::<Mainnet>(initial_state, headers, sparse_hints)
    } else {
        compute_final_state_warm::<Mainnet>(initial_state, headers, sparse_hints)
    }
}

#[tokio::test]
async fn test_success_100_headers() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 100).expect("batch witness");
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
    const RETARGET_HEIGHT: usize = 40320;
    const RETARGET_TIP_HEIGHT: usize = RETARGET_HEIGHT - 1;
    let db = open_db();
    let genesis = db.load_header_record(0).unwrap().expect("genesis record");
    let genesis_hash = BlockHash::from(&genesis.header);
    let initial_state = db
        .state_at_height::<Mainnet>(40270, genesis_hash)
        .unwrap()
        .expect("initial state at height 40270");

    let witness_retarget = db.load_header_batch_witness(40271, 49).expect("batch witness");
    let state = compute_final_state_warm::<Mainnet>(
        &initial_state,
        &witness_retarget.headers,
        &witness_retarget.median_time_past_hints,
    );

    let previous_epoch_bits = db
        .load_compact_target(RETARGET_TIP_HEIGHT as u64)
        .expect("target result")
        .expect("compact target")
        .into_inner();
    let next_header_bits = db
        .load_compact_target(RETARGET_HEIGHT as u64)
        .expect("target result")
        .expect("compact target")
        .into_inner();

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

    let witness_boundary = db.load_header_batch_witness(RETARGET_HEIGHT as u64, 1).expect("batch witness");
    let boundary_state = compute_final_state_warm::<Mainnet>(
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

    let witness_pre_boundary = db.load_header_batch_witness(40271, 48).expect("batch witness");
    let pre_boundary_state = compute_final_state_warm::<Mainnet>(
        &initial_state,
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

    Ok(())
}

/// The guest derives the height-zero state from the anchor header the program itself carries, so
/// a prover supplying any other initial state proves exactly the same chain.
#[tokio::test]
async fn test_tampered_anchor_state_is_derived_away() -> Result<(), String> {
    let witness = open_db().load_header_batch_witness(1, 10).expect("batch witness");
    let run = |state: State| {
        let stdin = build_stdin(
            VerifierKeyDigest::default(),
            &state,
            &witness.headers,
            &witness.median_time_past_hints,
        );
        run_and_get_pv(stdin)
    };

    let canonical = run(mainnet_genesis_state()).await?;
    expect_success(&canonical)?;

    type StateMutation = (&'static str, fn(&mut State));
    let anchor = mainnet_genesis_state();
    let mutations: [StateMutation; 6] = [
        ("chain_work", |s| s.chain_work = ChainWork::from_limbs([1_000_000, 0, 0, 0])),
        ("block_hash", |s| s.block_hash = BlockHash::new([0x11; 32])),
        ("genesis_hash", |s| s.genesis_hash = BlockHash::new([0x22; 32])),
        ("current_nbits", |s| s.current_nbits = CompactTarget::new(0x1d00_fffe)),
        ("epoch_timestamp", |s| s.epoch_timestamp = BlockTimestamp::default()),
        ("anchor timestamp slot", |s| s.timestamps[0] = BlockTimestamp::new(1)),
    ];

    let mut leaked = Vec::new();
    for (name, mutate) in mutations {
        let mut state = anchor;
        mutate(&mut state);
        assert_ne!(state, anchor, "the {name} case tampers with nothing");

        if run(state).await? != canonical {
            leaked.push(name);
        }
    }

    if !leaked.is_empty() {
        return Err(format!("the witness still reaches the proven chain through {leaked:?}"));
    }

    Ok(())
}

#[tokio::test]
async fn test_pv_public_values_format() -> Result<(), String> {
    let genesis_state = mainnet_genesis_state();
    let witness = open_db().load_header_batch_witness(1, 10).expect("batch witness");
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

#[cfg(any(feature = "test_proofs", feature = "test-proofs", test_proofs))]
#[path = "test_errors/proofs/mod.rs"]
mod proofs;
