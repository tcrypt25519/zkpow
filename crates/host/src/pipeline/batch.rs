use std::path::PathBuf;

use sp1_sdk::SP1ProofWithPublicValues;

use crate::pipeline::diagnostics::timed_sync;
use crate::pipeline::source::ControllerClient;
use crate::pipeline::BoxError;
use crate::util;

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

pub fn prepare_batch<S: ControllerClient>(
    prev_proof_path: Option<&PathBuf>,
    num_headers: u32,
    source: &mut S,
) -> Result<PreparedBatch, BoxError> {
    let previous_proof = load_optional_proof(prev_proof_path)?;
    let fetched = timed_sync("fetch_batch", || -> Result<_, BoxError> {
        source.next_work(num_headers)
    })?;

    Ok(PreparedBatch {
        previous_proof,
        current_state: fetched.current_state,
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
        let mut source = open_test_source();
        source
            .begin_session(zkpow_controller::SessionStart::Genesis)
            .unwrap();
        let config = test_config(None);
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.num_headers,
            &mut source,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 1);
        assert_eq!(batch.end_height, 1);
    }

    #[test]
    fn prepare_batch_uses_trusted_start_height_as_resume_cursor() {
        let mut source = open_test_source();
        source
            .begin_session(zkpow_controller::SessionStart::TrustedHeight(2016))
            .unwrap();
        let config = test_config(Some(2016));
        let batch = prepare_batch(
            config.prev_proof_path.as_ref(),
            config.num_headers,
            &mut source,
        )
        .unwrap();

        assert_eq!(batch.first_new_height, 2017);
        assert_eq!(batch.end_height, 2017);
    }
}
