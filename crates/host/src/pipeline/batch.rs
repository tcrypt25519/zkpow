use std::path::PathBuf;

use sp1_sdk::SP1ProofWithPublicValues;

use crate::pipeline::diagnostics::{format_claim_mismatch, timed_sync};
use crate::pipeline::source::BatchSource;
use crate::pipeline::BoxError;
use crate::util;
use crate::util::HeaderChainPublicValues;

pub use zkpow_controller::NO_HEADERS_REMAINING_PREFIX;

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

pub fn prepare_batch<S: BatchSource>(
    prev_proof_path: Option<&PathBuf>,
    trusted_start_height: Option<u32>,
    num_headers: u32,
    source: &S,
) -> Result<PreparedBatch, BoxError> {
    let previous_proof = load_optional_proof(prev_proof_path)?;
    let current_state =
        resolve_current_state(trusted_start_height, source, previous_proof.as_ref())?;
    let fetched = timed_sync("fetch_batch", || -> Result<_, BoxError> {
        source.fetch_batch(&current_state, num_headers)
    })?;

    Ok(PreparedBatch {
        previous_proof,
        current_state,
        headers: fetched.headers,
        median_hints: fetched.median_hints,
        expected_state: fetched.expected_state,
        expected_continuation_digest: fetched.expected_continuation_digest,
        first_new_height: fetched.first_new_height,
        end_height: fetched.end_height,
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

fn resolve_current_state<S: BatchSource>(
    trusted_start_height: Option<u32>,
    source: &S,
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
            if claim.genesis_hash != source.genesis_hash() {
                return Err("previous proof genesis mismatch".into());
            }

            let state = source.state_at(claim.height);
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
            return Ok(source.state_at(height));
        }

        Ok(source.state_at(0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::source::LocalControllerSource;
    use crate::pipeline::{ProofGenerationConfig, ProverBackend};
    use zkpow_controller::{Controller, HeaderStore};

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

    fn open_test_source() -> LocalControllerSource {
        let config = test_config(None);
        let genesis_hash = crate::pipeline::input::genesis_hash();
        let db = HeaderStore::open(&config.db_path).expect("failed to open test database");
        LocalControllerSource::new(Controller::new(db, genesis_hash))
    }

    #[test]
    fn prepare_batch_starts_at_genesis_without_resume_cursor() {
        let source = open_test_source();
        let config = test_config(None);
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.trusted_start_height,
            config.num_headers,
            &source,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 1);
        assert_eq!(batch.end_height, 1);
    }

    #[test]
    fn prepare_batch_uses_trusted_start_height_as_resume_cursor() {
        let source = open_test_source();
        let config = test_config(Some(2016));
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.trusted_start_height,
            config.num_headers,
            &source,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 2017);
        assert_eq!(batch.end_height, 2017);
    }
}
