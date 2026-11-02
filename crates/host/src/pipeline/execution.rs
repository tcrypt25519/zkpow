//! Guest execution in the RISC-V executor (execute-only sessions).

use sp1_sdk::{prelude::*, ExecutionReport};

use crate::pipeline::{diagnostics::timed_async, BoxError, ELF};

pub(crate) async fn execute_batch<P>(prover: &P, stdin: SP1Stdin) -> Result<(Vec<u8>, ExecutionReport), BoxError>
where
    P: Prover,
{
    timed_async("execute_program", || async {
        zkpow_worker::execute(prover, ELF, stdin)
            .await
            .map_err(|err| -> BoxError { err.into() })
    })
    .await
}

