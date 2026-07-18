//! Guest execution in the RISC-V executor (execute-only sessions).
//!
//! The host performs no validation of the results — the controller is the
//! authority on whether committed public values match its database. The one
//! exception is surfacing a guest-reported failure as an error instead of
//! completing with failure public values the controller would reject anyway.

use sp1_sdk::prelude::*;
use sp1_sdk::ExecutionReport;

use crate::pipeline::diagnostics::timed_async;
use crate::pipeline::BoxError;
use crate::pipeline::ELF;
use crate::util::HeaderChainPublicValues;

pub(crate) async fn execute_batch<P>(
    prover: &P,
    stdin: SP1Stdin,
) -> Result<(Vec<u8>, ExecutionReport), BoxError>
where
    P: Prover,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    let (public_values, execution_report) = timed_async("execute_program", || async {
        prover
            .execute(ELF, stdin)
            .deferred_proof_verification(false)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })
    })
    .await?;

    let public_values = public_values.to_vec();
    check_guest_success(&public_values, "execution")?;
    Ok((public_values, execution_report))
}

/// Error out on guest-reported failure or malformed public values.
pub(crate) fn check_guest_success(public_values: &[u8], stage: &str) -> Result<(), BoxError> {
    match HeaderChainPublicValues::parse(public_values) {
        Ok(HeaderChainPublicValues::Success { .. }) => Ok(()),
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => Err(format!(
            "guest {stage} failed with {} at height {}",
            failure.error_code, failure.failure_height
        )
        .into()),
        Err(err) => Err(format!("{stage} produced malformed public values: {err}").into()),
    }
}
