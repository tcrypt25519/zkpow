use memory_usage::StageSample;
use sp1_sdk::prelude::*;
use sp1_sdk::ExecutionReport;

use crate::memory_monitor;
use crate::pipeline::diagnostics::{timed_async, timed_sync};
use crate::pipeline::input::{build_recursive_proof, build_stdin};
use crate::pipeline::BoxError;
use crate::proof_pipeline::ELF;
use crate::util;
use crate::util::{HeaderChainPublicValues, Input, VerifierKeyDigest};

pub(crate) struct ExecutedBatchArtifacts {
    pub(crate) stdin: SP1Stdin,
    pub(crate) expected_pv: Vec<u8>,
    pub(crate) before_prove_sample: StageSample,
    pub(crate) execution_report: ExecutionReport,
}

pub(crate) struct UnprovenBatchInput<'a> {
    pub(crate) current_state: &'a util::State,
    pub(crate) headers: &'a [util::NewHeader],
    pub(crate) median_hints: &'a [util::BlockTimestamp],
    pub(crate) expected_state: &'a util::State,
    pub(crate) expected_continuation_digest: [u8; 32],
}

pub(crate) fn verify_public_values(
    pv: &[u8],
    expected_pv: &[u8],
    label: &str,
) -> Result<(), BoxError> {
    let parsed = HeaderChainPublicValues::parse(pv).map_err(|err| err.to_string())?;
    match parsed {
        HeaderChainPublicValues::Success { .. } => {
            if pv != expected_pv {
                return Err(format!(
                    "{label} public values mismatch: expected {}, got {}",
                    hex::encode(expected_pv),
                    hex::encode(pv),
                )
                .into());
            }
            Ok(())
        }
        HeaderChainPublicValues::Failure { failure, .. } => Err(format!(
            "{label} ended in error {} at height {}",
            failure.error_code, failure.failure_height,
        )
        .into()),
    }
}

pub(crate) fn find_first_diverging_state_index(
    states: &[util::State],
    first_new_height: u32,
    db_path: &str,
    genesis_hash: util::BlockHash,
) -> usize {
    assert!(
        !states.is_empty(),
        "find_first_diverging_state_index requires at least one state"
    );

    let mut lo: usize = 0;
    let mut hi: usize = states.len() - 1;

    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let height = first_new_height + (mid as u32);
        let db_state = util::state_from_db_at_height(db_path, height, genesis_hash);

        if states[mid].public_claim() == db_state.public_claim() {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    lo
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_batch_with_prover<P, K>(
    prover_name: &str,
    prover: &P,
    proving_key: &K,
    current_state: &util::State,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    headers: &[util::NewHeader],
    median_hints: &[util::BlockTimestamp],
    expected_pv_parts: (&util::State, [u8; 32]),
) -> Result<ExecutedBatchArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let (expected_state, expected_continuation_digest) = expected_pv_parts;
    let expected_pv = util::MinimalPublicValues::success(
        &expected_state.public_claim(),
        expected_continuation_digest,
        VerifierKeyDigest::from_raw(proving_key.verifying_key().hash_u32()),
    )
    .to_bytes()
    .to_vec();
    let recursive_proof = timed_sync("build_recursive_proof", || {
        build_recursive_proof(proving_key.verifying_key(), previous_proof)
    })?;
    let input = timed_sync("build_input", || -> Result<_, BoxError> {
        Ok(Input::new(current_state.public_claim(), recursive_proof))
    })?;
    let stdin = timed_sync("serialize_input", || {
        build_stdin(
            &input,
            current_state,
            headers,
            median_hints,
            previous_proof.map(|p| (p, proving_key.verifying_key())),
        )
    })?;

    let (public_values, report) = timed_async("execute_program", || async {
        prover
            .execute(ELF, stdin.clone())
            .deferred_proof_verification(false)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;
    tracing::info!(prover = prover_name, "Execution completed");

    let execution_public_values = public_values.to_vec();
    match HeaderChainPublicValues::parse(&execution_public_values) {
        Ok(HeaderChainPublicValues::Success { .. }) => {
            timed_sync(
                "verify_execution_public_values",
                || -> Result<(), BoxError> {
                    verify_public_values(&execution_public_values, &expected_pv, "execution")
                },
            )?;
        }
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => {
            return Err(format!(
                "execution failed with {} at height {}",
                failure.error_code, failure.failure_height
            )
            .into());
        }
        Err(err) => {
            return Err(format!("execution produced malformed public values: {err}").into());
        }
    }

    let before_prove_sample =
        memory_monitor::log_point("after_execute", "Memory snapshot after VM execution");

    Ok(ExecutedBatchArtifacts {
        stdin,
        expected_pv,
        before_prove_sample,
        execution_report: report,
    })
}

pub(crate) async fn execute_batch_without_proof<P>(
    prover_name: &str,
    prover: &P,
    batch: UnprovenBatchInput<'_>,
) -> Result<ExecutedBatchArtifacts, BoxError>
where
    P: Prover,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let verifier_key = VerifierKeyDigest::default();
    let expected_pv = util::MinimalPublicValues::success(
        &batch.expected_state.public_claim(),
        batch.expected_continuation_digest,
        verifier_key,
    )
    .to_bytes()
    .to_vec();
    let continuation_digest = util::continuation_digest_from_state(batch.current_state);
    let prior_public_values = util::MinimalPublicValues::success(
        &batch.current_state.public_claim(),
        continuation_digest,
        verifier_key,
    );
    let recursive_proof = util::RecursiveProof {
        verifier_key,
        public_values_digest: util::PublicValuesDigest::from_raw(util::compute_pv_digest(
            &prior_public_values.to_bytes(),
        )),
        ..Default::default()
    };
    let input = timed_sync("build_input", || -> Result<_, BoxError> {
        Ok(Input::new(
            batch.current_state.public_claim(),
            recursive_proof,
        ))
    })?;
    let stdin = timed_sync("serialize_input", || {
        build_stdin(
            &input,
            batch.current_state,
            batch.headers,
            batch.median_hints,
            None,
        )
    })?;

    let (public_values, report) = timed_async("execute_program", || async {
        prover
            .execute(ELF, stdin.clone())
            .deferred_proof_verification(false)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;
    let _ = prover_name;

    let execution_public_values = public_values.to_vec();
    verify_public_values(&execution_public_values, &expected_pv, "execution")?;

    let before_prove_sample =
        memory_monitor::log_point("after_execute", "Memory snapshot after VM execution");

    Ok(ExecutedBatchArtifacts {
        stdin,
        expected_pv,
        before_prove_sample,
        execution_report: report,
    })
}
