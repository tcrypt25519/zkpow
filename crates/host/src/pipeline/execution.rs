//! Guest execution in the RISC-V executor (no proof).
//!
//! The host performs no validation of the results. The controller is the
//! authority on whether committed public values match its database. The one
//! exception is a fail-fast on guest-reported failures: proving a batch the
//! guest already rejected would waste GPU time only for the controller to
//! reject it too.

use memuse::StageSample;
use sp1_sdk::prelude::*;
use sp1_sdk::ExecutionReport;

use crate::memory_monitor;
use crate::pipeline::diagnostics::timed_async;
use crate::pipeline::BoxError;
use crate::pipeline::ELF;
use crate::util::HeaderChainPublicValues;

pub(crate) struct ExecutedBatch {
    pub(crate) stdin: SP1Stdin,
    pub(crate) public_values: Vec<u8>,
    pub(crate) before_prove_sample: StageSample,
    pub(crate) execution_report: ExecutionReport,
}

pub(crate) async fn execute_batch<P>(prover: &P, stdin: SP1Stdin) -> Result<ExecutedBatch, BoxError>
where
    P: Prover,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let (public_values, execution_report) = timed_async("execute_program", || async {
        prover
            .execute(ELF, stdin.clone())
            .deferred_proof_verification(false)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;

    let public_values = public_values.to_vec();
    match HeaderChainPublicValues::parse(&public_values) {
        Ok(HeaderChainPublicValues::Success { .. }) => {}
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => {
            return Err(format!(
                "guest execution failed with {} at height {}",
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

    Ok(ExecutedBatch { stdin, public_values, before_prove_sample, execution_report })
}
