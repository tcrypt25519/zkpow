use std::path::Path;

use sp1_sdk::SP1ProofWithPublicValues;

use crate::pipeline::diagnostics::{format_claim_mismatch, timed_sync};
use crate::pipeline::execution::find_first_diverging_state_index;
use crate::pipeline::{BoxError, ProofGenerationConfig};
use crate::util;
use crate::util::HeaderChainPublicValues;

pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";

#[derive(Debug)]
pub struct PreparedBatch {
    pub previous_proof: Option<SP1ProofWithPublicValues>,
    pub current_state: util::State,
    pub headers: Vec<util::NewHeader>,
    pub median_hints: util::MedianTimePastHints,
    pub expected_state: util::State,
    pub expected_continuation_digest: [u8; 32],
    pub first_new_height: u32,
    pub end_height: u32,
}

pub fn path_to_str(path: &Path) -> Result<&str, BoxError> {
    path.to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", path.display()).into())
}

pub fn prepare_batch(
    config: &ProofGenerationConfig,
    genesis_hash: util::BlockHash,
) -> Result<PreparedBatch, BoxError> {
    let previous_proof = load_previous_proof(config)?;
    let current_state = resolve_current_state(config, genesis_hash, previous_proof.as_ref())?;
    let first_new_height = current_state.height + 1;
    let header_records = load_header_records(config, first_new_height)?;
    if header_records.is_empty() {
        return Err(format!(
            "{NO_HEADERS_REMAINING_PREFIX}: starting at height {}",
            first_new_height
        )
        .into());
    }
    let headers = decode_headers(&header_records)?;
    let median_hints = load_median_time_past_hints(&header_records)?;
    let loaded_count = headers.len() as u32;
    let end_height = current_state.height + loaded_count;
    let expected_state = simulate_expected_state(
        config,
        genesis_hash,
        &current_state,
        &headers,
        &median_hints,
        first_new_height,
        end_height,
    )?;
    let expected_continuation_digest = util::continuation_digest_from_state(&expected_state);

    Ok(PreparedBatch {
        previous_proof,
        current_state,
        headers,
        median_hints,
        expected_state,
        expected_continuation_digest,
        first_new_height,
        end_height,
    })
}

fn load_previous_proof(
    config: &ProofGenerationConfig,
) -> Result<Option<SP1ProofWithPublicValues>, BoxError> {
    timed_sync("load_previous_proof", || {
        config
            .prev_proof_path
            .as_ref()
            .map(SP1ProofWithPublicValues::load)
            .transpose()
    })
}

fn resolve_current_state(
    config: &ProofGenerationConfig,
    genesis_hash: util::BlockHash,
    previous_proof: Option<&SP1ProofWithPublicValues>,
) -> Result<util::State, BoxError> {
    timed_sync("resolve_current_state", || -> Result<_, BoxError> {
        let db_path = path_to_str(&config.db_path)?;
        if let Some(prev_proof) = previous_proof {
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

            let state = util::state_from_db_at_height(db_path, claim.height, genesis_hash);
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

        Ok(util::state_from_db_at_height(db_path, 0, genesis_hash))
    })
}

fn load_header_records(
    config: &ProofGenerationConfig,
    first_new_height: u32,
) -> Result<Vec<util::HeaderRecord>, BoxError> {
    timed_sync("load_header_records", || -> Result<_, BoxError> {
        Ok(util::load_header_records_from_db(
            path_to_str(&config.db_path)?,
            first_new_height as u64,
            config.num_headers as u64,
        ))
    })
}

fn decode_headers(records: &[util::HeaderRecord]) -> Result<Vec<util::NewHeader>, BoxError> {
    timed_sync("decode_headers", || -> Result<_, BoxError> {
        Ok(util::records_to_new_headers(records))
    })
}

fn load_median_time_past_hints(
    records: &[util::HeaderRecord],
) -> Result<util::MedianTimePastHints, BoxError> {
    timed_sync("load_median_time_past_hints", || -> Result<_, BoxError> {
        Ok(util::median_time_past_hints_from_records(records))
    })
}

fn simulate_expected_state(
    config: &ProofGenerationConfig,
    genesis_hash: util::BlockHash,
    current_state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &util::MedianTimePastHints,
    first_new_height: u32,
    end_height: u32,
) -> Result<util::State, BoxError> {
    let (expected_state, intermediate_states) =
        timed_sync("simulate_expected_state", || -> Result<_, BoxError> {
            Ok(util::compute_final_state_with_history(
                current_state,
                headers,
                median_hints,
            ))
        })?;

    let db_end_state = timed_sync("load_db_end_state", || -> Result<_, BoxError> {
        Ok(util::state_from_db_at_height(
            path_to_str(&config.db_path)?,
            end_height,
            genesis_hash,
        ))
    })?;

    if expected_state.public_claim() == db_end_state.public_claim() {
        return Ok(expected_state);
    }

    let bad_index = timed_sync("bisect_divergence", || -> Result<_, BoxError> {
        Ok(find_first_diverging_state_index(
            &intermediate_states,
            first_new_height,
            path_to_str(&config.db_path)?,
            genesis_hash,
        ))
    })?;

    let bad_height = first_new_height + (bad_index as u32);
    let expected_claim =
        util::state_from_db_at_height(path_to_str(&config.db_path)?, bad_height, genesis_hash)
            .public_claim();
    let actual_claim = intermediate_states[bad_index].public_claim();

    Err(format!(
        "host simulation diverges from database\n  first bad header index in batch: {}\n  bad height: {}\n{}",
        bad_index,
        bad_height,
        format_claim_mismatch(&actual_claim, &expected_claim),
    )
    .into())
}
