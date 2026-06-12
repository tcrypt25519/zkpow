use std::path::PathBuf;

use sp1_sdk::SP1ProofWithPublicValues;

use crate::pipeline::diagnostics::{format_claim_mismatch, timed_sync};
use crate::pipeline::BoxError;
use crate::util;
use crate::util::{DbConn, HeaderChainPublicValues};

pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";

#[derive(Debug)]
pub struct PreparedBatch {
    pub previous_proof: Option<SP1ProofWithPublicValues>,
    pub current_state: util::State,
    pub headers: Vec<util::NewHeader>,
    pub median_hints: Vec<util::BlockTimestamp>,
    pub expected_state: util::State,
    pub expected_continuation_digest: [u8; 32],
    pub first_new_height: u32,
    pub end_height: u32,
}

pub fn prepare_batch(
    prev_proof_path: Option<&PathBuf>,
    trusted_start_height: Option<u32>,
    num_headers: u32,
    db: &DbConn,
    genesis_hash: util::BlockHash,
) -> Result<PreparedBatch, BoxError> {
    let previous_proof = load_optional_proof(prev_proof_path)?;
    let current_state = resolve_current_state(
        trusted_start_height,
        db,
        genesis_hash,
        previous_proof.as_ref(),
    )?;
    let first_new_height = current_state.height + 1;
    let witness = load_header_witness(num_headers, db, first_new_height)?;
    if witness.headers.is_empty() {
        return Err(format!(
            "{NO_HEADERS_REMAINING_PREFIX}: starting at height {}",
            first_new_height
        )
        .into());
    }
    let loaded_count = witness.headers.len() as u32;
    let end_height = current_state.height + loaded_count;
    let expected_state = simulate_expected_state(
        db,
        genesis_hash,
        &current_state,
        &witness.headers,
        &witness.median_time_past_hints,
        first_new_height,
        end_height,
    )?;
    let expected_continuation_digest = util::continuation_digest_from_state(&expected_state);

    Ok(PreparedBatch {
        previous_proof,
        current_state,
        headers: witness.headers,
        median_hints: witness.median_time_past_hints,
        expected_state,
        expected_continuation_digest,
        first_new_height,
        end_height,
    })
}

fn load_optional_proof(
    optional_proof: Option<&PathBuf>,
) -> Result<Option<SP1ProofWithPublicValues>, BoxError> {
    timed_sync("load_optional_proof", || {
        optional_proof
            .map(SP1ProofWithPublicValues::load)
            .transpose()
    })
}

fn resolve_current_state(
    trusted_start_height: Option<u32>,
    db: &DbConn,
    genesis_hash: util::BlockHash,
    previous_proof: Option<&SP1ProofWithPublicValues>,
) -> Result<util::State, BoxError> {
    timed_sync("resolve_current_state", || -> Result<_, BoxError> {
        if let Some(prev_proof) = previous_proof {
            if trusted_start_height.is_some() {
                return Err("trusted start height cannot be combined with a previous proof".into());
            }
            let prev_public_values =
                HeaderChainPublicValues::parse(prev_proof.public_values.as_ref())
                    .map_err(|err| err.to_string())?;
            let claim = match prev_public_values {
                HeaderChainPublicValues::Success { claim, .. } => claim,
                HeaderChainPublicValues::Failure { failure, .. } => {
                    return Err(format!(
                        "previous proof ended in error: {} at height {}",
                        failure.error_code, failure.failure_height,
                    )
                    .into());
                }
            };
            if claim.genesis_hash != genesis_hash {
                return Err("previous proof genesis mismatch".into());
            }

            let state = util::state_from_db_at_height(db, claim.height, genesis_hash);
            if state.public_claim() != claim {
                return Err(format!(
                    "db state mismatch at height {}:\n{}",
                    claim.height,
                    format_claim_mismatch(&claim, &state.public_claim())
                )
                .into());
            }
            return Ok(state);
        }

        if let Some(height) = trusted_start_height {
            return Ok(util::state_from_db_at_height(db, height, genesis_hash));
        }

        Ok(util::state_from_db_at_height(db, 0, genesis_hash))
    })
}

fn load_header_witness(
    num_headers: u32,
    db: &DbConn,
    first_new_height: u32,
) -> Result<util::HeaderBatchWitness, BoxError> {
    timed_sync("load_header_witness", || -> Result<_, BoxError> {
        Ok(db.load_header_batch_witness(first_new_height as u64, num_headers as u64))
    })
}

fn simulate_expected_state(
    db: &DbConn,
    genesis_hash: util::BlockHash,
    current_state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &[util::BlockTimestamp],
    first_new_height: u32,
    end_height: u32,
) -> Result<util::State, BoxError> {
    let expected_state = timed_sync("simulate_expected_state", || -> Result<_, BoxError> {
        Ok(util::compute_final_state_with_hints(
            current_state,
            headers,
            median_hints,
        ))
    })?;

    let db_end_claim = timed_sync("load_db_end_claim", || -> Result<_, BoxError> {
        let record = db.load_header_record(end_height as u64);
        Ok(record.public_claim(genesis_hash))
    })?;

    if expected_state.public_claim() == db_end_claim {
        return Ok(expected_state);
    }

    let (bad_index, actual_state) = timed_sync("bisect_divergence", || -> Result<_, BoxError> {
        Ok(find_first_diverging_state(
            current_state,
            headers,
            median_hints,
            first_new_height,
            db,
            genesis_hash,
        ))
    })?;

    let bad_height = first_new_height + (bad_index as u32);
    let expected_claim = db.load_header_record(bad_height as u64).public_claim(genesis_hash);
    let actual_claim = actual_state.public_claim();

    Err(format!(
        "host simulation diverges from database\n  first bad header index in batch: {}\n  bad height: {}\n{}",
        bad_index,
        bad_height,
        format_claim_mismatch(&actual_claim, &expected_claim),
    )
    .into())
}

fn find_first_diverging_state(
    current_state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &[util::BlockTimestamp],
    first_new_height: u32,
    db: &DbConn,
    genesis_hash: util::BlockHash,
) -> (usize, util::State) {
    assert!(
        !headers.is_empty(),
        "find_first_diverging_state requires at least one header"
    );
    assert_eq!(
        headers.len(),
        median_hints.len(),
        "median hint count must match header count"
    );

    let mut replay_state = current_state.clone();
    let mut replayed_states = Vec::new();
    let mut lo = 0usize;
    let mut hi = headers.len() - 1;

    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        replay_states_through(
            &mut replay_state,
            &mut replayed_states,
            headers,
            median_hints,
            mid,
        );

        let height = first_new_height + (mid as u32);
        let db_claim = db.load_header_record(height as u64).public_claim(genesis_hash);
        if replayed_states[mid].public_claim() == db_claim {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    replay_states_through(
        &mut replay_state,
        &mut replayed_states,
        headers,
        median_hints,
        lo,
    );

    (lo, replayed_states[lo].clone())
}

fn replay_states_through(
    state: &mut util::State,
    replayed_states: &mut Vec<util::State>,
    headers: &[util::NewHeader],
    median_hints: &[util::BlockTimestamp],
    target_index: usize,
) {
    while replayed_states.len() <= target_index {
        let index = replayed_states.len();
        state
            .apply_headers(&[headers[index]], &[median_hints[index]], util::hash_header)
            .unwrap_or_else(|err| {
                panic!("host state transition should succeed at header index {index}: {err:?}")
            });
        replayed_states.push(state.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{ProverBackend, ProofGenerationConfig};

    fn test_config(trusted_start_height: Option<u32>) -> ProofGenerationConfig {
        ProofGenerationConfig {
            prev_proof_path: None,
            trusted_start_height,
            num_headers: 1,
            batch_count: 1,
            db_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db").into(),
            output_dir: ".".into(),
            generate_groth16: false,
            execute_only: true,
            prover_backend: ProverBackend::Mock,
            cuda_device_id: None,
        }
    }

    fn open_test_db() -> DbConn {
        let config = test_config(None);
        crate::util::DbConfig::new(&config.db_path)
            .connect()
            .expect("failed to open test database")
    }

    #[test]
    fn prepare_batch_starts_at_genesis_without_resume_cursor() {
        let genesis_hash = crate::pipeline::input::parse_genesis_hash().unwrap();
        let db = open_test_db();
        let config = test_config(None);
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.trusted_start_height,
            config.num_headers,
            &db,
            genesis_hash,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 1);
        assert_eq!(batch.end_height, 1);
    }

    #[test]
    fn prepare_batch_uses_trusted_start_height_as_resume_cursor() {
        let genesis_hash = crate::pipeline::input::parse_genesis_hash().unwrap();
        let db = open_test_db();
        let config = test_config(Some(2016));
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.trusted_start_height,
            config.num_headers,
            &db,
            genesis_hash,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 2017);
        assert_eq!(batch.end_height, 2017);
    }
}
