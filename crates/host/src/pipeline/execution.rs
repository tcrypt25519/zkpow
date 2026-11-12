//! Guest execution in the RISC-V executor (execute-only sessions).

use sp1_sdk::{prelude::*, ExecutionReport};

use crate::pipeline::{diagnostics::timed_async, BoxError, ELF};

pub(crate) async fn execute_batch<P>(prover: &P, stdin: SP1Stdin) -> Result<(Vec<u8>, ExecutionReport), BoxError>
where
    P: Prover,
{
    timed_async("execute_program", || async {
        let (public_values, report) = prover
            .execute(ELF, stdin)
            .deferred_proof_verification(false)
            .await
            .map_err(|err| -> BoxError { err.to_string().into() })?;
        Ok::<_, BoxError>((public_values.to_vec(), report))
    })
    .await
}
