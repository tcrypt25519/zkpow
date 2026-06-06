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
    pub median_hints: Vec<util::BlockTimestamp>,
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
    let witness = load_header_witness(config, first_new_height)?;
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
        config,
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
            if config.trusted_start_height.is_some() {
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

        if let Some(height) = config.trusted_start_height {
            return Ok(util::state_from_db_at_height(db_path, height, genesis_hash));
        }

        Ok(util::state_from_db_at_height(db_path, 0, genesis_hash))
    })
}

fn load_header_witness(
    config: &ProofGenerationConfig,
    first_new_height: u32,
) -> Result<util::HeaderBatchWitness, BoxError> {
    timed_sync("load_header_witness", || -> Result<_, BoxError> {
        Ok(util::load_header_batch_witness_from_db(
            path_to_str(&config.db_path)?,
            first_new_height as u64,
            config.num_headers as u64,
        ))
    })
}

fn simulate_expected_state(
    config: &ProofGenerationConfig,
    genesis_hash: util::BlockHash,
    current_state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &[util::BlockTimestamp],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ProverBackend;

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

    #[test]
    fn prepare_batch_starts_at_genesis_without_resume_cursor() {
        let genesis_hash = crate::pipeline::input::parse_genesis_hash().unwrap();

        let batch = prepare_batch(&test_config(None), genesis_hash).unwrap();

        assert_eq!(batch.first_new_height, 1);
        assert_eq!(batch.end_height, 1);
    }

    #[test]
    fn prepare_batch_uses_trusted_start_height_as_resume_cursor() {
        let genesis_hash = crate::pipeline::input::parse_genesis_hash().unwrap();

        let batch = prepare_batch(&test_config(Some(2016)), genesis_hash).unwrap();

        assert_eq!(batch.first_new_height, 2017);
        assert_eq!(batch.end_height, 2017);
    }
}
