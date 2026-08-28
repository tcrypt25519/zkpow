//! Guest execution in the RISC-V executor (execute-only sessions).
//!
//! The host performs no validation of the results. The controller is the
//! authority on whether committed public values match its database. The one
//! exception is surfacing a guest-reported failure as an error instead of
//! completing with failure public values the controller would reject anyway.

use sp1_sdk::{prelude::*, ExecutionReport};

use crate::{
    pipeline::{diagnostics::timed_async, BoxError, ELF},
    util::PublicValues,
};

pub(crate) async fn execute_batch<P>(prover: &P, stdin: SP1Stdin) -> Result<(Vec<u8>, ExecutionReport), BoxError>
where
    P: Prover,
{
    let (public_values, execution_report) = timed_async("execute_program", || async {
        zkpow_worker::execute(prover, ELF, stdin)
            .await
            .map_err(|err| -> BoxError { err.into() })
    })
    .await?;

    check_guest_success(&public_values, "execution")?;
    Ok((public_values, execution_report))
}

/// Error out on guest-reported failure or malformed public values.
pub(crate) fn check_guest_success(public_values: &[u8], stage: &str) -> Result<(), BoxError> {
    match PublicValues::parse(public_values) {
        Ok(pv) if pv.is_success() => Ok(()),
        Ok(pv) => {
            let error_code = pv.validation_error_code().expect("failure has an error code");
            Err(format!("guest {stage} failed with {error_code} at height {}", pv.state.height + 1).into())
        },
        Err(err) => Err(format!("{stage} produced malformed public values: {err}").into()),
    }
}
